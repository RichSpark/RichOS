//! STEERING AND STOP — the CEO's two mid-turn controls (UX §9.2, §9.3), and the durable
//! antechamber they need in order to exist at all.
//!
//! # The wall this module exists to get around, measured
//!
//! `Spine::submit_prompt` takes `&mut self` and does not return until the turn is over:
//! `deliver()` borrows `&mut self.ledger` across the whole of `lease.prompt(..)`
//! (`spine.rs`), and the Tauri shell holds the one `Spine` behind a `Mutex`. So for the
//! entire length of a turn — §6.2's own example is `Worked for 2h 17m 50s` — **every path
//! into the spine is blocked**, including the ledger. That is deliberate and it is what
//! makes rotation unable to race a CEO message (`spine.rs`, "Concurrency note").
//!
//! It also means a stop control cannot be implemented by calling the spine. Anything that
//! waits for that lock is a button that does nothing until the work it was meant to
//! interrupt has already finished.
//!
//! # What this module does instead
//!
//! One append-only file — the **intake log** — that the CEO's two mid-turn controls can
//! write to without touching the spine at all:
//!
//! ```text
//!   CEO presses stop   -> IntakeLog::append(Stop{turn_id})   -> fsync -> cancel the lease
//!   CEO steers         -> IntakeLog::append(Steer{text})     -> fsync -> wait for the boundary
//!   spine reaches the turn boundary -> drain into the LEDGER -> append Drained{through}
//! ```
//!
//! **A record here is never dropped in silence.** It is the CEO's own typed words before
//! anything at all has happened to them, so a record this build cannot read is counted,
//! classified and reported ([`IntakeLog::health`]) rather than skipped past — and it is
//! never fatal, because refusing to open the app is not an improvement over dropping one
//! message. The judgment is the ledger's, shared rather than re-derived ([`crate::skip`]).
//!
//! **The intake log is not a second source of truth.** Nothing reads it for display and
//! nothing renders from it. Every record has exactly two fates: it becomes a ledger event,
//! or it is refused and marked drained. The ledger remains the only thing that says what
//! happened. What the intake log adds is the ONE guarantee the ledger cannot give while it
//! is locked inside a running turn: *the CEO's words and the CEO's stop are on disk before
//! anything acts on them* — §9.2's *"persisted before delivery"* and §9.3's step 1,
//! *"persist a stop request"*, in that literal order.
//!
//! # What it deliberately does not do
//!
//! - **It does not inject a steering message into the running turn.** The agent runs one
//!   `session/prompt` at a time and the continuity design's turn-boundary controller is
//!   queue-not-interrupt by construction (§3.1). §25 asks that *"a steering message joins
//!   the active turn in durable order"*; what this build delivers is that the message is
//!   durable and ordered the instant the CEO presses Enter, and reaches Rich at the next
//!   turn boundary. That is a real difference and the UI says so in words rather than
//!   implying the message landed mid-turn.
//! - **It does not model `waiting_for_user`.** Nothing in this system emits it: §22 lists
//!   worker waiting state under "must not be faked", `worker_status.rs` refuses to infer
//!   it, and `TeammateIdle` cannot tell "paused for input" from "finished for good". A
//!   pause with no signal behind it would make §6.3's active-time accounting a guess, so
//!   there is no pause here and `Turn::active_ms` stays exact.

use crate::entity::EntityId;
use crate::skip::{SkipDialect, SkipKind, SkippedRecord};
use crate::util::now_millis;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, thiserror::Error)]
pub enum SteeringError {
    #[error("steering intake io: {0}")]
    Io(String),
    /// No durable intake log is attached, so a stop request cannot be written down before
    /// acting on it. Refusing is the point: a stop that is not recorded cannot later be
    /// told apart from a crash, and §6.1's `You stopped after {duration}` would be back to
    /// being an attribution with no evidence.
    #[error("no durable intake log is attached — RichOS will not act on a stop it cannot record")]
    NoDurableIntake,
    /// Steering requires something to steer.
    #[error("nothing is running to steer")]
    NoActiveTurn,
}

impl From<std::io::Error> for SteeringError {
    fn from(e: std::io::Error) -> Self {
        SteeringError::Io(e.to_string())
    }
}

// ---------------------------------------------------------------------------------------
// THE INTAKE LOG
// ---------------------------------------------------------------------------------------

/// One line of the intake log.
///
/// `id` is a per-file monotonic counter, and it is the ORDERING KEY for everything in this
/// module — including the one rule that needs two records compared to each other (a stop
/// cancels the steering that preceded it, and never the steering that followed it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "snake_case")]
pub enum IntakeRecord {
    /// §9.2: words the CEO added while Rich was working.
    Steer {
        id: u64,
        /// The thread the ACTIVE TURN belongs to — captured at write time, never
        /// re-derived at drain time. Same reason `spine::Queued` carries a binding: the
        /// active context can move while a record waits, and a record that is re-scoped to
        /// wherever the CEO happens to be looking has laundered itself across an entity
        /// boundary (ECS §3.4).
        thread_id: String,
        /// The turn that was running when he typed it. Recorded as EVIDENCE of the
        /// "while Rich was working" cue, not as a delivery target — see the module doc.
        steering_turn_id: String,
        entity_id: Option<EntityId>,
        text: String,
        at: u64,
    },
    /// **The CEO speaking to Rich through something that is not the desktop window** —
    /// today a paired phone (phone-client plan §4.2 i, *"Inbound goes through the intake
    /// log, never straight at the spine"*).
    ///
    /// A SEPARATE VARIANT FROM [`IntakeRecord::Steer`], and the difference is the whole
    /// reason it exists. `Steer` means *words the CEO added while Rich was working*, and it
    /// carries `steering_turn_id` as the evidence behind that cue. A phone message usually
    /// arrives at an **idle** app, where there is no such turn — so reusing `Steer` would
    /// mean either inventing a turn id (an attribution with nothing behind it, which is the
    /// one thing this file refuses) or making the field optional and changing the shape of
    /// a record every shipped build can read today. A new tag is the honest third option:
    /// an older build meets an unknown `record` tag, classifies it, counts it and reports
    /// it (`IntakeLog::health`), and never silently drops it.
    ///
    /// Everything else about it is deliberately identical to `Steer`: same durability, same
    /// `id` as the de-duplication key, same single fate — it becomes a ledger turn or it is
    /// refused. `Spine::drain_intake` handles the two arms the same way, because from the
    /// spine's side the CEO's words are the CEO's words whatever mouth they came through
    /// (plan §4.2 iv: *"The back end never learns which mouth the CEO used"*).
    Channel {
        id: u64,
        /// The thread the message is FOR, captured at write time. Never re-derived at drain
        /// time, for the reason `Steer::thread_id` gives: the active context can move while
        /// a record waits, and re-scoping it to wherever the desktop happens to be looking
        /// would launder it across an entity boundary (ECS §3.4).
        thread_id: String,
        entity_id: Option<EntityId>,
        text: String,
        at: u64,
        /// WHICH MOUTH — `"phone"` today. A field rather than a variant, so a second
        /// channel is a new value here and not a new record type every older build then
        /// cannot read.
        channel: String,
        /// **THE SENDER'S OWN ID FOR THIS MESSAGE** — the `client_id` the phone assigned at
        /// Send and keeps for its line from the tap to the Mac's row (adoption ledger §2.8
        /// row C5: the client id is the identity end to end). Carried onto the turn so the
        /// row the Mac sends back names the message the phone is already showing, even when
        /// that row beats the phone's receipt. `None` for every record written before this
        /// field existed and for a channel that sent no id; an older build reading a record
        /// that has it ignores the key.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client_id: Option<String>,
        /// **THESE WORDS ARE THE TRANSCRIPT OF A VOICE MESSAGE THE SENDER RECORDED** (a phone
        /// voice note the Mac transcribed at intake), not words the sender typed. Carried onto
        /// the turn so every row of it, including the live one that can beat the phone's
        /// receipt, is a voice row from its first appearance. A property of the message, not of
        /// the mouth: the spine still never learns which device sent it. `false`, and no key,
        /// for every typed message and every record written before this field existed; an older
        /// build reading a record that has it ignores the key.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        voice_note: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<crate::ledger::Source>,
    },
    /// **The desktop composer's own words, written here because the front desk was being
    /// primed when he pressed Send** — the CEO's §55.
    ///
    /// # Why this is not a `Channel` with a new mouth, and not a `Steer`
    ///
    /// `Channel` means *the CEO spoke through something that is not the desktop window*,
    /// and `Spine::drain_intake` treats it accordingly: it files the turn and nothing else.
    /// `Steer` means *words he added while Rich was working* and needs a running turn to
    /// name. This is neither. It is an ORDINARY DESKTOP PROMPT that could not be handed to
    /// the spine at the instant he sent it, because `ready_the_front_desk` holds the spine's
    /// mutex for the whole of a priming turn (`src-tauri/src/main.rs`) — so it takes the one
    /// road into the spine that does not need that mutex, and is delivered down the ORDINARY
    /// prompt path the moment the prime lets go.
    ///
    /// The distinction is load-bearing rather than tidy. `submit_prompt_inner` is *"the ONE
    /// function every CEO utterance passes through"* and it stages three correction families
    /// off it; `drain_intake`'s `Steer` and `Channel` arms do not. Filing a typed message as
    /// a `Channel` would therefore have dropped every correction he typed during a prime,
    /// silently. This variant exists so that a deferred desktop send is INDISTINGUISHABLE
    /// from a direct one once it lands — same acceptance, same emits, same staging, same
    /// `Source::Text`.
    ///
    /// **What it buys, stated exactly, because it is not what it looks like.** It does NOT
    /// make his first words arrive sooner: the priming turn is a model turn and the lease is
    /// serial (continuity §3.1), so his prompt queues behind it whether it waits on a mutex
    /// or on this log. What it buys is that his sentence is `fsync`'d BEFORE the call
    /// returns — today it lives only in the webview until the prime lets go, and quitting in
    /// that window loses it — and that the Send is accepted immediately instead of blocking.
    Desk {
        id: u64,
        /// The thread he typed it into, captured at write time. Never re-derived at drain
        /// time, for the reason `Steer::thread_id` gives.
        thread_id: String,
        entity_id: Option<EntityId>,
        text: String,
        at: u64,
    },
    /// §9.3 step 1: "Persist a stop request."
    Stop { id: u64, turn_id: String, at: u64 },
    /// Everything with `id <= through` has reached the ledger or been refused. Written
    /// AFTER the ledger write, so a crash in between re-presents the record rather than
    /// losing it — at-least-once into a ledger whose turn ids make the replay detectable.
    Drained { through: u64 },
}

impl IntakeRecord {
    pub fn id(&self) -> u64 {
        match self {
            IntakeRecord::Steer { id, .. }
            | IntakeRecord::Channel { id, .. }
            | IntakeRecord::Desk { id, .. }
            | IntakeRecord::Stop { id, .. } => *id,
            IntakeRecord::Drained { through } => *through,
        }
    }
}

/// EVERY `record` tag this build knows how to replay.
///
/// It is the only thing that can tell a record written by a NEWER RichOS apart from a
/// damaged one, so it has to stay exactly in step with [`IntakeRecord`]. That is not left
/// to care: `intake_forward_compat_tests.rs` maps every variant through an EXHAUSTIVE
/// match, so adding a variant here without adding its name does not compile.
pub const KNOWN_INTAKE_TAGS: &[&str] = &["steer", "channel", "desk", "stop", "drained"];

/// How the intake log's records are told apart from a newer build's and from damage.
/// The judgment itself is [`crate::skip::classify_line`] — the same one the ledger uses,
/// deliberately, so the two stores can never disagree about what damage looks like.
const INTAKE_SKIP: SkipDialect = SkipDialect {
    tag_key: "record",
    record_noun: "an intake record",
    known_tags: KNOWN_INTAKE_TAGS,
};

/// **What the app can honestly say about the intake log it has just replayed.**
///
/// The intake log is the ONE file that holds the CEO's own words before anything has
/// happened to them: a steering message he typed while Rich was working, or a stop he
/// pressed, fsync'd the instant he acted and not yet turned into a ledger turn. A record
/// dropped here is not a degraded feature — it is something he said to RichOS that never
/// arrives, and that nobody ever learns was lost.
///
/// So this exists for one reason: **so the drop cannot be silent.** It is deliberately NOT
/// a reason to refuse to start. This is a read path on the CEO's own machine, and refusing
/// to open the app is not an improvement over dropping one message.
///
/// `headline` and `detail` are empty strings when nothing was skipped, which is the
/// ordinary case and the signal to say nothing at all. There is no reassuring "the intake
/// log is fine" state, for the same reason `Ledger::history_health` has none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IntakeHealth {
    /// Non-empty lines found in the file.
    pub records_read: usize,
    /// Records replayed into the pending set (or into the drain watermark).
    pub records_applied: usize,
    pub skipped: usize,
    pub from_future: usize,
    pub damaged: usize,
    pub ambiguous: usize,
    /// One short sentence. Empty when nothing was skipped.
    pub headline: String,
    /// The honest explanation, in plain American English, with no stack trace in it.
    /// Empty when nothing was skipped.
    pub detail: String,
}

impl IntakeHealth {
    /// Every record on disk is accounted for. The normal state, and the state of every
    /// intake log in existence today.
    pub fn is_clean(&self) -> bool {
        self.skipped == 0
    }
}

/// The append-only intake file: the CEO's words and the CEO's stop, durable before
/// anything acts on them.
///
/// Same durability posture as the ledger (append + `sync_all`, one JSON object per line,
/// a malformed line skipped-and-REPORTED rather than fatal) and deliberately NOT the same
/// file: the ledger is evidence of what happened, and an intake record is a request that
/// has not happened yet.
pub struct IntakeLog {
    path: PathBuf,
    /// Records written and not yet drained, oldest first.
    pending: Vec<IntakeRecord>,
    next_id: u64,
    /// Lines currently in the file, so a long-lived log can be compacted once it is fully
    /// drained rather than growing for the life of the install.
    lines: usize,
    /// Every record on disk that this build could not replay, with the reason for each.
    /// Empty is the normal state and the state of every intake log in existence today.
    skipped: Vec<SkippedRecord>,
    /// Non-empty lines seen on replay, and how many were folded. Counted rather than
    /// derived, so "3 of 4" is a measurement and not an inference.
    records_read: usize,
    records_applied: usize,
    /// Every Stop on disk, drained or not, oldest first. A drained Stop still marks where
    /// his cut fell: a card answer accepted before it and delivered after a restart is
    /// covered by it (review rv-20261018T204635Z-c68a30ec-f61e#2). Compaction keeps the
    /// latest [`KEPT_STOPS`] of them.
    stops: Vec<IntakeRecord>,
}

/// How many Stop records a compaction keeps, so each recent cut survives it.
const KEPT_STOPS: usize = 16;

/// Compact the file once it is fully drained and has grown past this many lines. Chosen so
/// compaction is rare (a drained log is two lines per CEO action) and never happens while
/// anything is pending.
const COMPACT_AFTER_LINES: usize = 256;

impl IntakeLog {
    /// Open (creating if absent) and replay. Undrained records survive a crash and are
    /// returned by [`pending`](Self::pending) for the spine to reconcile at startup.
    ///
    /// # A dropped record here is the CEO's own words, so it is never dropped in silence
    ///
    /// Until 2026-09-05 this loop was `let Ok(rec) = .. else { continue }` with a comment
    /// explaining that a torn LAST line was never acknowledged to anybody. That reasoning
    /// is sound for the case it names and wrong for every other case: the same `continue`
    /// fired for a record written by a newer RichOS, for a record damaged in the MIDDLE of
    /// the file, and for a record shaped differently — and every one of them went with no
    /// count, no log line and no trace. This file holds what he typed BEFORE it becomes a
    /// turn, so that silence is a message he sent that never arrives and that nobody ever
    /// learns was lost.
    ///
    /// Three things changed, and none of them makes the read fatal:
    ///
    ///   1. **Every skipped record is classified and counted** ([`crate::skip`] — the same
    ///      judgment `Ledger::replay` uses). A torn final record and a record from a newer
    ///      build are benign and are labeled as such; a record damaged mid-file is not.
    ///   2. **The read no longer stops at a bad byte.** `lines().map_while(Result::ok)`
    ///      ends the ITERATOR at the first non-UTF-8 line, so one bad byte silently
    ///      discarded every record AFTER it, not just its own. `read_until` classifies that
    ///      line and carries on.
    ///   3. **A skipped record can no longer collide with a fresh one** — see the `next_id`
    ///      derivation below.
    ///
    /// Nothing is deleted and nothing is rewritten. A record this build cannot read stays
    /// exactly where it is, so a build new enough to understand it will read it — which is
    /// also why [`compact`](Self::compact) refuses to run while anything was skipped.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SteeringError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut records: Vec<IntakeRecord> = Vec::new();
        let mut drained_through: u64 = 0;
        let mut lines = 0usize;
        let mut skipped: Vec<SkippedRecord> = Vec::new();
        let mut records_read = 0usize;
        let mut records_applied = 0usize;
        // The highest id seen on a record this build could NOT fold. It is not in
        // `records`, so without it the counter can hand the same id out twice.
        let mut salvaged_id: u64 = 0;
        let mut stops: Vec<IntakeRecord> = Vec::new();
        if path.exists() {
            let file = std::fs::File::open(&path)?;
            let mut reader = BufReader::new(file);
            let mut raw: Vec<u8> = Vec::new();
            let mut line_no = 0usize;
            loop {
                raw.clear();
                // `read_until`, not `lines()`: `lines()` yields `Err` for a line that is
                // not valid UTF-8 and `map_while(Result::ok)` STOPS THERE, so one bad byte
                // used to take every request after it with it. A real IO error still
                // propagates from here; a bad byte is classified below.
                let n = reader.read_until(b'\n', &mut raw)?;
                if n == 0 {
                    break;
                }
                line_no += 1;
                let body = raw.strip_suffix(b"\n").unwrap_or(&raw);
                let text = match std::str::from_utf8(body) {
                    Ok(t) => t,
                    Err(_) => {
                        lines += 1;
                        records_read += 1;
                        skipped.push(crate::skip::not_utf8(line_no, body.len()));
                        continue;
                    }
                };
                let line = text.trim();
                if line.is_empty() {
                    continue;
                }
                lines += 1;
                records_read += 1;
                match serde_json::from_str::<IntakeRecord>(line) {
                    Ok(rec) => {
                        records_applied += 1;
                        match rec {
                            IntakeRecord::Drained { through } => {
                                drained_through = drained_through.max(through)
                            }
                            stop @ IntakeRecord::Stop { .. } => {
                                stops.push(stop.clone());
                                records.push(stop)
                            }
                            other => records.push(other),
                        }
                    }
                    Err(_) => {
                        let record = crate::skip::classify_line(line_no, line, &INTAKE_SKIP);
                        // SALVAGE THE ID, AND FROM ONE MORE KIND THAN THE LEDGER DOES.
                        //
                        // `Ledger::replay` salvages a fencing revision from `FromFuture`
                        // only, on the rule that a damaged line's numbers are not facts.
                        // That rule holds; the LINE is drawn differently here because the
                        // stakes are not symmetric. An intake id is a de-duplication key —
                        // `Spine::drain_intake` treats "the ledger already holds a turn for
                        // this id" as proof the record was handled — so a COLLISION
                        // discards a real message, while an inflated counter costs nothing
                        // at all (ids are opaque and monotonic, and `u64` does not run
                        // out). `Ambiguous` and `FromFuture` are both well-formed JSON
                        // objects carrying a plain-identifier tag, which is structure this
                        // build trusts; `Damaged` is not, and nothing is salvaged from it.
                        if record.kind != SkipKind::Damaged {
                            if let Some(id) = crate::skip::salvage_u64(line, "id") {
                                salvaged_id = salvaged_id.max(id);
                            }
                            if let Some(through) = crate::skip::salvage_u64(line, "through") {
                                salvaged_id = salvaged_id.max(through);
                            }
                        }
                        skipped.push(record);
                    }
                }
            }
        }
        // THE HIGH-WATER MARK OF EVERY ID THIS FILE HAS EVER USED, taken from all three
        // places one can appear. The old derivation was
        // `max(parsed ids).unwrap_or(drained_through) + 1`, which DROPS `drained_through`
        // the moment any record at all parses: a file holding `Steer{1}, Steer{2},
        // Drained{2}` whose `Steer{2}` is unreadable re-issued id 2, and two records with
        // one id make `Ledger::turn_for_intake` match the wrong turn and throw a real
        // message away as already handled.
        let highest = records
            .iter()
            .map(|r| r.id())
            .max()
            .unwrap_or(0)
            .max(drained_through)
            .max(salvaged_id);
        let next_id = highest.saturating_add(1);
        records.retain(|r| r.id() > drained_through);
        let log =
            IntakeLog { path, pending: records, next_id, lines, skipped, records_read, records_applied, stops };
        log.report_skipped();
        Ok(log)
    }

    /// **Skipping is never silent.** One line per skipped record, capped, then a summary
    /// line printed whenever anything at all was skipped.
    ///
    /// The cap exists because a badly damaged file could hold a hundred thousand
    /// unreadable lines, and a hundred thousand log lines is its own kind of silence. The
    /// COUNT is never capped — it is in the summary and in [`IntakeLog::health`].
    fn report_skipped(&self) {
        if self.skipped.is_empty() {
            return;
        }
        const CAP: usize = 20;
        for r in self.skipped.iter().take(CAP) {
            eprintln!(
                "[richos] INTAKE RECORD SKIPPED ({}): line {} ({} bytes) — {}",
                r.kind.label(),
                r.line,
                r.bytes,
                r.detail
            );
        }
        if self.skipped.len() > CAP {
            eprintln!(
                "[richos] INTAKE: {} further skipped records not listed individually",
                self.skipped.len() - CAP
            );
        }
        let h = self.health();
        eprintln!(
            "[richos] INTAKE SUMMARY for {}: {} records read, {} applied, {} skipped \
             ({} from a newer version, {} damaged, {} undetermined). These are things the \
             CEO asked for that Rich has not acted on yet. Nothing was deleted or rewritten.",
            self.path.display(),
            h.records_read,
            h.records_applied,
            h.skipped,
            h.from_future,
            h.damaged,
            h.ambiguous
        );
    }

    /// Every record on disk this build could not replay, with the reason for each.
    pub fn skipped_records(&self) -> &[SkippedRecord] {
        &self.skipped
    }

    /// **What the app can honestly say about the intake log it just replayed.**
    ///
    /// `headline`/`detail` are empty when nothing was skipped — the signal to say nothing
    /// at all, not to say something reassuring about a check that found nothing to report.
    ///
    /// The sentences are composed HERE and deliberately not shared with
    /// `Ledger::history_health`, because they are not the same statement: that one is about
    /// messages that are on screen and did not load, and this one is about something the
    /// CEO said that never reached Rich at all. Nothing should be in a position to
    /// substitute one for the other.
    pub fn health(&self) -> IntakeHealth {
        let count = |k: SkipKind| self.skipped.iter().filter(|r| r.kind == k).count();
        let from_future = count(SkipKind::FromFuture);
        let damaged = count(SkipKind::Damaged);
        let ambiguous = count(SkipKind::Ambiguous);
        let skipped = self.skipped.len();

        let (headline, detail) = if skipped == 0 {
            (String::new(), String::new())
        } else {
            let plural =
                |n: usize, one: &str, many: &str| if n == 1 { one.to_string() } else { many.to_string() };
            let headline = if damaged > 0 || ambiguous > 0 {
                plural(
                    damaged + ambiguous,
                    "Something you typed while Rich was working could not be read.",
                    "Some things you typed while Rich was working could not be read.",
                )
            } else {
                "Something you typed while Rich was working was written by a newer version of \
                 RichOS."
                    .to_string()
            };
            let mut parts: Vec<String> = Vec::new();
            if from_future > 0 {
                parts.push(format!(
                    "{} {} written by a newer version of RichOS than the one you are running, \
                     so this version does not know how to read {}. Updating will bring {} back.",
                    from_future,
                    plural(from_future, "request was", "requests were"),
                    plural(from_future, "it", "them"),
                    plural(from_future, "it", "them"),
                ));
            }
            if damaged > 0 {
                parts.push(format!(
                    "{} {} damaged and could not be read. If you were waiting on Rich to act on \
                     {}, {} never reached him — please ask again.",
                    damaged,
                    plural(damaged, "request is", "requests are"),
                    plural(damaged, "it", "them"),
                    plural(damaged, "it", "they"),
                ));
            }
            if ambiguous > 0 {
                parts.push(format!(
                    "{} {} not match any shape this version knows. That is either a newer \
                     version of RichOS or damage, and this version cannot tell which.",
                    ambiguous,
                    plural(ambiguous, "request does", "requests do"),
                ));
            }
            parts.push(format!(
                "Everything else was read: {} of {} requests. Nothing was deleted and nothing \
                 was rewritten: every record is still exactly where it was on disk.",
                self.records_applied, self.records_read,
            ));
            (headline, parts.join(" "))
        };

        IntakeHealth {
            records_read: self.records_read,
            records_applied: self.records_applied,
            skipped,
            from_future,
            damaged,
            ambiguous,
            headline,
            detail,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Undrained records, oldest first.
    pub fn pending(&self) -> &[IntakeRecord] {
        &self.pending
    }

    /// Every Stop on disk, drained or not, oldest first.
    pub fn stops(&self) -> &[IntakeRecord] {
        &self.stops
    }

    fn write(&mut self, rec: &IntakeRecord) -> Result<(), SteeringError> {
        // Stamped with this build's `written_by` — spec point 18. This log holds the CEO's
        // own typed words, so it is the last file that should ever leave a reader guessing
        // between "a newer RichOS wrote this" and "these bytes are damaged".
        let mut line = crate::skip::stamped_line(rec).map_err(|e| SteeringError::Io(e.to_string()))?;
        line.push('\n');
        let mut f = std::fs::OpenOptions::new().create(true).read(true).append(true).open(&self.path)?;
        crate::util::ensure_line_boundary(&mut f)?;
        f.write_all(line.as_bytes())?;
        // fsync, not flush. §9.2's "persisted before delivery" and §9.3's "persist a stop
        // request" are both statements about surviving the power going out, and a page
        // cache is not disk.
        f.sync_all()?;
        self.lines += 1;
        Ok(())
    }

    /// Durably record one message that arrived from a channel other than the desktop
    /// window — the phone (plan §4.2 i). Returns it, carrying the `id` the caller answers
    /// the phone with.
    ///
    /// **`fsync` before the caller can answer** is the whole point, and it is why the phone
    /// bridge writes here instead of calling the spine: `Spine::submit_prompt` does not
    /// return until the turn is over, and an HTTP request that waits for a two-hour turn is
    /// a request that times out. This returns in the time one `fsync` takes, and the
    /// message is on disk when it does.
    pub fn channel_message(
        &mut self,
        thread_id: &str,
        entity_id: Option<EntityId>,
        text: &str,
        channel: &str,
    ) -> Result<IntakeRecord, SteeringError> {
        self.channel_message_from(thread_id, entity_id, text, channel, None, false)
    }

    /// As [`Self::channel_message`], keeping the sender's own id for the message
    /// ([`IntakeRecord::Channel`]'s `client_id`). Written in the same record as the words, in
    /// the same `fsync`, so no drain can ever turn the words into a turn without it. The same
    /// holds for `voice_note` ([`IntakeRecord::Channel`]'s `voice_note`): whether the words are
    /// the transcript of a recording is written with them, never inferred later.
    pub fn channel_message_from(
        &mut self,
        thread_id: &str,
        entity_id: Option<EntityId>,
        text: &str,
        channel: &str,
        client_id: Option<&str>,
        voice_note: bool,
    ) -> Result<IntakeRecord, SteeringError> {
        let rec = IntakeRecord::Channel {
            id: self.next_id,
            thread_id: thread_id.to_string(),
            entity_id,
            text: text.to_string(),
            at: now_millis(),
            channel: channel.to_string(),
            client_id: client_id.map(str::to_string),
            voice_note,
            source: None,
        };
        self.write(&rec)?;
        self.next_id += 1;
        self.pending.push(rec.clone());
        Ok(rec)
    }

    /// Durably record one DESKTOP prompt that could not reach the spine at the moment he
    /// sent it, because the front desk was being primed ([`IntakeRecord::Desk`]).
    ///
    /// `fsync` before the caller can answer, exactly as [`Self::channel_message`] is and for
    /// the same reason: the spine's mutex is held for the whole of a turn, and the CEO's
    /// words must be on disk before the window is told anything at all.
    pub fn desk_message(
        &mut self,
        thread_id: &str,
        entity_id: Option<EntityId>,
        text: &str,
    ) -> Result<IntakeRecord, SteeringError> {
        let rec = IntakeRecord::Desk {
            id: self.next_id,
            thread_id: thread_id.to_string(),
            entity_id,
            text: text.to_string(),
            at: now_millis(),
        };
        self.write(&rec)?;
        self.next_id += 1;
        self.pending.push(rec.clone());
        Ok(rec)
    }

    /// Durably record one steering message. Returns its id.
    pub fn steer(
        &mut self,
        thread_id: &str,
        steering_turn_id: &str,
        entity_id: Option<EntityId>,
        text: &str,
    ) -> Result<IntakeRecord, SteeringError> {
        let rec = IntakeRecord::Steer {
            id: self.next_id,
            thread_id: thread_id.to_string(),
            steering_turn_id: steering_turn_id.to_string(),
            entity_id,
            text: text.to_string(),
            at: now_millis(),
        };
        self.write(&rec)?;
        self.next_id += 1;
        self.pending.push(rec.clone());
        Ok(rec)
    }

    /// Durably record one stop request, including its timestamp.
    pub fn stop(&mut self, turn_id: &str) -> Result<IntakeRecord, SteeringError> {
        let rec = IntakeRecord::Stop { id: self.next_id, turn_id: turn_id.to_string(), at: now_millis() };
        self.write(&rec)?;
        self.next_id += 1;
        self.pending.push(rec.clone());
        self.stops.push(rec.clone());
        Ok(rec)
    }

    /// Mark everything up to and including `through` as handled. Called AFTER the ledger
    /// write, never before.
    pub fn mark_drained(&mut self, through: u64) -> Result<(), SteeringError> {
        self.write(&IntakeRecord::Drained { through })?;
        self.pending.retain(|r| r.id() > through);
        // COMPACTION IS REFUSED WHILE ANYTHING WAS SKIPPED, and this is the difference
        // between "an older build cannot read your steering message" and "an older build
        // DELETED your steering message". `compact` replaces the whole file with a single
        // marker; a record this build could not parse is not in `pending`, so without this
        // clause a record written by a newer RichOS — the benign, recoverable case, the one
        // an update is supposed to bring back — would be destroyed by the next compaction
        // and would never come back at all. An uncompacted log costs some lines on disk.
        if self.pending.is_empty() && self.skipped.is_empty() && self.lines >= COMPACT_AFTER_LINES {
            self.compact(through)?;
        }
        Ok(())
    }

    /// Replace a fully-drained file with a single marker.
    ///
    /// Write-new-then-rename, so a crash mid-compaction leaves the ORIGINAL file intact:
    /// truncating in place would put the one file that records un-acted-on CEO requests
    /// into a window where it is empty and the requests are gone.
    fn compact(&mut self, through: u64) -> Result<(), SteeringError> {
        debug_assert!(self.pending.is_empty());
        debug_assert!(self.skipped.is_empty(), "compaction would delete a record nothing could read");
        let tmp = self.path.with_extension("compacting");
        // The latest Stops first, so their cuts outlive the compaction; the marker after
        // them drains them on replay as before.
        let kept = self.stops.split_off(self.stops.len().saturating_sub(KEPT_STOPS));
        let mut line = String::new();
        for rec in kept.iter().chain(std::iter::once(&IntakeRecord::Drained { through })) {
            line.push_str(&crate::skip::stamped_line(rec).map_err(|e| SteeringError::Io(e.to_string()))?);
            line.push('\n');
        }
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(line.as_bytes())?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, &self.path)?;
        self.lines = kept.len() + 1;
        self.stops = kept;
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------
// THE CANCEL SEAM
// ---------------------------------------------------------------------------------------

/// The outcome of the native turn that consumed submitted input.
pub struct InputOutcome {
    pub result: Result<String, String>,
    /// A folded native turn reports once, even if it consumed several inputs.
    pub report_failure: bool,
}

/// A handle that can interrupt the currently running turn from outside the spine lock.
/// Stop runs on another thread and never takes the lock held by that turn.
pub trait TurnCancel: Send + Sync {
    /// Apply a native model setting without canceling or replacing this session.
    fn set_model(&self, _model:crate::config::ModelChoice)->Result<bool,String> {Ok(false)}

    /// Worker facts from this live session's own task frames.
    fn worker_status(&self) -> Option<crate::worker_status::WorkerStatusView> { None }

    /// Queue ordinary input on this same session without interrupting it. The
    /// callback runs when the native turn ends. Admission alone is not completion.
    /// False means this provider has no live input path; leave the inbox pending.
    fn send_input(&self, _id: &str, _text: &str, _taken: Box<dyn FnOnce(InputOutcome) + Send>) -> Result<bool, String> {
        Ok(false)
    }

    /// Scope cancellation across preparation and the gaps between wire prompts.
    fn begin_operation(&self) {}
    fn end_operation(&self) {}

    /// Ask the lease to stop. Returns whether the signal was actually delivered — `false`
    /// means the lease had nothing to cancel (no prompt in flight, or the child is gone),
    /// which the caller reports rather than hides.
    fn cancel(&self) -> bool;
    /// Settle owned processes on app exit even when no turn is in flight.
    fn shutdown(&self) { let _ = self.cancel(); }
    /// Ask the lease to stop ONE command its provider is running in the background, by the
    /// provider's own task id, and say what the PROVIDER answered — never only that the
    /// request was written (hunt 2026-09-29 part 1, finding 11, and its v2 re-check: a
    /// written request the provider then refused was reported as a stopped command). The
    /// default is [`CommandStop::NotDelivered`], because a lease that cannot stop a command
    /// must say so rather than let a stop be reported as having reached it.
    fn stop_background_command(&self, _task_id: &str) -> CommandStop { CommandStop::NotDelivered }

    /// Stop the native agent launched by this exact prepared tool call.
    fn stop_worker(&self, _tool_use_id: &str) -> CommandStop { CommandStop::NotDelivered }

    /// The shell commands this session's own model started that are still running, foreground
    /// or background. [`Self::stop_background_command`] stops one by its task id.
    fn own_commands(&self) -> Vec<OwnCommand> { Vec::new() }
}

/// One shell command a session's own model started, as its stream reported it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnCommand {
    /// The provider's task id.
    pub task_id: String,
    /// When its `task_started` was read (ms since the epoch).
    pub started_at_ms: u64,
    /// The call's own `description`, where Stu names the job the command is for (stu.md).
    pub description: String,
}

/// What became of a request to stop one background command, from the provider's own frames.
///
/// Four states because the caller says four different true sentences. Only [`Self::Ended`]
/// lets it say the command stopped: that is the provider's own `task_notification` for the
/// task, the frame the native client already treats as the authority for an ending.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandStop {
    /// The provider reported the command ended.
    Ended,
    /// The provider answered the stop with an error; the command is still running. Carries
    /// the provider's own reason, for the log.
    Refused(String),
    /// The stop was delivered and not refused, but no ending was seen within the bound. It
    /// may still be running; nothing is claimed either way.
    Unconfirmed,
    /// The stop could not be written to the lease (no lease handle, or its input is gone).
    NotDelivered,
}

// ---------------------------------------------------------------------------------------
// THE CONTROL HANDLE
// ---------------------------------------------------------------------------------------

/// The turn that is running RIGHT NOW, as far as anything outside the spine lock can see.
///
/// Maintained by the spine at exactly the two points that already exist —
/// `mark_turn_started` and the terminal event — so this is a mirror of `turn_in_progress`
/// rather than a second opinion about it. It is not inferred from silence, from timers, or
/// from the absence of events (continuity §5.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveTurn {
    pub turn_id: String,
    pub thread_id: String,
    pub entity_id: Option<EntityId>,
    /// `None` for a turn that is journaled but not yet handed to a lease (§11 `queued`) —
    /// never `now()`, so nothing downstream can compute a duration that was not measured.
    pub started_at: Option<u64>,
}

/// A stop the CEO asked for, and the moment the request hit the disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StopClaim {
    pub turn_id: String,
    /// The intake record's id — the ordering key that decides which steering messages this
    /// stop cancels.
    pub intake_id: u64,
    /// When the request became durable. This is what `Ledger::stop_turn` quotes.
    pub requested_at: u64,
}

impl StopClaim {
    /// Stop covers its named turn and inputs accepted before its durable intake record.
    /// A later explicit submission is a new request, including after a provider closes.
    pub(crate) fn covers(&self, turn_id: &str, intake_id: Option<u64>) -> bool {
        self.turn_id == turn_id || intake_id.is_some_and(|id| id < self.intake_id)
    }
}

/// What actually happened when the CEO pressed stop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopOutcome {
    /// Nothing was running. Not an error and not a failure — the honest answer, and the
    /// UI's cue to leave the composer exactly as it was.
    NothingRunning,
    /// The request is on disk and the lease was told.
    Requested {
        turn_id: String,
        requested_at: u64,
        /// Whether the cancel signal reached a lease at all. `false` is a real and
        /// reportable state: the request is still durable and the turn will still be
        /// recorded as stopped when it ends, but nothing was there to interrupt it, so it
        /// may run to completion first. Never smoothed over into `true`.
        reached_lease: bool,
    },
}

#[derive(Default)]
struct ConversationState {
    active: Option<ActiveTurn>,
    stop: Option<StopClaim>,
    /// The last Stop cleared at a boundary. Its cut still covers a card answer accepted
    /// before it and delivered only after it settled.
    settled: Option<StopClaim>,
    cancel: Option<Arc<dyn TurnCancel>>,
    lease_session: Option<String>,
}

struct Inner {
    intake: Mutex<Option<IntakeLog>>,
    conversations: Mutex<std::collections::HashMap<String, ConversationState>>,
    /// Preparation claims by conversation. The guard covers only claim changes and
    /// durable intake writes. Lock order is SPINE then claims, or claims then INTAKE;
    /// no path takes SPINE while holding claims or holds INTAKE while taking claims.
    priming: Mutex<std::collections::HashSet<String>>,
}

/// The shared, lock-free-with-respect-to-the-spine control surface for §9.2 and §9.3.
///
/// Single-conversation actions require an explicit view:
/// ```compile_fail,E0599
/// let controls=richos_core::steering::TurnControl::detached();
/// controls.request_stop();
/// ```
///
/// Cloning clones the `Arc`: the Tauri shell keeps one handle beside the `Mutex<Spine>`
/// and the spine holds the same one, so a stop command never queues behind a turn.
#[derive(Clone)]
pub struct TurnControl {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for TurnControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TurnControl").field("active_count", &self.active_turn_count()).finish_non_exhaustive()
    }
}

impl TurnControl {
    /// A control with a durable intake log — the only shape the shipping app uses.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SteeringError> {
        let log = IntakeLog::open(path)?;
        Ok(TurnControl {
            inner: Arc::new(Inner {
                intake: Mutex::new(Some(log)),
                conversations: Mutex::new(std::collections::HashMap::new()),
                priming: Mutex::new(Default::default()),
            }),
        })
    }

    /// A control with NOWHERE DURABLE TO WRITE. Turn bookkeeping still works, so headless
    /// runs and every existing test keep behaving exactly as before, but `request_stop`
    /// and `steer` refuse with [`SteeringError::NoDurableIntake`] instead of quietly
    /// acting on a request that was never recorded.
    pub fn detached() -> Self {
        TurnControl {
            inner: Arc::new(Inner {
                intake: Mutex::new(None),
                conversations: Mutex::new(std::collections::HashMap::new()),
                priming: Mutex::new(Default::default()),
            }),
        }
    }

    pub fn is_durable(&self) -> bool {
        self.inner.intake.lock().unwrap().is_some()
    }

    pub fn intake_path(&self) -> Option<PathBuf> {
        self.inner.intake.lock().unwrap().as_ref().map(|l| l.path().to_path_buf())
    }

    /// Every Stop on disk, drained or not, oldest first (see [`IntakeLog::stops`]).
    pub fn stop_records(&self) -> Vec<IntakeRecord> {
        self.inner.intake.lock().unwrap().as_ref().map(|l| l.stops().to_vec()).unwrap_or_default()
    }

    // --- written by the spine, at the points that already exist -------------------------

    /// A view of the same durable intake and controls for one conversation.
    pub fn for_thread(&self, thread: &str) -> ConversationControl {
        ConversationControl {shared:self.clone(),thread:thread.to_string()}
    }

    pub fn active_turn_count(&self) -> usize {
        self.inner.conversations.lock().unwrap().values().filter(|state|state.active.is_some()).count()
    }

    pub fn has_active_turns(&self) -> bool {self.active_turn_count()!=0}

    pub fn has_active_turn_id(&self,turn_id:&str)->bool {
        self.inner.conversations.lock().unwrap().values()
            .any(|state|state.active.as_ref().is_some_and(|turn|turn.turn_id==turn_id))
    }

    pub fn end_turn(&self, turn_id: &str) {
        let mut states=self.inner.conversations.lock().unwrap();
        for state in states.values_mut().filter(|state|state.active.as_ref().is_some_and(|turn|turn.turn_id==turn_id)) {
            state.active=None;
            if let Some(cancel)=state.cancel.as_ref() {cancel.end_operation();}
        }
    }

    fn record_stop(&self,state:&mut ConversationState)->Result<(String,u64),SteeringError> {
        let active=state.active.as_ref().expect("only active turns are stopped");
        let record=self.inner.intake.lock().unwrap().as_mut().ok_or(SteeringError::NoDurableIntake)?.stop(&active.turn_id)?;
        let IntakeRecord::Stop{id,at,..}=record else {unreachable!()};
        state.stop=Some(StopClaim{turn_id:active.turn_id.clone(),intake_id:id,requested_at:at});
        Ok((active.turn_id.clone(),at))
    }

    /// Persist all active Stops before any cancellation or lease shutdown.
    pub fn request_stop_all(&self)->Result<Vec<StopOutcome>,SteeringError> {
        let mut states=self.inner.conversations.lock().unwrap();
        let mut saved=Vec::new();
        for (thread,state) in states.iter_mut().filter(|(_,state)|state.active.is_some()) {
            let (turn,at)=self.record_stop(state)?;
            saved.push((thread.clone(),turn,at));
        }
        Ok(saved.into_iter().map(|(thread,turn_id,requested_at)| {
            let reached_lease=states[&thread].cancel.as_ref().is_some_and(|cancel|cancel.cancel());
            StopOutcome::Requested{turn_id,requested_at,reached_lease}
        }).collect())
    }

    pub fn shutdown_leases(&self) {
        let cancels:Vec<_>=self.inner.conversations.lock().unwrap().values().filter_map(|state|state.cancel.clone()).collect();
        for cancel in cancels {cancel.shutdown();}
    }

    /// Update live Rich sessions. Copy control handles before provider I/O.
    pub fn set_model(&self,model:crate::config::ModelChoice)->Result<(),String> {
        let handles:Vec<_>=self.inner.conversations.lock().unwrap().values().filter_map(|state|state.cancel.clone()).collect();
        for handle in handles {handle.set_model(model)?;}
        Ok(())
    }

    pub fn lease_sessions(&self)->Vec<String> {
        self.inner.conversations.lock().unwrap().iter().filter(|(thread,_)|!thread.is_empty()).filter_map(|(_,state)|state.lease_session.clone()).collect()
    }

    /// Read every conversation's native worker facts without taking the spine lock.
    pub fn worker_statuses(&self)->Vec<crate::worker_status::WorkerStatusView> {
        let controls:Vec<_>=self.inner.conversations.lock().unwrap().values()
            .filter(|state|state.lease_session.is_some())
            .map(|state|state.cancel.clone()).collect();
        controls.into_iter().map(|handle|handle.and_then(|h|h.worker_status())
            .unwrap_or_else(||crate::worker_status::WorkerStatusView::unattributed(
                crate::worker_status::Unattributed::AppEvidenceUnavailable))).collect()
    }

    /// **The phone's road in** (plan §4.2 i). Durable on return, and it does NOT require a
    /// turn to be running — which is the one way it differs from [`ConversationControl::steer`],
    /// and the reason it is a different function rather than a relaxed flag on that one.
    ///
    /// `steer` REFUSES when nothing is running, correctly: "add this to what Rich is
    /// doing" is meaningless when Rich is doing nothing, and the desktop composer sends a
    /// prompt instead. The phone has no such fork — it posts one message and the Mac
    /// decides. So the two entry points answer two different questions and neither one
    /// should have to guess which it was asked.
    ///
    /// Reached WITHOUT the spine lock, exactly as `steer` and `request_stop` are, because
    /// `Spine::submit_prompt` holds that lock for the whole of a turn. Nothing here waits
    /// on Rich.
    pub fn submit_from_channel(
        &self,
        thread_id: &str,
        entity_id: Option<EntityId>,
        text: &str,
        channel: &str,
    ) -> Result<IntakeRecord, SteeringError> {
        self.submit_from_channel_as(thread_id, entity_id, text, channel, None)
    }

    /// As [`Self::submit_from_channel`], with the sender's own id for the message (the phone's
    /// `client_id`), so the turn and every row of it carry the identity the phone already shows
    /// ([`IntakeRecord::Channel`]'s `client_id`).
    pub fn submit_from_channel_as(
        &self,
        thread_id: &str,
        entity_id: Option<EntityId>,
        text: &str,
        channel: &str,
        client_id: Option<&str>,
    ) -> Result<IntakeRecord, SteeringError> {
        self.submit_channel_record(thread_id, entity_id, text, channel, client_id, false)
    }

    /// As [`Self::submit_from_channel_as`], for words that are the TRANSCRIPT OF A VOICE MESSAGE
    /// the sender recorded (a phone voice note), so every row of the turn is a voice row from its
    /// first appearance, the live one that can beat the sender's receipt included
    /// ([`IntakeRecord::Channel`]'s `voice_note`).
    pub fn submit_voice_note_from_channel(
        &self,
        thread_id: &str,
        entity_id: Option<EntityId>,
        text: &str,
        channel: &str,
        client_id: Option<&str>,
    ) -> Result<IntakeRecord, SteeringError> {
        self.submit_channel_record(thread_id, entity_id, text, channel, client_id, true)
    }

    pub fn submit_agent_message(&self, thread_id: &str, entity_id: Option<EntityId>, text: &str, sender: &str, message_id: &str) -> Result<IntakeRecord, SteeringError> {
        let mut guard = self.inner.intake.lock().unwrap();
        let log = guard.as_mut().ok_or(SteeringError::NoDurableIntake)?;
        let rec = IntakeRecord::Channel { id: log.next_id, thread_id: thread_id.into(), entity_id, text: text.into(), at: now_millis(), channel: sender.into(), client_id: Some(message_id.into()), voice_note: false, source: Some(crate::ledger::Source::Agent) };
        log.write(&rec)?;
        log.next_id += 1;
        log.pending.push(rec.clone());
        Ok(rec)
    }

    fn submit_channel_record(
        &self,
        thread_id: &str,
        entity_id: Option<EntityId>,
        text: &str,
        channel: &str,
        client_id: Option<&str>,
        voice_note: bool,
    ) -> Result<IntakeRecord, SteeringError> {
        let mut guard = self.inner.intake.lock().unwrap();
        let log = guard.as_mut().ok_or(SteeringError::NoDurableIntake)?;
        log.channel_message_from(thread_id, entity_id, text, channel, client_id, voice_note)
    }

    /// Reserve the existing intake order for an input saved directly in the ledger.
    /// Its normal ledger fsync supplies durability, without another write before native stdin.
    /// A question-card answer takes its order here when the app accepts the click, and the
    /// question store keeps it until the answer reaches the ledger.
    pub fn take_input_order(&self) -> Option<u64> {
        let mut guard = self.inner.intake.lock().unwrap();
        let log = guard.as_mut()?;
        let id = log.next_id;
        log.next_id += 1;
        Some(id)
    }

    /// Direct ledger inputs may have no intake-log row. Never reuse their durable IDs,
    /// including an order the question store saved with an answer not yet delivered.
    pub fn advance_input_order(&self, through: u64) {
        if let Some(log) = self.inner.intake.lock().unwrap().as_mut() {
            log.next_id = log.next_id.max(through.saturating_add(1));
        }
    }

    /// **The DESK's road in, and it is the same road for the same reason** — the one the
    /// window takes for every typed sentence, not only the ones that land while the front
    /// desk is being primed.
    ///
    /// [`Self::defer_send`] writes exactly this record and answers `None` unless a prime is
    /// in flight. That gate made sense while the only thing the window gained was skipping a
    /// wait it could see: the screen said *"Sending your message"* and the CEO watched it.
    /// It stopped making sense the day a PHONE was looking at the same thread, because his
    /// own sentence cannot reach that phone until his words are somewhere durable, and the
    /// ledger is behind the one mutex a turn holds for its whole length
    /// (`Ledger::record_prompt_received` is `&mut self`). Measured through the shipped
    /// listener: the phone path itself costs **5 ms**, and a spine held for 1,500 ms put his
    /// row on the phone at 1,507 ms (`phone::listen`'s two timing tests).
    ///
    /// So the window writes here FIRST, always, and the spine picks it up with
    /// [`crate::spine::Spine::poll_intake`] — the phone's own path, in the phone's own order,
    /// with the phone's own at-least-once guarantee into a ledger that can spot a replay.
    ///
    /// **Durable before the caller can say anything.** `fsync` inside, like every write on
    /// this log, because the whole point is that nothing downstream is claimed before his
    /// words survive a crash.
    pub fn submit_from_desk(
        &self,
        thread_id: &str,
        entity_id: Option<EntityId>,
        text: &str,
    ) -> Result<IntakeRecord, SteeringError> {
        let mut guard = self.inner.intake.lock().unwrap();
        let log = guard.as_mut().ok_or(SteeringError::NoDurableIntake)?;
        log.desk_message(thread_id, entity_id, text)
    }

    /// **Accept the CEO's typed message WITHOUT the spine, if and only if this conversation has
    /// an active priming claim** — the CEO's §55.
    ///
    /// Returns `Ok(Some(record))` when the message was taken and made durable here, and
    /// `Ok(None)` when it was not, which means exactly one thing: the caller must take the
    /// ordinary path into the spine. `None` is never a failure and never a loss.
    ///
    /// # Why the check and the write are one operation
    ///
    /// Both happen under the `priming` guard, and [`ConversationControl::end_front_desk_prime`] needs that
    /// same guard to clear the marker. So the two orderings are the only two there are, and
    /// both are safe:
    ///
    /// - this call wins the guard — the record is written while the priming thread waits to
    ///   clear the marker, and that thread's drain, which comes after the clear, finds it;
    /// - the priming thread wins — the marker is already `None`, this returns `Ok(None)`, and
    ///   the caller takes the ordinary short state lock.
    ///
    /// A plain "is it priming?" read followed by a separate write would have a third
    /// ordering — write after the drain — and that message would sit in the log until some
    /// later turn boundary happened to drain it. That is the bug this shape does not have.
    ///
    /// # What it refuses
    ///
    /// A control with no durable intake log ([`Self::detached`], every headless test) refuses
    /// with [`SteeringError::NoDurableIntake`] rather than accepting a message it cannot
    /// write down — the same refusal `steer` and `request_stop` make, for the same reason.
    /// The caller falls back to blocking, which is what it did before this existed.
    ///
    /// **THE SHELL NO LONGER CALLS THIS, and that is a widening rather than a retirement.**
    /// `send_message` takes [`Self::submit_from_desk`] for EVERY typed sentence now, because
    /// a prime was never the only thing his words could be stuck behind — an ordinary turn
    /// holds the same mutex for its whole length, and his phone shows the wait that the Mac's
    /// own window hides. This function is the narrower question ("is a prime in flight, and
    /// atomically so?") and it keeps its tests, because that question is the one thing
    /// `submit_from_desk` cannot answer and the day something needs it, it is here and proved.
    pub fn defer_send(
        &self,
        thread_id: &str,
        entity_id: Option<EntityId>,
        text: &str,
    ) -> Result<Option<IntakeRecord>, SteeringError> {
        let priming = self.inner.priming.lock().unwrap();
        if !priming.contains(thread_id) {
            return Ok(None);
        }
        let mut guard = self.inner.intake.lock().unwrap();
        let log = guard.as_mut().ok_or(SteeringError::NoDurableIntake)?;
        log.desk_message(thread_id, entity_id, text).map(Some)
    }

    // --- read by the spine, at the turn boundary ----------------------------------------

    /// The live stop claim, if it names this exact turn. Any other turn id returns `None`:
    /// a stop is a statement about ONE turn, and letting it fall through onto whatever ran
    /// next would attribute a stop the CEO never asked for.
    pub fn stop_claim_for(&self, turn_id: &str) -> Option<StopClaim> {
        self.inner.conversations.lock().unwrap().values().filter_map(|s|s.stop.as_ref())
            .find(|claim|claim.turn_id==turn_id).cloned()
    }

    /// Undrained intake records, oldest first.
    pub fn pending_intake(&self) -> Vec<IntakeRecord> {
        self.inner.intake.lock().unwrap().as_ref().map(|l| l.pending().to_vec()).unwrap_or_default()
    }

    /// **What could not be read out of the intake log at boot**, in a form a caller can act
    /// on and a surface can render.
    ///
    /// `None` for a [`detached`](Self::detached) control: there is no file, so there is no
    /// answer — which is a different statement from "nothing was skipped" and is never
    /// collapsed into it.
    ///
    /// The counts describe the LOAD, not the running session. They are taken at replay and
    /// do not move as this session appends, which is exactly the question being asked: what
    /// did the app fail to pick up from the last time it ran?
    pub fn intake_health(&self) -> Option<IntakeHealth> {
        self.inner.intake.lock().unwrap().as_ref().map(|l| l.health())
    }

    /// Every intake record on disk this build could not replay, with the reason for each.
    /// Empty for a detached control, which has no file to read.
    pub fn skipped_intake(&self) -> Vec<SkippedRecord> {
        self.inner.intake.lock().unwrap().as_ref().map(|l| l.skipped_records().to_vec()).unwrap_or_default()
    }

    /// Mark everything up to `through` handled. Called only after the corresponding ledger
    /// write has returned.
    pub fn mark_drained(&self, through: u64) -> Result<(), SteeringError> {
        let mut guard = self.inner.intake.lock().unwrap();
        match guard.as_mut() {
            Some(log) => log.mark_drained(through),
            None => Ok(()),
        }
    }
}


/// A control view bound to one conversation. It shares the app's intake and map.
#[derive(Clone)]
pub struct ConversationControl {
    shared:TurnControl,
    thread:String,
}
impl std::ops::Deref for ConversationControl {
    type Target=TurnControl;
    fn deref(&self)->&Self::Target {&self.shared}
}
impl std::fmt::Debug for ConversationControl {
    fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result {
        f.debug_struct("ConversationControl").field("thread",&self.thread).field("active",&self.active_turn()).finish_non_exhaustive()
    }
}
impl From<ConversationControl> for TurnControl {
    fn from(control:ConversationControl)->Self {control.shared}
}
impl ConversationControl {
    /// Claim only this conversation's preparation. Other conversations stay available.
    /// ```compile_fail,E0599
    /// let controls=richos_core::steering::TurnControl::detached();
    /// controls.end_front_desk_prime();
    /// ```
    pub fn begin_front_desk_prime(&self){self.shared.inner.priming.lock().unwrap().insert(self.thread.clone());}
    pub fn end_front_desk_prime(&self)->Option<String>{self.shared.inner.priming.lock().unwrap().take(&self.thread)}
    pub fn front_desk_priming(&self)->Option<String>{self.shared.inner.priming.lock().unwrap().get(&self.thread).cloned()}

    fn key(&self)->&str {&self.thread}

    pub fn begin_turn(&self, turn: ActiveTurn) {
        assert_eq!(turn.thread_id,self.thread,"turn control belongs to another conversation");
        let mut states=self.shared.inner.conversations.lock().unwrap();
        let state=states.entry(self.key().to_string()).or_default();
        if let Some(cancel)=state.cancel.as_ref() {cancel.begin_operation();}
        state.active=Some(turn);
    }

    pub fn set_model(&self,model:crate::config::ModelChoice)->Result<bool,String> {
        let handle=self.shared.inner.conversations.lock().unwrap().get(self.key()).and_then(|state|state.cancel.clone());
        match handle {Some(handle)=>handle.set_model(model),None=>Ok(false)}
    }

    pub fn shutdown_lease(&self) {
        let cancel=self.shared.inner.conversations.lock().unwrap().get(self.key()).and_then(|state|state.cancel.clone());
        if let Some(cancel)=cancel {cancel.shutdown();}
    }

    pub fn set_cancel(&self, cancel: Option<Arc<dyn TurnCancel>>) {
        let mut states=self.shared.inner.conversations.lock().unwrap();
        let state=states.entry(self.key().to_string()).or_default();
        if let Some(turn)=state.active.as_ref() {
            if let Some(handle)=cancel.as_ref() {
                handle.begin_operation();
                if state.stop.as_ref().is_some_and(|claim|claim.turn_id==turn.turn_id) {handle.cancel();}
            }
        }
        state.cancel=cancel;
    }

    pub fn set_lease_session(&self, session_id: Option<String>) {
        self.shared.inner.conversations.lock().unwrap().entry(self.key().to_string()).or_default().lease_session=session_id;
    }

    pub fn active_turn(&self)->Option<ActiveTurn> {
        self.shared.inner.conversations.lock().unwrap().get(self.key()).and_then(|state|state.active.clone())
    }

    pub fn lease_session(&self)->Option<String> {
        self.shared.inner.conversations.lock().unwrap().get(self.key()).and_then(|state|state.lease_session.clone())
    }

    pub fn worker_status(&self)->crate::worker_status::WorkerStatusView {
        let (session, handle)=self.shared.inner.conversations.lock().unwrap().get(self.key())
            .map(|state|(state.lease_session.clone(),state.cancel.clone())).unwrap_or_default();
        handle.and_then(|h|h.worker_status()).unwrap_or_else(||
            crate::worker_status::WorkerStatusView::unattributed(if session.is_some() {
                crate::worker_status::Unattributed::AppEvidenceUnavailable
            } else {crate::worker_status::Unattributed::NoSession}))
    }

    pub fn request_stop(&self) -> Result<StopOutcome, SteeringError> {
        self.request_stop_matching(None)
    }

    pub fn request_stop_for(&self, expected_turn_id: &str) -> Result<StopOutcome, SteeringError> {
        self.request_stop_matching(Some(expected_turn_id))
    }

    fn request_stop_matching(&self,expected:Option<&str>)->Result<StopOutcome,SteeringError> {
        let mut states=self.shared.inner.conversations.lock().unwrap();
        let Some(state)=states.get_mut(self.key()) else {return Ok(StopOutcome::NothingRunning)};
        let Some(active)=state.active.as_ref() else {return Ok(StopOutcome::NothingRunning)};
        if expected.is_some_and(|id|id!=active.turn_id) {return Ok(StopOutcome::NothingRunning)}
        let (turn_id,requested_at)=self.shared.record_stop(state)?;
        let reached_lease=state.cancel.as_ref().is_some_and(|cancel|cancel.cancel());
        Ok(StopOutcome::Requested{turn_id,requested_at,reached_lease})
    }

    /// §9.2: the CEO added words while Rich was working. Durable on return; delivered at
    /// the next turn boundary (see the module doc — this does not join the running turn).
    pub fn steer(&self, text: &str) -> Result<IntakeRecord, SteeringError> {
        let active = self.active_turn().ok_or(SteeringError::NoActiveTurn)?;
        let mut guard = self.inner.intake.lock().unwrap();
        let log = guard.as_mut().ok_or(SteeringError::NoDurableIntake)?;
        log.steer(&active.thread_id, &active.turn_id, active.entity_id.clone(), text)
    }


    pub fn stop_claim(&self)->Option<StopClaim> {
        self.shared.inner.conversations.lock().unwrap().get(self.key()).and_then(|state|state.stop.clone())
    }

    pub(crate) fn restore_stop_claim(&self, claim: StopClaim) {
        self.shared.inner.conversations.lock().unwrap().entry(self.key().to_string())
            .or_default().stop = Some(claim);
    }

    pub fn clear_stop_claim(&self) {
        if let Some(state)=self.shared.inner.conversations.lock().unwrap().get_mut(self.key()) {
            if let Some(claim)=state.stop.take() {state.settled=Some(claim);}
        }
    }

    /// A Stop read back from disk at startup: its cut covers what was accepted before it,
    /// exactly as when it settled in this process. The latest cut wins.
    pub(crate) fn restore_settled(&self, claim: StopClaim) {
        let mut states=self.shared.inner.conversations.lock().unwrap();
        let state=states.entry(self.key().to_string()).or_default();
        if state.settled.as_ref().is_none_or(|settled|settled.intake_id<claim.intake_id) {state.settled=Some(claim);}
    }

    /// The cleared Stop whose cut covers an input accepted at `order`, if any.
    pub fn settled_stop_covering(&self, order: u64) -> Option<StopClaim> {
        self.shared.inner.conversations.lock().unwrap().get(self.key())
            .and_then(|state|state.settled.clone()).filter(|claim|order<claim.intake_id)
    }
}

impl Default for TurnControl {
    fn default() -> Self {
        TurnControl::detached()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("richos-intake-{tag}-{}-{}.jsonl", std::process::id(), now_millis()))
    }

    struct RecordingCancel(Arc<Mutex<usize>>, bool);
    impl TurnCancel for RecordingCancel {
        fn cancel(&self) -> bool {
            *self.0.lock().unwrap() += 1;
            self.1
        }
    }

    fn active() -> ActiveTurn {
        ActiveTurn {
            turn_id: "turn_1".into(),
            thread_id: "thr_1".into(),
            entity_id: None,
            started_at: Some(1_000),
        }
    }

    #[test]
    fn simultaneous_conversations_keep_their_own_stops_and_cancellation_handles() {
        let path=tmp("parallel-stops");
        let control=TurnControl::open(&path).unwrap();
        let a=control.for_thread("A");
        let b=control.for_thread("B");
        let a_calls=Arc::new(Mutex::new(0));
        let b_calls=Arc::new(Mutex::new(0));
        a.set_cancel(Some(Arc::new(RecordingCancel(a_calls.clone(),true))));
        b.set_cancel(Some(Arc::new(RecordingCancel(b_calls.clone(),true))));
        a.begin_turn(ActiveTurn{turn_id:"a".into(),thread_id:"A".into(),..active()});
        b.begin_turn(ActiveTurn{turn_id:"b".into(),thread_id:"B".into(),..active()});
        assert_eq!(a.request_stop_for("b").unwrap(),StopOutcome::NothingRunning);
        assert!(control.pending_intake().is_empty());
        a.request_stop().unwrap();
        b.request_stop().unwrap();
        assert_eq!(*a_calls.lock().unwrap(),1);
        assert_eq!(*b_calls.lock().unwrap(),1);
        assert_eq!(a.stop_claim().unwrap().turn_id,"a");
        assert_eq!(b.stop_claim().unwrap().turn_id,"b");
        b.end_turn("b");
        b.clear_stop_claim();
        assert_eq!(a.active_turn().unwrap().turn_id,"a");
        assert_eq!(a.stop_claim().unwrap().turn_id,"a");
        assert!(b.active_turn().is_none());
        assert!(b.stop_claim().is_none());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn all_conversation_stops_are_durable_before_any_cancel_or_shutdown() {
        struct ChecksStops {path:PathBuf,calls:Arc<Mutex<usize>>}
        impl ChecksStops {
            fn check(&self) {
                let log=IntakeLog::open(&self.path).unwrap();
                let mut turns:Vec<_>=log.pending().iter().filter_map(|record|match record {
                    IntakeRecord::Stop{turn_id,..}=>Some(turn_id.as_str()),_=>None,
                }).collect();
                turns.sort_unstable();
                assert_eq!(turns,vec!["one","three","two"],"a lease ended before every Stop was durable");
                *self.calls.lock().unwrap()+=1;
            }
        }
        impl TurnCancel for ChecksStops {
            fn cancel(&self)->bool {self.check();true}
            fn shutdown(&self) {self.check();}
        }
        let path=tmp("all-stops");
        let control=TurnControl::open(&path).unwrap();
        let calls=Arc::new(Mutex::new(0));
        for thread in ["one","two","three"] {
            let conversation=control.for_thread(thread);
            conversation.set_cancel(Some(Arc::new(ChecksStops{path:path.clone(),calls:calls.clone()})));
            conversation.begin_turn(ActiveTurn{turn_id:thread.into(),thread_id:thread.into(),..active()});
        }
        assert_eq!(control.active_turn_count(),3);
        assert_eq!(control.request_stop_all().unwrap().len(),3);
        control.shutdown_leases();
        assert_eq!(*calls.lock().unwrap(),6);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn a_stop_request_is_on_disk_before_the_lease_is_ever_touched() {
        // §9.3's step ORDER, proven by making the cancel seam read the file it must not be
        // able to beat: if the write happened after the cancel, this read finds nothing.
        struct ReadsTheLog(PathBuf, Arc<Mutex<bool>>);
        impl TurnCancel for ReadsTheLog {
            fn cancel(&self) -> bool {
                let text = std::fs::read_to_string(&self.0).unwrap_or_default();
                *self.1.lock().unwrap() = text.contains("\"record\":\"stop\"");
                true
            }
        }
        let path = tmp("order");
        let ctl = TurnControl::open(&path).unwrap().for_thread("thr_1");
        ctl.begin_turn(active());
        let saw = Arc::new(Mutex::new(false));
        ctl.set_cancel(Some(Arc::new(ReadsTheLog(path.clone(), saw.clone()))));

        ctl.request_stop().unwrap();
        assert!(*saw.lock().unwrap(), "the stop request must be durable BEFORE the lease is cancelled");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn stopping_nothing_is_an_answer_not_an_error() {
        let path = tmp("nothing");
        let ctl = TurnControl::open(&path).unwrap().for_thread("thr_1");
        let calls = Arc::new(Mutex::new(0));
        ctl.set_cancel(Some(Arc::new(RecordingCancel(calls.clone(), true))));
        assert_eq!(ctl.request_stop().unwrap(), StopOutcome::NothingRunning);
        assert_eq!(*calls.lock().unwrap(), 0, "nothing was running, so nothing was cancelled");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_delayed_targeted_stop_never_writes_or_cancels_a_successor_turn() {
        let path = tmp("targeted-successor");
        let ctl = TurnControl::open(&path).unwrap().for_thread("thr_1");
        let calls = Arc::new(Mutex::new(0));
        ctl.set_cancel(Some(Arc::new(RecordingCancel(calls.clone(), true))));
        assert_eq!(ctl.request_stop_for("turn_1").unwrap(), StopOutcome::NothingRunning);
        ctl.begin_turn(active());
        assert_eq!(ctl.request_stop_for("other").unwrap(), StopOutcome::NothingRunning);
        assert!(ctl.pending_intake().is_empty());
        assert!(ctl.stop_claim().is_none());
        assert_eq!(*calls.lock().unwrap(), 0);

        assert!(matches!(ctl.request_stop_for("turn_1").unwrap(),
            StopOutcome::Requested { turn_id, reached_lease: true, .. } if turn_id == "turn_1"));
        ctl.end_turn("turn_1");
        ctl.begin_turn(ActiveTurn { turn_id: "turn_2".into(), ..active() });
        let before = std::fs::read(&path).unwrap();
        assert_eq!(ctl.request_stop_for("turn_1").unwrap(), StopOutcome::NothingRunning);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(ctl.stop_claim().unwrap().turn_id, "turn_1");
        assert_eq!(*calls.lock().unwrap(), 1);
        assert_eq!(ctl.active_turn().unwrap().turn_id, "turn_2");

        // Replacing the successor's handle must not forward the predecessor's claim.
        let successor_calls = Arc::new(Mutex::new(0));
        ctl.set_cancel(Some(Arc::new(RecordingCancel(successor_calls.clone(), true))));
        assert_eq!(*successor_calls.lock().unwrap(), 0);
        assert!(matches!(ctl.request_stop_for("turn_2").unwrap(),
            StopOutcome::Requested { turn_id, reached_lease: true, .. } if turn_id == "turn_2"));
        assert_eq!(*successor_calls.lock().unwrap(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn targeted_stop_holds_the_turn_identity_until_cancellation_returns() {
        use std::sync::mpsc;
        use std::time::Duration;

        struct BlockingCancel {
            entered: mpsc::Sender<()>,
            release: Mutex<mpsc::Receiver<()>>,
        }
        impl TurnCancel for BlockingCancel {
            fn cancel(&self) -> bool {
                self.entered.send(()).unwrap();
                self.release.lock().unwrap().recv_timeout(Duration::from_secs(5)).unwrap();
                true
            }
        }
        let path = tmp("targeted-atomic");
        let ctl = TurnControl::open(&path).unwrap().for_thread("thr_1");
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        ctl.set_cancel(Some(Arc::new(BlockingCancel {
            entered: entered_tx, release: Mutex::new(release_rx),
        })));
        ctl.begin_turn(active());
        let stopping = ctl.clone();
        let stop = std::thread::spawn(move || stopping.request_stop_for("turn_1").unwrap());
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        // This is an exact lock assertion, not a scheduling-dependent sleep: the
        // successor cannot begin or replace the handle while cancel is in progress.
        assert!(matches!(ctl.inner.conversations.try_lock(), Err(std::sync::TryLockError::WouldBlock)));
        assert!(std::fs::read_to_string(&path).unwrap().contains("\"record\":\"stop\""));
        let succeeding = ctl.clone();
        let transition = std::thread::spawn(move || {
            succeeding.end_turn("turn_1");
            succeeding.begin_turn(ActiveTurn { turn_id: "turn_2".into(), ..active() });
        });
        release_tx.send(()).unwrap();
        assert!(matches!(stop.join().unwrap(),
            StopOutcome::Requested { turn_id, reached_lease: true, .. } if turn_id == "turn_1"));
        transition.join().unwrap();
        assert_eq!(ctl.active_turn().unwrap().turn_id, "turn_2");
        assert!(ctl.stop_claim_for("turn_2").is_none());
        assert_eq!(ctl.request_stop_for("turn_1").unwrap(), StopOutcome::NothingRunning);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_cancel_that_reached_nothing_is_reported_as_reaching_nothing() {
        let path = tmp("unreached");
        let ctl = TurnControl::open(&path).unwrap().for_thread("thr_1");
        ctl.begin_turn(active());
        // No cancel seam installed at all — the honest degrade.
        match ctl.request_stop().unwrap() {
            StopOutcome::Requested { reached_lease, .. } => assert!(!reached_lease),
            other => panic!("expected Requested, got {other:?}"),
        }
        // Start an independent request for the seam-present case. Installing a
        // handle during the old stopped request now forwards that durable Stop.
        ctl.end_turn(&active().turn_id);
        ctl.clear_stop_claim();
        // And with a seam that says it delivered nothing.
        let calls = Arc::new(Mutex::new(0));
        ctl.set_cancel(Some(Arc::new(RecordingCancel(calls.clone(), false))));
        ctl.begin_turn(active());
        match ctl.request_stop().unwrap() {
            StopOutcome::Requested { reached_lease, .. } => assert!(!reached_lease),
            other => panic!("expected Requested, got {other:?}"),
        }
        assert_eq!(*calls.lock().unwrap(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn without_a_durable_intake_a_stop_is_refused_rather_than_faked() {
        let ctl = TurnControl::detached().for_thread("thr_1");
        ctl.begin_turn(active());
        assert!(matches!(ctl.request_stop(), Err(SteeringError::NoDurableIntake)));
        assert!(matches!(ctl.steer("go on then"), Err(SteeringError::NoDurableIntake)));
    }

    #[test]
    fn steering_with_nothing_running_is_refused_so_it_never_becomes_a_second_send_path() {
        let path = tmp("nosteer");
        let ctl = TurnControl::open(&path).unwrap().for_thread("thr_1");
        assert!(matches!(ctl.steer("hello"), Err(SteeringError::NoActiveTurn)));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn undrained_records_survive_a_reopen_and_drained_ones_do_not() {
        let path = tmp("replay");
        {
            let ctl = TurnControl::open(&path).unwrap().for_thread("thr_1");
            ctl.begin_turn(active());
            let a = ctl.steer("first").unwrap();
            ctl.steer("second").unwrap();
            ctl.mark_drained(a.id()).unwrap();
        }
        let reopened = TurnControl::open(&path).unwrap().for_thread("thr_1");
        let pending = reopened.pending_intake();
        assert_eq!(pending.len(), 1);
        match &pending[0] {
            IntakeRecord::Steer { text, .. } => assert_eq!(text, "second"),
            other => panic!("expected the undrained steer, got {other:?}"),
        }
        // And ids keep climbing past the drained ones, so a replayed record can never
        // collide with a fresh one.
        reopened.begin_turn(active());
        assert!(reopened.steer("third").unwrap().id() > pending[0].id());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_torn_final_line_is_skipped_and_the_rest_of_the_log_still_replays() {
        let path = tmp("torn");
        {
            let ctl = TurnControl::open(&path).unwrap().for_thread("thr_1");
            ctl.begin_turn(active());
            ctl.steer("intact").unwrap();
        }
        // Simulate the power going out mid-write.
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"{\"record\":\"steer\",\"id\":9,\"thre").unwrap();
        drop(f);

        let reopened = TurnControl::open(&path).unwrap().for_thread("thr_1");
        assert_eq!(reopened.pending_intake().len(), 1);
        reopened.begin_turn(active());
        reopened.steer("accepted after recovery").unwrap();
        drop(reopened);
        let recovered = TurnControl::open(&path).unwrap();
        assert_eq!(recovered.pending_intake().len(), 2);
        assert!(std::fs::read_to_string(&path).unwrap().contains("\"thre\n"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_stop_claim_belongs_to_one_turn_and_never_falls_through_onto_the_next() {
        let path = tmp("claim");
        let ctl = TurnControl::open(&path).unwrap().for_thread("thr_1");
        ctl.begin_turn(active());
        ctl.request_stop().unwrap();
        assert!(ctl.stop_claim_for("turn_1").is_some());
        assert!(ctl.stop_claim_for("turn_2").is_none(), "a stop is a statement about ONE turn");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_full_drain_compacts_the_file_without_ever_emptying_it() {
        let path = tmp("compact");
        let ctl = TurnControl::open(&path).unwrap().for_thread("thr_1");
        ctl.begin_turn(active());
        // A drained Stop is kept by the compaction: its cut still covers a card answer saved
        // before it (review rv-20261018T204635Z-c68a30ec-f61e#2).
        ctl.request_stop().unwrap();
        let stop = ctl.stop_claim().unwrap().intake_id;
        ctl.mark_drained(stop).unwrap();
        // Two lines per action (the steer, then its drain marker), so this crosses
        // COMPACT_AFTER_LINES = 256 with room to spare.
        for i in 0..200 {
            let r = ctl.steer(&format!("msg {i}")).unwrap();
            ctl.mark_drained(r.id()).unwrap();
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        assert!(lines.len() < 256, "a fully drained log compacts; got {} lines", lines.len());
        assert!(!lines.is_empty(), "compaction must never leave the file empty");

        // And it still replays to the same place.
        let reopened = TurnControl::open(&path).unwrap();
        assert!(reopened.stop_records().iter().any(|r| r.id() == stop), "the compaction lost the Stop's cut");
        let reopened = reopened.for_thread("thr_1");
        assert!(reopened.pending_intake().is_empty());
        let _ = std::fs::remove_file(&path);
    }
}

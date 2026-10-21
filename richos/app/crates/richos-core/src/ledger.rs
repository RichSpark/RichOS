//! Durable conversation input, observed output and user controls.
//!
//! This append-only log preserves receipts before native delivery and supplies
//! the conversation UI. It does not infer job outcomes or reconstruct session context.
//!
//! Three invariants this module exists to guarantee:
//!   1. CRASH-SAFETY (persist-before-send): a CEO prompt is journaled `received`
//!      and flushed to disk BEFORE it is ever handed to a compute session. A crash
//!      the instant after the CEO hits send can never eat the message.
//!   2. ANTI-FALSE-ATTRIBUTION: the ACTION ledger — recorded as actions happen,
//!      alongside native transcripts, preserving the operation and its source evidence.
//!   3. ENTITY SCOPE (ECS §3.2–3.4, UX brief §22/§25 Integrity): every thread has
//!      exactly one home entity, immutable after creation; every turn carries that
//!      entity; and **no event from one entity may ever render in another entity's
//!      thread**. Enforced at BOTH ends — a write cannot present a binding the ledger
//!      did not issue, and a read re-checks every stored turn against its thread's home
//!      so a corrupt, forged or badly-migrated log cannot leak across the boundary.
//!
//! ## Every other reader of a customer's file, and what it does with a record it cannot read
//!
//! Surveyed 2026-09-05, when [`Ledger::replay`] stopped aborting on one unreadable record.
//! The point of writing it down is that "the ledger is fixed" is not the same claim as
//! "the app is fixed", and the next person to hit this class of bug should not have to
//! re-derive the map.
//!
//! **The `?` was unique to this file.** `ledger.rs`'s per-record parse was the ONLY place
//! in `app/` where a single unreadable line in a customer's file aborted the whole read.
//! Nothing in `src-tauri/` and nothing in `richos-voice` has the shape at all.
//!
//! The JUDGMENT this module reached about that record — from a newer build, damaged, or
//! undecidable — now lives in [`crate::skip`] and has a second caller, `steering.rs`. It
//! was extracted rather than copied so the two stores cannot drift apart about what damage
//! looks like; the reasoning is in that module's doc.
//!
//! **Every sibling is already tolerant — and every one of them is SILENT about it.** Each
//! of these drops an unparseable record with `else { continue }` or `.ok()`, says nothing,
//! and returns a shorter list that is indistinguishable from a genuinely shorter file.
//!
//! Re-surveyed 2026-09-05 with the two questions that decide whether silence is a defect:
//! **what is actually lost**, and **would anyone ever find out**. Silence is CORRECT where
//! the record is derived from something durable that is still on disk — losing it costs a
//! re-read, not a fact. It is a DEFECT where the record is the only copy of something a
//! person did.
//!
//! | reader | file it reads | what one dropped record costs | would anyone notice |
//! |---|---|---|---|
//! | `steering.rs:304` | the intake log — **the CEO's own typed words, before they become turns** | a steering message or a stop that never becomes a turn: he typed it, watched it accepted, and it never reached Rich | **no — and it was the one place that mattered. FIXED here: classified, counted, reported, never fatal** |
//! | `correction.rs` `replay` | the loro correction desk | one proposal or one answered decision; a confirmed write can silently revert to pending, or a pending one vanish before he ever sees it | no. Worth the second look now that the intake log is done |
//! | `staging.rs:300` | the spoken-correction staging desk | one candidate question he was going to be asked | no — but §7 forbids a write without a human answer, so a lost candidate under-asks rather than acting alone. Fails safe |
//! | `journal.rs` `read_day_shard` | the per-thread machinery journal | one technical-mode row. `ThreadMachinery::Unreadable` covers a dead SHARD, not a dead LINE — and since 2026-09-17 it covers a shard that dies MID-read too, not only one that fails to open | not really — technical view only, and the ledger still holds the conversation |
//! | `journal.rs:668` | the raw-retention journal | one payload; the record above it still renders, with `payload: None` | no, and the shape degrades honestly |
//! | `heard.rs:715` | the dictation log | one dictation-review entry. The turn it produced is already in the ledger | no. This is a review aid over durable data |
//! | `feedback.rs:270` | the feedback log | one past feedback entry in his history list. Nothing sends | no, and nothing downstream depends on it |
//! | `worker_events.rs:267`, `worker_status.rs:286` | the engine's event logs (not the customer's files) | one row of a read-only inspector over a log this app does not own | no, and it must stay tolerant — a foreign format is expected to grow |
//! | `config.rs:488` | `config.json` (whole file) | every preference at once. Already handles the one field that DELETES by keeping `raw_retention: FOREVER` on a corrupt file | he would see his preferences reset. Loud by accident, correct by design |
//! | `src-tauri/src/nav.rs:108` | `navigation.json` (whole file) | sidebar and inspector widths | he would see the panels move. Pure view state |
//!
//! **A second, quieter defect was recorded here on 2026-09-05, and it has since been
//! CLOSED — but not in the shape it was written down.** The entry read: "`lines()
//! .map_while(Result::ok)` ends the ITERATOR at the first non-UTF-8 line, so ONE bad byte
//! discards every record after it rather than its own", still present in
//! `journal.rs:407/469/667`, `staging.rs:293` and `correction.rs:459`.
//!
//! Re-derived on 2026-09-17, site by site, because spec point 20 was about to be built on it:
//!
//!   * `correction.rs` was FIXED on 2026-09-05, the same day — it is `read_until` with
//!     `skip::classify_line`. The line number in the entry above was stale within hours.
//!   * `staging.rs` never had it. That loop is `split(b'\n')` with `let line = line?;` — an
//!     IO error propagates and there is no UTF-8 error to end anything.
//!   * `journal.rs` never had it EITHER, and this is the one worth understanding.
//!     `BufRead::split` yields `io::Result<Vec<u8>>`, so its `Err` arm is an IO error on the
//!     reader — a `Vec<u8>` has no encoding to be wrong about. Bad bytes always reached
//!     `serde_json::from_slice` and always cost one record. **The defect is specific to
//!     `lines()`**, which is what `steering.rs`, `correction.rs` and this module had.
//!
//! **What `map_while(Result::ok)` did cost at the `split` sites is different and louder:** a
//! genuine IO error part way down a shard ended the read and returned a SHORT LIST. Measured,
//! not reasoned: an `EISDIR` on one day shard made `read_thread_checked` answer
//! `ThreadMachinery::NothingRecorded` — "nothing was ever recorded for this thread", said
//! about a file sitting right there. That is point 19's other prohibition, treating what
//! cannot be read as absent. `journal.rs` now reports it; both properties are pinned by
//! `one_bad_line_in_the_middle_costs_its_own_record_and_no_other` and
//! `an_io_error_part_way_through_a_shard_is_reported_and_never_returned_as_a_short_list`.
//!
//! The general lesson, since this entry is the thing that got quoted into a spec and then
//! into a brief: a survey row is a claim with a date on it, and `lines()` and `split()` are
//! not the same iterator.
//!
//! ## The two roots this survey did not cover, and what they turn out to be (point 24)
//!
//! Spec point 24 names `~/Library/Application Support/RichOS/ecs` (`main.rs:267`) and the
//! app's `engine-state` directory (`updates.rs:662`) as "written ABOUT engine runs and
//! subject to 18-20 like anything else". Both were checked against this build on 2026-09-17.
//! The conclusion holds for one of them and is true of the other for a stronger reason than
//! the point gives.
//!
//! | root | who WRITES it | who READS it here | verdict against 18-20 |
//! |---|---|---|---|
//! | `RichOS/ecs` | the ENGINE's Python, via `bin/ecs --state-root` (`ecs.rs::EcsBridge::request`) | **nobody in this app** | out of scope by construction. This crate never opens a path inside it: it hands the directory to a subprocess and reads that subprocess's stdout. Point 18 needs a writer and there is none here; points 19-20 need a reader and there is none either. Its shape is versioned by the engine's own `ecs/migrations/`. |
//! **Point 18 is therefore not applied to the engine-owned ECS root**, and the rule is the same one that
//! selected the three stores that ARE stamped: this build stamps what this build writes. See
//! `docs/architecture/data-migration.md` for the stores still to be stamped and why they are
//! not urgent.
//!
//! One near-miss worth recording, since it looks like a point-21 hazard and is not:
//! `ecs::ToolScope` and `onboarding_tools::OnboardingToolScope` are whole-file documents this
//! app DOES write, and both carry `#[serde(deny_unknown_fields)]` — which in a durable
//! document is the point-21 failure in mirror image, a newer build's extra field costing an
//! older build the whole file. They are safe because they are not durable: each is created
//! fresh under a fresh UUID at lease start (`native.rs:1783-1785`, `:1800-1802`) inside a
//! `scopes/` working directory, and a file left behind by a dead process is garbage rather
//! than history. Recorded here so the next person does not have to re-derive it — and so that
//! moving either of them anywhere durable is visibly a decision.
//!
//! Two readers do it properly, and they are the precedent this module's
//! [`Ledger::history_health`] follows: `entity.rs:681` fails the whole registry CLOSED
//! and reports why through `RegistryLoad::notes`, and `launch.rs:389` keeps the file
//! untouched and carries an `unreadable_reason`. `launch.rs` also carries the
//! `schema_version` that would settle [`SkipKind::Ambiguous`] here if a ledger record
//! carried one.
//!
//! ## Threads written before entity binding existed
//!
//! They replay as [`ThreadEntity::Unbound`] and FAIL CLOSED on every scoped read and
//! write. They are deliberately NOT migrated by a heuristic: nothing durable in this log
//! records the repository root a thread was created under, `config.rs` holds one free-text
//! `company_name` (default `"My Company"`) rather than an entity id, and §22's
//! "Must not be faked" list names cross-entity context explicitly — a wrong binding is a
//! privacy-boundary violation, not a cosmetic bug. The only exit is
//! [`Ledger::adopt_unbound_thread`], an explicit, once-only, durably-recorded operator
//! decision. See that method's doc for exactly what an operator sees.

use crate::entity::{EntityId, PersonId, ThreadBinding, ThreadEntity};
use crate::util::{new_id, now_millis};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("ledger io: {0}")]
    Io(#[from] std::io::Error),
    #[error("ledger encode: {0}")]
    Encode(#[from] serde_json::Error),
    #[error("unknown thread: {0}")]
    UnknownThread(String),
    #[error("unknown turn: {0}")]
    UnknownTurn(String),
    /// A thread that predates entity binding. ECS §3.3: `entity_id` is REQUIRED before
    /// retrieval or mutation, so this is refused rather than served or guessed.
    #[error(
        "thread {0} has no entity binding: it predates entity scoping, and Rich will not guess \
         which entity this work belongs to. An operator must bind it explicitly."
    )]
    UnboundThread(String),
    /// A presented binding contradicts the thread's immutable home entity.
    #[error("scope mismatch on thread {thread_id}: home entity is {home}, binding presented {presented}")]
    ScopeMismatch { thread_id: String, home: String, presented: String },
    /// ECS §3.4's fencing token: an old context tried to write into a newer one.
    #[error("stale binding on thread {thread_id}: presented revision {presented}, current {current}")]
    StaleBinding { thread_id: String, presented: u64, current: u64 },
    /// The one-way, once-only adoption of a legacy unbound thread was attempted twice.
    #[error("thread {thread_id} is already bound to {entity_id} — a thread's entity home is immutable")]
    ThreadAlreadyBound { thread_id: String, entity_id: String },
    /// A reserved thread id was presented to [`Ledger::create_thread_with_reserved_id`] and a
    /// record already carries it. **Never expected and never recovered from by picking another
    /// id**: the ids are uuid v4, so a collision here means the reservation was used twice,
    /// which would put two conversations in one partition.
    #[error("thread id {0} is already in this ledger — a reserved id may be spent exactly once")]
    ThreadIdAlreadySpent(String),
}

/// Lifecycle state of a single conversational turn (§5.1 of the continuity design).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnState {
    /// Journaled + flushed to disk, not yet delivered to a session. The crash-safe floor.
    Received,
    /// Handed to a compute session; a reply is streaming.
    InFlight,
    /// Terminal, ended cleanly (carries the turn's stop reason).
    Completed,
    /// Terminal, ended by crash/cancel/rotation before turn-end.
    ///
    /// **This variant no longer covers a CEO stop.** It did until 2026-08-29, and that is
    /// exactly why the §6.1 label *"You stopped after {duration}"* could not be rendered:
    /// attributing a compute-lease crash to the CEO is a false statement about who did what.
    /// [`TurnState::Stopped`] carries that one case now, and it is written only from a
    /// durably-recorded stop REQUEST (`steering.rs`), never inferred.
    Interrupted,
    /// Terminal, ended because the CEO asked it to stop (UX §9.3).
    ///
    /// The distinction from [`Interrupted`](Self::Interrupted) is the whole point: this
    /// state is the evidence behind §6.1's `You stopped after {duration}`, which is an
    /// ATTRIBUTION to the CEO. It is reachable only through [`Ledger::stop_turn`], which
    /// the spine calls only when a stop request for that exact turn id is on disk.
    Stopped,
}

/// How the CEO's input arrived. Voice and text land in ONE thread (fixes the
/// ephemeral-huddle hazard: nothing said by voice is lost).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// A message from another agent. Its text is context, never user authorization.
    Agent,
    Text,
    Jam,
    /// A system-authored internal prompt (re-prime / handoff-summary request) — NEVER
    /// rendered to the CEO.
    Internal,
    /// Rich speaking unprompted (continuity design §9 / UX doc §5 "proactive messages").
    /// Carries NO user_text — there was no CEO prompt. Render eligibility is gated by
    /// `Turn::tier` (Tier 3 / Silent never renders — UX §5.1).
    Proactive,
}

/// The three-tier assertiveness posture a proactive message is raised at
/// (UX doc §5.1 / §5.2). Default posture is Quiet (config.rs), independent of a given
/// message's tier — the dial controls how readily Rich reaches for tier 1, not what a
/// tier means once chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionTier {
    /// Rare: real decisions / genuinely time-sensitive. OS notification + slim accent edge.
    InterruptNow,
    /// Most things: a single batched Rich message, delivered at a natural moment.
    Digest,
    /// FYI only. Never appears in the conversation, never notifies (UX §5.1 Tier 3) —
    /// lives in the ledger for a CEO who goes looking (no dedicated activity-view UI yet;
    /// deliberately deferred past v1 per the UX doc §7).
    Silent,
}

impl AttentionTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            AttentionTier::InterruptNow => "interrupt_now",
            AttentionTier::Digest => "digest",
            AttentionTier::Silent => "silent",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "interrupt_now" | "interrupt-now" => Some(AttentionTier::InterruptNow),
            "digest" => Some(AttentionTier::Digest),
            "silent" => Some(AttentionTier::Silent),
            _ => None,
        }
    }
}

/// Status of an action in the action ledger (the double-execution guard, §5.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionStatus {
    /// Claimed BEFORE execution (claim-then-execute inheritance, §6.4).
    Claimed,
    Completed,
    Failed,
}

/// Who an action is FOR. Both classes are equally DURABLE — this is a rendering
/// property, never a knowledge property (precisely the conflation the governance
/// design named: "clean output was implemented as *drop* rather than *route*").
///
/// The distinction is required because the re-prime injects the action digest into a
/// live session that is under a standing order to never reveal or reference session
/// rotation (§6.2, `reprime.rs::identity_assertion`). Feeding "I rotated my session"
/// into a section headed "ground truth for what Rich has done — authoritative" would
/// manufacture exactly the machinery leak that order forbids. So:
///
/// - `CeoFacing` — something Rich did in the CEO's world. Injected into the re-prime's
///   ACTION LEDGER section as authoritative ground truth.
/// - `Internal` — app machinery (lease rotation, re-prime injection, crash recovery).
///   Durably recorded for the audit trail and for app-side idempotency, and
///   deliberately NOT injected into any priming prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ActionVisibility {
    /// Rich acting in the CEO's world. Default, so any on-disk record written before
    /// this field existed replays with its original meaning.
    #[default]
    CeoFacing,
    /// App machinery. Durable, auditable, never injected into a priming prompt.
    Internal,
}

/// One appended, immutable fact. The log is the source of record; the in-memory
/// projection below is a disposable fold over it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event")]
pub enum Event {
    /// A thread and its IMMUTABLE entity home (ECS §3.2). The three scope fields are
    /// `#[serde(default)]` so a record written before entity binding existed still
    /// replays — as `entity_id: None`, i.e. [`ThreadEntity::Unbound`], which fails closed
    /// rather than inheriting anybody's guess.
    ThreadCreated {
        thread_id: String,
        title: String,
        at: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        entity_id: Option<EntityId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        person_id: Option<PersonId>,
        #[serde(default)]
        binding_revision: u64,
    },
    /// The ONE-WAY, once-only adoption of a legacy unbound thread by an explicit operator
    /// decision (see [`Ledger::adopt_unbound_thread`]). It can only move a thread from
    /// `Unbound` to `Bound`; applying it to an already-bound thread is refused, so it can
    /// never become a rebinding path.
    ThreadEntityBound {
        thread_id: String,
        entity_id: EntityId,
        person_id: PersonId,
        binding_revision: u64,
        /// Who made the call. Recorded because this is the one binding in the system that
        /// did not come from thread creation, so "who decided" is part of the evidence.
        adopted_by: String,
        at: u64,
    },
    /// The crash-safety event: written + fsync'd BEFORE the prompt is sent. Carries the
    /// scope it was accepted under (ECS §3.4) — turns are bound to entities, not merely
    /// to threads, so a turn stamped with the wrong entity is detectable on replay.
    PromptReceived {
        turn_id: String,
        thread_id: String,
        text: String,
        source: Source,
        at: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        entity_id: Option<EntityId>,
        #[serde(default)]
        binding_revision: u64,
        /// The `steering::IntakeLog` record this turn was drained from, when it came from
        /// one (UX §9.2). THE DE-DUPLICATION KEY, and the reason it is on the event rather
        /// than kept in memory: the intake drain is at-least-once by construction — the
        /// ledger write comes first and the drain marker second, because a crash between
        /// them must re-present the CEO's words rather than lose them. Re-presenting them
        /// would create a SECOND turn with the same text, and this is what lets the drain
        /// recognize its own earlier work and skip it.
        ///
        /// Direct desktop inputs reserve an ID from the same intake counter and save it
        /// in this existing ledger write, without an extra intake row or fsync. Stop uses
        /// this common order for typed, spoken and pressed inputs. Account continuations
        /// retain their original ID. `None` covers older records and detached controls.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        intake_id: Option<u64>,
        /// **WAS RICH'S OWN VOICE AUDIBLE IN THE ROOM WHILE THIS WAS BEING RECORDED?**
        ///
        /// The gap this closes, named by the voice engineer's own handoff (richos `c712ccd5`):
        /// a `PromptReceived` with `source: Source::Jam` carried no provenance at all, so an
        /// echo-born spoken turn and a genuine one were **indistinguishable after the fact**.
        /// Candidate .5 put Rich's own counting into the CEO's thread as the CEO's message
        /// (`docs/verification/2026-09-17-nightly-1.2.0-20260917.5-onscreen-audit.md`), and the
        /// bubble is still sitting there; nothing in the ledger says which of the two it was.
        ///
        /// ## THE THREE STATES ARE THREE DIFFERENT FACTS AND THE TYPE KEEPS THEM APART
        ///
        /// | value | means |
        /// |---|---|
        /// | `None` | **not recorded.** Every typed turn, every turn written before this field existed, and any spoken turn whose capture could not say. NEVER read as "no". |
        /// | `Some(false)` | measured: Rich's audible window was **closed** for the whole of this recording. Nothing of his can be in it. |
        /// | `Some(true)` | measured: the window was **open during some part of it**, so his voice may be in the audio these words came from. |
        ///
        /// `Some(true)` is emphatically **not** "this is echo". A turn reaches this field only
        /// by being ADMITTED, and an admitted utterance recorded while Rich was audible is the
        /// CEO deliberately talking over him (a barge-in cleared the taint) or a canceller
        /// confident enough to vouch for the residual. Both are real turns. What the flag
        /// records is that the audible window overlapped the audio — the one fact the capture
        /// path knows and nothing downstream can reconstruct.
        ///
        /// **Why it is `Option<bool>` and not a `bool`.** Slice 3b declined to add an
        /// always-`None` field on the grounds that it *"reads as never echo-born, worse than
        /// the gap"* — and that reasoning applies to a defaulting `bool` exactly as hard: a
        /// missing field deserializing to `false` would silently assert of every historical
        /// turn that Rich was provably silent, which is a claim nobody measured. The writer is
        /// the capture path (`CapMsg::Started { .. }` and the taint re-evaluated at
        /// utterance end), so this field is only ever written where the answer is known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rich_audible: Option<bool>,
        /// **WHICH MOUTH HIS WORDS CAME THROUGH** — `"desk"` for the Mac's own composer and
        /// microphone, `"phone"` for a paired phone (`steering::IntakeRecord::Channel`'s own
        /// value), written only on an install whose spine was told to keep it (operator
        /// back-end spec r3 (s): *"On an operator install the spine keeps the intake's
        /// `channel` on the prompt record, a new optional field the product path does not
        /// read"*).
        ///
        /// **`None` is "not recorded", never "the desk"**, the same three-state discipline as
        /// `rich_audible` above: every product turn, every turn written before this field
        /// existed, and every internal or proactive turn reads back as `None`, and a reader
        /// that must decide who may give work treats it as
        /// no origin at all. That is what keeps a phone sentence recorded before the keeping
        /// began from being read as one typed at the desk.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        channel: Option<String>,
        /// **THE SENDER'S OWN ID FOR THESE WORDS** — the `client_id` a paired phone assigned
        /// at Send (`steering::IntakeRecord::Channel`'s `client_id`), kept so the CEO row the
        /// Mac sends back names the message the phone is already showing (adoption ledger §2.8
        /// row C5: the client id is the identity end to end). Without it, a row that beat the
        /// phone's receipt could only be guessed at, and a voice note could not be matched at
        /// all until the receipt brought its transcript hash.
        ///
        /// `None` for every turn that did not come from a phone and every turn written before
        /// this field existed. Not a display field and not a de-duplication key: the intake id
        /// is that, and the phone's own receipt store is what makes its retries free.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        client_id: Option<String>,
        /// **THESE WORDS ARE THE TRANSCRIPT OF A VOICE MESSAGE HE RECORDED** — a phone voice
        /// note, transcribed by the Mac at intake (`steering::IntakeRecord::Channel`'s
        /// `voice_note`). `source` stays `Text` for it: `Jam` is the desk's live voice mode, and
        /// changing it would also change what the engine is told (`native.rs`) and the desk
        /// voice origin, which this field leaves alone. Kept so
        /// every row of the turn, the live one that can beat the phone's receipt included, is a
        /// voice row from its first appearance instead of a text row that turns into one.
        ///
        /// `false`, and no key, for everything else and every turn written before this field
        /// existed; an older build ignores the key.
        #[serde(default, skip_serializing_if = "is_false")]
        voice_note: bool,
        /// **THESE ARE NOT HIS WORDS: THE APP SENT THEM BECAUSE HE PRESSED A BUTTON** (product
        /// hunt 2026-10-15, finding 14). "Start the questions" sends Rich the sentence that
        /// starts the interview, and it used to appear in the conversation as a message he had
        /// typed. Rich still receives it; nothing shows it in his name.
        ///
        /// `false`, and no key, for everything else and every turn written before this field
        /// existed; an older build ignores the key.
        #[serde(default, skip_serializing_if = "is_false")]
        pressed: bool,
    },
    TurnStarted { turn_id: String, session_id: String, at: u64 },
    /// A streamed partial reply chunk — persisted incrementally so a half-written
    /// reply survives a mid-turn crash (§5.1).
    ///
    /// `seq` is the SHARED per-turn counter assigned at the client's drain point
    /// (`native.rs`'s `prompt` drain loop, techy-mode §1.4 G1) — the same counter `MachineryRecord.seq`
    /// carries. Persisting it is what makes *"he said X, then ran Y, then said Z"*
    /// survive a restart: without it the ledger holds only the concatenated reply, so the
    /// interleaving G1 guarantees LIVE is lost the moment the process exits.
    ///
    /// `#[serde(default)]` ⇒ `None` for every delta written before this field existed.
    /// `None` means "position not recorded", NOT position 0 — a legacy run is reported
    /// with an unknown position rather than a fabricated one (§22: show unknown).
    AssistantDelta {
        turn_id: String,
        text: String,
        at: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        seq: Option<u64>,
    },
    TurnCompleted { turn_id: String, stop_reason: String, at: u64 },
    TurnInterrupted {
        turn_id: String,
        reason: String,
        at: u64,
        /// Reopening observes that the old request ended, but cannot know when.
        #[serde(default, skip_serializing_if = "is_false")]
        recovered_after_restart: bool,
    },
    /// **The turn died to the UPSTREAM MODEL API** — `529 Overloaded`, `429` quota, a
    /// `5xx`. `open-items.md` row 3.30, measured 2026-09-03.
    ///
    /// **A SEPARATE event from `TurnInterrupted`, for the same reason `TurnStopped` is
    /// one.** `TurnInterrupted` carries a `reason` STRING, which is a convention: a broken
    /// pipe, a full disk, a rotation and an Anthropic outage all arrive on it and are told
    /// apart only by parsing English that nothing guarantees. Whether the failure was
    /// upstream decides what the CEO is told — whether waiting is a plan, and whether
    /// asking again costs him quota for nothing — so it is a fact the log states rather
    /// than a fact a later reader infers.
    ///
    /// It is written IN ADDITION to `TurnInterrupted`, never instead of it: the turn's
    /// state, `ended_at` and durable partial reply all come from that event and none of
    /// that changes. This one only attaches the classification and the sentences the CEO
    /// was actually shown.
    UpstreamFailure { turn_id: String, record: crate::upstream::UpstreamRecord, at: u64 },
    /// **WHY the turn was interrupted, classified at the failure boundary** — the nightly's
    /// D2 (`docs/verification/2026-09-17-nightly-1.2.0-20260917.1-onscreen-audit.md`).
    ///
    /// Written BESIDE `TurnInterrupted`, never instead of it, exactly as `UpstreamFailure`
    /// is and for exactly the same reason: `TurnInterrupted` carries a `reason` STRING, and
    /// a broken pipe, a missing sign-in and a rejected API key all arrive on it and are told
    /// apart only by parsing English that nothing guarantees. Whether the CEO is looking at
    /// something he can fix, something somebody else must fix, or something that clears on
    /// its own decides what he is told and whether a retry control is drawn at all — so it
    /// is a fact the log states rather than a fact a later reader infers.
    ///
    /// It carries the SENTENCES as well as the tag (`interruption::InterruptionRecord`), so
    /// reopening an old thread shows what he was told at the time rather than what a later
    /// build would say today.
    TurnInterruptionCause {
        turn_id: String,
        record: crate::interruption::InterruptionRecord,
        at: u64,
    },
    /// The CEO stopped this turn (UX §9.3 step 1-2). A SEPARATE event from
    /// `TurnInterrupted` on purpose: replaying the log must be able to tell a stop from a
    /// crash forever, and a `reason` string on the old event would have been a convention
    /// rather than a guarantee.
    ///
    /// `requested_at` is when the stop request became durable on disk (`steering.rs`), and
    /// `at` is when the turn actually ended. They differ by however long the lease took to
    /// let go — recorded, not averaged away, because the gap is the honest measure of how
    /// immediate "immediate" was.
    TurnStopped {
        turn_id: String, requested_at: u64, at: u64,
        #[serde(default, skip_serializing_if = "is_false")]
        recovered_after_restart: bool,
    },
    /// Recorded AS the action happens (not at turn-end) so replay can't double-execute (§5.4).
    /// `turn_id` is `None` for actions that are not turn-scoped — lease rotation and
    /// re-prime injection happen AT a turn boundary, BETWEEN turns, and claiming them
    /// against an arbitrary neighboring turn would be a fabricated association.
    /// Both new fields are `#[serde(default)]` so records written before they existed
    /// still replay (`visibility` defaults to `CeoFacing`, their original meaning).
    ActionRecorded {
        action_id: String,
        #[serde(default)]
        turn_id: Option<String>,
        kind: String,
        detail: String,
        status: ActionStatus,
        #[serde(default)]
        visibility: ActionVisibility,
        at: u64,
    },
    ActionUpdated { action_id: String, status: ActionStatus, at: u64 },
    /// A compute-lease swap. The conversation is unbroken; only the backing session changed.
    SessionRotated { from_session: String, to_session: String, reason: String, at: u64 },
    /// Rich reaching out unprompted (continuity §9 / UX §5) — written ATOMICALLY (no
    /// separate started/delta/completed cycle needed: the app already has the full text
    /// in hand when it raises this, unlike a live-streamed CEO-turn reply).
    ProactiveMessage {
        turn_id: String,
        thread_id: String,
        tier: AttentionTier,
        text: String,
        at: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        entity_id: Option<EntityId>,
        #[serde(default)]
        binding_revision: u64,
    },
    /// The self-authored handoff summary an outgoing lease produces before a clean
    /// rotation (continuity §2.4) — the highest-fidelity rolling-summary source. Keyed
    /// per thread; the latest one wins (each rotation replaces it, not appends).
    HandoffSummaryUpdated { thread_id: String, summary: String, at: u64 },
    /// Mid-turn-crash recovery (§5.3): `turn_id` was replayed as `by_turn_id` on a fresh
    /// lease. `turn_id` stays `interrupted` forever (the durable crash record — never
    /// edited in place) but is EXCLUDED from `messages()`/re-prime once superseded, so
    /// the CEO sees one clean exchange (the successful replay), not a duplicate.
    TurnSuperseded { turn_id: String, by_turn_id: String, at: u64 },
    /// One durable record creates a continuation and supersedes the uncertain attempt.
    TurnReplayed { turn_id: String, by_turn_id: String, at: u64 },
}

#[derive(Debug, Clone, Serialize)]
pub struct Thread {
    pub id: String,
    pub title: String,
    pub created_at: u64,
    /// The thread's home entity. PRIVATE and accessor-only: the entity home is immutable
    /// after creation (ECS §3.2), so there is no field to reach in and change. The single
    /// legal transition is `Unbound -> Bound` via `Ledger::adopt_unbound_thread`, applied
    /// inside this module and refused if the thread is already bound.
    entity: ThreadEntity,
}

impl Thread {
    pub fn entity(&self) -> &ThreadEntity {
        &self.entity
    }

    pub fn entity_id(&self) -> Option<&EntityId> {
        self.entity.entity_id()
    }

    pub fn is_bound(&self) -> bool {
        self.entity.is_bound()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Turn {
    pub id: String,
    pub thread_id: String,
    /// The entity this turn was accepted under. `None` only for turns written before
    /// entity scoping existed — those inherit their thread's home (the thread_id IS the
    /// binding; the stamp is a denormalized fence on top of it). A stamp that CONTRADICTS
    /// the thread's home is the cross-entity leak this field exists to catch, and such a
    /// turn is quarantined (see `Turn::quarantined`).
    pub entity_id: Option<EntityId>,
    /// The active-context revision this turn was accepted at (ECS §3.4 fencing token).
    pub binding_revision: u64,
    /// Set by `reconcile_scope` when this turn's entity stamp contradicts its thread's
    /// immutable home, or when its thread does not exist. A quarantined turn is excluded
    /// from every scoped projection — `messages()`, `thread_turns()` and therefore the
    /// re-prime payload. It is never deleted: the bytes stay as evidence.
    pub quarantined: bool,
    pub user_text: String,
    pub source: Source,
    pub state: TurnState,
    pub session_id: Option<String>,
    /// EVERY session a `TurnStarted` named for this turn, in order. `session_id` keeps only the
    /// last, and one turn can be started on two leases (a rotation before it ran, observed on the
    /// test VM 2026-10-05): the output record joins a hook witness to its thread by ANY session
    /// the ledger started a turn of that thread on (Output side panel PRD §4.1 (b)). Folded from
    /// the log like every other field here; not serialized, so no payload changes.
    #[serde(skip_serializing)]
    pub started_sessions: Vec<String>,
    /// Accumulated assistant reply (concatenated deltas). May be partial if interrupted.
    pub assistant_text: String,
    /// The SAME reply, split into contiguous RUNS of the shared per-turn sequence.
    ///
    /// `assistant_text` answers *"what did Rich say?"*. This answers *"where in the turn
    /// did he say each part of it?"* — the question §5.2-vs-§5.4 (commentary vs final
    /// response) is a special case of, and which one concatenated string cannot answer.
    /// A run ends wherever a non-text item consumed a sequence position, i.e. wherever a
    /// tool call interrupted the prose.
    ///
    /// Derived purely by folding `AssistantDelta`: no new event, no second counter.
    pub text_runs: Vec<TextRun>,
    pub stop_reason: Option<String>,
    pub created_at: u64,
    /// When the turn was handed to a compute lease (`TurnStarted.at`). `None` for a turn
    /// that was journaled but never started, and for an atomically-written proactive turn
    /// (there was no delivery span to measure).
    pub started_at: Option<u64>,
    /// When the turn reached a terminal state (`TurnCompleted.at` / `TurnInterrupted.at`).
    /// `None` while in flight AND for a turn whose process was killed mid-turn — a hard
    /// kill writes no terminal event, so the end of that span is genuinely unrecorded.
    /// UX §6.3's rule depends on this staying `None`: the elapsed time of an unfinished
    /// turn must never be recomputed from the current clock at read time, or an overnight
    /// restart turns a five-minute task into a twelve-hour one.
    pub ended_at: Option<u64>,
    /// Set only for `Source::Proactive` turns (the tier it was raised at).
    pub tier: Option<AttentionTier>,
    /// Set once this turn has been superseded by a mid-turn-crash replay (§5.3) — the
    /// id of the turn that completed the work instead. `messages()`/re-prime skip it.
    pub superseded_by: Option<String>,
    /// When the CEO's stop request became durable, for a turn in [`TurnState::Stopped`].
    /// `None` for every other state. Kept beside `ended_at` rather than folded into it so
    /// the LAG between "he asked" and "it let go" stays measurable after a restart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_requested_at: Option<u64>,
    /// Shared input order, also the intake drain's de-duplication key (UX §9.2).
    /// Direct desktop inputs may have no intake row. See `Event::PromptReceived::intake_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intake_id: Option<u64>,
    /// Set when this turn died to the UPSTREAM MODEL API (row 3.30). Carries the
    /// classification AND the sentences the CEO was shown at the time, so reopening the
    /// thread months later shows him what he was actually told rather than what this
    /// build would say today. `None` for every other outcome, including every ordinary
    /// interruption — absence here is a positive statement that the failure was local.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_failure: Option<crate::upstream::UpstreamRecord>,
    /// Set when this turn ended without finishing and the cause was classified (D2).
    /// Carries the classification AND the sentences the CEO was shown at the time.
    ///
    /// **`None` is not "no cause"** — it is "this build, or the build that wrote the record,
    /// did not classify it". Every turn written before 2026-09-17 reads back this way, and
    /// the surface falls back to the generic card for them rather than inventing a cause
    /// nobody recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interruption: Option<crate::interruption::InterruptionRecord>,
    /// Whether Rich's own voice was audible in the room while the audio this turn came from
    /// was being recorded. See `Event::PromptReceived::rich_audible` for the full three-state
    /// meaning; the short version is that **`None` is "not recorded" and never "no"**.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rich_audible: Option<bool>,
    /// Which mouth his words came through, when this install keeps it. See
    /// `Event::PromptReceived::channel`: **`None` is "not recorded"**, never "the desk".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    /// The phone's own id for these words, when they came from a phone. See
    /// `Event::PromptReceived::client_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// These words are the transcript of a voice message he recorded (a phone voice note). See
    /// `Event::PromptReceived::voice_note`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub voice_note: bool,
    /// The app sent these words for a button he pressed; they are never shown as his. See
    /// `Event::PromptReceived::pressed`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub pressed: bool,
}

impl Turn {
    /// The MEASURED active span of this turn, in millis — `ended_at - started_at`, and
    /// `None` whenever either endpoint is missing.
    ///
    /// Three refusals, all deliberate (UX §6.3, §22 "elapsed active time after restart"):
    ///   1. an in-flight turn returns `None`, never `now() - started_at` — a read at
    ///      09:00 the next morning would otherwise report a five-minute task as twelve
    ///      hours, which is the exact failure §6.3 calls out;
    ///   2. a turn killed mid-flight (no terminal event) returns `None` forever, because
    ///      when it actually stopped was never written down;
    ///   3. `checked_sub` ⇒ a clock that went backwards between the two events yields
    ///      `None` rather than a u64 that wrapped to ~584 million years.
    ///
    /// There is no pause accounting because there is no pause: §11's `waiting_for_user`
    /// state does not exist in this runtime yet, so no interval can be excluded. When it
    /// lands, this measure becomes wall time and MUST be replaced by accumulated active
    /// time, not extended.
    pub fn active_ms(&self) -> Option<u64> {
        self.ended_at?.checked_sub(self.started_at?)
    }
}

/// One contiguous stretch of assistant text inside a turn's shared sequence.
///
/// `start_seq`/`end_seq` are INCLUSIVE positions in the ONE per-turn counter shared with
/// machinery (§1.4 G1). Both are `None` for a run folded from pre-`seq` deltas: the text
/// is intact, its position was never recorded, and it is reported as unknown rather than
/// guessed at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TextRun {
    pub start_seq: Option<u64>,
    pub end_seq: Option<u64>,
    pub text: String,
    /// Epoch millis of the FIRST delta in the run. A label, never the ordering key.
    pub at: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Action {
    pub id: String,
    /// `None` for turn-BOUNDARY (not turn-scoped) actions — see `Event::ActionRecorded`.
    pub turn_id: Option<String>,
    pub kind: String,
    pub detail: String,
    pub status: ActionStatus,
    pub visibility: ActionVisibility,
    pub at: u64,
}

/// A rendered chat message — the CLEAN-OUTPUT view. Only user turns and assistant
/// reply text ever become messages; `Internal` (re-prime/proactive) turns never do.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Message {
    pub role: String, // "user" | "assistant"
    pub text: String,
    pub turn_id: String,
    pub at: u64,
}

/// One rejected cross-entity (or orphan) turn, recorded so the leak is VISIBLE rather
/// than silently dropped. UX §22's "Must not be faked" list means the honest answer to a
/// contradiction is to refuse to render it AND to say so — not to quietly show less.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScopeViolation {
    pub thread_id: String,
    pub turn_id: String,
    /// The thread's immutable home entity, or `None` if the thread is unbound/missing.
    pub thread_entity: Option<String>,
    /// The entity the turn claimed.
    pub turn_entity: Option<String>,
    pub detail: String,
}

/// EVERY `event` tag this build knows how to fold.
///
/// It is the only thing that can tell a record written by a NEWER RichOS apart from a
/// damaged one, so it has to stay exactly in step with [`Event`]. That is not left to
/// care: `ledger_forward_compat_tests.rs` maps every variant through an EXHAUSTIVE match,
/// so adding a variant to `Event` without adding its name here does not compile.
pub const KNOWN_EVENT_TAGS: &[&str] = &[
    "ThreadCreated",
    "ThreadEntityBound",
    "PromptReceived",
    "TurnStarted",
    "AssistantDelta",
    "TurnCompleted",
    "TurnInterrupted",
    "UpstreamFailure",
    "TurnInterruptionCause",
    "TurnStopped",
    "ActionRecorded",
    "ActionUpdated",
    "SessionRotated",
    "ProactiveMessage",
    "HandoffSummaryUpdated",
    "TurnSuperseded",
    "TurnReplayed",
];

/// The vocabulary for a record this build could not read — SHARED with `steering.rs`'s
/// intake log, and re-exported here so `ledger::SkipKind` keeps naming the same type it
/// always has. Definitions live in [`crate::skip`]; the reasoning for the split is in that
/// module's doc.
pub use crate::skip::{SkipKind, SkippedRecord};

/// How the ledger's own records are told apart from a newer build's and from damage.
///
/// `event` is the tag key every ledger record carries; the noun is what goes in the one
/// sentence that has to name the file, and "carries no writer version" is the fact that
/// makes `Ambiguous` unresolvable here (see [`Ledger::history_health`]).
const LEDGER_SKIP: crate::skip::SkipDialect = crate::skip::SkipDialect {
    tag_key: "event",
    record_noun: "a ledger record",
    known_tags: KNOWN_EVENT_TAGS,
};

/// What the app can honestly say about a history it has just loaded.
///
/// Serializable on purpose: this is the state the window renders. `headline` and `detail`
/// are empty strings when nothing was skipped, which is the ordinary case and the signal
/// to render nothing at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HistoryHealth {
    /// Non-empty lines found in the file.
    pub records_read: usize,
    /// Records folded into the projection.
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

impl HistoryHealth {
    /// Every record on disk is in the projection.
    pub fn is_clean(&self) -> bool {
        self.skipped == 0
    }
}

type ReadObserver = std::sync::Arc<dyn Fn(&Event) + Send + Sync>;

pub struct Ledger {
    path: PathBuf,
    file: Option<File>,
    read_observer: Option<ReadObserver>,
    threads: Vec<Thread>,
    turns: Vec<Turn>,
    actions: Vec<Action>,
    /// thread_id -> latest self-authored handoff summary (continuity §2.4).
    handoff_summaries: std::collections::HashMap<String, String>,
    /// Every turn whose entity stamp contradicted its thread's home, found on replay.
    scope_violations: Vec<ScopeViolation>,
    /// Every record on disk that this build could not fold. Empty is the normal state.
    skipped: Vec<SkippedRecord>,
    /// Non-empty lines seen on replay, and how many of them were applied. Counted rather
    /// than derived, so "71 of 74" is a measurement and not an inference.
    records_read: usize,
    records_applied: usize,
    /// The next active-context binding revision to hand out (ECS §3.4 fencing token).
    ///
    /// Monotonic in-process. On open it resumes at one past the highest revision EVER
    /// stamped on a persisted event, so a restart never re-issues a revision that a
    /// durable record already used. Slice 1 writes no event per activation, so a
    /// revision consumed by an activation that produced no write is not durable and can
    /// be reused after a restart. That is safe today because a fencing token cannot
    /// outlive the process that captured it; if activations ever need auditing, an
    /// explicit activation event is the fix, and it is not in this slice.
    next_revision: u64,
}

fn is_false(value: &bool) -> bool { !*value }

impl Ledger {
    // Snapshots have no file handle and expose no mutation capability to window readers.
    pub(crate) fn read_copy(&self) -> Self {
        Self {
            path: self.path.clone(), file: None, read_observer: None,
            threads: self.threads.clone(), turns: self.turns.clone(), actions: self.actions.clone(),
            handoff_summaries: self.handoff_summaries.clone(),
            scope_violations: self.scope_violations.clone(), skipped: self.skipped.clone(),
            records_read: self.records_read, records_applied: self.records_applied,
            next_revision: self.next_revision,
        }
    }

    pub(crate) fn observe_reads(&mut self, observer: ReadObserver) {
        self.read_observer = Some(observer);
    }

    pub(crate) fn apply_read_event(&mut self, event: Event) {
        let reconcile = matches!(event, Event::ThreadEntityBound { .. });
        self.apply(event);
        if reconcile { self.reconcile_scope(); }
    }

    /// Open (creating if needed) and replay the on-disk log into the projection.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, LedgerError> {
        let path = path.as_ref().to_path_buf();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut ledger = Ledger {
            path: path.clone(),
            file: Some(OpenOptions::new().create(true).append(true).read(true).open(&path)?),
            read_observer: None,
            threads: Vec::new(),
            turns: Vec::new(),
            actions: Vec::new(),
            handoff_summaries: std::collections::HashMap::new(),
            scope_violations: Vec::new(),
            skipped: Vec::new(),
            records_read: 0,
            records_applied: 0,
            next_revision: 1,
        };
        ledger.replay()?;
        // Scope reconciliation runs AFTER the whole log is folded, so the verdict never
        // depends on event ORDER (a thread's adoption event can legally arrive after the
        // turns it covers). Every turn is checked against its thread's final home entity.
        ledger.reconcile_scope();
        Ok(ledger)
    }

    /// Fold the whole log, and **survive any single record it cannot read.**
    ///
    /// # Why this is not `serde_json::from_str(line)?`
    ///
    /// It was, until 2026-09-05, and `?` on a per-record parse means ONE unreadable line
    /// aborts the entire replay. The failure is total, not partial: not "that message is
    /// missing" but "your conversation history does not load". At the app's only real call
    /// site — `src-tauri/src/main.rs`, `Ledger::open(&ledger_path).expect("open ledger")` —
    /// it is not even a blank history, it is a panic inside the Tauri setup hook, so the
    /// window never opens.
    ///
    /// A record type added by a newer RichOS is, to an older binary, exactly that
    /// unreadable line. v1.0.0, v1.0.1 and v1.0.2 are all still published and downloadable,
    /// the updater has no rollback, and reverting code does not revert data — so the moment
    /// a newer build writes a record type an older one cannot name, any customer who
    /// reinstalls an older build has lost their history until they update again.
    ///
    /// # What it does instead
    ///
    /// Every line that parses is folded. Every line that does not is recorded in
    /// [`Ledger::skipped_records`] with a REASON, counted, and printed — never silently
    /// dropped. Nothing is deleted, nothing is rewritten; the bytes stay exactly where they
    /// were, so a build new enough to understand them will read them.
    ///
    /// # What still fails
    ///
    /// A genuine IO error. If the disk cannot be read, that is not a damaged record and
    /// must not be reported as one — it propagates, as it always did.
    fn replay(&mut self) -> Result<(), LedgerError> {
        let mut reader = BufReader::new(File::open(&self.path)?);
        let mut raw: Vec<u8> = Vec::new();
        let mut line_no = 0usize;
        loop {
            raw.clear();
            // `read_until`, not `lines()`, for one reason: `lines()` yields `Err` for a line
            // that is not valid UTF-8, and the `?` on that error would abort the whole
            // replay over one bad byte — the exact failure this function exists to stop. A
            // real IO error still propagates from here; a bad byte is classified below.
            let n = reader.read_until(b'\n', &mut raw)?;
            if n == 0 {
                break;
            }
            line_no += 1;
            // `lines()` strips the terminator and any preceding CR, and the old code then
            // called `.trim()`. Stripping the `\n` and calling the same `.trim()` gives a
            // byte-identical slice — `trim` removes the CR and everything else `lines()`
            // would have.
            let body = raw.strip_suffix(b"\n").unwrap_or(&raw);
            let text = match std::str::from_utf8(body) {
                Ok(t) => t,
                Err(_) => {
                    // Not text at all. It cannot have a tag, so it cannot be from the
                    // future; no version of RichOS has ever written a non-UTF-8 record.
                    self.skipped.push(crate::skip::not_utf8(line_no, body.len()));
                    self.records_read += 1;
                    continue;
                }
            };
            let line = text.trim();
            if line.is_empty() {
                continue;
            }
            self.records_read += 1;
            match serde_json::from_str::<Event>(line) {
                Ok(event) => {
                    self.records_applied += 1;
                    self.apply(event);
                }
                Err(_) => {
                    let record = Self::classify_skip(line_no, line);
                    // A record from a newer RichOS still carries a real fencing token, and
                    // the counter has to stay ahead of every revision DURABLY recorded or a
                    // restart re-issues one this file already used (ECS §3.4). Salvaged
                    // only for `FromFuture`: a damaged line's number is not a fact.
                    if record.kind == SkipKind::FromFuture {
                        if let Some(r) = Self::salvage_revision(line) {
                            self.observe_revision(r);
                        }
                    }
                    self.skipped.push(record);
                }
            }
        }
        self.report_skipped();
        Ok(())
    }

    /// Decide WHY a line did not parse. The judgment — and every sentence it composes —
    /// lives in [`crate::skip::classify_line`], which `steering.rs` calls with its own
    /// dialect so the two stores cannot drift apart about what damage looks like.
    fn classify_skip(line_no: usize, line: &str) -> SkippedRecord {
        crate::skip::classify_line(line_no, line, &LEDGER_SKIP)
    }

    /// Pull a `binding_revision` off a record this build could not fold. Only ever called
    /// for a record from a newer RichOS, whose revision is a real durable fencing token.
    fn salvage_revision(line: &str) -> Option<u64> {
        crate::skip::salvage_u64(line, "binding_revision")
    }

    /// **Skipping is never silent.** One line per skipped record, capped, then a summary
    /// line that is printed whenever anything at all was skipped.
    ///
    /// The cap exists because a badly damaged file could hold a hundred thousand
    /// unreadable lines, and a hundred thousand log lines is its own kind of silence. The
    /// COUNT is never capped — it is in the summary and in [`Ledger::history_health`].
    fn report_skipped(&self) {
        if self.skipped.is_empty() {
            return;
        }
        const CAP: usize = 20;
        for r in self.skipped.iter().take(CAP) {
            eprintln!(
                "[richos] LEDGER RECORD SKIPPED ({}): line {} ({} bytes) — {}",
                r.kind.label(),
                r.line,
                r.bytes,
                r.detail
            );
        }
        if self.skipped.len() > CAP {
            eprintln!(
                "[richos] LEDGER: {} further skipped records not listed individually",
                self.skipped.len() - CAP
            );
        }
        let h = self.history_health();
        eprintln!(
            "[richos] LEDGER SUMMARY for {}: {} records read, {} applied, {} skipped \
             ({} from a newer version, {} damaged, {} undetermined). Nothing was deleted \
             or rewritten.",
            self.path.display(),
            h.records_read,
            h.records_applied,
            h.skipped,
            h.from_future,
            h.damaged,
            h.ambiguous
        );
    }

    /// Fold one event into the in-memory projection. Pure; no I/O.
    /// (`Self::push_text_run` does the text-run splitting.)
    fn apply(&mut self, event: Event) {
        match event {
            Event::ThreadCreated { thread_id, title, at, entity_id, person_id, binding_revision } => {
                self.observe_revision(binding_revision);
                let entity = match entity_id {
                    Some(entity_id) => ThreadEntity::Bound {
                        person_id: person_id.unwrap_or_default(),
                        entity_id,
                        binding_revision,
                    },
                    // Pre-entity record. NOT migrated by a heuristic — quarantined.
                    None => ThreadEntity::Unbound,
                };
                self.threads.push(Thread { id: thread_id, title, created_at: at, entity });
            }
            Event::ThreadEntityBound { thread_id, entity_id, person_id, binding_revision, at: _, adopted_by: _ } => {
                self.observe_revision(binding_revision);
                if let Some(t) = self.threads.iter_mut().find(|t| t.id == thread_id) {
                    // The single legal transition, and it is one-way. An adoption event
                    // aimed at an already-bound thread is IGNORED on replay rather than
                    // applied — a forged or duplicated record must never rebind a thread.
                    if !t.entity.is_bound() {
                        t.entity = ThreadEntity::Bound { person_id, entity_id, binding_revision };
                    }
                }
            }
            Event::PromptReceived {
                turn_id,
                thread_id,
                text,
                source,
                at,
                entity_id,
                binding_revision,
                intake_id,
                rich_audible,
                channel,
                client_id,
                voice_note,
                pressed,
            } => {
                self.observe_revision(binding_revision);
                self.turns.push(Turn {
                    id: turn_id,
                    thread_id,
                    entity_id,
                    binding_revision,
                    quarantined: false,
                    user_text: text,
                    source,
                    state: TurnState::Received,
                    session_id: None,
                    started_sessions: Vec::new(),
                    assistant_text: String::new(),
                    text_runs: Vec::new(),
                    stop_reason: None,
                    created_at: at,
                    started_at: None,
                    ended_at: None,
                    tier: None,
                    superseded_by: None,
                    stop_requested_at: None,
                    intake_id,
                    upstream_failure: None,
                    interruption: None,
                    rich_audible,
                    channel,
                    client_id,
                    voice_note,
                    pressed,
                });
            }
            Event::TurnStarted { turn_id, session_id, at } => {
                if let Some(t) = self.turn_mut(&turn_id) {
                    t.state = TurnState::InFlight;
                    if !session_id.is_empty() && !t.started_sessions.contains(&session_id) {
                        t.started_sessions.push(session_id.clone());
                    }
                    // An accepted request can start connecting before a session exists.
                    // A later start record attaches the real session without resetting time.
                    t.session_id = (!session_id.is_empty()).then_some(session_id);
                    // FIRST start wins. A mid-turn-crash replay is a NEW turn id (§5.3),
                    // so a second start on THIS id would be a duplicated record, not a
                    // legitimate restart of the same span.
                    t.started_at.get_or_insert(at);
                }
            }
            Event::AssistantDelta { turn_id, text, at, seq } => {
                if let Some(t) = self.turn_mut(&turn_id) {
                    t.assistant_text.push_str(&text);
                    Self::push_text_run(&mut t.text_runs, seq, &text, at);
                }
            }
            Event::TurnCompleted { turn_id, stop_reason, at } => {
                if let Some(t) = self.turn_mut(&turn_id) {
                    t.state = TurnState::Completed;
                    t.stop_reason = Some(stop_reason);
                    t.ended_at = Some(at);
                }
            }
            Event::TurnInterrupted { turn_id, reason, at, recovered_after_restart } => {
                if let Some(t) = self.turn_mut(&turn_id) {
                    t.state = TurnState::Interrupted;
                    t.stop_reason = Some(format!("interrupted: {reason}"));
                    t.ended_at = (!recovered_after_restart).then_some(at);
                }
            }
            // ATTACHES ONLY. It sets no state, no `ended_at` and no `stop_reason` —
            // `TurnInterrupted` owns all three and this event is written beside it, so a
            // build that skipped this record entirely would still read the turn as
            // interrupted at the same instant with the same partial reply.
            Event::UpstreamFailure { turn_id, record, .. } => {
                if let Some(t) = self.turn_mut(&turn_id) {
                    t.upstream_failure = Some(record);
                }
            }
            // ATTACHES ONLY, for the reason the arm above gives: `TurnInterrupted` owns the
            // state, the `ended_at` and the `stop_reason`, and a build that skipped this
            // record entirely would still read the turn as interrupted at the same instant.
            Event::TurnInterruptionCause { turn_id, record, .. } => {
                if let Some(t) = self.turn_mut(&turn_id) {
                    t.interruption = Some(record);
                }
            }
            Event::TurnStopped { turn_id, requested_at, at, recovered_after_restart } => {
                if let Some(t) = self.turn_mut(&turn_id) {
                    // A stop OVERRIDES nothing that already ended: a turn that completed
                    // before the stop request reached the lease stays completed, because
                    // it did. Only a turn still open is stopped.
                    //
                    // REACHABLE FROM THE LIVE PATH SINCE 2026-08-29, and it was not before.
                    // `Spine::deliver` used to take its stop branch on the existence of a
                    // stop claim alone, so the turn was still `InFlight` when `stop_turn`
                    // was called and this clause could only ever fire in a unit test that
                    // called the ledger directly. `finish_completed_turn_the_stop_missed`
                    // now calls `complete_turn` first and `stop_turn` second, which is what
                    // makes the request a recorded fact AND leaves the verdict alone.
                    if matches!(t.state, TurnState::Received | TurnState::InFlight) {
                        t.state = TurnState::Stopped;
                        t.stop_reason = Some("stopped_by_ceo".to_string());
                        t.ended_at = (!recovered_after_restart).then_some(at);
                        t.stop_requested_at = Some(requested_at);
                    }
                }
            }
            Event::ActionRecorded { action_id, turn_id, kind, detail, status, visibility, at } => {
                self.actions.push(Action { id: action_id, turn_id, kind, detail, status, visibility, at });
            }
            Event::ActionUpdated { action_id, status, .. } => {
                if let Some(a) = self.actions.iter_mut().find(|a| a.id == action_id) {
                    a.status = status;
                }
            }
            Event::SessionRotated { .. } => { /* projection-neutral; kept for audit/replay */ }
            Event::ProactiveMessage { turn_id, thread_id, tier, text, at, entity_id, binding_revision } => {
                self.observe_revision(binding_revision);
                // A proactive message is written as a COMPLETE turn in one atomic event —
                // no started/delta/completed cycle, unlike a live-streamed CEO-turn reply
                // (the app already holds the full text when it raises this).
                self.turns.push(Turn {
                    id: turn_id,
                    thread_id,
                    entity_id,
                    binding_revision,
                    quarantined: false,
                    user_text: String::new(),
                    source: Source::Proactive,
                    state: TurnState::Completed,
                    session_id: None,
                    started_sessions: Vec::new(),
                    text_runs: vec![TextRun { start_seq: None, end_seq: None, text: text.clone(), at }],
                    assistant_text: text,
                    stop_reason: Some("proactive".to_string()),
                    created_at: at,
                    // NO delivery span: this turn was written atomically, never streamed
                    // through a lease. Both endpoints stay `None` so nothing can render a
                    // 0ms "Worked for" — that would be a measurement claim about work this
                    // ledger never saw happen.
                    started_at: None,
                    ended_at: None,
                    tier: Some(tier),
                    superseded_by: None,
                    stop_requested_at: None,
                    intake_id: None,
                    upstream_failure: None,
                    interruption: None,
                    // A proactive turn has no captured audio at all, so there is nothing to
                    // record about Rich's audible window. `None` is the honest answer and the
                    // only one: `Some(false)` here would assert a measurement of a microphone
                    // that was never opened.
                    rich_audible: None,
                    channel: None,
                    client_id: None,
                    voice_note: false,
                    pressed: false,
                });
            }
            Event::HandoffSummaryUpdated { thread_id, summary, .. } => {
                self.handoff_summaries.insert(thread_id, summary);
            }
            Event::TurnReplayed { turn_id, by_turn_id, at } => {
                if let Some(index) = self.turns.iter().position(|turn| turn.id == turn_id) {
                    let original = self.turns[index].clone();
                    if matches!(original.state, TurnState::Received | TurnState::InFlight) {
                        let retired = self.turn_mut(&turn_id).unwrap();
                        retired.state = TurnState::Interrupted;
                        retired.ended_at = None;
                    }
                    self.apply(Event::PromptReceived {
                        turn_id: by_turn_id.clone(), thread_id: original.thread_id,
                        text: original.user_text, source: original.source, at,
                        entity_id: original.entity_id, binding_revision: original.binding_revision,
                        intake_id: original.intake_id, rich_audible: original.rich_audible,
                        channel: original.channel, client_id: None,
                        voice_note: original.voice_note, pressed: original.pressed,
                    });
                    // A continuation replaces this request's place in acceptance order.
                    // Appending it after later requests reverses corrections on another restart.
                    let continuation = self.turns.pop().unwrap();
                    self.turns.insert(index + 1, continuation);
                    self.apply(Event::TurnSuperseded { turn_id, by_turn_id, at });
                }
            }
            Event::TurnSuperseded { turn_id, by_turn_id, .. } => {
                if let Some(t) = self.turn_mut(&turn_id) {
                    t.superseded_by = Some(by_turn_id);
                }
            }
        }
    }

    fn turn_mut(&mut self, id: &str) -> Option<&mut Turn> {
        self.turns.iter_mut().find(|t| t.id == id)
    }

    /// Fold one delta into a turn's text RUNS.
    ///
    /// A delta EXTENDS the open run when it sits at the very next sequence position.
    /// Anything else starts a NEW run, because a gap in the shared counter means a
    /// non-text item occupied the positions in between (§1.4 G1) — and that gap is
    /// exactly the boundary these runs exist to record.
    ///
    /// Two legacy deltas (both `seq: None`) also extend one another: they are known to be
    /// consecutive in ARRIVAL order, which is the order the append-only log holds them
    /// in, even though their absolute positions were never written down. A positioned
    /// delta never merges with an unpositioned one in either direction — that would
    /// assert an adjacency nothing recorded.
    fn push_text_run(runs: &mut Vec<TextRun>, seq: Option<u64>, text: &str, at: u64) {
        let extend = match runs.last() {
            Some(last) => match (last.end_seq, seq) {
                (Some(end), Some(next)) => end + 1 == next,
                (None, None) => true,
                _ => false,
            },
            None => false,
        };
        match runs.last_mut() {
            Some(last) if extend => {
                last.text.push_str(text);
                last.end_seq = seq;
            }
            _ => runs.push(TextRun { start_seq: seq, end_seq: seq, text: text.to_string(), at }),
        }
    }

    /// Keep the revision counter ahead of everything durably recorded.
    ///
    /// `saturating_add` rather than `+ 1` since 2026-09-05: this is now also fed from a
    /// record this build could not fold, so the number arriving here can be anything a
    /// damaged or forged line contains, up to `u64::MAX`. `seen + 1` on `u64::MAX` panics
    /// in a debug build and wraps to 0 in a release one, and a fencing counter that wraps
    /// to 0 is worse than one that stops moving.
    fn observe_revision(&mut self, seen: u64) {
        if seen >= self.next_revision {
            self.next_revision = seen.saturating_add(1);
        }
    }

    /// Hand out the next active-context binding revision (ECS §3.4 fencing token).
    pub(crate) fn take_revision(&mut self) -> u64 {
        let r = self.next_revision;
        self.next_revision += 1;
        r
    }

    /// THE CROSS-ENTITY GUARD (UX §25 Integrity: *"An event with the wrong entity, thread
    /// or binding revision is rejected"*; §22: cross-entity context must not be faked).
    ///
    /// Walks every turn once the whole log is folded and quarantines any turn that:
    ///
    ///   - claims an entity DIFFERENT from its thread's immutable home — the actual
    ///     privacy-boundary leak, whether it arrived from a corrupt log, a forged line,
    ///     a bad migration or a future writer bug that stamped a stale active context; or
    ///   - claims an entity while its thread has no home at all (incoherent: a turn
    ///     cannot be scoped to an entity that its thread is not in); or
    ///   - references a thread that does not exist (an orphan has no scope to check
    ///     against, so it fails closed).
    ///
    /// A turn with NO stamp inside a bound thread is NOT a violation: it predates entity
    /// scoping, its `thread_id` is the binding, and the thread's home is the authority.
    /// Inheriting there is the thread record speaking, not a guess.
    ///
    /// Quarantine excludes, it never deletes — the bytes remain on disk as evidence and
    /// the violation is reported through [`Ledger::scope_violations`].
    fn reconcile_scope(&mut self) {
        self.scope_violations.clear();
        let homes: Vec<(String, Option<EntityId>, bool)> = self
            .threads
            .iter()
            .map(|t| (t.id.clone(), t.entity_id().cloned(), t.is_bound()))
            .collect();
        let mut violations = Vec::new();
        for turn in self.turns.iter_mut() {
            let home = homes.iter().find(|(id, _, _)| id == &turn.thread_id);
            let (thread_entity, thread_exists) = match home {
                Some((_, e, _)) => (e.clone(), true),
                None => (None, false),
            };
            let violation_detail = if !thread_exists {
                Some("turn references a thread that does not exist — no scope to verify against".to_string())
            } else {
                match (&thread_entity, &turn.entity_id) {
                    (Some(home), Some(claimed)) if home != claimed => Some(format!(
                        "turn claims entity {claimed} but its thread's immutable home is {home} — \
                         cross-entity event rejected"
                    )),
                    (None, Some(claimed)) => Some(format!(
                        "turn claims entity {claimed} but its thread has no entity home — \
                         incoherent scope, rejected"
                    )),
                    _ => None,
                }
            };
            match violation_detail {
                Some(detail) => {
                    turn.quarantined = true;
                    violations.push(ScopeViolation {
                        thread_id: turn.thread_id.clone(),
                        turn_id: turn.id.clone(),
                        thread_entity: thread_entity.as_ref().map(|e| e.to_string()),
                        turn_entity: turn.entity_id.as_ref().map(|e| e.to_string()),
                        detail,
                    });
                }
                None => turn.quarantined = false,
            }
        }
        for v in &violations {
            // Loud, not silent: a rejected cross-entity event is an integrity incident.
            eprintln!("[richos] SCOPE VIOLATION rejected: turn={} thread={} {}", v.turn_id, v.thread_id, v.detail);
        }
        self.scope_violations = violations;
    }

    /// Every cross-entity/orphan turn rejected by [`Ledger::reconcile_scope`].
    pub fn scope_violations(&self) -> &[ScopeViolation] {
        &self.scope_violations
    }

    /// Every record on disk this build could not fold, with the reason for each.
    ///
    /// Empty is the normal state and the one every existing ledger produces today.
    pub fn skipped_records(&self) -> &[SkippedRecord] {
        &self.skipped
    }

    /// **What the app can honestly say about the history it just loaded**, in a form the
    /// window can render.
    ///
    /// `headline`/`detail` are empty when nothing was skipped — that is the signal to show
    /// nothing at all, not to show a reassuring green tick over a check that found nothing
    /// to say.
    ///
    /// ## The one thing this cannot answer, and the smallest change that would
    ///
    /// `ambiguous` counts records whose TYPE this build knows and whose FIELDS do not fit.
    /// A newer RichOS that added a required field to an existing record and a record whose
    /// bytes were damaged in place produce byte-identical evidence, and this build refuses
    /// to guess between them.
    ///
    /// The minimal fix is one field: a writer-schema number on every record — `launch.rs`
    /// already does exactly this with `StoredLaunches::schema_version`. A reader could then
    /// say `written_by > mine ⇒ from the future`, `<= mine ⇒ damaged`, with no ambiguity
    /// left. Adding it is safe by construction: serde ignores unknown fields on a known
    /// variant, which `ledger_forward_compat_tests.rs` pins, so a record carrying the new
    /// field still replays cleanly on v1.0.0-v1.0.2. It is NOT done here, because starting
    /// to write a new field is a change to what goes on customers' disks and this change is
    /// deliberately read-only.
    pub fn history_health(&self) -> HistoryHealth {
        let count = |k: SkipKind| self.skipped.iter().filter(|r| r.kind == k).count();
        let from_future = count(SkipKind::FromFuture);
        let damaged = count(SkipKind::Damaged);
        let ambiguous = count(SkipKind::Ambiguous);
        let skipped = self.skipped.len();

        let (headline, detail) = if skipped == 0 {
            (String::new(), String::new())
        } else {
            let plural = |n: usize, one: &str, many: &str| if n == 1 { one.to_string() } else { many.to_string() };
            let headline = if damaged > 0 || ambiguous > 0 {
                format!(
                    "{} of this conversation could not be read.",
                    plural(damaged + ambiguous, "One record", "Some records")
                )
            } else {
                "Part of this conversation was written by a newer version of RichOS."
                    .to_string()
            };
            let mut parts: Vec<String> = Vec::new();
            if from_future > 0 {
                parts.push(format!(
                    "{} {} written by a newer version of RichOS than the one you are running, \
                     so this version does not know how to read {}. Updating will bring {} back.",
                    from_future,
                    plural(from_future, "record was", "records were"),
                    plural(from_future, "it", "them"),
                    plural(from_future, "it", "them"),
                ));
            }
            if damaged > 0 {
                parts.push(format!(
                    "{} {} damaged and could not be read.",
                    damaged,
                    plural(damaged, "record is", "records are"),
                ));
            }
            if ambiguous > 0 {
                parts.push(format!(
                    "{} {} not match any shape this version knows. That is either a newer \
                     version of RichOS or damage, and this version cannot tell which.",
                    ambiguous,
                    plural(ambiguous, "record does", "records do"),
                ));
            }
            parts.push(format!(
                "Everything else loaded: {} of {} records. Nothing was deleted and nothing was \
                 rewritten: every record is still exactly where it was on disk.",
                self.records_applied, self.records_read,
            ));
            (headline, parts.join(" "))
        };

        HistoryHealth {
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

    fn thread_ref(&self, thread_id: &str) -> Result<&Thread, LedgerError> {
        self.threads
            .iter()
            .find(|t| t.id == thread_id)
            .ok_or_else(|| LedgerError::UnknownThread(thread_id.to_string()))
    }

    /// Obtain the durable scope of a thread. **This is the only way to get a
    /// [`ThreadBinding`] outside this crate** — the entity comes from the immutable
    /// record, never from the caller. An unbound (pre-entity) thread fails closed here,
    /// which is what makes `entity_id` genuinely required before retrieval (ECS §3.3).
    pub fn thread_binding(&self, thread_id: &str) -> Result<ThreadBinding, LedgerError> {
        match &self.thread_ref(thread_id)?.entity {
            ThreadEntity::Bound { person_id, entity_id, binding_revision } => Ok(ThreadBinding::new(
                person_id.clone(),
                entity_id.clone(),
                thread_id,
                *binding_revision,
            )),
            ThreadEntity::Unbound => Err(LedgerError::UnboundThread(thread_id.to_string())),
        }
    }

    /// Re-issue a thread's binding at a NEW fencing revision (an active-context
    /// activation transaction — ECS §11.3). The entity is re-read from the durable
    /// record, so activation can never move a thread between entities; only the fence
    /// advances.
    pub(crate) fn rebind_at_new_revision(&mut self, thread_id: &str) -> Result<ThreadBinding, LedgerError> {
        let base = self.thread_binding(thread_id)?;
        let revision = self.take_revision();
        Ok(ThreadBinding::new(base.person_id().clone(), base.entity_id().clone(), thread_id, revision))
    }

    /// Verify a presented binding against the thread's immutable home before any scoped
    /// read or write. Three checks, all of which must pass:
    ///   1. the thread exists and is bound (else `UnknownThread` / `UnboundThread`);
    ///   2. the presented entity equals the home entity (else `ScopeMismatch`);
    ///   3. the presented revision is not older than the thread's own binding revision
    ///      (else `StaleBinding`).
    pub fn verify_binding(&self, binding: &ThreadBinding) -> Result<(), LedgerError> {
        let thread = self.thread_ref(binding.thread_id())?;
        match &thread.entity {
            ThreadEntity::Unbound => Err(LedgerError::UnboundThread(binding.thread_id().to_string())),
            ThreadEntity::Bound { entity_id, binding_revision, .. } => {
                if entity_id != binding.entity_id() {
                    return Err(LedgerError::ScopeMismatch {
                        thread_id: binding.thread_id().to_string(),
                        home: entity_id.to_string(),
                        presented: binding.entity_id().to_string(),
                    });
                }
                if binding.binding_revision() < *binding_revision {
                    return Err(LedgerError::StaleBinding {
                        thread_id: binding.thread_id().to_string(),
                        presented: binding.binding_revision(),
                        current: *binding_revision,
                    });
                }
                Ok(())
            }
        }
    }

    /// Append + FLUSH one event durably, then fold it into the projection.
    /// `sync` forces an fsync for crash-critical events (the persist-before-send floor).
    fn append(&mut self, event: Event, sync: bool) -> Result<(), LedgerError> {
        // `stamped_line`, not `to_string` — spec point 18. The line carries this build's
        // `written_by` so a LATER build can tell a record from the future from a damaged
        // one without guessing; see [`crate::skip::classify_line`] step 4 and the note on
        // [`Ledger::history_health`] that asked for it.
        let mut line = crate::skip::stamped_line(&event)?;
        line.push('\n');
        let file = self.file.as_mut().ok_or_else(|| std::io::Error::other("read-only ledger snapshot"))?;
        crate::util::ensure_line_boundary(file)?;
        file.write_all(line.as_bytes())?;
        file.flush()?;
        if sync {
            file.sync_data()?;
        }
        self.apply(event.clone());
        if let Some(observer) = &self.read_observer {
            observer(&event);
        }
        Ok(())
    }

    // ---- write API ---------------------------------------------------------

    /// Create a thread with its IMMUTABLE entity home (ECS §3.2 / UX §3.3 step 1:
    /// *"Persist the thread with immutable `entity_id`"*). There is no entity-less
    /// overload on purpose — the app cannot mint an unbound thread, so `Unbound` can only
    /// ever be a pre-existing on-disk record. Returns the new thread id.
    pub fn create_thread(&mut self, title: &str, entity_id: &EntityId) -> Result<String, LedgerError> {
        self.create_thread_for(&PersonId::default_ceo(), title, entity_id)
    }

    /// **A thread id minted WITHOUT a record** — the one thing a spare front desk needs and
    /// the one thing UX §3.3 does not forbid.
    ///
    /// §3.3 is *"no pre-created thread record until the CEO sends the first message"*, and
    /// this writes nothing: no event, no file, no append. It hands back a uuid v4 under the
    /// `thr` prefix so that a child process can be SCOPED to the conversation the CEO is
    /// about to start — `EngineProfile::scope_to` pins `(entity_id, thread_id)` into the
    /// child's environment and workspace partition before it is spawned, and a spare that
    /// was spawned under some other thread's id and later adopted would be that child
    /// writing under a name that is not its own.
    ///
    /// **An id that is never spent costs nothing**, because nothing was written. A reserved
    /// id is spent exactly once, by [`Ledger::create_thread_with_reserved_id`], which
    /// refuses a second spend rather than appending a duplicate.
    pub fn reserve_thread_id() -> String {
        new_id("thr")
    }

    /// Create a thread under an id [`Ledger::reserve_thread_id`] handed out earlier.
    ///
    /// Identical to [`Ledger::create_thread`] in every other respect — same event, same
    /// immutable entity home, same fresh binding revision — so the record the CEO's first
    /// send produces is the record he would have got anyway. The id is the only thing that
    /// was decided in advance, and it was decided so that a child spawned before this moment
    /// is scoped to THIS conversation.
    pub fn create_thread_with_reserved_id(
        &mut self,
        thread_id: &str,
        title: &str,
        entity_id: &EntityId,
    ) -> Result<String, LedgerError> {
        if self.threads.iter().any(|t| t.id == thread_id) {
            return Err(LedgerError::ThreadIdAlreadySpent(thread_id.to_string()));
        }
        self.append_thread_created(&PersonId::default_ceo(), thread_id, title, entity_id)
    }

    pub fn create_thread_for(
        &mut self,
        person_id: &PersonId,
        title: &str,
        entity_id: &EntityId,
    ) -> Result<String, LedgerError> {
        let id = new_id("thr");
        self.append_thread_created(person_id, &id, title, entity_id)
    }

    fn append_thread_created(
        &mut self,
        person_id: &PersonId,
        id: &str,
        title: &str,
        entity_id: &EntityId,
    ) -> Result<String, LedgerError> {
        let id = id.to_string();
        let binding_revision = self.take_revision();
        self.append(
            Event::ThreadCreated {
                thread_id: id.clone(),
                title: title.to_string(),
                at: now_millis(),
                entity_id: Some(entity_id.clone()),
                person_id: Some(person_id.clone()),
                binding_revision,
            },
            false,
        )?;
        Ok(id)
    }

    /// Threads with no entity home — i.e. written before entity scoping existed. Listing
    /// them is safe (an unassigned thread has no other entity to leak into) and necessary:
    /// it is exactly what an operator needs in order to decide.
    pub fn unbound_threads(&self) -> Vec<&Thread> {
        self.threads.iter().filter(|t| !t.is_bound()).collect()
    }

    /// **The one and only exit from `Unbound`, and it is an explicit operator decision.**
    ///
    /// This is the deliberate answer to "existing threads have no `entity_id`". The
    /// alternative — a deterministic migration rule — was rejected because there is no
    /// defensible rule available: nothing in this log records the repository root a thread
    /// was created under, and the only entity-ish value the app has ever persisted is
    /// `config.rs`'s single free-text `company_name` (default `"My Company"`), which is a
    /// display string, not an entity id. Deriving a privacy boundary from it would be a
    /// heuristic on a label, and §22 names cross-entity context as something that must not
    /// be faked.
    ///
    /// So a legacy thread stays quarantined until a human says which entity it is, and
    /// that statement is recorded with its author. The transition is ONE-WAY and
    /// ONCE-ONLY: a second call fails with [`LedgerError::ThreadAlreadyBound`], and a
    /// duplicated adoption event is ignored on replay. Immutability therefore holds from
    /// the moment a thread is bound, by either route.
    ///
    /// What an operator sees today: `unbound_threads()` lists them; any attempt to read or
    /// write one returns [`LedgerError::UnboundThread`], whose message is
    /// *"…it predates entity scoping, and Rich will not guess which entity this work
    /// belongs to. An operator must bind it explicitly."* There is no UI for the choice in
    /// slice 1 — the UI is slice 4 — so today this is a programmatic/back-office call.
    pub fn adopt_unbound_thread(
        &mut self,
        thread_id: &str,
        entity_id: &EntityId,
        adopted_by: &str,
    ) -> Result<ThreadBinding, LedgerError> {
        let thread = self.thread_ref(thread_id)?;
        if let ThreadEntity::Bound { entity_id: home, .. } = &thread.entity {
            return Err(LedgerError::ThreadAlreadyBound {
                thread_id: thread_id.to_string(),
                entity_id: home.to_string(),
            });
        }
        let person_id = PersonId::default_ceo();
        let binding_revision = self.take_revision();
        self.append(
            Event::ThreadEntityBound {
                thread_id: thread_id.to_string(),
                entity_id: entity_id.clone(),
                person_id: person_id.clone(),
                binding_revision,
                adopted_by: adopted_by.to_string(),
                at: now_millis(),
            },
            true, // fsync — a privacy-boundary decision is crash-critical
        )?;
        // The adoption changes a thread's home, which changes the verdict for every turn
        // in it — re-run the guard rather than leaving stale quarantine flags behind.
        self.reconcile_scope();
        Ok(ThreadBinding::new(person_id, entity_id.clone(), thread_id, binding_revision))
    }

    /// THE crash-safety invariant, now SCOPED. Journals + fsyncs the CEO's prompt as
    /// `received` and returns the turn id. Callers MUST call this before handing the
    /// prompt to any compute session.
    ///
    /// Takes a [`ThreadBinding`] rather than a bare `thread_id` so a turn cannot be
    /// written without a scope. The binding is verified against the thread's immutable
    /// home BEFORE anything is persisted (ECS §3.4: *"an old Rich instance is never
    /// allowed to send, dispatch or write into a newly switched entity/thread"*), and the
    /// turn is stamped with that entity and revision.
    ///
    /// Verification runs before the durable write and not after, because ECS §3.4 also
    /// says the store rejects an UNSCOPED event: persisting first and scoping second would
    /// mean the crash window contains exactly the record the model forbids. Nothing is
    /// lost by refusing — the send is blocked with the caller still holding the text,
    /// which is the behavior UX §21 "Entity binding failure" prescribes.
    pub fn record_prompt_received(
        &mut self,
        binding: &ThreadBinding,
        text: &str,
        source: Source,
    ) -> Result<String, LedgerError> {
        self.record_prompt_received_with(binding, text, source, None, None, None, None)
    }

    /// As above for a SPOKEN turn, stamping what the capture path knew about Rich's own
    /// audible window while the audio was being recorded.
    ///
    /// A separate entry point rather than a fifth argument on
    /// [`record_prompt_received`](Self::record_prompt_received), so that a typed turn cannot
    /// acquire a measurement nobody took: every caller of the plain function writes `None`
    /// there BY CONSTRUCTION rather than by remembering to.
    pub fn record_prompt_received_spoken(
        &mut self,
        binding: &ThreadBinding,
        text: &str,
        source: Source,
        rich_audible: bool,
    ) -> Result<String, LedgerError> {
        self.record_prompt_received_with(binding, text, source, None, Some(rich_audible), None, None)
    }

    /// As above, stamping the `steering::IntakeLog` record this turn was drained from
    /// (UX §9.2). Use [`turn_for_intake`](Self::turn_for_intake) before calling it: the
    /// drain is at-least-once and this is the key that makes a replay recognizable.
    pub fn record_prompt_received_from_intake(
        &mut self,
        binding: &ThreadBinding,
        text: &str,
        source: Source,
        intake_id: u64,
    ) -> Result<String, LedgerError> {
        self.record_prompt_received_with(binding, text, source, Some(intake_id), None, None, None)
    }

    /// Preserve desktop capture/button provenance and the shared input order in one write.
    pub(crate) fn record_prompt_received_desktop(&mut self, binding: &ThreadBinding, text: &str,
        source: Source, intake_id: Option<u64>, rich_audible: Option<bool>, pressed: bool,
    ) -> Result<String, LedgerError> {
        self.record_prompt_received_inner(binding, text, source, intake_id, rich_audible, None, None, false, pressed)
    }

    /// **The same record, with the mouth his words came through** (operator back-end spec r3
    /// (s)). Called only by a spine told to keep it
    /// ([`crate::spine::Spine::keep_intake_channel`]), so every product install keeps writing
    /// exactly the lines the three calls above write. It carries the other two stamps too,
    /// because a kept channel must not cost a spoken turn its audibility or an intake turn its
    /// de-duplication key.
    pub fn record_prompt_received_via(
        &mut self,
        binding: &ThreadBinding,
        text: &str,
        source: Source,
        intake_id: Option<u64>,
        rich_audible: Option<bool>,
        channel: &str,
    ) -> Result<String, LedgerError> {
        self.record_prompt_received_with(binding, text, source, intake_id, rich_audible, Some(channel), None)
    }

    /// **The phone's words, carrying the phone's own id for them** (`client_id`, adoption
    /// ledger §2.8 row C5). The same record as
    /// [`record_prompt_received_from_intake`](Self::record_prompt_received_from_intake), or as
    /// [`record_prompt_received_via`](Self::record_prompt_received_via) when the install keeps
    /// the channel, plus the one field that lets the CEO row the Mac sends back name the
    /// message the phone is already showing, and the one that says the words are the
    /// transcript of a voice note (`voice_note`, see `Event::PromptReceived::voice_note`).
    #[allow(clippy::too_many_arguments)]
    pub fn record_prompt_received_from_client(
        &mut self,
        binding: &ThreadBinding,
        text: &str,
        source: Source,
        intake_id: u64,
        channel: Option<&str>,
        client_id: Option<&str>,
        voice_note: bool,
    ) -> Result<String, LedgerError> {
        self.record_prompt_received_inner(binding, text, source, Some(intake_id), None, channel, client_id, voice_note, false)
    }

    /// **Words the app sends Rich because he pressed a button** ("Start the questions"),
    /// recorded so that nothing shows them as his (`Event::PromptReceived::pressed`).
    pub fn record_prompt_received_pressed(
        &mut self,
        binding: &ThreadBinding,
        text: &str,
        source: Source,
    ) -> Result<String, LedgerError> {
        self.record_prompt_received_inner(binding, text, source, None, None, None, None, false, true)
    }

    #[allow(clippy::too_many_arguments)]
    fn record_prompt_received_with(
        &mut self,
        binding: &ThreadBinding,
        text: &str,
        source: Source,
        intake_id: Option<u64>,
        rich_audible: Option<bool>,
        channel: Option<&str>,
        client_id: Option<&str>,
    ) -> Result<String, LedgerError> {
        self.record_prompt_received_inner(binding, text, source, intake_id, rich_audible, channel, client_id, false, false)
    }

    #[allow(clippy::too_many_arguments)]
    fn record_prompt_received_inner(
        &mut self,
        binding: &ThreadBinding,
        text: &str,
        source: Source,
        intake_id: Option<u64>,
        rich_audible: Option<bool>,
        channel: Option<&str>,
        client_id: Option<&str>,
        voice_note: bool,
        pressed: bool,
    ) -> Result<String, LedgerError> {
        self.verify_binding(binding)?;
        let turn_id = new_id("turn");
        self.append(
            Event::PromptReceived {
                turn_id: turn_id.clone(),
                thread_id: binding.thread_id().to_string(),
                text: text.to_string(),
                source,
                at: now_millis(),
                entity_id: Some(binding.entity_id().clone()),
                binding_revision: binding.binding_revision(),
                intake_id,
                rich_audible,
                channel: channel.map(str::to_string),
                client_id: client_id.map(str::to_string),
                voice_note,
                pressed,
            },
            true, // fsync — never lose the CEO's input
        )?;
        Ok(turn_id)
    }

    /// The turn already drained from intake record `intake_id`, if there is one.
    ///
    /// The drain writes the LEDGER first and the drain marker second, deliberately: a
    /// crash between the two must re-present the CEO's words rather than lose them. That
    /// makes the drain at-least-once, and this is how the second attempt recognizes the
    /// first one's work instead of filing the same sentence twice.
    /// A question answer is a durable input with its own replay identity. It is
    /// rendered in its card, so the derived turn id also identifies hidden user bubbles.
    /// `intake_id` is its place in the shared acceptance order, so a Stop covers it like
    /// any other input he sent before pressing it.
    pub fn record_question_answer(&mut self,binding:&ThreadBinding,delivery:&crate::questions::Delivery,intake_id:Option<u64>)->Result<String,LedgerError> {
        self.verify_binding(binding)?;
        let turn_id=format!("{}:{}",if delivery.set_id.is_some(){"question-answer"}else{"question-followup"},delivery.id);
        if self.turn(&turn_id).is_some(){return Ok(turn_id);}
        self.append(Event::PromptReceived {turn_id:turn_id.clone(),thread_id:binding.thread_id().into(),
            text:delivery.text.clone(),source:Source::Text,at:now_millis(),entity_id:Some(binding.entity_id().clone()),
            binding_revision:binding.binding_revision(),intake_id,rich_audible:None,channel:None,client_id:None,voice_note:false,pressed:false},true)?;
        Ok(turn_id)
    }

    pub fn turn_for_intake(&self, intake_id: u64) -> Option<&Turn> {
        self.turns.iter().find(|t| t.intake_id == Some(intake_id))
    }

    pub fn mark_turn_started(&mut self, turn_id: &str, session_id: &str) -> Result<(), LedgerError> {
        self.append(
            Event::TurnStarted { turn_id: turn_id.to_string(), session_id: session_id.to_string(), at: now_millis() },
            false,
        )
    }

    /// Persist one streamed delta AT its position in the shared per-turn sequence.
    ///
    /// `seq` is not invented here: it is the value the lease assigned at its drain point
    /// and handed over on `TurnItem::Text` (§1.4 G1). The spine passes it straight
    /// through, so there is still exactly ONE counter in the system.
    /// Returns the persisted event timestamp for the live event and emission trace.
    pub fn append_assistant_delta(&mut self, turn_id: &str, text: &str, seq: u64) -> Result<u64, LedgerError> {
        let at = now_millis();
        self.append(
            Event::AssistantDelta {
                turn_id: turn_id.to_string(),
                text: text.to_string(),
                at,
                seq: Some(seq),
            },
            false,
        )?;
        Ok(at)
    }

    pub fn complete_turn(&mut self, turn_id: &str, stop_reason: &str) -> Result<(), LedgerError> {
        self.append(
            Event::TurnCompleted { turn_id: turn_id.to_string(), stop_reason: stop_reason.to_string(), at: now_millis() },
            true,
        )
    }

    pub fn interrupt_turn(&mut self, turn_id: &str, reason: &str) -> Result<(), LedgerError> {
        self.append(
            Event::TurnInterrupted { turn_id: turn_id.to_string(), reason: reason.to_string(), at: now_millis(), recovered_after_restart: false },
            true,
        )
    }

    /// Make an orphan request terminal without claiming it ran until this launch.
    pub fn interrupt_turn_after_restart(&mut self, turn_id: &str, reason: &str) -> Result<(), LedgerError> {
        self.append(Event::TurnInterrupted {
            turn_id: turn_id.to_string(), reason: reason.to_string(), at: now_millis(),
            recovered_after_restart: true,
        }, true)
    }

    /// Attach the UPSTREAM classification and the sentences the CEO was shown to a turn
    /// that has already been interrupted (row 3.30).
    ///
    /// **Order matters and the caller owns it:** `interrupt_turn` FIRST, this second. The
    /// interruption is what ended the turn and it must be durable before anything
    /// describing it is; a crash between the two leaves a turn that is correctly
    /// interrupted with no explanation attached, which is the safe direction. The reverse
    /// order would leave an explanation for a turn the log still says is in flight.
    ///
    /// fsync'd like every other terminal write: this is the record of what the CEO was
    /// told, and a statement about a failure that did not survive the failure is worth
    /// nothing.
    pub fn record_upstream_failure(
        &mut self,
        turn_id: &str,
        record: &crate::upstream::UpstreamRecord,
    ) -> Result<(), LedgerError> {
        self.append(
            Event::UpstreamFailure {
                turn_id: turn_id.to_string(),
                record: record.clone(),
                at: now_millis(),
            },
            true,
        )
    }

    /// Attach WHY a turn was interrupted, and the sentences the CEO is about to be shown.
    ///
    /// **Order matters and the caller owns it:** `interrupt_turn` FIRST, this second — the
    /// same contract [`record_upstream_failure`](Self::record_upstream_failure) states, for
    /// the same reason. The interruption is what ended the turn and must be durable before
    /// anything describing it is; a crash between the two leaves a correctly interrupted
    /// turn with no explanation attached, which is the safe direction. The reverse order
    /// would leave an explanation for a turn the log still says is in flight.
    pub fn record_interruption_cause(
        &mut self,
        turn_id: &str,
        record: &crate::interruption::InterruptionRecord,
    ) -> Result<(), LedgerError> {
        self.append(
            Event::TurnInterruptionCause {
                turn_id: turn_id.to_string(),
                record: record.clone(),
                at: now_millis(),
            },
            true,
        )
    }

    /// Record that the CEO stopped this turn (UX §9.3 step 5's evidence).
    ///
    /// `requested_at` must be the timestamp of the DURABLE stop request
    /// (`steering::IntakeLog`), not `now()` — the request is what makes the attribution
    /// true, and it was written to disk before the lease was touched. Callers that do not
    /// hold a real stop request must use [`interrupt_turn`](Self::interrupt_turn): there is
    /// no code path that turns a crash into a CEO stop.
    pub fn stop_turn(&mut self, turn_id: &str, requested_at: u64) -> Result<(), LedgerError> {
        self.stop_delivery_chain(turn_id, requested_at, false)
    }

    /// Honor a durable Stop after a crash without inventing the time it took effect.
    pub fn stop_turn_after_restart(&mut self, turn_id: &str, requested_at: u64) -> Result<(), LedgerError> {
        self.stop_delivery_chain(turn_id, requested_at, true)
    }

    /// Existing replay links identify attempts of the same request, within one scope.
    pub(crate) fn delivery_chain(&self, turn_id: &str) -> Vec<String> {
        let scope = self.turn(turn_id).map(|turn| (turn.thread_id.clone(), turn.entity_id.clone()));
        let mut next = Some(turn_id.to_string());
        let mut chain = Vec::new();
        while let Some(id) = next {
            if chain.contains(&id) { break; }
            next = self.turn(&id).and_then(|turn| turn.superseded_by.clone()).filter(|successor| {
                self.turn(successor).is_some_and(|turn|
                    scope.as_ref() == Some(&(turn.thread_id.clone(), turn.entity_id.clone())))
            });
            chain.push(id);
        }
        chain
    }

    fn stop_delivery_chain(&mut self, turn_id: &str, requested_at: u64, recovered_after_restart: bool) -> Result<(), LedgerError> {
        for id in self.delivery_chain(turn_id) {
            self.append(Event::TurnStopped {
                turn_id: id, requested_at, at: now_millis(), recovered_after_restart,
            }, true)?;
        }
        Ok(())
    }

    /// Claim a CEO-FACING, turn-scoped action BEFORE executing it (claim-then-execute,
    /// §6.4). Returns the action id; settle it later with `update_action`.
    pub fn record_action(&mut self, turn_id: &str, kind: &str, detail: &str) -> Result<String, LedgerError> {
        self.record_action_with(Some(turn_id), kind, detail, ActionVisibility::CeoFacing, ActionStatus::Claimed)
    }

    /// The general form. `turn_id: None` records a turn-BOUNDARY action (rotation,
    /// re-prime injection, crash recovery) which by construction belongs to no turn.
    /// `status` lets an atomically-already-done action be recorded in ONE event rather
    /// than a meaningless claim/complete pair; for a genuinely two-phase action (spawn
    /// a child, THEN swap it in) pass `Claimed` and follow with `update_action`.
    ///
    /// `detail` is truncated to `ACTION_DETAIL_MAX_CHARS` on a CHAR boundary (never a
    /// byte boundary — splitting a multi-byte codepoint would panic): the CEO-facing
    /// digest is re-injected verbatim on every rotation and is billed per rotation under
    /// BYO-Anthropic, so it is budgeted, not dumped (continuity §2.1).
    pub fn record_action_with(
        &mut self,
        turn_id: Option<&str>,
        kind: &str,
        detail: &str,
        visibility: ActionVisibility,
        status: ActionStatus,
    ) -> Result<String, LedgerError> {
        let action_id = new_id("act");
        self.append(
            Event::ActionRecorded {
                action_id: action_id.clone(),
                turn_id: turn_id.map(|t| t.to_string()),
                kind: kind.to_string(),
                detail: truncate_detail(detail),
                status,
                visibility,
                at: now_millis(),
            },
            true,
        )?;
        Ok(action_id)
    }

    pub fn update_action(&mut self, action_id: &str, status: ActionStatus) -> Result<(), LedgerError> {
        self.append(Event::ActionUpdated { action_id: action_id.to_string(), status, at: now_millis() }, true)
    }











    /// Persist both sides of a continuation in one fsynced event. A crash cannot
    /// leave a second input without its supersession, or retire the only saved input.
    pub fn replay_turn(&mut self, binding: &ThreadBinding, turn_id: &str) -> Result<String, LedgerError> {
        self.verify_binding(binding)?;
        let original = self.thread_turns(binding.thread_id())?.into_iter()
            .find(|turn| turn.id == turn_id).ok_or_else(|| LedgerError::UnknownTurn(turn_id.into()))?;
        if let Some(id) = &original.superseded_by { return Ok(id.clone()); }
        let by_turn_id = new_id("turn");
        self.append(Event::TurnReplayed { turn_id: turn_id.into(), by_turn_id: by_turn_id.clone(), at: now_millis() }, true)?;
        Ok(by_turn_id)
    }



    // ---- read / projection API --------------------------------------------

    pub fn threads(&self) -> &[Thread] {
        &self.threads
    }

    /// The RAW, UNSCOPED turn log — the audit view, including quarantined turns. Every
    /// CEO-facing projection must go through [`Ledger::thread_turns`] instead.
    pub fn turns(&self) -> &[Turn] {
        &self.turns
    }

    /// The SCOPED turn view for one thread: the guard that makes "no event from one
    /// entity may ever render in another entity's thread" true.
    ///
    /// Fails closed for an unbound thread, and excludes every quarantined turn — the ones
    /// whose entity stamp contradicts this thread's immutable home (see
    /// [`Ledger::reconcile_scope`]). This is the single chokepoint that `messages()` and
    /// the re-prime payload both read through, so there is one place the check can be
    /// removed and one place it has to hold.
    pub fn thread_turns(&self, thread_id: &str) -> Result<Vec<&Turn>, LedgerError> {
        let binding = self.thread_binding(thread_id)?;
        Ok(self
            .turns
            .iter()
            .filter(|t| {
                t.thread_id == thread_id
                    // THE CROSS-ENTITY GUARD. Removing either clause makes
                    // `no_event_from_one_entity_renders_in_another_entitys_thread` fail.
                    && !t.quarantined
                    && t.entity_id.as_ref().is_none_or(|e| e == binding.entity_id())
            })
            .collect())
    }

    /// Same view, but verifying a binding the caller already holds first (so a caller
    /// operating under a stale or foreign context is refused rather than served).
    pub fn thread_turns_scoped(&self, binding: &ThreadBinding) -> Result<Vec<&Turn>, LedgerError> {
        self.verify_binding(binding)?;
        self.thread_turns(binding.thread_id())
    }

    pub fn actions(&self) -> &[Action] {
        &self.actions
    }

    pub fn turn(&self, id: &str) -> Option<&Turn> {
        self.turns.iter().find(|t| t.id == id)
    }

    /// The CLEAN-OUTPUT rendered view for a thread: user prompts + assistant replies,
    /// in order. `Internal` (re-prime / proactive) turns are structurally excluded —
    /// they have no render path at all.
    ///
    /// Now fallible, because retrieval REQUIRES a scope (ECS §3.3). An unbound thread
    /// returns [`LedgerError::UnboundThread`] rather than an empty list: "I will not serve
    /// this" and "there is nothing here" are different statements and the UI must not
    /// render the second when the first is true.
    pub fn messages(&self, thread_id: &str) -> Result<Vec<Message>, LedgerError> {
        let scoped = self.thread_turns(thread_id)?;
        let mut out = Vec::new();
        // Superseded turns (mid-turn-crash replay, §5.3) are excluded — the CEO sees the
        // ONE clean exchange (the successful replay), never a duplicated user line.
        for t in scoped.into_iter().filter(|t| t.source != Source::Internal && t.superseded_by.is_none()) {
            if t.source == Source::Proactive {
                // Tier 3 (Silent) never appears in the conversation (UX §5.1) — it has no
                // render path here at all, matching Internal's treatment above.
                if t.tier != Some(AttentionTier::Silent) {
                    out.push(Message {
                        role: "assistant".into(),
                        text: t.assistant_text.clone(),
                        turn_id: t.id.clone(),
                        at: t.created_at,
                    });
                }
                continue;
            }
            // Words the app sent for a button he pressed are not his line (finding 14).
            if !t.pressed && t.source != Source::Agent {
                out.push(Message { role: "user".into(), text: t.user_text.clone(), turn_id: t.id.clone(), at: t.created_at });
            }
            if !t.assistant_text.is_empty() {
                out.push(Message { role: "assistant".into(), text: t.assistant_text.clone(), turn_id: t.id.clone(), at: t.created_at });
            }
        }
        Ok(out)
    }

    /// The same clean-output view, but verifying a binding the caller already holds.
    pub fn messages_scoped(&self, binding: &ThreadBinding) -> Result<Vec<Message>, LedgerError> {
        self.verify_binding(binding)?;
        self.messages(binding.thread_id())
    }

    /// Turns not yet terminal — used by crash recovery to find work to resume.
    /// Quarantined turns are excluded: a turn that failed the scope check must not be
    /// resumed into any entity.
    pub fn pending_turns(&self) -> Vec<&Turn> {
        self.turns
            .iter()
            .filter(|t| !t.quarantined && t.superseded_by.is_none() && matches!(t.state, TurnState::Received | TurnState::InFlight))
            .collect()
    }

    /// Open (claimed, not-yet-terminal) actions — the anti-double-execution guard set.
    pub fn open_actions(&self) -> Vec<&Action> {
        self.actions.iter().filter(|a| a.status == ActionStatus::Claimed).collect()
    }

    /// CEO-facing actions only — the subset the re-prime digest is allowed to assert as
    /// "what Rich has done" (see `ActionVisibility`). UNSCOPED: the audit view.
    pub fn ceo_facing_actions(&self) -> Vec<&Action> {
        self.actions.iter().filter(|a| a.visibility == ActionVisibility::CeoFacing).collect()
    }

    /// The entity an action belongs to, derived through its turn's thread. `None` for a
    /// turn-BOUNDARY action (`turn_id: None` — rotation, re-prime, crash recovery), for
    /// an action on a quarantined turn, and for one whose thread is unbound.
    pub fn action_entity(&self, action: &Action) -> Option<&EntityId> {
        let turn_id = action.turn_id.as_deref()?;
        let turn = self.turns.iter().find(|t| t.id == turn_id)?;
        if turn.quarantined {
            return None;
        }
        self.threads.iter().find(|t| t.id == turn.thread_id)?.entity_id()
    }

    /// CEO-facing actions belonging to ONE entity — what the re-prime digest for a thread
    /// in that entity is allowed to contain.
    ///
    /// This closes a real leak that predates entity binding: the digest was assembled from
    /// `ceo_facing_actions()` across the WHOLE ledger, so every rotation injected every
    /// entity's actions into every entity's session as *"ground truth for what Rich has
    /// done — authoritative"*. That is cross-entity context, which §22 says must not be
    /// faked, and §3.5 forbids it explicitly ("A write has one home entity").
    ///
    /// Scoping by ENTITY, not by thread, is deliberate: threads are views over one shared
    /// substrate WITHIN an entity, so this is strictly narrower than the old behavior
    /// only across entities. Nothing that used to be visible inside an entity disappears,
    /// so no new false-denial risk is introduced.
    ///
    /// Turn-boundary actions are all `Internal` today and were never in the digest anyway.
    /// A CEO-facing action that cannot be resolved to this entity is EXCLUDED here and
    /// remains available unscoped through `ceo_facing_actions()` for audit — it is not
    /// deleted, just not asserted into a session that has no claim to it.
    pub fn ceo_facing_actions_for_entity(&self, entity_id: &EntityId) -> Vec<&Action> {
        self.actions
            .iter()
            .filter(|a| a.visibility == ActionVisibility::CeoFacing)
            .filter(|a| self.action_entity(a) == Some(entity_id))
            .collect()
    }

    /// Internal (machinery) actions only — the durable audit trail for rotation,
    /// re-prime injection and crash recovery. Never injected into a priming prompt.
    pub fn internal_actions(&self) -> Vec<&Action> {
        self.actions.iter().filter(|a| a.visibility == ActionVisibility::Internal).collect()
    }
}

/// Per-entry cap on `Action.detail`. The CEO-facing digest is re-injected VERBATIM into
/// every successor's priming prompt, so each entry is bounded. The NUMBER of entries is
/// deliberately NOT capped: silently dropping a recorded action to save tokens would
/// reintroduce exactly the false-DENIAL failure this ledger exists to prevent.
pub const ACTION_DETAIL_MAX_CHARS: usize = 160;

/// Char-boundary-safe truncation. `&detail[..160]` would panic mid-codepoint on any
/// non-ASCII input (a CEO's em-dash or accented company name is enough).
fn truncate_detail(detail: &str) -> String {
    if detail.chars().count() <= ACTION_DETAIL_MAX_CHARS {
        return detail.to_string();
    }
    let mut out: String = detail.chars().take(ACTION_DETAIL_MAX_CHARS).collect();
    out.push('\u{2026}');
    out
}

impl Ledger {
    pub fn record_proactive_message(
        &mut self,
        binding: &ThreadBinding,
        tier: AttentionTier,
        text: &str,
    ) -> Result<String, LedgerError> {
        self.record_proactive_message_as(binding, tier, text, new_id("turn"))
    }

    pub(crate) fn record_proactive_message_as(
        &mut self, binding: &ThreadBinding, tier: AttentionTier, text: &str, turn_id: String,
    ) -> Result<String, LedgerError> {
        // Rich speaking unprompted is still a scoped write: an unbound or mis-scoped
        // thread must not receive one either.
        self.verify_binding(binding)?;
        if self.turn(&turn_id).is_some() { return Ok(turn_id); }
        self.append(
            Event::ProactiveMessage {
                turn_id: turn_id.clone(),
                thread_id: binding.thread_id().to_string(),
                tier,
                text: text.to_string(),
                at: now_millis(),
                entity_id: Some(binding.entity_id().clone()),
                binding_revision: binding.binding_revision(),
            },
            true,
        )?;
        Ok(turn_id)
    }

}

impl Ledger {
    pub fn mark_turn_superseded(&mut self, turn_id: &str, by_turn_id: &str) -> Result<(), LedgerError> {
        self.append(
            Event::TurnSuperseded { turn_id: turn_id.to_string(), by_turn_id: by_turn_id.to_string(), at: now_millis() },
            false,
        )
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A RESERVED ID IS NOT A RECORD** — UX §3.3, *"no pre-created thread record until the CEO
    /// sends the first message"*. Reserving one appends nothing, lists nothing and survives
    /// nothing; spending it produces exactly the record `create_thread` would have produced; and
    /// spending it twice is refused rather than duplicated, because two conversations under one
    /// id would be two conversations in one workspace partition
    /// (`EngineProfile::workspace_state` hashes `(entity, thread)`).
    #[test]
    fn a_reserved_thread_id_writes_nothing_until_it_is_spent_and_is_spent_only_once() {
        let path = std::env::temp_dir().join(crate::util::new_id("ledger-reserved"));
        let entity = EntityId::parse("acme").unwrap();
        let ledger = Ledger::open(&path).unwrap();

        let reserved = Ledger::reserve_thread_id();
        assert!(reserved.starts_with("thr_"));
        assert_eq!(ledger.threads().len(), 0, "reserving appended nothing");
        assert!(ledger.thread_binding(&reserved).is_err(), "and it is not a thread");

        // The app is quit before he types: nothing about the reservation is on disk.
        drop(ledger);
        let mut ledger = Ledger::open(&path).unwrap();
        assert_eq!(ledger.threads().len(), 0);

        // He sends. The id is spent, and the record is the ordinary one.
        let id = ledger.create_thread_with_reserved_id(&reserved, "Pricing", &entity).unwrap();
        assert_eq!(id, reserved);
        let binding = ledger.thread_binding(&id).unwrap();
        assert_eq!(binding.entity_id(), &entity);
        assert_eq!(ledger.threads().len(), 1);

        let again = ledger.create_thread_with_reserved_id(&reserved, "Pricing again", &entity);
        assert!(matches!(again, Err(LedgerError::ThreadIdAlreadySpent(_))), "{again:?}");
        assert_eq!(ledger.threads().len(), 1, "the refused call appended nothing");
        std::fs::remove_file(path).unwrap();
    }

    /// **THE MOUTH HIS WORDS CAME THROUGH, KEPT ON THE PROMPT RECORD** (operator back-end spec
    /// r3 (s): *"the spine keeps the intake's `channel` on the prompt record, a new optional
    /// field the product path does not read"*). A turn recorded with a channel keeps it across
    /// a restart, with its intake id beside it; every turn recorded without one writes no
    /// `channel` key at all, so the lines a product install writes are what they were.
    #[test]
    fn a_turn_keeps_the_channel_it_was_recorded_with_and_one_without_writes_no_channel_key() {
        let path = std::env::temp_dir().join(crate::util::new_id("ledger-channel"));
        let entity = EntityId::parse("acme").unwrap();
        let mut ledger = Ledger::open(&path).unwrap();
        let thread = ledger.create_thread("Pricing", &entity).unwrap();
        let binding = ledger.thread_binding(&thread).unwrap();
        let phone = ledger.record_prompt_received_via(&binding, "from the couch", Source::Text, Some(7), None, "phone").unwrap();
        let spoken = ledger.record_prompt_received_via(&binding, "at the desk, aloud", Source::Jam, None, Some(false), "desk").unwrap();
        let typed = ledger.record_prompt_received(&binding, "at the desk", Source::Text).unwrap();
        let drained = ledger.record_prompt_received_from_intake(&binding, "no mouth kept", Source::Text, 8).unwrap();
        drop(ledger);
        let ledger = Ledger::open(&path).unwrap();
        let turn = |id: &str| ledger.turn(id).unwrap().clone();
        assert_eq!(turn(&phone).channel.as_deref(), Some("phone"));
        assert_eq!(turn(&phone).intake_id, Some(7), "the de-duplication key rides beside the channel");
        assert_eq!(turn(&spoken).channel.as_deref(), Some("desk"));
        assert_eq!(turn(&spoken).rich_audible, Some(false), "a spoken turn keeps its audibility too");
        assert_eq!(turn(&typed).channel, None, "not recorded is None, never a guessed mouth");
        assert_eq!(turn(&drained).channel, None);
        let text = std::fs::read_to_string(&path).unwrap();
        let prompts: Vec<&str> = text.lines().filter(|l| l.contains("\"PromptReceived\"")).collect();
        assert_eq!(prompts.len(), 4);
        assert_eq!(prompts.iter().filter(|l| l.contains("\"channel\"")).count(), 2, "{text}");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn accepted_prompt_survives_restart_after_a_torn_tail() {
        let path = std::env::temp_dir().join(crate::util::new_id("ledger-tail"));
        let entity = EntityId::parse("acme").unwrap();
        let mut ledger = Ledger::open(&path).unwrap();
        let thread = ledger.create_thread("original", &entity).unwrap();
        drop(ledger);
        OpenOptions::new().append(true).open(&path).unwrap()
            .write_all(b"{\"event\":\"interrupted").unwrap();
        let mut ledger = Ledger::open(&path).unwrap();
        let binding = ledger.thread_binding(&thread).unwrap();
        let turn = ledger.record_prompt_received(&binding, "after recovery", Source::Text).unwrap();
        drop(ledger);
        let mut ledger = Ledger::open(&path).unwrap();
        assert!(ledger.turn(&turn).is_some());
        assert_eq!(ledger.skipped_records().len(), 1);
        assert!(std::fs::read_to_string(&path).unwrap().contains("interrupted\n"));
        std::fs::remove_file(path).unwrap();
    }


    fn runs(input: &[(Option<u64>, &str)]) -> Vec<TextRun> {
        let mut out = Vec::new();
        for (seq, text) in input {
            Ledger::push_text_run(&mut out, *seq, text, 1_700_000_000_000);
        }
        out
    }

    #[test]
    fn consecutive_deltas_fold_into_one_run() {
        let r = runs(&[(Some(0), "he said "), (Some(1), "X")]);
        assert_eq!(r.len(), 1, "no gap in the shared counter ⇒ one uninterrupted stretch of prose");
        assert_eq!(r[0].text, "he said X");
        assert_eq!((r[0].start_seq, r[0].end_seq), (Some(0), Some(1)));
    }

    #[test]
    fn a_gap_in_the_shared_sequence_splits_the_run() {
        // "he said X, then ran Y, then said Z" — seq 2 is the tool call, and it is the
        // ONLY evidence that the prose was interrupted. Machinery consumed position 2
        // (§1.4 G1); the text jumps 1 -> 3.
        let r = runs(&[(Some(0), "he said "), (Some(1), "X"), (Some(3), "then said Z")]);
        assert_eq!(r.len(), 2, "the gap at seq 2 is a boundary, not noise");
        assert_eq!(r[0].text, "he said X");
        assert_eq!(r[1].text, "then said Z");
        assert_eq!(r[1].start_seq, Some(3));
        // The concatenation the ledger has always held is unchanged and still complete —
        // runs ADD structure, they never subtract text.
        let joined: String = r.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(joined, "he said Xthen said Z");
    }

    #[test]
    fn legacy_deltas_with_no_seq_stay_one_run_with_an_unknown_position() {
        // Every delta written before this field existed. The text is intact; the position
        // is reported as unknown rather than back-filled with a plausible-looking 0.
        let r = runs(&[(None, "old "), (None, "reply")]);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].text, "old reply");
        assert_eq!((r[0].start_seq, r[0].end_seq), (None, None));
    }

    #[test]
    fn a_positioned_delta_never_merges_with_an_unpositioned_one() {
        // The mixed case a live upgrade actually produces: a turn that began streaming
        // under the old build and continued under the new one. Merging would assert an
        // adjacency nothing recorded.
        let r = runs(&[(None, "old"), (Some(4), "new")]);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].start_seq, None);
        assert_eq!(r[1].start_seq, Some(4));
    }

    #[test]
    fn a_legacy_assistant_delta_line_still_replays_and_carries_no_position() {
        // Byte-for-byte, a line written before `seq` existed (the shape asserted in
        // tests/entity_binding_tests.rs:76).
        let line = r#"{"event":"AssistantDelta","turn_id":"t","text":"hi","at":6}"#;
        let event: Event = serde_json::from_str(line).expect("legacy line still parses");
        match event {
            Event::AssistantDelta { seq, text, .. } => {
                assert_eq!(seq, None, "absent ⇒ unknown position, not position 0");
                assert_eq!(text, "hi");
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn a_turns_active_span_is_measured_and_refuses_to_be_invented() {
        let mut turn = Turn {
            id: "t".into(),
            thread_id: "thr".into(),
            entity_id: None,
            binding_revision: 0,
            quarantined: false,
            user_text: String::new(),
            source: Source::Text,
            state: TurnState::InFlight,
            session_id: None,
            started_sessions: Vec::new(),
            assistant_text: String::new(),
            text_runs: Vec::new(),
            stop_reason: None,
            created_at: 1_000,
            started_at: Some(1_000),
            ended_at: None,
            tier: None,
            superseded_by: None,
            stop_requested_at: None,
            intake_id: None,
            upstream_failure: None,
            interruption: None,
            rich_audible: None,
            channel: None,
            client_id: None,
            voice_note: false,
            pressed: false,
        };
        // IN FLIGHT: unknown, never `now() - started_at` (UX §6.3's twelve-hour trap).
        assert_eq!(turn.active_ms(), None);

        // COMPLETED: an actual measurement. 1_000 -> 19_360 millis = 18.36s, which §6.2
        // renders as `18s` (the display rounding is the renderer's, not the record's).
        turn.ended_at = Some(19_360);
        turn.state = TurnState::Completed;
        assert_eq!(turn.active_ms(), Some(18_360));

        // A CLOCK THAT WENT BACKWARDS between the two events: unknown, not a u64 that
        // wrapped to ~584 million years.
        turn.ended_at = Some(999);
        assert_eq!(turn.active_ms(), None);

        // NEVER STARTED (journaled, then the process died before delivery): unknown.
        turn.started_at = None;
        turn.ended_at = Some(19_360);
        assert_eq!(turn.active_ms(), None);
    }
}

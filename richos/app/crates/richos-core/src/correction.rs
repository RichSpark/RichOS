//! THE CORRECTION DESK — "read what the system believes and correct it", made reachable.
//!
//! `loro-structure.md` calls this the property that must not be lost:
//!
//! > The wiki's essential property is not its content. It is that **a human can read what
//! > the system believes and correct it when it is wrong.**
//!
//! Loro's writer has been complete and demonstrable since 2026-08-29 and reachable only
//! through a CLI — `loro-writer.md` says so in its own words: *"Rich does not call the
//! writer yet… the writer's reachable surface is the CLI."* A capability the CEO can only
//! reach by opening a terminal is not a property of the product he uses. This module is the
//! loop: **propose → show him exactly what would change → he says yes → write.**
//!
//! # Ask, never infer — enforced by the type, not by a paragraph
//!
//! `ceo-decisions.md` §7 governs the write path absolutely: *"Inference cannot tell 'ship
//! Thursday' → 'ship Friday' (a change of mind) from 'deep gram' → 'Deepgram' (a real
//! correction). Asking removes the class of error entirely… **Nothing is ever learned
//! silently.**"* ECS §5.1 draws the same line: a correction is a human statement, an
//! inference is at most a candidate.
//!
//! So there is no function here that writes loro. There is [`CorrectionDesk::propose`],
//! which runs the writer with `--dry-run` and stores what WOULD be written; and there is
//! [`CorrectionDesk::confirm`], which is the only path to a write and refuses anything that
//! is not a proposal the CEO has in front of him. A caller cannot skip the ask, because
//! there is no argument it could pass to do so.
//!
//! §7's three outcomes are the state machine, verbatim: confirm (written), decline (not
//! written, **and still offered again** — a decline is ambiguous: not a record / not now /
//! misclicked), decline permanently (suppressed, and the suppression is inspectable, or a
//! record silently refuses to be correctable with no way to see why).
//!
//! # Durability
//!
//! Same posture as `steering::IntakeLog`, for the same reason: a proposal the CEO has not
//! answered yet must survive a crash, a rotation and a relaunch, or "he confirms after
//! lunch" silently loses the correction. Append-only JSONL, `sync_all` on every record, a
//! record this build cannot read skipped rather than fatal — and, since 2026-09-05,
//! **counted, classified and reported** rather than dropped in silence.
//!
//! ## What the desk's replay is, and why a dropped record is not like a dropped message
//!
//! This file is not a list of proposals. It is an EVENT LOG, and the projection is built by
//! folding each record onto the ones before it: a `Proposed` record carries the whole
//! proposal in state `AwaitingCeo`, and the CEO's ANSWER is a separate, later record
//! (`Written`, `Failed`, `Declined`, `Suppressed`). So the two ends of the file fail in
//! opposite directions, and only one of them is dangerous:
//!
//! - **Drop the earlier `Proposed` record** and the answer records that follow it find no
//!   proposal to fold onto and are silent no-ops. The proposal disappears. That is a loss,
//!   and nothing WRONG is put in front of the CEO.
//! - **Drop the later answer record** and the proposal replays in its original state —
//!   `AwaitingCeo` — which is to say **a decision he confirmed silently reverts to pending**.
//!   `pending_for` then offers it again, and `confirm` accepts it again because its state
//!   passes the check, so the write can run a second time. Being asked twice is how a person
//!   learns the machine does not remember him.
//!
//! So the reader never presents a state it cannot vouch for AS IF it could. A proposal whose
//! answer could have been among the records this build could not read goes one of two ways,
//! and which one is decided by whether the missing answer can ever come back:
//!
//! - **Written by a NEWER RichOS** — the record is intact and updating reads it. The
//!   proposal is [`ProposalState::Unresolved`]: held back, counted, inspectable, and settled
//!   by the update, so nothing is asked in the meantime.
//! - **Damaged, or unreadable for good** — the bytes will never be readable, so a hold would
//!   be a hold forever. **The CEO ruled on exactly this (2026-09-28, `ceo-decisions.md` §96,
//!   escalation `esc-20260905T124300Z-3ce871e9`).** Asked *"If the file holding your past
//!   answers gets corrupted, RichOS can't tell whether you already answered. Should it ask
//!   you again, or stay silent about that one?"*, he answered *"Ask me again"*. So the
//!   proposal is an ordinary question again — in `pending_for`, answerable by `confirm` and
//!   `decline` — carrying [`Proposal::asked_again`], the plain sentence saying RichOS could
//!   not read his earlier answer. The cost he accepted is that he may see one he already
//!   answered; the sentence is how he knows that is what he is looking at.
//!
//! A correction he DEMONSTRABLY answered is never asked again: an answer that loaded settles
//! it, a `confirmed` record that loaded holds it (the write may have landed), and a record he
//! said never to ask about stays unasked. See [`CorrectionDesk::unresolved`] and
//! [`CorrectionDesk::asked_again`].
//!
//! # What this deliberately does NOT do
//!
//! - **It never widens a scope.** `loro-writer.md`: widening `ceo-private` →
//!   `org-shared`/`external` is refused without `--widen-scope`, *"because that is the one
//!   edit that turns the CEO's private view into something every worker sees, and it must
//!   be a decision rather than the side effect of fixing a typo."* The flag is not reachable
//!   from this module. Narrowing needs no ceremony and is available.
//! - **It never edits prose.** A `wiki:` ref is refused by the writer itself (exit 5) with
//!   the file path to open, and that refusal is surfaced rather than worked around: *"a
//!   machine rewriting the CEO's synthesis is not a correction, it is a substitution."*
//! - **It does not detect corrections.** Nothing here watches for "that's wrong" and files
//!   a proposal on its own. Something has to propose — Rich, in conversation, or the CEO
//!   directly — and that trigger is named as unbuilt rather than faked.

use crate::loro::{CorpusPaths, LoroInstall, LoroRoot, LoroTools};
use crate::skip::{SkipDialect, SkipKind, SkippedRecord};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Debug, thiserror::Error)]
pub enum CorrectionError {
    #[error("correction desk io: {0}")]
    Io(String),
    #[error("no proposal {0}")]
    NoSuchProposal(String),
    #[error("proposal {id} is {state:?}, not awaiting the CEO — it cannot be confirmed twice")]
    NotAwaiting { id: String, state: ProposalState },
    #[error("proposal {id} belongs to entity {owner:?}, not {asked:?}")]
    WrongEntity { id: String, owner: String, asked: String },
    #[error("a correction needs the CEO's own words for what was wrong (--why)")]
    NoReason,
    #[error("proposal {id} was confirmed but its writer outcome is unknown; reconcile the existing record before trying another write")]
    WriteOutcomeUnknown { id: String },
    #[error("{0:?} was permanently declined for correction — clear the suppression to propose again")]
    Suppressed(String),
    #[error(
        "proposal {id} cannot be answered: the record of what you already decided about it \
         could not be read, so this version does not know whether you have answered it \
         already"
    )]
    AnswerUnreadable { id: String },
    #[error(
        "{record_ref:?} is on hold: a decision you made about it could not be read, so this \
         version does not know whether you asked never to be asked about it again — clear the \
         hold to propose against it"
    )]
    HeldRecord { record_ref: String },
    #[error("the loro writer refused (exit {code}): {message}")]
    WriterRefused { code: i32, message: String },
    #[error("the loro writer could not be run: {0}")]
    WriterUnavailable(String),
}

impl From<std::io::Error> for CorrectionError {
    fn from(e: std::io::Error) -> Self {
        CorrectionError::Io(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// what can be proposed
// ---------------------------------------------------------------------------

/// The three writer operations the app exposes, and no fourth.
///
/// `append` records a belief, `correct` fixes metadata in place, `supersede` replaces a
/// belief that is wrong. `create-company` is deliberately absent: creating a partition is a
/// structural decision about the corpus layout, which is exactly what CEO decision 1.6
/// ("one loro, two homes") is still open on — see `crate::loro::LaneMap`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
// `rename_all` names the VARIANTS (the `op` tag); `rename_all_fields` names the fields.
// Both matter because this type crosses the Tauri IPC boundary verbatim — the webview
// sends `{op: "supersede", recordRef: …}` and the rest of that surface is camelCase.
#[serde(tag = "op", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum ProposedWrite {
    /// A new record. `loro-writer.md`: append REFUSES to overwrite — a belief is superseded,
    /// never silently replaced.
    Append {
        id: String,
        kind: String,
        /// Omitted resolves to `ceo-private` on the way IN (`record.js` invariant 1), not
        /// merely filtered as private on the way out. Ambiguity resolves to the more
        /// private reading, always.
        scope: Option<String>,
        title: Option<String>,
        body: String,
        /// `ceo` (default), `unfiled`, or a company id. Filing never blocks a write.
        partition: Option<String>,
    },
    /// Metadata fixed in place. The BODY is never machine-rewritten unless asked.
    Correct {
        record_ref: String,
        title: Option<String>,
        kind: Option<String>,
        confidence: Option<f64>,
        tags: Option<Vec<String>>,
        /// Only ever NARROWING. Widening is unreachable from here on purpose (module doc).
        narrow_scope_to: Option<String>,
        body: Option<String>,
    },
    /// The belief is wrong. The replacement is written, the old record points at it, and
    /// **nothing is deleted** — it stops being current and stays fetchable by ref.
    Supersede { record_ref: String, new_id: String, kind: String, scope: Option<String>, body: String },
}

impl ProposedWrite {
    /// The record this write is ABOUT, when there is one. `None` for an append — there is
    /// no prior belief to suppress or to have declined.
    pub fn target_ref(&self) -> Option<&str> {
        match self {
            ProposedWrite::Append { .. } => None,
            ProposedWrite::Correct { record_ref, .. } | ProposedWrite::Supersede { record_ref, .. } => {
                Some(record_ref)
            }
        }
    }

    pub fn verb(&self) -> &'static str {
        match self {
            ProposedWrite::Append { .. } => "append",
            ProposedWrite::Correct { .. } => "correct",
            ProposedWrite::Supersede { .. } => "supersede",
        }
    }

    /// The argv after the verb, and the body to send on stdin. `--body-stdin` throughout,
    /// for the same reason the compiler takes `--topic-stdin`: a record body is multi-line
    /// prose and must not go through shell quoting.
    fn args(&self) -> (Vec<String>, Option<String>) {
        let mut a: Vec<String> = Vec::new();
        let mut push = |k: &str, v: &str| {
            a.push(k.into());
            a.push(v.into());
        };
        match self {
            ProposedWrite::Append { id, kind, scope, title, body, partition } => {
                push("--id", id);
                push("--kind", kind);
                if let Some(s) = scope {
                    push("--scope", s);
                }
                if let Some(t) = title {
                    push("--title", t);
                }
                if let Some(p) = partition {
                    push("--partition", p);
                }
                a.push("--body-stdin".into());
                (a, Some(body.clone()))
            }
            ProposedWrite::Correct { record_ref, title, kind, confidence, tags, narrow_scope_to, body } => {
                push("--ref", record_ref);
                if let Some(t) = title {
                    push("--title", t);
                }
                if let Some(k) = kind {
                    push("--kind", k);
                }
                if let Some(c) = confidence {
                    push("--confidence", &c.to_string());
                }
                if let Some(t) = tags {
                    push("--tags", &t.join(","));
                }
                if let Some(s) = narrow_scope_to {
                    push("--scope", s);
                }
                match body {
                    Some(b) => {
                        a.push("--body-stdin".into());
                        (a, Some(b.clone()))
                    }
                    None => (a, None),
                }
            }
            ProposedWrite::Supersede { record_ref, new_id, kind, scope, body } => {
                push("--ref", record_ref);
                push("--id", new_id);
                push("--kind", kind);
                if let Some(s) = scope {
                    push("--scope", s);
                }
                a.push("--body-stdin".into());
                (a, Some(body.clone()))
            }
        }
    }
}

/// One `--json` object back from `loro-write`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteOutput {
    #[serde(default)]
    pub op: String,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub r#ref: String,
    #[serde(default)]
    pub superseded_ref: Option<String>,
    #[serde(default)]
    pub file: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub changed: Vec<String>,
}

/// The write side of loro, behind a trait so `cargo test -p richos-core` stays hermetic:
/// richos ships no `loro/` directory and no corpus, so a test that shelled out to the real
/// writer could only run on one machine.
pub trait LoroWriteBackend: Send {
    /// `--dry-run`: return exactly what WOULD be written. Touches nothing.
    fn preview(&self, write: &ProposedWrite, why: &str) -> Result<WriteOutput, CorrectionError>;
    /// The real write. **Only [`CorrectionDesk::confirm`] calls this.**
    fn commit(&self, write: &ProposedWrite, why: &str) -> Result<WriteOutput, CorrectionError>;
    /// The file behind a ref — the answer to "what does loro actually believe?". Read-only.
    fn show(&self, record_ref: &str) -> Result<WriteOutput, CorrectionError>;
}

/// The shipped backend: `node loro/bin/loro-write.mjs … --json`.
#[derive(Debug)]
pub struct CliLoroWriter {
    tools: LoroTools,
    root: LoroRoot,
}

impl CliLoroWriter {
    pub fn new(tools: LoroTools, root: LoroRoot) -> Self {
        CliLoroWriter { tools, root }
    }

    /// WHICH CORPUS THIS WRITER WRITES TO. Public because the one property that matters is
    /// that it is the SAME corpus the reader compiles from, and a property nobody can
    /// observe is a property nobody can test.
    pub fn root(&self) -> &LoroRoot {
        &self.root
    }

    /// The loro checkout supplying `bin/loro-write.mjs`, and the `node` that runs it.
    pub fn tools(&self) -> &LoroTools {
        &self.tools
    }

    /// The WRITE half of a resolved [`LoroInstall`] — the same value the read half is built
    /// from, so the two cannot be looking at different corpora.
    ///
    /// # What used to be here, and why it is gone
    ///
    /// `CliLoroWriter::from_env()`, which read `LORO_CORPUS` / `LORO_ROOT` /
    /// `RICHOS_LORO_DIR` and NOTHING ELSE. That is a `cargo run` assumption: a
    /// double-clicked `.app` is handed launchd's environment — measured by `ps eww` on a
    /// real Finder launch as `HOME`, `USER` and `PATH=/usr/bin:/bin:/usr/sbin:/sbin`, and
    /// nothing more — so on the CEO's installed app the correction desk was `None` on every
    /// boot. He could be shown a proposal and confirm it, and the confirmation reached a
    /// writer that could not find the corpus.
    ///
    /// The read path was given a resolver at `46d8f56`; this file, one directory over, never
    /// got the twin. It is not given one now either: **a twin would be the third
    /// near-identical candidate walk**, and the reason this defect existed is that the second
    /// one was written without the third being noticed. There is one walk
    /// ([`LoroInstall::locate`]) and two constructors that consume its result.
    ///
    /// The env-only constructor is REMOVED rather than deprecated, because the next person
    /// wiring a writer must not have it to reach for.
    pub fn from_install(install: &LoroInstall) -> Self {
        CliLoroWriter::new(install.tools().clone(), install.root().clone())
    }

    /// Resolve and build in one step, for a caller that has no install in hand.
    ///
    /// `Ok(None)` = no corpus resolved, which is an ordinary install and not an error; the
    /// `tried` list travels with it so the caller can say WHAT IT LOOKED FOR rather than
    /// falling silent. A confirm that quietly writes nothing is worse than a refusal, and a
    /// refusal that cannot say where it looked is barely better.
    ///
    /// **Prefer [`Self::from_install`] where an install already exists** (`src-tauri`'s boot
    /// does): resolving twice is two answers, and two answers can differ if a corpus appears
    /// between them.
    pub fn locate(p: &CorpusPaths) -> Result<(Option<Self>, Vec<String>), crate::loro::LoroError> {
        let (install, tried) = LoroInstall::locate(p)?;
        Ok((install.as_ref().map(CliLoroWriter::from_install), tried))
    }

    /// A real write with the writer's own arguments, for a caller whose write is not a CEO-confirmed
    /// correction: onboarding's saved files (`onboarding_loro.rs`, CEO 2026-11-14).
    pub(crate) fn run_verb(&self, verb: &str, extra: &[String], body: Option<&str>) -> Result<WriteOutput, CorrectionError> {
        self.run(verb, extra, body, false)
    }

    fn run(&self, verb: &str, extra: &[String], body: Option<&str>, dry_run: bool) -> Result<WriteOutput, CorrectionError> {
        use std::process::Stdio;
        let (root_flag, root_path) = self.root.args();
        let mut argv = vec![
            self.tools.write_bin().display().to_string(),
            verb.to_string(),
            root_flag.to_string(),
            root_path.display().to_string(),
            "--json".to_string(),
        ];
        if dry_run {
            argv.push("--dry-run".into());
        }
        argv.extend(extra.iter().cloned());

        let mut child = crate::runtime::interpreter_command(self.tools.node())
            .args(&argv)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| CorrectionError::WriterUnavailable(e.to_string()))?;
        if let Some(mut stdin) = child.stdin.take() {
            if let Some(b) = body {
                let _ = stdin.write_all(b.as_bytes());
            }
        }
        let out = child.wait_with_output().map_err(|e| CorrectionError::WriterUnavailable(e.to_string()))?;
        if !out.status.success() {
            // The exit code IS the contract (0/2/3/5/1) and stderr carries the sentence.
            // It is surfaced verbatim rather than reworded: a refusal like "that is a PROSE
            // section, corrected by editing the page" is an instruction to the CEO, and
            // paraphrasing it would lose the path he needs.
            return Err(CorrectionError::WriterRefused {
                code: out.status.code().unwrap_or(-1),
                message: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            });
        }
        serde_json::from_slice(&out.stdout)
            .map_err(|e| CorrectionError::WriterUnavailable(format!("the writer's --json output did not parse: {e}")))
    }
}

impl LoroWriteBackend for CliLoroWriter {
    fn preview(&self, write: &ProposedWrite, why: &str) -> Result<WriteOutput, CorrectionError> {
        let (mut args, body) = write.args();
        if !matches!(write, ProposedWrite::Append { .. }) {
            args.push("--why".into());
            args.push(why.into());
        }
        self.run(write.verb(), &args, body.as_deref(), true)
    }

    fn commit(&self, write: &ProposedWrite, why: &str) -> Result<WriteOutput, CorrectionError> {
        let (mut args, body) = write.args();
        if !matches!(write, ProposedWrite::Append { .. }) {
            args.push("--why".into());
            args.push(why.into());
        }
        self.run(write.verb(), &args, body.as_deref(), false)
    }

    fn show(&self, record_ref: &str) -> Result<WriteOutput, CorrectionError> {
        self.run("show", &["--ref".to_string(), record_ref.to_string()], None, false)
    }
}

// ---------------------------------------------------------------------------
// the desk
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProposalState {
    /// Shown to the CEO, nothing written. The ONLY state `confirm` accepts.
    AwaitingCeo,
    /// He said no. Not written — and §7: still offered again if the same correction comes
    /// up, because a decline is ambiguous and a repeat is evidence.
    Declined,
    /// He said yes and the write landed.
    Written,
    /// He said yes and the writer refused. The reason is kept — a failed write that
    /// disappears is indistinguishable from one that never happened.
    Failed,
    /// **This build cannot tell whether he has answered this or not, and waiting will
    /// settle it** — or he demonstrably answered and the write's outcome is unknown.
    ///
    /// Never written to disk and never minted by `propose` — it exists only in a projection,
    /// recomputed from scratch on every open. Held back from [`CorrectionDesk::pending_for`]
    /// and refused by `confirm`/`decline`. Since the CEO's 2026-09-28 answer (§96, module
    /// doc) it is reached ONLY when holding is not a hold forever:
    ///
    ///   * the record that could have been his answer was written by a NEWER RichOS, which
    ///     updating reads — so the proposal returns to its true state without him being asked;
    ///   * a `confirmed` record loaded and the writer's receipt did not — he answered, and a
    ///     second yes could run the write twice;
    ///   * the proposal's record is one he said never to ask about, which lifting that
    ///     suppression turns back into a question.
    ///
    /// A proposal whose answer is unreadable FOR GOOD is not this state: it is asked again,
    /// as [`ProposalState::AwaitingCeo`] with [`Proposal::asked_again`] set.
    Unresolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: String,
    pub at: u64,
    pub entity_id: String,
    pub thread_id: String,
    pub write: ProposedWrite,
    /// The CEO's own words for what is wrong. The writer requires it (exit 2 without one)
    /// and so does this desk, before a process is ever started.
    pub why: String,
    /// The writer's OWN `--dry-run` output: exactly what would be written, byte for byte.
    /// This is what the CEO is shown. A preview generated by anything other than the writer
    /// would be a description of a write rather than the write.
    pub preview: String,
    pub state: ProposalState,
    /// Set once the answer is in.
    pub outcome: Option<WriteOutput>,
    pub failure: Option<String>,
    /// **Present only when this is a question RichOS is asking him AGAIN**, because the
    /// record of his earlier answer could not be read (§96, module doc). It is the sentence
    /// every surface shows above the card, verbatim — [`asked_again_sentence`]`(1)` — so the
    /// desktop card, any other client and the health notice say one thing, not three.
    ///
    /// PROJECTION ONLY. `propose` mints `None`, `None` is never serialized, and the replay
    /// clears whatever a `proposed` line might carry before deciding afresh — so this never
    /// reaches the desk log and changes no byte an older build reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asked_again: Option<String>,
}

/// **What RichOS says when it asks him again about a correction whose earlier answer it
/// could not read.** One composer for every surface: the card carries `n = 1` on
/// [`Proposal::asked_again`], and [`DeskHealth::detail`] carries the count.
///
/// Plain words, and it says the one thing he needs to judge the card by: he may already have
/// answered this. That is the cost he accepted when he said *"Ask me again"* (§96), and a
/// re-ask that hid it would read as the machine forgetting him.
pub fn asked_again_sentence(n: usize) -> String {
    if n == 1 {
        "RichOS could not read your earlier answer to this correction, so it is asking you \
         again. You may already have answered it."
            .to_string()
    } else {
        format!(
            "RichOS could not read your earlier answers to {n} corrections, so it is asking \
             you again. You may already have answered them."
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "rec", rename_all = "kebab-case")]
enum DeskRecord {
    Proposed(Proposal),
    Confirmed { id: String, at: u64 },
    Written { id: String, at: u64, outcome: WriteOutput },
    Failed { id: String, at: u64, reason: String },
    Declined { id: String, at: u64, permanent: bool },
    /// A permanent decline, keyed by REF rather than by proposal, so the suppression
    /// survives the proposal being forgotten. §7: the suppression list must be inspectable,
    /// "or a term silently refuses to learn with no way to see why".
    Suppressed { record_ref: String, at: u64 },
    Unsuppressed { record_ref: String, at: u64 },
}

/// EVERY `rec` tag this build knows how to fold.
///
/// It is the only thing that can tell a record written by a NEWER RichOS apart from a
/// damaged one, so it has to stay exactly in step with `DeskRecord`. That is not left to
/// care: `the_known_tag_table_matches_the_record_type_exactly` below maps every variant
/// through an EXHAUSTIVE match, so adding a variant without adding its name here does not
/// compile.
///
/// A missing entry would be the dangerous direction: this build would report a genuinely
/// DAMAGED answer of that type as a benign one from the future, and go on waiting for an
/// update that is never coming.
pub const KNOWN_DESK_TAGS: &[&str] =
    &["proposed", "confirmed", "written", "failed", "declined", "suppressed", "unsuppressed"];

/// How the desk's records are told apart from a newer build's and from damage.
///
/// The judgment itself is [`crate::skip::classify_line`] — the SAME one `ledger.rs` and
/// `steering.rs` call, deliberately, so the three stores cannot drift apart about what
/// damage looks like. This file is the third reader to need it and it invents nothing.
const DESK_SKIP: SkipDialect = SkipDialect {
    tag_key: "rec",
    record_noun: "a correction-desk record",
    known_tags: KNOWN_DESK_TAGS,
};

/// The number in a desk proposal id, or `None` if the id is not one this desk minted.
///
/// STRICTLY `prop-` followed by ASCII decimal characters and nothing else. `u64::from_str`
/// would accept a leading `+`, and this runs on lines that may be damaged, so the shape is
/// checked rather than inferred from a successful parse.
fn proposal_number(id: &str) -> Option<u64> {
    let n = id.strip_prefix("prop-")?;
    if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    n.parse::<u64>().ok()
}

/// Pull a proposal number off a record this build could not fold.
///
/// **Structure only, and it can hold nothing of his.** The only value that can come back is
/// a run of ASCII decimal characters behind a fixed `prop-` prefix — a number this desk
/// minted itself. Every desk record except `Suppressed`/`Unsuppressed` carries `id` at the
/// top level, including `Proposed`, whose newtype payload serde flattens under the `rec`
/// tag.
fn salvage_proposal_number(line: &str) -> Option<u64> {
    let id = serde_json::from_str::<serde_json::Value>(line).ok()?.get("id")?.as_str()?.to_string();
    proposal_number(&id)
}

/// **What the app can honestly say about the correction desk it has just replayed.**
///
/// The desk log is the file that holds what the CEO was ASKED and what he ANSWERED. It is
/// not the ledger (what happened) and it is not the intake log (what he typed and Rich has
/// not acted on): it is the record of decisions he has already made, which is exactly why a
/// dropped record here has a cost neither of the others has — see the module doc.
///
/// `headline` and `detail` are empty when no record was skipped and no confirmed
/// write is unresolved. That is the
/// ordinary case and the signal to say nothing at all. There is no reassuring "the desk is
/// fine" state, for the same reason `Ledger::history_health` and `IntakeLog::health` have
/// none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeskHealth {
    /// Non-empty lines found in the file.
    pub records_read: usize,
    /// Records folded into the projection.
    pub records_applied: usize,
    pub skipped: usize,
    pub from_future: usize,
    pub damaged: usize,
    pub ambiguous: usize,
    /// Proposals held back because an answer was unreadable or a confirmed write
    /// has no durable outcome. See [`ProposalState::Unresolved`].
    pub unresolved: usize,
    /// Proposals put back in front of him because the record of his earlier answer could
    /// not be read (§96). See [`CorrectionDesk::asked_again`].
    pub asked_again: usize,
    /// Records not proposed against while their earlier decision or writer outcome
    /// is unresolved. See [`CorrectionDesk::held_records`].
    pub held_records: usize,
    /// One short sentence. Empty when there is no damaged record or unresolved write.
    pub headline: String,
    /// The honest explanation, in plain American English, with no stack trace in it.
    /// Empty when nothing was skipped.
    pub detail: String,
}

impl DeskHealth {
    /// Every record is readable and every confirmed write has a recorded outcome.
    pub fn is_clean(&self) -> bool {
        self.skipped == 0 && self.unresolved == 0
    }
}

/// The one place in the app that can change what loro believes — and it cannot do it
/// without the CEO having said yes first.
pub struct CorrectionDesk {
    path: PathBuf,
    writer: Box<dyn LoroWriteBackend>,
    proposals: Vec<Proposal>,
    suppressed: Vec<String>,
    next: u64,
    /// Every record on disk that this build could not fold, with the reason for each.
    /// Empty is the normal state.
    skipped: Vec<SkippedRecord>,
    /// Non-empty lines seen on replay, and how many of them were folded. Counted rather
    /// than derived, so "6 of 7" is a measurement and not an inference.
    records_read: usize,
    records_applied: usize,
    /// Records this build will not propose against because a decision he made about them
    /// could not be read. Distinct from `suppressed`, which is the list of refs he actually
    /// declined permanently — telling him a record was permanently declined when the truth
    /// is that this build cannot read the answer would be a wrong statement, not a short one.
    held: Vec<String>,
    /// Every ref that has ever carried an `Unsuppressed` record. A precautionary hold he has
    /// explicitly lifted must stay lifted across restarts, and the lift is already durable —
    /// this is how the replay sees it.
    ever_unsuppressed: Vec<String>,
    /// Proposals that would be asked again (§96) but name a record he said never to ask
    /// about. Held instead; [`CorrectionDesk::unsuppress`] on that record asks them.
    held_by_suppression: Vec<String>,
}

impl CorrectionDesk {
    /// Replace only the executable adapter after an idle engine installation.
    /// Proposal and confirmation history remain in this same desk.
    pub fn replace_writer(&mut self, writer: Box<dyn LoroWriteBackend>) { self.writer = writer; }

    pub fn open(path: impl AsRef<Path>, writer: Box<dyn LoroWriteBackend>) -> Result<Self, CorrectionError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut desk = CorrectionDesk {
            path,
            writer,
            proposals: Vec::new(),
            suppressed: Vec::new(),
            next: 1,
            skipped: Vec::new(),
            records_read: 0,
            records_applied: 0,
            held: Vec::new(),
            ever_unsuppressed: Vec::new(),
            held_by_suppression: Vec::new(),
        };
        desk.replay()?;
        Ok(desk)
    }

    /// Replay the log into the projection. **Never fatal on a record, never silent about
    /// one, and it never rewrites a byte of the file.**
    ///
    /// # What this used to do, and what each of it cost
    ///
    /// Until 2026-09-05 this loop was `lines().map_while(Result::ok)` with
    /// `let Ok(rec) = … else { continue }`, and the comment above the `continue` reasoned
    /// about a torn LAST line. Two things were wrong with that, and they are the same two
    /// the ledger and the intake log were fixed for earlier the same day:
    ///
    ///   1. **The read stopped at the first bad byte.** `lines()` yields `Err` for a line
    ///      that is not valid UTF-8, and `map_while(Result::ok)` ends the ITERATOR there —
    ///      so one damaged byte in the middle of the file silently discarded every record
    ///      BELOW it, not just its own. On this file that means every answer the CEO gave
    ///      after the damage.
    ///   2. **A skip left no trace at all.** No count, no log line, nothing a person could
    ///      read. The reasoning was that a half-written proposal "was never shown to
    ///      anybody", which is true of the last line and true of nothing else in the file.
    ///
    /// `read_until` fixes the first; [`crate::skip::classify_line`] — the judgment the
    /// ledger and the intake log already share — fixes the second. A real IO error still
    /// propagates, because a disk that cannot be read is not a damaged record and must not
    /// be reported as one.
    fn replay(&mut self) -> Result<(), CorrectionError> {
        if !self.path.exists() {
            return Ok(());
        }
        // The highest proposal number seen on a record this build could NOT fold. It is not
        // in `proposals`, so without it the counter can hand the same id out twice — the
        // same hazard `steering::IntakeLog::open` guards, and salvaged on the same rule.
        let mut salvaged_number: u64 = 0;
        // WHICH PROPOSALS COULD THE UNREADABLE RECORDS HAVE BEEN THE ANSWER TO?
        //
        // An answer record is always appended AFTER the `proposed` record it answers, so a
        // skipped line can only have answered a proposal that loaded ABOVE it. Two levels of
        // precision, and the narrow one is preferred wherever the file supports it:
        //
        //   * `at_risk_numbers` — a skipped line that is not `Damaged` is a well-formed JSON
        //     object with a plain-identifier tag, so its `id` is structure this build trusts.
        //     If it reads as `prop-<n>`, that ONE proposal is at risk and no other.
        //   * `broad_*_line` — every other skipped line (damaged, or naming no proposal,
        //     which is what a `suppressed`/`unsuppressed` record looks like) could have
        //     answered anything above it, so every proposal above the LAST such line is at
        //     risk. Deliberately blunt: the alternative is guessing.
        //
        // AND EACH IS KEPT IN TWO KINDS, because the CEO's 2026-09-28 answer (§96) turns on
        // whether the missing answer can ever come back:
        //
        //   * `FromFuture` — a NEWER RichOS wrote it, it is intact, and updating reads it.
        //     Holding the proposal until then is a wait, not a silence.
        //   * everything else — `Damaged`, not UTF-8 at all, or `Ambiguous`. Damaged bytes
        //     never become readable. `Ambiguous` is a known tag whose fields do not fit and
        //     that carries NO writer stamp; every build that can write a newer record stamps
        //     it (`skip::stamped_line`, spec point 18), so an unstamped misfit is not a newer
        //     build's record either, and no update will ever read it. Holding such a proposal
        //     is holding it forever, which is exactly what he said no to: it is asked again.
        let mut future_numbers: Vec<u64> = Vec::new();
        let mut unreadable_numbers: Vec<u64> = Vec::new();
        let mut broad_future_line = 0usize;
        let mut broad_unreadable_line = 0usize;
        let mut proposal_lines: Vec<(String, usize)> = Vec::new();
        let mut reader = BufReader::new(std::fs::File::open(&self.path)?);
        let mut raw: Vec<u8> = Vec::new();
        let mut line_no = 0usize;
        loop {
            raw.clear();
            let n = reader.read_until(b'\n', &mut raw)?;
            if n == 0 {
                break;
            }
            line_no += 1;
            let body = raw.strip_suffix(b"\n").unwrap_or(&raw);
            let text = match std::str::from_utf8(body) {
                Ok(t) => t,
                Err(_) => {
                    // Not text at all, so it cannot carry a tag and cannot be from the
                    // future: no version of RichOS has ever written a non-UTF-8 record.
                    //
                    // And it could have been any answer to anything above it. Until the
                    // §96 change this line was counted and then left out of the at-risk
                    // reckoning altogether, so a proposal whose answer had turned into
                    // non-text replayed as a plain, unexplained question.
                    self.records_read += 1;
                    self.skipped.push(crate::skip::not_utf8(line_no, body.len()));
                    broad_unreadable_line = broad_unreadable_line.max(line_no);
                    continue;
                }
            };
            let line = text.trim();
            if line.is_empty() {
                continue;
            }
            self.records_read += 1;
            match serde_json::from_str::<DeskRecord>(line) {
                Ok(rec) => {
                    self.records_applied += 1;
                    if let DeskRecord::Proposed(p) = &rec {
                        proposal_lines.push((p.id.clone(), line_no));
                    }
                    self.apply(rec);
                }
                Err(_) => {
                    let record = crate::skip::classify_line(line_no, line, &DESK_SKIP);
                    // SALVAGE THE PROPOSAL NUMBER, AND FROM NON-`Damaged` RECORDS ONLY.
                    //
                    // `Ambiguous` and `FromFuture` are both well-formed JSON objects
                    // carrying a plain-identifier tag — structure this build trusts — and
                    // both are lines a NEWER build will read, so an id minted on top of one
                    // of them collides for real. A `Damaged` line is neither: its numbers
                    // are not facts, and its bytes never become readable, so nothing is
                    // salvaged from it. That is the identical line `steering.rs` draws.
                    let from_future = record.kind == SkipKind::FromFuture;
                    let mut attributed = false;
                    if record.kind != SkipKind::Damaged {
                        if let Some(n) = salvage_proposal_number(line) {
                            salvaged_number = salvaged_number.max(n);
                            if from_future {
                                future_numbers.push(n);
                            } else {
                                unreadable_numbers.push(n);
                            }
                            attributed = true;
                        }
                    }
                    if !attributed {
                        if from_future {
                            broad_future_line = broad_future_line.max(line_no);
                        } else {
                            broad_unreadable_line = broad_unreadable_line.max(line_no);
                        }
                    }
                    self.skipped.push(record);
                }
            }
        }
        let highest_loaded = self
            .proposals
            .iter()
            .filter_map(|p| proposal_number(&p.id))
            .max()
            .unwrap_or(0);
        self.next = highest_loaded.max(salvaged_number) + 1;

        // EVERY PROPOSAL WHOSE ANSWER MIGHT HAVE BEEN ONE OF THE RECORDS THAT DID NOT LOAD is
        // either held (the answer comes back with an update) or asked again (it never will —
        // §96). A proposal that has an answer is left exactly as it is: its answer is on disk
        // and `confirm`/`decline` already refuse a second one.
        let at_risk_from = |numbers: &[u64], broad_line: usize| -> Vec<String> {
            numbers
                .iter()
                .map(|n| format!("prop-{n}"))
                .chain(
                    proposal_lines
                        .iter()
                        .filter(|(_, line)| *line < broad_line)
                        .map(|(id, _)| id.clone()),
                )
                .collect()
        };
        let at_risk_future = at_risk_from(&future_numbers, broad_future_line);
        let at_risk_unreadable = at_risk_from(&unreadable_numbers, broad_unreadable_line);
        for p in self.proposals.iter_mut() {
            let future = at_risk_future.contains(&p.id);
            let unreadable = at_risk_unreadable.contains(&p.id);
            if !(future || unreadable) {
                continue;
            }
            match p.state {
                // HE DEMONSTRABLY ANSWERED: a `confirmed` record loaded, and only the
                // writer's receipt is missing. Never asked again — a second yes could run the
                // write a second time. Held, exactly as before.
                ProposalState::Unresolved => {
                    // Preserve the existing unreadable-answer diagnosis when a
                    // writer receipt was present but damaged. A missing receipt
                    // with an otherwise intact log is the distinct crash case.
                    p.failure = Some("answer-unreadable".into());
                }
                ProposalState::AwaitingCeo => {
                    let suppressed_target =
                        p.write.target_ref().is_some_and(|r| self.suppressed.iter().any(|s| s == r));
                    if future || suppressed_target {
                        // A newer build's record: updating settles it, so nothing is asked
                        // meanwhile. Or a record he said never to ask about: asking would
                        // override the one answer this build CAN read.
                        p.state = ProposalState::Unresolved;
                        p.failure = Some("answer-unreadable".into());
                        if !future && suppressed_target {
                            self.held_by_suppression.push(p.id.clone());
                        }
                    } else {
                        // UNREADABLE FOR GOOD. His answer (§96): ask me again.
                        p.asked_again = Some(asked_again_sentence(1));
                    }
                }
                _ => {}
            }
        }

        // AND DO NOT ASK HIM ABOUT THE SAME RECORD FROM A DIFFERENT DIRECTION. The lost
        // answer could have been `Declined { permanent: true }` — "never ask me about this
        // again" — so a fresh proposal naming the same record would be exactly the repeat
        // this is here to stop. Not folded into `suppressed`: that list is what he actually
        // declined, and this one is what this build cannot read. A hold he has explicitly
        // lifted stays lifted, because `unsuppress` writes a durable record and the replay
        // above collected every ref that carries one.
        let holds: Vec<String> = self
            .proposals
            .iter()
            .filter(|p| p.state == ProposalState::Unresolved)
            .filter_map(|p| p.write.target_ref().map(str::to_string))
            .filter(|r| !self.ever_unsuppressed.contains(r))
            .collect();
        for r in holds {
            if !self.held.contains(&r) {
                self.held.push(r);
            }
        }

        self.report_skipped();
        Ok(())
    }

    /// **Skipping is never silent.** One line per skipped record, capped, then a summary
    /// line printed whenever anything at all was skipped.
    ///
    /// The cap exists because a badly damaged file could hold a hundred thousand unreadable
    /// lines, and a hundred thousand log lines is its own kind of silence. The COUNT is
    /// never capped — it is in the summary and in [`CorrectionDesk::desk_health`].
    fn report_skipped(&self) {
        if self.skipped.is_empty() {
            return;
        }
        const CAP: usize = 20;
        for r in self.skipped.iter().take(CAP) {
            eprintln!(
                "[richos] CORRECTION DESK RECORD SKIPPED ({}): line {} ({} bytes) — {}",
                r.kind.label(),
                r.line,
                r.bytes,
                r.detail
            );
        }
        if self.skipped.len() > CAP {
            eprintln!(
                "[richos] CORRECTION DESK: {} further skipped records not listed individually",
                self.skipped.len() - CAP
            );
        }
        let h = self.desk_health();
        eprintln!(
            "[richos] CORRECTION DESK SUMMARY for {}: {} records read, {} applied, {} skipped \
             ({} from a newer version, {} damaged, {} undetermined). These are corrections the \
             CEO was asked about and the answers he gave. Nothing was deleted or rewritten.",
            self.path.display(),
            h.records_read,
            h.records_applied,
            h.skipped,
            h.from_future,
            h.damaged,
            h.ambiguous
        );
    }

    fn apply(&mut self, rec: DeskRecord) {
        match rec {
            DeskRecord::Proposed(mut p) => {
                // A projection field, decided by the replay and never by a line on disk.
                p.asked_again = None;
                self.proposals.push(p)
            }
            DeskRecord::Confirmed { id, .. } => {
                if let Some(p) = self.find_mut(&id) {
                    p.state = ProposalState::Unresolved;
                    p.failure = Some("confirmed-write-outcome-unknown".into());
                }
            }
            DeskRecord::Written { id, outcome, .. } => {
                if let Some(p) = self.find_mut(&id) {
                    p.state = ProposalState::Written;
                    p.outcome = Some(outcome);
                    p.failure = None;
                }
            }
            DeskRecord::Failed { id, reason, .. } => {
                if let Some(p) = self.find_mut(&id) {
                    p.state = ProposalState::Failed;
                    p.failure = Some(reason);
                }
            }
            DeskRecord::Declined { id, permanent, .. } => {
                let target = self.find_mut(&id).map(|p| {
                    p.state = ProposalState::Declined;
                    p.write.target_ref().map(str::to_string)
                });
                // A PERMANENT DECLINE SUPPRESSES ON ITS OWN, not only via the `suppressed`
                // record `decline` writes next. The two are always written together and
                // fsync'd separately, so the second can be the one that is unreadable — and
                // without this, that alone would put a record he permanently declined back
                // in front of him. Deriving it from the decline costs nothing and makes the
                // `suppressed` record confirmatory rather than load-bearing.
                if permanent {
                    if let Some(Some(record_ref)) = target {
                        if !self.suppressed.contains(&record_ref) {
                            self.suppressed.push(record_ref);
                        }
                    }
                }
            }
            DeskRecord::Suppressed { record_ref, .. } => {
                if !self.suppressed.contains(&record_ref) {
                    self.suppressed.push(record_ref);
                }
            }
            DeskRecord::Unsuppressed { record_ref, .. } => {
                self.suppressed.retain(|r| r != &record_ref);
                self.held.retain(|r| r != &record_ref);
                if !self.ever_unsuppressed.contains(&record_ref) {
                    self.ever_unsuppressed.push(record_ref);
                }
            }
        }
    }

    fn find_mut(&mut self, id: &str) -> Option<&mut Proposal> {
        self.proposals.iter_mut().find(|p| p.id == id)
    }

    fn write_record(&mut self, rec: &DeskRecord) -> Result<(), CorrectionError> {
        // Stamped with this build's `written_by` — spec point 18, the same stamp the ledger
        // and the intake log write, so all three speak one dialect to `skip::classify_line`.
        let mut line = crate::skip::stamped_line(rec).map_err(|e| CorrectionError::Io(e.to_string()))?;
        line.push('\n');
        let mut f = std::fs::OpenOptions::new().create(true).read(true).append(true).open(&self.path)?;
        crate::util::ensure_line_boundary(&mut f)?;
        f.write_all(line.as_bytes())?;
        // fsync, not flush. A proposal the CEO has not answered must survive the power
        // going out — same reason `steering::IntakeLog` fsyncs, and a page cache is not disk.
        f.sync_all()?;
        Ok(())
    }

    // -- the loop ----------------------------------------------------------

    /// Stage a change and show the CEO exactly what it would do. **Writes nothing.**
    ///
    /// The preview comes from the writer's own `--dry-run`, so what he approves is the
    /// bytes that will land, not a description of them.
    pub fn propose(
        &mut self,
        entity_id: &str,
        thread_id: &str,
        write: ProposedWrite,
        why: &str,
    ) -> Result<Proposal, CorrectionError> {
        let why = why.trim();
        if why.is_empty() {
            // Refused here rather than at exit 2, because a proposal with no stated reason
            // is the shape an INFERRED correction takes. The reason is the human statement.
            return Err(CorrectionError::NoReason);
        }
        if let Some(target) = write.target_ref() {
            if self.suppressed.iter().any(|r| r == target) {
                return Err(CorrectionError::Suppressed(target.to_string()));
            }
            // A DIFFERENT REFUSAL, WITH A DIFFERENT SENTENCE. He did not decline this; this
            // build cannot read what he decided. Saying "you permanently declined it" would
            // be a wrong statement rather than a short one.
            if self.held.iter().any(|r| r == target) {
                return Err(CorrectionError::HeldRecord { record_ref: target.to_string() });
            }
        }
        let preview = self.writer.preview(&write, why)?;
        let id = format!("prop-{}", self.next);
        self.next += 1;
        let proposal = Proposal {
            id,
            at: crate::util::now_millis(),
            entity_id: entity_id.to_string(),
            thread_id: thread_id.to_string(),
            write,
            why: why.to_string(),
            preview: preview.text,
            state: ProposalState::AwaitingCeo,
            outcome: None,
            failure: None,
            asked_again: None,
        };
        self.write_record(&DeskRecord::Proposed(proposal.clone()))?;
        self.proposals.push(proposal.clone());
        Ok(proposal)
    }

    /// **The only path to a loro write in this application.**
    ///
    /// `entity_id` is the entity the CEO is looking at when he says yes, and it must match
    /// the proposal's own. A confirmation that crossed entities would let a click in one
    /// company's window change another company's memory — the write-path twin of the
    /// cross-entity leak the read seam re-asserts against.
    pub fn confirm(&mut self, entity_id: &str, id: &str) -> Result<Proposal, CorrectionError> {
        let p = self.proposals.iter().find(|p| p.id == id).ok_or_else(|| CorrectionError::NoSuchProposal(id.into()))?;
        if p.entity_id != entity_id {
            return Err(CorrectionError::WrongEntity {
                id: id.into(),
                owner: p.entity_id.clone(),
                asked: entity_id.into(),
            });
        }
        // Checked BEFORE `NotAwaiting`, because "it cannot be confirmed twice" would be a
        // claim this build is in no position to make: it does not know whether it was
        // confirmed once.
        if p.state == ProposalState::Unresolved {
            if p.failure.as_deref() == Some("confirmed-write-outcome-unknown") {
                return Err(CorrectionError::WriteOutcomeUnknown { id: id.into() });
            }
            return Err(CorrectionError::AnswerUnreadable { id: id.into() });
        }
        if p.state != ProposalState::AwaitingCeo {
            return Err(CorrectionError::NotAwaiting { id: id.into(), state: p.state });
        }
        let (write, why) = (p.write.clone(), p.why.clone());
        let at = crate::util::now_millis();
        // The confirmation is durable BEFORE the write, so a crash between the two leaves
        // evidence that the CEO said yes rather than losing his answer.
        self.write_record(&DeskRecord::Confirmed { id: id.into(), at })?;
        self.apply(DeskRecord::Confirmed { id: id.into(), at });
        match self.writer.commit(&write, &why) {
            Ok(outcome) => {
                self.write_record(&DeskRecord::Written { id: id.into(), at, outcome: outcome.clone() })?;
                self.apply(DeskRecord::Written { id: id.into(), at, outcome });
            }
            Err(e) => {
                let reason = e.to_string();
                self.write_record(&DeskRecord::Failed { id: id.into(), at, reason: reason.clone() })?;
                self.apply(DeskRecord::Failed { id: id.into(), at, reason });
            }
        }
        Ok(self.proposals.iter().find(|p| p.id == id).cloned().expect("just applied"))
    }

    /// He said no. §7: a decline is NOT permanent unless he says so, because it is
    /// ambiguous — not a record / not now / misclicked — while a repeat is evidence.
    pub fn decline(&mut self, id: &str, permanent: bool) -> Result<(), CorrectionError> {
        let p = self.proposals.iter().find(|p| p.id == id).ok_or_else(|| CorrectionError::NoSuchProposal(id.into()))?;
        if p.state == ProposalState::Unresolved {
            return Err(CorrectionError::AnswerUnreadable { id: id.into() });
        }
        if p.state != ProposalState::AwaitingCeo {
            return Err(CorrectionError::NotAwaiting { id: id.into(), state: p.state });
        }
        let target = p.write.target_ref().map(str::to_string);
        let at = crate::util::now_millis();
        self.write_record(&DeskRecord::Declined { id: id.into(), at, permanent })?;
        self.apply(DeskRecord::Declined { id: id.into(), at, permanent });
        if permanent {
            if let Some(target) = target {
                self.write_record(&DeskRecord::Suppressed { record_ref: target.clone(), at })?;
                self.apply(DeskRecord::Suppressed { record_ref: target, at });
            }
        }
        Ok(())
    }

    /// Lift a permanent decline. §7 requires the suppression list to be inspectable; a list
    /// you can see and cannot clear is only half of that.
    ///
    /// It lifts a precautionary hold ([`held_records`](Self::held_records)) too, and that is
    /// deliberately the SAME verb: both lists answer one question — may RichOS raise a
    /// correction about this record? — and a second verb would be a second thing to
    /// understand for no gain. The `unsuppressed` record it writes is durable, so a hold he
    /// lifts is not re-applied by the next replay.
    ///
    /// A proposal held ONLY because it names a record he had said never to ask about (its own
    /// answer being unreadable for good) becomes a question again the moment he lifts that —
    /// the same thing the next replay would decide, without making him wait for a restart.
    pub fn unsuppress(&mut self, record_ref: &str) -> Result<(), CorrectionError> {
        let at = crate::util::now_millis();
        self.write_record(&DeskRecord::Unsuppressed { record_ref: record_ref.into(), at })?;
        self.apply(DeskRecord::Unsuppressed { record_ref: record_ref.into(), at });
        let waiting = std::mem::take(&mut self.held_by_suppression);
        for id in waiting {
            let Some(p) = self.proposals.iter_mut().find(|p| p.id == id) else { continue };
            if p.state == ProposalState::Unresolved && p.write.target_ref() == Some(record_ref) {
                p.state = ProposalState::AwaitingCeo;
                p.failure = None;
                p.asked_again = Some(asked_again_sentence(1));
            } else {
                self.held_by_suppression.push(id);
            }
        }
        Ok(())
    }

    /// What does loro actually believe? The answer is a file. Read-only, no proposal needed.
    pub fn show(&self, record_ref: &str) -> Result<WriteOutput, CorrectionError> {
        self.writer.show(record_ref)
    }

    /// Proposals in front of the CEO for one entity, oldest first.
    pub fn pending_for(&self, entity_id: &str) -> Vec<&Proposal> {
        self.proposals
            .iter()
            .filter(|p| p.state == ProposalState::AwaitingCeo && p.entity_id == entity_id)
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<&Proposal> {
        self.proposals.iter().find(|p| p.id == id)
    }

    pub fn suppressed(&self) -> &[String] {
        &self.suppressed
    }

    /// Every record on disk this build could not fold, with the reason for each.
    pub fn skipped_records(&self) -> &[SkippedRecord] {
        &self.skipped
    }

    /// **Proposals this build refuses to put in front of the CEO, because it cannot tell
    /// whether he has already answered them.** Empty is the normal state.
    ///
    /// A proposal lands here when a record that COULD have been its answer was one this
    /// build could not read. "Could have been" is as narrow as the file allows:
    ///
    ///   * an unreadable record that is not damaged and names `prop-<n>` puts exactly that
    ///     one proposal here — a well-formed record from a newer build carries a trustworthy
    ///     id, so nothing else needs to be doubted;
    ///   * any other unreadable record — damaged, or naming no proposal at all — puts every
    ///     proposal that loaded above it here, because an answer is always appended after the
    ///     proposal it answers and there is nothing else in the file to narrow it with.
    ///
    /// Nothing is lost: the proposal is untouched on disk, and the same replay on a build
    /// that can read the record resolves it. For a record from a newer RichOS that is what
    /// updating does. For genuinely damaged bytes it is not, and this list is the honest
    /// statement of that rather than a quiet one.
    ///
    /// **Since the CEO's 2026-09-28 answer (§96) the second bullet applies only where the
    /// record can come back.** A proposal whose possible answer is unreadable for good —
    /// damaged bytes, or an unstamped record no build will ever read — is not held here; it
    /// is asked again ([`asked_again`](Self::asked_again)). What stays here: a record from a
    /// newer build, a confirmation whose writer outcome is unknown, and a proposal naming a
    /// record he said never to ask about.
    pub fn unresolved(&self) -> Vec<&Proposal> {
        self.proposals.iter().filter(|p| p.state == ProposalState::Unresolved).collect()
    }

    /// **Questions RichOS is asking him again** because the record of his earlier answer
    /// could not be read and never will be (§96). Every one of them is also in
    /// [`pending_for`](Self::pending_for) for its entity — an ordinary question, confirmable
    /// and declinable — and carries [`Proposal::asked_again`] so the card says so.
    pub fn asked_again(&self) -> Vec<&Proposal> {
        self.proposals
            .iter()
            .filter(|p| p.state == ProposalState::AwaitingCeo && p.asked_again.is_some())
            .collect()
    }

    /// Records this build will not raise a NEW correction about, because the answer it
    /// cannot read might have been "never ask me about this again".
    ///
    /// Distinct from [`suppressed`](Self::suppressed), which is what he actually declined
    /// permanently. [`unsuppress`](Self::unsuppress) clears either.
    pub fn held_records(&self) -> &[String] {
        &self.held
    }

    /// **What the app can honestly say about the desk it just replayed.**
    ///
    /// `headline`/`detail` are empty when nothing was skipped — the signal to say nothing at
    /// all, not to say something reassuring about a check that found nothing to report.
    ///
    /// The sentences are composed HERE and deliberately not shared with
    /// `Ledger::history_health` or `IntakeLog::health`, because the three are not the same
    /// statement and nothing should be in a position to substitute one for another. That one
    /// is about messages on screen that did not load; the intake one is about something he
    /// typed that never reached Rich; this one is about a decision he was asked to make and
    /// the answer he gave.
    ///
    /// ## The one thing this cannot answer, and the smallest change that would
    ///
    /// `ambiguous` counts records whose TYPE this build knows and whose FIELDS do not fit. A
    /// newer RichOS that added a required field to an existing record and a record whose
    /// bytes were damaged in place produce byte-identical evidence, and this build refuses to
    /// guess between them. The minimal fix is the same one-field change
    /// `Ledger::history_health` names: a writer-schema number on every record. It is NOT
    /// done here, because starting to write a new field changes what goes on customers'
    /// disks and this change is deliberately read-only.
    pub fn desk_health(&self) -> DeskHealth {
        let count = |k: SkipKind| self.skipped.iter().filter(|r| r.kind == k).count();
        let from_future = count(SkipKind::FromFuture);
        let damaged = count(SkipKind::Damaged);
        let ambiguous = count(SkipKind::Ambiguous);
        let skipped = self.skipped.len();
        let unresolved = self.proposals.iter().filter(|p| p.state == ProposalState::Unresolved).count();
        let asked_again = self.asked_again().len();
        let held_records = self.held.len();

        let (headline, detail) = if skipped == 0 && unresolved > 0 {
            ("A confirmed knowledge write needs reconciliation.".into(),
             "Your confirmation was saved, but the writer outcome was not recorded. RichOS has retained the proposal and will not repeat the write or claim it succeeded. The corpus and source evidence must be checked before resolving it.".into())
        } else if skipped == 0 {
            (String::new(), String::new())
        } else {
            let plural =
                |n: usize, one: &str, many: &str| if n == 1 { one.to_string() } else { many.to_string() };
            let headline = if unresolved > 0 {
                // THE CONSEQUENCE HE CAN FEEL LEADS, ahead of the cause. Whether the record
                // was damaged or written by a newer build matters less to him than the fact
                // that a decision he may have made is not being acted on.
                plural(
                    unresolved,
                    "A correction you may have already answered is being held back.",
                    "Some corrections you may have already answered are being held back.",
                )
            } else if asked_again > 0 {
                // The same consequence-first order: what he will SEE is a question he may
                // already have answered, and that leads.
                plural(
                    asked_again,
                    "A correction you may have already answered is being asked again.",
                    "Some corrections you may have already answered are being asked again.",
                )
            } else if damaged > 0 || ambiguous > 0 {
                plural(
                    damaged + ambiguous,
                    "Part of the record of corrections you have been asked about could not be read.",
                    "Parts of the record of corrections you have been asked about could not be read.",
                )
            } else {
                "Part of the record of corrections you have been asked about was written by a \
                 newer version of RichOS."
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
            if unresolved > 0 {
                parts.push(format!(
                    "{} {} being held back rather than put in front of you again, because this \
                     version cannot tell from the file whether you have already answered {} — \
                     and asking you a second time about something you have already decided \
                     would be worse than waiting.",
                    unresolved,
                    plural(unresolved, "correction is", "corrections are"),
                    plural(unresolved, "it", "them"),
                ));
            }
            if asked_again > 0 {
                // THE SAME SENTENCE THE CARD CARRIES, from the same composer, so the notice
                // and the question he is looking at cannot say two different things (§96).
                parts.push(asked_again_sentence(asked_again));
            }
            if held_records > 0 {
                parts.push(format!(
                    "For the same reason, RichOS will not raise a new correction about {} {} \
                     those held-back proposals name.",
                    held_records,
                    plural(held_records, "the record", "the records"),
                ));
            }
            parts.push(format!(
                "Everything else loaded: {} of {} records. Nothing was deleted and nothing was \
                 rewritten: every record is still exactly where it was on disk.",
                self.records_applied, self.records_read,
            ));
            (headline, parts.join(" "))
        };

        DeskHealth {
            records_read: self.records_read,
            records_applied: self.records_applied,
            skipped,
            from_future,
            damaged,
            ambiguous,
            unresolved,
            asked_again,
            held_records,
            headline,
            detail,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Behind an `Arc<Mutex<_>>`, so the SAME desk is reachable from the turn path (where
    /// `belief.rs`'s trigger files a proposal) and from the answer path (where the CEO
    /// confirms one) without either waiting on the other. The posture
    /// `staging::CandidateDesk::shared` already takes, for the same reason: `send_message`
    /// holds the spine lock for a whole turn, and a correction panel that froze until Rich
    /// finished would not be used.
    pub fn shared(self) -> SharedCorrectionDesk {
        Arc::new(Mutex::new(self))
    }
}

pub type SharedCorrectionDesk = Arc<Mutex<CorrectionDesk>>;

/// The event a filed loro proposal is announced on.
///
/// A separate name from `staging::EVENT_CORRECTION_STAGED` because the two carry different
/// payloads and a surface's subscription list is the proof of what it renders — but the
/// same JOB: the proposal is already durable on the desk's own log before this fires, so a
/// webview that missed it loses a badge update and never a record.
pub const EVENT_LORO_PROPOSED: &str = "rich://loro-proposed";

/// A sink for filed proposals — whatever renders the desk.
pub trait ProposalObserver: Send {
    /// MUST be non-blocking and infallible from the spine's view, for the reason
    /// `staging::CorrectionObserver` must: a UI that is not listening never stalls a turn.
    fn on_correction_proposed(&self, proposal: &Proposal);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// A backend that reports every call and, on a real commit, actually touches a file —
    /// so "propose wrote nothing" is checked against the disk rather than against a flag.
    #[derive(Clone)]
    struct FakeWriter {
        dir: PathBuf,
        calls: Arc<Mutex<Vec<String>>>,
        refuse_commit: Option<(i32, String)>,
    }

    impl FakeWriter {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("richos-desk-{tag}-{}-{}", std::process::id(), crate::util::now_millis()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            FakeWriter { dir, calls: Arc::new(Mutex::new(Vec::new())), refuse_commit: None }
        }
        fn corpus_files(&self) -> Vec<String> {
            let mut v: Vec<String> = std::fs::read_dir(&self.dir)
                .unwrap()
                .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().to_string()))
                .collect();
            v.sort();
            v
        }
    }

    impl LoroWriteBackend for FakeWriter {
        fn preview(&self, write: &ProposedWrite, why: &str) -> Result<WriteOutput, CorrectionError> {
            self.calls.lock().unwrap().push(format!("preview:{}:{why}", write.verb()));
            Ok(WriteOutput {
                op: write.verb().into(),
                dry_run: true,
                r#ref: "rec:ceo/records/x".into(),
                file: self.dir.join("x.md").display().to_string(),
                text: "---\nkind: decision\n---\n\nWhat would be written.\n".into(),
                ..Default::default()
            })
        }
        fn commit(&self, write: &ProposedWrite, why: &str) -> Result<WriteOutput, CorrectionError> {
            self.calls.lock().unwrap().push(format!("commit:{}:{why}", write.verb()));
            if let Some((code, message)) = &self.refuse_commit {
                return Err(CorrectionError::WriterRefused { code: *code, message: message.clone() });
            }
            std::fs::write(self.dir.join("x.md"), "written\n").unwrap();
            Ok(WriteOutput {
                op: write.verb().into(),
                dry_run: false,
                r#ref: "rec:ceo/records/x".into(),
                file: self.dir.join("x.md").display().to_string(),
                text: "written\n".into(),
                ..Default::default()
            })
        }
        fn show(&self, record_ref: &str) -> Result<WriteOutput, CorrectionError> {
            self.calls.lock().unwrap().push(format!("show:{record_ref}"));
            Ok(WriteOutput { op: "show".into(), r#ref: record_ref.into(), ..Default::default() })
        }
    }

    fn desk(tag: &str) -> (CorrectionDesk, FakeWriter, PathBuf) {
        let w = FakeWriter::new(tag);
        let log = std::env::temp_dir().join(format!("richos-desk-log-{tag}-{}-{}.jsonl", std::process::id(), crate::util::now_millis()));
        let _ = std::fs::remove_file(&log);
        let d = CorrectionDesk::open(&log, Box::new(w.clone())).unwrap();
        (d, w, log)
    }

    fn a_supersede() -> ProposedWrite {
        ProposedWrite::Supersede {
            record_ref: "rec:ceo/records/old".into(),
            new_id: "new".into(),
            kind: "decision".into(),
            scope: Some("org-shared".into()),
            body: "What is actually true.".into(),
        }
    }

    #[test]
    fn accepted_proposal_survives_restart_after_a_torn_tail() {
        let (mut d, w, log) = desk("torn-tail");
        d.propose("acme", "thr-1", a_supersede(), "before crash").unwrap();
        drop(d);
        std::fs::OpenOptions::new().append(true).open(&log).unwrap()
            .write_all(b"{\"record\":\"interrupted").unwrap();
        let mut d = CorrectionDesk::open(&log, Box::new(w.clone())).unwrap();
        let mut next = a_supersede();
        if let ProposedWrite::Supersede { record_ref, .. } = &mut next {
            *record_ref = "rec:ceo/records/another".into();
        }
        let accepted = d.propose("acme", "thr-1", next, "after recovery").unwrap();
        drop(d);
        let recovered = CorrectionDesk::open(&log, Box::new(w)).unwrap();
        assert_eq!(recovered.get(&accepted.id).unwrap().state, ProposalState::AwaitingCeo);
        assert!(recovered.get(&accepted.id).unwrap().asked_again.is_none(), "proposed below the tear");
        // The torn line could have been his answer to prop-1 and will never be readable, so
        // prop-1 is asked AGAIN (§96) and says so; the proposal filed after it is not.
        let pending: Vec<&str> = recovered.pending_for("acme").iter().map(|p| p.id.as_str()).collect();
        assert_eq!(pending, ["prop-1", accepted.id.as_str()]);
        assert_eq!(recovered.get("prop-1").unwrap().asked_again.as_deref(), Some(asked_again_sentence(1).as_str()));
        std::fs::remove_file(log).unwrap();
    }

    #[test]
    fn proposing_shows_the_writers_own_dry_run_and_touches_nothing() {
        let (mut d, w, log) = desk("propose");
        let before = w.corpus_files();
        let p = d.propose("femcboost", "thr-1", a_supersede(), "we never decided that").unwrap();
        assert_eq!(p.state, ProposalState::AwaitingCeo);
        assert!(p.preview.contains("What would be written"), "the PREVIEW is the writer's own bytes");
        assert_eq!(w.corpus_files(), before, "propose must not write a single file");
        assert_eq!(*w.calls.lock().unwrap(), vec!["preview:supersede:we never decided that"]);
        let _ = std::fs::remove_file(&log);
    }

    #[test]
    fn a_confirmation_without_a_writer_receipt_never_replays_as_an_unanswered_proposal() {
        let (mut d, w, log) = desk("confirmed-crash");
        let p = d.propose("fictional", "thread", a_supersede(), "explicit correction").unwrap();
        d.write_record(&DeskRecord::Confirmed { id: p.id.clone(), at: 1 }).unwrap();
        drop(d);
        let mut reopened = CorrectionDesk::open(&log, Box::new(w.clone())).unwrap();
        assert!(reopened.pending_for("fictional").is_empty());
        assert!(matches!(reopened.confirm("fictional", &p.id), Err(CorrectionError::WriteOutcomeUnknown {..})));
        assert!(w.corpus_files().is_empty());
        let health = reopened.desk_health();
        assert!(!health.is_clean());
        assert!(health.headline.contains("needs reconciliation"));
        assert_eq!(health.skipped, 0);
        let _ = std::fs::remove_file(log);
    }

    #[test]
    fn nothing_is_written_until_the_ceo_says_yes_and_then_exactly_once() {
        let (mut d, w, log) = desk("confirm");
        let p = d.propose("femcboost", "thr-1", a_supersede(), "wrong").unwrap();
        assert!(w.corpus_files().is_empty());
        let done = d.confirm("femcboost", &p.id).unwrap();
        assert_eq!(done.state, ProposalState::Written);
        assert_eq!(w.corpus_files(), vec!["x.md".to_string()]);
        // A second confirm is refused — a double-click must not write twice.
        assert!(matches!(d.confirm("femcboost", &p.id), Err(CorrectionError::NotAwaiting { .. })));
        assert_eq!(w.calls.lock().unwrap().iter().filter(|c| c.starts_with("commit")).count(), 1);
        let _ = std::fs::remove_file(&log);
    }

    #[test]
    fn a_crash_between_the_ask_and_the_answer_keeps_the_proposal_and_writes_nothing() {
        // The whole reason this log fsyncs. "He confirms after lunch" must survive a
        // rotation, a relaunch and a power cut.
        let (mut d, w, log) = desk("crash");
        let p = d.propose("femcboost", "thr-1", a_supersede(), "wrong").unwrap();
        drop(d);
        let reopened = CorrectionDesk::open(&log, Box::new(w.clone())).unwrap();
        let back = reopened.get(&p.id).expect("the proposal survived");
        assert_eq!(back.state, ProposalState::AwaitingCeo);
        assert_eq!(back.why, "wrong");
        assert!(back.preview.contains("What would be written"));
        assert!(w.corpus_files().is_empty(), "and still nothing was written");
        assert_eq!(reopened.pending_for("femcboost").len(), 1);
        let _ = std::fs::remove_file(&log);
    }

    #[test]
    fn a_decline_writes_nothing_and_is_still_offered_again() {
        // ceo-decisions.md §7: a decline is ambiguous (not a record / not now / misclicked)
        // and a REPEAT is the evidence. So declining must not suppress by itself.
        let (mut d, w, log) = desk("decline");
        let p = d.propose("femcboost", "thr-1", a_supersede(), "wrong").unwrap();
        d.decline(&p.id, false).unwrap();
        assert_eq!(d.get(&p.id).unwrap().state, ProposalState::Declined);
        assert!(w.corpus_files().is_empty());
        assert!(matches!(d.confirm("femcboost", &p.id), Err(CorrectionError::NotAwaiting { .. })));
        // ...and the same correction can be raised again.
        let again = d.propose("femcboost", "thr-1", a_supersede(), "wrong, again").unwrap();
        assert_eq!(again.state, ProposalState::AwaitingCeo);
        let _ = std::fs::remove_file(&log);
    }

    #[test]
    fn a_permanent_decline_suppresses_the_ref_and_the_suppression_is_inspectable_and_liftable() {
        let (mut d, _w, log) = desk("suppress");
        let p = d.propose("femcboost", "thr-1", a_supersede(), "wrong").unwrap();
        d.decline(&p.id, true).unwrap();
        assert_eq!(d.suppressed(), ["rec:ceo/records/old".to_string()]);
        match d.propose("femcboost", "thr-1", a_supersede(), "wrong again") {
            Err(CorrectionError::Suppressed(r)) => assert_eq!(r, "rec:ceo/records/old"),
            other => panic!("expected a suppression refusal, got {other:?}"),
        }
        d.unsuppress("rec:ceo/records/old").unwrap();
        assert!(d.suppressed().is_empty());
        assert!(d.propose("femcboost", "thr-1", a_supersede(), "wrong again").is_ok());
        let _ = std::fs::remove_file(&log);
    }

    #[test]
    fn a_proposal_with_no_stated_reason_is_refused_before_a_process_is_started() {
        // A correction with no reason is the shape an INFERRED one takes. The writer also
        // exits 2 on it; refusing here means nothing is even spawned.
        let (mut d, w, log) = desk("noreason");
        assert!(matches!(d.propose("femcboost", "thr-1", a_supersede(), "   "), Err(CorrectionError::NoReason)));
        assert!(w.calls.lock().unwrap().is_empty(), "not even a dry run was attempted");
        let _ = std::fs::remove_file(&log);
    }

    // ---- THE CROSS-ENTITY NEGATIVE CONTROL, WRITE SIDE -------------------
    #[test]
    fn a_confirmation_from_another_entity_is_refused_and_writes_nothing() {
        let (mut d, w, log) = desk("entity");
        let p = d.propose("femcboost", "thr-1", a_supersede(), "wrong").unwrap();
        match d.confirm("deeply", &p.id) {
            Err(CorrectionError::WrongEntity { owner, asked, .. }) => {
                assert_eq!(owner, "femcboost");
                assert_eq!(asked, "deeply");
            }
            other => panic!("a cross-entity confirmation must be refused, got {other:?}"),
        }
        assert!(w.corpus_files().is_empty(), "and nothing was written");
        assert!(w.calls.lock().unwrap().iter().all(|c| !c.starts_with("commit")));
        // The rightful entity still can.
        assert_eq!(d.confirm("femcboost", &p.id).unwrap().state, ProposalState::Written);
        assert_eq!(d.pending_for("deeply").len(), 0, "and it was never in the other entity's queue");
        let _ = std::fs::remove_file(&log);
    }

    #[test]
    fn a_writer_refusal_is_recorded_rather_than_swallowed() {
        // "would edit prose" (exit 5) is an INSTRUCTION to the CEO — it names the file to
        // open. A failed write that disappears is indistinguishable from one never made.
        let mut w = FakeWriter::new("refuse");
        w.refuse_commit = Some((5, "loro write: \"wiki:x.md#y\" is a PROSE section — edit the page".into()));
        let log = std::env::temp_dir().join(format!("richos-desk-refuse-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&log);
        let mut d = CorrectionDesk::open(&log, Box::new(w.clone())).unwrap();
        let p = d.propose("femcboost", "thr-1", a_supersede(), "wrong").unwrap();
        let after = d.confirm("femcboost", &p.id).unwrap();
        assert_eq!(after.state, ProposalState::Failed);
        assert!(after.failure.as_deref().unwrap().contains("PROSE section"), "{:?}", after.failure);
        assert!(w.corpus_files().is_empty());
        // Reopening keeps the failure — it is not a transient in-memory note.
        let reopened = CorrectionDesk::open(&log, Box::new(w)).unwrap();
        assert_eq!(reopened.get(&p.id).unwrap().state, ProposalState::Failed);
        let _ = std::fs::remove_file(&log);
    }

    #[test]
    fn showing_a_record_needs_no_proposal_because_reading_is_not_correcting() {
        let (d, w, log) = desk("show");
        d.show("rec:ceo/records/old").unwrap();
        assert_eq!(*w.calls.lock().unwrap(), vec!["show:rec:ceo/records/old"]);
        assert!(w.corpus_files().is_empty());
        let _ = std::fs::remove_file(&log);
    }

    #[test]
    fn scope_widening_is_not_reachable_from_this_module() {
        // loro-writer.md: widening ceo-private -> org-shared/external is the one edit that
        // turns the CEO's private view into something every worker sees, and it must be a
        // decision rather than the side effect of fixing a typo. The writer refuses it
        // without --widen-scope; this module never emits that flag, so the app cannot
        // widen at all. Narrowing is available and needs no ceremony.
        let src = include_str!("correction.rs");
        // The SHIPPING half only. The scan must not read its own assertion, which names the
        // flag it is looking for — that is how a self-referential source scan passes or
        // fails for the wrong reason.
        let shipping = &src[..src.find("\n#[cfg(test)]").unwrap_or(src.len())];
        let code: String = shipping.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
        assert!(!code.contains("--widen-scope"), "the widening flag must not be reachable from the app");
        // Positive probe: the scan is looking at real argv-building code, not at nothing.
        assert!(code.contains("--body-stdin"), "the scan must be reading the argv builder");
    }

    #[test]
    fn the_write_path_reads_no_configuration_from_the_environment() {
        // THE GUARD AGAINST A FOURTH INSTANCE, in the only place it can be structural.
        //
        // Three times on 2026-09-01 a component read its configuration from environment
        // variables a Finder launch does not have: the engine directory, the corpus READ
        // path, and then this file's writer, which was `CliLoroWriter::from_env()` and was
        // therefore `None` on every double-click. The fix was not a fourth resolver — it was
        // to delete the env-only constructor and take a resolved `LoroInstall`.
        //
        // A deleted function comes back. This scan is what stops it coming back HERE: the
        // shipping half of this module must contain no environment read at all, so the next
        // writer constructor has to be handed its configuration rather than going looking
        // for it in a process that has none.
        let src = include_str!("correction.rs");
        let shipping = &src[..src.find("\n#[cfg(test)]").unwrap_or(src.len())];
        let code: Vec<&str> = shipping
            .lines()
            .filter(|l| {
                let t = l.trim_start();
                !t.starts_with("//")
            })
            .collect();
        let offenders: Vec<&&str> = code.iter().filter(|l| l.contains("env::var")).collect();
        assert!(
            offenders.is_empty(),
            "the write path must be GIVEN its corpus, never look one up in an environment a \
             GUI launch does not have: {offenders:?}"
        );
        // Positive probe: the scan is reading the real constructor, not an empty string.
        assert!(
            code.iter().any(|l| l.contains("pub fn from_install(install: &LoroInstall)")),
            "the scan must be reading the constructor it is guarding"
        );
    }

    #[test]
    fn confirm_is_the_only_desk_method_that_reaches_a_write() {
        // The module's central claim, checked structurally rather than promised — the same
        // posture loro takes with `assertNoSideEffects` over its own compiler modules.
        let src = include_str!("correction.rs");
        let impl_start = src.find("impl CorrectionDesk {").expect("the desk impl");
        let impl_src = &src[impl_start..src.find("\n#[cfg(test)]").unwrap_or(src.len())];
        let mut current = String::new();
        let mut offenders: Vec<String> = Vec::new();
        let mut commit_sites = 0usize;
        for line in impl_src.lines() {
            let t = line.trim_start();
            if t.starts_with("pub fn ") || t.starts_with("fn ") {
                current = t.trim_start_matches("pub ").trim_start_matches("fn ").split('(').next().unwrap_or("").to_string();
            }
            if t.contains("self.writer.commit(") {
                commit_sites += 1;
                if current != "confirm" {
                    offenders.push(current.clone());
                }
            }
        }
        assert_eq!(commit_sites, 1, "the scan must find the write it is guarding — it found {commit_sites}");
        assert!(offenders.is_empty(), "these desk methods reach a loro write without a confirmation: {offenders:?}");
    }

    /// EXHAUSTIVE on purpose, and in this module rather than in
    /// `tests/correction_forward_compat_tests.rs` because `DeskRecord` is private.
    ///
    /// Adding a variant to `DeskRecord` without adding its name to [`KNOWN_DESK_TAGS`] does
    /// not COMPILE here — which is the point, because a missing entry would make this build
    /// report a genuinely DAMAGED record of that type as a benign one from the future, and
    /// go on waiting for an update that is never coming.
    fn tag_of(r: &DeskRecord) -> &'static str {
        match r {
            DeskRecord::Proposed(_) => "proposed",
            DeskRecord::Confirmed { .. } => "confirmed",
            DeskRecord::Written { .. } => "written",
            DeskRecord::Failed { .. } => "failed",
            DeskRecord::Declined { .. } => "declined",
            DeskRecord::Suppressed { .. } => "suppressed",
            DeskRecord::Unsuppressed { .. } => "unsuppressed",
        }
    }

    #[test]
    fn the_known_tag_table_matches_the_record_type_exactly() {
        let proposal = Proposal {
            id: "prop-1".into(),
            at: 1,
            entity_id: "ent".into(),
            thread_id: "thr".into(),
            write: a_supersede(),
            why: "because".into(),
            preview: "preview".into(),
            state: ProposalState::AwaitingCeo,
            outcome: None,
            failure: None,
            asked_again: None,
        };
        let samples = [
            DeskRecord::Proposed(proposal),
            DeskRecord::Confirmed { id: "prop-1".into(), at: 1 },
            DeskRecord::Written { id: "prop-1".into(), at: 1, outcome: WriteOutput::default() },
            DeskRecord::Failed { id: "prop-1".into(), at: 1, reason: "no".into() },
            DeskRecord::Declined { id: "prop-1".into(), at: 1, permanent: false },
            DeskRecord::Suppressed { record_ref: "rec:x".into(), at: 1 },
            DeskRecord::Unsuppressed { record_ref: "rec:x".into(), at: 1 },
        ];
        let mut seen: Vec<String> = Vec::new();
        for r in &samples {
            let tag = tag_of(r);
            // The tag the exhaustive match names is the tag serde actually writes.
            let written = serde_json::to_value(r).unwrap();
            assert_eq!(written.get("rec").unwrap().as_str().unwrap(), tag);
            assert!(KNOWN_DESK_TAGS.contains(&tag), "{tag} is missing from KNOWN_DESK_TAGS");
            seen.push(tag.to_string());
        }
        seen.sort();
        let mut known: Vec<String> = KNOWN_DESK_TAGS.iter().map(|s| s.to_string()).collect();
        known.sort();
        assert_eq!(seen, known, "KNOWN_DESK_TAGS names a tag DeskRecord does not write");
    }

    /// The `Proposed` variant is a NEWTYPE under an internal tag, so its payload is
    /// flattened and `id` sits at the top level of the line. `salvage_proposal_number`
    /// depends on that, and a serde attribute change would break it silently.
    #[test]
    fn a_proposal_number_is_readable_from_the_raw_line_of_every_record_that_names_one() {
        let (mut d, _w, log) = desk("flat-id");
        d.propose("ent", "thr", a_supersede(), "because").unwrap();
        d.confirm("ent", "prop-1").unwrap();
        drop(d);
        let text = std::fs::read_to_string(&log).unwrap();
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        assert_eq!(lines.len(), 3, "proposed, confirmed, written");
        for line in &lines {
            assert_eq!(
                salvage_proposal_number(line),
                Some(1),
                "the proposal number must be readable from the raw line: {line}"
            );
        }
        // And a record that names no proposal yields nothing rather than something wrong.
        assert_eq!(salvage_proposal_number(r#"{"rec":"suppressed","record_ref":"rec:x","at":1}"#), None);
        // Shape is CHECKED, not inferred from a successful parse.
        assert_eq!(proposal_number("prop-+5"), None, "a leading sign is not a proposal number");
        assert_eq!(proposal_number("prop-"), None);
        assert_eq!(proposal_number("proposal-5"), None);
        assert_eq!(proposal_number("prop-5"), Some(5));
        let _ = std::fs::remove_file(log);
    }
}

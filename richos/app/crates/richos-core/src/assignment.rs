//! The existing durable job record, partitioned by company and conversation.
//! Registration records the request and never dispatches it. Stu's canonical inbox
//! carries execution authority. Stu reports progress through the engine work tools;
//! verified completion and the user's durable Stop are the other status sources.
//! Process shutdown, restart and permission answers never infer a job outcome.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct AssignmentError(pub String);

/// Persisted report vocabulary. Historical variants remain readable without migration.
/// Only primary reports and user Stop establish current execution state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AssignmentState {
    /// Request metadata recorded, before Stu reports execution.
    Registered,
    /// A saved report of waiting for access to the screen.
    WaitingForScreen,
    /// A saved report of waiting for provider allowance.
    WaitingForQuota,
    /// A saved report of preparation.
    Preparing,
    /// Stu reported running work or the engine witnessed a worker start.
    Running,
    /// A reported outstanding user decision.
    Blocked,
    /// Verified completion.
    Settled,
    /// An explicit failure report.
    Failed,
    /// User Stop when accompanied by durable provenance. Bare legacy labels are unreported.
    Interrupted,
    /// A historical host inference, never a current report. Restart does not write it.
    Unknown,
}

impl AssignmentState {
    /// Whether a reported state describes unfinished work. For a persisted row,
    /// use Assignment::reported_state first so legacy host inferences stay historical.
    pub fn is_open(self) -> bool {
        matches!(
            self,
            Self::Registered | Self::WaitingForScreen | Self::WaitingForQuota | Self::Preparing | Self::Running | Self::Blocked
        )
    }
    /// Historical classification retained for notice compatibility. Current questions
    /// are read directly from the conversation's question store.
    pub fn awaits_his_word(self) -> bool {
        matches!(self, Self::Blocked | Self::Unknown)
    }
    /// **Is this assignment waiting for something outside the app that will arrive by
    /// itself?** This includes the screen and an automatic allowance hold. It exists so a surface can say *"waiting"*
    /// without having to mean *"waiting for you"*.
    pub fn waits_for_the_world(self) -> bool {
        matches!(self, Self::WaitingForScreen | Self::WaitingForQuota)
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Registered => "registered",
            Self::WaitingForScreen => "waiting-for-screen",
            Self::WaitingForQuota => "waiting-for-quota",
            Self::Preparing => "preparing",
            Self::Running => "running",
            Self::Blocked => "blocked",
            Self::Settled => "settled",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
            // The same word the engine's own receipts use for the same condition
            // (`docs/architecture/desktop-work.md:28-29`), so the two records do not need a
            // translation between them.
            Self::Unknown => "unknown",
        }
    }
}

/// What a notice is ABOUT, so the surface never has to parse a sentence to decide how to
/// treat it. Spec §3.7: a notice must survive being spoken.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NoticeKind {
    /// Spec §0 row 7 / §7.8. The one sentence this whole leg exists to keep apart from "done".
    ReadyToApprove,
    Settled,
    Failed,
    Interrupted,
    /// Spec §6.2's relaunch sentence: it was running when RichOS was last looking and
    /// nothing has witnessed how it ended. A kind of its own because it is neither a
    /// result nor a stop, and a surface that sorted it under either would be telling him
    /// something that is not known.
    Unknown,
    /// **The answer to a question he asked** — the CEO's ruling §58 (`richos-hq`
    /// `wiki/ceo-decisions.md:2884-2914`, 2026-09-18): *"The answer arrives on the timeline
    /// as an answer, never as 'done'."*
    ///
    /// A kind of its own because every other kind above reports what happened to a piece of
    /// WORK, and this one carries what he asked for. A surface that sorted it under
    /// [`Self::Settled`] would put his answer behind the word "finished", which is the one
    /// framing §58 names and refuses.
    Answer,
    /// **His request has not started and will start by itself**: the conversation's queue is
    /// held until nobody-signed-in or a back end that would not open clears (`work_host.rs`,
    /// `Held`). Said once, when the queue is first held. Neither a result nor a stop, so a kind
    /// of its own for the reason [`Self::Unknown`] is one.
    Held,
}

impl NoticeKind {
    /// **What a job ended on, or the answer it brought**: a line that stays true once he has
    /// heard it, so it is written into his conversation (nightly 57's hunt, finding 6: "Will and
    /// Beth have both joined your team" was a card in the window and was gone after a relaunch).
    /// The other kinds say where a job stands now (waiting on his approval, held, not known after
    /// a relaunch), which a later moment can make untrue, so they stay cards.
    pub fn is_result(self) -> bool {
        matches!(self, Self::Answer | Self::Settled | Self::Failed | Self::Interrupted)
    }
}

/// Rich's classification of delegated work. This is retained as work metadata and
/// does not select a workflow or compose conversational replies.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AssignmentKind {
    /// Requested work.
    #[default]
    Task,
    /// A delegated lookup.
    Check,
    /// A delegated investigation.
    Investigate,
}

impl AssignmentKind {
    /// Is this a question of his rather than a piece of work?
    ///
    /// **Everything that behaves differently for a question branches on THIS**, never on
    /// `Check` or `Investigate` separately: the two differ only in their recorded classification and
    /// the word beside the timer. What the back end is asked to do, and how its answer comes
    /// back, is the same for both.
    pub fn is_question(self) -> bool {
        matches!(self, Self::Check | Self::Investigate)
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Check => "check",
            Self::Investigate => "investigate",
        }
    }
    /// Parse the one word the model reports. Anything unrecognized is refused rather than
    /// defaulted: a mistyped kind must not silently change a question into a task.
    pub fn parse(word: &str) -> Result<Self, AssignmentError> {
        match word {
            "task" => Ok(Self::Task),
            "check" => Ok(Self::Check),
            "investigate" => Ok(Self::Investigate),
            _ => Err(AssignmentError(
                "an assignment is a task, a check or an investigate, and nothing else".into(),
            )),
        }
    }
}

/// One thing to say to him, held on disk until he has been told.
///
/// **`delivered_at_ms` is why this is a record and not an event.** Spec §3.4: a result that
/// lands while he is away must be shown when he comes back, and §2.4a makes that survive a
/// relaunch — so the flag that says "he has seen this" has to be as durable as the notice.
/// A notice held in memory is lost in exactly the case the spec was written for.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Notice {
    pub kind: NoticeKind,
    /// The sentence itself. Spoken-safe, no identifiers, no counts he cannot act on.
    pub text: String,
    pub raised_at_ms: u64,
    #[serde(default)]
    pub delivered_at_ms: Option<u64>,
}

/// The persisted assignment. `schema` is checked on every read: a record written by a
/// newer RichOS is refused, never half-understood.
///
/// The app has no arbitrary execution-state writer:
/// ```compile_fail,E0425
/// richos_core::assignment::advance(std::path::Path::new("."), "company", "thread", "job",
///     richos_core::assignment::AssignmentState::Interrupted, "Quit");
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Assignment {
    pub schema: u32,
    pub id: String,
    /// **The ECS obligation this assignment is the work of**, and the engine's join key for
    /// everything about it. `richos_work.prepare` refuses without an accepted open
    /// obligation (`richos/engine/mega-lander/app.py:267-270`), and the seat reconciler
    /// maps a seat back to its assignment through exactly this value
    /// (`app.py:704`: `assignment, seat = row["turn_id"], row["person_id"]`).
    pub obligation_id: String,
    /// The ECS seat this assignment's work runs on — spec §5.8c, one seat per assignment,
    /// created with it and released with it.
    ///
    /// **Its spelling is `work-seat:<obligation_id>` and that is a contract, not a
    /// preference.** The engine reads two fields off the seat row to reconcile it
    /// (`app.py:700-704`): `audience` must be `worker`, and `turn_id` must be the
    /// obligation. The person spelling is the app's, and this is the one the engine's own
    /// tests use, so a seat written here is a seat that reconciler recognizes.
    pub seat: String,
    pub entity_id: String,
    pub thread_id: String,
    /// The turn in which he gave it. Frozen for the life of the assignment (spec §3.6):
    /// the engine's completion gate wants an instruction that really was his and visible,
    /// not one that is live this second.
    pub instruction_ledger_ref: String,
    pub instruction_sha256: String,
    /// His assignment in his own terms. Bounded and single-line; see [`sanitize_title`].
    pub title: String,
    /// Rich's complete handoff. A display title is never an execution contract.
    #[serde(default)]
    pub handoff: String,
    /// **Work he asked for, or a question he asked** — the CEO's ruling §58.
    ///
    /// `#[serde(default)]` so every record written before 2026-09-18 reads back as
    /// [`AssignmentKind::Task`], which is what all of them are. It is deliberately NOT a
    /// schema bump: the schema number exists to refuse a record this build cannot
    /// understand, and a missing `kind` is one this build understands exactly. A record
    /// written HERE and read by an older RichOS is refused by that build's
    /// `deny_unknown_fields` with *"written by a newer RichOS"*, which is the honest answer
    /// and the one that path was built to give.
    #[serde(default)]
    pub kind: AssignmentKind,
    pub repositories: Vec<String>,
    pub state: AssignmentState,
    /// What the state is waiting on or stopped by. Never a stack trace.
    pub detail: String,
    pub registered_at_ms: u64,
    pub updated_at_ms: u64,
    /// A user Stop request, independent of whether provider cancellation succeeded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_stop_requested_at_ms: Option<u64>,
    #[serde(default)]
    pub notices: Vec<Notice>,
    /// **The back end's session id, written down when this assignment went on the lease.**
    ///
    /// Spec §6.1 reconciles receipts against the EVIDENCE FILE, and that file is keyed by
    /// session (`app_workers.rs:38`). Until this field existed the work lease's session id
    /// lived only in the running process (`work_host.rs`'s `Inner::lease_session`), so a
    /// relaunch had nothing to re-witness against and *"reconciled against the evidence
    /// file"* was a sentence with no path behind it. `None` means this assignment never
    /// reached a lease — which is itself a positive fact about it, not a gap.
    #[serde(default)]
    pub work_session: Option<String>,
    /// **Where his repositories stood when this assignment started**, so §6.1's *"reconciled
    /// against Git"* is a comparison rather than a look.
    ///
    /// Written on the work lease after his turn has ended, never during it (§1.1: the turn
    /// boundary is one file write). A pin is evidence in one direction only: an unmoved
    /// branch is positive proof that nothing was landed — which is the assertion §7.8 asks
    /// for in Git rather than on the screen — and a moved one proves only that the
    /// repository changed, never that this assignment is what changed it.
    #[serde(default)]
    pub repository_pins: Vec<RepositoryPin>,
    /// **Does this assignment's work need the Mac's screen?** The CEO's ruling §56's one
    /// input, and the reason the app can wait for an unlock without polling the screen for
    /// work that does not care about it.
    ///
    /// **Nothing infers it, and nothing can.** The app cannot tell from a title whether a job
    /// will drive a window, take a screenshot or walk a build, so this is declared by the
    /// conversation that writes the assignment down (`assignment_tools.rs`'s `needs_screen`)
    /// — the same shape as `after_questions`, which is also a fact only the model knows. The
    /// cost of getting it wrong is bounded in both directions and neither direction loses
    /// work: a false `true` waits for a screen it did not need, a false `false` runs on a
    /// locked screen exactly as this app did before §56.
    ///
    /// **`serde(default)` keeps `schema` at 1 on purpose.** An assignment written before this
    /// field existed reads back as `false`, which is the truthful answer for it — no old job
    /// declared a screen need. The reverse direction is already this module's documented
    /// behavior: an older RichOS meeting a record with this field refuses it as *"written by a
    /// newer RichOS"* rather than half-understanding it, which is what `deny_unknown_fields`
    /// is there for.
    #[serde(default)]
    pub needs_screen: bool,
    /// **`detail` is Rich's waiting sentence while a teammate works** ("I've asked Will for
    /// advice, and he's working on it now."), written by Stu's work report and cleared by
    /// every other Stu's work report. His row shows it in both views, for a task and a question
    /// alike (`work_host.rs`'s `for_him`; the review of 3b91f1223, finding 1): it is free text
    /// from the back end, so no fixed list of sentences can recognize it. Written only while
    /// it is true, so every other record keeps its shape.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub waiting_sentence: bool,
}

/// **The record's detail for an assignment that was ANSWERED rather than worked on** — a
/// question of his (CEO ruling §58), or a task the back end carried out itself with no helper
/// (esc-20260927T093052Z-85f3303f). Never spoken: what he hears is the answer itself, on his
/// timeline. One constant because the pane and the status read decide their wording on it.
pub const ANSWERED_DETAIL: &str = "Answered.";

impl Assignment {
    /// User Stop has durable provenance. Old host-written interruptions are not current reports.
    pub fn reported_state(&self)->Option<AssignmentState> {
        if self.user_stop_requested_at_ms.is_some() {return Some(AssignmentState::Interrupted)}
        match self.state {AssignmentState::Unknown|AssignmentState::Interrupted=>None,state=>Some(state)}
    }

    /// Unverified old process records cannot revoke the user's pending work or answers.
    pub fn accepts_input(&self)->bool {
        !matches!(self.reported_state(),Some(AssignmentState::Settled|AssignmentState::Failed|AssignmentState::Interrupted))
    }

    /// **Was this closed by an answer on his timeline rather than by work that finished?**
    ///
    /// A settled QUESTION always was. A settled TASK was only when the back end did it itself
    /// and reported in its own words — and for that task "Finished." would be a claim the host
    /// never witnessed (it may be the back end telling him what it needs before it can begin),
    /// so the surfaces say what is true of both: the answer is in his conversation.
    pub fn was_answered(&self) -> bool {
        self.state == AssignmentState::Settled && (self.kind.is_question() || self.detail == ANSWERED_DETAIL)
    }
}

/// One repository, as it stood when an assignment started.
///
/// **`head` is what `git rev-parse HEAD` answered, verbatim, or `None` when it could not be
/// read** — an unreadable repository is recorded as unreadable rather than as unchanged,
/// because "I could not look" and "nothing moved" are the two answers this whole module
/// exists to keep apart.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RepositoryPin {
    pub path: String,
    pub head: Option<String>,
}

/// What a caller must supply to register. Everything else is derived here, so a model
/// cannot choose an id, a seat, a company or an instruction.
///
/// **The instruction arrives already attested, and that is deliberate.** The digest is of
/// the CEO's exact ledger text and is computed by the host at turn start
/// (`native.rs`'s `prepare_work_turn`); nothing here recomputes it from a string a model
/// supplied, and his words are not copied into a second file to make that possible.
#[derive(Clone, Debug)]
pub struct Registration {
    pub entity_id: String,
    pub thread_id: String,
    /// The ECS obligation this assignment carries out. Opened by the conversation before
    /// it records the assignment; the engine refuses a dispatch without an accepted open
    /// one (`mega-lander/app.py:267-270`).
    pub obligation_id: String,
    /// `ledger:<thread>:<turn>` for the turn he gave the assignment in (spec §3.6).
    pub instruction_ledger_ref: String,
    /// The host's SHA-256 of that turn's exact ledger text.
    pub instruction_sha256: String,
    pub title: String,
    pub repositories: Vec<String>,
    /// Whether this work needs the Mac's screen — see [`Assignment::needs_screen`]. Declared
    /// by the conversation, never inferred here.
    pub needs_screen: bool,
}

/// What the turn ends with.
pub struct Receipt {
    pub id: String,
    pub title: String,
    /// Which of the three sentences [`Self::sentence`] hands back. Set by the register from
    /// what was actually written down, never by the caller — so the reply he hears and the
    /// record on disk cannot be two different kinds of thing.
    pub kind: AssignmentKind,
}

// **Where Ray's row 8 double period went, and why there is no helper here for it.**
//
// He saw *"… and land it.. It's running now"*: the model's title ended in a period and the
// receipt put its own straight after it. Two things removed that, and between them nothing in
// this module now writes a period directly after a title:
//
//   1. [`sanitize_title`] strips a trailing period at the source. That is the production fix,
//      and it is the one that matters for the seven sentences in [`says`] — a title of *"land
//      it."* used to make every one of them read *"land it. is finished."*
//   2. The CEO's ruling §55 took the title out of the receipt altogether, which was the only
//      sentence in the module that appended punctuation to one.
//
// A first pass here added an `after_title` helper for the case `sanitize_title` deliberately
// leaves — a truncated title keeps the ellipsis it needs. It was dropped at `25b933f2` on the
// reasoning that *"every sentence in [`says`] continues with a word rather than a mark, so an
// ellipsis is followed by a space and reads correctly"*.
//
// **THAT REASONING WAS WRONG, AND RAY'S CANDIDATE-.8 ROW 8 IS IT BEING WRONG ON HIS SCREEN**
// (`docs/verification/2026-09-18-nightly-1.2.0-20260918.2-onscreen-audit.md` §1 row 8), verbatim:
//
//   "…and land it. (Third try; the first two stopped before they… stopped before it finished."
//
// An ellipsis is TERMINAL punctuation. A lowercase verb phrase running straight on from one does
// not read as a continuation; it reads as a sentence that broke off and restarted — and it reads
// that way aloud, which is the half that matters on a voice-first surface. The join is back, as
// [`says::continues`], for the case the old one was written for and now against the evidence that
// the case is real rather than hypothetical. It is still ONE funnel and not seven, for the same
// reason [`sanitize_title`] strips the period once. The property is asserted over every sentence
// in [`says`], which is what catches the eighth one somebody adds.

/// Spec §1.4: *"A failed registration is never softened into 'I have started it'."*
///
/// It says "write down" for the same reason [`Receipt::sentence`] does: the two sentences are
/// the success and the failure of one act, and he should not hear the act called two things.
pub fn failed_registration_sentence(why: &str) -> String {
    format!("I could not write that assignment down: {why} Nothing is running and nothing was started.")
}

/// One line, bounded, no control characters — a title travels into a spoken sentence and
/// onto his timeline, and it comes from the model.
///
/// The cap is 160 characters because the sentence it lands in is read aloud; a title that
/// has to be truncated is truncated on a word boundary and ends in an ellipsis, so the
/// spoken form never trails off mid-word.
///
/// **A TRAILING PERIOD IS STRIPPED, because a title is never the end of a sentence.** Every
/// sentence that embeds one — [`Receipt::sentence`] and all seven in [`says`] — continues
/// talking after it (*"{title} is finished"*, *"{title} stopped before it finished"*), so a
/// title carrying its own period produced *"… and land it.. It's running now"* on his
/// timeline, which is what Ray's candidate-.7 walk found at row 8. It is stripped HERE, once,
/// rather than at each of the eight joins: a fix at one join would have left the other seven
/// reading *"and land it. is finished."*
///
/// The truncation path is deliberately left ending in `…`, which is terminal punctuation it
/// needs; [`after_title`] is what keeps a second mark off that one.
pub fn sanitize_title(raw: &str) -> Result<String, AssignmentError> {
    let line = sanitize_line(raw)?;
    // A truncated line already ends in the ellipsis the truncation needs, and that is
    // terminal punctuation to keep — [`after_title`] is what stops a second mark landing
    // after it.
    if line.ends_with('…') {
        return Ok(line);
    }
    let stripped = line.trim_end_matches('.').trim_end();
    // A "title" that was nothing but punctuation is refused rather than silently emptied —
    // an empty title would be read aloud as a missing clause.
    if stripped.is_empty() {
        return Err(AssignmentError("an assignment needs a title in the CEO's own terms".into()));
    }
    Ok(stripped.to_string())
}

/// Flatten to one bounded line, and nothing else.
///
/// **This is the half a DETAIL wants, and the reason the two are now separate functions.**
/// A detail is a whole sentence — *"The review asked for changes, so nothing was landed."* —
/// and it keeps its period. A title is a noun phrase that something else always says more
/// after, so it loses one. Both were [`sanitize_title`] until 2026-09-18, and stripping the
/// period for the title's sake took the period off three of `work_host`'s outcome sentences
/// before the tests caught it.
fn sanitize_line(raw: &str) -> Result<String, AssignmentError> {
    let flattened: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if flattened.is_empty() {
        return Err(AssignmentError("an assignment needs a title in the CEO's own terms".into()));
    }
    if flattened.chars().count() <= 160 {
        return Ok(flattened);
    }
    let mut cut: String = flattened.chars().take(160).collect();
    if let Some(space) = cut.rfind(' ') {
        cut.truncate(space);
    }
    Ok(format!("{}…", cut.trim_end_matches(['.', ',', ';', ':', ' '])))
}

fn partition(entity: &str, thread: &str) -> Result<String, AssignmentError> {
    use sha2::{Digest, Sha256};
    let identity = serde_json::to_vec(&[entity, thread]).map_err(|e| AssignmentError(e.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(identity)))
}

fn folder(state: &Path, entity: &str, thread: &str) -> Result<PathBuf, AssignmentError> {
    let parent = state.join("assignments");
    let root = parent.join(partition(entity, thread)?);
    if parent.is_symlink() || root.is_symlink() {
        return Err(AssignmentError("Assignment records have been redirected.".into()));
    }
    Ok(root)
}

/// Atomic, fsynced, 0600. Same shape as `ecs::write_scope`, and for the same reason: a
/// half-written assignment is a request the app cannot honor and cannot report.
fn write(path: &Path, value: &Assignment) -> Result<(), AssignmentError> {
    use std::io::Write;
    let bytes = serde_json::to_vec(value).map_err(|e| AssignmentError(e.to_string()))?;
    let parent = path.parent().ok_or_else(|| AssignmentError("assignment has no parent".into()))?;
    std::fs::create_dir_all(parent).map_err(|e| AssignmentError(e.to_string()))?;
    let temporary = parent.join(format!(".assignment-{}.incoming", uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> std::io::Result<()> {
        let mut file = options.open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        std::fs::rename(&temporary, path)?;
        std::fs::File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(|e| AssignmentError(e.to_string()))
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Bounded, ascii-safe identity — the same alphabet `app_workers::status` demands of a
/// session id, so a seat can never be a path component with a surprise in it.
fn usable_identity(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// **The whole of the turn boundary.** One validation pass, one file, one fsync.
///
/// Nothing here spawns, prepares, binds or waits. The caller gets a [`Receipt`] and ends
/// the turn with it (spec §1.1); the work host picks the assignment up afterwards.
pub fn register(state: &Path, request: &Registration) -> Result<Receipt, AssignmentError> {
    register_kind(state, request, AssignmentKind::Task)
}

/// The same turn boundary, for a QUESTION of his as well as a piece of work — the CEO's
/// ruling §58, 2026-09-18.
///
/// **It is a second entry point rather than a field on [`Registration`], and the reason is
/// blast radius.** `Registration` is a struct literal in six places across four modules, three
/// of which belong to other slices in flight today. A field would have edited all six to say
/// "task" and given this change a merge conflict in every one of them; this gives every
/// existing caller the behavior it already had, with no edit at all, and the one caller that
/// needs the choice makes it explicitly.
///
/// Nothing else differs. The same validation, the same one-open-assignment-per-obligation
/// refusal, the same single fsynced file. A question is a piece of work the back end does
/// and reports on; what changes is the sentence he hears, the word beside the timer, and how
/// the result comes back.
pub fn register_kind(
    state: &Path,
    request: &Registration,
    kind: AssignmentKind,
) -> Result<Receipt, AssignmentError> {
    if !usable_identity(&request.thread_id) {
        return Err(AssignmentError("this conversation cannot be identified".into()));
    }
    if serde_json::to_string(&request.title).map_err(|e| AssignmentError(e.to_string()))?.len() > 64 * 1024 {
        return Err(AssignmentError("the complete handoff is too large to record".into()));
    }
    if request.entity_id.is_empty() || request.entity_id.len() > 128 {
        return Err(AssignmentError("this conversation has no company binding".into()));
    }
    // The instruction must be the host's, attested, and from a turn — spec §3.6's whole
    // point is that the reference is real and frozen, not that it is present.
    if !request.instruction_ledger_ref.starts_with(&format!("ledger:{}:", request.thread_id))
        || request.instruction_ledger_ref.len() > 512
    {
        return Err(AssignmentError("this assignment has no visible turn behind it".into()));
    }
    if request.instruction_sha256.len() != 64
        || !request.instruction_sha256.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(AssignmentError("this assignment has no attested instruction behind it".into()));
    }
    // The obligation is the engine's join key and becomes the seat's `turn_id`, so it must
    // be a plain bounded identifier. A colon is refused because the seat is spelled
    // `work-seat:<obligation_id>` and an obligation carrying one would make that spelling
    // ambiguous to read back.
    if !usable_identity(&request.obligation_id) {
        return Err(AssignmentError("this assignment names no obligation to carry out".into()));
    }
    if request.repositories.len() > 32 {
        return Err(AssignmentError("that is more repositories than one assignment can name".into()));
    }
    for repository in &request.repositories {
        if repository.is_empty() || repository.len() > 4096 || repository.contains(['\n', '\0']) {
            return Err(AssignmentError("a named repository is not usable".into()));
        }
    }
    let title = sanitize_title(&request.title)?;
    // **One OPEN assignment per obligation, refused here rather than discovered later.**
    //
    // The seat is `work-seat:<obligation_id>` because the engine's reconciler reads the
    // seat row's `turn_id` as the obligation (`mega-lander/app.py:704`) — so the seat's
    // cardinality is the obligation's. Two open assignments on one obligation would be one
    // seat shared by two, which is spec §5.8c's reproduced failure exactly: the table is
    // keyed by person, registering the second upserts the first one's cursor, and the
    // first's frozen binding is stale from then on.
    //
    // The engine makes the same refusal one level down — *"this obligation already has
    // unresolved work; inspect its receipt before retrying"* (`app.py:314-317`) — so this
    // agrees with it rather than inventing a second rule.
    if read_all(state, &request.entity_id, &request.thread_id)?
        .iter()
        .any(|row| row.obligation_id == request.obligation_id && row.state.is_open())
    {
        return Err(AssignmentError(
            "there is already an assignment running for that piece of work".into(),
        ));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let at = now_ms();
    let record = Assignment {
        schema: 1,
        // `work-seat:<obligation_id>` — the spelling the engine's reconciler and its own
        // tests use (`mega-lander/app.py:700-714`).
        seat: format!("work-seat:{}", request.obligation_id),
        obligation_id: request.obligation_id.clone(),
        id: id.clone(),
        entity_id: request.entity_id.clone(),
        thread_id: request.thread_id.clone(),
        instruction_ledger_ref: request.instruction_ledger_ref.clone(),
        instruction_sha256: request.instruction_sha256.clone(),
        title: title.clone(),
        handoff: request.title.clone(),
        kind,
        repositories: request.repositories.clone(),
        state: AssignmentState::Registered,
        // **A question has not been "prepared" either, and it is not waiting to be.** The
        // word he would hear about this state comes from `status_tools.rs`'s `starting`
        // section and from the timer, not from this string; what this says is what is true
        // of both — it is written down and the back end has not been asked yet.
        detail: if kind.is_question() {
            // **No seam in it.** The work details around here say "the back end", which is
            // fine on a work row: the saved-work pane appends a work row's detail and never a
            // question's, so a question's detail is read only by the front desk — and the
            // front desk may say it out loud. It is the pane's own question wording, so the
            // two cannot describe one state two ways.
            "Written down. Rich hasn't started looking yet.".into()
        } else {
            "Written down. Preparation has not started.".to_string()
        },
        registered_at_ms: at,
        updated_at_ms: at,
        user_stop_requested_at_ms: None,
        notices: Vec::new(),
        // Nothing has been on a lease and nothing has been looked at in Git. Both are
        // written later, on the work lease, after this turn has ended (§7.1).
        work_session: None,
        repository_pins: Vec::new(),
        needs_screen: request.needs_screen,
        waiting_sentence: false,
    };
    let root = folder(state, &request.entity_id, &request.thread_id)?;
    write(&root.join(format!("{id}.json")), &record)?;
    Ok(Receipt { id, title, kind })
}

fn read_one(path: &Path, entity: &str, thread: &str) -> Result<Assignment, AssignmentError> {
    let size = std::fs::symlink_metadata(path)
        .map_err(|_| AssignmentError("An assignment record could not be read.".into()))?;
    if !size.is_file() || size.len() > 256 * 1024 {
        return Err(AssignmentError("An assignment record is too large or redirected.".into()));
    }
    let bytes =
        std::fs::read(path).map_err(|_| AssignmentError("An assignment record could not be read.".into()))?;
    let record: Assignment = serde_json::from_slice(&bytes)
        .map_err(|_| AssignmentError("An assignment record is damaged or was written by a newer RichOS.".into()))?;
    if record.schema != 1 || record.entity_id != entity || record.thread_id != thread {
        return Err(AssignmentError("An assignment record has an unsupported schema or different scope.".into()));
    }
    Ok(record)
}

/// Every assignment on this thread, oldest first. A damaged record is an error, never an
/// omission — spec §6.1's rule that unreadable evidence is reported, not counted as zero.
pub fn read_all(state: &Path, entity: &str, thread: &str) -> Result<Vec<Assignment>, AssignmentError> {
    let scan = scan_all(state, entity, thread)?;
    match scan.unreadable.into_iter().next() {
        Some(error) => Err(error),
        None => Ok(scan.rows),
    }
}

/// **What a walk of the register could read, and what it could not** (hunt part 1 finding
/// 08, richos-hq `docs/audits/2026-09-29-hunt/part-1-codex.md`). One damaged record used to
/// end the whole walk, so a single broken file of long-finished work stopped recovery, and
/// the delivery of what recovery raised, for every healthy open assignment beside it. A scan
/// keeps both halves: the rows it read, and one error for each record it could not. It never
/// drops the second half, so a caller cannot read "nothing unreadable" into it by accident.
#[derive(Debug, Default)]
pub struct Scan {
    pub rows: Vec<Assignment>,
    pub unreadable: Vec<AssignmentError>,
}

/// [`read_all`], keeping every record it can read beside the ones it cannot. An `Err` is only
/// a folder that could not be listed at all. Callers that must never mistake a partial list
/// for a whole one keep using [`read_all`].
pub fn scan_all(state: &Path, entity: &str, thread: &str) -> Result<Scan, AssignmentError> {
    let root = folder(state, entity, thread)?;
    let entries = match std::fs::read_dir(&root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Scan::default()),
        Err(_) => return Err(AssignmentError("Assignments could not be read.".into())),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry.map_err(|_| AssignmentError("Assignments could not be listed.".into()))?.path();
        if path.extension().is_some_and(|e| e == "json") {
            paths.push(path);
        }
        if paths.len() > 10000 {
            return Err(AssignmentError(
                "There are too many assignments on this conversation to read at once.".into(),
            ));
        }
    }
    paths.sort();
    let mut scan = Scan::default();
    for path in paths {
        match read_one(&path, entity, thread) {
            Ok(row) => scan.rows.push(row),
            Err(error) => scan.unreadable.push(error),
        }
    }
    scan.rows.sort_by_key(|row| (row.registered_at_ms, row.id.clone()));
    Ok(scan)
}

pub fn read(state: &Path, entity: &str, thread: &str, id: &str) -> Result<Assignment, AssignmentError> {
    if !usable_identity(id) {
        return Err(AssignmentError("that assignment cannot be identified".into()));
    }
    read_one(&folder(state, entity, thread)?.join(format!("{id}.json")), entity, thread)
}

/// Read, change, write — the one mutation path, so every state change goes through the
/// schema and scope checks on the way in.
///
/// **IT DOES NOT TOUCH `updated_at_ms`, AND THAT IS RAY'S CANDIDATE-.8 ROW 9.** It used to
/// stamp `now_ms()` on every call, which meant any write bumped it — including
/// [`take_pending_notices`], whose whole job is to mark a notice delivered and which changes
/// nothing about the assignment itself. Measured on his walk
/// (`docs/verification/2026-09-18-nightly-1.2.0-20260918.2-onscreen-audit.md` §1 row 9): job 1
/// failed at `10:35:49.190Z`, **6.280 s** after its `10:35:42.910Z` registration; after the
/// relaunch delivered its notice the record read `10:39:47.649Z`, **244.739 s**. Both figures
/// re-derived from his timestamps here. He only had the true one because he read the file live.
///
/// **The field is consumed as "when did this last MOVE".** `status_tools.rs:138` publishes it
/// to the front desk's model under the name `last_moved_at_ms`, and `:195` SORTS the list by
/// it — so a delivery did not merely record a wrong elapsed time, it reordered "what moved most
/// recently" by the order notices happened to be handed over.
///
/// Movement timestamps belong to primary reports and controls. Notice delivery has
/// its own timestamp and never changes when a job last progressed.
fn update(
    state: &Path,
    entity: &str,
    thread: &str,
    id: &str,
    change: impl FnOnce(&mut Assignment),
) -> Result<Assignment, AssignmentError> {
    // Atomic replacement alone cannot prevent two writers losing one another's updates.
    #[cfg(unix)]
    let _lock = {
        use std::os::{fd::AsRawFd, unix::fs::OpenOptionsExt};
        let root = folder(state, entity, thread)?;
        let file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false)
            .mode(0o600).custom_flags(libc::O_NOFOLLOW).open(root.join(".updates.lock"))
            .map_err(|e| AssignmentError(e.to_string()))?;
        // SAFETY: the descriptor belongs to this guard and closes after the update.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(AssignmentError(std::io::Error::last_os_error().to_string()));
        }
        file
    };
    #[cfg(not(unix))]
    let _lock = {
        static UPDATES: std::sync::Mutex<()> = std::sync::Mutex::new(());
        UPDATES.lock().unwrap()
    };
    let mut record = read(state, entity, thread, id)?;
    change(&mut record);
    write(&folder(state, entity, thread)?.join(format!("{id}.json")), &record)?;
    Ok(record)
}

/// Record the user's control before cancellation. Provider observations may refine
/// the detail but cannot erase or override this Stop.
pub fn record_user_stop(state: &Path, entity: &str, thread: &str, id: &str) -> Result<Assignment, AssignmentError> {
    update(state, entity, thread, id, |record| {
        let at=*record.user_stop_requested_at_ms.get_or_insert_with(crate::util::now_millis);
        record.state=AssignmentState::Interrupted;
        record.detail="Stop requested by the user.".into();
        record.updated_at_ms=at;
        record.waiting_sentence=false;
    })
}

/// The native provider's response to an already durable user Stop. No execution state is inferred.
pub fn record_stop_observation(state:&Path,entity:&str,thread:&str,id:&str,detail:&str)->Result<Assignment,AssignmentError> {
    if read(state,entity,thread,id)?.user_stop_requested_at_ms.is_none() {
        return Err(AssignmentError("No user Stop was recorded".into()))
    }
    let detail=sanitize_line(detail)?;
    update(state,entity,thread,id,|record|{record.detail=detail;})
}

/// **The app's own screen wait, the CEO's ruling §56** (`work_host.rs`, `held_for_screen`).
/// `waiting` moves a job that declared [`Assignment::needs_screen`] from `Registered` to
/// `WaitingForScreen`; `!waiting` moves it back once the screen is usable, with the sentence
/// that says it is carrying on. Nothing else moves: a job that has started, finished or been
/// stopped keeps its record exactly as it is.
pub fn record_screen_wait(state: &Path, entity: &str, thread: &str, id: &str, waiting: bool) -> Result<Assignment, AssignmentError> {
    update(state, entity, thread, id, |record| {
        let (from, to, detail) = if waiting {
            (AssignmentState::Registered, AssignmentState::WaitingForScreen, crate::screen::says::detail())
        } else {
            (AssignmentState::WaitingForScreen, AssignmentState::Registered, crate::screen::says::carrying_on())
        };
        if record.needs_screen && record.user_stop_requested_at_ms.is_none() && record.state == from {
            record.state = to;
            record.detail = detail.into();
            record.updated_at_ms = now_ms();
        }
    })
}

#[cfg(test)]
pub fn advance(
    state: &Path,
    entity: &str,
    thread: &str,
    id: &str,
    to: AssignmentState,
    detail: &str,
) -> Result<Assignment, AssignmentError> {
    move_to(state, entity, thread, id, to, detail, false)
}

#[cfg(test)]
fn move_to(
    state: &Path,
    entity: &str,
    thread: &str,
    id: &str,
    to: AssignmentState,
    detail: &str,
    waiting_sentence: bool,
) -> Result<Assignment, AssignmentError> {
    // A detail is a whole sentence and keeps its own period — see [`sanitize_line`].
    let detail = sanitize_line(detail).unwrap_or_else(|_| "No detail was recorded.".into());
    update(state, entity, thread, id, |record| {
        record.state = to;
        record.detail = detail;
        record.waiting_sentence = waiting_sentence;
        // **THE ONLY PLACE `updated_at_ms` IS STAMPED** — see [`update`] for Ray's row 9 and
        // the two numbers. This is the only function in the module that writes `record.state`,
        // so a time stamped here is a time the assignment actually moved, which is what
        // `status_tools.rs` publishes it as (`last_moved_at_ms`) and sorts the list by.
        //
        // **Stamped on a detail-only `advance` too, deliberately.** A detail is the sentence
        // the front desk would read him about this job; a job that is still `Running` but is
        // now waiting on something else HAS moved as far as he is concerned. What must never
        // touch it is a write that changes nothing he could be told — which is delivery, and
        // delivery does not come through here.
        record.updated_at_ms = now_ms();
    })
}

/// **Write down which back end took this assignment, and where his repositories stood.**
///
/// Called once, on the work lease, the moment the assignment is bound to it — after his
/// turn has ended (§7.1), so neither the session lookup nor the Git reads are on the
/// boundary his *"answer in seconds"* is measured at.
///
/// Both halves are what a relaunch reconciles against (§6.1). Failing to write them is not
/// fatal to the work: the caller logs and carries on, and recovery then reports honestly
/// that it has nothing to re-witness against — which is the truth in that case.
pub fn note_start(
    state: &Path,
    entity: &str,
    thread: &str,
    id: &str,
    session: &str,
    pins: Vec<RepositoryPin>,
) -> Result<Assignment, AssignmentError> {
    if !usable_identity(session) {
        return Err(AssignmentError("that work connection cannot be identified".into()));
    }
    update(state, entity, thread, id, move |record| {
        record.work_session = Some(session.to_string());
        record.repository_pins = pins;
    })
}

/// Raise the one thing to say to him about this assignment. Held until delivered.
pub fn raise_notice(
    state: &Path,
    entity: &str,
    thread: &str,
    id: &str,
    kind: NoticeKind,
    text: &str,
) -> Result<Notice, AssignmentError> {
    let notice = Notice { kind, text: text.to_string(), raised_at_ms: now_ms(), delivered_at_ms: None };
    let held = notice.clone();
    update(state, entity, thread, id, move |record| {
        // Bounded: an assignment that somehow raised thousands of notices is a defect, and
        // the newest are the ones he needs.
        if record.notices.len() >= 64 {
            record.notices.remove(0);
        }
        record.notices.push(held);
    })?;
    Ok(notice)
}

/// One notice, with the assignment it belongs to, on its way to his screen.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingNotice {
    pub assignment_id: String,
    pub title: String,
    pub kind: NoticeKind,
    pub text: String,
    pub raised_at_ms: u64,
}

impl PendingNotice {
    /// Stable across delivery retries and independent of the assignment's display title.
    pub fn delivery_id(&self, entity: &str, thread: &str) -> String {
        use sha2::{Digest, Sha256};
        let identity = serde_json::to_vec(&(entity, thread, &self.assignment_id,
            self.kind, self.raised_at_ms, &self.text)).expect("notice fields serialize");
        format!("work-report-{:x}", Sha256::digest(identity))
    }
}

/// Read without consuming. The conversation acknowledges a report only after its reply is durable.
pub fn pending_notices_of(
    state: &Path, entity: &str, thread: &str, wanted: impl Fn(NoticeKind) -> bool,
) -> Result<Vec<PendingNotice>, AssignmentError> {
    let scan = scan_all(state, entity, thread)?;
    for error in &scan.unreadable {
        eprintln!("[richos] assignments: a record could not be read, so its notices wait: {error}");
    }
    let mut pending = Vec::new();
    for row in scan.rows {
        for notice in row.notices.into_iter().filter(|n| n.delivered_at_ms.is_none() && wanted(n.kind)) {
            pending.push(PendingNotice { assignment_id: row.id.clone(), title: row.title.clone(),
                kind: notice.kind, text: notice.text, raised_at_ms: notice.raised_at_ms });
        }
    }
    pending.sort_by_key(|n| n.raised_at_ms);
    Ok(pending)
}

pub fn acknowledge_notice(
    state: &Path, entity: &str, thread: &str, delivered: &PendingNotice,
) -> Result<(), AssignmentError> {
    update(state, entity, thread, &delivered.assignment_id, |record| {
        for notice in &mut record.notices {
            if notice.kind == delivered.kind && notice.raised_at_ms == delivered.raised_at_ms
                && notice.text == delivered.text && notice.delivered_at_ms.is_none() {
                notice.delivered_at_ms = Some(now_ms());
            }
        }
    })?;
    Ok(())
}

/// Everything he has not been told yet, oldest first — **and it marks them delivered.**
///
/// This is the half of spec §3.4 that makes a notice survive a relaunch: the surface reads
/// this on launch as well as on a live event, and the durable flag is what stops him being
/// told the same thing twice. It is deliberately a single call that both reads and marks,
/// because a read that leaves the marking to the caller loses the notice the moment the
/// caller is the process that exits.
pub fn take_pending_notices(
    state: &Path,
    entity: &str,
    thread: &str,
) -> Result<Vec<PendingNotice>, AssignmentError> {
    take_pending_notices_of(state, entity, thread, |_| true)
}

/// [`take_pending_notices`] for the notices whose kind `wanted` takes, and only those: the
/// others stay pending for the read that wants them. The conversation takes a job's results
/// to write them into itself ([`NoticeKind::is_result`]), the window the rest as cards.
pub fn take_pending_notices_of(
    state: &Path,
    entity: &str,
    thread: &str,
    wanted: impl Fn(NoticeKind) -> bool,
) -> Result<Vec<PendingNotice>, AssignmentError> {
    // Every notice that can be read is handed over; a damaged record's cannot be, and it is
    // said on stderr rather than taking the healthy ones with it (finding 08). The boot
    // reconciliation reports it as unreadable in its own line.
    let scan = scan_all(state, entity, thread)?;
    for error in &scan.unreadable {
        eprintln!("[richos] assignments: a record on this conversation could not be read, so its notices wait: {error}");
    }
    let rows = scan.rows;
    let at = now_ms();
    let mut pending = Vec::new();
    for row in rows {
        if row.notices.iter().all(|n| n.delivered_at_ms.is_some() || !wanted(n.kind)) {
            continue;
        }
        let title = row.title.clone();
        let id = row.id.clone();
        let updated = update(state, entity, thread, &row.id, |record| {
            for notice in record.notices.iter_mut().filter(|n| n.delivered_at_ms.is_none() && wanted(n.kind)) {
                notice.delivered_at_ms = Some(at);
            }
        })?;
        for notice in updated.notices.iter().filter(|n| n.delivered_at_ms == Some(at)) {
            pending.push(PendingNotice {
                assignment_id: id.clone(),
                title: title.clone(),
                kind: notice.kind,
                text: notice.text.clone(),
                raised_at_ms: notice.raised_at_ms,
            });
        }
    }
    pending.sort_by_key(|n| n.raised_at_ms);
    Ok(pending)
}

/// Assignments that are still the app's problem, on every thread this state root knows.
///
/// Used by the settle check (spec §2.9) and by the work host's stop-everything path. It
/// walks partitions rather than one thread because quit does not know which conversation
/// is open, and an assignment on another thread is still running work.
pub fn open(state: &Path) -> Result<Vec<Assignment>, AssignmentError> {
    let scan = scan_open(state)?;
    match scan.unreadable.into_iter().next() {
        Some(error) => Err(error),
        None => Ok(scan.rows),
    }
}

/// [`open`], keeping every open record it can read beside one error for each record it
/// cannot (finding 08; see [`Scan`]). A damaged record is not known to be open or closed, so
/// it is never among the rows and never dropped from the errors. An `Err` is only a register
/// that could not be listed at all. The boot reconciliation reads this; the quit and settle
/// checks keep [`open`], where a partial list must never read as the whole one.
pub fn scan_open(state: &Path) -> Result<Scan, AssignmentError> {
    let parent = state.join("assignments");
    if parent.is_symlink() {
        return Err(AssignmentError("Assignment records have been redirected.".into()));
    }
    let partitions = match std::fs::read_dir(&parent) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Scan::default()),
        Err(_) => return Err(AssignmentError("Assignments could not be read.".into())),
    };
    let mut scan = Scan::default();
    for partition in partitions {
        let directory = partition.map_err(|_| AssignmentError("Assignments could not be listed.".into()))?.path();
        if !directory.is_dir() || directory.is_symlink() {
            continue;
        }
        let files = std::fs::read_dir(&directory)
            .map_err(|_| AssignmentError("Assignments could not be read.".into()))?;
        for file in files {
            let path = file.map_err(|_| AssignmentError("Assignments could not be listed.".into()))?.path();
            if path.extension().is_some_and(|e| e == "json") {
                let Ok(bytes) = std::fs::read(&path) else {
                    scan.unreadable.push(AssignmentError("An assignment record could not be read.".into()));
                    continue;
                };
                let Ok(record) = serde_json::from_slice::<Assignment>(&bytes) else {
                    scan.unreadable.push(AssignmentError(
                        "An assignment record is damaged or was written by a newer RichOS.".into(),
                    ));
                    continue;
                };
                if record.schema == 1 && record.state.is_open() {
                    scan.rows.push(record);
                }
            }
        }
    }
    scan.rows.sort_by_key(|row| (row.registered_at_ms, row.id.clone()));
    Ok(scan)
}

/// The sentences. They live together so the one distinction the CEO is most likely to be
/// surprised by — spec §0 row 7 — is visible in one place rather than spread across the
/// callers that raise it.
///
/// **American English, spoken-safe, comparative rather than absolute, no identifiers.**
pub mod says {
    /// **THE TITLE IS NEVER THE SUBJECT OF THE SENTENCE. It closes, and the verdict is its own
    /// sentence about "It".**
    ///
    /// # Ray's candidate-.11 defect 1.1, which is what made this the rule for every title
    ///
    /// The outcome card on his walk
    /// (`docs/verification/2026-09-19-nightly-1.2.0-nightly.20260918.6-mac-and-android-audit.md`
    /// §1.1, screenshot 08), verbatim:
    ///
    /// > "Add a line to notes.txt in the QA fixture repository saying the candidate eleven walk
    /// > happened, **and land it is finished.** It landed on main in qa-fixture-repo. …"
    ///
    /// His note: *"A person stumbles on 'and land it is finished'. The rest of the sentence is
    /// good, which makes the stumble worse."*
    ///
    /// **The cause is grammatical and it is not fixable by editing the clause.** A title is in
    /// HIS OWN TERMS — the register asks for *"one plain sentence you would be happy to read
    /// back to him"* — and his terms are almost always an imperative: *"add a line … and land
    /// it"*, *"land the pricing branch"*. An imperative clause cannot be the subject of
    /// *"{title} is finished"*, so the join produced a sentence that does not parse, and it
    /// would do so again for *"stopped before it finished"*, *"did not start"*, *"was stopped"*
    /// and the other four the moment any of them reported on a job he had described that way.
    ///
    /// **So the title gets its own sentence and the verdict talks about "It".** This is not a
    /// new shape: it is exactly what the truncated branch has done since Ray's candidate-.8 row
    /// 8, extended to every title instead of the one shape that forced it. The result reads and
    /// speaks the same way — *"Add a line to notes.txt … and land it. It is finished. It landed
    /// on main in qa-fixture-repo."*
    ///
    /// **What this costs, stated rather than left to be discovered.** A title that IS a noun
    /// phrase — *"the pricing review"* — used to read *"the pricing review is finished"* and now
    /// reads *"The pricing review. It is finished."* That is a little blunter, and it is the
    /// price of a rule that cannot produce a sentence which fails to parse. The alternative was
    /// detecting an imperative, which is a guess about English made on the model's wording, and
    /// a wrong guess puts the broken sentence back on his card.
    ///
    /// **It is never a second mark, and that distinction is the one `25b933f2` got right.** A
    /// title already ending in terminal punctuation — the truncation's `…` is the only shape
    /// [`super::sanitize_title`] leaves — has nothing appended to it; a word is inserted after
    /// it. So the double-period defect of candidate .7's row 8 (*"… and land it.. It's running
    /// now"*) cannot come back through this door.
    fn continues(title: &str, clause: &str) -> String {
        if title.ends_with(['…', '.', '!', '?']) {
            format!("{title} It {clause}")
        } else {
            format!("{title}. It {clause}")
        }
    }

    /// **What he hears when a job is waiting on a decision of his** — spec §0 row 7 / §7.8's
    /// *"you hear 'ready for you to approve', never 'done'"*, narrowed by the CEO's ruling
    /// §52 (2026-09-18) to the only case that can still produce it.
    ///
    /// **Its second clause is gone, and it had to go.** It read *"Nothing has been changed in
    /// your repository yet — the last step is yours"*, which was true while the only thing
    /// that could stop a background job was a land. A job now lands on its own and stops only
    /// at a step the desk would have asked him about in a visible turn — a command, a write
    /// outside its workspace — which it may well reach AFTER landing something. So the old
    /// sentence would have told him his repository was untouched while his branch had already
    /// moved. What is left claims only what is known: it is waiting on him.
    pub fn ready_to_approve(title: &str) -> String {
        continues(title, "is waiting on a decision from you before it can go on.")
    }
    /// **What he hears when a job is finished, and `outcome` is the whole of §52 in this
    /// module.** *"There's nothing that ever not lands on its own here in the terminal …
    /// So, yes, always land on its own."* — so the sentence names what LANDED, in the words
    /// `work_host`'s `what_happened` read off the receipts, rather than stopping at
    /// "finished".
    ///
    /// **`outcome` is allowed to be empty and the sentence still stands.** An assignment can
    /// close with nothing to land, and a caller with nothing to say is better than a caller
    /// inventing a clause.
    pub fn settled(title: &str, outcome: &str) -> String {
        let outcome = outcome.trim();
        if outcome.is_empty() {
            return continues(title, "is finished, and I've kept everything it produced with your saved work.");
        }
        continues(title, &format!("is finished. {outcome} Everything it produced is with your saved work."))
    }
    /// Spec §3.7: *"A failure says what stopped and what it is waiting on."*
    ///
    /// **Only for a job that actually got going** — see [`did_not_start`] for the other
    /// half. "Stopped before it finished" carries the claim that there was something to
    /// stop.
    pub fn failed(title: &str, waiting_on: &str) -> String {
        continues(title, &format!("stopped before it finished. {waiting_on}"))
    }

    /// **A job that never got going, said as that and not as a job that stopped.**
    ///
    /// Ray's candidate-.7 row 2 is the reason this sentence exists. The failures on his walk
    /// happened BEFORE the back end had taken the turn — the work connection had not opened
    /// — and every one of them was reported with [`failed`], *"… stopped before it
    /// finished"*. That sentence tells him work began and then broke off, which invites
    /// exactly the wrong question ("how far did it get?") about a job that got nowhere at
    /// all. Nothing had started, so nothing stopped.
    ///
    /// **Which of the two a caller uses is decided by evidence, never by which arm it is
    /// in.** `work_host.rs`'s `run_one` sets a flag when the back end's first stream item
    /// arrives — a positive signal from the child, the only thing that establishes the turn
    /// was taken — and picks the sentence off that flag.
    pub fn did_not_start(title: &str, why: &str) -> String {
        continues(title, &format!("did not start. {why} Nothing was changed, and nothing is running."))
    }
    pub fn interrupted(title: &str) -> String {
        continues(title, "was stopped. Everything it had done is kept, and nothing was landed.")
    }
    /// He declined the step it stopped at (spec §5.7: nothing is approved on his behalf, and
    /// nothing he answered is thrown away either).
    ///
    /// **A separate sentence from [`interrupted`] on purpose.** "It was stopped" is what a
    /// stop he pressed produces; this is an answer he gave to a question, and telling him the
    /// work "was stopped" when he declined one step would describe his own decision back to
    /// him in somebody else's words.
    pub fn declined(title: &str) -> String {
        continues(
            title,
            "stopped at the step you declined. Nothing in your repository was changed, \
             and everything it produced is kept with your saved work.",
        )
    }

    /// **A QUESTION THAT DID NOT GET ANSWERED, SAID AS THAT.**
    ///
    /// [`failed`] and [`did_not_start`] both put the title where a job's name goes —
    /// *"{title} stopped before it finished."* A question's title is his question, so those
    /// produce *"why the nightly is red stopped before it finished"*, which is not English
    /// and not a thing anybody would say to him. Same fact, said the way a person says it:
    /// the answer is what did not arrive, and the reason follows.
    ///
    /// **It does not distinguish "did not start" from "stopped part way", and that is
    /// deliberate.** For a piece of work the difference matters, because it decides what he
    /// asks next ("how far did it get?"). For a question it does not exist: either he has the
    /// answer or he does not, and nothing was half-answered. The reason still travels, so
    /// nothing is hidden — only the distinction that would be about work he never asked for.
    pub fn could_not_answer(question: &str, why: &str) -> String {
        let why = why.trim();
        if why.is_empty() {
            return format!("I couldn't get you an answer on {question}. Ask me again and I'll try it a different way.");
        }
        format!("I couldn't get you an answer on {question}. {why}")
    }

    /// The failure sentence for whichever kind of thing this was — work, or a question.
    ///
    /// **The dispatch lives here rather than at the three call sites in `work_host.rs`**, so
    /// that a fourth failure path added later gets the right shape without anybody
    /// remembering this rule. `took_the_turn` is the evidence-based distinction the work
    /// sentences need (Ray's candidate-.7 row 2: a job that got nowhere must not be described
    /// as one that stopped part way) and it is ignored for a question, which has no such
    /// middle state — see [`could_not_answer`].
    pub fn failure(kind: super::AssignmentKind, title: &str, why: &str, took_the_turn: bool) -> String {
        if kind.is_question() {
            return could_not_answer(title, why);
        }
        if took_the_turn {
            failed(title, why)
        } else {
            did_not_start(title, why)
        }
    }

    /// **THE ANSWER TO A QUESTION, SAID AS AN ANSWER** — the CEO's ruling §58, 2026-09-18:
    /// *"The answer arrives on the timeline as an answer, never as 'done'."*
    ///
    /// **It is the only function in this module that adds no words of its own, and that is
    /// the whole of it.** Every other sentence here wraps a fact in the app's framing —
    /// *"{title} is finished."*, *"{title} stopped before it finished."* — because every
    /// other sentence reports on a piece of WORK. He did not ask for work; he asked a
    /// question, and the thing he is owed is the answer to it. A receipt sentence wrapped
    /// around an answer is the *"done/landed"* framing §58 names and refuses, and prefixing
    /// *"Here's what I found:"* would be this app narrating over the back end's own words.
    ///
    /// So: the back end's answer, flattened to what a timeline line and a spoken line can
    /// both carry, and nothing else.
    ///
    /// **`answer` empty is a real case with a real sentence.** A back end that closed the
    /// question without saying anything has told us nothing, and *"I looked and found
    /// nothing"* would be a claim about the world rather than about the run. The fallback
    /// says which of the two happened and names the one thing that moves it — the same shape
    /// [`unknown`] uses, and for the same reason.
    pub fn answered(title: &str, answer: &str) -> String {
        let answer = answer.trim();
        if answer.is_empty() {
            return format!(
                "I came back from {title} with nothing I can tell you: the answer never \
                 reached me. Ask me again and I'll go at it properly."
            );
        }
        answer.to_string()
    }
}

/// An answer, bounded and safe to put on the timeline and read aloud.
///
/// **It is NOT [`sanitize_title`] and it is NOT [`sanitize_line`], and the difference is the
/// cap.** A title is a noun phrase that lands inside a spoken sentence, so 160 characters is
/// generous. An answer IS the whole reply — a paragraph of his answer is the normal case, and
/// truncating it at 160 would throw away the thing he asked for.
///
/// 8,000 characters is the ceiling, and it bounds a record rather than an answer: the notice
/// is held in a JSON file that `read_one` refuses above 256 KiB, and 64 notices of 8,000
/// characters is 512 KiB — so without a cap here a long-winded back end could write an
/// assignment record this app can no longer read, which would lose his answer entirely
/// rather than shorten it. Control characters other than newline are flattened to spaces;
/// newlines survive, because an answer with a list in it reads as a list on his timeline and
/// the markdown renderer is already there (`ui/timeline.js`).
pub fn sanitize_answer(raw: &str) -> String {
    let flattened: String = raw
        .chars()
        .map(|c| if c.is_control() && c != '\n' { ' ' } else { c })
        .collect();
    // Collapse runs of blank lines and trailing spaces without touching single newlines.
    let tidy: String =
        flattened.lines().map(str::trim_end).collect::<Vec<_>>().join("\n").trim().to_string();
    if tidy.chars().count() <= 8000 {
        return tidy;
    }
    let mut cut: String = tidy.chars().take(8000).collect();
    if let Some(space) = cut.rfind(char::is_whitespace) {
        cut.truncate(space);
    }
    format!("{}…", cut.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **RAY'S CANDIDATE-.8 ROW 8, AS A TEST — and the sentence it reproduces is his, verbatim.**
    ///
    /// He read this off the third failure card
    /// (`docs/verification/2026-09-18-nightly-1.2.0-20260918.2-onscreen-audit.md` §1 row 8):
    ///
    /// > "…and land it. (Third try; the first two stopped before they… stopped before it
    /// > finished. …"
    ///
    /// **The mechanism is the truncation ellipsis, not a doubled prefix.** His instruction was
    /// 168 characters; `sanitize_line` caps a title at 160 and cuts on a word boundary, which
    /// lands it on *"…stopped before they"* and appends the `…` the truncation needs. `says::failed`
    /// then continued with a lowercase verb phrase, and an ellipsis is terminal punctuation — so
    /// the card read as a sentence that broke off and restarted. Asserted below by rebuilding his
    /// title from a 168-character instruction and comparing the join to the string he quoted.
    ///
    /// **The positive control is the first assertion**: the old join is still constructed here and
    /// still produces the collision, so the property assertions underneath are a defect that went
    /// away rather than a check that moved.
    ///
    /// The property is asserted over all seven sentences rather than the one he saw, because six
    /// of them were equally capable of it and none had been checked.
    ///
    /// **What this does NOT do, said plainly rather than left implied:** it enumerates the seven by
    /// hand, so an EIGHTH sentence added to [`says`] is not covered until somebody adds it to the
    /// list here, and a title-bearing sentence written in another module is not covered at all.
    /// (Checked against the screen-wait slice landed the same day: `screen::says`'s three sentences
    /// are `&'static str` and embed no title, so nothing there needs this.) The structural guard is
    /// that [`says::continues`] is the only join in this module; the enumeration is the evidence
    /// that it is applied, not a promise about code that does not exist yet.
    #[test]
    fn concurrent_reports_acknowledgments_and_state_updates_are_all_retained() {
        let state = root();
        let receipt = std::sync::Arc::new(register(&state, &registration()).unwrap());
        let gate = std::sync::Arc::new(std::sync::Barrier::new(5));
        let threads: Vec<_> = (0..5).map(|worker| {
            let (state, receipt, gate) = (state.clone(), receipt.clone(), gate.clone());
            std::thread::spawn(move || {
                gate.wait();
                for step in 0..10 {
                    if worker == 4 {
                        advance(&state, "depot", "thread-one", &receipt.id, AssignmentState::Running, "Active.").unwrap();
                    } else {
                        let text = format!("Report {worker}:{step}");
                        let notice = raise_notice(&state, "depot", "thread-one", &receipt.id, NoticeKind::Answer, &text).unwrap();
                        acknowledge_notice(&state, "depot", "thread-one", &PendingNotice {
                            assignment_id: receipt.id.clone(), title: receipt.title.clone(),
                            kind: notice.kind, text, raised_at_ms: notice.raised_at_ms,
                        }).unwrap();
                    }
                }
            })
        }).collect();
        for thread in threads { thread.join().unwrap(); }
        let saved = read(&state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(saved.notices.len(), 40);
        assert!(saved.notices.iter().all(|notice| notice.delivered_at_ms.is_some()));
        assert_eq!(saved.state, AssignmentState::Running);
        std::fs::remove_dir_all(state).unwrap();
    }

    #[test]
    fn the_complete_authored_handoff_survives_a_shortened_display_title() {
        let state = root();
        let mut request = registration();
        request.title = format!("{}\nPreserve this final requirement exactly.", "A complete request with context. ".repeat(12));
        let receipt = register(&state, &request).unwrap();
        let record = read(&state, "depot", "thread-one", &receipt.id).unwrap();
        assert!(record.title.chars().count() <= 161);
        assert_eq!(record.handoff, request.title);
        let mut legacy = serde_json::to_value(record).unwrap();
        legacy.as_object_mut().unwrap().remove("handoff");
        assert!(serde_json::from_value::<Assignment>(legacy).unwrap().handoff.is_empty());
        std::fs::remove_dir_all(state).unwrap();
    }

    #[test]
    fn a_truncated_title_never_collides_with_the_verdict_that_continues_from_it() {
        let instruction = "Add a line to the notes file in the QA fixture repository saying the \
                           candidate eight walk happened, and land it. (Third try; the first two \
                           stopped before they started.)";
        let instruction = instruction.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(instruction.chars().count(), 168, "the fixture stopped being an over-length title");
        let title = sanitize_title(&instruction).unwrap();
        assert!(title.ends_with('…'), "the fixture no longer truncates, so it tests nothing: {title}");

        // ---- POSITIVE CONTROL: the old join, and it is his card ---------------------------
        let before = format!("{title} stopped before it finished. Choose a company before saving interview answers.");
        assert!(
            before.contains("stopped before they… stopped before it finished."),
            "the collision this test exists for can no longer be constructed: {before}"
        );

        // ---- THE FIX, over every sentence in `says` --------------------------------------
        let cards = |title: &str| {
            vec![
                says::ready_to_approve(title),
                says::settled(title, ""),
                says::settled(title, "It landed on cc/pricing."),
                says::failed(title, "Choose a company before saving interview answers."),
                says::did_not_start(title, "The work connection could not be opened."),
                says::interrupted(title),
                says::declined(title),
            ]
        };

        for card in cards(&title) {
            // An ellipsis is terminal, so nothing lowercase may run on from one.
            let mut chars = card.chars().peekable();
            while let Some(c) = chars.next() {
                if c == '…' {
                    let tail: String = chars.clone().collect();
                    let next = tail.trim_start().chars().next();
                    assert!(
                        !matches!(next, Some(n) if n.is_lowercase()),
                        "a clause runs straight on from the truncation ellipsis: {card}"
                    );
                }
            }
            // And candidate .7's row 8 must not come back the other way: no second mark.
            for double in ["..", ". .", "…."] {
                assert!(!card.contains(double), "two terminal marks in a row: {card}");
            }
            assert!(card.starts_with(&title), "the title stopped leading its own sentence: {card}");
        }

        // The one he saw, now, in full.
        assert_eq!(
            says::failed(&title, "Choose a company before saving interview answers."),
            format!("{title} It stopped before it finished. Choose a company before saving interview answers.")
        );

        // ---- AND AN UNTRUNCATED TITLE CLOSES TOO, WHICH IS RAY'S CANDIDATE-.11 §1.1 -------
        //
        // **THIS BLOCK USED TO ASSERT THE OPPOSITE, and it was a stale assertion rather than a
        // defect.** It read *"AND NOTHING CHANGES FOR A TITLE SHORT ENOUGH TO SURVIVE THE CAP …
        // `continues` inserts a word only after an ellipsis"*, which was true and was the
        // narrower rule. It survived one more walk: candidate .11's outcome card read
        // *"Add a line to notes.txt in the QA fixture repository saying the candidate eleven walk
        // happened, and land it is finished."* — an untruncated title, straight into the verdict,
        // and not a sentence. See [`says::continues`] for why a title in HIS terms cannot be the
        // subject of one.
        let short = sanitize_title("landing the three branches").unwrap();
        assert_eq!(short, "landing the three branches");
        assert_eq!(
            says::failed(&short, "The land lock timed out."),
            "landing the three branches. It stopped before it finished. The land lock timed out."
        );
        assert_eq!(
            says::interrupted(&short),
            "landing the three branches. It was stopped. Everything it had done is kept, and nothing was landed."
        );
        // **THE ONE HE ACTUALLY SAW, in full, both before and after.** His title is an
        // imperative, which is what the register asks for ("in HIS OWN TERMS"), and an
        // imperative clause cannot be a subject.
        let his = sanitize_title(
            "Add a line to notes.txt in the QA fixture repository saying the candidate eleven \
             walk happened, and land it",
        )
        .unwrap();
        assert_eq!(
            says::settled(&his, "It landed on main in qa-fixture-repo."),
            "Add a line to notes.txt in the QA fixture repository saying the candidate eleven \
             walk happened, and land it. It is finished. It landed on main in qa-fixture-repo. \
             Everything it produced is with your saved work."
        );
        assert!(
            !says::settled(&his, "").contains("and land it is finished"),
            "the sentence a person stumbles on is back: {}",
            says::settled(&his, "")
        );
        // Asserted at the JOIN and not over the whole card: an outcome of his own may perfectly
        // well contain "It" ("It landed on cc/pricing."), and a test that demanded or banned the
        // word anywhere would pass or fail for reasons that have nothing to do with the title.
        for card in cards(&short) {
            let after = card.strip_prefix(&format!("{short}. ")).unwrap_or_else(|| {
                panic!("the title no longer closes its own sentence: {card}")
            });
            assert!(
                after.starts_with("It "),
                "the verdict is not its own sentence about the job: {card}"
            );
        }
    }

    fn root() -> PathBuf {
        let path = std::env::temp_dir().join(format!("assignment-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    /// **RAY'S CANDIDATE-.8 ROW 9: the record overwrote when a job failed.**
    ///
    /// Measured on his walk
    /// (`docs/verification/2026-09-18-nightly-1.2.0-20260918.2-onscreen-audit.md` §1 row 9) and
    /// re-derived from his own timestamps rather than quoted: job 1 registered `10:35:42.910Z`
    /// and failed `10:35:49.190Z`, which is **6.280 s**. After the relaunch delivered its notice
    /// the same record read `10:39:47.649Z` — **244.739 s**. Both of his figures reproduce. He
    /// only had the true one because he read the file while it was live; anyone reading it
    /// afterwards got the wrong elapsed time.
    ///
    /// **And it was worse than a wrong number on a card.** `status_tools.rs:138` publishes this
    /// field to the front desk's model as `last_moved_at_ms` and `:195` sorts the list by it, so
    /// delivery order was silently reordering "what moved most recently".
    ///
    /// **The positive controls are the two halves that must NOT change.** `advance` must still
    /// move the time — otherwise this is a field that stopped working rather than a defect that
    /// went away — and `take_pending_notices` must still actually mark the notice delivered,
    /// otherwise its write never happened and the negative below is vacuous.
    #[test]
    fn delivering_a_notice_never_rewrites_when_the_assignment_last_moved() {
        let state = root();
        let receipt = register(&state, &registration()).unwrap();
        let registered = read(&state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(registered.updated_at_ms, registered.registered_at_ms);

        // ---- POSITIVE CONTROL 1: a real move still moves the time ------------------------
        std::thread::sleep(std::time::Duration::from_millis(5));
        let failed = advance(
            &state,
            "depot",
            "thread-one",
            &receipt.id,
            AssignmentState::Failed,
            "The work connection could not be opened.",
        )
        .unwrap();
        assert!(
            failed.updated_at_ms > registered.updated_at_ms,
            "advance stopped recording when the assignment moved"
        );
        let moved_at = failed.updated_at_ms;

        // Raising the notice is not a move either — it is the same event being written down.
        std::thread::sleep(std::time::Duration::from_millis(5));
        raise_notice(
            &state,
            "depot",
            "thread-one",
            &receipt.id,
            NoticeKind::Failed,
            "It stopped before it finished.",
        )
        .unwrap();
        assert_eq!(
            read(&state, "depot", "thread-one", &receipt.id).unwrap().updated_at_ms,
            moved_at,
            "raising a notice rewrote when the assignment moved"
        );

        // Nor is writing down which back end took it.
        std::thread::sleep(std::time::Duration::from_millis(5));
        note_start(&state, "depot", "thread-one", &receipt.id, "sess_one", Vec::new()).unwrap();
        assert_eq!(
            read(&state, "depot", "thread-one", &receipt.id).unwrap().updated_at_ms,
            moved_at,
            "noting the work session rewrote when the assignment moved"
        );

        // ---- THE DEFECT: delivery, which is what his relaunch did ------------------------
        std::thread::sleep(std::time::Duration::from_millis(5));
        let delivered = take_pending_notices(&state, "depot", "thread-one").unwrap();
        assert_eq!(delivered.len(), 1, "nothing was delivered, so the negative below proves nothing");
        let after = read(&state, "depot", "thread-one", &receipt.id).unwrap();

        // ---- POSITIVE CONTROL 2: the delivery write really happened ----------------------
        assert!(
            after.notices[0].delivered_at_ms.is_some(),
            "delivery was not recorded, so no write went past `update` and the test is vacuous"
        );
        assert!(
            after.notices[0].delivered_at_ms.unwrap() > moved_at,
            "the delivery time is not even later than the move, so nothing was timed"
        );
        // Delivery is recorded, separately and durably, on the notice. It is not recorded on
        // the assignment, because the assignment did not move.
        assert_eq!(
            after.updated_at_ms, moved_at,
            "delivering a notice rewrote when the assignment moved — Ray's row 9"
        );
        assert_eq!(after.state, AssignmentState::Failed);

        // A second delivery has nothing to deliver and still must not touch it.
        assert!(take_pending_notices(&state, "depot", "thread-one").unwrap().is_empty());
        assert_eq!(
            read(&state, "depot", "thread-one", &receipt.id).unwrap().updated_at_ms,
            moved_at
        );
        std::fs::remove_dir_all(state).unwrap();
    }

    /// The host's digest of the CEO's exact ledger text, as `prepare_work_turn` computes it.
    fn instruction_digest() -> String {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(b"land these three branches"))
    }

    fn registration() -> Registration {
        Registration {
            entity_id: "depot".into(),
            thread_id: "thread-one".into(),
            obligation_id: "obligation-7".into(),
            instruction_ledger_ref: "ledger:thread-one:turn-7".into(),
            instruction_sha256: instruction_digest(),
            title: "landing the three branches".into(),
            repositories: vec!["/fictional/project".into()],
            needs_screen: false,
        }
    }

    /// **The CEO's ruling §56's one input survives a write and a read, and an assignment
    /// written before the field existed still reads.**
    ///
    /// The second half is the one that matters on the day this lands: there are records on
    /// disk with no `needs_screen` in them, and `serde(deny_unknown_fields)` plus a `schema`
    /// check makes this module deliberately strict about shape. `serde(default)` is what keeps
    /// `schema` at 1 — and `false` is the TRUTHFUL default for an old record, because no
    /// assignment written before today declared a screen need.
    #[test]
    fn a_screen_need_survives_the_record_and_an_older_record_still_reads_as_not_needing_one() {
        let state = root();
        let plain = register(&state, &registration()).unwrap();
        let screen = register(
            &state,
            &Registration { obligation_id: "obligation-8".into(), needs_screen: true, ..registration() },
        )
        .unwrap();

        assert!(!read(&state, "depot", "thread-one", &plain.id).unwrap().needs_screen);
        assert!(read(&state, "depot", "thread-one", &screen.id).unwrap().needs_screen);

        // **A record written by the RichOS that existed yesterday.** Built by deleting the
        // field from the JSON on disk, which is exactly the shape an older build left behind.
        let folder = folder(&state, "depot", "thread-one").unwrap();
        let path = folder.join(format!("{}.json", screen.id));
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        value.as_object_mut().unwrap().remove("needs_screen");
        assert!(value.get("needs_screen").is_none(), "the field was not removed, so this proves nothing");
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();

        let old = read(&state, "depot", "thread-one", &screen.id).unwrap();
        assert!(!old.needs_screen, "an older record must read as not needing the screen");
        assert_eq!(old.schema, 1, "reading an older record must not have changed its schema");
        std::fs::remove_dir_all(state).unwrap();
    }

    /// The new state word, and the two questions every surface asks of it. `WaitingForScreen`
    /// is open (an update must not install over it) and is NOT his to decide.
    #[test]
    fn waiting_for_the_screen_is_open_work_that_is_not_waiting_on_him() {
        let state = AssignmentState::WaitingForScreen;
        assert_eq!(state.as_str(), "waiting-for-screen");
        assert!(state.is_open());
        assert!(!state.awaits_his_word());
        assert!(state.waits_for_the_world());
        // Nothing else waits for the world, so a future state cannot join it by accident.
        for other in [
            AssignmentState::Registered,
            AssignmentState::Preparing,
            AssignmentState::Running,
            AssignmentState::Blocked,
            AssignmentState::Settled,
            AssignmentState::Failed,
            AssignmentState::Interrupted,
            AssignmentState::Unknown,
        ] {
            assert!(!other.waits_for_the_world(), "{:?}", other);
            assert_ne!(other.as_str(), "waiting-for-screen");
        }
        // The kebab-case wire form is what `serde` writes, so the record and the word agree.
        let json = serde_json::to_string(&AssignmentState::WaitingForScreen).unwrap();
        assert_eq!(json, "\"waiting-for-screen\"");
    }

    /// Spec §1.1/§7.1: the turn boundary is the receipt, so registration must be a write
    /// and nothing else. The number is the point of the test — 3.0-3.8 s is the cost
    /// §7.1 moved OFF this turn, so a registration path that drifted into seconds would
    /// have silently undone the decision the whole ask rests on.
    ///
    /// The ceiling is 5 s for TEN registrations — 500 ms each against the 3.0–3.8 s one
    /// `prepare` costs, a margin of roughly six. It is deliberately not tighter: ten
    /// fsynced writes on a machine running the rest of this suite in parallel measured
    /// 253 ms once while this test's first ceiling was 250 ms, and a timing test that
    /// fails on load teaches people to ignore timing tests. It still fails, by a wide
    /// margin, the regression it exists for — a registration path that starts spawning
    /// a lease or preparing a workspace.
    #[test]
    fn registration_is_a_write_and_never_pays_for_preparation() {
        let state = root();
        let started = std::time::Instant::now();
        for n in 0..10 {
            // A distinct obligation each time: one open assignment per obligation, because
            // the seat is spelled from it (see the seat test below).
            register(&state, &Registration { obligation_id: format!("obligation-{n}"), ..registration() })
                .unwrap();
        }
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_millis(5_000),
            "ten registrations took {elapsed:?}; registration has started paying for something"
        );
        assert_eq!(read_all(&state, "depot", "thread-one").unwrap().len(), 10);
        std::fs::remove_dir_all(state).unwrap();
    }

    #[test]
    fn registration_records_acceptance_without_claiming_execution() {
        let state = root();
        let receipt = register(&state, &registration()).unwrap();
        let rows = read_all(&state, "depot", "thread-one").unwrap();
        assert_eq!(rows[0].state, AssignmentState::Registered);
        assert_eq!(receipt.id, rows[0].id);
        assert_eq!(receipt.kind, rows[0].kind);
        std::fs::remove_dir_all(state).unwrap();
    }

    /// **Ray's row 8, at the join that built it.** A title carrying its own terminal
    /// punctuation never produces a second mark, in any of the seven sentences that embed one —
    /// and the assertion is over ALL of them, because fixing only one join would have left the
    /// rest reading *"and land it. is finished."*
    ///
    /// **The receipt is no longer one of them, and the fix still matters.** He saw the double
    /// period in *"… and land it.. It's running now"*, which the CEO's ruling §55 has since
    /// replaced with *"On it!"* — but every sentence that reports a RESULT still names the
    /// assignment in his own words, and those are the ones he reads for the rest of the job's
    /// life.
    #[test]
    fn a_title_that_ends_a_sentence_never_gets_a_second_period() {
        // What Ray actually saw, verbatim from his walk: the model's title ended in a period
        // and the sentence added its own.
        let title = sanitize_title("review the branches and land it.").unwrap();
        assert_eq!(title, "review the branches and land it", "the trailing period survived the sanitizer");
        // The truncation path keeps its ellipsis, which is terminal punctuation it needs, and is
        // asserted through the sentences themselves rather than through a helper — see the note
        // above `sanitize_title`'s neighbors for why there is no helper.
        let truncated = sanitize_title(&"branch ".repeat(60)).unwrap();
        assert!(truncated.ends_with('…'));

        // Every sentence that embeds a title, over all three shapes: his own period, an
        // ellipsis, and a real truncated title.
        for embedded in [title.as_str(), "review the branches and land it…", truncated.as_str()] {
            for line in [
                says::ready_to_approve(embedded),
                says::settled(embedded, "It landed on a branch in his depot."),
                says::settled(embedded, ""),
                says::failed(embedded, "The work connection could not be opened."),
                says::interrupted(embedded),
                says::declined(embedded),
                says::did_not_start(embedded, "The work connection could not be opened."),
                // §58's empty-answer fallback is the one arm of `answered` that embeds a
                // title, so it belongs in this loop. The ordinary arm returns the back end's
                // own words and embeds nothing — a title cannot collide with punctuation
                // that is not there.
                says::answered(embedded, ""),
            ] {
                assert!(!line.contains(".."), "double period: {line}");
                assert!(!line.contains("…."), "ellipsis then period: {line}");
            }
        }
        // A title that is nothing but punctuation is refused, not silently emptied.
        assert!(sanitize_title("...").is_err());
        assert!(sanitize_title(".").is_err());
    }

    #[test]
    fn delegated_questions_retain_the_kind_chosen_by_rich() {
        let state = root();
        for (n, kind) in [AssignmentKind::Check, AssignmentKind::Investigate, AssignmentKind::Task].into_iter().enumerate() {
            let receipt = register_kind(&state, &Registration {
                obligation_id: format!("obligation-{n}"), ..registration()
            }, kind).unwrap();
            assert_eq!(receipt.kind, kind);
        }
        let rows = read_all(&state, "depot", "thread-one").unwrap();
        assert_eq!(rows.len(), 3);
        for row in rows { assert_eq!(row.state, AssignmentState::Registered); }
        std::fs::remove_dir_all(state).unwrap();
    }

    /// **A record written before 2026-09-18 has no `kind`, and it reads back as a task** —
    /// which is what every one of them is. Positive control: a record whose `kind` IS on
    /// disk reads back as that kind rather than as the default.
    #[test]
    fn an_older_record_without_a_kind_is_a_task_and_a_newer_one_keeps_what_it_says() {
        let state = root();
        let receipt = register_kind(&state, &registration(), AssignmentKind::Investigate).unwrap();
        let path = folder(&state, "depot", "thread-one").unwrap().join(format!("{}.json", receipt.id));
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"kind\":\"investigate\""), "the kind was not written: {text}");
        assert_eq!(read(&state, "depot", "thread-one", &receipt.id).unwrap().kind, AssignmentKind::Investigate);
        // Now the pre-§58 shape: the same record with the field removed entirely.
        let older: serde_json::Value = serde_json::from_str(&text).unwrap();
        let mut older = older.as_object().unwrap().clone();
        older.remove("kind");
        std::fs::write(&path, serde_json::to_vec(&older).unwrap()).unwrap();
        let back = read(&state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(back.kind, AssignmentKind::Task, "an older record did not read back as a task");
        assert!(!back.kind.is_question());
        // A kind the model could mistype is refused rather than defaulted to a task: the
        // wrong reply is the one defect §58 exists to remove.
        assert!(AssignmentKind::parse("checking").is_err());
        assert!(AssignmentKind::parse("").is_err());
        assert_eq!(AssignmentKind::parse("check").unwrap(), AssignmentKind::Check);
        std::fs::remove_dir_all(state).unwrap();
    }

    /// **§58: the answer arrives as an answer, never as "done".**
    #[test]
    fn an_answer_is_delivered_as_itself_and_never_wrapped_in_a_receipt_sentence() {
        let answer = "Three branches are waiting on you, and the oldest has been there since Tuesday.";
        let said = says::answered("what is waiting on me", answer);
        assert_eq!(said, answer, "the answer was wrapped in the app's own words");
        for forbidden in ["is finished", "landed", "done", "your saved work", "assignment"] {
            assert!(!said.to_lowercase().contains(forbidden), "a work receipt's framing reached an answer: {said}");
        }
        // The empty case says which of the two happened rather than claiming the world is empty.
        let nothing = says::answered("what is waiting on me", "   ");
        assert!(nothing.contains("nothing I can tell you"), "{nothing}");
        assert!(!nothing.contains("I looked and found nothing"), "{nothing}");
        // Bounded, and a newline survives because a list reads as a list.
        let long = says::answered("x", &sanitize_answer(&"word ".repeat(4000)));
        assert!(long.chars().count() <= 8001, "the answer was not bounded: {}", long.chars().count());
        assert!(long.ends_with('…'));
        assert_eq!(sanitize_answer("one\ntwo\u{7}three  \n\n"), "one\ntwo three");
    }

    /// **Spec §5.8c, in the shape the landed engine forces it into.** One seat per
    /// assignment, and the seat is spelled from the OBLIGATION because that is the field
    /// the engine's reconciler reads back (`mega-lander/app.py:704`). So the cardinality
    /// this test defends is the obligation's: two seats differ because two obligations do,
    /// and a second open assignment on ONE obligation is refused rather than silently
    /// given the first one's seat — which is §5.8c's reproduced failure, the CEO replaced
    /// by the work lease's own next assignment.
    #[test]
    fn every_assignment_gets_its_own_seat_and_no_assignment_gets_the_ceo_seat() {
        let state = root();
        let first = register(&state, &registration()).unwrap();
        let second = register(
            &state,
            &Registration { obligation_id: "obligation-8".into(), ..registration() },
        )
        .unwrap();
        let rows = read_all(&state, "depot", "thread-one").unwrap();
        let seats: Vec<_> = rows.iter().map(|r| r.seat.clone()).collect();
        assert_eq!(seats.len(), 2);
        assert_ne!(seats[0], seats[1]);
        assert_ne!(first.id, second.id);
        assert!(seats.contains(&"work-seat:obligation-7".to_string()), "{seats:?}");
        assert!(seats.contains(&"work-seat:obligation-8".to_string()), "{seats:?}");
        for seat in &seats {
            assert_ne!(seat, "ceo-default");
        }
        // A SECOND open assignment on obligation-7 would share its seat, so it is refused.
        assert!(register(&state, &registration()).is_err());
        // Positive control: once the first has settled, its obligation is free again.
        advance(&state, "depot", "thread-one", &first.id, AssignmentState::Settled, "Closed.").unwrap();
        assert!(register(&state, &registration()).is_ok());
        std::fs::remove_dir_all(state).unwrap();
    }

    /// Spec §3.6: the instruction reference is frozen at registration, and the digest is
    /// of the CEO's own words. A later turn does not move it.
    #[test]
    fn the_instruction_reference_is_frozen_at_the_turn_he_gave_it_in() {
        let state = root();
        let receipt = register(&state, &registration()).unwrap();
        let row = read(&state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(row.instruction_ledger_ref, "ledger:thread-one:turn-7");
        assert_eq!(row.instruction_sha256, instruction_digest());
        advance(&state, "depot", "thread-one", &receipt.id, AssignmentState::Running, "Worker start recorded.")
            .unwrap();
        let later = read(&state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(later.instruction_ledger_ref, "ledger:thread-one:turn-7");
        std::fs::remove_dir_all(state).unwrap();
    }

    /// **Spec §7.8's rule survives the CEO's ruling §52; its example does not.** The rule —
    /// *"the wording IS the test"* — is that a sentence about an unfinished job never speaks
    /// of finishing. The example it was written around, a job held at its land, is gone: a
    /// job lands on its own now (2026-09-18).
    ///
    /// So the negative scan runs against the sentence that CAN still be produced — a job
    /// waiting on a decision of his — and its positive control is the settled sentence, which
    /// does speak of finishing. Without that control a vacuous scan would pass.
    #[test]
    fn a_blocked_assignment_says_it_is_waiting_and_is_never_called_done() {
        let ready = says::ready_to_approve("landing the three branches");
        assert!(ready.contains("waiting on a decision from you"), "{ready}");
        for forbidden in ["done", "finished", "complete", "landed"] {
            assert!(!ready.to_lowercase().contains(forbidden), "approval notice implied completion: {ready}");
        }
        // **AND IT MAKES NO CLAIM ABOUT HIS REPOSITORY.** Its old second clause said nothing
        // had been changed there, which a job that lands on its own can no longer promise.
        assert!(!ready.to_lowercase().contains("repository"), "the notice still claims his repository is untouched: {ready}");
        // Positive control: the forbidden-word scan can fail.
        let settled = says::settled("landing the three branches", "");
        assert!(settled.to_lowercase().contains("finished"));
        assert!(AssignmentState::Blocked.is_open(), "blocked is waiting for him, not settled");
        assert!(!AssignmentState::Settled.is_open());
        assert!(!AssignmentState::Interrupted.is_open());
    }

    /// **§52: he hears the OUTCOME.** The settled sentence carries what landed — the branch,
    /// the repository, the verdict — in the words `work_host`'s `what_happened` read off the
    /// receipts, and it still stands on its own when there is nothing to report rather than
    /// inventing a clause.
    #[test]
    fn the_settled_sentence_says_what_landed_and_survives_having_nothing_to_say() {
        let with = says::settled("landing the three branches", "It landed on cc/echo-1 in project. An independent review passed it first.");
        assert!(with.contains("cc/echo-1 in project"), "{with}");
        assert!(with.contains("An independent review passed it first."), "{with}");
        assert!(with.contains("is finished."), "{with}");
        // Whitespace-only is the same as nothing: a caller with nothing to say must not
        // produce "X is finished.  Everything it produced…" with a hole in the middle.
        for nothing in ["", "   "] {
            let bare = says::settled("landing the three branches", nothing);
            assert!(bare.contains("is finished, and I've kept everything it produced"), "{bare}");
            assert!(!bare.contains("  "), "an empty outcome left a gap in the sentence: {bare:?}");
        }
    }

    /// Spec §3.4: a notice is held until he has been told, and taking it marks it. The
    /// second read returns nothing — that is what stops him hearing it twice after a
    /// relaunch, and the durable flag is the only reason it survives one.
    #[test]
    fn a_notice_is_held_until_he_is_told_and_is_only_told_once() {
        let state = root();
        let receipt = register(&state, &registration()).unwrap();
        advance(&state, "depot", "thread-one", &receipt.id, AssignmentState::Blocked, "Waiting for your approval.")
            .unwrap();
        raise_notice(
            &state,
            "depot",
            "thread-one",
            &receipt.id,
            NoticeKind::ReadyToApprove,
            &says::ready_to_approve(&receipt.title),
        )
        .unwrap();
        let first = take_pending_notices(&state, "depot", "thread-one").unwrap();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].kind, NoticeKind::ReadyToApprove);
        assert!(first[0].text.contains("waiting on a decision from you"));
        // The flag is on disk, not in memory: re-read from the path a relaunch would use.
        let second = take_pending_notices(&state, "depot", "thread-one").unwrap();
        assert!(second.is_empty(), "a notice was delivered twice: {second:?}");
        let row = read(&state, "depot", "thread-one", &receipt.id).unwrap();
        assert!(row.notices[0].delivered_at_ms.is_some());
        std::fs::remove_dir_all(state).unwrap();
    }

    /// Scope is a boundary, not a filter: a record belonging to another company is an
    /// error, never an empty list that reads as "nothing is running".
    #[test]
    fn a_record_from_another_scope_is_refused_rather_than_omitted() {
        let state = root();
        let receipt = register(&state, &registration()).unwrap();
        assert!(read_all(&state, "other", "thread-one").unwrap().is_empty());
        let path = folder(&state, "depot", "thread-one").unwrap().join(format!("{}.json", receipt.id));
        let mut row: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        row["entity_id"] = "other".into();
        std::fs::write(&path, serde_json::to_vec(&row).unwrap()).unwrap();
        assert!(read_all(&state, "depot", "thread-one").is_err());
        // And a newer schema is refused rather than half-understood.
        row["entity_id"] = "depot".into();
        row["schema"] = 2.into();
        std::fs::write(&path, serde_json::to_vec(&row).unwrap()).unwrap();
        assert!(read_all(&state, "depot", "thread-one").is_err());
        std::fs::write(&path, b"partial").unwrap();
        assert!(read_all(&state, "depot", "thread-one").is_err());
        std::fs::remove_dir_all(state).unwrap();
    }

    #[test]
    fn open_walks_every_thread_because_quit_does_not_know_which_one_is_on_screen() {
        let state = root();
        let mine = register(&state, &registration()).unwrap();
        let elsewhere = register(
            &state,
            &Registration {
                thread_id: "thread-two".into(),
                instruction_ledger_ref: "ledger:thread-two:turn-3".into(),
                ..registration()
            },
        )
        .unwrap();
        assert_eq!(open(&state).unwrap().len(), 2);
        advance(&state, "depot", "thread-one", &mine.id, AssignmentState::Settled, "Witnessed finishing.").unwrap();
        let still_open = open(&state).unwrap();
        assert_eq!(still_open.len(), 1);
        assert_eq!(still_open[0].id, elsewhere.id);
        // Blocked is OPEN: it is waiting for him, and quit must still stop it and say so.
        advance(&state, "depot", "thread-two", &elsewhere.id, AssignmentState::Blocked, "Waiting for your approval.")
            .unwrap();
        assert_eq!(open(&state).unwrap().len(), 1);
        std::fs::remove_dir_all(state).unwrap();
    }

    #[test]
    fn a_title_from_the_model_is_flattened_bounded_and_never_empty() {
        assert_eq!(sanitize_title("  land   these\nthree  branches ").unwrap(), "land these three branches");
        assert!(sanitize_title("   \n\t ").is_err());
        let long = "branch ".repeat(60);
        let cut = sanitize_title(&long).unwrap();
        assert!(cut.chars().count() <= 161, "{}", cut.chars().count());
        assert!(cut.ends_with('…'));
        assert!(!cut.contains('\n'));
    }

    #[test]
    fn registration_refuses_what_it_cannot_scope_or_attest() {
        let state = root();
        let bad_thread = Registration { thread_id: "thread one".into(), ..registration() };
        assert!(register(&state, &bad_thread).is_err());
        let no_entity = Registration { entity_id: String::new(), ..registration() };
        assert!(register(&state, &no_entity).is_err());
        let no_turn = Registration { instruction_ledger_ref: String::new(), ..registration() };
        assert!(register(&state, &no_turn).is_err());
        // A reference for a DIFFERENT thread is refused too: an assignment cannot borrow
        // an instruction the CEO gave in another conversation.
        let wrong_thread =
            Registration { instruction_ledger_ref: "ledger:thread-two:turn-7".into(), ..registration() };
        assert!(register(&state, &wrong_thread).is_err());
        let no_instruction = Registration { instruction_sha256: "not-a-digest".into(), ..registration() };
        assert!(register(&state, &no_instruction).is_err());
        let too_many = Registration { repositories: vec!["/r".to_string(); 33], ..registration() };
        assert!(register(&state, &too_many).is_err());
        // Positive control: the same shape with all of them fixed does register.
        assert!(register(&state, &registration()).is_ok());
        std::fs::remove_dir_all(state).unwrap();
    }

    /// **Spec §6.1: a relaunch reconciles against the evidence file and against Git, and
    /// neither is reachable without something written down BEFORE the crash.**
    ///
    /// The session id and the repository heads are that something. This asserts they
    /// survive a write/read round trip, and that a record written before they existed
    /// still reads — an assignment already on his disk must not become unreadable because
    /// recovery landed.
    #[test]
    fn what_a_relaunch_reconciles_against_is_written_down_before_the_crash() {
        let state = root();
        let receipt = register(&state, &registration()).unwrap();
        let fresh = read(&state, "depot", "thread-one", &receipt.id).unwrap();
        // Nothing has been on a lease yet, and that is a fact rather than a gap.
        assert_eq!(fresh.work_session, None);
        assert!(fresh.repository_pins.is_empty());

        note_start(
            &state,
            "depot",
            "thread-one",
            &receipt.id,
            "work-session-abc",
            vec![
                RepositoryPin { path: "/fictional/project".into(), head: Some("a".repeat(40)) },
                // An unreadable repository is recorded as unreadable, never as unchanged.
                RepositoryPin { path: "/fictional/unreadable".into(), head: None },
            ],
        )
        .unwrap();
        let after = read(&state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(after.work_session.as_deref(), Some("work-session-abc"));
        assert_eq!(after.repository_pins.len(), 2);
        assert_eq!(after.repository_pins[1].head, None);

        // A session id that could not be a path component is refused rather than written.
        assert!(note_start(&state, "depot", "thread-one", &receipt.id, "../escape", Vec::new()).is_err());

        // **The backward-compatibility control.** A record written by the build before
        // this field existed has neither key; it must still read, with the same honest
        // "nothing was written down" answer.
        let mut older = serde_json::to_value(&fresh).unwrap();
        let object = older.as_object_mut().unwrap();
        object.remove("work_session");
        object.remove("repository_pins");
        let older: Assignment = serde_json::from_value(older).unwrap();
        assert_eq!(older.work_session, None);
        assert!(older.repository_pins.is_empty());
        std::fs::remove_dir_all(state).unwrap();
    }

    /// **Row 9's one sentence: nothing invents a completion, and `unknown` is not a stop
    /// either.** The three states a relaunch can reach are kept apart here, in the two
    /// places a reader meets them — the state's own predicates, and the words he hears.
    #[test]
    fn unknown_is_neither_finished_nor_stopped_nor_running() {
        // Not open: nothing is running on it, so it can never hold an update forever
        // (§6.5's trap) and it can never keep a windowless app alive (§2.4a).
        assert!(!AssignmentState::Unknown.is_open());
        // And not a settlement of any kind: it is his to decide (§6.3).
        assert!(AssignmentState::Unknown.awaits_his_word());
        assert!(AssignmentState::Blocked.awaits_his_word());
        // Positive controls, so this test fails if `awaits_his_word` ever degenerates into
        // "everything" or `is_open` into "nothing".
        assert!(!AssignmentState::Settled.awaits_his_word());
        assert!(!AssignmentState::Interrupted.awaits_his_word());
        assert!(AssignmentState::Running.is_open());
        assert_eq!(AssignmentState::Unknown.as_str(), "unknown");


    }
}

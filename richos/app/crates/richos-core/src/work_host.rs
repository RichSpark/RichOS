//! The WORK HOST — the actor that is not the conversation, and the second compute lease
//! it owns.
//!
//! The background-work spec (richos-hq `docs/plans/background-work-spec-2026-09-17.md`
//! revision 5, `9255e71a`) §2.1: *"a second compute lease inside the app process, owned by
//! a work host that is not the spine."* §2.2 says why there is no cheaper answer: the
//! conversation's lease is held for the entire turn (`src-tauri/src/main.rs:561`, still
//! held at `:582`), and today's workers are helpers INSIDE that turn
//! (`native.rs`'s `nested_worker_text_and_results_never_become_the_leads_conversation`).
//! Work leaves the turn only by leaving that lease.
//!
//! **Three structural rules, each of which is a test in this file.**
//!
//! 1. **The host never takes the spine's lock.** It cannot: it has no reference to the
//!    spine. Everything it needs — the state root, the thread binding, the assignment
//!    register — is passed in or read from disk. That is why §0 row 2's *"answers within
//!    seconds"* survives a background assignment that runs for an hour.
//! 2. **The work lease is never attached to the conversation's `TurnControl`** (spec §4.2).
//!    `TurnControl`'s cancel slot is a single `Option<Arc<dyn TurnCancel>>` that
//!    `set_cancel` overwrites (`steering.rs:753-761`), so attaching the work lease would
//!    make Stop kill the work and leave the conversation running — the exact inverse of
//!    what the CEO decided. The host keeps its own handle and reaches it by name.
//! 3. **Nothing infers a finish.** A work turn ending is not a settlement
//!    (`docs/architecture/desktop-work.md:29`). The host settles an assignment only on the
//!    same evidence the conversation's turn-end check uses, read against the WORK lease's
//!    session id (spec §2.9) — and anything it cannot witness counts as still running.
//!
//! **THE CEO's §51 SHAPE, AND WHERE THIS FILE DEPARTS FROM THE SPEC BECAUSE OF IT.**
//! `richos-hq/wiki/ceo-decisions.md` §51 (2026-09-17): *"the job of the front desk Rich is
//! solely talking to the CEO and relaying info to and from the back-end Rich."* One front
//! desk — the conversation lease — and **one standing, invisible back-end Rich** that starts
//! and stops jobs, dispatches, lands and keeps the record. The ruling names the consequence
//! for this file in its own words: *"the work host runs ONE standing back-end lease instead
//! of one per job"*, which overrules the spec's per-assignment lease wherever the two
//! disagree.
//!
//! So [`WorkHost::ensure_lease`] opens the back end once and every assignment afterwards runs
//! on that same lease — pinned by `the_back_end_is_one_standing_lease_across_every_assignment`
//! rather than left to a return-early nobody would notice changing. What stays per assignment
//! is what §51 keeps per assignment: an assignment is *"a bookkeeping unit inside the back
//! end (its own record, seat, stop, approval line) — not a separate mind"*, so the seat, the
//! standing grant and the permission queue's hold are still one each, created and released
//! with the assignment.
//!
//! **4. The back end renews itself, and only ever between assignments.** Sage's finding 11
//!    on the CEO's page: a STANDING back end per thread is a provider session that
//!    accumulates context for as long as the thread lives — *"forever"* is his own word for
//!    what a thread does — and the annex never had this problem, because its work lease was
//!    short-lived. [`WorkHost::rotate_if_needed`] is the seam, at the one moment
//!    continuity §3.1 allows: a completed assignment, with nothing live. The successor is
//!    primed from the durable register rather than from a transcript, which is what makes
//!    the renewal crash-safe and cheap.
//!
//! **Where rows 5 and 9 live.** This file holds their two seams — [`WorkHost::settlement`]
//! and [`WorkHost::open_assignments`] — plus the three things the process model needs from
//! a host: [`WorkNotifier::nothing_left_to_do`] (§2.4a's trigger, decided by the shell
//! because only the shell knows whether a window is open), [`WorkHost::remember_binding`]
//! (so his approval can resume an assignment after a relaunch) and the start-of-assignment
//! pin that recovery reconciles against ([`crate::assignment::note_start`]). The
//! reconciliation itself is [`crate::recovery`]; the exit arms are `src-tauri/src/main.rs`.

use crate::assignment::{self, Assignment, AssignmentState, NoticeKind, PendingNotice, Registration};
use crate::cognition::{Cognition, CognitionError, LeaseFactory, TurnItem, WorkAssignment};
use crate::ecs::Binding;
use crate::entity::ThreadBinding;
use crate::permissions::PermissionDesk;
use crate::steering::{CommandStop, TurnCancel};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

/// How a notice reaches his screen when he happens to be there.
///
/// Spec §3.4: the lane must be PUSHED, because the UI's three-second poll is gated on
/// `mainView === "conversation" && !document.hidden` (`ui/main.js:2859-2863`). The durable
/// record is what makes the notice survive him not being there at all; this is how it
/// arrives when he is.
///
/// **It is a trait and not a channel because the host must not know what a webview is.**
/// `richos-core` is UI-agnostic (`lib.rs`), and the Tauri shell supplies the emitter.
pub trait WorkNotifier: Send + Sync {
    fn raised(&self, thread_id: &str, notice: &PendingNotice);

    /// **An assignment has just finished and the register has nothing open left** — spec
    /// §2.4a's trigger, and nothing more than the trigger.
    ///
    /// The decision it feeds is the shell's, because it turns on a question this crate
    /// must not be able to ask: *is a window open?* §2.4a is *"when the last registered
    /// assignment settles AND no window is open, the app quits itself"*, and `richos-core`
    /// is UI-agnostic (`lib.rs`). So the host reports the half it can witness and the shell
    /// owns the half it can.
    ///
    /// **It is not "settled".** It fires for any ending — settled, failed, interrupted —
    /// because the question §2.4a asks is whether anything is still registered, not how the
    /// last thing ended. An assignment blocked on his approval is still registered
    /// (`AssignmentState::is_open`), so this does not fire for §7.8's case, which is
    /// exactly the distinction §7.8 spends a paragraph on.
    ///
    /// The default is a no-op: a host whose notifier has nothing to do with windows says
    /// nothing, which is the truthful answer for it.
    fn nothing_left_to_do(&self) {}
}

/// **His team, on an operator install** — the operator back end's app side (richos-hq
/// `docs/verification/2026-09-25-operator-client/README.md` §7 item 1: *"an operator
/// assignment is relayed (with its handle) rather than put on a work lease"*).
///
/// Installed by the shell only when `operator.json` passed the gate at launch
/// ([`WorkHost::set_operator`]); with none installed this host is the product's, unchanged.
/// With one installed, **every adopted assignment goes to it and none ever reaches a work
/// lease**: the operator desk relays it to that conversation's lead, or closes it with the
/// sentence that says why not. The per-assignment Stop goes to it too.
pub trait OperatorIntake: Send + Sync {
    /// Take one adopted assignment. Returns at once: the hand-over runs on the intake's own
    /// thread, because a lead's first start is seconds and must never sit inside his send.
    fn take(&self, record: Assignment);
    /// The per-assignment Stop (r3 (d) item 5). `Err` is the sentence he is shown.
    fn stop(&self, entity: &str, thread: &str, id: &str) -> Result<(), String>;
}

/// **Closing, in the engine, an assignment the back end handled itself** — a command he
/// asked for, the reason it could not begin, or the answer to his question — on the words
/// he was given (the engine's host-only `answer-complete`).
///
/// Such an assignment has no worker, and the engine's `complete` needs one
/// (`engine/mega-lander/app.py` `complete`: `1 <= len(ids)`), so until this it could never
/// close and the back end told him so: *"It's still open there"* (richos-hq
/// `docs/verification/2026-09-27-background-command-finish`, vm-run-2). It is called only
/// from the two arms that settle on the back end's own words with no helper on the
/// receipts, and the engine refuses it for any assignment a helper was recorded on, so a
/// code change still closes only through `complete`, with its workers.
///
/// Installed by the shell ([`WorkHost::set_answered_close`]). With none installed the
/// obligation stays open, which is what every build before this did.
pub trait AnsweredClose: Send + Sync {
    /// `Err` is logged and never spoken: what he hears is the report, and the report is true
    /// whether or not the engine's bookkeeping took it.
    fn close_answered(&self, record: &Assignment, answer: &str) -> Result<(), String>;
}

/// The default: say nothing to anybody. Used by tests that are asserting on the durable
/// record, which is the half that has to be right.
pub struct SilentNotifier;
impl WorkNotifier for SilentNotifier {
    fn raised(&self, _thread_id: &str, _notice: &PendingNotice) {}
}

/// What the host is doing with an assignment right now, as far as anything outside it can
/// see. Deliberately not the same type as [`AssignmentState`]: that one is durable and is
/// about the WORK, this one is about the lease and is gone when the process is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveAssignment {
    pub id: String,
    pub entity_id: String,
    pub thread_id: String,
    pub seat: String,
}

/// One item of work for the runner: an assignment, the binding it was registered under, and
/// whether this is the FIRST time it goes on the lease or a continuation he approved.
///
/// The binding is minted by the ledger and is the only thing that can say which company a
/// thread belongs to (`entity.rs`), so the host never reconstructs one from a record.
#[derive(Clone)]
struct Scheduled {
    binding: ThreadBinding,
    record: Assignment,
    /// Spec §5.7: *"when he answers, the answer applies to the assignment"*. A resumed run
    /// is told what he decided, and it is the only way an approval given after the provider
    /// call ended can reach the step that asked for it.
    resumed: bool,
}

/// **A job whose background command outlived the wait inside `run_one`** — the wait yielded
/// to his next job on this thread, or reached [`COMMAND_WAIT_BUDGET`]. The job stays open
/// ("A command it started is still running.") and the runner watches the command between
/// jobs; when it ends, the back end is asked for this job's report on this job's seat
/// (`run_one` with a [`Report`]). Before this, the host stopped waiting and closed the job on
/// the words he already had, and the finish was lost (richos-hq
/// `2026-09-27-background-command-finish`, "Not done").
#[derive(Clone)]
struct Watched {
    binding: ThreadBinding,
    record: Assignment,
    /// The provider's task id and description for each command it is waiting on.
    commands: Vec<(String, String)>,
    /// The words he was already told, so they are never sent twice.
    told: Option<String>,
    /// The lease session the commands run under; a different one cannot report them.
    session: Option<String>,
    /// That lease's cancel handle, kept so his Stop can tell these commands to stop while
    /// another job holds the lease (hunt 2026-09-29 part 1, finding 11). `None` for a lease
    /// that has no handle, which then says it could not reach them.
    stopper: Option<Arc<dyn TurnCancel>>,
}

/// The sentence a Stop of a watched job leaves on it, from what the provider said about each
/// of its commands (finding 11, v2 re-check). Worst first: a refusal is a command known to be
/// running; an undelivered stop or one with no ending seen is a command that may be. Only
/// when every command was seen to end does it say plainly that the job stopped.
fn watched_stop_detail(has_stopper: bool, outcomes: &[CommandStop]) -> &'static str {
    if outcomes.iter().any(|o| matches!(o, CommandStop::Refused(_))) {
        WATCHED_STOP_REFUSED
    } else if !has_stopper || outcomes.contains(&CommandStop::NotDelivered) {
        WATCHED_STOP_NOT_DELIVERED
    } else if outcomes.contains(&CommandStop::Unconfirmed) {
        WATCHED_STOP_UNCONFIRMED
    } else {
        "Stopped. The workspace and the receipts are kept."
    }
}
const WATCHED_STOP_REFUSED: &str = "Stopped. A command it started refused to stop, so it is still running. \
     The workspace and the receipts are kept.";
const WATCHED_STOP_NOT_DELIVERED: &str = "Stopped. A command it started could not be told to stop, so it may still be running. \
     The workspace and the receipts are kept.";
const WATCHED_STOP_UNCONFIRMED: &str = "Stopped. A command it started was told to stop and has not been seen to end, \
     so it may still be running. The workspace and the receipts are kept.";

/// What a watched job's report turn starts from: which of its commands ended, and what he
/// has already been told.
struct Report {
    ended: Vec<String>,
    told: Option<String>,
}

/// How a wait for background commands ended (`wait_for_background_commands`).
#[derive(Debug, PartialEq, Eq)]
enum CommandWait {
    /// Every one of them was witnessed ending.
    Ended,
    /// His next job on this thread arrived, or the bound was reached: the commands still run
    /// and the job is handed to the watch ([`Watched`]).
    Handed,
    /// His Stop, a quit, the lease gone, or a reading that cannot say: nothing is claimed.
    Over,
}

struct Inner {
    peer_input: Option<Arc<dyn TurnCancel>>,
    peer_active_through: Option<u64>,
    peer_stopper: Option<(String, Arc<dyn TurnCancel>)>,
    // Includes pre-lease waits and final receipt writes, not just a live lease.
    processing: bool,
    /// Jobs whose background commands outlived their wait ([`Watched`]).
    watching: Vec<Watched>,
    queue: VecDeque<Scheduled>,
    /// The assignment that is ON this thread's back end right now. At most one: a lease runs
    /// one prompt at a time, so its assignments are serialized onto it and the host says so
    /// rather than pretending three of them are in flight.
    live: Option<LiveAssignment>,
    /// This back end's own cancel handle — spec §4.2's *"second cancel registry"*. Never the
    /// one in `TurnControl`'s slot.
    cancel: Option<Arc<dyn TurnCancel>>,
    /// This back end's session id, for the settle check against its OWN evidence directory
    /// (spec §2.9; `app_workers.rs:38` keys evidence by session).
    lease_session: Option<String>,
    /// Assignments a stop reached while they were queued rather than live.
    stopped: Vec<String>,
    /// **The assignment currently parked on a locked screen, and the wait it is parked in**
    /// — the CEO's ruling §56.
    ///
    /// It is a field of its own and NOT `live`, because `live` means *on the lease* and the
    /// screen gate runs strictly before the lease is opened. It exists so a stop and a quit
    /// can REACH a parked runner: [`crate::screen::ScreenWatch`] sleeps on its own condition
    /// variable, so `backend.wake.notify_all()` does not touch it, and without this handle a
    /// locked screen would make the app unquittable — §56 gives the wait no timeout.
    screen_wait: Option<(String, Arc<crate::screen::ScreenWatch>)>,
    /// The ledger's thread binding for this conversation, kept from whatever put an
    /// assignment on the queue. A resume needs one and must never reconstruct it: only the
    /// ledger can say which company a thread belongs to (`entity.rs`).
    binding: Option<ThreadBinding>,
    closing: bool,
    /// Bumped whenever this back end finishes one assignment, so a test can wait on progress
    /// without sleeping on a guess.
    completed: u64,
    /// **What this back end has consumed since it was last (re)primed** — the estimate
    /// half of the watermark, the same chars÷4 proxy the spine keeps
    /// (`spine.rs:143`, `CHARS_PER_TOKEN_ESTIMATE`), and the fallback rather than the
    /// primary trigger for the reason that constant's own doc gives: it was measured
    /// wrong by 2.3× to 40.6×.
    context_chars: usize,
    /// The MEASURED `{used, size}` this back end's lease last reported, read off the
    /// machinery stream as it goes past (`spine.rs:2034-2041` does the same for the
    /// conversation). `None` until the first `usage_update` arrives.
    context_usage: Option<crate::machinery::ContextUsage>,
    /// How many times this back end has been rotated, and why the last one happened. Kept
    /// so a test — and a boot log — can say that a rotation happened rather than infer it
    /// from a session id changing.
    rotations: u64,
    last_rotation_reason: Option<String>,
    /// Since when a due renewal has been waiting for a command this back end started (reap
    /// gap C6). `None` when nothing is waiting.
    rotation_deferred_since: Option<std::time::Instant>,
    /// **The repositories this back end was opened with** — his company's connected
    /// repositories as the registry said when the lease was spawned, which is what
    /// `EngineProfile::configure` gave it as `--add-dir` roots and as the auto mode's trusted
    /// list. `None` when there is no lease, or the registry could not be read then.
    lease_repositories: Option<Vec<PathBuf>>,
    /// **The team this back end was opened with** — the factory's
    /// [`LeaseFactory::team_digest`], read just before the lease was spawned, so a teammate
    /// saved while it opened can only cost one renewal too many, never one too few. `None`
    /// when there is no lease or the factory could not say; nothing is renewed on a `None`.
    lease_team: Option<String>,
    /// **His answers inside the prompt in flight, not yet taken** (the work-path design, richos-hq
    /// `docs/plans/2026-09-27-work-path-answer-delivery-design.md` D1-D3). Set under this lock
    /// before the send and cleared at the back end's first item of that turn, or when the prompt
    /// returns without one. While an id is here the answer is still pending on disk, and both of
    /// its readers leave it alone: the question worker does not queue the live job again for it
    /// (D2), and the settle does not read it as unconsumed (D3).
    carrying: Vec<String>,
    /// Answers this back end took whose "taken" could not be written to disk, with the session
    /// that took them. That session is never sent them again (D1); a fresh one is.
    taken_unsaved: Vec<(String, String)>,
    /// How many runs of this launch have carried each answer to a back end (design D4's bound).
    answer_starts: std::collections::HashMap<String, u32>,
    /// **The overload schedule for this back end's turns** (CEO, 2026-10-07): consecutive
    /// `529`s since its last turn that answered, the same budget the conversation keeps.
    overload: crate::upstream::RetryBudget,
    /// **This back end's queue is held** ([`Held`]): the jobs in `queue` wait for the cause to
    /// clear and are started by the host when it does. `None` is the ordinary queue.
    held: Option<Held>,
}

/// **ONE BACK-END RICH, AND THERE IS ONE PER CONVERSATION THREAD.**
///
/// The CEO's Two Riches spec (richos-hq `docs/plans/two-riches-spec-2026-09-17.md`), in his
/// own words: *"each conversation thread always holds one front desk Rich and one back-end
/// Rich. Regardless of the number of assignments within a given conversation thread."* So the
/// unit is the THREAD: its own lease, its own queue, its own runner, its own live assignment
/// and its own settle session. Two threads never share a back end, and one thread never opens
/// a second one however many assignments it is given.
///
/// **This is the one place the engineering annex lost.** The background-work spec's §2.1 says
/// *"a second compute lease"* for the app and its §5.3/§5.4 language is per-assignment; §51
/// and this page make it one per thread, standing. Where the two disagree, the CEO's page
/// wins.
struct Backend {
    /// The lease itself, held by whichever thread is running an assignment. Separate from
    /// `inner` so a stop can take `inner` while a turn is in flight.
    lease: Mutex<Option<Box<dyn Cognition>>>,
    inner: Mutex<Inner>,
    wake: Condvar,
}

impl Backend {
    fn new() -> Arc<Self> {
        Arc::new(Backend {
            lease: Mutex::new(None),
            inner: Mutex::new(Inner {
                queue: VecDeque::new(),
                watching: Vec::new(),
                live: None,
                cancel: None,
                peer_input: None,
                peer_active_through: None,
                peer_stopper: None,
                lease_session: None,
                stopped: Vec::new(),
                screen_wait: None,
                processing: false,
                binding: None,
                closing: false,
                completed: 0,
                context_chars: 0,
                context_usage: None,
                rotations: 0,
                last_rotation_reason: None,
                rotation_deferred_since: None,
                lease_repositories: None,
                lease_team: None,
                carrying: Vec::new(),
                taken_unsaved: Vec::new(),
                answer_starts: std::collections::HashMap::new(),
                overload: crate::upstream::RetryBudget::new(),
                held: None,
            }),
            wake: Condvar::new(),
        })
    }
}

pub struct WorkHost {
    /// The engine state root (`<data_dir>/engine-state`), the same root
    /// `app_workers::status` and `work_status::read` are given.
    state: PathBuf,
    factory: Mutex<Box<dyn LeaseFactory>>,
    /// **One back end per conversation thread**, opened on that thread's first assignment and
    /// standing from then on (the Two Riches spec). Keyed by thread id, which is the CEO's
    /// own unit: *"each conversation thread always holds one front desk Rich and one back-end
    /// Rich."*
    backends: Mutex<std::collections::HashMap<String, Arc<Backend>>>,
    /// The whole host is closing (quit). A thread that has never been opened must not be
    /// opened on the way out, which is a question no single back end can answer.
    closing: Mutex<bool>,
    notifier: Arc<dyn WorkNotifier>,
    /// **The one permission desk, shared with the conversation** (spec §2.7, §5.5: it is
    /// already an `Arc<PermissionDesk>` on the engine profile, so one desk serves both
    /// leases). The host holds it for one reason only — the lifecycle §5.4 states: a
    /// queued request and a standing decision belong to an assignment and are dropped with
    /// it. Nothing here reads his queue to decide anything.
    desk: Arc<PermissionDesk>,
    /// Every assignment this host has ever taken on, so a boundary sweep cannot adopt one
    /// twice. Its own lock, because `enqueue` consults it while holding `inner`.
    seen: Mutex<std::collections::HashSet<String>>,
    /// **The rotation budget — Sage's finding 11 on the CEO's page, and the only problem
    /// the Two Riches shape creates that the engineering annex never had.**
    ///
    /// The annex's work lease was short-lived: prepare, dispatch, return. A STANDING
    /// back-end Rich per thread is a provider session that accumulates context for as long
    /// as the thread lives, which is forever — and *"forever"* is the CEO's own word for
    /// what a thread does. Re-derived at `eef1580b` before building on it: the grep in the
    /// finding — `context_chars|usage_update|watermark|rotate` against this file — still
    /// returned **no matches**, so the gap was real and unchanged.
    budget: Mutex<(usize, f64)>,
    /// How long a due renewal waits for a command the back end started (reap gap C6).
    rotation_command_wait: Mutex<std::time::Duration>,
    /// Where his repositories stand, for the pin taken when an assignment starts
    /// (spec §6.1's Git half). Injected so this crate keeps no opinion about how a
    /// repository is read, and so a build with no verified runtime says it could not look
    /// rather than implying it did.
    repositories: Mutex<Box<dyn crate::recovery::Repositories>>,
    /// **How the Mac's screen is read** — the CEO's ruling §56. Injected for the same reason
    /// `repositories` is: this crate keeps no framework link and no opinion about an operating
    /// system, and the shell supplies the real reader (`src-tauri/src/screen.rs`).
    ///
    /// The default is [`crate::screen::UnknownScreen`], and because `Unknown` never blocks
    /// ([`crate::screen`]'s polarity rule) a host nobody gave a reader to runs every
    /// screen-bound assignment immediately rather than parking it forever. That is the honest
    /// failure for a build that cannot see the screen.
    screen: Mutex<Arc<dyn crate::screen::ScreenSource>>,
    /// How often a screen wait looks. [`crate::screen::SCREEN_POLL`] in production; this suite
    /// shortens it for the same reason [`WorkHost::set_context_budget`] exists — a test that
    /// slept two seconds per sample is a test nobody runs.
    screen_poll: Mutex<std::time::Duration>,
    quota: Mutex<Option<Arc<crate::quota::Service>>>,
    /// **His team, when this is an operator install** ([`OperatorIntake`]). `None` on every
    /// product install, which is what keeps this host byte for byte what it was.
    operator: Mutex<Option<Arc<dyn OperatorIntake>>>,
    /// How an assignment the back end handled itself is closed in the engine
    /// ([`AnsweredClose`]); `None` leaves it open, as every build before this did.
    answered_close: Mutex<Option<Arc<dyn AnsweredClose>>>,
    /// **The central folder his company notes are saved in** (`company.rs`,
    /// `<central>/companies/<entity>/company.md`), or `None` when the shell has not said. Every
    /// assignment carries the scoped facts ([`Self::company_notes`]).
    central_root: Mutex<Option<PathBuf>>,
    /// [`COMMAND_WAIT_BUDGET`] in production; shortened by tests, like `screen_poll`.
    command_wait: Mutex<std::time::Duration>,
    /// [`WORKER_WAIT_BUDGET`] in production; shortened by tests, like `command_wait`.
    worker_wait: Mutex<std::time::Duration>,
    /// The output record, when the shell attached one ([`WorkHost::set_output_store`]).
    output: Mutex<Option<crate::output::OutputStore>>,
    /// How an overload retry waits out the CEO's schedule; shortened by tests, like `screen_poll`.
    retry_clock: Mutex<Arc<dyn crate::upstream::RetryClock>>,
    /// **Is the Claude account a work lease would run under signed in?** `Some(true)` yes,
    /// `Some(false)` no, `None` could not tell. Installed by the shell
    /// ([`WorkHost::set_sign_in`]); with none, a queue held for sign-in waits for
    /// [`WorkHost::signed_in`].
    sign_in: Mutex<Option<SignInReading>>,
    /// [`HOLD_CHECK_EVERY`] in production; shortened by tests, like `screen_poll`.
    hold_check: Mutex<std::time::Duration>,
}

/// One reading of whether the account a work lease would run under is signed in; see
/// [`WorkHost::set_sign_in`].
pub type SignInReading = Arc<dyn Fn() -> Option<bool> + Send + Sync>;

/// The context window this build assumes while a back end has reported nothing, and the
/// fraction of it that schedules a rotation.
///
/// **Both numbers are the spine's, and they are re-exported rather than re-chosen.** The
/// conversation lease's watermark has been tuned against measured traffic
/// (`spine.rs:144-176`: the fallback window is deliberately NOT raised to the measured
/// 1_000_000, and 0.70 is the continuity design's §8 Q2 starting point). A back end that
/// rotated on a second, quietly different pair of numbers would be a second opinion about
/// the same question, and the one that drifted would be the one nobody was watching.
pub const DEFAULT_WORK_CONTEXT_WINDOW_TOKENS: usize = crate::spine::DEFAULT_CONTEXT_WINDOW_TOKENS;
pub const DEFAULT_WORK_WATERMARK_RATIO: f64 = crate::spine::DEFAULT_WATERMARK_RATIO;

/// **How long a due renewal waits for a command the back end started** (the product reap gap
/// design C6). Retiring a lease now ends its tool commands (the supervisor's reap), and a
/// renewal is invisible by his standing order, so it must not visibly end a background command
/// he asked for. It is not allowed to wait forever either: a renewal exists because the context
/// window is filling, and a command that never ends (`tail -f`) would hold it at the watermark
/// indefinitely. Ten minutes covers an ordinary build, test run or install; past it the renewal
/// goes ahead and says so in the log. `estimate:` the design names the bound, not its length.
pub const ROTATION_WAITS_FOR_COMMANDS: std::time::Duration = std::time::Duration::from_secs(600);

/// What the outgoing back end is asked for at a rotation — **the compaction half**.
///
/// It asks for what the register cannot hold: what was in the middle of being worked out.
/// It does not ask for a summary of the assignments, because those are read off disk and a
/// model's recollection of them would be a second, softer copy of a record that already
/// exists.
const HANDOFF_ASK: &str = r#"{"event":"session_handoff_requested","max_chars":1500}"#;

/// Added to [`HANDOFF_ASK`] when the hand-over is in the MIDDLE of an assignment (fill-first
/// between continuation turns): the next turn may need a file only the outgoing back end
/// knows the name of — a background command's output is *"in the file named when it
/// started"* ([`COMMAND_ENDED_TAIL`]).
const HANDOFF_ASK_MID_RUN: &str = r#"{"carrying_assignment":true}"#;

/// How a successor that takes an assignment over part-way is told so, and what it does next.
const MID_RUN_HANDOVER_OPENING: &str = r#"{"event":"session_replaced","reason":"account_changed","carrying_assignment":true}"#;
const MID_RUN_HANDOVER_CLOSING: &str = r#""#;

/// The bound on that answer, so one back end's verbosity cannot become the next one's
/// priming. Roughly five sentences of prose.
const HANDOFF_BUDGET_CHARS: usize = 1500;

/// **What the back end is told when the helper it launched has been witnessed ending** —
/// `run_one` step 3b.
///
/// The first half is all the host actually knows: a run this lease started is over, by
/// `SubagentStop` in the app's own journal. The second half names the NEXT STEP, and that is
/// there because of what happened without it: on the first end-to-end run of this loop the
/// worker finished and committed, and the back end then asked `prepare` for a second WORKER.
/// That was refused — correctly, by the spawn registration, because the first worker's work
/// was neither landed nor discarded — and the assignment failed with the work sitting done in
/// its workspace. `DESKTOP.md` step 5 says a reviewer comes next; this says it at the one
/// moment it applies, the same argument as the async-launch sentence in the brief.
const WORKER_ENDED_CONTINUATION: &str =
    r#"{"event":"helper_finished","duty":"worker"}"#;

/// **What the back end is told when helpers it started were handed off at the weekly switch**
/// (richos-hq `docs/plans/2026-10-07-weekly-switch-handoff.md` §2). The quota gate ordered
/// each of them, on the account being left, to commit, write a `RichOS handoff:` commit and
/// end (`quota::gate::HANDOFF_ORDER`); its marker says so, and the host names every one whose
/// run has ended between this head and [`HANDOFF_CONTINUATION_TAIL`]. Chosen first, before the
/// consult and review-passed sentences, and sent only once this back end is off the account
/// being left (`wait_for_the_switch`).
const HANDOFF_CONTINUATION_HEAD: &str =
    r#"Helpers handed off at an account switch (recorded facts):"#;
const HANDOFF_CONTINUATION_TAIL: &str =
    r#"End of recorded handoffs."#;

/// **What the back end is told when the helper that ended was the REVIEWER, and it passed.**
///
/// The sentence above names the next step for a WORKER that has ended, and it is the wrong
/// sentence for the step after that: read at the end of a review it tells the back end to
/// prepare a reviewer for work a reviewer has just passed. On the green run of 2026-09-18
/// that is exactly the turn that was lost — the reviewer passed the commit byte for byte
/// (`docs/verification/worker-turn-grant-2026-09-18.md` §5) and nothing ever asked for the
/// land.
///
/// **Which of the two is sent is read off the receipts, never off a counter in this
/// process**: [`crate::work_status::trail`] says a reviewer's own receipt recorded `passed`
/// and that no land is recorded against the worker. A host that guessed from "how many times
/// have I waited" would say this on a second worker's first pass.
const REVIEW_PASSED_CONTINUATION: &str =
    r#"{"event":"helper_finished","review":"passed","integration_recorded":false}"#;

/// **What the back end is told when the helper that ended was a CONSULT** (proto-teammate shelf
/// plan §3, slice 3): a named teammate asked on a job that changes no repository, whose final
/// message is the deliverable. The worker sentence above would send the back end to prepare a
/// reviewer for an answer, which has nothing to review and nothing to land.
const CONSULT_ENDED_CONTINUATION: &str =
    r#"{"event":"helper_finished","duty":"consult"}"#;

/// How many times ONE continuation may be asked again after a turn that produced nothing.
///
/// **A turn that streams no item at all is a turn this host did not get**, and the run that
/// proved it is the same one: the host's second continuation returned 1.847 s after the
/// reviewer ended with zero tool calls, because the `result` of a turn the platform injected
/// was handed to it. `native.rs` now correlates the two (see `PendingTurn`), and this is the
/// second line of that defense for the degraded case where the child names no turn at all:
/// ask once more rather than report a job failed on the strength of a turn that never ran.
///
/// **ONE, and the number is a spend decision rather than a taste.** Every ask is a real model
/// turn on a real subscription. One re-ask covers a single stolen result; a second would be
/// this host guessing, and a loop would be this host paying for its own guess.
const CONTINUATION_REASK_LIMIT: usize = 1;

/// How long ONE wait for a helper lasts before the row says it is taking longer than usual.
///
/// **It is a bound on what the row says, never a verdict on the worker, and never the end of
/// the wait.** Twenty minutes is longer than any worker run measured on this product (the
/// longest, a `work_lease_roundtrip` worker that actually wrote and committed, was 3 m 41 s)
/// and short enough that a CEO who walks away is not left with a row that never moves: at the
/// bound it moves to [`HELPER_TAKING_LONGER_DETAIL`], which names his Stop as the way out.
///
/// It used to END the wait, and the job then failed with its grant revoked while the journal
/// still showed its helper at work (hunt part 1 finding 06, richos-hq
/// `docs/audits/2026-09-29-hunt/part-1-codex.md`). Elapsed time is not evidence that a helper
/// failed; only its own `SubagentStop`, his Stop or a quit ends the wait.
const WORKER_WAIT_BUDGET: std::time::Duration = std::time::Duration::from_secs(20 * 60);

/// What the row says once a helper has outlived [`WORKER_WAIT_BUDGET`] and is still witnessed
/// open. The job is still `Running`, which is true, and the sentence names the way out.
pub const HELPER_TAKING_LONGER_DETAIL: &str =
    "A helper is still doing the work. It is taking longer than usual; Stop ends it if you do not want to wait.";

/// How often the wait re-reads the journal. One small file; chosen for how soon the back end
/// gets its next turn, not for cost.
const WORKER_WAIT_POLL: std::time::Duration = std::time::Duration::from_secs(2);

/// **The back end said, in the engine's own words, that it is waiting on a helper.**
///
/// `stop-declared: waiting-on-teammate` is one of the engine's three declared stops ("a
/// teammate is running and the next step needs it", `engine/ass-kicker/guard-stated-actions.py`),
/// and the stated-actions guard REQUIRES it of a turn that ends promising to act on a
/// teammate's answer. So it is not a phrase this host hopes to see: it is the line the engine
/// makes the back end write in exactly the case below.
///
/// **Walk 6, 2026-10-05 (`walk-10608166d53f`, esc-20261005T151403Z-2f5c3737).** The back end
/// messaged its reviewer with `SendMessage` and ended its turn with this line. The reviewer's
/// last `SubagentStop` was already in the journal, so [`crate::app_workers::status`] saw no
/// helper open, the loop ended, the grant was revoked and the job was written `failed` ("It
/// stopped before it finished. The work ran and nothing was landed.") while the reviewer ran
/// four more times, every hand-back of its verdict refused because its grant was gone. So when
/// a turn ends on this declaration and nothing is open, the host waits for the next helper run
/// to END ([`crate::app_workers::run_ends`]) and then hands the back end its continuation.
///
/// Read the way the engine reads it: at the start of a line, after any of ` \t>*-•`.
fn declares_waiting_on_a_helper(said: &str) -> bool {
    said.lines().any(|line| declared_wait(line).is_some())
}

/// One line read as the engine's `stop-declared: waiting-on-teammate` declaration, the way
/// the engine reads it ([`declares_waiting_on_a_helper`]): what follows the case word.
fn declared_wait(line: &str) -> Option<&str> {
    const CASE: &str = "waiting-on-teammate";
    let rest = line.trim_start_matches([' ', '\t', '>', '*', '-', '\u{2022}']).strip_prefix("stop-declared:")?;
    let rest = rest.trim_start_matches([' ', '\t']);
    rest.get(..CASE.len()).filter(|case| case.eq_ignore_ascii_case(CASE)).map(|_| &rest[CASE.len()..])
}

/// **Rich's waiting sentence**: the one sentence for him on a turn's declared wait, saying who
/// is working on what (`DESKTOP.md`, the consult paragraph). Shown on his row while the
/// teammate works, and never part of his answer ([`keep_words`]).
fn waiting_sentence(said: &str) -> Option<String> {
    let why = said.lines().filter_map(declared_wait).next_back()?;
    let why = why.trim_start_matches([' ', '\t', '\u{2014}', '\u{2013}', '-', ':']).trim_end_matches(['*', ' ', '\t']).trim();
    (!why.is_empty()).then(|| why.to_string())
}

/// **The work path's one injected fault** (the VM crash matrix's P4e cell, design §4.2): with the
/// `crash-points` feature and `RICHOS_CRASH_POINT` naming it, it is true ONCE per process. Every
/// product build compiles it to `false`. The aborts are `operator_host::crash_point`'s.
#[cfg(feature = "crash-points")]
fn work_fault(name: &str) -> bool {
    static FIRED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    std::env::var("RICHOS_CRASH_POINT").is_ok_and(|point| point == name)
        && !FIRED.swap(true, std::sync::atomic::Ordering::SeqCst)
}

#[cfg(not(feature = "crash-points"))]
#[inline(always)]
fn work_fault(_name: &str) -> bool {
    false
}

/// **The one line before the brief of work he picked back up after RichOS closed on it** (the
/// work-path design's D6 note, on C6's option B path).
pub const PICKED_UP_NOTE: &str =
    r#"{"event":"resumed_after_interruption","prior_effects_may_exist":true}"#;

/// **Where an answer to his team goes** from the question worker's pass
/// ([`WorkHost::deliver_team_answers`]): the operator desk's inbox, which returns its receipt.
pub type LeadSink<'a> = &'a dyn Fn(&crate::questions::Delivery) -> Result<String, String>;

/// **The quota hold's detail once the back end has taken a turn of this run** ([`WorkHost::quota_gate`]'s
/// `started`): held, and no current reading. Recovery reads a `WaitingForQuota` row carrying
/// either one as a started job, never re-run by itself (the work-path design D6).
pub const QUOTA_WAIT_AFTER_START: [&str; 2] = [
    "Waiting for the allowance to refresh. It had already started; its work is saved and it will continue automatically.",
    "Waiting for a current allowance reading before continuing. It had already started; its work is saved.",
];

/// **What the row says while his answer waits for a back end that could not take it yet**
/// (the work-path design D4), in the design's words.
pub const ANSWER_RETRY_DETAIL: &str = "Your answer is saved. The back end couldn't take it yet, so I'm trying again.";

/// **What an assignment's row says once the work has been taken up**, after "Running." in the
/// Under the hood panel. It said "The back end has started on it." until hunt 57's finding 11
/// (2026-10-15): inside the app the user only ever meets Rich, so it is Rich who has started.
pub const STARTED_DETAIL: &str = "Rich has started on it.";

/// **What a held job's row says while nobody is signed in to Claude** (T3 idea 1, finished:
/// richos-hq `docs/plans/2026-10-09-automatic-second-review-and-t3-ideas.md` §3, and Sage's
/// check of it, §1.1). The job never started, nothing was asked of the back end, and the host
/// starts it by itself when the sign-in comes back. The row it sits on stays `registered`.
pub const HELD_FOR_SIGN_IN: &str = "Waiting for you to sign in to Claude, in Settings under Claude accounts. \
     It will start by itself once you have.";

/// The second half of a held job's row when the back end itself would not open; the first
/// half is the reason it gave.
pub const HELD_FOR_BACK_END: &str = "It will start by itself as soon as the work connection opens.";

/// What a job's row says while a question it asked him is still open, or his answer waits for
/// the next run.
pub const WAITING_FOR_YOUR_ANSWER: &str = "Waiting for your answer. Independent work can continue.";

/// **The recorded lines that tell him what to do**, which Under the hood shows in its ordinary
/// view (`get_assignments`' `forYou`). Every other recorded detail is the back end's own record
/// and shows only with Technical view on (hunt 57, finding 11; the review of 25b540804, which
/// found a job held for sign-in reading only "Written down. Nothing has been prepared yet.").
pub fn tells_him_what_to_do(detail: &str) -> bool {
    [HELD_FOR_SIGN_IN, HELPER_TAKING_LONGER_DETAIL, WAITING_FOR_YOUR_ANSWER].contains(&detail)
}

/// **Does his row show its recorded detail in the ordinary view** (`get_assignments`'
/// `forYou`)? A line that tells him what to do, or Rich's waiting sentence while a teammate
/// works ([`Assignment::waiting_sentence`]; the review of 3b91f1223, finding 1).
pub fn for_him(row: &Assignment) -> bool {
    row.waiting_sentence || tells_him_what_to_do(&row.detail)
}

/// **How often the host looks again at the cause of a held queue**, with no model turn: the
/// sign-in reading the shell installs ([`WorkHost::set_sign_in`]), or one attempt to open the
/// back end. Sage's check §1.1: *"checking the sign-in state at most every few minutes, without
/// a model turn, is a probe and not a retry of work."* `estimate:` two minutes, not measured
/// against how soon he signs in; the shell also says so the moment a sign-in completes
/// ([`WorkHost::signed_in`]), so this bounds only a sign-in made outside the app.
const HOLD_CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(120);

/// **Why this conversation's queue is held.**
#[derive(Clone, Debug, PartialEq, Eq)]
enum HoldCause {
    /// A fresh job's first turn was refused because nobody is signed in to Claude
    /// ([`crate::interruption::InterruptionCause::NotSignedIn`]). The back end opens when
    /// signed out (measured, Claude Code 2.1.295: `system/init`, a `<synthetic>` assistant
    /// text "Not logged in · Please run /login", then a result with `is_error: true`), so this
    /// is found at the first turn and never at the open.
    SignIn,
    /// The back end itself would not open ([`WorkHost::ensure_lease`]), for this reason.
    BackEnd(String),
}

impl HoldCause {
    fn detail(&self) -> String {
        match self {
            HoldCause::SignIn => HELD_FOR_SIGN_IN.to_string(),
            HoldCause::BackEnd(why) => format!("{} {HELD_FOR_BACK_END}", honest(why)),
        }
    }

    /// **The one thing he is told when a queue is first held** (Sage's check §1.1: *"Telling
    /// the user once … is the only words it needs"*): which request, how many wait with it,
    /// why, what clears it, and that it then starts by itself. Spoken as written.
    fn notice(&self, title: &str, others: usize) -> String {
        let (what, it) = match others {
            0 => (format!("\"{title}\""), "it"),
            1 => (format!("\"{title}\" or the one request waiting behind it"), "they"),
            n => (format!("\"{title}\" or the {n} requests waiting behind it"), "they"),
        };
        let starts = if it == "it" { "it will start by itself" } else { "they will start by themselves" };
        match self {
            HoldCause::SignIn => format!(
                "I haven't started {what} yet, because nobody is signed in to Claude on this Mac. \
                 Sign in from Settings, under Claude accounts, and {starts}."
            ),
            HoldCause::BackEnd(why) => format!(
                "I haven't started {what} yet, because the work connection would not open. {} \
                 As soon as it opens, {starts}.",
                honest(why)
            ),
        }
    }
}

/// **A held queue** — the shape of `WaitingForQuota` and `WaitingForScreen` (`quota_gate`,
/// `screen_gate`): strictly before any work, and it ends by itself. While it stands the runner
/// takes nothing off this back end's queue.
struct Held {
    cause: HoldCause,
    /// The cause has cleared, or may have: the runner takes the FIRST held job only. If that job
    /// is held again for a cause, the queue stays held until the next clearance and nothing else
    /// was started; if it is not, the hold is over and the rest follow in order (Sage §1.1).
    trial: bool,
    /// When the cause was last looked at ([`HOLD_CHECK_EVERY`]).
    checked_at: std::time::Instant,
}

/// The row's detail while a run cut by a usage limit starts again under the next Claude
/// account (fill-first, plan §15 answer 3).
pub const LIMIT_SWITCH_DETAIL: &str = "A Claude usage limit stopped this run. Starting it again on your next Claude account.";

/// What he is told when two runs of this launch could not get his answer taken (design D4).
const ANSWER_NOT_TAKEN: &str = "The back end did not take your answer, after two tries.";

/// The recorded reason for a renewal made because his company's connected repositories
/// changed after this back end was opened (`run_one`'s step 0b).
pub const REPOSITORIES_CHANGED: &str = "repositories-connected";

/// The recorded reason for a renewal made because the team the back end's lease registered
/// is no longer the team on disk: a teammate saved, refitted or retired since it opened
/// (proto-teammate shelf plan §1, slice 5).
pub const TEAM_CHANGED: &str = "team-changed";

/// **How a successor renewed for its team in the middle of an assignment is told so** (slice
/// 5): the account-switch opening's words, with the reason that is true here.
const MID_RUN_TEAM_HANDOVER_OPENING: &str = r#"{"event":"session_replaced","reason":"team_changed","carrying_assignment":true}"#;

/// **What the back end is told on the turn a team renewal gave it** (slice 5): nothing was
/// open, its last turn changed the team, and the host renewed it so the change is registered.
/// A saved teammate is now one it can name; the assignment goes on from where it was.
const TEAM_CHANGED_CONTINUATION: &str = r#"{"event":"team_changed","registered":true}"#;

/// What the Under the hood pane says while an assignment waits on a command its back end
/// started in the background.
pub const COMMAND_STILL_RUNNING_DETAIL: &str = "A command it started is still running.";

/// **How long one assignment waits for a command its back end started in the background**
/// (`run_one` step 3c), while nothing else is queued on its thread.
///
/// A bound on a wait and never a verdict: reaching it claims nothing, and the assignment
/// closes on the words he already has. It is longer than [`WORKER_WAIT_BUDGET`] because the
/// wait yields the moment he gives this thread another job, so what it costs is a row that
/// reads "still running" and a back end nothing else is asking for. An hour covers an
/// ordinary build, test run or install. `estimate:` not measured against his commands.
const COMMAND_WAIT_BUDGET: std::time::Duration = std::time::Duration::from_secs(60 * 60);

/// How often that wait reads the lease. The reading is in memory (`native.rs`'s reader
/// state), so this is chosen for how soon the report follows the ending, not for cost.
const COMMAND_WAIT_POLL: std::time::Duration = std::time::Duration::from_millis(250);

/// **What the back end is told when a command it started in the background has ended** —
/// `run_one` step 3c. The first half is what the host knows, from the provider's own
/// `task_notification`; the list of commands goes between the two halves.
const COMMAND_ENDED_HEAD: &str =
    r#"Background commands ended (provider observations):"#;
const COMMAND_ENDED_TAIL: &str =
    r#""#;

fn command_ended_continuation(ended: &[&crate::cognition::BackgroundCommand]) -> String {
    let each: Vec<String> = ended
        .iter()
        .map(|command| {
            let how = command
                .ended
                .as_ref()
                .map(|end| if end.summary.trim().is_empty() { end.status.clone() } else { end.summary.clone() })
                .unwrap_or_default();
            format!("\"{}\" ({how})", command.description)
        })
        .collect();
    format!("{COMMAND_ENDED_HEAD} {}. {COMMAND_ENDED_TAIL}", and_list(&each))
}

/// **What a turn is told about the commands this back end is still running for his OTHER
/// jobs** ([`Watched`]). Empty when there are none. The provider folds a command's ending into
/// whatever turn is running when it comes (`cap-fold.jsonl`), so the turn is told to keep it
/// out of its own answer; the host asks for that job's report on that job's seat afterwards.
fn still_running_for_others(watching: &[Watched]) -> String {
    let each: Vec<String> = watching
        .iter()
        .flat_map(|watched| {
            watched.commands.iter().map(move |(_, what)| format!("\"{what}\" (for \"{}\")", watched.record.title))
        })
        .collect();
    if each.is_empty() {
        return String::new();
    }
    format!("\n\nOther assignments' background commands (observed): {}", and_list(&each))
}

fn with_report(evidence: &str, report: &str) -> String {
    let report = report.trim();
    if report.is_empty() { evidence.to_string() } else { format!("{evidence}\n\nStu's report:\n{report}") }
}

/// Add one turn's words to the account, bounded the way the stream was: 64 KiB is eight times
/// the answer cap, so no real answer is cut by it.
///
/// **Each turn's words are their own paragraph, and the same words twice are said once**
/// (handoff round 1, 2026-10-07: his status read "Handed over cleanly.Handed over cleanly.",
/// the back end's first turn and its handoff continuation run together). A turn that repeats
/// what the previous one ended with adds nothing he has not read.
///
/// **The engine's declared wait is not his answer** (CEO, 2026-10-15, "1 and 3"): that line
/// carries Rich's waiting sentence ([`waiting_sentence`]), which his row shows while the
/// teammate works. Every other word of the turn is kept exactly as the back end wrote it.
fn keep_words(answer: &mut String, said: &str) {
    let said = words_for_him(said);
    let said = said.as_str();
    if said.is_empty() || answer.len() >= 64 * 1024 || answer.trim_end().ends_with(said) {
        return;
    }
    if !answer.trim().is_empty() {
        answer.truncate(answer.trim_end().len());
        answer.push_str("\n\n");
    }
    answer.push_str(said);
}

/// A turn's words without the engine's declared wait ([`declared_wait`]), trimmed; every
/// other line exactly as the back end wrote it.
fn words_for_him(said: &str) -> String {
    if !said.lines().any(|line| declared_wait(line).is_some()) {
        return said.trim().to_string();
    }
    said.lines().filter(|line| declared_wait(line).is_none()).collect::<Vec<_>>().join("\n").trim().to_string()
}

/// A report describes the current outcome. Earlier turns remain in the agent's context,
/// not concatenated into a later report as if their progress were still current.
fn fold_turn(answer: &mut String, said: &str) {
    if !said.trim().is_empty() {
        answer.clear();
        keep_words(answer, said);
    }
}

/// Add one streamed piece of a turn's prose to what the turn said.
///
/// **Different notices are their own paragraphs** (video-finish walks on `ec12621f8`: his
/// report read "…because the video runs for an hour.The transcription is running now."). One
/// turn says one thing, starts a tool, then says the next; the deltas of one notice arrive
/// back to back and are joined as they came, and the first words after anything else the turn
/// did start a new paragraph. A piece that is only whitespace waits for the words after it.
fn add_streamed_words(said: &mut String, text: &str, between_notices: &mut bool) {
    if text.trim().is_empty() {
        if !*between_notices {
            said.push_str(text);
        }
        return;
    }
    if std::mem::take(between_notices) {
        said.clear();
        said.push_str(text.trim_start());
    } else {
        said.push_str(text);
    }
}

impl WorkHost {
    pub fn new(
        state: &Path,
        factory: Box<dyn LeaseFactory>,
        notifier: Arc<dyn WorkNotifier>,
        desk: Arc<PermissionDesk>,
    ) -> Arc<Self> {
        Arc::new(WorkHost {
            state: state.to_path_buf(),
            factory: Mutex::new(factory),
            backends: Mutex::new(std::collections::HashMap::new()),
            closing: Mutex::new(false),
            notifier,
            desk,
            seen: Mutex::new(std::collections::HashSet::new()),
            budget: Mutex::new((DEFAULT_WORK_CONTEXT_WINDOW_TOKENS, DEFAULT_WORK_WATERMARK_RATIO)),
            rotation_command_wait: Mutex::new(ROTATION_WAITS_FOR_COMMANDS),
            // Nothing can be read until the shell says what to read with. Honest by
            // construction: with no reader, an assignment is pinned with `head: None` and
            // recovery says it could not look (`recovery.rs`).
            repositories: Mutex::new(Box::new(crate::recovery::UnreadableRepositories)),
            // No reader until the shell installs one. `Unknown` never blocks, so this is a
            // host that loses the feature rather than one that loses the work.
            screen: Mutex::new(Arc::new(crate::screen::UnknownScreen)),
            screen_poll: Mutex::new(crate::screen::SCREEN_POLL),
            quota: Mutex::new(None),
            operator: Mutex::new(None),
            answered_close: Mutex::new(None),
            central_root: Mutex::new(None),
            command_wait: Mutex::new(COMMAND_WAIT_BUDGET),
            worker_wait: Mutex::new(WORKER_WAIT_BUDGET),
            output: Mutex::new(None),
            retry_clock: Mutex::new(Arc::new(crate::upstream::SystemRetryClock)),
            sign_in: Mutex::new(None),
            hold_check: Mutex::new(HOLD_CHECK_EVERY),
        })
    }

    /// **How the host reads whether a held queue's sign-in has come back** — no model turn,
    /// only the account's own sign-in state (the shell's `claude auth status`, the same reading
    /// Settings shows). Installed by the shell.
    pub fn set_sign_in(&self, reading: SignInReading) {
        *self.sign_in.lock().unwrap() = Some(reading);
    }

    /// [`HOLD_CHECK_EVERY`], shortened. Test scaffolding, same reason and shape as
    /// [`Self::set_screen_poll`].
    #[doc(hidden)]
    pub fn set_hold_check_every(&self, every: std::time::Duration) {
        *self.hold_check.lock().unwrap() = every;
    }

    /// **A sign-in has just completed** (the shell's word, from the sign-in Settings runs). Every
    /// queue held for sign-in starts its first held job now rather than at its next look. It is
    /// a clearance like any other: if that job still finds nobody signed in, the queue stays held
    /// and nothing else is started.
    pub fn signed_in(&self) {
        let backends: Vec<Arc<Backend>> = self.backends.lock().unwrap().values().cloned().collect();
        for backend in backends {
            let mut inner = backend.inner.lock().unwrap();
            if let Some(held) = inner.held.as_mut().filter(|held| held.cause == HoldCause::SignIn) {
                held.trial = true;
                backend.wake.notify_all();
            }
        }
    }

    /// Replace the clock an overload retry waits on. Tests only; the app keeps the real one.
    pub fn set_retry_clock(&self, clock: Arc<dyn crate::upstream::RetryClock>) {
        *self.retry_clock.lock().unwrap() = clock;
    }

    /// Attach the output record (Output side panel PRD §3, §4.1 (b)/(c)). With it, the back
    /// end's `writes.jsonl` — its own writes, its workers', and the files its commands made —
    /// is projected into the assignment's thread after every back-end turn and on every pass
    /// of the worker wait. Without it (every test that does not ask) nothing is recorded.
    pub fn set_output_store(&self, store: crate::output::OutputStore) {
        *self.output.lock().unwrap() = Some(store);
    }

    /// Witnesses (b) and (c) for one back-end session, joined to the assignment it is working
    /// (`BackendWork::for_assignment`: its thread, and the turn he gave the work in). A failure
    /// is logged and never fails the work (§4.7).
    fn project_output(&self, record: &Assignment, session: &str) {
        let Some(store) = self.output.lock().unwrap().clone() else { return };
        let join = crate::output::SessionJoin::from_parts(
            Vec::new(),
            vec![crate::output::BackendWork::for_assignment(record, session)],
        );
        if let Err(e) = store.project_session(&record.thread_id, &self.state.join("evidence"), session, &join) {
            eprintln!("[richos] output: the back end's written files could not be recorded: {e}");
        }
    }

    /// How long `run_one` waits for a background command before handing it to the watch.
    /// Test scaffolding, same reason and shape as [`Self::set_screen_poll`].
    #[doc(hidden)]
    pub fn set_command_wait_budget(&self, budget: std::time::Duration) {
        *self.command_wait.lock().unwrap() = budget;
    }

    /// [`WORKER_WAIT_BUDGET`], shortened. Test scaffolding, same reason and shape as
    /// [`Self::set_command_wait_budget`].
    #[doc(hidden)]
    pub fn set_worker_wait_budget(&self, budget: std::time::Duration) {
        *self.worker_wait.lock().unwrap() = budget;
    }

    /// **How an assignment the back end handled itself is closed in the engine** — see
    /// [`AnsweredClose`]. Installed by the shell.
    pub fn set_answered_close(&self, close: Arc<dyn AnsweredClose>) {
        *self.answered_close.lock().unwrap() = Some(close);
    }

    /// **Where his company notes are** (the shell's `company::install_central_root`, the same
    /// folder the interview saves them in), so every assignment receives the same scoped facts.
    pub fn set_central_root(&self, root: PathBuf) {
        *self.central_root.lock().unwrap() = Some(root);
    }

    /// **Hand every assignment from here on to his team** (operator mode, decided once at
    /// launch by the shell). See [`OperatorIntake`].
    pub fn set_operator(&self, intake: Arc<dyn OperatorIntake>) {
        *self.operator.lock().unwrap() = Some(intake);
    }

    pub fn set_quota(&self, quota: Arc<crate::quota::Service>) {
        *self.quota.lock().unwrap() = Some(quota);
    }

    /// Wait between background turns, never cancel a turn to enforce a quota hold.
    /// No lease/config lock is acquired here. Stop and quit remain reachable.
    ///
    /// `started`: the back end has already taken a turn of this run (a continuation, a wait, a
    /// report). The hold then says so in its own detail ([`QUOTA_WAIT_AFTER_START`]), because
    /// recovery reads `WaitingForQuota` as "nothing was asked of the back end" only when it is
    /// not that (the work-path design D6: a started job is never re-run by itself).
    fn quota_gate(&self, backend: &Arc<Backend>, record: &Assignment, started: bool) -> bool {
        let quota = self.quota.lock().unwrap().clone();
        let Some(quota) = quota else { return true };
        let mut waiting = false;
        let mut observation: Option<crate::quota::holds::Guard> = None;
        loop {
            let inner = backend.inner.lock().unwrap();
            if inner.closing || inner.stopped.iter().any(|id| id == &record.id) { return false; }
            let admission = quota.view().admission;
            if admission.allows_work() {
                if let Some(guard) = observation.take() { guard.release(); }
                let next = if inner.live.is_some() { AssignmentState::Running } else { record.state };
                drop(inner);
                if waiting {
                    let _publication = assignment::advance(&self.state, &record.entity_id, &record.thread_id, &record.id,
                        next, "The allowance is available. Continuing where it paused.");
                }
                return true;
            }
            if !waiting {
                observation = crate::quota::holds::Guard::begin(&self.state, crate::quota::holds::Hold {
                    kind: "assignment".into(), id: record.id.clone(), entity_id: record.entity_id.clone(),
                    thread_id: record.thread_id.clone(), session_id: String::new(),
                    name: record.title.clone(), task: None, since_at: crate::util::now_millis(), released_at: None,
                }).ok();
                let detail = match (admission, started) {
                    (crate::quota::Admission::Held { .. }, false) => "Waiting for the allowance to refresh. Work is saved and will continue automatically.",
                    (_, false) => "Waiting for a current allowance reading before continuing. Work is saved.",
                    (crate::quota::Admission::Held { .. }, true) => QUOTA_WAIT_AFTER_START[0],
                    (_, true) => QUOTA_WAIT_AFTER_START[1],
                };
                let _publication = assignment::advance(&self.state, &record.entity_id, &record.thread_id, &record.id,
                    AssignmentState::WaitingForQuota, detail);
                waiting = true;
            }
            drop(backend.wake.wait_timeout(inner, std::time::Duration::from_millis(250)).unwrap());
        }
    }

    /// How often a screen wait looks. Test scaffolding, same reason and same shape as
    /// [`Self::set_context_budget`].
    #[doc(hidden)]
    pub fn set_screen_poll(&self, poll: std::time::Duration) {
        *self.screen_poll.lock().unwrap() = poll;
    }

    /// **How the Mac's screen is read for the CEO's ruling §56 wait.** Installed by the shell,
    /// which is the only place a framework call belongs.
    pub fn set_screen(&self, screen: Arc<dyn crate::screen::ScreenSource>) {
        *self.screen.lock().unwrap() = screen;
    }

    /// One reading of the screen, for a caller that wants to look rather than wait.
    pub fn screen_reading(&self) -> crate::screen::ScreenReading {
        self.screen.lock().unwrap().read()
    }

    /// The window and the watermark, same meaning as [`crate::spine::Spine::set_context_budget`].
    /// Used by tests to make a rotation happen without a million tokens of traffic.
    pub fn set_context_budget(&self, window_tokens: usize, watermark_ratio: f64) {
        *self.budget.lock().unwrap() = (window_tokens.max(1), watermark_ratio.clamp(0.0, 1.0));
    }

    /// How long a due renewal waits for a running command ([`ROTATION_WAITS_FOR_COMMANDS`]).
    pub fn set_rotation_command_wait(&self, wait: std::time::Duration) {
        *self.rotation_command_wait.lock().unwrap() = wait;
    }

    /// Whether `thread`'s back-end lease lock is held right now: the probe a fake lease's
    /// `Drop` uses to prove a retired lease ends with no lock held (reap gap C6).
    #[cfg(test)]
    fn lease_locked(&self, thread: &str) -> bool {
        self.backends.lock().unwrap().get(thread).is_some_and(|backend| backend.lease.try_lock().is_err())
    }

    /// How his repositories are read for the start-of-assignment pin (spec §6.1).
    pub fn set_repositories(&self, repositories: Box<dyn crate::recovery::Repositories>) {
        *self.repositories.lock().unwrap() = repositories;
    }

    /// **Remember which company a conversation belongs to, without starting anything.**
    ///
    /// After a relaunch, an assignment that was waiting for him is still waiting for him
    /// (§7.8, `recovery.rs`), and his approval has to be able to put it back on a lease.
    /// [`Self::resume`] needs the ledger's thread binding to do that and must never rebuild
    /// one — only the ledger can say which company a thread belongs to (`entity.rs`) — so
    /// the boot sweep hands it over here.
    ///
    /// **It enqueues nothing.** §6.3 forbids anything that restarts work by itself, and
    /// this is the seam that makes his decision POSSIBLE rather than a seam that takes it
    /// for him. It does open that conversation's back-end desk (a parked thread on a
    /// condition variable, no provider connection — [`Self::ensure_lease`] is still lazy),
    /// which is the cost of being able to answer him at all.
    pub fn remember_binding(self: &Arc<Self>, binding: &ThreadBinding) {
        if let Some(backend) = self.backend_for(binding.thread_id()) {
            backend.inner.lock().unwrap().binding = Some(binding.clone());
        }
    }

    /// The existing filesystem watcher wakes the runner and forwards corrections
    /// through its current session, including while that runner is inside a job.
    pub fn wake_messages(self:&Arc<Self>,binding:&ThreadBinding) {
        let Some(backend)=self.backend_for(binding.thread_id()) else {return};
        let (input, after, session) = {
            let mut inner=backend.inner.lock().unwrap();
            inner.binding=Some(binding.clone());
            if inner.closing {return}
            (inner.peer_input.clone(), inner.peer_active_through.unwrap_or(0), inner.lease_session.clone())
        };
        backend.wake.notify_all();
        let Some(input)=input else {return};
        let scope=self.message_scope(binding);
        let result=(|| {
            let Some((message,through))=crate::session_tools::pending_live_after(&scope,"stu",after)? else {return Ok::<_,String>(())};
            use sha2::Digest;
            let digest=sha2::Sha256::digest(format!("{}\0{}\0{through}",scope.entity_id,scope.thread_id));
            let id=uuid::Uuid::from_slice(&digest[..16]).unwrap().to_string();
            let text=serde_json::to_string(&message).map_err(|e|e.to_string())?;
            let host=Arc::downgrade(self);
            let binding=binding.clone();
            if let Some(session)=session {
                crate::session_tools::begin_attempt(&scope,"stu",&format!("peer:{through}"),&session)?;
            }
            input.send_input(&id,&text,Box::new(move || {
                if let Err(error)=crate::session_tools::acknowledge(&scope,"stu",through) {
                    eprintln!("[richos] operator input was taken but its receipt remains pending: {error}");
                    return;
                }
                if let Some(host)=host.upgrade() {host.wake_messages(&binding)}
            }))?;
            Ok(())
        })();
        if let Err(error)=result {eprintln!("[richos] operator live input remains pending: {error}");}
    }

    fn publish_peer_input(self:&Arc<Self>,backend:&Arc<Backend>,binding:&ThreadBinding) {
        {
            let mut inner=backend.inner.lock().unwrap();
            inner.peer_input=inner.cancel.clone();
        }
        self.wake_messages(binding);
    }

    fn message_scope(&self,binding:&ThreadBinding)->crate::status_tools::StatusToolScope {
        crate::status_tools::StatusToolScope {version:1,state_root:self.state.clone(),
            entity_id:binding.entity_id().to_string(),thread_id:binding.thread_id().to_string()}
    }

    fn run_message(self:&Arc<Self>,backend:&Arc<Backend>,binding:&ThreadBinding,
        message:&crate::session_tools::Message,turn:&str)->Result<String,String> {
        // A native connection owns its workers as well as its shell commands.
        let workers_finished=|| {
            let session=backend.inner.lock().unwrap().lease_session.clone();
            session.is_none_or(|session| {
                let view=crate::app_workers::status(&self.state,Some(&session));
                view.is_attributed() && view.active + view.liveness_unknown == 0
            })
        };
        // Peer delivery uses the same connection lifecycle as assignment delivery.
        if workers_finished() {
            self.renew_if_repositories_changed(backend,binding);
            self.renew_if_account_changed(backend,binding);
            self.renew_if_model_changed(backend,binding);
            self.renew_if_team_changed(backend,binding);
        }
        let original=serde_json::to_string(message).map_err(|e|e.to_string())?;
        let mut input=original.clone();
        loop {
            self.ensure_lease(backend,binding)?;
            let mut guard=backend.lease.lock().unwrap();
            let lease=guard.as_mut().ok_or("The operator connection closed")?;
            lease.prepare_agent_message(binding,turn,message).map_err(|e|e.to_string())?;
            {
                let mut inner=backend.inner.lock().unwrap();
                inner.cancel=lease.cancel_handle();
                inner.peer_stopper=inner.cancel.clone().map(|handle|(lease.session_id().to_string(),handle));
                inner.peer_active_through=turn.strip_prefix("peer:").and_then(|n|n.parse().ok());
            }
            let session=lease.session_id().to_string();
            crate::session_tools::begin_attempt(&self.message_scope(binding),"stu",turn,&session)?;
            let mut diagnostics=String::new();
            let mut published=false;
            let result=lease.prompt(&input,&mut |item| {
                if !published {published=true;self.publish_peer_input(backend,binding);}
                if let TurnItem::Text {text,..}=item {diagnostics.push_str(text)}
            });
            let result=result.and_then(|reason|lease.work_readiness_after_turn().map(|_|reason));
            {
                let mut inner=backend.inner.lock().unwrap();
                inner.peer_input=None;inner.peer_active_through=None;inner.cancel=None;
            }
            if matches!(&result,Err(CognitionError::Io(_))) {drop(guard.take());}
            let reason=result.map_err(|e|e.to_string())?;
            if reason != "end_turn" {
                return Err(format!("Operator input did not complete: {reason}"));
            }
            if crate::upstream::UpstreamFailure::classify_lines(&diagnostics).is_some() {return Err(diagnostics)}
            drop(guard);
            // A saved teammate is registered only by a new provider connection.
            // Continue this same accepted handoff after renewal, never dispatch it again.
            if workers_finished() { self.renew_if_team_changed(backend,binding); }
            if backend.inner.lock().unwrap().lease_session.as_deref() == Some(&session) {
                return Ok(session);
            }
            input=format!("{TEAM_CHANGED_CONTINUATION}\n{original}");
        }
    }

    /// **The turn boundary, and the whole of it.** Spec §1.1: the assignment is recorded,
    /// the receipt is written, and the turn is over.
    ///
    /// Everything expensive happens after this returns. `prepare` — the measured 3.0–3.8 s
    /// of guards and workspace creation, against a 300 s budget (spec §7.1) — runs on the
    /// work lease, on the runner thread, after the conversation's turn has ended. So does
    /// spawning the lease, which is the other multi-second term.
    ///
    /// A failure here is a **failed registration** and the caller says so in those words
    /// (spec §1.4, [`assignment::failed_registration_sentence`]). It is never softened into
    /// "I have started it", because at this point nothing has been started.
    pub fn register(
        self: &Arc<Self>,
        binding: &ThreadBinding,
        request: &Registration,
    ) -> Result<assignment::Receipt, String> {
        self.register_kind(binding, request, assignment::AssignmentKind::Task)
    }

    /// The same, for a QUESTION of his as well as a piece of work (CEO ruling §58). The
    /// queueing, the failure path and the receipt are identical; only what gets written down
    /// differs, which is what decides the sentence and the word beside his timer.
    pub fn register_kind(
        self: &Arc<Self>,
        binding: &ThreadBinding,
        request: &Registration,
        kind: assignment::AssignmentKind,
    ) -> Result<assignment::Receipt, String> {
        let receipt = assignment::register_kind(&self.state, request, kind).map_err(|e| e.to_string())?;
        let record = assignment::read(&self.state, &request.entity_id, &request.thread_id, &receipt.id)
            .map_err(|e| e.to_string())?;
        if !self.enqueue(binding, record) {
            let _ = assignment::advance(
                &self.state,
                &request.entity_id,
                &request.thread_id,
                &receipt.id,
                AssignmentState::Failed,
                "RichOS is closing, so this was not started.",
            );
            return Err("RichOS is closing down. Nothing was started.".into());
        }
        Ok(receipt)
    }

    /// Wake the peer inbox at a conversation boundary. An operational record never
    /// independently starts desktop work. Only the separate operator client adopts
    /// records into its explicit intake; that client has no peer handoff path.
    pub fn adopt_registered(self: &Arc<Self>, binding: &ThreadBinding) -> usize {
        self.wake_messages(binding);
        // Desktop work is dispatched only through Stu's durable peer input.
        // The separate operator client retains its explicit assignment intake.
        if self.operator.lock().unwrap().is_none() { return 0; }
        let Ok(rows) = assignment::read_all(&self.state, &binding.entity_id().to_string(), binding.thread_id())
        else {
            return 0;
        };
        let mut adopted = 0;
        for record in rows.into_iter().filter(|row| row.state == AssignmentState::Registered) {
            if self.seen.lock().unwrap().contains(&record.id) {
                continue;
            }
            if self.enqueue(binding, record) {
                adopted += 1;
            }
        }
        adopted
    }

    /// Put one assignment on the queue exactly once. `false` means the host is closing.
    fn enqueue(self: &Arc<Self>, binding: &ThreadBinding, record: Assignment) -> bool {
        if !self.seen.lock().unwrap().insert(record.id.clone()) {
            return true;
        }
        // **On an operator install the assignment is his team's, and never a lease's.** The
        // intake closes it itself if it cannot hand it over, so it is never left registered.
        let operator = self.operator.lock().unwrap().clone();
        if let Some(operator) = operator {
            operator.take(record);
            return true;
        }
        self.schedule(binding, record, false)
    }

    /// Put an assignment on the queue, first time or again. `false` means the host is
    /// closing. The `seen` ledger is deliberately NOT consulted here: a resume is a second
    /// pass over an assignment this host has already adopted, which is exactly what `seen`
    /// refuses for the boundary sweep.
    fn schedule(self: &Arc<Self>, binding: &ThreadBinding, record: Assignment, resumed: bool) -> bool {
        let Some(backend) = self.backend_for(&record.thread_id) else { return false };
        let mut inner = backend.inner.lock().unwrap();
        if inner.closing {
            return false;
        }
        inner.binding = Some(binding.clone());
        // A fresh job that joins a held queue waits with it, and its row says so. Written under
        // `inner`, so a stop that takes it off the queue writes its own ending after this.
        if let Some(held) = inner.held.as_ref().filter(|_| !resumed) {
            if let Err(error) = assignment::advance(&self.state, &record.entity_id, &record.thread_id, &record.id,
                AssignmentState::Registered, &held.cause.detail())
            {
                eprintln!("[richos] work: a job joining a held queue could not say so on its row: {error}");
            }
        }
        inner.queue.push_back(Scheduled { binding: binding.clone(), record, resumed });
        backend.wake.notify_all();
        true
    }

    /// **This conversation's back end, opened once and standing from then on.**
    ///
    /// The CEO's page decides the unit: *"each conversation thread always holds one front
    /// desk Rich and one back-end Rich. Regardless of the number of assignments."* So the
    /// first assignment on a thread opens that thread's back end and starts its runner; every
    /// assignment after it, including a resume, finds the same one. `None` means the app is
    /// closing, which is the one state in which a new back end must not be opened.
    ///
    /// **The provider connection itself is still opened lazily, inside the runner**
    /// ([`Self::ensure_lease`]) — the seconds it costs belong after the CEO's turn, never
    /// inside it (spec §7.1). What is created here is the desk, the queue and the thread that
    /// serves them.
    fn backend_for(self: &Arc<Self>, thread: &str) -> Option<Arc<Backend>> {
        if *self.closing.lock().unwrap() {
            return None;
        }
        let mut backends = self.backends.lock().unwrap();
        if let Some(existing) = backends.get(thread) {
            return Some(Arc::clone(existing));
        }
        let backend = Backend::new();
        backends.insert(thread.to_string(), Arc::clone(&backend));
        drop(backends);
        let host = Arc::clone(self);
        let runner = Arc::clone(&backend);
        std::thread::Builder::new()
            // Named per conversation, so a stack from a stuck back end says which one.
            .name(format!("richos-back-end:{}", &thread[..thread.len().min(24)]))
            .spawn(move || host.run(runner))
            .expect("a back end could not be started");
        Some(backend)
    }

    /// Open for business. Back ends are opened per conversation on their first assignment, so
    /// there is no single runner to start any more — this marks the host live and is kept
    /// because the shell and the tests say it, and because a host that has been shut down
    /// must not silently accept work again.
    pub fn start(self: &Arc<Self>) {
        *self.closing.lock().unwrap() = false;
    }

    fn run(self: Arc<Self>, backend: Arc<Backend>) {
        enum Next {
            Work(Scheduled),
            Message(ThreadBinding,crate::session_tools::Message,u64),
            Answer(ThreadBinding,crate::questions::Delivery),
            Report(Watched, Vec<String>),
            Lost(Watched),
        }
        loop {
            let next = {
                let mut inner = backend.inner.lock().unwrap();
                loop {
                    if inner.closing {
                        return;
                    }
                    if let Some(binding)=inner.binding.clone() {
                        if let Ok(answers)=crate::question_work::pending(&self.state) {
                            if let Some(answer)=answers.into_iter().find(|d|d.asker=="stu" && d.entity_id==binding.entity_id().as_str() && d.thread_id==binding.thread_id()) {
                                inner.processing=true;break Next::Answer(binding,answer)
                            }
                        }
                        match crate::session_tools::pending(&self.message_scope(&binding),"stu") {
                            Ok(Some((message,through)))=>{inner.processing=true;break Next::Message(binding,message,through)},
                            Ok(None)=>{},
                            Err(error)=>eprintln!("[richos] operator inbox: {error}"),
                        }
                    }
                    // **A WATCHED JOB WHOSE COMMAND HAS ENDED GOES FIRST** ([`Watched`]): it
                    // ended before the next job starts, and its report is one short turn. The
                    // reading is the lease's in-memory reader state, taken with `inner`
                    // released, because the lease lock is always taken before `inner`.
                    if !inner.watching.is_empty() {
                        let watching = inner.watching.clone();
                        let session = inner.lease_session.clone();
                        drop(inner);
                        let verdict = self.watched_verdict(&backend, &watching, session.as_deref());
                        inner = backend.inner.lock().unwrap();
                        if inner.closing {
                            return;
                        }
                        if let Some((id, ended)) = verdict {
                            // His Stop may have taken it off the watch meanwhile.
                            if let Some(at) = inner.watching.iter().position(|w| w.record.id == id) {
                                let watched = inner.watching.remove(at);
                                inner.processing = true;
                                break match ended {
                                    Some(ended) => Next::Report(watched, ended),
                                    None => Next::Lost(watched),
                                };
                            }
                            continue;
                        }
                    }
                    // **A HELD QUEUE STARTS NOTHING UNTIL ITS CAUSE CLEARS** ([`Held`]). Every
                    // [`HOLD_CHECK_EVERY`] the host looks at the cause itself, with no model
                    // turn; when it has cleared, the first held job is taken as the trial.
                    if inner.held.is_some() && inner.queue.is_empty() {
                        // Everything it held was stopped: there is nothing left to hold.
                        inner.held = None;
                    }
                    if let Some(held) = inner.held.as_ref().filter(|held| !held.trial) {
                        let every = *self.hold_check.lock().unwrap();
                        let since = held.checked_at.elapsed();
                        if since < every {
                            let wait = if inner.watching.is_empty() { every - since } else { (every - since).min(COMMAND_WAIT_POLL) };
                            inner = backend.wake.wait_timeout(inner, wait).unwrap().0;
                            continue;
                        }
                        let (cause, binding) = (held.cause.clone(), inner.binding.clone());
                        drop(inner);
                        let cleared = self.hold_cleared(&backend, &cause, binding.as_ref());
                        inner = backend.inner.lock().unwrap();
                        if inner.closing {
                            return;
                        }
                        if let Some(held) = inner.held.as_mut() {
                            held.checked_at = std::time::Instant::now();
                            held.trial |= cleared;
                        }
                        continue;
                    }
                    if let Some(item) = inner.queue.pop_front() {
                        inner.processing = true;
                        break Next::Work(item);
                    }
                    // Parked on the condition variable as before; while a command is watched,
                    // it also looks again every [`COMMAND_WAIT_POLL`].
                    inner = if inner.watching.is_empty() {
                        backend.wake.wait(inner).unwrap()
                    } else {
                        backend.wake.wait_timeout(inner, COMMAND_WAIT_POLL).unwrap().0
                    };
                }
            };
            let binding = match next {
                Next::Message(binding,message,through)=>{
                    if let Err(error)=self.run_message(&backend,&binding,&message,&format!("peer:{through}"))
                        .and_then(|_|crate::session_tools::acknowledge(&self.message_scope(&binding),"stu",through)) {
                        eprintln!("[richos] operator message remains pending: {error}");
                        // The ordinary host owns retry and shutdown. No message is acknowledged on failure.
                        let mut inner=backend.inner.lock().unwrap();
                        inner.processing=false;
                        if inner.closing {return}
                        inner=backend.wake.wait_timeout(inner,*self.hold_check.lock().unwrap()).unwrap().0;
                        drop(inner);
                    }
                    backend.inner.lock().unwrap().completed+=1;
                    backend.wake.notify_all();
                    binding
                }
                Next::Answer(binding,delivery)=>{
                    let result=self.question_answer_instruction(&delivery).and_then(|instruction| {
                        let message=crate::session_tools::Message {handoff:false,from:"user".into(),text:delivery.text.clone(),instruction};
                        self.run_message(&backend,&binding,&message,&format!("answer:{}",delivery.id))
                    });
                    match result {
                        Ok(session)=>if let Err(error)=crate::question_work::taken(&self.state,std::slice::from_ref(&delivery.id),&session)
                            .and_then(|_|crate::session_tools::complete_attempt(&self.message_scope(&binding),"stu",&format!("answer:{}",delivery.id))) {
                            eprintln!("[richos] operator answer acknowledgement: {error}");
                        },
                        Err(error)=>{
                            eprintln!("[richos] operator answer remains saved: {error}");
                            let inner=backend.inner.lock().unwrap();
                            if inner.closing {return}
                            drop(backend.wake.wait_timeout(inner,*self.hold_check.lock().unwrap()).unwrap());
                        }
                    }
                    backend.inner.lock().unwrap().completed+=1;
                    backend.wake.notify_all();
                    binding
                }
                Next::Work(Scheduled { binding, record, resumed }) => {
                    // **A stop that landed between the dequeue and the start.** The ordinary
                    // queued-stop path never reaches here — `stop_assignment` takes that
                    // assignment off the queue itself — so this is the race: the stop was
                    // recorded a moment after this runner had already picked the assignment
                    // up. It still ENDS, so it takes the same boundary as every other ending
                    // below rather than an early `continue` that skips §2.4a's report; a
                    // windowless app whose last assignment ended down this path would
                    // otherwise sit there waiting for something that was never coming.
                    let ran = !backend.inner.lock().unwrap().stopped.iter().any(|id| *id == record.id);
                    if ran {
                        self.run_one(&backend, &binding, &record, resumed, None);
                        let mut inner = backend.inner.lock().unwrap();
                        // The trial job was not held again: its cause has cleared, and the rest
                        // of the queue follows it in order (Sage's check §1.1).
                        if inner.held.as_ref().is_some_and(|held| held.trial) {
                            inner.held = None;
                        }
                        inner.completed += 1;
                        inner.live = None;
                        inner.carrying.clear();
                        backend.wake.notify_all();
                    } else {
                        self.settle_stopped(&backend, &record);
                    }
                    binding
                }
                Next::Report(watched, ended) => {
                    // Only a job that is still open is asked about: anything that closed it
                    // meanwhile (a quit's sweep, a path that forgot the watch) already spoke.
                    let open = assignment::read(&self.state, &watched.record.entity_id, &watched.record.thread_id,
                                                &watched.record.id)
                        .is_ok_and(|row| row.state.is_open());
                    if open {
                        let report = Report { ended, told: watched.told.clone() };
                        self.run_one(&backend, &watched.binding, &watched.record, false, Some(report));
                    } else {
                        eprintln!("[richos] work: a watched assignment had already closed when its command ended; nothing was asked");
                    }
                    let mut inner = backend.inner.lock().unwrap();
                    inner.completed += 1;
                    inner.live = None;
                    inner.carrying.clear();
                    backend.wake.notify_all();
                    drop(inner);
                    watched.binding
                }
                Next::Lost(watched) => {
                    self.watch_lost(&watched);
                    let mut inner = backend.inner.lock().unwrap();
                    inner.completed += 1;
                    backend.wake.notify_all();
                    drop(inner);
                    watched.binding
                }
            };
            // **THE ASSIGNMENT BOUNDARY, AND IT IS THE ONLY PLACE EITHER OF THESE HAPPENS.**
            //
            // Rotation first: a back end that has crossed its watermark is retired here,
            // between two assignments, never inside one. The continuity design's §3.1 rule
            // — rotation never happens inside a turn — is structural on this path rather
            // than checked: this line cannot be reached while a work turn is in flight,
            // because `run_one` has returned and `live` is `None`.
            // A back end that never ran anything has consumed nothing, so this is a no-op
            // on the stopped-while-queued path rather than a special case.
            self.rotate_if_needed(&backend, &binding);
            // Then §2.4a's report. It asks the REGISTER rather than this back end, because
            // the question is whether anything at all is still registered anywhere — a
            // second conversation's running assignment must keep the app alive just as
            // this one's would.
            {
                let mut inner=backend.inner.lock().unwrap();
                inner.peer_input=None;inner.peer_active_through=None;inner.processing=false;
            }
            if self.open_assignments().map(|open| open.is_empty()).unwrap_or(false) {
                self.notifier.nothing_left_to_do();
            }
            {
                let mut inner=backend.inner.lock().unwrap();
                inner.peer_input=None;inner.peer_active_through=None;inner.processing=false;
            }
            backend.wake.notify_all();
        }
    }

    /// **WAIT FOR THE SCREEN IF THIS ASSIGNMENT NEEDS IT** — the CEO's ruling §56.
    ///
    /// Returns `true` when the assignment may go on, `false` when the runner must put it down
    /// — which happens only on a stop or a quit, and in both of those cases the state on disk
    /// is written by the path that did the stopping (`stop_assignment`, `shutdown`) rather than
    /// here. That division matters: two writers for one ending is how a receipt ends up saying
    /// `running` after a quit.
    ///
    /// **Nothing is said to him.** No notice is raised, because §56 is a wait that looks after
    /// itself and a push about one would be a nag. The state and its detail are the whole of
    /// the visibility, and they are there for as long as the wait lasts — which is why
    /// `on_wait` writes them rather than the caller writing them afterwards.
    fn screen_gate(
        self: &Arc<Self>,
        backend: &Arc<Backend>,
        record: &Assignment,
        advance: &impl Fn(AssignmentState, &str),
    ) -> bool {
        if !record.needs_screen {
            return true;
        }
        let source = Arc::clone(&*self.screen.lock().unwrap());
        let poll = *self.screen_poll.lock().unwrap();
        let watch = crate::screen::ScreenWatch::with_poll(source, poll);
        {
            // **The two races this block closes, both of which end with a runner parked on a
            // locked screen that nobody can reach.** A stop or a quit that landed between this
            // assignment being dequeued and this line would otherwise be invisible to a wait
            // that had not begun yet — so the watch is pre-stopped instead of hoping the
            // ordering held.
            let mut inner = backend.inner.lock().unwrap();
            if inner.closing || inner.stopped.iter().any(|held| *held == record.id) {
                watch.stop();
            }
            inner.screen_wait = Some((record.id.clone(), Arc::clone(&watch)));
        }
        let outcome = watch.wait_for_screen(|_| {
            advance(AssignmentState::WaitingForScreen, crate::screen::says::detail());
        });
        backend.inner.lock().unwrap().screen_wait = None;
        outcome.is_available()
    }

    /// One assignment, start to the end of its back end's own turn.
    ///
    /// With a [`Report`], it is a WATCHED job coming back ([`Watched`]): its background
    /// command has ended, and the back end is asked for this job's report on this job's seat.
    /// Nothing about starting the job is repeated — no screen wait, no renewal, no new pin, no
    /// `Preparing` — and the rest (the waits, the settle arms) is the same path every job
    /// takes, so a report that starts another command is watched again like any other.
    fn run_one(
        self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding, record: &Assignment, resumed: bool,
        report: Option<Report>,
    ) {
        let reporting = report.is_some();
        if !self.quota_gate(backend, record, reporting) {
            self.settle_stopped(backend, record);
            self.let_go_if_ended(record);
            return;
        }
        let scope = (record.entity_id.clone(), record.thread_id.clone(), record.id.clone());
        let advance = |to: AssignmentState, detail: &str| {
            let _ = assignment::advance(&self.state, &scope.0, &scope.1, &scope.2, to, detail);
        };

        // 0. **THE SCREEN, AND IT IS STEP ZERO BECAUSE NOTHING MAY HAVE BEEN ASKED OF THE
        //    BACK END WHILE THIS WAITS.** The CEO's ruling §56: *"when Rich needs the screen
        //    and it is locked, the app waits for the unlock on its own and carries on."*
        //
        //    Being strictly before `ensure_lease` is what makes every other decision about
        //    this state honest: no provider connection is spent sitting on a locked screen, no
        //    seat is bound, nothing is in flight, and so `recovery.rs` can say truthfully that
        //    an assignment found here had not started. Putting it one line later would have
        //    made all three of those sentences false.
        //
        //    An unlocked or unreadable screen costs ONE reading and this block is invisible.
        //    A report is not a start: the job already ran, and it asks nothing of the screen.
        if !reporting && !self.screen_gate(backend, record, &advance) {
            self.let_go_if_ended(record);
            return;
        }

        if !reporting {
            advance(AssignmentState::Preparing, "Opening the work connection.");
        }

        // Resolve the frozen request before spending a work lease. The display title is
        // intentionally shortened and cannot carry all of the user's constraints. A report
        // sends no brief, so it needs none.
        let instruction = if reporting {
            String::new()
        } else {
            match instruction_for(&self.state, record) {
                Ok(text) => text,
                Err(why) => {
                    advance(AssignmentState::Failed, why);
                    self.raise(record, NoticeKind::Failed,
                        &assignment::says::failure(record.kind, &record.title, why, false));
                    self.let_go_if_ended(record);
                    return;
                }
            }
        };

        // 0b. A repository he connected since this back end opened (see
        //     `renew_if_repositories_changed`). Nothing is live yet, so this is a boundary.
        //     Never before a report: renewing would retire the one back end that knows where
        //     the command's output is.
        if !reporting {
            self.renew_if_repositories_changed(backend, binding);
            // 0c. Fill-first: this back end's account must be left (its freshest reading and
            //     measured speed, `claude_accounts.rs`) — move it now, at this boundary.
            self.renew_if_account_changed(backend, binding);
            // 0c'. §119: the model he chose in Preferences for directing the team, from the
            //      next job after it closed.
            self.renew_if_model_changed(backend, binding);
            // 0d. A teammate saved, refitted or retired since this back end opened (slice 5).
            //     After the two above, because either renewal registers the team as it is now.
            self.renew_if_team_changed(backend, binding);
        }

        // 1. The lease. Spawning it is seconds, and this is where those seconds belong —
        //    after the turn, never inside it. A report needs the lease its command ran on,
        //    which the runner checked a moment ago ([`Self::watched_verdict`]); a fresh one
        //    could not report it, so none is opened for it.
        if reporting && backend.lease.lock().unwrap().is_none() {
            self.report_lost(record);
            return;
        }
        if let Err(why) = self.ensure_lease(backend, binding) {
            // **A quit reached this back end while it was opening** (finding 28): the quit's
            // sweep has already written this job's honest state, `interrupted`, and a "did
            // not start" written after it would be the last word on a job he quit.
            if backend.inner.lock().unwrap().closing {
                self.let_go_if_ended(record);
                return;
            }
            // **An answer run that could not open its back end** (the work-path design's C10:
            // D4 takes this exit). His answer is saved and nothing was asked of anybody, so the
            // job waits on it again and is tried once more, within D4's bound.
            if !reporting && self.retry_answer_run(backend, binding, record, resumed) {
                eprintln!("[richos] work: the back end did not open for his answer ({why}); trying again");
                return;
            }
            // **A FRESH JOB IS HELD, NOT FAILED** (T3 idea 1, finished; Sage's check §1.1).
            // Nothing was asked of the back end, and it is the same back end for every job
            // queued behind this one, so this job and those wait together, and the host starts
            // them by itself once the back end opens. `4a54115d3` settled them all as Failed,
            // each to be asked for again by hand.
            if !reporting && !resumed {
                eprintln!("[richos] work: the back end did not open ({why}); this conversation's queue is held");
                self.hold(backend, binding, record, HoldCause::BackEnd(why));
                return;
            }
            // **"Did not start", not "stopped before it finished".** Nothing has been asked of
            // the back end at this line, so there is nothing that could have stopped. Ray's
            // candidate-.7 failures were all of this shape and all reported as the other one,
            // which invites the wrong question ("how far did it get?") about a job that got
            // nowhere at all.
            advance(AssignmentState::Failed, &honest(&why));
            // §58: a question that did not get answered is said as that, not as a job that
            // never started — `says::failure` picks the shape off the kind.
            self.raise(
                record,
                NoticeKind::Failed,
                &assignment::says::failure(record.kind, &record.title, &honest(&why), false),
            );
            self.let_go_if_ended(record);
            return;
        }

        // 2. The seat and the standing grant, per assignment (spec §5.4, §5.8c).
        let work = WorkAssignment {
            entity_id: record.entity_id.clone(),
            thread_id: record.thread_id.clone(),
            assignment_id: record.id.clone(),
            obligation_id: record.obligation_id.clone(),
            seat: record.seat.clone(),
            instruction_ledger_ref: record.instruction_ledger_ref.clone(),
            instruction_sha256: record.instruction_sha256.clone(),
            // The words verified above. A report reads none for its brief, so it reads them
            // here, and takes them only when they still verify: a report turn can prepare a
            // reviewer too, and must not be the one turn that briefs it without his words.
            instruction_text: if reporting {
                instruction_for(&self.state, record).ok()
            } else {
                Some(instruction.clone())
            },
        };
        let session = {
            let mut lease = backend.lease.lock().unwrap();
            let Some(lease) = lease.as_mut() else {
                advance(AssignmentState::Failed, "The work connection closed before the assignment started.");
                drop(lease);
                self.let_go_if_ended(record);
                return;
            };
            if let Err(why) = lease.bind_work_assignment(&work) {
                // Pre-turn, like `ensure_lease` above: the seat could not be bound, so the back
                // end was never asked for anything.
                advance(AssignmentState::Failed, &honest(&why.to_string()));
                self.raise(
                    record,
                    NoticeKind::Failed,
                    &assignment::says::failure(record.kind, &record.title, &honest(&why.to_string()), false),
                );
                self.let_go_if_ended(record);
                return;
            }
            let session = lease.session_id().to_string();
            let mut inner = backend.inner.lock().unwrap();
            // **The quit path reads this back end's cancel handle in the same critical section
            // that sets `closing`**, so exactly one of two things is true here: quit has not
            // begun, and the handle published below is the one it will cancel; or it has, and
            // it will never see this handle. In the second case the grant just bound is
            // revoked here and no turn starts, because quit no longer waits for this lease
            // (hunt 2026-09-29 part 1, finding 28) and nothing else would revoke it.
            if inner.closing {
                drop(inner);
                if let Err(error) = lease.revoke_work_assignment() {
                    eprintln!("[richos] work: a grant bound as RichOS closed could not be revoked: {error}");
                }
                self.let_go_if_ended(record);
                return;
            }
            inner.cancel = lease.cancel_handle();
            inner.live = Some(LiveAssignment {
                id: record.id.clone(),
                entity_id: record.entity_id.clone(),
                thread_id: record.thread_id.clone(),
                seat: record.seat.clone(),
            });
            drop(inner);
            // **WHAT A RELAUNCH WILL RECONCILE AGAINST, WRITTEN BEFORE THE WORK STARTS.**
            //
            // Spec §6.1 reconciles against the evidence file and against Git, and neither is
            // reachable after a crash unless the session and the repository heads were
            // written down first. This is the only moment both are known and the work has
            // not yet begun. It is on the WORK lease, after his turn ended (§7.1), so the
            // Git reads are not on the boundary his "answer in seconds" is measured at.
            //
            // A failure here is logged and never fatal: the work is more important than the
            // pin, and recovery already says honestly that it has nothing to compare
            // against when the pin is missing.
            //
            // A report keeps the pin its job's start wrote: it is the same job.
            if !reporting {
                let pins = crate::recovery::pins_for(record, self.repositories.lock().unwrap().as_ref());
                if let Err(error) =
                    assignment::note_start(&self.state, &record.entity_id, &record.thread_id, &record.id, &session, pins)
                {
                    eprintln!("[richos] work: this assignment's starting point was not recorded: {error}");
                }
            }
            session
        };

        // 3. The work turn. This is the long one, and nothing about it is on his turn.
        //
        // **IT IS STILL `Preparing` HERE, AND THAT IS RAY'S CANDIDATE-.7 ROW 2.** This line
        // used to write `Running` — before `prompt` below had been called, so before the back
        // end had been asked for anything, let alone answered. The old detail string on it
        // said "Preparing the workspace and starting the work", which is the state word the
        // record should have carried in the first place.
        //
        // What "running" is worth to him depends entirely on it being false until it is true:
        // on his walk the receipt said the job was running and the job failed three seconds
        // later, having never started. So `Running` is now written from ONE place, below,
        // when the child's first stream item proves it took the turn.
        //
        // A report leaves the row as it is: `Running`, its command having just ended.
        if !reporting {
            advance(
                AssignmentState::Preparing,
                if resumed {
                    "You approved the step it stopped at. Handing it back to the back end now."
                } else {
                    "Preparing the workspace and starting the work."
                },
            );
        }
        // **THE WATERMARK'S TWO INPUTS, READ AS THE TURN GOES PAST.** Same shape as the
        // conversation's (`spine.rs:1980, 2034-2041`) and for the same reason: a
        // `usage_update` lands in the evictable Tier B of the journal, so the stream is the
        // only place it can be consumed. They are accumulated locally and written to the
        // back end once, after the turn — a lock per streamed item would be the one cost
        // this file has spent a module doc avoiding.
        //
        // **What else this back end is still running for his other jobs** ([`Watched`]): the
        // provider folds a command's ending into whatever turn is running when it comes
        // (`cap-fold.jsonl`), so this turn is told to leave such an ending out of its own
        // answer. That job's report is asked for on its own seat when this one is done.
        //
        // **A watched job that is run again** (his answer to its question,
        // `queue_question_answer`) takes itself off the watch: its commands are its own again,
        // so the waits below treat them as this run's, and it keeps what he was already told.
        let (others, rewatched): (Vec<Watched>, Option<Watched>) = {
            let mut inner = backend.inner.lock().unwrap();
            let own = inner.watching.iter().position(|w| w.record.id == record.id).map(|at| inner.watching.remove(at));
            (inner.watching.clone(), own)
        };
        let mut prompt = match &report {
            Some(report) => {
                let ended: Vec<crate::cognition::BackgroundCommand> = self
                    .background_commands(backend)
                    .into_iter()
                    .filter(|c| report.ended.contains(&c.task_id))
                    .collect();
                format!(
                    "This is about his earlier request \"{}\", which you are carrying on this same seat. {}",
                    record.title,
                    command_ended_continuation(&ended.iter().collect::<Vec<_>>())
                )
            }
            // **Picked back up after RichOS closed on it** (C6, option B): it is registered again
            // and it had reached a back end before, so some of it may already be done. **And
            // started again after a usage limit cut it** (weekly-switch plan §3): the new back
            // end reads its receipts, handed-off workers included, before redoing anything.
            None if !resumed && ((record.state == AssignmentState::Registered && record.work_session.is_some())
                || (record.state == AssignmentState::Blocked && record.detail == LIMIT_SWITCH_DETAIL)) => {
                format!("{PICKED_UP_NOTE}\n\n{}", brief_for(record, resumed, &instruction))
            }
            None => brief_for(record, resumed, &instruction),
        };
        if let Some(notes) = self.company_notes(record) {
            prompt.push_str("\n\n");
            prompt.push_str(&notes);
        }
        // **HIS ANSWERS, CARRIED, AND NOT LET GO UNTIL THE BACK END HAS THEM** (the work-path
        // design D1, D5). Reading them changes nothing on disk: they are let go at this turn's
        // first item ([`Self::answers_taken`]), so a crash or a failed send before that leaves
        // them pending for the next run. Read and marked `carrying` under ONE hold of `inner`,
        // so the question worker never sees them pending and not carried while this prompt is
        // on its way (D2). Answers a back end took in a session that is gone are told again,
        // under their own heading: this session has never seen them (D5).
        let carried = {
            let mut inner = backend.inner.lock().unwrap();
            match crate::question_work::peek(&self.state, &record.entity_id, &record.thread_id, &record.obligation_id, &session) {
                Ok(mut carried) => {
                    let unsaved = &inner.taken_unsaved;
                    carried.new.retain(|d| !unsaved.iter().any(|(id, s)| *id == d.id && *s == session));
                    inner.carrying = carried.new.iter().map(|d| d.id.clone()).collect();
                    for d in &carried.new {
                        *inner.answer_starts.entry(d.id.clone()).or_insert(0) += 1;
                    }
                    carried
                }
                Err(error) => {
                    drop(inner);
                    advance(AssignmentState::Failed, &format!("The saved answer could not be read: {error}"));
                    self.let_go_if_ended(record);
                    return;
                }
            }
        };
        if !carried.earlier.is_empty() {
            prompt.push_str("\nAnswers he gave earlier on this assignment (a previous connection had them; they still apply):\n");
            for input in &carried.earlier { prompt.push_str(&input.text); prompt.push_str("\n\n"); }
        }
        if !carried.new.is_empty() {
            prompt.push_str("\nThe user answered your questions. These complete sets include every answer and withdrawal. Continue this assignment with these decisions:\n");
            for input in &carried.new { prompt.push_str(&input.text); prompt.push_str("\n\n"); }
            crate::operator_host::crash_point("WORK-CARRY"); // carried, nothing sent
        }
        // Let go at the first item of THIS prompt, and only of this one: the continuations below
        // carry no answers. A `RefCell` because the item callback and the reading after the send
        // both need it, on this one thread.
        let to_take: std::cell::RefCell<Option<Vec<String>>> =
            std::cell::RefCell::new((!carried.is_empty()).then(|| carried.ids()));
        prompt.push_str(&still_running_for_others(&others));
        // The SAME measure the spine takes, deliberately: the prompt sent plus the reply
        // that came back, in bytes (`spine.rs:2114-2118`). It is an undercount — by 2.3× to
        // 40.6×, measured (`spine.rs:138-143`) — which is exactly why it is the FALLBACK
        // and the adapter's own `usage_update` is what actually decides below.
        //
        // Cells, so a run that moves to another Claude account between two of its turns can
        // start the successor's count at zero (`switch_account_between_turns`): what the old
        // session consumed is not the new one's.
        let chars = std::cell::Cell::new(prompt.len());
        let measured: std::cell::Cell<Option<crate::machinery::ContextUsage>> = std::cell::Cell::new(None);
        // **THE ANSWER TO A QUESTION, KEPT — the CEO's ruling §58, 2026-09-18.**
        //
        // The back end's assistant text has always come past this closure and has only ever
        // been COUNTED (`chars += text.len()` below): its turns are never rendered, so
        // nothing needed the words. §58's *"The answer arrives on the timeline as an answer"*
        // needs exactly those words, and there is no other channel for them — `what_happened`
        // reads land receipts and can say what landed on which branch, which is not an answer
        // to a question and never will be.
        //
        // **Kept for every kind now, and bounded** (esc-20260927T093052Z-85f3303f). A TASK
        // that no helper ever worked on has no receipts to be read off, and the back end's
        // own words are the only account of it there is — the command it ran and what that
        // printed, or why it could not start (*"the Acme repository isn't connected"*). Until
        // this they were counted and dropped, and he was told "No work was started". They are
        // still never retained past this function and are spoken only through the one arm
        // that reads them for a task ("A TASK THE BACK END DID ITSELF", below). The cap is
        // `assignment::sanitize_answer`'s, applied once at the end rather than per item, so
        // a runaway back end costs memory for one turn and cannot write a record the app can
        // no longer read.
        let mut answer = String::new();
        let keeping_an_answer = true;
        // **THE ONE PLACE `Running` IS WRITTEN, AND THE EVIDENCE THAT MAKES IT TRUE.**
        //
        // Ray's candidate-.7 walk, row 2: *"It's running now"* was on his screen at
        // 08:13:07Z for a job that failed at 08:13:10Z. A CEO who reads the first answer and
        // looks away has been told something false, so the word has to be attached to a fact
        // rather than to a step the app has reached.
        //
        // The fact is this: a `TurnItem` is parsed off the child's own stream in reply to the
        // prompt just below. Its arrival is a POSITIVE signal from the back end that it has
        // taken the turn — the same standard the continuity design's §5.2 holds the crash
        // watchdog to, and for the same reason. Nothing here is inferred from elapsed time,
        // from the lease being open, or from silence.
        //
        // **Why not `work_readiness_after_turn`, which the brief for this change named.**
        // That reading is real and it stays exactly where it is (step 3a below), but it is
        // reachable only AFTER `prompt` has returned — after the whole work turn, the long
        // one. Gating the word on it would leave every assignment reading `preparing` for the
        // entire run and `running` only once the run had ended, which is the same defect
        // pointing the other way. The two are different questions: that one asks whether the
        // lease was ever equipped, this one asks whether the turn was taken.
        // A report's job has long since been `Running`; nothing is re-written for it.
        let mut confirmed = reporting;
        let started_detail: &str = if resumed {
            "You approved the step it stopped at. Carrying it out now."
        } else {
            STARTED_DETAIL
        };
        // **HOW MANY ITEMS THIS ONE TURN DELIVERED** — passed in rather than captured, so the
        // caller can read it per turn. `confirmed` above is the whole run's "did the back end
        // ever take a turn"; this is "did THIS turn produce anything at all", and the two are
        // different questions. See [`CONTINUATION_REASK_LIMIT`].
        //
        // **AND WHAT THIS ONE TURN SAID**, for the same reason: the caller decides whether a
        // turn's words add to the account or replace it (a command's finish replaces the
        // "it has started" before it — step 3c).
        let mut say = |lease: &mut Box<dyn Cognition>, text: &str, items: &mut usize, said: &mut String| {
            if !self.quota_gate(backend, record, confirmed) {
                return Err(CognitionError::Protocol("The waiting assignment was stopped.".into()));
            }
            // The switch notice only once this work really runs under the account a switch
            // went to (`claude_accounts.rs`, `ran_on`); a switch still waiting on a command
            // this back end started says nothing yet.
            let quota = self.quota.lock().unwrap().clone();
            if let (Some(quota), Some(account)) = (quota, lease.account()) {
                quota.accounts.ran_on(account);
            }
            // **AN OVERLOADED TURN IS SENT AGAIN, ON THIS SAME SESSION, ON HIS SCHEDULE** (CEO,
            // 2026-10-07): 1, 2, 5, 10, 20, 40 and 80 minutes before retries 1 to 7. Classified
            // here, at the turn, beside the usage-limit backstop the settle arms carry, so every
            // turn of a run gets it (the first and each continuation) and nothing of the run is
            // started again. His Stop and a quit end the wait at once; past the 7th retry the
            // turn's own failure goes on to the arms below exactly as before.
            loop {
                let (items_before, said_before) = (*items, said.len());
                // Set by anything the turn did between two pieces of its prose (a tool call
                // above all): the words after it are a new notice ([`add_streamed_words`]).
                let mut between_notices = false;
                let mut published=false;
                let sent = lease.prompt(text, &mut |item: TurnItem| {
                    if !published {published=true;self.publish_peer_input(backend,binding);}
                    *items += 1;
                    // The back end has his answers: its first item of the turn that carried them.
                    if let Some(ids) = to_take.borrow_mut().take() {
                        self.answers_taken(backend, &ids, &session);
                    }
                    if !confirmed {
                        confirmed = true;
                        advance(AssignmentState::Running, started_detail);
                    }
                    match item {
                    TurnItem::Text { text, .. } => {
                        chars.set(chars.get() + text.len());
                        // Bounded while it accumulates, not only at the end: a back end that
                        // streamed a hundred megabytes would otherwise hold all of it before
                        // anything trimmed it. 64 KiB is eight times the answer cap, so no
                        // real answer can reach this line and be cut by it.
                        if keeping_an_answer && said.len() < 64 * 1024 {
                            add_streamed_words(said, text, &mut between_notices);
                        }
                    }
                    TurnItem::Machinery(record) => {
                        // A usage reading or a keep-alive (`Unknown`) can land between two
                        // deltas of one sentence and ends nothing; every other kind ends the
                        // run of prose in front of it, as the ledger's `text_runs` fold does.
                        if record.kind == crate::machinery::MachineryKind::ToolCall
                            && record.parent_tool_use_id.is_none()
                            && record.status == Some(crate::machinery::ToolStatus::Pending)
                        {
                            between_notices = true;
                        }
                        // **The one machinery record that is READ rather than retained**,
                        // same as `spine.rs:2034`. The back end's own machinery is not
                        // otherwise journalled: its turns are never rendered.
                        if let Some(usage) = record.context_usage() {
                            measured.set(Some(usage));
                        }
                    }
                    TurnItem::AnswerComplete => {}
                    }
                });
                // The failed turn's own words, or the words it streamed (the two shapes the
                // conversation's `detect_upstream_failure` reads), and only an overload.
                let overloaded = sent.as_ref().err()
                    .and_then(|why| crate::upstream::UpstreamFailure::classify_lines(&why.to_string()))
                    .or_else(|| said.get(said_before..).and_then(crate::upstream::UpstreamFailure::classify_lines))
                    .filter(|failure| failure.fault == crate::upstream::UpstreamFault::Overloaded);
                let Some(failure) = overloaded else {
                    if sent.is_ok() {
                        // An answered turn: the next overload starts again at the first wait.
                        backend.inner.lock().unwrap().overload.succeeded();
                    }
                    return sent;
                };
                let stop_asked = || {
                    let inner = backend.inner.lock().unwrap();
                    inner.closing || inner.stopped.contains(&record.id)
                };
                let wait = {
                    let mut inner = backend.inner.lock().unwrap();
                    let granted = inner.overload.charge(failure.fault);
                    granted.then(|| inner.overload.wait_before_retry()).flatten()
                };
                let Some(wait) = wait.filter(|_| !stop_asked()) else { return sent };
                let minutes = wait.as_secs() / 60;
                eprintln!("[richos] work: {}; trying the same turn again in {minutes} min on the same session", failure.summary());
                advance(
                    if confirmed { AssignmentState::Running } else { AssignmentState::Preparing },
                    &format!(
                        "Anthropic's servers are at capacity. Trying again in {} on the same connection.",
                        if minutes == 1 { "1 minute".to_string() } else { format!("{minutes} minutes") }
                    ),
                );
                let clock = Arc::clone(&*self.retry_clock.lock().unwrap());
                if !clock.wait(wait, &stop_asked) {
                    return sent;
                }
                // The failed attempt's words are not part of the answer.
                *items = items_before;
                said.truncate(said_before);
            }
        };
        let mut items = 0usize;
        let mut said = String::new();
        // **How many helper runs had ended when the latest turn began** (and that turn's words,
        // `last_said`): a turn that ends declaring it waits on a helper is answered by a helper
        // run that ends AFTER it began ([`declares_waiting_on_a_helper`]). A run that ended
        // while the turn was going counts, because the back end cannot have read its outcome.
        let ends_now = || {
            backend.inner.lock().unwrap().lease_session.clone()
                .and_then(|session| crate::app_workers::run_ends(&self.state, &session))
                .unwrap_or(0)
        };
        let mut ends_at_turn_start = ends_now();
        // The background commands this lease already knew about before this assignment's
        // first turn: none of them is this assignment's (step 3c).
        let mut commands_seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        // P4e's fault: the back end died while the job waited for his answer, so the prompt
        // carrying it finds no child and fails before anything is written (`native.rs`'s
        // `Closed`). Dropping the lease ends its child, as a death would.
        if !carried.new.is_empty() && work_fault("WORK-DEAD-LEASE") {
            eprintln!("crash point WORK-DEAD-LEASE: the back end is gone before his answer is sent");
            *backend.lease.lock().unwrap() = None;
        }
        let mut outcome = {
            let mut lease = backend.lease.lock().unwrap();
            match lease.as_mut() {
                Some(lease) => {
                    let own: Vec<&str> = rewatched.iter().flat_map(|w| w.commands.iter().map(|(id, _)| id.as_str())).collect();
                    commands_seen.extend(lease.background_commands().unwrap_or_default().into_iter()
                        .map(|c| c.task_id).filter(|id| !own.contains(&id.as_str())));
                    say(lease, &prompt, &mut items, &mut said)
                }
                None => Err(CognitionError::Protocol("The work connection closed.".into())),
            }
        };
        // **NOBODY IS SIGNED IN: HELD, NOT FAILED** (T3 idea 1, finished; Sage's check §1.1).
        // Claude Code opens a back end with nobody signed in and refuses its first turn
        // locally, with no model call: a `<synthetic>` "Not logged in · Please run /login" and
        // an error result (measured on 2.1.295). So a fresh job whose first turn ends that way
        // got nothing done, and every job queued behind it would end the same way. They wait
        // together, and the host starts them by itself when the sign-in comes back. Only a
        // fresh job carrying nothing of his: a job he approved, answered or is watching keeps
        // the path it had. His Stop outranks it.
        let signed_out = outcome.as_ref().err().is_some_and(|why| {
            crate::interruption::classify(&why.to_string()) == crate::interruption::InterruptionCause::NotSignedIn
        });
        if signed_out && !reporting && !resumed && carried.is_empty() && rewatched.is_none() && !{
            let inner = backend.inner.lock().unwrap();
            inner.closing || inner.stopped.contains(&record.id)
        } {
            if let Some(lease) = backend.lease.lock().unwrap().as_mut() {
                if let Err(error) = lease.revoke_work_assignment() {
                    eprintln!("[richos] work: the grant of a job held for sign-in could not be revoked: {error}");
                }
            }
            self.forget_at_the_desk(record);
            // Retired as every failed turn's back end is (the `Err` arm below): the next
            // attempt opens a fresh one under whatever sign-in there is then.
            *backend.lease.lock().unwrap() = None;
            {
                let mut inner = backend.inner.lock().unwrap();
                inner.cancel = None;
                inner.lease_session = None;
                inner.context_chars = 0;
                inner.context_usage = None;
            }
            eprintln!("[richos] work: nobody is signed in to Claude; this conversation's queue is held");
            self.hold(backend, binding, record, HoldCause::SignIn);
            return;
        }
        keep_words(&mut answer, &said);
        let mut last_said = said.clone();
        // The back-end turn is over: what it, its workers and its commands wrote so far goes
        // on the thread's output list (Output side panel PRD §4.1 (b), the per-turn reader).
        self.project_output(record, &session);
        // **Carried and not taken**: the prompt returned and no item of its turn ever arrived.
        // It is no longer in flight, so it is no longer `carrying`; what that means is decided
        // before the settle (D4).
        let untaken: Vec<String> = match to_take.borrow_mut().take() {
            Some(_) => {
                backend.inner.lock().unwrap().carrying.clear();
                carried.new.iter().map(|d| d.id.clone()).collect()
            }
            None => Vec::new(),
        };
        let carried_ids: Vec<String> = carried.new.iter().map(|d| d.id.clone()).collect();
        // The words he was told while a command of this assignment was still running, if any
        // (step 3c). The settle arms below do not tell him the same words twice.
        let mut told_while_running: Option<String> = report.as_ref().and_then(|report| report.told.clone())
            .or_else(|| rewatched.as_ref().and_then(|w| w.told.clone()));
        // Set when this job's commands outlive the wait below: it is handed to the watch
        // ([`Watched`]) instead of being settled.
        let mut handed: Option<Vec<String>> = None;

        // ===================================================================================
        // 3b. THE HELPER OUTLIVES THE TURN THAT STARTED IT, SO THE HOST WAITS AND ASKS AGAIN
        // ===================================================================================
        //
        // **This is the whole of candidate .10's "the job does not land", and it is a fact
        // about the platform rather than about the back end.** The prepared payload is
        // `run_in_background: true` — it has to be: `guard-worktree-isolation.sh` clause 7b
        // refuses a file-capable spawn with `run_in_background: false`, and the engine joins
        // the worker's platform id to its app receipt at `PostToolUse[Agent]`, which a
        // synchronous call does not deliver until the worker has already finished (measured:
        // every one of its tool calls refused with *"worker identity has not joined its app
        // receipt"*). So `Agent` answers `{"status": "async_launched"}` at once, and the turn
        // ends seconds later with the worker just getting started.
        //
        // **`DESKTOP.md` step 4 told the back end to wait for it with `TaskOutput`, and
        // `TaskOutput` IS NOT IN THIS LEASE'S TOOL INVENTORY.** Read off the child's own
        // `system/init` frame on 2026-09-18: 30 tools, including `Task`, `TaskStop`,
        // `Monitor`, `ListAgents` and `SendMessage`, and no `TaskOutput` and no `BashOutput`.
        // The one instruction standing between "launched" and "landed" has always named a
        // tool that is not there — which is why it was never called in any run measured, with
        // the lease's tools deferred and again with them resident, and with the wait spelled
        // out in the assignment brief itself.
        //
        // **So the waiting is the HOST's, where it cannot be forgotten, and it is done on a
        // positive signal.** `app_workers::status` counts a run open until a `SubagentStop`
        // for that `agent_id` is in the journal; this loop waits for that row, never for a
        // timer to expire and never for the filesystem to go quiet. When it arrives the lease
        // is given one more turn, on the same seat and the same open grant, saying the helper
        // has ended and to carry on. A CEO Stop and a quit both end the wait at once.
        //
        // Reaching the bound claims nothing either way: the row says the helper is taking
        // longer than usual and the wait goes on (hunt part 1 finding 06). Only when the wait
        // ends for another reason (his Stop, a quit, the lease or the evidence gone) do the
        // readings below report what they can witness.
        //
        // ===================================================================================
        // 3c. SO DOES A COMMAND IT STARTED IN THE BACKGROUND — esc-20260927T093052Z-85f3303f
        // ===================================================================================
        //
        // **"Start it and tell me when it finishes."** The back end runs the command with
        // `run_in_background`, ends its turn with "I've started it", and the command ends
        // later. The provider then reports the ending in a turn of its own, with no prompt of
        // ours parked, so its words go to `native.rs`'s between-turn lane (`route`, the
        // `None` arm), which this host never reads; and this assignment had already settled on
        // "it has started". That was the whole of the finish he never heard (test VM,
        // 2026-09-27, session `52e71a95` rows 15, 19-20, 25-26).
        //
        // So the host waits for it the way it waits for a helper, on a POSITIVE signal: the
        // provider's own `task_notification` for that command
        // ([`crate::cognition::BackgroundCommand`]). Meanwhile he already has the back end's
        // words (one notice, `told_while_running`) and the pane says the command is running.
        // When it ends outside a turn of ours, the back end is asked for its report, and that
        // report is the answer this assignment settles on. An ending the provider folded
        // into the back end's own turn is not asked about again: that turn's words already
        // carry it (`cap-fold.jsonl`).
        //
        // The wait YIELDS to his next job on this thread (`wait_for_background_commands`):
        // a command that never ends — a server he asked to have started — must not hold his
        // back end, and he already has its words.
        let mut waits = 0usize;
        // **RICH'S WAITING SENTENCE, ON HIS ROW WHILE THE TEAMMATE WORKS** (CEO, 2026-10-15,
        // "1 and 3"; Ray's check of nightly 57, `team/hire4-delegate-3-end.png`, where "They're
        // working on it now" stood above the answer it promised). On a job that only consults,
        // the back end says who is working on what in its declared wait ([`waiting_sentence`],
        // `DESKTOP.md`'s consult paragraph); the row shows that sentence while the teammate works,
        // and his answer, which never holds it, replaces it when it arrives. Any other wait
        // shows the row's own sentence, as before. Set while the row shows a waiting sentence.
        let mut shows_waiting = false;
        let mut show_waiting = |waits: usize, said: &str| {
            let waiting = waiting_sentence(said).filter(|_| {
                crate::work_status::trail(&self.state, &record.entity_id, &record.thread_id, &record.obligation_id)
                    .is_ok_and(|trail| trail.consults > 0 && trail.workers == 0)
            });
            if waits == 0 || waiting.is_some() || shows_waiting {
                match waiting.as_deref() {
                    Some(sentence) => {
                        let _shown = assignment::advance_waiting(&self.state, &scope.0, &scope.1, &scope.2, sentence);
                    }
                    None => advance(AssignmentState::Running, "A helper is doing the work."),
                }
                shows_waiting = waiting.is_some();
            }
        };
        // A turn that never took his answer is not waited on: nothing of it ran (D4).
        while outcome.is_ok() && untaken.is_empty() {
            let Some(session) = backend.inner.lock().unwrap().lease_session.clone() else { break };
            let view = crate::app_workers::status(&self.state, Some(&session));
            // Unattributed evidence is NOT a reason to wait: it is the one thing this loop
            // could wait on forever, and `settlement` below already reads it as unsettled and
            // says so. Only a run this host can see open is worth waiting for.
            let helper_open = view.is_attributed() && view.active + view.liveness_unknown > 0;
            let continuation = if helper_open {
                let report = words_for_him(&last_said);
                if !report.is_empty() && told_while_running.as_deref() != Some(report.as_str()) {
                    self.raise(record, NoticeKind::Answer, &report);
                    told_while_running = Some(report);
                }
                show_waiting(waits, &last_said);
                waits += 1;
                let one_end = Some(ends_at_turn_start);
                if !self.wait_for_owned_workers(backend, record, &session, one_end) {
                    break;
                }
                // **WHAT THE NEXT STEP ACTUALLY IS, READ OFF THE RECEIPTS.** A worker that has
                // ended and a reviewer that has passed are two different places to be, and the
                // one sentence that used to serve both sent the back end to prepare a reviewer
                // for work a reviewer had already passed.
                self.continuation_after_a_helper_ended(record, &session)
            } else {
                let mine: Vec<crate::cognition::BackgroundCommand> = self
                    .background_commands(backend)
                    .into_iter()
                    .filter(|c| !commands_seen.contains(&c.task_id))
                    .collect();
                if mine.is_empty() && self.team_changed(backend)
                    && !matches!(self.outcome(backend, record), Outcome::Settled)
                    && self.pending_decision(record).is_none()
                {
                    // **NOTHING IS OPEN, AND ITS LAST TURN CHANGED THE TEAM** (proto-teammate
                    // shelf slice 5, plan §1). The back end saved the teammate Dean fitted and
                    // ended its turn, because it cannot name that teammate until its connection
                    // is renewed. So the host renews it here, between two turns of the one run,
                    // carrying the assignment, and gives the successor the next turn: the
                    // teammate is usable in the same assignment, which is not started again. A
                    // command it started that may still be running defers the renewal, as every
                    // renewal is deferred, and the run then ends as it would have; the next
                    // assignment's start renews it (step 0d). A hiring with a proposed teammate
                    // unsaved goes on on the connection it has.
                    let context = || {
                        format!("{prompt}\n\nWhat you had said on it so far:\n{}", assignment::sanitize_answer(&answer))
                    };
                    let next = if self.renew_team_between_turns(backend, binding, record, &work, context) {
                        TEAM_CHANGED_CONTINUATION.to_string()
                    } else {
                        break;
                    };
                    waits += 1;
                    chars.set(next.len());
                    measured.set(None);
                    next
                } else if mine.is_empty() && !declares_waiting_on_a_helper(&last_said) {
                    break;
                } else if mine.is_empty() {
                    // **NOTHING IS OPEN, AND THE BACK END SAID IT IS WAITING ON A HELPER**
                    // ([`declares_waiting_on_a_helper`]): the job is not over, so it is not
                    // settled. The host waits for the next helper run to end, then hands the
                    // back end its continuation, on the same seat and the same open grant. A
                    // closed obligation or a question of his on the desk is never waited on.
                    if !declares_waiting_on_a_helper(&last_said)
                        || matches!(self.outcome(backend, record), Outcome::Settled)
                        || self.pending_decision(record).is_some()
                    {
                        break;
                    }
                    show_waiting(waits, &last_said);
                    waits += 1;
                    if !self.wait_for_a_helper_run_to_end(backend, record, &session, ends_at_turn_start) {
                        break;
                    }
                    self.continuation_after_a_helper_ended(record, &session)
                } else {
                    waits += 1;
                    let running: Vec<String> =
                        mine.iter().filter(|c| c.ended.is_none()).map(|c| c.task_id.clone()).collect();
                    if !running.is_empty() {
                        advance(AssignmentState::Running, COMMAND_STILL_RUNNING_DETAIL);
                        let words = assignment::sanitize_answer(&answer);
                        // Stu's report reaches Rich through the common message channel.
                        if told_while_running.as_deref() != Some(words.as_str()) && !words.trim().is_empty() {
                            self.raise(record, NoticeKind::Answer, &assignment::says::answered(&record.title, &words));
                            told_while_running = Some(words);
                        }
                        match self.wait_for_background_commands(backend, record, &running) {
                            CommandWait::Ended => {}
                            // Still running, and he has another job or the bound was reached: the
                            // job is watched from here, not settled ([`Watched`]).
                            CommandWait::Handed => {
                                handed = Some(running);
                                break;
                            }
                            CommandWait::Over => break,
                        }
                    }
                    let ended: Vec<crate::cognition::BackgroundCommand> = self
                        .background_commands(backend)
                        .into_iter()
                        .filter(|c| mine.iter().any(|m| m.task_id == c.task_id))
                        .collect();
                    commands_seen.extend(ended.iter().map(|c| c.task_id.clone()));
                    let unreported: Vec<&crate::cognition::BackgroundCommand> = ended
                        .iter()
                        .filter(|c| c.ended.as_ref().is_some_and(|e| !e.during_a_turn_of_ours))
                        .collect();
                    if unreported.is_empty() {
                        continue;
                    }
                    command_ended_continuation(&unreported)
                }
            };
            let mut asked_again = 0usize;
            loop {
                // **FILL-FIRST BEFORE EVERY CONTINUATION TURN**, not only before the run (step
                // 0c): a long run whose account crosses its check point between two of its turns
                // moves to the account in use HERE, with the assignment carried across, and this
                // continuation is sent there. The run goes on; it is not started again. The limit
                // backstop below stays for one turn that by itself uses up what remained.
                let context = || {
                    format!("{prompt}\n\nWhat you had said on it so far:\n{}", assignment::sanitize_answer(&answer))
                };
                // **AND THE TEAM** (proto-teammate shelf slice 5): a teammate saved during the
                // turn that just ended is registered here, on a successor carrying the run.
                // After the account check, because a switch registers the team as it is now.
                // Never while a helper of this back end is still running: its run lives in the
                // connection a renewal would retire (a turn between two helpers' ends).
                if !self.helpers_open(&session)
                    && (self.switch_account_between_turns(backend, binding, record, &work, context)
                        || self.renew_team_between_turns(backend, binding, record, &work, context))
                {
                    chars.set(continuation.len());
                    measured.set(None);
                }
                // A handoff continuation never goes out on the account being left (plan §2.3).
                if continuation.starts_with(HANDOFF_CONTINUATION_HEAD) {
                    match self.wait_for_the_switch(backend, binding, record, &work, context) {
                        Some(true) => {
                            chars.set(continuation.len());
                            measured.set(None);
                        }
                        Some(false) => {}
                        None => {
                            outcome = Err(CognitionError::Protocol("The waiting assignment was stopped.".into()));
                            break;
                        }
                    }
                }
                let mut items = 0usize;
                let mut said = String::new();
                ends_at_turn_start = ends_now();
                outcome = {
                    let mut lease = backend.lease.lock().unwrap();
                    match lease.as_mut() {
                        Some(lease) => say(lease, &continuation, &mut items, &mut said),
                        None => Err(CognitionError::Protocol("The work connection closed.".into())),
                    }
                };
                // The current report replaces earlier working notes.
                last_said = said.clone();
                fold_turn(&mut answer, &said);
                // Each continuation turn is a back-end turn too (§4.1 (b)).
                self.project_output(record, &session);
                        // **A TURN THAT PRODUCED NOTHING IS A TURN THIS HOST DID NOT GET.** Not
                // inferred from elapsed time and not inferred from silence on a timer: the
                // child answered with a terminal `result` and delivered no item at all
                // between the send and it, which no turn that prepares, reviews or lands
                // anything can do. `native.rs` now refuses such a result outright when the
                // child names its turns; this is the same claim made where the child does
                // not. Asked again ONCE — see [`CONTINUATION_REASK_LIMIT`].
                if !(outcome.is_ok() && items == 0 && asked_again < CONTINUATION_REASK_LIMIT) {
                    break;
                }
                asked_again += 1;
                eprintln!(
                    "[richos] work: the continuation turn produced nothing at all, which is                      not a turn this assignment was given. Asking once more."
                );
            }
        }
        {
            let mut inner = backend.inner.lock().unwrap();
            inner.context_chars += chars.get();
            if let Some(usage) = measured.take() {
                inner.context_usage = Some(usage);
            }
        }

        // **3a. THE READINESS FACTS, READ WHERE THEY EXIST.**
        //
        // The engine plugin, the work tools and automatic permission checks are read off the
        // child's `system/init` frame, and that frame arrives with the FIRST TURN
        // (`native.rs`'s `child_args` doc). `bind_work_assignment` therefore cannot ask — and
        // until 2026-09-18 it asked anyway, read "nobody has told us" as "we were told no",
        // and failed every background job this product was ever given before its lease ran a
        // single turn.
        //
        // The bind now refuses only a REPORTED absence, so this is the other half: the same
        // three facts, the same three sentences, asked on the same lease one turn later. It is
        // read here, before the settle reading below, because a run that had no engine plugin
        // must be reported as THAT and not as the vague "stopped before it finished" an open
        // obligation would otherwise produce.
        let readiness = backend
            .lease
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|lease| lease.work_readiness_after_turn().err())
            .map(|why| honest(&why.to_string()));

        // 4. The grant goes away with the assignment, whatever happened (spec §5.4).
        if let Some(lease) = backend.lease.lock().unwrap().as_mut() {
            let _ = lease.revoke_work_assignment();
        }
        // Off the back end, as before: nothing may cancel a turn that has ended. A job handed
        // to the watch below keeps it, for its commands only.
        let stopper = backend.inner.lock().unwrap().cancel.take();

        // 5. What state it is in is read from evidence, never from the turn ending.
        //
        // **A quit that arrived while the turn was in flight counts as a stop here**, and
        // it has to: `shutdown` marks every open assignment `interrupted`, and a runner
        // that then wrote its own verdict over the top would turn a quit into "still
        // running" — the one state a quit must never produce.
        let stopped = {
            let inner = backend.inner.lock().unwrap();
            inner.closing || inner.stopped.iter().any(|id| *id == record.id)
        };
        // ===================================================================================
        // HIS ANSWER WAS NOT TAKEN: A DELIVERY ERROR, NOT THE JOB'S ENDING (design D4)
        // ===================================================================================
        //
        // The prompt carrying his answer came back with no item of its turn: a lease that died
        // while the job waited for him (`Closed` before the write, or a failed write), or the
        // child refusing or discarding the message (`native.rs`'s `command_lifecycle`). Nothing of
        // it ran, so the answer is still pending on disk and the job goes back to waiting on it,
        // and is run again now, on a fresh lease after an error. Bounded: two runs of this launch
        // per answer (the lead path's `RETRY_STARTS`), and past that the arms below fail it once.
        // His Stop and a quit outrank all of it.
        let past_bound = {
            let inner = backend.inner.lock().unwrap();
            untaken.iter().any(|id| inner.answer_starts.get(id).copied().unwrap_or(0) >= crate::operator_host::RETRY_STARTS)
        };
        let lifecycle_stop = outcome.as_ref().is_ok_and(|reason| reason == crate::native::STOP_REASON_CANCELLED);
        if !untaken.is_empty() && !stopped && !lifecycle_stop && !past_bound {
            eprintln!("[richos] work: the back end did not take his answer ({}); trying again",
                      match &outcome { Ok(reason) => reason.clone(), Err(e) => e.to_string() });
            if outcome.is_err() {
                // Retired exactly as the failure arm below does: the next run opens a fresh one.
                *backend.lease.lock().unwrap() = None;
                let mut inner = backend.inner.lock().unwrap();
                inner.lease_session = None;
                inner.context_chars = 0;
                inner.context_usage = None;
            }
            advance(AssignmentState::Blocked, ANSWER_RETRY_DETAIL);
            let latest = assignment::read(&self.state, &record.entity_id, &record.thread_id, &record.id)
                .unwrap_or_else(|_| record.clone());
            if !self.schedule(binding, latest, resumed) {
                eprintln!("[richos] work: RichOS is closing; his answer stays saved for the next launch");
            }
            return;
        }
        // ===================================================================================
        // CUT BY A USAGE LIMIT: STARTED AGAIN UNDER THE NEXT ACCOUNT (plan §15 answer 3)
        // ===================================================================================
        //
        // *"Work cut off by a limit continues on the next account, background work included."*
        // The switch normally happens before the turn (step 0c before the run, and before every
        // continuation turn inside it, `switch_account_between_turns`); this is the one turn
        // that by itself used up what remained. The account is marked gone and, when another has room,
        // the corpse is retired and the assignment is scheduled again: its next lease is spawned
        // under the account now in use. Bounded by construction — every pass marks one more
        // account gone — and with no account left it falls through to the failure arms below,
        // exactly as with one account. His Stop and a quit outrank it.
        if let Err(why) = &outcome {
            let limit = (!stopped).then(|| crate::claude_accounts::parse_usage_limit(&why.to_string())).flatten();
            let account = backend.lease.lock().unwrap().as_ref().and_then(|l| l.account().map(str::to_string));
            let quota = self.quota.lock().unwrap().clone();
            if let (Some(resets_at), Some(account), Some(quota)) = (limit, account, quota) {
                if quota.limit_reached(&account, resets_at) == crate::claude_accounts::AfterLimit::Continue {
                    eprintln!("[richos] work: a usage limit cut this run; starting it again on the next Claude account");
                    *backend.lease.lock().unwrap() = None;
                    {
                        let mut inner = backend.inner.lock().unwrap();
                        inner.lease_session = None;
                        inner.context_chars = 0;
                        inner.context_usage = None;
                    }
                    advance(AssignmentState::Blocked, LIMIT_SWITCH_DETAIL);
                    let latest = assignment::read(&self.state, &record.entity_id, &record.thread_id, &record.id)
                        .unwrap_or_else(|_| record.clone());
                    if !self.schedule(binding, latest, resumed) {
                        eprintln!("[richos] work: RichOS is closing; the assignment continues at the next launch");
                    }
                    return;
                }
            }
        }
        // **ITS COMMAND OUTLIVED THE WAIT: WATCHED, NOT SETTLED** ([`Watched`]). He already has
        // the back end's words (step 3c raised them), the row says the command is running, and
        // the runner asks for this job's report when the provider says it ended. Only on a
        // turn that answered, on a lease that was equipped, and with no Stop or quit: every
        // other case settles below exactly as before.
        if let Some(ids) = handed.filter(|_| outcome.is_ok() && readiness.is_none() && !stopped) {
            let reading = self.background_commands(backend);
            let commands = ids
                .iter()
                .map(|id| {
                    let what = reading.iter().find(|c| c.task_id == *id).map(|c| c.description.clone()).unwrap_or_default();
                    (id.clone(), what)
                })
                .collect();
            advance(AssignmentState::Running, COMMAND_STILL_RUNNING_DETAIL);
            let mut inner = backend.inner.lock().unwrap();
            let session = inner.lease_session.clone();
            inner.watching.push(Watched {
                binding: binding.clone(),
                record: record.clone(),
                commands,
                told: told_while_running,
                session,
                stopper,
            });
            backend.wake.notify_all();
            return;
        }
        // **Which failure sentence he hears is decided by EVIDENCE, not by which arm we are
        // in.** A turn that broke before the child said anything did not start; one that broke
        // after it had been talking stopped before it finished. Both are true statements about
        // different events, and they are not interchangeable.
        //
        // **There are TWO positive signals and the honest test needs both.** A stream item is
        // the earlier one — it is what moves the record to `Running` above. `outcome.is_ok()`
        // is the other: `prompt` answers `Ok` only on the child's own terminal `result` frame,
        // which is proof it took the turn even for a turn that streamed nothing this host
        // retained. Testing the first alone reported three of this module's own scenarios — a
        // job that ran and did not land, a land the reviewer refused, a land with the
        // assignment still open — as jobs that "did not start", which is Ray's row 2 inverted
        // and every bit as false. Absence of a signal is still never read as a signal here.
        // A turn that never took his answer did not start, whatever it returned (C4).
        let took_the_turn = untaken.is_empty() && (confirmed || outcome.is_ok());
        // §58 adds a third shape to this: a QUESTION that did not get answered is neither
        // "stopped before it finished" nor "did not start" — both put his own question where
        // a job's name goes. The dispatch is in `says::failure` so a fourth failure path
        // added later cannot forget the rule.
        let tell = |title: &str, why: &str| assignment::says::failure(record.kind, title, why, took_the_turn);
        match outcome {
            // **His Stop, or a quit, ended the back end, and he is told he stopped it.** On the
            // real work lease a stop kills the child inside `cancel()` (`native.rs`,
            // `settle_workers_on_stop`), so the turn comes back as the reader's `Closed`, never
            // as the `cancelled` result the arm below expects. Written as `failed`, his own Stop
            // read "It stopped before it finished. Claude channel closed (child exited?)." (the
            // reap walk, guest walk-0ddfdbf8ff00, 2026-09-27). The child is gone either way, so
            // the lease is retired exactly as for any other `Err`.
            Err(_) if stopped => {
                advance(AssignmentState::Interrupted, "Stopped. The workspace and the receipts are kept.");
                self.forget_at_the_desk(record);
                self.raise(record, NoticeKind::Interrupted, &assignment::says::interrupted(&record.title));
                *backend.lease.lock().unwrap() = None;
                let mut inner = backend.inner.lock().unwrap();
                inner.lease_session = None;
                inner.context_chars = 0;
                inner.context_usage = None;
            }
            Err(why) => {
                let sentence = honest(&why.to_string());
                advance(AssignmentState::Failed, &sentence);
                self.forget_at_the_desk(record);
                self.raise(record, NoticeKind::Failed, &tell(&record.title, &sentence));
                // **A BACK END THAT FAILED A TURN IS RETIRED, NOT HANDED THE NEXT ONE.**
                //
                // `ensure_lease` short-circuits on `lease.is_some()` — *"this conversation's
                // back end opens once and stands"* — and several of the errors that reach
                // this arm have already killed the child on their way here: the three grant
                // revocations and the worker-settlement check in `native.rs` all do
                // `child.kill()` before returning. So the lease that stays in this slot is a
                // corpse, and the next assignment on this thread talks to it.
                //
                // **That is the second half of Ray's candidate-.10 walk, on his screen.**
                // Attempt 1 ended at the settlement check, which stopped the child. Attempt 2
                // — Rich's own offer, accepted — came back *"The first attempt stopped before
                // finishing did not start"*: `did not start` because `prompt` on the dead
                // child returned before anything streamed, so `took_the_turn` was false. Two
                // different failures, one cause, and the second one told him nothing true
                // about itself. There is only one work-lease evidence session in that state
                // root for two attempts, which is the same fact from the other side.
                //
                // Retiring is cheap and always safe: `ensure_lease` opens a fresh back end on
                // the next assignment (seconds, and `run_one` step 1 says that is where those
                // seconds belong), the register is durable so nothing is lost with the
                // connection, and dropping the lease runs its own shutdown. It is done for
                // EVERY `Err` rather than only the ones known to kill, because "which errors
                // leave the child alive" is a list that goes stale silently, and the cost of
                // being wrong in this direction is one extra spawn.
                *backend.lease.lock().unwrap() = None;
                {
                    let mut inner = backend.inner.lock().unwrap();
                    inner.lease_session = None;
                    inner.context_chars = 0;
                    inner.context_usage = None;
                }
            }
            Ok(reason) if stopped || reason == crate::native::STOP_REASON_CANCELLED => {
                advance(AssignmentState::Interrupted, "Stopped. The workspace and the receipts are kept.");
                self.forget_at_the_desk(record);
                self.raise(record, NoticeKind::Interrupted, &assignment::says::interrupted(&record.title));
            }
            // Past D4's bound, on a turn that ended with nothing run: said as what it is, once,
            // and never as a job that "stopped before it finished" (C4).
            Ok(_) if !untaken.is_empty() => {
                advance(AssignmentState::Failed, ANSWER_NOT_TAKEN);
                self.forget_at_the_desk(record);
                self.raise(record, NoticeKind::Failed, &tell(&record.title, ANSWER_NOT_TAKEN));
            }
            // **Reader 1** (design D3): a question of his still open, or an answer of his that
            // arrived during this run and waits for the next one. Never an answer THIS run
            // carried: that one was taken at the first item, and waiting on it would leave the
            // job blocked on an answer it already has.
            Ok(_) if crate::questions::Store::new(&self.state).list(&record.entity_id,&record.thread_id).is_ok_and(|qs|qs.iter().any(|q|q.asker==record.obligation_id && !q.delivered && q.state!=crate::questions::State::Withdrawn)) || self.answer_waits_for_next_run(backend, record, &session, &carried_ids) => {
                advance(AssignmentState::Blocked, WAITING_FOR_YOUR_ANSWER);
            }
            // ===========================================================================
            // HIS ANSWER — the CEO's ruling §58, 2026-09-18
            // ===========================================================================
            //
            // *"The answer arrives on the timeline as an answer, never as 'done'."*
            //
            // **It is decided on POSITIVE EVIDENCE and on nothing else**, the same standard
            // `Running` is written on above: `outcome` is `Ok` only on the child's own
            // terminal `result` frame, and `answer` is non-empty only because assistant text
            // came off its stream. Silence produces nothing here and falls through.
            //
            // **It sits BEFORE the obligation reading, and that is the substantive decision
            // in this arm.** `outcome()` asks whether the obligation closed, which is how a
            // piece of WORK is known to be finished. A question is not finished by closing a
            // record — it is finished by being answered — and the ordinary back end answering
            // a question has no reason to have closed anything. Reading the obligation first
            // would have sorted nearly every answered question into `NotSettled` and told him
            // his question "stopped before it finished" while its answer sat in this variable.
            //
            // It also sits before the readiness reading, deliberately: that reading asks
            // whether the lease was equipped to do WORK — engine plugin, work tools, land
            // permissions — and none of those is needed to answer a question. Withholding an
            // answer he has actually been given, because the landing tooling was absent,
            // would be refusing him the thing he asked for over a fact about something else.
            // A question with NO answer still falls through to that reading and hears its
            // precise reason.
            Ok(_) if record.kind.is_question() && !answer.trim().is_empty() => {
                let said = assignment::sanitize_answer(&answer);
                // The record's own detail, which is never spoken: `settled` here means
                // answered, and the pane reads the kind to say so in his words.
                advance(AssignmentState::Settled, assignment::ANSWERED_DETAIL);
                self.forget_at_the_desk(record);
                // Never the same words twice: he was told these while a command of this
                // assignment was still running (step 3c), and nothing newer came after.
                if told_while_running.as_deref() != Some(said.as_str()) {
                    self.raise(record, NoticeKind::Answer, &assignment::says::answered(&record.title, &said));
                }
                // The engine's bookkeeping LAST: it is subprocess calls, and his answer must
                // never wait on it (VM runs 1 and 5, 2026-09-27).
                self.close_answered(record, &said);
            }
            // **A LEASE THAT TOLD US, ONE TURN LATE, THAT IT WAS NEVER EQUIPPED.** Step 3a's
            // reading, and it sits AFTER the stop arm on purpose: a stop is his own action and
            // nothing outranks it. Ahead of the settle reading, because that reading would
            // describe this as an obligation that failed to close and say so vaguely, when
            // what is actually known is precisely which piece was missing.
            Ok(_) if readiness.is_some() => {
                let sentence = readiness.expect("checked by the guard");
                advance(AssignmentState::Failed, &sentence);
                self.forget_at_the_desk(record);
                self.raise(record, NoticeKind::Failed, &tell(&record.title, &sentence));
            }
            // ===========================================================================
            // A TASK THE BACK END DID ITSELF — esc-20260927T093052Z-85f3303f
            // ===========================================================================
            //
            // **No helper was ever prepared for it, the back end said what it did, and
            // nothing of his is waiting on the desk for it.** A command he asked to have run,
            // or the back end telling him why it could not begin. The receipts below have
            // nothing to say about such a task — `what_happened` would read none and say *"No
            // work was started, so nothing was landed"* — and the obligation can never close,
            // because the engine's `complete` needs at least one worker. That is the sentence
            // the test VM measured on 2026-09-27, 3.6-4.1 s after the command he asked for had
            // started, with the back end's own report thrown away beside it.
            //
            // **So what he hears is the back end's report, as the report**: the §58 shape, and
            // for the §58 reason — it is Rich telling him what was done or what is needed, and
            // wrapping it in a receipt sentence would narrate over it. The record says
            // `Settled` with the answered detail, exactly as an answered question does, so the
            // pane says it is in his conversation and never "Finished." It is decided on
            // POSITIVE evidence only: the child's own terminal `result` (`Ok`), words off its
            // stream, and receipts that were READ and hold no worker, no review and no land.
            // An unreadable trail, a helper that ran, a silent back end, and a question of his
            // still on the desk all fall through to the readings below, unchanged.
            Ok(_) if !answer.trim().is_empty()
                && self.no_helper_ever_ran(record)
                && self.pending_decision(record).is_none() =>
            {
                let said = assignment::sanitize_answer(&answer);
                advance(AssignmentState::Settled, assignment::ANSWERED_DETAIL);
                self.forget_at_the_desk(record);
                if told_while_running.as_deref() != Some(said.as_str()) {
                    self.raise(record, NoticeKind::Answer, &assignment::says::answered(&record.title, &said));
                }
                // Its obligation too, on these words: `complete` needs a worker and this
                // assignment has none ([`AnsweredClose`]). LAST, after his report is his: the
                // close is subprocess calls, and on VM runs 1 and 5 (2026-09-27) the report
                // waited behind it long enough that the app was quit before it was raised.
                self.close_answered(record, &said);
            }
            // ===========================================================================
            // WHAT HE HEARS IS THE OUTCOME — the CEO's ruling §52, 2026-09-18
            // ===========================================================================
            //
            // *"There's nothing that ever not lands on its own here in the terminal …
            // So, yes, always land on its own."* So there are now three endings rather
            // than the old two, and the split is made HERE because only the desk can say
            // which of the last two this is.
            //
            // **The settle reading itself is unchanged**: it is still the obligation, and
            // nothing else, that says whether the assignment is closed (`outcome`).
            Ok(_) => match self.outcome(backend, record) {
                Outcome::Settled => {
                    // **The outcome is the sentence, and it is read off the receipts**, not
                    // composed from the fact that the obligation closed: what landed, on
                    // which branch, in which repository, and whether a reviewer passed it.
                    let landed = self.what_happened(record);
                    advance(AssignmentState::Settled, &landed);
                    self.forget_at_the_desk(record);
                    self.raise(record, NoticeKind::Settled, &with_report(&assignment::says::settled(&record.title, &landed), &answer));
                }
                // **A REAL QUESTION OF HIS IS WAITING.** The only thing `blocked` means
                // now: the run asked for something the desk would have put in front of him
                // in a visible turn and could not, so it waited (§5.2/§5.5/§5.7). The
                // approval queue and its Approve/Decline controls exist for exactly this
                // and no longer for a land.
                //
                // `forget_at_the_desk` is deliberately NOT called here — the request has
                // to outlive the turn that raised it, which is the whole of §5.7.
                Outcome::NotSettled if self.pending_decision(record).is_some() => {
                    advance(AssignmentState::Blocked, &self.waiting_on(record));
                    self.raise(
                        record,
                        NoticeKind::ReadyToApprove,
                        &assignment::says::ready_to_approve(&record.title),
                    );
                }
                // **NOTHING IS WAITING FOR HIM AND THE JOB DID NOT FINISH: a failed land,
                // reported as one.** This is the arm §52 created. It used to be impossible
                // to reach — an open obligation with no question on the desk meant the call
                // had not got that far — and it is now the ordinary way a job fails: the
                // land lock timed out, the reviewer asked for changes, the engine refused
                // the merge, or the run stopped somewhere short of closing the assignment.
                //
                // It is `Failed` and not `Blocked`, and the reason is that `Blocked` is a
                // thing he can act on with one press. There is nothing here for him to
                // press: the sentence has to say what went wrong. `says::failed` is
                // *"{title} stopped before it finished. {why}"* — which is exactly true of
                // every case above, including the one where the land itself succeeded and
                // the assignment was never closed.
                //
                // It goes through `tell` all the same: reaching this arm means the obligation
                // is open, which a turn the child never answered also produces. In that case
                // "stopped before it finished" would be the false half of Ray's row 2 again,
                // and `confirmed` is the only thing that can tell the two apart.
                Outcome::NotSettled => {
                    let why = self.what_happened(record);
                    advance(AssignmentState::Failed, &why);
                    self.forget_at_the_desk(record);
                    self.raise(record, NoticeKind::Failed, &with_report(&tell(&record.title, &why), &answer));
                }
                // **NOTHING COULD BE WITNESSED FINISHING IT — AND THE TURN HAS ENDED, SO
                // THAT IS A FAILURE AND NEVER "STILL RUNNING".**
                //
                // This arm used to write `Running` with the settle reading's own sentence,
                // and that is the silent hang the first real run of the whole flow produced
                // (`esc-20260918T115854Z-852c5f9c`): the back end's turn ended at t+350.207 s
                // with nothing prepared, the record went to *"The work connection's records
                // could not be read, so this counts as still running"*, and it sat there for
                // the remaining 1,150 s with `notices: []`. **The CEO was told nothing at
                // all**, which is worse for him than the failure card Ray complained about —
                // a card at least ends the wait, and this state has no watcher that could
                // ever end it: `run_one` returns here and nothing re-reads a `Running` row.
                //
                // **`StillRunning` is not a state; it is a MISSING WITNESS.** §0's rule —
                // *"anything we cannot witness counts as still running"* — is a rule about
                // what may be CLAIMED, and it holds while a turn is open. At this line the
                // turn has provably ended: `prompt` returned `Ok`, the grant has been
                // revoked (step 4), and the lease is idle. So "we could not witness it
                // ending" and "it is still going" have come apart, and reporting the second
                // is a claim about a job that is not running.
                //
                // **What he hears is read off the receipts, exactly like the arm above**, and
                // that is deliberate: it is the same `what_happened` funnel, so this cannot
                // invent a claim the `NotSettled` arm would not make. With no receipts at all
                // it says *"No work was started, so nothing was landed."* — which is the run
                // that produced this, said as what it was. With receipts that DO show a land
                // it names the land, because a worker journal we could not read is no
                // evidence at all about what the engine wrote under its land lock.
                //
                // **Why the technical reason goes to the log and not to him.** The four
                // readings that reach here (worker evidence unreadable, workers still open,
                // obligation absent, obligation unreadable) are four different things for an
                // engineer and one thing for him: it did not finish and here is what it did.
                Outcome::StillRunning(detail) => {
                    eprintln!(
                        "[richos] work: this assignment's turn ended and nothing could be \
                         witnessed finishing it: {detail}"
                    );
                    let why = self.what_happened(record);
                    advance(AssignmentState::Failed, &why);
                    self.forget_at_the_desk(record);
                    // **A QUESTION HEARS NOTHING ABOUT LANDS.** `what_happened` reads the
                    // WORK receipts, and for a question there are none — so it would say "No
                    // work was started, so nothing was landed", which is literally true and a
                    // non-sequitur about something he never asked for (§58). `could_not_answer`
                    // has its own sentence for a reason it cannot name, and that is the honest
                    // one here. The row's own `detail` above is unchanged: it is not spoken.
                    let spoken = if record.kind.is_question() { "" } else { why.as_str() };
                    self.raise(record, NoticeKind::Failed, &tell(&record.title, spoken));
                }
            },
        }
        self.let_go_if_ended(record);
    }

    /// **When the job itself has ended, his open questions for it are withdrawn and its saved
    /// answers let go** — the cleanup every ending takes, the early ones included (the work-path
    /// design's C10: an early return used to skip it, leaving an input queued and refused on
    /// every wake). It reads the record, so it acts only on an ending that is on disk.
    fn let_go_if_ended(&self, record: &Assignment) {
        if let Ok(latest) = assignment::read(&self.state, &record.entity_id, &record.thread_id, &record.id) {
            if matches!(latest.state, AssignmentState::Settled | AssignmentState::Failed | AssignmentState::Interrupted) {
                if let Err(error) = crate::questions::Store::new(&self.state).close(&record.entity_id, &record.thread_id,
                    Some(&record.obligation_id), "This assignment has ended", false) {
                    eprintln!("[richos] could not close assignment questions: {error}");
                }
                drop(crate::question_work::discard(&self.state, &record.entity_id, &record.thread_id, &record.obligation_id));
            }
        }
    }

    /// **D4 for a run that never reached its back end** (C10): his answers for this job are
    /// saved and pending, and this launch has carried each of them fewer than
    /// `RETRY_STARTS` times. Counted as a start, the job goes back to waiting on them and is
    /// scheduled again. `false` when there is no such answer, or the bound is reached.
    fn retry_answer_run(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding, record: &Assignment,
                        resumed: bool) -> bool {
        let Ok(pending) = crate::question_work::pending(&self.state) else { return false };
        let ids: Vec<String> = pending.into_iter()
            .filter(|d| d.entity_id == record.entity_id && d.thread_id == record.thread_id && d.asker == record.obligation_id)
            .map(|d| d.id).collect();
        if ids.is_empty() {
            return false;
        }
        {
            let mut inner = backend.inner.lock().unwrap();
            if inner.closing || inner.stopped.contains(&record.id) {
                return false;
            }
            // This run was a start that carried them, even though it never reached a prompt.
            for id in &ids {
                *inner.answer_starts.entry(id.clone()).or_insert(0) += 1;
            }
            if ids.iter().any(|id| inner.answer_starts[id] >= crate::operator_host::RETRY_STARTS) {
                return false;
            }
        }
        if let Err(error) = assignment::advance(&self.state, &record.entity_id, &record.thread_id, &record.id,
                                                AssignmentState::Blocked, ANSWER_RETRY_DETAIL) {
            eprintln!("[richos] work: the job could not be put back to waiting on his answer: {error}");
        }
        let latest = assignment::read(&self.state, &record.entity_id, &record.thread_id, &record.id)
            .unwrap_or_else(|_| record.clone());
        self.schedule(binding, latest, resumed)
    }

    /// **The back end has his answers** (design D1): the first item of the turn that carried
    /// them arrived, which is the same positive evidence `Running` is written on. Written to the
    /// inbox (synced) BEFORE they leave `carrying`, so the question worker never sees them
    /// pending and uncarried in between. A write that fails is logged, and this session is
    /// never sent them again ([`Inner::taken_unsaved`]); a fresh session would be, which is
    /// right there (D5). Called on the prompt's thread with the lease held and `inner` free.
    fn answers_taken(&self, backend: &Arc<Backend>, ids: &[String], session: &str) {
        crate::operator_host::crash_point("WORK-FIRST-ITEM"); // the child started it; not yet written
        let written = crate::question_work::taken(&self.state, ids, session);
        crate::operator_host::crash_point("WORK-TAKEN"); // taken and synced
        let mut inner = backend.inner.lock().unwrap();
        inner.carrying.clear();
        if let Err(error) = written {
            eprintln!("[richos] work: the back end took his answer and that could not be written down ({error}); \
                       this connection will not be sent it again");
            for id in ids {
                inner.taken_unsaved.push((id.clone(), session.to_string()));
            }
        }
    }

    /// **Reader 1's second clause** (design D3): an answer of his for this job that is still
    /// waiting for a run, which is one that arrived while this run was going. The answers this
    /// run carried are not it, and neither is one this session took but could not write down.
    fn answer_waits_for_next_run(&self, backend: &Arc<Backend>, record: &Assignment, session: &str, carried: &[String]) -> bool {
        let unsaved: Vec<String> = backend.inner.lock().unwrap().taken_unsaved.iter()
            .filter(|(_, s)| s == session).map(|(id, _)| id.clone()).collect();
        crate::question_work::pending(&self.state).is_ok_and(|ds| ds.iter().any(|d| {
            d.entity_id == record.entity_id && d.thread_id == record.thread_id && d.asker == record.obligation_id
                && !carried.contains(&d.id) && !unsaved.contains(&d.id)
        }))
    }

    /// **Which continuation this assignment's back end is handed, decided by the receipts.**
    ///
    /// Two sentences exist ([`WORKER_ENDED_CONTINUATION`] and
    /// [`REVIEW_PASSED_CONTINUATION`]) and the choice between them is a reading, never a
    /// count of how many times this loop has been round:
    ///
    /// - a reviewer's OWN receipt recorded `passed` ([`crate::work_status::WorkTrail::reviews_passed`]),
    /// - and the app's records carry no land for this assignment (`lands` is empty),
    /// - and a worker's run has ended with nothing landed (`not_landed`), so there is
    ///   something to land.
    ///
    /// All three, because any two of them are also true of a state that needs a different
    /// next step. An unreadable trail falls back to the worker sentence, which is the one
    /// that is right at the start of the flow and wrong only later — the safe direction for
    /// a reading that failed.
    ///
    /// **Before either: a helper handed off at the weekly switch** (weekly-switch plan §2,
    /// steps 1 and 1a). Both callers come here, so both paths get the same answer.
    fn continuation_after_a_helper_ended(&self, record: &Assignment, session: &str) -> String {
        if let Some(handed) = self.handoff_continuation(record, session) {
            return handed;
        }
        match crate::work_status::trail(
            &self.state,
            &record.entity_id,
            &record.thread_id,
            &record.obligation_id,
        ) {
            // A consult's answer is read, never reviewed: decided by the newest end on the
            // receipts, so a worker ending after a consult still gets the worker sentence.
            Ok(trail) if trail.last_ended_was_consult => CONSULT_ENDED_CONTINUATION.to_string(),
            Ok(trail)
                if trail.reviews_passed > 0 && trail.lands.is_empty() && trail.not_landed > 0 =>
            {
                REVIEW_PASSED_CONTINUATION.to_string()
            }
            Ok(_) => WORKER_ENDED_CONTINUATION.to_string(),
            Err(why) => {
                eprintln!(
                    "[richos] work: this assignment's receipts could not be read, so the back \
                     end was told a helper ended and nothing more: {why}"
                );
                WORKER_ENDED_CONTINUATION.to_string()
            }
        }
    }

    /// **The handoff continuation, when this session has helpers the weekly switch handed off
    /// whose runs have ended** (weekly-switch plan §2). The markers are the gate's
    /// (`quota::gate::handoffs`): this session's, not yet used, and whose helper is no longer
    /// open in the journal. On the path where one run's end is the signal
    /// (`wait_for_a_helper_run_to_end`), a handed-off helper still committing is named at its
    /// own end, not before. Each marker named is used here, at the choice (1a): the chosen
    /// sentence is what goes out, through the quota wait and a turn asked again, and a marker
    /// counts once. `None` when there is none, or the journal cannot say which are open.
    fn handoff_continuation(&self, record: &Assignment, session: &str) -> Option<String> {
        let view = crate::app_workers::status(&self.state, Some(session));
        if !view.is_attributed() {
            return None;
        }
        let open: Vec<&str> = view.items.iter().filter_map(|item| item.agent_id.as_deref()).collect();
        let handed: Vec<_> = crate::quota::gate::handoffs(&self.state).into_iter()
            .filter(|(_, m)| m.session == session && m.continued_at.is_none() && !open.contains(&m.agent.as_str()))
            .collect();
        if handed.is_empty() {
            return None;
        }
        let now = crate::util::now_millis();
        for (path, marker) in &handed {
            if let Err(error) = crate::quota::gate::mark_continued(path, marker, now) {
                eprintln!("[richos] work: the handoff of {} could not be marked continued: {error}", marker.agent);
            }
        }
        let receipts = crate::work_status::trail(&self.state, &record.entity_id, &record.thread_id, &record.obligation_id)
            .map(|trail| trail.helpers).unwrap_or_default();
        let named: Vec<String> = handed.iter().map(|(_, m)| match receipts.iter().find(|(agent, _, _)| *agent == m.agent) {
            Some((_, id, role)) => format!("- the {role} on receipt `{id}`"),
            None => format!("- the helper whose run was `{}` (find its receipt with `inspect`)", m.agent),
        }).collect();
        Some(format!("{HANDOFF_CONTINUATION_HEAD}\n{}\n\n{HANDOFF_CONTINUATION_TAIL}", named.join("\n")))
    }

    /// **The handoff continuation goes out only once this back end is off the account being
    /// left** (weekly-switch plan §2, step 3). The switch is tried first, as before every
    /// continuation; when a command this back end started defers it (`account_switch_due`),
    /// the host waits here and tries again, rather than send the continuation on the account
    /// being left, where the successor would be ordered at once. With no next account nothing
    /// is pending, and the continuation waits in the quota hold instead (§2, step 5).
    /// `Some(true)` when the run moved to a new lease here, `None` when his Stop or a quit
    /// ended the wait.
    fn wait_for_the_switch(
        self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding, record: &Assignment,
        work: &WorkAssignment, context: impl Fn() -> String + Copy,
    ) -> Option<bool> {
        let mut switched = false;
        loop {
            if !self.switch_pending(backend) {
                return Some(switched);
            }
            if self.switch_account_between_turns(backend, binding, record, work, context) {
                switched = true;
                continue;
            }
            let inner = backend.inner.lock().unwrap();
            if inner.closing || inner.stopped.contains(&record.id) {
                return None;
            }
            drop(backend.wake.wait_timeout(inner, WORKER_WAIT_POLL).unwrap());
        }
    }

    /// This back end's lease is on an account being left for its week, and another account is
    /// in use: the switch is due and has not happened yet.
    fn switch_pending(&self, backend: &Arc<Backend>) -> bool {
        let Some(quota) = self.quota.lock().unwrap().clone() else { return false };
        let account = backend.lease.lock().unwrap().as_ref().and_then(|lease| lease.account().map(str::to_string));
        let Some(account) = account else { return false };
        quota.view().leaving.contains(&account) && quota.lease_account().id != account
    }

    /// **Wait for every run this lease has open to be WITNESSED ending.** `true` when they
    /// all did, `false` when the wait ended for any other reason — a CEO Stop, a quit, the
    /// quota hold being stopped, the lease going away, or the evidence becoming unreadable.
    /// Reaching [`WORKER_WAIT_BUDGET`] is not one of them: it changes what the row says and
    /// nothing else.
    ///
    /// A `false` claims nothing at all. The readings after the caller's loop are what report,
    /// and they report what can be witnessed, exactly as they did before this existed.
    ///
    /// **The signal is `SubagentStop` in the app's own callback journal and nothing else.**
    /// Not elapsed time, not the worktree's mtime, not the worker's branch appearing — the
    /// same positive-signal-only rule the continuity design's §5.2 holds the crash watchdog
    /// to. The poll interval is a read of one small file, so [`WORKER_WAIT_POLL`] is chosen
    /// for how soon the back end is handed its next turn rather than for cost.
    ///
    /// `one_end` also returns after one new helper end, so Stu can process that
    /// result while independent helpers remain active.
    fn wait_for_owned_workers(
        self: &Arc<Self>, backend: &Arc<Backend>, record: &Assignment, session: &str, one_end: Option<usize>,
    ) -> bool {
        let budget = *self.worker_wait.lock().unwrap();
        let mut deadline = std::time::Instant::now() + budget;
        let mut told = false;
        loop {
            let before = std::time::Instant::now();
            if !self.quota_gate(backend, record, true) { return false; }
            deadline += before.elapsed();
            {
                let inner = backend.inner.lock().unwrap();
                if inner.closing || inner.stopped.iter().any(|id| *id == record.id) {
                    return false;
                }
            }
            if backend.lease.lock().unwrap().is_none() {
                return false;
            }
            let view = crate::app_workers::status(&self.state, Some(session));
            // A background worker writes while this waits; its files reach the list on this
            // pass rather than at the next turn (Output side panel PRD §4.1 (b)).
            self.project_output(record, session);
            if !view.is_attributed() {
                return false;
            }
            if view.active + view.liveness_unknown == 0 {
                return true;
            }
            if one_end.is_some_and(|seen| crate::app_workers::run_ends(&self.state, session).is_some_and(|ends| ends > seen)) {
                return true;
            }
            // **The bound is where the row says so, never where the wait ends** (hunt part 1
            // finding 06). Ending here used to revoke the grant the helper works under and
            // fail the job on "nothing could be witnessed finishing it", about a helper the
            // journal still showed at work. So the wait goes on, on the same positive signal,
            // and the row stops reading like an ordinary wait: it says this one is taking
            // longer than usual and that his Stop ends it (the check at the top of this loop).
            if !told && std::time::Instant::now() >= deadline {
                told = true;
                eprintln!(
                    "[richos] work: waited {budget:?} for this back end's helpers and \
                     {} are still open; still waiting for them to end, and nothing is being \
                     claimed about them",
                    view.active + view.liveness_unknown
                );
                if let Err(error) = assignment::advance(&self.state, &record.entity_id, &record.thread_id, &record.id,
                                                        AssignmentState::Running, HELPER_TAKING_LONGER_DETAIL) {
                    eprintln!("[richos] work: the row could not say the helper is taking longer: {error}");
                }
            }
            std::thread::sleep(WORKER_WAIT_POLL);
        }
    }

    /// Is a helper run of this session witnessed open (or of unknown liveness)?
    fn helpers_open(&self, session: &str) -> bool {
        let view = crate::app_workers::status(&self.state, Some(session));
        view.is_attributed() && view.active + view.liveness_unknown > 0
    }

    /// **Wait for the helper the back end said it is waiting on to end a run** — `true` when
    /// the journal holds more `SubagentStop` rows than `seen`, `false` when the wait ended for
    /// any other reason: his Stop, a quit, the quota hold being stopped, the lease or the
    /// journal gone, or [`WORKER_WAIT_BUDGET`] passing with no helper run ending.
    ///
    /// **Unlike [`Self::wait_for_owned_workers`], the bound ends this wait**, and that is the
    /// difference between the two facts each one waits on. There, a helper is witnessed open
    /// and only its end is evidence. Here nothing is witnessed open: the one fact is the back
    /// end's declaration, so twenty minutes with no helper run ending anywhere in its session
    /// is a real answer, and the readings after the loop report what can be witnessed, as they
    /// did before. A `false` claims nothing.
    fn wait_for_a_helper_run_to_end(
        self: &Arc<Self>, backend: &Arc<Backend>, record: &Assignment, session: &str, seen: usize,
    ) -> bool {
        let budget = *self.worker_wait.lock().unwrap();
        let mut deadline = std::time::Instant::now() + budget;
        loop {
            let before = std::time::Instant::now();
            if !self.quota_gate(backend, record, true) { return false; }
            deadline += before.elapsed();
            {
                let inner = backend.inner.lock().unwrap();
                if inner.closing || inner.stopped.contains(&record.id) {
                    return false;
                }
            }
            if backend.lease.lock().unwrap().is_none() {
                return false;
            }
            match crate::app_workers::run_ends(&self.state, session) {
                Some(ends) if ends > seen => return true,
                Some(_) => {}
                None => return false,
            }
            if std::time::Instant::now() >= deadline {
                eprintln!(
                    "[richos] work: the back end declared it was waiting on a helper and no helper \
                     run ended within {budget:?}; reading what can be witnessed"
                );
                return false;
            }
            std::thread::sleep(WORKER_WAIT_POLL);
        }
    }

    /// This back end's background commands, as its lease reports them. Empty when there is no
    /// lease or the lease cannot say — neither of which is a reason to wait.
    fn background_commands(&self, backend: &Arc<Backend>) -> Vec<crate::cognition::BackgroundCommand> {
        backend.lease.lock().unwrap().as_ref().and_then(|lease| lease.background_commands()).unwrap_or_default()
    }

    /// **Wait for each of `ids` to be WITNESSED ending** — the provider's own
    /// `task_notification` for it, read by the lease. [`CommandWait::Ended`] when all of
    /// them did. [`CommandWait::Handed`] when they still run and the wait gives way:
    ///
    /// - **his next job on this thread** — the back end is his, and a command that never ends
    ///   (a server) must not keep it from him; he already has the words it said;
    /// - the bound (60 min in production).
    ///
    /// Either way the job is watched from then on ([`Watched`]) and its finish still reaches
    /// him. [`CommandWait::Over`], which claims nothing, for everything else: his Stop, a
    /// quit, the quota hold being stopped, the lease going away, or a command the lease no
    /// longer reports at all, which cannot be witnessed ending.
    fn wait_for_background_commands(self: &Arc<Self>, backend: &Arc<Backend>, record: &Assignment, ids: &[String]) -> CommandWait {
        let budget = *self.command_wait.lock().unwrap();
        let mut deadline = std::time::Instant::now() + budget;
        loop {
            let before = std::time::Instant::now();
            if !self.quota_gate(backend, record, true) {
                return CommandWait::Over;
            }
            deadline += before.elapsed();
            {
                let inner = backend.inner.lock().unwrap();
                if inner.closing || inner.stopped.contains(&record.id) {
                    return CommandWait::Over;
                }
                if !inner.queue.is_empty() {
                    eprintln!(
                        "[richos] work: another assignment is waiting on this thread, so {} \
                         background command(s) of this one are watched between assignments \
                         from here",
                        ids.len()
                    );
                    return CommandWait::Handed;
                }
            }
            let reading = match backend.lease.lock().unwrap().as_ref() {
                Some(lease) => lease.background_commands(),
                None => return CommandWait::Over,
            };
            let Some(reading) = reading else { return CommandWait::Over };
            let mut all_ended = true;
            for id in ids {
                match reading.iter().find(|command| command.task_id == *id) {
                    Some(command) if command.ended.is_some() => {}
                    Some(_) => all_ended = false,
                    None => return CommandWait::Over,
                }
            }
            if all_ended {
                return CommandWait::Ended;
            }
            if std::time::Instant::now() >= deadline {
                eprintln!(
                    "[richos] work: waited {budget:?} for this assignment's background \
                     command(s) and they are still running; they are watched between \
                     assignments from here"
                );
                return CommandWait::Handed;
            }
            let inner = backend.inner.lock().unwrap();
            // Woken early by a new job on this thread (`schedule` notifies), which the next
            // pass reads as the reason to stop waiting.
            drop(backend.wake.wait_timeout(inner, COMMAND_WAIT_POLL).unwrap());
        }
    }

    /// **Which watched job, if any, the runner should act on now** ([`Watched`]): `Some((id,
    /// Some(ended)))` when every command it waits on has ended; `Some((id, None))` when they
    /// can no longer be witnessed — the lease it ran on is gone or was replaced, the lease
    /// cannot say, or it no longer reports one of them; `None` while they all still run.
    ///
    /// Positive signals only: an ending is the provider's own `task_notification`, and "can no
    /// longer be witnessed" is a different session or an absent reading, never a quiet one.
    fn watched_verdict(
        &self, backend: &Arc<Backend>, watching: &[Watched], session: Option<&str>,
    ) -> Option<(String, Option<Vec<String>>)> {
        let reading = backend.lease.lock().unwrap().as_ref().and_then(|lease| lease.background_commands());
        for watched in watching {
            let lost = || Some((watched.record.id.clone(), None));
            if watched.session.as_deref() != session || session.is_none() {
                return lost();
            }
            let Some(reading) = reading.as_ref() else { return lost() };
            let mut ended = Vec::new();
            for (id, _) in &watched.commands {
                match reading.iter().find(|command| command.task_id == *id) {
                    None => return lost(),
                    Some(command) if command.ended.is_some() => ended.push(id.clone()),
                    Some(_) => {}
                }
            }
            if ended.len() == watched.commands.len() {
                return Some((watched.record.id.clone(), Some(ended)));
            }
        }
        None
    }

    /// **A watched job whose commands can no longer be witnessed** ([`Self::watched_verdict`]):
    /// the connection it ran on closed, so how its command ended cannot be read. He is told
    /// that, never nothing — he had been told the command was running.
    fn watch_lost(self: &Arc<Self>, watched: &Watched) {
        self.report_lost(&watched.record);
    }

    fn report_lost(self: &Arc<Self>, record: &Assignment) {
        const WHY: &str = "The work connection it was running on closed before the command it started \
                           had finished, so how that command ended could not be read.";
        if let Err(error) =
            assignment::advance(&self.state, &record.entity_id, &record.thread_id, &record.id, AssignmentState::Failed, WHY)
        {
            eprintln!("[richos] work: a watched assignment's ending could not be recorded: {error}");
        }
        self.forget_at_the_desk(record);
        self.raise(record, NoticeKind::Failed, &assignment::says::failure(record.kind, &record.title, WHY, true));
    }

    /// Close `record`'s obligation in the engine on the words he was given ([`AnsweredClose`]).
    /// A refusal is logged and never spoken, and nothing is retried: the obligation is then
    /// open, which is what it was before this existed, and what he heard is unchanged.
    fn close_answered(&self, record: &Assignment, said: &str) {
        let close = self.answered_close.lock().unwrap().clone();
        let Some(close) = close else { return };
        if let Err(why) = close.close_answered(record, said) {
            eprintln!(
                "[richos] work: this assignment was answered and its engine record could not be \
                 closed on that answer, so it stays open there: {why}"
            );
        }
    }

    fn settle_stopped(self: &Arc<Self>, backend: &Arc<Backend>, record: &Assignment) {
        let _ = assignment::advance(
            &self.state,
            &record.entity_id,
            &record.thread_id,
            &record.id,
            AssignmentState::Interrupted,
            "Stopped before it started. Nothing was prepared.",
        );
        self.raise(record, NoticeKind::Interrupted, &assignment::says::interrupted(&record.title));
        let mut inner = backend.inner.lock().unwrap();
        inner.completed += 1;
        backend.wake.notify_all();
    }

    /// **The desk's hold on this assignment ends when the assignment does** (spec §5.4).
    ///
    /// Anything of his that was still waiting on it is dropped, and so is any standing
    /// decision he gave for it. A question left on his screen pointing at work that has
    /// stopped is worse than no question, and a standing approval that outlived its
    /// assignment would be an approval attached to nothing.
    fn forget_at_the_desk(&self, record: &Assignment) {
        self.desk.forget(&record.entity_id, &record.thread_id, &record.obligation_id);
    }

    /// What a blocked assignment is waiting on, in his words. Reads the desk, never guesses.
    ///
    /// **The `None` arm's old sentence is gone** (CEO ruling §52). It read *"The work has run
    /// and stopped at the step that would change your repository"*, which was the truth when
    /// a land was the only thing that could stop a run — and would be a false claim now,
    /// because a land no longer stops anything. With nothing on the desk the honest sentence
    /// says only that a step of his is outstanding, and the caller no longer reaches this arm
    /// at all: `settle` tests the desk before choosing `Blocked`.
    fn waiting_on(&self, record: &Assignment) -> String {
        match self.pending_decision(record) {
            Some(request) => format!(
                "The work has run and stopped at a step that is yours to decide: {}.",
                plain_action(&request.tool)
            ),
            None => "The work has run and stopped at a step that is yours to decide.".into(),
        }
    }

    /// **WHAT ACTUALLY HAPPENED TO THIS ASSIGNMENT'S WORK, in his words, read off the
    /// receipts** — the CEO's ruling §52, 2026-09-18: a job that lands on its own has to be
    /// able to say what it landed, and he hears the OUTCOME rather than a state word.
    ///
    /// Every clause here comes from [`crate::work_status::trail`], which reads the land
    /// record the engine wrote onto the work receipt under the repository's land lock. Nothing
    /// is inferred: a land is a land because `integration.verified` is set, a review passed
    /// because the REVIEWER's own receipt says so, and an unreadable record says it is
    /// unreadable rather than saying nothing landed.
    ///
    /// **A land whose cleanup did not finish is still reported as a land**, with the leftover
    /// beside it. Reporting it as a failure would tell him his branch had not moved when it
    /// had, which is the same class of wrong answer as a false "done" and is worse to act on.
    fn what_happened(&self, record: &Assignment) -> String {
        let trail = match crate::work_status::trail(
            &self.state,
            &record.entity_id,
            &record.thread_id,
            &record.obligation_id,
        ) {
            Ok(trail) => trail,
            // "I could not look" is never "nothing happened". Spec §6.2's rule, at the one
            // place in this file where a read failure could have become a claim.
            Err(_) => {
                return "I could not read the work records, so I can't tell you what was landed.".into()
            }
        };
        let mut parts: Vec<String> = Vec::new();
        if trail.lands.is_empty() {
            parts.push(
                if trail.changes_requested > 0 {
                    "The review asked for changes, so nothing was landed."
                } else if trail.not_landed > 0 {
                    "The work ran and nothing was landed."
                } else if trail.workers == 0 {
                    "No work was started, so nothing was landed."
                } else {
                    "Nothing was landed."
                }
                .to_string(),
            );
        } else {
            let where_it_went: Vec<String> = trail
                .lands
                .iter()
                .map(|land| format!("{} in {}", land.branch, land.repository_name()))
                .collect();
            parts.push(format!("It landed on {}.", and_list(&where_it_went)));
            let reviewed = trail.lands.iter().filter(|land| land.reviewed).count();
            // **The verdict is a reading with three answers, not a yes/no.** "All of it was
            // reviewed" and "I have no review on record" are different things to be told, and
            // collapsing the middle case into either would be a claim.
            parts.push(
                if reviewed == trail.lands.len() {
                    "An independent review passed it first.".to_string()
                } else if reviewed == 0 {
                    "I have no passing review on record for it.".to_string()
                } else {
                    format!("An independent review passed {reviewed} of the {} pieces.", trail.lands.len())
                },
            );
            if trail.cleanup_pending {
                parts.push("One of its workspaces was left behind and still needs clearing up.".to_string());
            }
            if trail.not_landed > 0 {
                parts.push(format!(
                    "{} more {} and did not land.",
                    trail.not_landed,
                    if trail.not_landed == 1 { "piece of it ran" } else { "pieces of it ran" }
                ));
            }
        }
        parts.join(" ")
    }

    /// **Were the receipts READ, and do they hold no helper at all for this assignment?** No
    /// worker, no reviewer verdict, no land. `false` when the trail cannot be read: "I could
    /// not look" is never "nothing ran" (spec §6.2, the same rule `what_happened` keeps).
    fn no_helper_ever_ran(&self, record: &Assignment) -> bool {
        matches!(
            crate::work_status::trail(&self.state, &record.entity_id, &record.thread_id, &record.obligation_id),
            Ok(trail) if trail.workers == 0
                && trail.lands.is_empty()
                && trail.changes_requested == 0
                && trail.reviews_passed == 0
        )
    }

    /// The same scoped company facts are available for every assignment.
    fn company_notes(&self, record: &Assignment) -> Option<String> {
        let root = self.central_root.lock().unwrap().clone()?;
        let entity = crate::entity::EntityId::parse(&record.entity_id).ok()?;
        crate::company::CompanyLayer::read(&root, &entity).render_for_priming(&entity)
    }

    /// The request this assignment is waiting on him for, if one is on the desk — the head
    /// of that assignment's own queue (spec §5.5).
    pub fn pending_decision(&self, record: &Assignment) -> Option<crate::permissions::PermissionRequest> {
        self.desk.background_queue().into_iter().find(|request| {
            crate::permissions::assignment_key(&request.binding)
                == (record.entity_id.clone(), record.thread_id.clone(), record.obligation_id.clone())
        })
    }

    /// **The work path's question worker, one pass** — what the app's `richos-question-work`
    /// thread does on every question-store wake and at launch (the work-path design D8: moved
    /// here from `src-tauri/src/question_host.rs` so the unit tests and the VM crash matrix run
    /// reader 2 as it ships, not a copy of it).
    ///
    /// 1. Every saved answer still waiting in the inbox for a work job goes to
    ///    [`Self::queue_question_answer`] again: after a relaunch nothing else schedules it.
    /// 2. Every answered set not yet delivered is delivered: to his team's `lead` sink for an
    ///    `operator:` asker, to this host's inbox otherwise. The front desk's own answers are
    ///    the conversation's and are left to its worker.
    ///
    /// `binding_for` is the ledger's thread binding, which only the shell's ledger can give.
    pub fn deliver_team_answers(
        self: &Arc<Self>,
        binding_for: &dyn Fn(&str) -> Option<ThreadBinding>,
        lead: Option<LeadSink<'_>>,
    ) {
        for delivery in crate::question_work::pending(&self.state).unwrap_or_default() {
            if delivery.asker.starts_with("operator:") {
                continue;
            }
            if let Some(binding) = binding_for(&delivery.thread_id) {
                drop(self.queue_question_answer(&binding, &delivery));
            }
        }
        let store = crate::questions::Store::new(&self.state);
        for (entity, thread, asker) in store.pending_threads().unwrap_or_default() {
            if asker == "front_desk" {
                continue;
            }
            let Some(binding) = binding_for(&thread) else { continue };
            if let Err(error) = store.deliver(&entity, &thread, &asker, |d| {
                if d.asker.starts_with("operator:") {
                    lead.ok_or("Your team is unavailable")?(d)
                } else {
                    self.queue_question_answer(&binding, d)
                }
            }) {
                eprintln!("[richos] team answer remains saved: {error}");
            }
        }
    }

    fn question_answer_instruction(&self, delivery:&crate::questions::Delivery)->Result<Option<crate::ecs::UserInstruction>,String> {
        let Some(owner)=delivery.obligation_id.as_deref() else {return Ok(None)};
        let record=assignment::read_all(&self.state,&delivery.entity_id,&delivery.thread_id).map_err(|e|e.to_string())?
            .into_iter().find(|record|record.obligation_id==owner).ok_or("The asking assignment is unavailable")?;
        if record.user_stop_requested_at_ms.is_some() || (!record.state.is_open() && record.state != AssignmentState::Unknown) {
            crate::question_work::discard(&self.state,&delivery.entity_id,&delivery.thread_id,owner)?;
            return Err("The asking assignment has stopped".into());
        }
        let text=instruction_for(&self.state,&record).ok().filter(|text|text.len()<=crate::ecs::INSTRUCTION_TEXT_LIMIT);
        Ok(Some(crate::ecs::UserInstruction {ledger_ref:record.instruction_ledger_ref,sha256:record.instruction_sha256,text}))
    }

    /// Both surfaces enter the same durable receiving inbox, without a permission hold.
    pub fn queue_question_answer(
        self: &Arc<Self>,
        binding: &ThreadBinding,
        delivery: &crate::questions::Delivery,
    ) -> Result<String, String> {
        if binding.entity_id().as_str() != delivery.entity_id
            || binding.thread_id() != delivery.thread_id
        {
            return Err("Question scope mismatch".into());
        }
        if delivery.asker=="stu" {
            self.question_answer_instruction(delivery)?;
            crate::question_work::enqueue(&self.state,delivery)?;
            self.wake_messages(binding);
            return Ok(format!("work-input:{}",delivery.id));
        }
        let record = assignment::read_all(&self.state, &delivery.entity_id, &delivery.thread_id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|r| r.obligation_id == delivery.asker)
            .ok_or("The asking assignment is unavailable")?;
        if !record.state.is_open() && record.state != AssignmentState::Unknown {
            return Err("The asking assignment has stopped".into());
        }
        crate::question_work::enqueue(&self.state, delivery)?;
        // **A job RichOS closed on while it ran is not re-run by his answer** (the work-path
        // design's C6, option B; spec §6.3). Its answer is saved here, and it goes to that job
        // when he says to pick it back up (`assignment::pick_up`): the run that follows carries
        // every answer he gave it. The notice already told him nothing is running.
        if record.state == AssignmentState::Unknown {
            return Ok(format!("work-input:{}", delivery.id));
        }
        // A restart may land between durable enqueue and waking the backend. An
        // existing unconsumed input still needs a scheduled turn on this process.
        if crate::question_work::pending(&self.state)?
            .iter()
            .any(|d| d.id == delivery.id)
        {
            let backend = self
                .backend_for(&delivery.thread_id)
                .ok_or("RichOS is closing")?;
            let mut inner = backend.inner.lock().unwrap();
            if inner.closing || inner.stopped.contains(&record.id) {
                return Err("The asking assignment has stopped".into());
            }
            inner.binding = Some(binding.clone());
            // **Reader 2** (design D2): the live run already carries this answer and has not yet
            // heard back that the back end took it. Queuing the job again here would send its
            // whole brief a second time to the same back end, and end its command wait early.
            // An answer that arrived during the run and is NOT carried still queues it: that is
            // the next run, and the yield is intended.
            let carried_live = inner.live.as_ref().is_some_and(|live| live.id == record.id)
                && inner.carrying.contains(&delivery.id);
            let taken_here = inner.lease_session.as_ref()
                .is_some_and(|now| inner.taken_unsaved.iter().any(|(id, s)| *id == delivery.id && s == now));
            if !carried_live && !taken_here && !inner.queue.iter().any(|s| s.record.id == record.id) {
                inner.queue.push_back(Scheduled {
                    binding: binding.clone(),
                    record,
                    resumed: false,
                });
                backend.wake.notify_all();
            }
        }
        Ok(format!("work-input:{}", delivery.id))
    }

    /// **His answer, applied to the assignment it belongs to** — spec §5.7's *"when he
    /// answers, the answer applies to the assignment, not to a call that has long since
    /// returned"*.
    ///
    /// Reached only from [`crate::permissions::Answered::ToAssignment`], which the desk
    /// returns only when the provider call had already ended at its deadline. An answer that
    /// still has a call waiting for it never comes here — that call takes it directly.
    pub fn apply_decision(self: &Arc<Self>, binding: &Binding, allow: bool) -> Result<(), String> {
        let key = crate::permissions::assignment_key(binding);
        let record = assignment::read_all(&self.state, &key.0, &key.1)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|row| row.obligation_id == key.2)
            .ok_or_else(|| "That assignment is no longer on this conversation.".to_string())?;
        if allow {
            self.resume(&record)
        } else {
            self.record_declined(&record)
        }
    }

    /// Put an approved assignment back on the lease.
    ///
    /// **This is not a retry and §6.3 is the reason the distinction matters.** Nothing here
    /// restarts work by itself: this path exists only because HE pressed approve, which is
    /// precisely the *"resuming is a decision, and the decision is his"* case the spec keeps
    /// open while refusing every automatic one.
    fn resume(self: &Arc<Self>, record: &Assignment) -> Result<(), String> {
        if !record.state.is_open() {
            return Err("That assignment has already stopped.".into());
        }
        // The thread binding is the ledger's, kept from the registration rather than rebuilt
        // here: only the ledger can say which company a thread belongs to (`entity.rs`).
        let binding = self
            .backend(&record.thread_id)
            .and_then(|backend| backend.inner.lock().unwrap().binding.clone())
            .ok_or_else(|| "This conversation has not registered any work in this session.".to_string())?;
        assignment::advance(
            &self.state,
            &record.entity_id,
            &record.thread_id,
            &record.id,
            AssignmentState::Running,
            "You approved the step it stopped at. Carrying it out now.",
        )
        .map_err(|e| e.to_string())?;
        if !self.schedule(&binding, record.clone(), true) {
            return Err("RichOS is closing down. Nothing was started.".into());
        }
        Ok(())
    }

    /// He declined the step. The work stops where it is, nothing is thrown away, and the
    /// receipt says what happened — never `settled`, and never the `interrupted` sentence
    /// that belongs to a stop he made for a different reason.
    fn record_declined(self: &Arc<Self>, record: &Assignment) -> Result<(), String> {
        assignment::advance(
            &self.state,
            &record.entity_id,
            &record.thread_id,
            &record.id,
            AssignmentState::Interrupted,
            "You declined the last step, so nothing in your repository was changed. \
             Everything the work produced is kept.",
        )
        .map_err(|e| e.to_string())?;
        self.forget_at_the_desk(record);
        self.raise(record, NoticeKind::Interrupted, &assignment::says::declined(&record.title));
        Ok(())
    }

    /// **Has this back end crossed its watermark?** Measured first, estimated only as a
    /// fallback — the same rule and the same two branches as the conversation's
    /// (`spine.rs:1288-1298`), so the two leases cannot come to disagree about what "full"
    /// means.
    fn watermark_reached(&self, inner: &Inner) -> bool {
        let (window, ratio) = *self.budget.lock().unwrap();
        match inner.context_usage {
            // The adapter's own arithmetic, whenever it has spoken.
            Some(usage) if usage.size > 0 => usage.fraction() >= ratio,
            // Nothing measured yet: the chars÷4 proxy over the configured window. An
            // undercount, which delays rotation rather than causing a spurious one.
            _ => {
                let threshold = (window as f64 * ratio) as usize;
                inner.context_chars / crate::spine::CHARS_PER_TOKEN_ESTIMATE >= threshold
            }
        }
    }

    /// **ROTATION, AT AN ASSIGNMENT BOUNDARY AND NOWHERE ELSE** — Sage's finding 11, and
    /// the shape the conversation spine already proved (`spine.rs:2953`, `rotate_lease`).
    ///
    /// *"A standing back-end Rich per thread is a provider session that accumulates context
    /// for as long as the thread lives, which is forever … So the back end fills its
    /// context window and there is no seam that notices or acts."* This is that seam.
    ///
    /// **Four properties, each of which is a test in this file.**
    ///
    /// 1. **Never inside a work turn.** It is called from the runner between two
    ///    assignments, with `live` already `None`. Continuity §3.1 forbids mid-turn
    ///    rotation, and the boundary makes it free: the host already serializes one
    ///    assignment at a time onto this lease, so there is a safe moment between every
    ///    pair and no mid-turn rotation is ever needed.
    /// 2. **The successor is opened BEFORE the incumbent is dropped.** Same ordering and
    ///    same reason as the spine's step 4: a spawn failure must leave the work on a
    ///    still-working lease rather than strand the conversation lease-less.
    /// 3. **Nothing of an assignment's is lost across it**, because nothing of an
    ///    assignment's is held by the lease between assignments. The seat and the standing
    ///    grant are per assignment and are bound and revoked inside `run_one`; an
    ///    assignment that is still OPEN when a rotation happens — one blocked on his
    ///    approval — re-binds both on the successor when he approves, through the same
    ///    `bind_work_assignment` its first run used.
    /// 4. **The payload is COMPACTION, not a transcript.** The successor is primed from
    ///    the assignment register — the durable record — and from one short self-authored
    ///    handoff the outgoing back end is asked for. The register is the crash-safe floor:
    ///    it is on disk and it is the same thing recovery reads, so a rotation whose
    ///    handoff ask fails still produces a successor that knows what it is carrying.
    fn rotate_if_needed(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding) {
        {
            let inner = backend.inner.lock().unwrap();
            if inner.closing || inner.live.is_some() || !self.watermark_reached(&inner) {
                return;
            }
        }
        // The reason is recorded before the attempt, so a FAILED rotation is still legible
        // — the spine's rule that a failed rotation is durable rather than invisible.
        let reason = {
            let inner = backend.inner.lock().unwrap();
            match inner.context_usage {
                Some(_) => "context-watermark-measured",
                None => "context-watermark-estimated",
            }
        };
        // **A COMMAND IT STARTED WOULD END WITH IT** (reap gap C6). Deferred while its
        // supervisor's state file shows one, or cannot be read (never read as a zero), up to
        // the bound; the next boundary asks again.
        let commands = backend.lease.lock().unwrap().as_ref().and_then(|lease| lease.running_commands());
        {
            use crate::lease_commands::CommandReading;
            let mut inner = backend.inner.lock().unwrap();
            if matches!(commands, Some(CommandReading::Running(_) | CommandReading::Unreadable)) {
                let wait = *self.rotation_command_wait.lock().unwrap();
                let since = *inner.rotation_deferred_since.get_or_insert_with(std::time::Instant::now);
                let what = match commands {
                    Some(CommandReading::Running(n)) => format!("{n} command(s) it started are still running"),
                    _ => "it could not be read whether a command it started is still running".to_string(),
                };
                if since.elapsed() < wait {
                    eprintln!("[richos] back end: renewal deferred: {what}; it renews when they end, or after {} s", wait.as_secs());
                    return;
                }
                eprintln!("[richos] back end: renewing after waiting {} s although {what}", since.elapsed().as_secs());
            }
            inner.rotation_deferred_since = None;
        }
        if let Err(why) = self.rotate(backend, binding, reason) {
            // Never fatal, and never silent. The incumbent is still in the chair and still
            // works; the next boundary tries again.
            eprintln!("[richos] back end: this conversation's work connection was not renewed ({why})");
        }
    }

    fn rotate(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding, reason: &str) -> Result<(), String> {
        self.rotate_carrying(backend, binding, reason, None)
    }

    /// [`Self::rotate`], and with `carrying` the one difference a rotation INSIDE a run needs
    /// (fill-first between continuation turns, [`Self::switch_account_between_turns`]): the
    /// assignment in progress goes across with its context. The successor is told it is taking
    /// the assignment over part-way (`carrying.1`: its brief and what it has said so far, on
    /// top of the register and the outgoing back end's own handoff), and is bound to the same
    /// seat and grant BEFORE the swap, so a successor that cannot take the assignment is
    /// dropped and the incumbent keeps it. After the swap his Stop reaches the successor (the
    /// cancel handle moves with it), and the incumbent's grant is revoked as it retires.
    fn rotate_carrying(
        self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding, reason: &str,
        carrying: Option<(&WorkAssignment, &str)>,
    ) -> Result<(), String> {
        // Step 1 — the handoff, asked of the OUTGOING back end. One cheap internal turn,
        // never rendered, and BEST EFFORT: a failure here is not fatal, because the
        // register below is the floor. `prompt_context_only` is the no-tools path
        // (`cognition.rs`), so this ask cannot take an action; its standing grant is closed
        // at this point anyway, since grants live and die with an assignment (§5.4).
        let ask = match carrying {
            Some(_) => format!("{HANDOFF_ASK} {HANDOFF_ASK_MID_RUN}"),
            None => HANDOFF_ASK.to_string(),
        };
        let mut handoff = String::new();
        {
            let mut lease = backend.lease.lock().unwrap();
            if let Some(lease) = lease.as_mut() {
                let said = lease.prompt_context_only(&ask, &mut |item: TurnItem| {
                    if let TurnItem::Text { text, .. } = item {
                        // Bounded on the way in: a back end that answered with an essay
                        // must not turn the successor's priming into one.
                        if handoff.len() < HANDOFF_BUDGET_CHARS {
                            handoff.push_str(text);
                        }
                    }
                });
                if said.is_err() {
                    // Best effort, and the register below is the floor.
                    handoff.clear();
                }
            }
        }
        handoff.truncate(handoff.char_indices().nth(HANDOFF_BUDGET_CHARS).map(|(i, _)| i).unwrap_or(handoff.len()));
        // Step 2 — the successor, opened first. Its team is read just before it is spawned
        // (see `Inner::lease_team`).
        let (team, fresh) = {
            let factory = self.factory.lock().unwrap();
            (factory.team_digest(), factory.spawn_work(binding))
        };
        let mut fresh = fresh.map_err(|e| e.to_string())?;
        // Step 3 — the payload: the durable register, compacted, plus the handoff if the
        // outgoing back end managed one.
        let payload = self.handover_payload_with(binding, &handoff, carrying.map(|(_, context)| context), reason);
        if let Err(why) = fresh.reprime(&payload, &mut |_item: TurnItem| {}) {
            // The successor is dropped unused; the incumbent keeps working. A back end
            // that could not be primed must never take an assignment, because an unprimed
            // one would carry on without knowing what it is carrying.
            return Err(why.to_string());
        }
        // Mid-run: the successor takes the assignment's seat and grant before it takes the
        // chair. A refusal drops it unused, and the incumbent, still bound, keeps the run.
        if let Some((work, _)) = carrying {
            fresh.bind_work_assignment(work).map_err(|why| why.to_string())?;
        }
        let cancel = carrying.and_then(|_| fresh.cancel_handle());
        // Step 4 — the swap, and the accounting reset. The usage belongs to the session
        // that consumed it; a successor that inherited the count would rotate itself on its
        // first assignment (`spine.rs:249-256` makes the same point about a parked desk).
        let session = fresh.session_id().to_string();
        // Read before either lock is taken: it is a file read.
        let repositories = self.connected_repositories(binding);
        let mut lease = backend.lease.lock().unwrap();
        let retired = lease.replace(fresh);
        let mut inner = backend.inner.lock().unwrap();
        inner.lease_session = Some(session);
        inner.context_chars = 0;
        inner.context_usage = None;
        inner.rotations += 1;
        inner.last_rotation_reason = Some(reason.to_string());
        inner.lease_repositories = repositories;
        inner.lease_team = team;
        if carrying.is_some() {
            inner.cancel = cancel;
        }
        // **The incumbent ends with NO lock held** (reap gap C6): its teardown is now a SIGTERM
        // and its supervisor's reap, bounded at `owned_process::SUPERVISED_BOUND`, and nothing
        // else on this back end should wait behind it.
        drop(inner);
        drop(lease);
        if let Some(mut retired) = retired {
            // Mid-run, the assignment's grant on the incumbent goes with it (§5.4): the
            // successor holds its own, bound above.
            if carrying.is_some() {
                if let Err(error) = retired.revoke_work_assignment() {
                    eprintln!("[richos] work: the retired back end's grant could not be revoked: {error}");
                }
            }
            drop(retired);
        }
        Ok(())
    }

    /// **What the successor is told, and it is a digest rather than a transcript.**
    ///
    /// Everything here comes off the durable register (`assignment::read_all`) — the same
    /// record recovery reads — so it is crash-safe, bounded, and true whether or not the
    /// outgoing back end managed to say anything. No identifiers travel: the back end is
    /// given the CEO's own words for each open assignment and its state, which is what it
    /// needs to carry on, and ids are the app's (`desktop-work.md:42-43`).
    ///
    /// With `carrying`, the assignment a mid-run switch hands over part-way
    /// ([`Self::rotate_carrying`]): its brief and what it has said so far, and that the next
    /// message continues it. Without `carrying` the text is the between-assignments payload,
    /// unchanged.
    fn handover_payload_with(&self, binding: &ThreadBinding, handoff: &str, carrying: Option<&str>, reason: &str) -> String {
        let mut payload = String::from(match carrying {
            None => {
                r#"{"event":"session_replaced","carrying_assignment":false}"#
            }
            Some(_) if reason == TEAM_CHANGED => MID_RUN_TEAM_HANDOVER_OPENING,
            Some(_) => MID_RUN_HANDOVER_OPENING,
        });
        if let Some(context) = carrying {
            payload.push_str("\nThe assignment you are carrying on, as the previous connection was given it:\n");
            payload.push_str(context.trim());
            payload.push('\n');
        }
        match assignment::read_all(&self.state, &binding.entity_id().to_string(), binding.thread_id()) {
            Ok(rows) => {
                let open: Vec<&Assignment> = rows.iter().filter(|row| row.state.is_open()).collect();
                if open.is_empty() {
                    payload.push_str("\nNo assignment on this conversation is open right now.\n");
                } else {
                    payload.push_str("\nWhat is still open on this conversation:\n");
                    for row in open.iter().take(20) {
                        payload.push_str(&format!("- {} — {}\n", row.title, row.state.as_str()));
                    }
                }
                let settled = rows.len() - open.len();
                if settled > 0 {
                    payload.push_str(&format!("\nAnd {settled} earlier assignment(s) here have already ended.\n"));
                }
            }
            // Honest, and it changes nothing about what the successor may do: it takes its
            // assignment from the host, not from this text.
            Err(_) => payload.push_str("\nThe assignment record could not be read for this handover.\n"),
        }
        if !handoff.trim().is_empty() {
            payload.push_str("\nWhat the previous connection said it was in the middle of:\n");
            payload.push_str(handoff.trim());
            payload.push('\n');
        }
        // **THE CEO'S RULING §52 (2026-09-18) holds across a renewal too.** This line used to
        // read "Stop at any step that would change his repository and wait for him to approve
        // it" (operator spec r1 §8, r3 §8), which held the successor at a land the assignment's
        // own brief tells it to make: *"So, yes, always land on its own."* The successor now
        // takes its orders from that brief (`brief_for`), which says to land the reviewed
        // result and never to claim a land it did not make.
        payload.push_str(match carrying {
            None => {
                ""
            }
            Some(_) => MID_RUN_HANDOVER_CLOSING,
        });
        payload
    }

    /// How many times this conversation's back end has been renewed, and why the last one
    /// was. For tests and for the boot log — never for him: he is never told about session
    /// rotation.
    pub fn rotations(&self, thread: &str) -> (u64, Option<String>) {
        match self.backend(thread) {
            None => (0, None),
            Some(backend) => {
                let inner = backend.inner.lock().unwrap();
                (inner.rotations, inner.last_rotation_reason.clone())
            }
        }
    }

    /// **Hold this conversation's queue** ([`Held`]): `record`, a fresh job that got nothing
    /// done for `cause`, goes back to the FRONT of the queue, and every fresh job in the queue
    /// is `registered` again with the row saying what it waits for and that it will start by
    /// itself. No notice: nothing ended. The runner takes nothing off the queue until the
    /// cause clears ([`Self::hold_cleared`], [`Self::signed_in`]).
    ///
    /// The rows are written under `inner`, so a stop that takes one off the queue
    /// (`stop_assignment`) writes its ending after this and never under it.
    fn hold(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding, record: &Assignment, cause: HoldCause) {
        let detail = cause.detail();
        let mut inner = backend.inner.lock().unwrap();
        // A quit's sweep has already written this job's honest state, `interrupted`; a Stop
        // that reached it before its back end opened wrote "Stopped before it started"
        // (`stop_assignment`: it was not live). The one caller that runs on a live job checks
        // for a Stop itself, so the Stop's own ending is written there.
        if inner.closing || inner.stopped.contains(&record.id) {
            drop(inner);
            self.let_go_if_ended(record);
            return;
        }
        inner.queue.push_front(Scheduled { binding: binding.clone(), record: record.clone(), resumed: false });
        // Told once: when the queue is first held, never again at a trial that is held again.
        let first = inner.held.is_none();
        let notice = first.then(|| cause.notice(&record.title, inner.queue.len() - 1));
        inner.held = Some(Held { cause, trial: false, checked_at: std::time::Instant::now() });
        for job in inner.queue.iter().filter(|job| !job.resumed) {
            if let Err(error) = assignment::advance(&self.state, &job.record.entity_id, &job.record.thread_id,
                &job.record.id, AssignmentState::Registered, &detail)
            {
                eprintln!("[richos] work: a held job could not say so on its row: {error}");
            }
        }
        backend.wake.notify_all();
        drop(inner);
        if let Some(text) = notice {
            self.raise(record, NoticeKind::Held, &text);
        }
    }

    /// **Has a held queue's cause cleared?** Looked at with no model turn: for a sign-in, the
    /// shell's reading of the account ([`Self::set_sign_in`]; none installed is "not yet"); for
    /// a back end that would not open, one attempt to open it, which on success stands as the
    /// back end the first held job then runs on.
    fn hold_cleared(self: &Arc<Self>, backend: &Arc<Backend>, cause: &HoldCause, binding: Option<&ThreadBinding>) -> bool {
        match cause {
            HoldCause::SignIn => {
                let reading = self.sign_in.lock().unwrap().clone();
                reading.is_some_and(|read| read() == Some(true))
            }
            HoldCause::BackEnd(_) => binding.is_some_and(|binding| match self.ensure_lease(backend, binding) {
                Ok(()) => true,
                Err(why) => {
                    eprintln!("[richos] work: the back end still does not open ({why}); the queue stays held");
                    false
                }
            }),
        }
    }

    /// **This conversation's back end opens once and stands** (the Two Riches spec). The
    /// second assignment on the same thread finds the same connection; a second THREAD gets
    /// its own, because the CEO's unit is the conversation and not the app.
    fn ensure_lease(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding) -> Result<(), String> {
        let mut lease = backend.lease.lock().unwrap();
        if lease.is_some() {
            return Ok(());
        }
        // The team it registers, read just before it is spawned (see `Inner::lease_team`).
        let (team, opened) = {
            let factory = self.factory.lock().unwrap();
            (factory.team_digest(), factory.spawn_work(binding))
        };
        let opened = opened.map_err(|e| e.to_string())?;
        // **A back end that finished opening after quit began is not kept** (hunt 2026-09-29
        // part 1, finding 28). Quit no longer waits without bound for this lock, so it may
        // already have swept past this back end; keeping the lease would hand a connection to
        // a job that the sweep has already marked `interrupted`. It is dropped with the lock
        // released, the way every other retirement here drops one.
        let closing = {
            let mut inner = backend.inner.lock().unwrap();
            if !inner.closing {
                inner.lease_session = Some(opened.session_id().to_string());
                inner.lease_repositories = self.connected_repositories(binding);
                inner.lease_team = team;
            }
            inner.closing
        };
        if closing {
            drop(lease);
            drop(opened);
            return Err("RichOS is closing down. Nothing was started.".into());
        }
        *lease = Some(opened);
        Ok(())
    }

    /// **His company's connected repositories, as the registry says now** — the same file
    /// and the same filter `EngineProfile::configure` reads when it opens a back end, sorted
    /// so an order change is not a change. `None` when the registry cannot be read: an
    /// unreadable registry is not a list, and nothing is renewed on the strength of it.
    fn connected_repositories(&self, binding: &ThreadBinding) -> Option<Vec<PathBuf>> {
        let data_dir = self.state.parent()?;
        let load = crate::entity::EntityRegistry::load(&crate::entity::entity_registry_path(data_dir));
        if load.source == crate::entity::RegistrySource::Unreadable {
            return None;
        }
        let mut repositories = load
            .registry
            .get(binding.entity_id())
            .map(|entity| entity.connected_repositories.clone())
            .unwrap_or_default();
        repositories.sort();
        Some(repositories)
    }

    /// **A REPOSITORY HE CONNECTED AFTER THIS BACK END OPENED** — `run_one` step 0b.
    ///
    /// The back end's sandbox roots (`--add-dir`) and its auto mode's trusted repositories
    /// are fixed when its provider is spawned (`EngineProfile::configure`), and a running
    /// provider cannot be given more; `connect_repository` changes the registry, which the
    /// back end's `repositories` tool reads live. So a connection made since the lease opened
    /// renews the back end here, before the assignment that would otherwise run outside it —
    /// a rotation like any other ([`Self::rotate`]): at an assignment boundary with nothing
    /// live, the successor opened first and primed from the register.
    ///
    /// **Deferred, never forced, while the back end has a command running**, for the reason a
    /// context renewal is (reap gap C6): retiring the lease ends its commands, and a command
    /// he asked for must not end because he connected a repository. The next boundary asks
    /// again. A failed renewal leaves the incumbent working, as every renewal does.
    fn renew_if_repositories_changed(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding) {
        if backend.lease.lock().unwrap().is_none() {
            return;
        }
        let Some(now) = self.connected_repositories(binding) else { return };
        let Some(opened_with) = backend.inner.lock().unwrap().lease_repositories.clone() else { return };
        if opened_with == now {
            return;
        }
        let commands = backend.lease.lock().unwrap().as_ref().and_then(|lease| lease.running_commands());
        if matches!(
            commands,
            Some(crate::lease_commands::CommandReading::Running(_) | crate::lease_commands::CommandReading::Unreadable)
        ) {
            eprintln!(
                "[richos] back end: a repository was connected since it opened; renewal deferred \
                 while a command it started may still be running"
            );
            return;
        }
        if let Err(why) = self.rotate(backend, binding, REPOSITORIES_CHANGED) {
            eprintln!("[richos] back end: not renewed for the newly connected repository ({why})");
        }
    }

    /// **FILL-FIRST AT AN ASSIGNMENT BOUNDARY** — `run_one` step 0c (plan §15 answers 3 and
    /// 6-8). The back end's own streamed reading goes to the quota service, which decides on
    /// the freshest readings and measured speed whether its account must be left; if the
    /// account in use is now another one, the back end is renewed under it here, with nothing
    /// live — a rotation like any other ([`Self::rotate`]). The same rule as a repository
    /// change: a command the back end started and may still be running defers it, because
    /// retiring the lease would end that command.
    fn renew_if_account_changed(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding) {
        if !self.account_switch_due(backend) {
            return;
        }
        if let Err(why) = self.rotate(backend, binding, crate::spine::ACCOUNT_SWITCH) {
            eprintln!("[richos] back end: not moved to the next Claude account ({why})");
        }
    }

    /// **THE MODEL HE CHOSE FOR DIRECTING HIS TEAM** (§119) — `run_one` step 0c'. The
    /// conversation's [`crate::spine::MODEL_CHANGED`] renewal for the back end: at an
    /// assignment boundary with nothing live, a back end started on a model his setting no
    /// longer names is renewed, so the next job runs on the one he chose when Preferences
    /// closed. A run already under way finishes on the model it started on. Deferred while a
    /// command it started may still be running; a lease or factory that cannot say renews
    /// nothing; a failed renewal leaves the incumbent working.
    fn renew_if_model_changed(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding) {
        let Some(wanted) = self.factory.lock().unwrap().lease_models() else { return };
        let running_on = backend.lease.lock().unwrap().as_ref().and_then(|lease| lease.lease_model());
        if running_on.is_none_or(|model| model == wanted.stu)
            || self.renewal_waits_for_a_command(backend, "its model changed in Preferences")
        {
            return;
        }
        if let Err(why) = self.rotate(backend, binding, crate::spine::MODEL_CHANGED) {
            eprintln!("[richos] back end: not moved to the model chosen in Preferences ({why})");
        }
    }

    /// **Must this back end leave its Claude account now?** The check before every turn, the
    /// conversation's (`spine.rs`'s `account_switch_due`) for background work: the lease's own
    /// streamed reading goes to the quota service, which decides on the freshest readings and
    /// measured speed (`claude_accounts.rs`) and answers which account is in use. `false` when
    /// it is still this lease's, when the lease cannot say its account, or when a command it
    /// started may still be running — retiring the lease would end that command, so the
    /// switch waits for the next boundary (and the limit backstop covers the gap).
    fn account_switch_due(&self, backend: &Arc<Backend>) -> bool {
        let Some(quota) = self.quota.lock().unwrap().clone() else { return false };
        let (account, streamed, commands) = {
            let lease = backend.lease.lock().unwrap();
            let Some(lease) = lease.as_ref() else { return false };
            let Some(account) = lease.account() else { return false };
            (account.to_string(), lease.streamed_usage(), lease.running_commands())
        };
        if quota.before_turn(&account, streamed) == account {
            return false;
        }
        if matches!(commands, Some(crate::lease_commands::CommandReading::Running(_) | crate::lease_commands::CommandReading::Unreadable)) {
            eprintln!("[richos] back end: its Claude account is to be left; renewal deferred while a command it started may still be running");
            return false;
        }
        true
    }

    /// **FILL-FIRST BETWEEN THE CONTINUATION TURNS OF ONE RUN** (plan §15 answers 3 and 6-8;
    /// the CEO, 2026-10-04: *"How can 'work cut off by a limit' happen if THE WHOLE ... JOB OF
    /// THIS ENTIRE ... FEATURE IS TO PREVENT THAT FROM HAPPENING????"*). `run_one` calls this
    /// before every continuation turn, as the conversation checks before every turn: a long
    /// run whose account crosses its check point moves to the account in use at the next
    /// boundary, with the assignment carried across ([`Self::rotate_carrying`]), and the run
    /// goes on there. It is not started again. Never inside a turn (continuity §3.1): between
    /// two turns of a run nothing is live on the lease — its helpers were witnessed ending
    /// (step 3b) and a command still running defers the switch ([`Self::account_switch_due`]).
    ///
    /// `true` when the run is now on a new lease. A switch that fails leaves the run on the
    /// incumbent, still bound, and the limit backstop covers a turn that then runs out.
    fn switch_account_between_turns(
        self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding, record: &Assignment,
        work: &WorkAssignment, context: impl FnOnce() -> String,
    ) -> bool {
        if !self.account_switch_due(backend) {
            return false;
        }
        let context = context();
        if let Err(why) = self.rotate_carrying(backend, binding, crate::spine::ACCOUNT_SWITCH, Some((work, &context))) {
            eprintln!("[richos] work: this run stays on its Claude account for now; the move to the next one failed ({why})");
            return false;
        }
        eprintln!("[richos] work: this run moved to the next Claude account between two of its turns and carries on there");
        self.note_successor_session(backend, record);
        true
    }

    /// What a relaunch reconciles against is the session a run is on now (spec §6.1): written
    /// after a renewal between two of its turns moved it to a successor.
    fn note_successor_session(&self, backend: &Arc<Backend>, record: &Assignment) {
        let session = backend.inner.lock().unwrap().lease_session.clone();
        if let Some(session) = session {
            let pins = assignment::read(&self.state, &record.entity_id, &record.thread_id, &record.id)
                .map(|row| row.repository_pins)
                .unwrap_or_else(|_| record.repository_pins.clone());
            if let Err(error) = assignment::note_start(&self.state, &record.entity_id, &record.thread_id, &record.id, &session, pins) {
                eprintln!("[richos] work: the run's new connection was not recorded: {error}");
            }
        }
    }

    /// **Has the team on disk moved since this back end's lease registered its own?** The
    /// third renewal reason (proto-teammate shelf plan §1, slice 5), on the design of
    /// `operator_snapshot.rs`: a session reads its definitions once, at start, so the digest of
    /// what it read is kept and compared at each boundary. `false` with no lease, or when
    /// either digest is unknown: a reading that failed is never a change.
    fn team_changed(&self, backend: &Arc<Backend>) -> bool {
        if backend.lease.lock().unwrap().is_none() {
            return false;
        }
        let Some(opened_with) = backend.inner.lock().unwrap().lease_team.clone() else { return false };
        let Some(now) = self.factory.lock().unwrap().team_digest() else { return false };
        opened_with != now
    }

    /// `true`, and said in the log, when a command this back end started may still be
    /// running: retiring its lease would end that command, so a renewal waits for the next
    /// boundary (reap gap C6). Unreadable is never read as nothing running.
    fn renewal_waits_for_a_command(&self, backend: &Arc<Backend>, why: &str) -> bool {
        let commands = backend.lease.lock().unwrap().as_ref().and_then(|lease| lease.running_commands());
        let waits = matches!(
            commands,
            Some(crate::lease_commands::CommandReading::Running(_) | crate::lease_commands::CommandReading::Unreadable)
        );
        if waits {
            eprintln!("[richos] back end: {why}; renewal deferred while a command it started may still be running");
        }
        waits
    }

    /// **A TEAMMATE SAVED, REFITTED OR RETIRED SINCE THIS BACK END OPENED** — `run_one` step
    /// 0d (slice 5). The same renewal as a repository connected since it opened
    /// ([`Self::renew_if_repositories_changed`]): at an assignment boundary with nothing live,
    /// the successor opened first and primed from the register, deferred while a command it
    /// started may still be running, and a failed renewal leaves the incumbent working.
    fn renew_if_team_changed(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding) {
        if !self.team_changed(backend) || self.renewal_waits_for_a_command(backend, "its team changed since it opened") {
            return;
        }
        let started = std::time::Instant::now();
        match self.rotate(backend, binding, TEAM_CHANGED) {
            Ok(()) => eprintln!(
                "[richos] back end: renewed because its team changed, before the next assignment, in {} ms",
                started.elapsed().as_millis()
            ),
            Err(why) => eprintln!("[richos] back end: not renewed for its changed team ({why})"),
        }
    }

    /// **THE TEAM, BETWEEN THE TURNS OF ONE RUN** (slice 5; plan §1: *"a teammate Dean
    /// activates is usable on the back end's next turn after Dean's run ends, in the same
    /// assignment, with no restart of the job"*). Beside [`Self::switch_account_between_turns`]
    /// and built the same way: never inside a turn, the assignment carried across
    /// ([`Self::rotate_carrying`]) to a successor that registers the team as it is now, and a
    /// command the back end started that may still be running defers it to the next boundary.
    ///
    /// `true` when the run is now on the renewed lease. A renewal that fails leaves the run on
    /// the incumbent, still bound, and the saved teammate waits for the next boundary.
    fn renew_team_between_turns(
        self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding, record: &Assignment,
        work: &WorkAssignment, context: impl FnOnce() -> String,
    ) -> bool {
        if !self.team_changed(backend) || self.renewal_waits_for_a_command(backend, "its team changed during this run") {
            return false;
        }
        let context = context();
        let started = std::time::Instant::now();
        if let Err(why) = self.rotate_carrying(backend, binding, TEAM_CHANGED, Some((work, &context))) {
            eprintln!("[richos] work: this run keeps its connection for now; renewing it for the changed team failed ({why})");
            return false;
        }
        eprintln!(
            "[richos] work: this run's connection was renewed between two of its turns because its team changed, in {} ms, and the run carries on there",
            started.elapsed().as_millis()
        );
        self.note_successor_session(backend, record);
        true
    }

    fn raise(self: &Arc<Self>, record: &Assignment, kind: NoticeKind, text: &str) {
        if assignment::raise_notice(&self.state, &record.entity_id, &record.thread_id, &record.id, kind, text)
            .is_err()
        {
            return;
        }
        self.notifier.raised(
            &record.thread_id,
            &PendingNotice {
                assignment_id: record.id.clone(),
                title: record.title.clone(),
                kind,
                text: text.to_string(),
                raised_at_ms: assignment::now_ms(),
            },
        );
    }

    /// **Spec §2.9, and it is the whole of it.** The turn-end settlement check reads the
    /// CONVERSATION lease's session id (`native.rs:2059`) and is structurally blind to a
    /// second lease, whose evidence lives in its own session-keyed directory
    /// (`app_workers.rs:38`). So §0 row 8's guarantee is re-founded here: the same
    /// evidence, the same function, the work lease's session id.
    ///
    /// The rules are unchanged (`app_workers.rs:33-47`): unattributed or unreadable
    /// evidence counts as still running, never as zero.
    ///
    /// **`StillRunning` here means "not witnessed as ended", and the sentence it carries is
    /// only true while a turn is open.** This function answers what can be WITNESSED; it
    /// does not answer what state an assignment is in. `run_one` is the one caller, it calls
    /// this only after `prompt` has returned, and since 2026-09-18 it reads this answer as a
    /// missing witness rather than as a running job — see the `Outcome::StillRunning` arm
    /// there for the run that made the difference matter.
    pub fn settlement(&self, thread: &str) -> Settlement {
        // **No back end on this conversation is NOT a zero here.** It is the same reading it
        // has always been — no session, therefore nothing attributed, therefore *"still
        // running"* — because this module's standing rule is that anything it cannot witness
        // counts as running rather than as nothing (`app_workers.rs:33-47`). The declared
        // exception to fail-toward-waiting lives in `work_gate`, where it is about whether an
        // UPDATE may install; it is not this function's to borrow.
        let session = self
            .backend(thread)
            .and_then(|backend| backend.inner.lock().unwrap().lease_session.clone());
        let view = crate::app_workers::status(&self.state, session.as_deref());
        if !view.is_attributed() {
            // **WHICH of the reasons it was, in the log and never on his screen.** The
            // sentence below is one thing for him; `Unattributed` is four different things
            // for whoever has to fix it (no session, an unusable session id, an unreadable
            // evidence directory, a callback row that did not belong to this session). The
            // first real run of the whole flow ended on this branch and the record could not
            // say which, because nothing wrote the reason down.
            eprintln!(
                "[richos] work: no worker evidence could be attributed to this back end: {:?}",
                view.unattributed
            );
            return Settlement::StillRunning(
                "The work connection's records could not be read, so this counts as still running.".into(),
            );
        }
        if view.active > 0 || view.liveness_unknown > 0 {
            return Settlement::StillRunning("Workers are still open on this assignment.".into());
        }
        Settlement::Settled
    }

    /// **What the assignment's state actually is once its work turn has ended — and the
    /// order of the two questions is the whole of it.**
    ///
    /// 1. **Are this lease's workers all observed ending?** (spec §2.9, the work lease's
    ///    own session). If not, it is still running, and anything unwitnessed counts as
    ///    running rather than as zero.
    /// 2. **Only then, is the OBLIGATION closed?** The engine is explicit that the workers
    ///    do not get to answer this: *"An assignment whose workers have all stopped is NOT
    ///    settled: that is exactly the state where it has run, stopped at integrate and is
    ///    waiting for him"* (`richos/engine/mega-lander/app.py:664-669`).
    ///
    /// **The app built the opposite of this before that engine landed** — every worker
    /// observed ending, therefore settled — and it would have told him a thing was finished
    /// at the precise moment it was waiting for him, which is spec §0 row 7's one sentence
    /// inverted. An obligation this build cannot read is `StillRunning`, never `Settled`.
    fn outcome(&self, backend: &Arc<Backend>, record: &Assignment) -> Outcome {
        if let Settlement::StillRunning(detail) = self.settlement(&record.thread_id) {
            return Outcome::StillRunning(detail);
        }
        let state = {
            let lease = backend.lease.lock().unwrap();
            match lease.as_ref() {
                Some(lease) => lease.obligation_state(&record.obligation_id),
                None => Err(CognitionError::Protocol("The work connection closed.".into())),
            }
        };
        match state {
            Ok(crate::cognition::ObligationState::Settled) => Outcome::Settled,
            Ok(crate::cognition::ObligationState::Open) => Outcome::NotSettled,
            // An obligation that is not there is not a finished one. It is a record this
            // build cannot account for, and the honest answer is that it is unresolved.
            Ok(crate::cognition::ObligationState::Absent) => Outcome::StillRunning(
                "The record behind this assignment could not be found, so nothing is being called finished."
                    .into(),
            ),
            Err(_) => Outcome::StillRunning(
                "The record behind this assignment could not be read, so this counts as still running.".into(),
            ),
        }
    }

    /// Stop ONE assignment — spec §4.2's per-assignment stop control, *"where the work is
    /// already visible"*.
    ///
    /// It revokes that assignment's grant and cancels the work lease only if that
    /// assignment is the one on it. A queued assignment is marked and never starts. Spec
    /// §5.4: *"Stopping one assignment revokes one grant and one seat and leaves every
    /// other assignment's pair alone."*
    pub fn stop_assignment(self: &Arc<Self>, entity: &str, thread: &str, id: &str) -> Result<(), String> {
        let record = assignment::read(&self.state, entity, thread, id).map_err(|e| e.to_string())?;
        if !record.state.is_open() {
            return Err("That assignment has already stopped.".into());
        }
        // Revocation must outlive a crash even while dispatch holds its receipt lock.
        let stopped = assignment::record_user_stop(&self.state, entity, thread, id).map_err(|e| e.to_string())?;
        let operator = self.operator.lock().unwrap().clone();
        if let Some(operator) = operator {
            return operator.stop(entity, thread, id);
        }
        let peer = self.backend(thread).and_then(|backend| {
            let inner = backend.inner.lock().unwrap();
            if inner.live.is_some() { return None; }
            inner.peer_stopper.clone()
        });
        if let Some((session, stopper)) = peer {
            let dispatch_lock = crate::work_status::lock_receipts(&self.state, entity, thread)?;
            let trail = crate::work_status::trail(&self.state, entity, thread, &record.obligation_id)?;
            drop(dispatch_lock);
            let mut targets = trail.stop_targets;
            targets.extend(crate::work_status::direct_stop_targets(&self.state, &session,
                &record.instruction_ledger_ref, &record.instruction_sha256)?.into_iter().map(|tool| (session.clone(), tool)));
            let outcomes: Vec<CommandStop> = targets.iter().map(|(owner, tool)| {
                if owner == &session {
                    stopper.stop_worker(tool)
                } else { CommandStop::NotDelivered }
            }).collect();
            self.forget_at_the_desk(&record);
            crate::questions::Store::new(&self.state).close(entity,thread,Some(&record.obligation_id),"The assignment was stopped",false)?;
            drop(crate::question_work::discard(&self.state,entity,thread,&record.obligation_id));
            assignment::advance(&self.state, entity, thread, id, AssignmentState::Interrupted,
                if outcomes.is_empty() { "Stop requested by the user. No native execution is recorded." }
                else { watched_stop_detail(true, &outcomes) }).map_err(|e| e.to_string())?;
            let scope = crate::status_tools::StatusToolScope {version:1,state_root:self.state.clone(),entity_id:entity.into(),thread_id:thread.into()};
            crate::session_tools::observe_user_stop(&scope, id, &record.obligation_id, stopped.user_stop_requested_at_ms.unwrap())?;
            if let Some(binding) = self.backend(thread).and_then(|b| b.inner.lock().unwrap().binding.clone()) {
                self.wake_messages(&binding);
            }
            return Ok(());
        }
        // **Stopping one assignment takes its question off his screen** (spec §5.4: one
        // grant, one seat, one queue entry, all released together and none of anybody
        // else's). A request left waiting for an assignment that has stopped is a question
        // he can answer into nothing.
        self.forget_at_the_desk(&record);
        crate::questions::Store::new(&self.state).close(entity,thread,Some(&record.obligation_id),"The assignment was stopped",false)?;
        drop(crate::question_work::discard(&self.state,entity,thread,&record.obligation_id));
        // Only this conversation's back end. A stop on one thread never reaches another's —
        // the CEO's page puts a whole back-end Rich behind each thread, and two threads are
        // two pieces of work he thinks of separately.
        let cancel = match self.backend(thread) {
            None => None,
            Some(backend) => {
                let mut inner = backend.inner.lock().unwrap();
                if !inner.stopped.iter().any(|held| held == id) {
                    inner.stopped.push(id.to_string());
                }
                // **A job watched for its command** ([`Watched`]): its turn is long over, so
                // it ends the way a stop inside its wait does, and its finish is not asked for.
                if let Some(at) = inner.watching.iter().position(|watched| watched.record.id == id) {
                    let watched = inner.watching.remove(at);
                    drop(inner);
                    // **And its commands are told to stop** (hunt 2026-09-29 part 1, finding
                    // 11). Taking the job off the watch alone left them running and changing
                    // his workspace after he saw it stopped. Each is stopped by the provider's
                    // own task id on the lease it runs on, so no turn is interrupted and no
                    // other job's command is touched.
                    //
                    // **He is told a command stopped only when the provider said it ended**
                    // (finding 11, v2 re-check): a written request used to count as stopped,
                    // and a provider that then refused it left the command running under a
                    // job that said "Stopped." with no qualifier. Each command's answer is the
                    // provider's own ([`CommandStop`]); the sentence is the worst of them.
                    let outcomes: Vec<CommandStop> = watched
                        .commands
                        .iter()
                        .map(|(task, what)| {
                            let outcome = watched
                                .stopper
                                .as_ref()
                                .map_or(CommandStop::NotDelivered, |stop| stop.stop_background_command(task));
                            if outcome != CommandStop::Ended {
                                eprintln!("[richos] work: a stopped job's command was not seen to stop ({what}): {outcome:?}");
                            }
                            outcome
                        })
                        .collect();
                    let detail = watched_stop_detail(watched.stopper.is_some(), &outcomes);
                    assignment::advance(&self.state, entity, thread, id, AssignmentState::Interrupted, detail)
                        .map_err(|e| e.to_string())?;
                    if let Err(error) = assignment::raise_notice(&self.state, entity, thread, id, NoticeKind::Interrupted,
                        &assignment::says::interrupted(&record.title))
                    {
                        eprintln!("[richos] work: a stopped assignment's notice could not be recorded: {error}");
                    }
                    return Ok(());
                }
                // **A stop must reach an assignment parked on a locked screen** (the CEO's
                // ruling §56). It is not `live` — the screen gate runs before the lease — and
                // its wait sleeps on its own condition variable, so neither the cancel handle
                // below nor `backend.wake` would touch it. Stopping the watch makes the runner
                // return, and the `None` arm below then writes the honest state: "Stopped
                // before it started. Nothing was prepared." — which is exactly true of a
                // screen wait.
                if let Some((waiting, watch)) = inner.screen_wait.as_ref() {
                    if waiting == id {
                        watch.stop();
                    }
                }
                let live = inner.live.as_ref().is_some_and(|live| live.id == id);
                if live {
                    inner.cancel.clone()
                } else {
                    inner.queue.retain(|scheduled| scheduled.record.id != id);
                    None
                }
            }
        };
        match cancel {
            Some(handle) => {
                handle.cancel();
            }
            None => {
                assignment::advance(
                    &self.state,
                    entity,
                    thread,
                    id,
                    AssignmentState::Interrupted,
                    "Stopped before it started. Nothing was prepared.",
                )
                .map_err(|e| e.to_string())?;
                let notice = assignment::says::interrupted(&record.title);
                let _ = assignment::raise_notice(&self.state, entity, thread, id, NoticeKind::Interrupted, &notice);
            }
        }
        Ok(())
    }

    /// Everything still open, on every thread. The seam the update gate (spec §6.4) and the
    /// window-closed exit decision (§2.4) need; built here, wired in the next slice.
    pub fn open_assignments(&self) -> Result<Vec<Assignment>, String> {
        assignment::open(&self.state).map_err(|e| e.to_string())
    }

    /// **What the update gate must see** — background-work spec §6.4, which calls reading it
    /// *"not optional and not a follow-up"*.
    ///
    /// **The derivation lives here rather than in the shell for the reason the gate's own
    /// module doc gives about every other reading:** `app/src-tauri` carries the whole webview
    /// dependency tree and is deliberately detached from the workspace, so a count made there
    /// would be a count this suite could not test. The shell calls this and passes the answer
    /// to [`crate::work_gate::background`].
    ///
    /// An assignment holding a question for him is counted apart from one that is running,
    /// because §6.5 insists they are different sentences and only one of them is about him.
    /// A register that cannot be read is `readable: false` — never a zero.
    pub fn background_work(&self) -> crate::work_gate::BackgroundWork {
        // **On an operator install the register is not what says whether his team is
        // running** (r3 (m)): an assignment stays open until his lead reports on it, which may
        // be never, and counting it would hold every update and keep a windowless app alive
        // for nothing. The shell reads `work_gate::operator_team` for that. An unreadable
        // register is still never read as zero.
        if self.operator.lock().unwrap().is_some() {
            return match self.open_assignments() {
                Ok(_) => crate::work_gate::BackgroundWork::nothing(),
                Err(_) => crate::work_gate::BackgroundWork { running: 0, awaiting_you: 0, readable: false },
            };
        }
        match self.open_assignments() {
            Ok(open) => {
                // **A queue held because the back end would not open is not open work.** An update
                // is the likeliest cure for a broken build, so counting those jobs would hold the
                // very update that fixes it (and keep a windowless app open for nothing). A queue
                // held for sign-in still counts: an update does not fix a sign-in.
                let back_end_held = self.back_end_held_ids();
                let open: Vec<Assignment> = open.into_iter().filter(|row| !back_end_held.contains(&row.id)).collect();
                let awaiting_you = open.iter().filter(|row| self.pending_decision(row).is_some()).count();
                crate::work_gate::BackgroundWork {
                    running: open.len() - awaiting_you,
                    awaiting_you,
                    readable: true,
                }
            }
            Err(_) => crate::work_gate::BackgroundWork { running: 0, awaiting_you: 0, readable: false },
        }
    }

    pub fn peer_work_pending(&self)->bool {
        let backends:Vec<_>=self.backends.lock().unwrap().values().cloned().collect();
        backends.into_iter().any(|backend| {
            let inner=backend.inner.lock().unwrap();
            if inner.processing && inner.live.is_none() {return true}
            inner.binding.as_ref().is_some_and(|binding| {
                let scope=self.message_scope(binding);
                ["rich","stu"].iter().any(|role|crate::session_tools::pending(&scope,role).map(|m|m.is_some()).unwrap_or(true))
            })
        })
    }

    /// The ids of the fresh jobs waiting in a queue held because the back end would not open
    /// ([`HoldCause::BackEnd`]); see [`Self::background_work`].
    fn back_end_held_ids(&self) -> std::collections::HashSet<String> {
        let backends: Vec<Arc<Backend>> = self.backends.lock().unwrap().values().cloned().collect();
        let mut ids = std::collections::HashSet::new();
        for backend in backends {
            let inner = backend.inner.lock().unwrap();
            if inner.held.as_ref().is_some_and(|held| matches!(held.cause, HoldCause::BackEnd(_))) {
                ids.extend(inner.queue.iter().filter(|job| !job.resumed).map(|job| job.record.id.clone()));
            }
        }
        ids
    }

    /// **The fresh jobs waiting in THIS conversation's held queue** ([`Held`], any cause): written
    /// down and not started, and none of them will start until the cause clears. The window counts
    /// them apart from jobs that are starting (nightly 48's walk, D26: on d0d06501c its status line
    /// read "2 assignments starting" right under the notice saying neither had started, because a
    /// held job's row is `registered` like any job about to start). Read from the live hold, not
    /// from a row's words: a job still queued after the hold ends keeps the hold's words on its
    /// row until it is taken. Never opens a back end.
    pub fn held_on(&self, thread: &str) -> std::collections::HashSet<String> {
        let Some(backend) = self.backend(thread) else { return Default::default() };
        let inner = backend.inner.lock().unwrap();
        if inner.held.is_none() {
            return Default::default();
        }
        inner.queue.iter().filter(|job| !job.resumed).map(|job| job.record.id.clone()).collect()
    }

    /// This conversation's back end, if one has been opened. Never creates one — a reader
    /// must not be able to start a back end by asking about it.
    fn backend(&self, thread: &str) -> Option<Arc<Backend>> {
        self.backends.lock().unwrap().get(thread).map(Arc::clone)
    }

    /// Which assignment is on THIS CONVERSATION's back end right now, if any. Read WITHOUT
    /// the spine lock, so the surface can show running work between turns — spec §7's
    /// observability note.
    ///
    /// **It is per thread because the back end is** (the Two Riches spec): asking the host
    /// globally would answer with another conversation's work, which is the one answer that
    /// is never useful on a thread's own surface.
    pub fn live_on(&self, thread: &str) -> Option<LiveAssignment> {
        self.backend(thread)?.inner.lock().unwrap().live.clone()
    }

    /// Every open back end's session id, one per conversation that has one. The update gate
    /// reads all of them (spec §6.4): one thread's back end is as invisible to the
    /// conversation lease's session as any other's.
    pub fn lease_sessions(&self) -> Vec<String> {
        self.backends
            .lock()
            .unwrap()
            .values()
            .filter_map(|backend| backend.inner.lock().unwrap().lease_session.clone())
            .collect()
    }

    /// Quit. Spec §2.5: *"quit must stop the WORK lease explicitly"* — the conversation's
    /// `shutdown_lease()` reaches one cancel handle (`steering.rs:749-751`), a second
    /// lease's fence is not on it, and destructors are not guaranteed to run (§2.3).
    ///
    /// Every assignment that was open when quit arrived becomes `interrupted`. Nothing
    /// becomes `settled` on the way out.
    pub fn shutdown(&self) {
        // Refuse a NEW conversation's back end from here on. A thread that has never been
        // opened must not be opened on the way out, and no single back end can answer that.
        *self.closing.lock().unwrap() = true;
        let backends: Vec<Arc<Backend>> = self.backends.lock().unwrap().values().map(Arc::clone).collect();
        for backend in &backends {
            let cancel = {
                let mut inner = backend.inner.lock().unwrap();
                inner.closing = true;
                inner.queue.clear();
                // **A QUIT MUST REACH A RUNNER PARKED ON A LOCKED SCREEN** (the CEO's ruling
                // §56). Its wait has no timeout by design, and it sleeps on its OWN condition
                // variable — so `backend.wake.notify_all()` below does not wake it and the
                // `live.is_some()` drain further down never sees it, because a screen wait
                // happens before the lease is opened and `live` is still `None`. Without this
                // line a locked screen would make the app unquittable.
                if let Some((_, watch)) = inner.screen_wait.as_ref() {
                    watch.stop();
                }
                inner.cancel.clone()
            };
            backend.wake.notify_all();
            if let Some(handle) = cancel {
                handle.shutdown();
            }
        }
        // **Let each runner put its live assignment down before sweeping.** The sweep below
        // is a write, and so is a runner's own last write; racing them is how a quit ends
        // with a receipt that says `running`. Bounded, because a lease that will not answer
        // its cancel must not hold the app open — after the bound the sweep runs anyway and
        // the honest state is still `interrupted`.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        for backend in &backends {
            let mut inner = backend.inner.lock().unwrap();
            while inner.processing {
                let left = deadline.saturating_duration_since(std::time::Instant::now());
                if left.is_zero() {
                    break;
                }
                inner = backend.wake.wait_timeout(inner, left).unwrap().0;
            }
        }
        if let Ok(open) = assignment::open(&self.state) {
            for record in open {
                // Nothing may be left waiting for an answer from a process that is going
                // away (§5.4's revocation, at quit, for every assignment then registered).
                self.forget_at_the_desk(&record);
                let _ = assignment::advance(
                    &self.state,
                    &record.entity_id,
                    &record.thread_id,
                    &record.id,
                    AssignmentState::Interrupted,
                    "RichOS was closed while this was running. Everything it had done is kept.",
                );
            }
        }
        // **The same bound holds here** (hunt 2026-09-29 part 1, finding 28). The lease lock
        // is held for as long as a back end takes to OPEN (`ensure_lease` holds it across
        // `spawn_work`), and a blocking lock here waited that long after the bound above had
        // expired. A lock still held at the bound is skipped, and nothing it guards is lost:
        // a back end still opening has been given no grant (the bind comes after it), and
        // `ensure_lease` does not keep one that finishes opening now; a back end still in a
        // turn had its grants revoked and its process fence killed by `handle.shutdown()`
        // above (`native.rs`'s `NativeCancelHandle`).
        for backend in &backends {
            let Some(mut lease) = lock_until(&backend.lease, deadline) else {
                eprintln!(
                    "[richos] work: quit did not wait for a back end that was still opening or ending its turn"
                );
                continue;
            };
            if let Some(lease) = lease.as_mut() {
                let _ = lease.revoke_work_assignment();
            }
            *lease = None;
        }
    }

    /// Block until the runner has finished `count` assignments. Test-only scaffolding, and
    /// it is here rather than in the test module because a test that slept on a guess would
    /// be a flake generator on a loaded machine.
    #[doc(hidden)]
    pub fn wait_for_completed(&self, count: u64, limit: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + limit;
        loop {
            // Summed across back ends, because the unit of progress a test cares about is
            // "assignments finished", and they may be finishing on two conversations at once.
            let done: u64 = self
                .backends
                .lock()
                .unwrap()
                .values()
                .map(|backend| backend.inner.lock().unwrap().completed)
                .sum();
            if done >= count {
                return true;
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
}

/// **Take `lock` if it can be had by `deadline`**, trying at least once however late it is.
/// For the quit path, where a lock held by a back end that is still opening must not hold the
/// app open (hunt 2026-09-29 part 1, finding 28). A poisoned lock is still a lock: its
/// holder panicked and is gone, so what it guards is taken as it is.
fn lock_until<T>(lock: &Mutex<T>, deadline: std::time::Instant) -> Option<std::sync::MutexGuard<'_, T>> {
    loop {
        match lock.try_lock() {
            Ok(guard) => return Some(guard),
            Err(std::sync::TryLockError::Poisoned(poisoned)) => return Some(poisoned.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => {}
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// What the work lease's own EVIDENCE says about its workers (spec §2.9) — and note that
/// `Settled` here means *"every worker this lease started was observed ending"*, which is
/// necessary and **not sufficient** for the assignment to be finished. [`Outcome`] is the
/// thing that decides that.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Settlement {
    Settled,
    StillRunning(String),
}

/// What the ASSIGNMENT is, once its work turn has ended. Three answers, and the middle one
/// changed meaning on 2026-09-18 without changing its reading.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The obligation is closed.
    Settled,
    /// **The workers are done and the obligation is still open**, which used to be named
    /// `ReadyToApprove` because it could only mean one thing: the work had reached the step
    /// that would change his repository and stopped there, waiting for him (spec §0 row 7,
    /// §5.4, §7.8).
    ///
    /// **CEO ruling §52 (2026-09-18) removed that single meaning, so the name had to go
    /// too.** A land is no longer a question, so a run that ends with its obligation open is
    /// one of two different things, and only the DESK can say which: a genuine permission
    /// request of his is waiting (still `blocked`, still his to answer), or nothing is
    /// waiting and the job simply did not finish — a failed land, reported as one. This
    /// variant deliberately does not decide between them: the reading is the same, and the
    /// decision belongs where the evidence is.
    NotSettled,
    StillRunning(String),
}

/// The assignment, as the work lease is told it.
///
/// **It carries no ids.** Spec §1.2 and `desktop-work.md:42-43`: users describe their
/// assignment, Rich owns internal obligation and receipt ids. The seat and the frozen
/// instruction reference reach the child through its scope file, not through its prompt,
/// so a model that decided to quote its own prompt back cannot leak either.
fn instruction_for(state:&Path,record:&Assignment)->Result<String,&'static str> {
    verified_instruction(state,&record.entity_id,&record.thread_id,&record.instruction_ledger_ref,&record.instruction_sha256)
}

pub(crate) fn verified_instruction(state:&Path,entity:&str,thread:&str,ledger_ref:&str,sha256:&str)->Result<String,&'static str> {
    use sha2::{Digest, Sha256};
    use std::io::BufRead;
    const UNAVAILABLE: &str = "The original request could not be verified. Nothing was started.";
    let prefix = format!("ledger:{thread}:");
    let turn = ledger_ref.strip_prefix(&prefix)
        .filter(|turn| !turn.is_empty()).ok_or(UNAVAILABLE)?;
    let path = state.parent().ok_or(UNAVAILABLE)?.join("conversation-ledger.jsonl");
    let file = std::fs::File::open(path).map_err(|_| UNAVAILABLE)?;
    let mut reader = std::io::BufReader::new(file);
    let mut line = String::new();
    let mut found = None;
    loop {
        line.clear();
        if reader.read_line(&mut line).map_err(|_| UNAVAILABLE)? == 0 { break; }
        // A concurrent append may not have completed its last record yet.
        if !line.ends_with('\n') { break; }
        let Ok(row) = serde_json::from_str::<serde_json::Value>(&line) else { continue; };
        if row["event"] != "PromptReceived" || row["turn_id"] != turn { continue; }
        let text = row["text"].as_str().ok_or(UNAVAILABLE)?;
        if found.is_some() || row["thread_id"] != thread
            || row["entity_id"] != entity
            || !matches!(row["source"].as_str(), Some("text" | "jam"))
            || format!("{:x}", Sha256::digest(text.as_bytes())) != sha256
        { return Err(UNAVAILABLE); }
        found = Some(text.to_owned());
    }
    found.ok_or(UNAVAILABLE)
}

fn brief_for(record: &Assignment, resumed: bool, instruction: &str) -> String {
    serde_json::json!({"from":"rich", "assignment": {
        "message": if record.handoff.is_empty() { &record.title } else { &record.handoff },
        "user_request": instruction,
        "repositories": record.repositories,
        "question": record.kind.is_question(),
        "resumed_after_approval": resumed,
    }}).to_string()
}

/// **A provider tool name, in his language.** `Bash` is a wire identifier; what he is being
/// asked is whether to run a command on his Mac.
///
/// Anything unrecognized falls back to a phrase that claims nothing about what the step
/// does — a made-up description of an action he is about to authorize would be worse than
/// no description at all.
///
/// **`mcp__richos_work__integrate` was the first arm of this table and is deliberately
/// gone** (CEO ruling §52, 2026-09-18). A land never reaches the permission desk now
/// (`permissions.rs`), so nothing can put it in his queue and nothing can render the phrase
/// *"putting the finished work into your repository"*. Leaving the arm in would have left
/// wording in the product for a state the product can no longer be in, which is the same
/// defect as a missing one wearing the opposite costume.
pub fn plain_action(tool: &str) -> &'static str {
    match tool {
        "Bash" => "running a command on your Mac",
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => "changing a file on your Mac",
        _ => "a step it cannot take without you",
    }
}

/// One, two or several things said the way a person says them: `a`, `a and b`, `a, b and c`.
///
/// **A list read aloud is the case this exists for.** A background job may land in more than
/// one repository, and `cc/one, cc/two` spoken is a sentence he cannot parse. It is also why
/// there is no trailing comma before the `and`: the CEO's own list convention throughout this
/// app is the spoken one.
fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [first, second] => format!("{first} and {second}"),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// A failure sentence he can act on, with no stack trace in it (spec §3.7).
fn honest(why: &str) -> String {
    let flattened: String = why.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    let trimmed = flattened.split_whitespace().collect::<Vec<_>>().join(" ");
    let named = strip_engineer_prefix(&trimmed);
    // **The detail is not discarded, it is MOVED.** Row 8 asks for the seam label off his
    // timeline, not for it to stop existing: a plugin that did not load is diagnosed from the
    // label. Logged only when there was one to strip, so the log gains a line exactly when the
    // card loses one.
    if !std::ptr::eq(named, trimmed.as_str()) {
        eprintln!("[richos] work: reported to him as \"{named}\"; the connection said: {trimmed}");
    }
    let bounded: String = named.chars().take(200).collect();
    if bounded.is_empty() {
        return "No reason was recorded.".into();
    }
    // **A card is a sentence, so it starts like one.** This became load-bearing the moment the
    // seam label came off: the label used to supply the opening word, and behind it sit reasons
    // that are lowercase by construction — `CognitionError::Io(e.to_string())` wraps whatever
    // the operating system said. The curated sentences are already capitalized and are
    // untouched by this. Only a leading lowercase ASCII letter is raised, so a reason that opens
    // on a quote, a number or a path is left exactly as it is.
    let mut sentence = bounded;
    if sentence.starts_with(|c: char| c.is_ascii_lowercase()) {
        let rest = sentence.split_off(1);
        sentence = sentence.to_ascii_uppercase() + &rest;
    }
    if sentence.ends_with('.') {
        sentence
    } else {
        format!("{sentence}.")
    }
}

/// **Take the engineer's label off a sentence that is about to appear on his timeline.**
///
/// Ray's candidate-.7 audit, row 8: his failure card read *"cognition protocol: The desktop
/// engine plugin did not load"*. The sentence after the colon is a good sentence — it names
/// what did not happen, in his terms, and it was written for him. The prefix is `thiserror`'s
/// `Display` on [`CognitionError`] (`cognition.rs:19-24`), which exists so the wire, the
/// stderr log and a panic message all say which seam an error came from.
///
/// **So the prefix is not wrong, it is just not his.** It is stripped here and nowhere else:
/// `honest` is the one funnel between a `CognitionError` and a sentence he reads, so
/// [`CognitionError`]'s own `Display` keeps the label for every log, every `eprintln!` and
/// every test that asserts on a seam. The protocol detail goes to the log, the card names what
/// did not happen.
///
/// Matching is by exact prefix on the two variants that carry a seam label, never by searching
/// for a colon: a reason of his own can legitimately contain one ("the land lock timed out
/// after 300 s: nothing was merged"), and cutting at the first colon would eat it.
fn strip_engineer_prefix(sentence: &str) -> &str {
    // `cognition.rs:19-24`. `PrimingStopped` is deliberately absent — "Preparation stopped
    // with …" is already a sentence about the job rather than about a seam.
    const LABELS: [&str; 2] = ["cognition protocol: ", "cognition io: "];
    for label in LABELS {
        if let Some(rest) = sentence.strip_prefix(label) {
            return rest.trim_start();
        }
    }
    sentence
}

#[cfg(test)]
mod honest_sentence_tests {
    use super::*;

    /// **Handoff round 1 (2026-10-07), run 6's status: "Handed over cleanly.Handed over
    /// cleanly."** The back end's first turn and its handoff continuation (a helper turn, which
    /// adds to the account) both said "Handed over cleanly.", and the two ran together. The
    /// same words twice are said once; different words are two paragraphs, never one run-on.
    #[test]
    fn a_continuation_that_says_the_same_words_again_is_said_once_and_new_words_are_their_own_paragraph() {
        let mut answer = String::new();
        keep_words(&mut answer, "Handed over cleanly.");
        keep_words(&mut answer, "Handed over cleanly.");
        let said = assignment::says::answered("Start the Northwind job", &assignment::sanitize_answer(&answer));
        assert_eq!(said.matches("Handed over cleanly.").count(), 1, "{said:?}");
        let mut answer = String::new();
        keep_words(&mut answer, "Mark is on it.");
        keep_words(&mut answer, "");
        keep_words(&mut answer, "Mark handed over; Ann continues.");
        assert_eq!(assignment::sanitize_answer(&answer), "Mark is on it.\n\nMark handed over; Ann continues.");
    }

    /// **Ray's candidate-.7 row 8: the failure card said "cognition protocol:".**
    ///
    /// The exact string off his walk is the first case. The sentence after the label was always
    /// the right sentence — it names what did not happen, in his terms — and the label is
    /// `thiserror`'s `Display` on `CognitionError` (`cognition.rs:19-24`), written for logs.
    #[test]
    fn a_failure_card_names_what_did_not_happen_and_never_the_seam_it_came_from() {
        let card = honest(&CognitionError::Protocol(crate::native::ENGINE_PLUGIN_ABSENT.into()).to_string());
        assert_eq!(card, "The desktop engine plugin did not load.");
        assert!(!card.contains("cognition"), "{card}");
        assert!(!card.contains("protocol:"), "{card}");
        // The other variant that carries a seam label, same treatment.
        assert_eq!(
            honest(&CognitionError::Io("the work connection could not be opened".into()).to_string()),
            "The work connection could not be opened."
        );

        // **RAY'S CANDIDATE-.8 ROW 8, PINNED BY ITS EXACT STRING.** He read
        // `cognition io: Choose a company before saving interview answers.` off the third
        // failure card at 10:45Z. This function landed at `8926fd93` (2026-09-18 09:24:43Z),
        // AFTER the `1.2.0-nightly.20260918.2` binary he walked was built — which is why the
        // label was on his screen and is not on main. So his row is a stale assertion against
        // an older binary on this half, and a live defect only on the truncation half
        // (`assignment.rs`'s `says::continues`). Pinned here rather than argued: the exact
        // sentence he saw, through the funnel it now goes through.
        assert_eq!(
            honest(&CognitionError::Io("Choose a company before saving interview answers.".into()).to_string()),
            "Choose a company before saving interview answers."
        );

        // **Positive control on the log half: the label still EXISTS.** Row 8 asks for it off
        // his timeline, not out of the system — a plugin that did not load is diagnosed from
        // it. `honest` is the only funnel to a sentence he reads, so `Display` is untouched.
        let raw = CognitionError::Protocol(crate::native::ENGINE_PLUGIN_ABSENT.into()).to_string();
        assert!(raw.starts_with("cognition protocol: "), "{raw}");

        // **A colon of his own is never eaten.** Matching is by exact label prefix, not by
        // cutting at the first colon, which a real reason can legitimately contain.
        let his = "the land lock timed out after 300 s: nothing was merged";
        assert_eq!(honest(his), "The land lock timed out after 300 s: nothing was merged.");

        // **The card starts like a sentence, which the stripped label used to do for it.** The
        // reasons behind `Io` are lowercase by construction — it wraps what the OS said.
        assert_eq!(honest("no such file or directory"), "No such file or directory.");
        // A reason that opens on something other than a lowercase letter is left alone.
        assert_eq!(honest("\"claude\" is not on the path"), "\"claude\" is not on the path.");
        assert_eq!(honest("300 s elapsed"), "300 s elapsed.");

        // `PrimingStopped` deliberately carries no seam label: it is already about the job.
        let priming = honest(&CognitionError::PrimingStopped("a stop you pressed".into()).to_string());
        assert_eq!(priming, "Preparation stopped with a stop you pressed.");

        // Unchanged behavior: bounded, flattened, and an empty reason still says so.
        assert_eq!(honest("  \n\t "), "No reason was recorded.");
        assert_eq!(honest("line one\nline two"), "Line one line two.");
        assert!(honest(&"x".repeat(500)).chars().count() <= 201);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{EntityId, PersonId};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[derive(Default)]
    struct Fence {
        stop_seen: AtomicBool,
        shutdowns: AtomicUsize,
        /// **A stop that ends the child, the way the real work lease's does**: `native.rs`'s
        /// `NativeCancelHandle::cancel` kills the process fence when `settle_workers_on_stop`
        /// (every lease with an engine profile), so the turn comes back as the reader's
        /// `NativeError::Closed`, never as the `cancelled` result the default fake returns.
        stop_ends_child: AtomicBool,
        /// Every background command this lease was asked to stop, by task id, in order.
        commands_stopped: Mutex<Vec<String>>,
        /// What the provider answers such a stop with. `None` is `Ended`: the provider
        /// stopped it, which is every test written before finding 11's v2 re-check.
        stop_answer: Mutex<Option<CommandStop>>,
    }
    impl TurnCancel for Fence {
        fn cancel(&self) -> bool {
            self.stop_seen.store(true, Ordering::SeqCst);
            true
        }
        fn shutdown(&self) {
            self.shutdowns.fetch_add(1, Ordering::SeqCst);
            self.cancel();
        }
        fn stop_background_command(&self, task_id: &str) -> CommandStop {
            self.commands_stopped.lock().unwrap().push(task_id.to_string());
            self.stop_answer.lock().unwrap().clone().unwrap_or(CommandStop::Ended)
        }
    }

    /// A work lease that runs a scripted turn and records what it was asked to bind.
    struct WorkLease {
        session: String,
        /// §119: the model the factory's stored setting named when this lease was spawned.
        model: Option<crate::config::ModelChoice>,
        bound: Arc<Mutex<Vec<WorkAssignment>>>,
        revoked: Arc<AtomicUsize>,
        cancel: Arc<Fence>,
        /// How long the scripted turn takes, so a stop can arrive while one is in flight.
        step: std::time::Duration,
        /// What the OBLIGATION says. `None` means it cannot be read at all, which must
        /// never settle anything.
        obligation: Arc<Mutex<Option<crate::cognition::ObligationState>>>,
        /// What this lease reports about its own context consumption, if anything — the
        /// MEASURED half of the watermark, emitted the way the real client emits it
        /// (`MachineryRecord::from_context_usage`, the only constructor for it).
        usage: Arc<Mutex<Option<crate::machinery::ContextUsage>>>,
        /// Every priming payload this lease was handed, so a rotation test can read what
        /// the successor was actually told rather than assume it.
        reprimes: Arc<Mutex<Vec<String>>>,
        /// Every handoff ask the OUTGOING lease was given, and what it answered.
        handoffs: Arc<Mutex<Vec<String>>>,
        handoff_reply: String,
        /// **What this lease says out loud on a WORK turn**, so the §58 answer path has a
        /// back end that actually answers. Empty by default, which is every test written
        /// before today: the real back end's assistant text was only ever counted, so no
        /// fake needed to produce any.
        answer_reply: String,
        /// What this lease's own init frame turned out to say about the three facts a
        /// background assignment needs (`native.rs`'s `work_readiness_after_turn`).
        /// `Some(sentence)` is a lease that came up without one of them and could only say so
        /// once its first turn had brought the frame that reports them.
        readiness: Arc<Mutex<Option<String>>>,
        /// **A turn that FAILS, which no fake here could produce before 2026-09-18.** The
        /// real `native.rs` kills its child on its way out of several of these (the three
        /// grant revocations and the worker-settlement check), so an `Err` from `prompt` is
        /// the one case where the lease left standing is a corpse.
        turn_error: Arc<Mutex<Option<String>>>,
        /// **Every WORK turn's prompt, in order.** `handoffs` already keeps the rotation
        /// asks; this keeps the ones that carry the job, so a test can read what the back
        /// end was actually told at each step rather than assume the constant.
        work_prompts: Arc<Mutex<Vec<String>>>,
        /// What its supervisor's state file would say about its tool commands (reap gap C6).
        commands: Arc<Mutex<Option<crate::lease_commands::CommandReading>>>,
        /// Set by the one test that asks whether a lease is dropped with its back end's lease
        /// lock held; every drop is then recorded as (session, lock held).
        drop_probe: Arc<Mutex<Option<std::sync::Weak<WorkHost>>>>,
        drops: Arc<Mutex<Vec<(String, bool)>>>,
        /// What the provider's own task frames say about background commands. `None` is a
        /// lease that cannot say, which is every test written before 2026-09-27.
        background: Arc<Mutex<Option<Vec<crate::cognition::BackgroundCommand>>>>,
        /// Commands a work turn puts in the background, one entry per turn, in order.
        start_in_turn: Arc<Mutex<VecDeque<Vec<crate::cognition::BackgroundCommand>>>>,
        /// What each work turn says, one per turn, in order; `answer_reply` once they run out.
        replies: Arc<Mutex<VecDeque<String>>>,
        /// **A turn that streams NOTHING** (design C11). Every real turn streams at least one
        /// item, so by default a turn with no words still streams one neutral item; set this
        /// to script the degraded "the host did not get this turn" case.
        silent: Arc<AtomicBool>,
        /// Holds a work turn's first item until the test releases it (design §4.1): the window
        /// between sending his answer and the back end taking it.
        first_item_gate: Arc<StartGate>,
        /// The next work turns end `Ok` with this reason and stream nothing, one per turn:
        /// `refused_before_it_ran` / `discarded_before_it_ran` (`native.rs`'s lifecycle).
        unrun: Arc<Mutex<VecDeque<String>>>,
        /// The next work turns fail before streaming, one per turn (a dead lease, `Closed`).
        fail_next: Arc<Mutex<VecDeque<String>>>,
        /// While set, nobody is signed in: every work turn is refused the way Claude Code
        /// refuses one (measured on 2.1.295, see `HoldCause::SignIn`).
        signed_out: Arc<AtomicBool>,
        /// Fill-first: the account this lease was spawned under, and the shared script.
        account: Option<String>,
        fill: Arc<Mutex<FillFirst>>,
        team: Arc<Mutex<TeamScript>>,
    }

    /// **The team on disk, for the fake factory** (slice 5): what `team_digest` answers, and
    /// what each work turn changes it to, one entry per turn (`None` changes nothing), as the
    /// back end saving a teammate with `richos_work.team` during that turn would.
    #[derive(Default)]
    struct TeamScript {
        /// §119: the stored setting for directing the team, as `lease_models` answers it.
        /// `None` is a factory that cannot say — every test written before round 23.
        model: Option<crate::config::ModelChoice>,
        digest: Option<String>,
        in_turn: VecDeque<Option<String>>,
        /// Rows each work turn appends to a journal while it runs, one entry per turn (`None`
        /// appends nothing): a helper that starts and ends inside the turn.
        journal_in_turn: VecDeque<Option<(std::path::PathBuf, String)>>,
    }

    /// **Fill-first's knobs for the fake back end.** With a quota service set, every lease is
    /// spawned under the account it says is in use (as `main.rs`'s `spawn_work` does), and a
    /// lease on `limited` has its work turns refused the way `native.rs` reports a usage limit.
    #[derive(Default)]
    struct FillFirst {
        quota: Option<Arc<crate::quota::Service>>,
        limited: Option<String>,
        spawned: Vec<String>,
        /// What a lease on each account streams as its own usage reading (the real lease's
        /// `rate_limit_event`, `native.rs`), by account id.
        streamed: std::collections::BTreeMap<String, crate::quota::StreamedReading>,
        /// The account each work turn was sent under, in order.
        turns: Vec<Option<String>>,
        /// How many times a lease's running commands were read: each switch check reads them
        /// (`account_switch_due`), so a test can wait for the host to have checked again.
        command_reads: usize,
    }

    /// Claude Code's own words for a turn it refuses with nobody signed in (`interruption.rs`).
    const NOT_LOGGED_IN: &str = "Not logged in \u{b7} Please run /login";

    /// In a scripted reply: a keep-alive `ping` frame between two deltas.
    const PING: char = '\u{1}';
    /// In a scripted reply: a tool call opening between two pieces of prose.
    const TOOL_CALL: char = '\u{2}';

    /// A scripted reply streamed the way `native.rs` streams one: its prose as text items, and
    /// each [`PING`] or [`TOOL_CALL`] as the machinery that real frame becomes in between.
    fn stream_reply(reply: &str, session: &str, on: &mut dyn FnMut(TurnItem)) {
        let mut piece = String::new();
        for c in reply.chars() {
            let frame = match c {
                PING => serde_json::json!({"type": "stream_event", "event": {"type": "ping"}}),
                TOOL_CALL => serde_json::json!({"type": "stream_event", "event": {"type": "content_block_start",
                    "content_block": {"type": "tool_use", "id": "toolu_1", "name": "Bash", "input": {}}}}),
                _ => {
                    piece.push(c);
                    continue;
                }
            };
            if !piece.is_empty() {
                on(TurnItem::Text { seq: 0, text: &piece });
                piece.clear();
            }
            for record in crate::machinery::MachineryRecord::from_native_event(&frame, session, 0) {
                on(TurnItem::Machinery(record));
            }
        }
        if !piece.is_empty() {
            on(TurnItem::Text { seq: 0, text: &piece });
        }
    }

    /// **Rich's progress notices are their own sentences** (video-finish walks on `ec12621f8`:
    /// "…because the video runs for an hour.The transcription is running now."). One turn says
    /// one notice, starts a tool, and says the next; a keep-alive between two deltas of one
    /// notice breaks nothing.
    #[test]
    fn the_closing_report_replaces_working_notes_and_keepalives_do_not_split_it() {
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
        h.replies.lock().unwrap().push_back(format!(
            "I will transcribe it in the background{PING} because the video runs for an hour.{TOOL_CALL}\
             The transcription{PING} is running now."));
        h.host.start();
        h.host.register(&h.binding, &registration(&h)).unwrap();
        let notices = until_notices(&h, 1);
        assert_eq!(notices[0].text, "The transcription is running now.");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    impl Drop for WorkLease {
        fn drop(&mut self) {
            if let Some(host) = self.drop_probe.lock().unwrap().as_ref().and_then(std::sync::Weak::upgrade) {
                let held = host.lease_locked("thread-one");
                self.drops.lock().unwrap().push((self.session.clone(), held));
            }
        }
    }

    impl Cognition for WorkLease {
        fn session_id(&self) -> &str {
            &self.session
        }
        fn lease_model(&self) -> Option<crate::config::ModelChoice> {
            self.model
        }
        fn running_commands(&self) -> Option<crate::lease_commands::CommandReading> {
            self.fill.lock().unwrap().command_reads += 1;
            *self.commands.lock().unwrap()
        }
        fn background_commands(&self) -> Option<Vec<crate::cognition::BackgroundCommand>> {
            self.background.lock().unwrap().clone()
        }
        fn reprime(&mut self, priming: &str, _o: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
            self.reprimes.lock().unwrap().push(priming.to_string());
            Ok(())
        }
        fn prompt_context_only(
            &mut self, text: &str, on: &mut dyn FnMut(TurnItem),
        ) -> Result<String, CognitionError> {
            self.handoffs.lock().unwrap().push(text.to_string());
            on(TurnItem::Text { seq: 0, text: &self.handoff_reply });
            Ok("end_turn".to_string())
        }
        fn account(&self) -> Option<&str> {
            self.account.as_deref()
        }
        fn streamed_usage(&self) -> Option<crate::quota::StreamedReading> {
            self.account.as_ref().and_then(|account| self.fill.lock().unwrap().streamed.get(account).cloned())
        }
        fn prompt(&mut self, _text: &str, _on: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
            self.work_prompts.lock().unwrap().push(_text.to_string());
            self.fill.lock().unwrap().turns.push(self.account.clone());
            if self.signed_out.load(Ordering::SeqCst) {
                // The `<synthetic>` assistant text, then the result `native.rs` turns into this
                // error (`is_error: true`, no `errors` key, so the `result` string is the detail).
                _on(TurnItem::Text { seq: 0, text: NOT_LOGGED_IN });
                return Err(CognitionError::Protocol(format!("\"{NOT_LOGGED_IN}\"")));
            }
            if self.account.is_some() && self.account == self.fill.lock().unwrap().limited {
                return Err(CognitionError::Protocol(crate::claude_accounts::usage_limit_error(
                    None, "[\"You've hit your session limit\"]")));
            }
            if let Some(why) = self.turn_error.lock().unwrap().clone() {
                return Err(CognitionError::Protocol(why));
            }
            if let Some(why) = self.fail_next.lock().unwrap().pop_front() {
                return Err(CognitionError::Io(why));
            }
            if let Some(reason) = self.unrun.lock().unwrap().pop_front() {
                return Ok(reason);
            }
            let reply = self.replies.lock().unwrap().pop_front().unwrap_or_else(|| self.answer_reply.clone());
            {
                let mut team = self.team.lock().unwrap();
                if let Some(Some(saved)) = team.in_turn.pop_front() {
                    team.digest = Some(saved);
                }
                if let Some(Some((path, rows))) = team.journal_in_turn.pop_front() {
                    let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
                    std::io::Write::write_all(&mut file, rows.as_bytes()).unwrap();
                }
            }
            if !self.silent.load(Ordering::SeqCst) {
                self.first_item_gate.wait_until_open();
            }
            // A stop that arrived before the first item: nothing of the turn was streamed.
            if self.cancel.stop_seen.load(Ordering::SeqCst) {
                if self.cancel.stop_ends_child.load(Ordering::SeqCst) {
                    return Err(CognitionError::Io("claude channel closed (child exited?)".into()));
                }
                return Ok(crate::native::STOP_REASON_CANCELLED.to_string());
            }
            if !reply.is_empty() {
                stream_reply(&reply, &self.session, _on);
            } else if !self.silent.load(Ordering::SeqCst) {
                // The neutral item every real turn streams (C11): no words, nothing counted.
                _on(TurnItem::Text { seq: 0, text: "" });
            }
            if let Some(started) = self.start_in_turn.lock().unwrap().pop_front() {
                // A command already known is this turn REPORTING ITS ENDING (the provider folding
                // a `task_notification` into the turn); a new one is a command it started.
                let mut reading = self.background.lock().unwrap();
                let known = reading.get_or_insert_with(Vec::new);
                for command in started {
                    match known.iter_mut().find(|c| c.task_id == command.task_id) {
                        Some(existing) => *existing = command,
                        None => known.push(command),
                    }
                }
            }
            if let Some(usage) = *self.usage.lock().unwrap() {
                _on(TurnItem::Machinery(crate::machinery::MachineryRecord::from_context_usage(
                    usage.used,
                    usage.size,
                    &serde_json::json!({"input_tokens": usage.used}),
                    &self.session,
                    0,
                )));
            }
            let deadline = std::time::Instant::now() + self.step;
            while std::time::Instant::now() < deadline {
                if self.cancel.stop_seen.load(Ordering::SeqCst) {
                    if self.cancel.stop_ends_child.load(Ordering::SeqCst) {
                        // What `NativeError::Closed` becomes on its way here (`native.rs`,
                        // `impl From<NativeError> for CognitionError`).
                        return Err(CognitionError::Io("claude channel closed (child exited?)".into()));
                    }
                    return Ok(crate::native::STOP_REASON_CANCELLED.to_string());
                }
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            if self.cancel.stop_seen.load(Ordering::SeqCst) {
                return Ok(crate::native::STOP_REASON_CANCELLED.to_string());
            }
            Ok("end_turn".to_string())
        }
        fn cancel_handle(&self) -> Option<Arc<dyn TurnCancel>> {
            Some(self.cancel.clone())
        }
        fn bind_work_assignment(&mut self, assignment: &WorkAssignment) -> Result<(), CognitionError> {
            // **It accepts the binding even when `readiness` says the lease is not equipped**,
            // and that is the fix under test rather than a lax fake: at the binding the child
            // has not sent its init frame yet, so nothing is known to refuse on. The real
            // lease behaves identically (`native.rs`'s `bind_work_assignment`).
            self.bound.lock().unwrap().push(assignment.clone());
            Ok(())
        }
        fn work_readiness_after_turn(&self) -> Result<(), CognitionError> {
            match self.readiness.lock().unwrap().clone() {
                Some(sentence) => Err(CognitionError::Protocol(sentence)),
                None => Ok(()),
            }
        }
        fn revoke_work_assignment(&mut self) -> Result<(), CognitionError> {
            self.revoked.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn obligation_state(&self, _obligation: &str) -> Result<crate::cognition::ObligationState, CognitionError> {
            match *self.obligation.lock().unwrap() {
                Some(state) => Ok(state),
                None => Err(CognitionError::Protocol("the obligation could not be read".into())),
            }
        }
    }

    /// **A hold on the work's FIRST OBSERVABLE STEP, so "registration does not wait for the
    /// work" can be stated as a fact instead of as a duration.**
    ///
    /// Opening a back-end lease (`ensure_lease` → `spawn_work`) is the first thing that
    /// happens to an assignment after it is queued, and it happens on the runner's thread.
    /// Hold it shut and there are only two possible outcomes: `register` returns while the
    /// work provably has not begun — the property under test — or `register` was preparing
    /// the work on the CALLER's thread, in which case it is now waiting on this gate and
    /// cannot return. No clock distinguishes those; the gate does.
    ///
    /// **Open by default**, so every other test in this file is untouched and never waits.
    ///
    /// The one duration in here is not part of the passing path. `forced` exists so a build
    /// with the defect FAILS with a sentence rather than hanging a suite forever: after a
    /// deliberately generous wait the gate lets go by itself and remembers that it had to.
    /// The green path opens it explicitly, in microseconds, and never reaches that branch.
    #[derive(Default)]
    struct StartGate {
        open: Mutex<bool>,
        changed: Condvar,
        forced: AtomicBool,
    }

    impl StartGate {
        fn open_now() -> Arc<Self> {
            Arc::new(StartGate { open: Mutex::new(true), changed: Condvar::new(), forced: AtomicBool::new(false) })
        }
        fn shut(&self) {
            *self.open.lock().unwrap() = false;
        }
        fn release(&self) {
            *self.open.lock().unwrap() = true;
            self.changed.notify_all();
        }
        /// Called on whatever thread is about to take the work's first step.
        fn wait_until_open(&self) {
            let mut open = self.open.lock().unwrap();
            let deadline = std::time::Duration::from_secs(30);
            while !*open {
                let (guard, timed_out) = self.changed.wait_timeout(open, deadline).unwrap();
                open = guard;
                if timed_out.timed_out() && !*open {
                    self.forced.store(true, Ordering::SeqCst);
                    *open = true;
                }
            }
        }
        /// True only if the gate had to let go of itself — which, in the test below, means
        /// the caller's thread was the one waiting at it.
        fn had_to_force(&self) -> bool {
            self.forced.load(Ordering::SeqCst)
        }
    }

    struct WorkFactory {
        bound: Arc<Mutex<Vec<WorkAssignment>>>,
        revoked: Arc<AtomicUsize>,
        fence: Arc<Fence>,
        step: std::time::Duration,
        obligation: Arc<Mutex<Option<crate::cognition::ObligationState>>>,
        usage: Arc<Mutex<Option<crate::machinery::ContextUsage>>>,
        reprimes: Arc<Mutex<Vec<String>>>,
        handoffs: Arc<Mutex<Vec<String>>>,
        handoff_reply: Arc<Mutex<String>>,
        answer_reply: Arc<Mutex<String>>,
        readiness: Arc<Mutex<Option<String>>>,
        turn_error: Arc<Mutex<Option<String>>>,
        work_prompts: Arc<Mutex<Vec<String>>>,
        /// The next `spawn_work` fails once, so the "a successor that cannot be opened
        /// leaves the incumbent working" branch has a way to happen.
        refuse_next: Arc<AtomicBool>,
        /// While set, EVERY `spawn_work` fails (a provider that stays down); `attempts` counts
        /// each one, success or not.
        broken: Arc<AtomicBool>,
        attempts: Arc<AtomicUsize>,
        /// How many back-end leases this host has asked for. The CEO's §51 shape is ONE,
        /// standing, for every assignment — so this is an observable rather than a counter
        /// nobody reads.
        spawns: Arc<AtomicUsize>,
        /// Open everywhere but in the one test that shuts it. See `StartGate`.
        start_gate: Arc<StartGate>,
        commands: Arc<Mutex<Option<crate::lease_commands::CommandReading>>>,
        drop_probe: Arc<Mutex<Option<std::sync::Weak<WorkHost>>>>,
        drops: Arc<Mutex<Vec<(String, bool)>>>,
        background: Arc<Mutex<Option<Vec<crate::cognition::BackgroundCommand>>>>,
        start_in_turn: Arc<Mutex<VecDeque<Vec<crate::cognition::BackgroundCommand>>>>,
        replies: Arc<Mutex<VecDeque<String>>>,
        silent: Arc<AtomicBool>,
        first_item_gate: Arc<StartGate>,
        unrun: Arc<Mutex<VecDeque<String>>>,
        fail_next: Arc<Mutex<VecDeque<String>>>,
        signed_out: Arc<AtomicBool>,
        fill: Arc<Mutex<FillFirst>>,
        team: Arc<Mutex<TeamScript>>,
    }

    impl LeaseFactory for WorkFactory {
        fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
            Err(CognitionError::Protocol("a conversation lease is not what this factory is for".into()))
        }
        fn team_digest(&self) -> Option<String> {
            self.team.lock().unwrap().digest.clone()
        }
        fn lease_models(&self) -> Option<crate::config::LeaseModels> {
            let stu = self.team.lock().unwrap().model?;
            Some(crate::config::LeaseModels { rich: crate::config::ModelChoice::Opus, stu })
        }
        fn spawn_work(&self, _binding: &ThreadBinding) -> Result<Box<dyn Cognition>, CognitionError> {
            // BEFORE the counter and before the refusal, because this is the point the one
            // test that shuts the gate needs to be able to say has not been reached.
            self.start_gate.wait_until_open();
            self.attempts.fetch_add(1, Ordering::SeqCst);
            if self.broken.load(Ordering::SeqCst) {
                return Err(CognitionError::Protocol("the provider is down".into()));
            }
            if self.refuse_next.swap(false, Ordering::SeqCst) {
                return Err(CognitionError::Protocol("no second connection could be opened".into()));
            }
            let n = self.spawns.fetch_add(1, Ordering::SeqCst);
            let account = {
                let mut fill = self.fill.lock().unwrap();
                let account = fill.quota.as_ref().map(|quota| quota.lease_account().id);
                if let Some(account) = &account { fill.spawned.push(account.clone()); }
                account
            };
            Ok(Box::new(WorkLease {
                model: self.team.lock().unwrap().model,
                account,
                fill: self.fill.clone(),
                // The FIRST back end keeps the name every other test in this file already
                // uses; a rotated successor is visibly a different session, which is what
                // makes a rotation observable rather than inferred.
                session: if n == 0 { "work-session-one".to_string() } else { format!("work-session-rotated-{n}") },
                bound: self.bound.clone(),
                revoked: self.revoked.clone(),
                cancel: self.fence.clone(),
                step: self.step,
                obligation: self.obligation.clone(),
                usage: self.usage.clone(),
                reprimes: self.reprimes.clone(),
                handoffs: self.handoffs.clone(),
                handoff_reply: self.handoff_reply.lock().unwrap().clone(),
                answer_reply: self.answer_reply.lock().unwrap().clone(),
                readiness: self.readiness.clone(),
                turn_error: self.turn_error.clone(),
                work_prompts: self.work_prompts.clone(),
                commands: self.commands.clone(),
                drop_probe: self.drop_probe.clone(),
                drops: self.drops.clone(),
                background: self.background.clone(),
                start_in_turn: self.start_in_turn.clone(),
                replies: self.replies.clone(),
                silent: self.silent.clone(),
                first_item_gate: self.first_item_gate.clone(),
                unrun: self.unrun.clone(),
                fail_next: self.fail_next.clone(),
                signed_out: self.signed_out.clone(),
                team: self.team.clone(),
            }))
        }
    }

    struct Recorder(Mutex<Vec<(String, PendingNotice)>>);
    impl WorkNotifier for Recorder {
        fn raised(&self, thread_id: &str, notice: &PendingNotice) {
            self.0.lock().unwrap().push((thread_id.to_string(), notice.clone()));
        }
    }

    struct Harness {
        root: PathBuf,
        state: PathBuf,
        host: Arc<WorkHost>,
        bound: Arc<Mutex<Vec<WorkAssignment>>>,
        revoked: Arc<AtomicUsize>,
        fence: Arc<Fence>,
        notices: Arc<Recorder>,
        binding: ThreadBinding,
        obligation: Arc<Mutex<Option<crate::cognition::ObligationState>>>,
        desk: Arc<crate::permissions::PermissionDesk>,
        spawns: Arc<AtomicUsize>,
        usage: Arc<Mutex<Option<crate::machinery::ContextUsage>>>,
        reprimes: Arc<Mutex<Vec<String>>>,
        handoffs: Arc<Mutex<Vec<String>>>,
        handoff_reply: Arc<Mutex<String>>,
        answer_reply: Arc<Mutex<String>>,
        refuse_next: Arc<AtomicBool>,
        broken: Arc<AtomicBool>,
        attempts: Arc<AtomicUsize>,
        readiness: Arc<Mutex<Option<String>>>,
        turn_error: Arc<Mutex<Option<String>>>,
        work_prompts: Arc<Mutex<Vec<String>>>,
        start_gate: Arc<StartGate>,
        commands: Arc<Mutex<Option<crate::lease_commands::CommandReading>>>,
        drop_probe: Arc<Mutex<Option<std::sync::Weak<WorkHost>>>>,
        drops: Arc<Mutex<Vec<(String, bool)>>>,
        background: Arc<Mutex<Option<Vec<crate::cognition::BackgroundCommand>>>>,
        start_in_turn: Arc<Mutex<VecDeque<Vec<crate::cognition::BackgroundCommand>>>>,
        replies: Arc<Mutex<VecDeque<String>>>,
        silent: Arc<AtomicBool>,
        first_item_gate: Arc<StartGate>,
        unrun: Arc<Mutex<VecDeque<String>>>,
        fail_next: Arc<Mutex<VecDeque<String>>>,
        signed_out: Arc<AtomicBool>,
        fill: Arc<Mutex<FillFirst>>,
        team: Arc<Mutex<TeamScript>>,
    }

    #[test]
    fn a_correction_reaches_stus_live_input_while_his_job_owns_the_lease() {
        type Delivery=(String,String,Box<dyn FnOnce()+Send>);
        struct Input(std::sync::mpsc::Sender<Delivery>);
        impl TurnCancel for Input {
            fn cancel(&self)->bool {panic!("a correction must not cancel the job")}
            fn send_input(&self,id:&str,text:&str,taken:Box<dyn FnOnce()+Send>)->Result<bool,String> {
                self.0.send((id.into(),text.into(),taken)).unwrap();Ok(true)
            }
        }
        let h=harness(0);
        let scope=h.host.message_scope(&h.binding);
        let path=h.state.join("rich-status.json");
        crate::session_tools::bind(&path,&scope,"native-rich","rich",None).unwrap();
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"Original request"})).unwrap();
        let original=crate::session_tools::pending(&scope,"stu").unwrap().unwrap().1;
        let backend=Backend::new();
        let (tx,rx)=std::sync::mpsc::channel();
        {
            let mut inner=backend.inner.lock().unwrap();
            inner.processing=true;inner.binding=Some(h.binding.clone());
            inner.peer_active_through=Some(original);
            inner.peer_input=Some(Arc::new(Input(tx)));inner.lease_session=Some("same-stu".into());
        }
        h.host.backends.lock().unwrap().insert(h.binding.thread_id().into(),backend.clone());
        let running=backend.lease.lock().unwrap();
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"Correction: keep the existing release date"})).unwrap();
        h.host.wake_messages(&h.binding);
        let (_,text,taken)=rx.recv_timeout(std::time::Duration::from_secs(5)).expect("hang guard: correction must reach input while the job still holds the lease");
        assert!(text.contains("keep the existing release date"));
        assert_eq!(crate::session_tools::pending(&scope,"stu").unwrap().unwrap().1,original,"writing is not consumption");
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"Then retain the current team"})).unwrap();
        taken();
        let (_,text,taken)=rx.recv_timeout(std::time::Duration::from_secs(5)).expect("hang guard: consumption forwards the next durable message");
        assert!(text.contains("retain the current team"));taken();
        crate::session_tools::acknowledge(&scope,"stu",original).unwrap();
        assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_none(),"finishing the original turn must not rewind the consumed cursor");
        assert!(backend.inner.lock().unwrap().processing,"the running job was not ended to deliver input");
        assert!(h.host.open_assignments().unwrap().is_empty());
        use sha2::Digest;
        let instruction=crate::ecs::UserInstruction::with_text("ledger:thread-one:turn-7".into(),
            format!("{:x}",sha2::Sha256::digest("landing the three branches")),Some("landing the three branches"));
        crate::session_tools::bind(&path,&scope,"native-rich","rich",Some(instruction)).unwrap();
        crate::session_tools::delegate(&path,&scope,serde_json::json!({"text":"New execution request"})).unwrap();
        h.host.wake_messages(&h.binding);
        assert!(matches!(rx.try_recv(),Err(std::sync::mpsc::TryRecvError::Empty)),"new work must wait for a prepared turn, not replace current authority through live input");
        let queued=crate::session_tools::pending(&scope,"stu").unwrap().unwrap().1;
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"Correction to the current job: do not change the date"})).unwrap();
        h.host.wake_messages(&h.binding);
        let (_,text,taken)=rx.recv_timeout(std::time::Duration::from_secs(5)).expect("hang guard: a queued new job must not block a quick correction to current work");
        assert!(text.contains("do not change the date"));taken();
        assert_eq!(crate::session_tools::pending(&scope,"stu").unwrap().unwrap().1,queued,"the unprepared job was lost behind the correction receipt");
        crate::session_tools::acknowledge(&scope,"stu",queued).unwrap();
        assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_none(),"an accepted live correction was replayed after restart");
        drop(running);h.host.shutdown();std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn ordinary_peer_input_runs_without_an_assignment_and_is_not_replayed_after_ack() {
        let h=harness(0);
        let scope=h.host.message_scope(&h.binding);
        let path=h.state.join("rich-status.json");
        crate::session_tools::bind(&path,&scope,"native-rich","rich",None).unwrap();
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"Inspect the current evidence"})).unwrap();
        h.host.wake_messages(&h.binding);
        assert!(h.host.wait_for_completed(1,std::time::Duration::from_secs(10)),"hang guard: wait for the receiving turn to complete");
        assert!(h.bound.lock().unwrap().is_empty(),"communication manufactured an assignment");
        assert!(h.host.open_assignments().unwrap().is_empty());
        assert_eq!(h.work_prompts.lock().unwrap().len(),1);
        assert!(h.work_prompts.lock().unwrap()[0].contains("Inspect the current evidence"));
        assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_none());
        h.host.shutdown();
    }

    #[test]
    fn a_standing_stu_question_answer_reaches_his_turn_without_an_assignment() {
        let h=harness(0);
        let answer=crate::questions::Delivery { obligation_id:None,id:"standing-answer".into(),entity_id:"depot".into(),
            thread_id:"thread-one".into(),asker:"stu".into(),set_id:Some("standing-choice".into()),
            text:"Use the existing launch date".into(),receipt:None};
        h.host.queue_question_answer(&h.binding,&answer).unwrap();
        assert!(h.host.wait_for_completed(1,std::time::Duration::from_secs(10)),"hang guard: wait for the answer turn");
        assert!(h.host.open_assignments().unwrap().is_empty());
        assert!(h.bound.lock().unwrap().is_empty());
        assert_eq!(h.work_prompts.lock().unwrap().len(),1);
        assert!(h.work_prompts.lock().unwrap()[0].contains(&answer.text));
        assert!(crate::question_work::pending(&h.state).unwrap().is_empty());
        let wrong=ThreadBinding::new(PersonId::default_ceo(),EntityId::parse("depot").unwrap(),"thread-two",1);
        assert!(h.host.queue_question_answer(&wrong,&answer).is_err());
        h.host.shutdown();
        // A's answer retains A's authority even when B is the latest session input.
        let a=assignment::register(&h.state,&registration(&h)).unwrap();
        let b=assignment::register(&h.state,&Registration {obligation_id:"work-B".into(),
            instruction_ledger_ref:"ledger:thread-one:turn-B".into(),instruction_sha256:"b".repeat(64),..registration(&h)}).unwrap();
        let record=assignment::read(&h.state,"depot","thread-one",&a.id).unwrap();
        let selected=crate::questions::Delivery {obligation_id:Some(record.obligation_id.clone()),..answer.clone()};
        assert_eq!(h.host.question_answer_instruction(&selected).unwrap().unwrap().ledger_ref,record.instruction_ledger_ref);
        let b_record=assignment::read(&h.state,"depot","thread-one",&b.id).unwrap();
        let status=h.state.join("stu-status.json");
        crate::session_tools::bind(&status,&h.host.message_scope(&h.binding),"question-stu","stu",Some(crate::ecs::UserInstruction {
            ledger_ref:b_record.instruction_ledger_ref,sha256:b_record.instruction_sha256,text:None})).unwrap();
        let qscope=h.state.join("questions-scope.json");
        crate::question_tools::write_scope(&qscope,&crate::question_tools::Scope {context:crate::questions::AskScope {
            obligation_id:None,root:h.state.clone(),entity_id:"depot".into(),thread_id:"thread-one".into(),turn_id:"peer:latest-B".into(),
            asker:"stu".into(),session_id:"question-stu".into(),engine:None,entity_root:None,app_run:None},
            actions_allowed:true,answer_method:"typed".into(),surface:"mac".into()}).unwrap();
        let args=serde_json::json!({"questions":[{"text":"When should the release ship?", "options":[
            {"label":"Ship today","description":"Earlier fixes"},{"label":"Ship tomorrow","description":"More checks"}]}]});
        let current=crate::question_tools::call(&qscope,"ask",args.clone()).unwrap();
        assert_eq!(current["questions"][0]["obligation_id"],"work-B");
        let mut for_a=args.clone();for_a["obligation_id"]=record.obligation_id.clone().into();
        let owned=crate::question_tools::call(&qscope,"ask",for_a.clone()).unwrap();
        assert_eq!(owned["questions"][0]["obligation_id"],record.obligation_id);
        assignment::record_user_stop(&h.state,"depot","thread-one",&b.id).unwrap();
        assert!(crate::question_tools::call(&qscope,"ask",args).is_err());
        assert!(crate::question_tools::call(&qscope,"ask",for_a).is_ok(),"B's Stop revoked questions about A");
        assignment::record_user_stop(&h.state,"depot","thread-one",&a.id).unwrap();
        assert!(h.host.question_answer_instruction(&selected).is_err(),"stale answer reauthorized a stopped job");
        std::fs::remove_dir_all(h.root).unwrap();
    }

    fn harness(step_ms: u64) -> Harness {
        let root = std::env::temp_dir().join(format!("work-host-{}", uuid::Uuid::new_v4()));
        let state = root.join("engine-state");
        std::fs::create_dir_all(&state).unwrap();
        let lines = ["thread-one", "thread-two"].map(|thread| serde_json::json!({
            "event":"PromptReceived", "turn_id":if thread == "thread-one" { "turn-7" } else { "turn-8" }, "thread_id":thread,
            "entity_id":"depot", "source":"text", "text":"landing the three branches"
        }).to_string()).join("\n") + "\n";
        std::fs::write(root.join("conversation-ledger.jsonl"), lines).unwrap();
        let binding =
            ThreadBinding::new(PersonId::default_ceo(), EntityId::parse("depot").unwrap(), "thread-one", 1);
        let bound = Arc::new(Mutex::new(Vec::new()));
        let revoked = Arc::new(AtomicUsize::new(0));
        let fence = Arc::new(Fence::default());
        let notices = Arc::new(Recorder(Mutex::new(Vec::new())));
        let obligation = Arc::new(Mutex::new(None));
        let spawns = Arc::new(AtomicUsize::new(0));
        let usage = Arc::new(Mutex::new(None));
        let reprimes = Arc::new(Mutex::new(Vec::new()));
        let handoffs = Arc::new(Mutex::new(Vec::new()));
        let handoff_reply = Arc::new(Mutex::new(String::new()));
        let answer_reply = Arc::new(Mutex::new(String::new()));
        let refuse_next = Arc::new(AtomicBool::new(false));
        let readiness = Arc::new(Mutex::new(None));
        let turn_error = Arc::new(Mutex::new(None));
        let work_prompts = Arc::new(Mutex::new(Vec::new()));
        let start_gate = StartGate::open_now();
        let commands = Arc::new(Mutex::new(None));
        let drop_probe = Arc::new(Mutex::new(None));
        let drops = Arc::new(Mutex::new(Vec::new()));
        let background = Arc::new(Mutex::new(None));
        let start_in_turn = Arc::new(Mutex::new(VecDeque::new()));
        let replies = Arc::new(Mutex::new(VecDeque::new()));
        let silent = Arc::new(AtomicBool::new(false));
        let first_item_gate = StartGate::open_now();
        let unrun = Arc::new(Mutex::new(VecDeque::new()));
        let fail_next = Arc::new(Mutex::new(VecDeque::new()));
        let signed_out = Arc::new(AtomicBool::new(false));
        let broken = Arc::new(AtomicBool::new(false));
        let attempts = Arc::new(AtomicUsize::new(0));
        let fill = Arc::new(Mutex::new(FillFirst::default()));
        let team = Arc::new(Mutex::new(TeamScript::default()));
        let factory = WorkFactory {
            signed_out: signed_out.clone(),
            team: team.clone(),
            fill: fill.clone(),
            broken: broken.clone(),
            attempts: attempts.clone(),
            silent: silent.clone(),
            first_item_gate: first_item_gate.clone(),
            unrun: unrun.clone(),
            fail_next: fail_next.clone(),
            background: background.clone(),
            start_in_turn: start_in_turn.clone(),
            replies: replies.clone(),
            commands: commands.clone(),
            drop_probe: drop_probe.clone(),
            drops: drops.clone(),
            start_gate: start_gate.clone(),
            bound: bound.clone(),
            revoked: revoked.clone(),
            fence: fence.clone(),
            step: std::time::Duration::from_millis(step_ms),
            obligation: obligation.clone(),
            spawns: spawns.clone(),
            usage: usage.clone(),
            reprimes: reprimes.clone(),
            handoffs: handoffs.clone(),
            handoff_reply: handoff_reply.clone(),
            answer_reply: answer_reply.clone(),
            refuse_next: refuse_next.clone(),
            readiness: readiness.clone(),
            turn_error: turn_error.clone(),
            work_prompts: work_prompts.clone(),
        };
        let desk = Arc::new(crate::permissions::PermissionDesk::default());
        let host = WorkHost::new(&state, Box::new(factory), notices.clone(), Arc::clone(&desk));
        Harness { root, state, host, bound, revoked, fence, notices, binding, obligation, desk, spawns,
            usage, reprimes, handoffs, handoff_reply, answer_reply, refuse_next, readiness, turn_error,
            work_prompts, start_gate, commands, drop_probe, drops, background, start_in_turn, replies,
            silent, first_item_gate, unrun, fail_next, signed_out, broken, attempts, fill, team }
    }

    /// **A second lease factory over the SAME scripted state as `h`'s** — every knob and every
    /// record shared. A test builds a second host with it: a relaunch over the same engine state
    /// (design §4.1 tests 2 and 3), or a host with a different notifier.
    fn factory_over(h: &Harness, step_ms: u64) -> WorkFactory {
        WorkFactory {
            signed_out: h.signed_out.clone(),
            team: h.team.clone(),
            fill: h.fill.clone(),
            broken: h.broken.clone(),
            attempts: h.attempts.clone(),
            start_gate: h.start_gate.clone(),
            bound: h.bound.clone(),
            revoked: h.revoked.clone(),
            fence: h.fence.clone(),
            step: std::time::Duration::from_millis(step_ms),
            obligation: h.obligation.clone(),
            spawns: h.spawns.clone(),
            usage: h.usage.clone(),
            reprimes: h.reprimes.clone(),
            handoffs: h.handoffs.clone(),
            handoff_reply: h.handoff_reply.clone(),
            answer_reply: h.answer_reply.clone(),
            refuse_next: h.refuse_next.clone(),
            readiness: h.readiness.clone(),
            turn_error: h.turn_error.clone(),
            work_prompts: h.work_prompts.clone(),
            commands: h.commands.clone(),
            drop_probe: h.drop_probe.clone(),
            drops: h.drops.clone(),
            background: h.background.clone(),
            start_in_turn: h.start_in_turn.clone(),
            replies: h.replies.clone(),
            silent: h.silent.clone(),
            first_item_gate: h.first_item_gate.clone(),
            unrun: h.unrun.clone(),
            fail_next: h.fail_next.clone(),
        }
    }

    fn registration(harness: &Harness) -> Registration {
        Registration {
            entity_id: harness.binding.entity_id().to_string(),
            thread_id: harness.binding.thread_id().to_string(),
            obligation_id: "obligation-7".into(),
            instruction_ledger_ref: "ledger:thread-one:turn-7".into(),
            instruction_sha256: { use sha2::Digest; format!("{:x}", sha2::Sha256::digest(b"landing the three branches")) },
            title: "landing the three branches".into(),
            repositories: vec!["/fictional/project".into()],
            needs_screen: false,
        }
    }

    /// An operator intake that records what it was handed.
    #[derive(Default)]
    struct TakenBy {
        taken: Mutex<Vec<String>>,
        stopped: Mutex<Vec<String>>,
    }
    impl OperatorIntake for TakenBy {
        fn take(&self, record: Assignment) {
            self.taken.lock().unwrap().push(record.id);
        }
        fn stop(&self, _entity: &str, _thread: &str, id: &str) -> Result<(), String> {
            self.stopped.lock().unwrap().push(id.to_string());
            Ok(())
        }
    }

    /// **On an operator install every assignment is his team's and none opens a work lease**
    /// (the operator-client record's §7 item 1). The adopted assignment goes to the intake
    /// once, however often the boundary sweeps; its Stop goes there too; the register no
    /// longer speaks for his team in the update gate. The same host with no intake is the
    /// product's: the positive control, in the same test, where the lease IS opened.
    #[test]
    fn an_operator_install_hands_every_assignment_to_his_team_and_opens_no_work_lease() {
        let h = harness(0);
        let intake = Arc::new(TakenBy::default());
        h.host.set_operator(intake.clone());
        let receipt = assignment::register_kind(&h.state, &registration(&h), assignment::AssignmentKind::Task).unwrap();
        assert_eq!(h.host.adopt_registered(&h.binding), 1);
        assert_eq!(h.host.adopt_registered(&h.binding), 0, "adopted once");
        assert_eq!(*intake.taken.lock().unwrap(), std::slice::from_ref(&receipt.id));
        std::thread::sleep(std::time::Duration::from_millis(200));
        assert_eq!(h.spawns.load(Ordering::SeqCst), 0, "no work lease was opened for his team's assignment");
        assert_eq!(h.host.background_work(), crate::work_gate::BackgroundWork::nothing(),
                   "the register does not speak for his team");
        assert_eq!(h.host.stop_assignment("depot", "thread-one", &receipt.id), Ok(()));
        assert_eq!(*intake.stopped.lock().unwrap(), std::slice::from_ref(&receipt.id));

        // Desktop records alone never dispatch. Its peer handoff is tested separately.
        let product = harness(0);
        assignment::register_kind(&product.state, &registration(&product), assignment::AssignmentKind::Task).unwrap();
        assert_eq!(product.host.adopt_registered(&product.binding), 0);
        assert_eq!(product.spawns.load(Ordering::SeqCst), 0, "a record is not an execution channel");
        product.host.shutdown();
    }

    #[test]
    fn background_receives_verbatim_constraints_beyond_the_display_title() {
        use sha2::Digest;
        let h = harness(1);
        let text = format!("{}\nLand only on integration. Run python3 -m unittest. Do not push.",
            "Make the scoped arithmetic change without changing other behavior. ".repeat(5));
        let request = Registration {
            title: text.clone(),
            instruction_sha256: format!("{:x}", sha2::Sha256::digest(text.as_bytes())),
            ..registration(&h)
        };
        let row = serde_json::json!({"event":"PromptReceived", "turn_id":"turn-7",
            "thread_id":"thread-one", "entity_id":"depot", "source":"text", "text":text});
        std::fs::write(h.root.join("conversation-ledger.jsonl"), row.to_string()+"\n").unwrap();
        witnessed(&h.state, "work-session-one");
        h.host.start();
        let receipt = h.host.register(&h.binding, &request).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        let prompts = h.work_prompts.lock().unwrap();
        assert_eq!(prompts.len(), 1);
        let input: serde_json::Value = serde_json::from_str(&prompts[0]).unwrap();
        assert_eq!(input["assignment"]["user_request"], text, "the actual work lease lost the original constraints");
        assert_eq!(input["assignment"]["message"], text, "Rich's full handoff was shortened");
        // Second review, slice 4: the same verified words reach the lease's scope, beside their
        // hash, so a reviewer it prepares is briefed with them verbatim.
        let bound = h.bound.lock().unwrap().clone();
        assert_eq!(bound.len(), 1);
        assert_eq!(bound[0].instruction_text.as_deref(), Some(text.as_str()));
        let saved = assignment::read(&h.state,"depot","thread-one",&receipt.id).unwrap();
        assert!(!saved.title.contains("integration"), "positive control: title must be truncated");
        drop(prompts);
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn an_ordinary_assignment_receives_its_company_facts_without_title_routing() {
        let h = harness(1);
        let central = h.root.join("central");
        let entity = crate::entity::EntityId::parse("depot").unwrap();
        crate::company::save_company_notes(&central, &entity,
            "The business serves independent garden centers. Preserve the existing subscription prices.",
            crate::company::InterviewProgress::Complete).unwrap();
        let other = crate::entity::EntityId::parse("other").unwrap();
        crate::company::save_company_notes(&central, &other, "UNRELATED COMPANY FACTS",
            crate::company::InterviewProgress::Complete).unwrap();
        h.host.set_central_root(central);
        witnessed(&h.state, "work-session-one");
        h.host.start();
        h.host.register(&h.binding, &registration(&h)).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        let prompts = h.work_prompts.lock().unwrap();
        assert_eq!(prompts.len(), 1);
        assert!(prompts[0].contains("independent garden centers"));
        assert!(prompts[0].contains("Preserve the existing subscription prices"));
        assert!(!prompts[0].contains("UNRELATED COMPANY FACTS"));
        drop(prompts);
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn question_answer_recovers_before_wake_and_reaches_only_its_backend() {
        let h=harness(1);
        let receipt=assignment::register(&h.state,&registration(&h)).unwrap();
        let record=assignment::read(&h.state,"depot","thread-one",&receipt.id).unwrap();
        let d=crate::questions::Delivery{ obligation_id:None,id:"saved-before-crash".into(),entity_id:"depot".into(),thread_id:"thread-one".into(),asker:record.obligation_id.clone(),set_id:Some("release-set".into()),text:"When should the release ship? You answered: Ship tomorrow (phone_tap on phone)".into(),receipt:None};
        crate::question_work::enqueue(&h.state,&d).unwrap();
        // Both durable inputs exist, with no process-local schedule after the crash.
        h.host.queue_question_answer(&h.binding,&d).unwrap();
        h.host.start();
        assert!(h.host.wait_for_completed(1,std::time::Duration::from_secs(10)));
        let prompts=h.work_prompts.lock().unwrap();
        assert_eq!(prompts.iter().filter(|p|p.contains(&d.text)).count(),1);
        drop(prompts);
        assert!(crate::question_work::pending(&h.state).unwrap().is_empty());
        h.host.shutdown();std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn open_question_survives_task_report_while_independent_task_finishes() {
        use crate::questions::{AskScope, OptionInput, QuestionInput, State, Store};
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
        *h.answer_reply.lock().unwrap() = "I checked the release notes.".into();
        let request = registration(&h);
        let store = Store::new(&h.state);
        let question = store.ask(&AskScope { obligation_id:None,
            root: h.state.clone(),
            entity_id: "depot".into(),
            thread_id: "thread-one".into(),
            turn_id: "turn-7".into(),
            asker: request.obligation_id.clone(),
            session_id: "work-session-one".into(),
            engine: None,
            entity_root: None,
            app_run: None,
        }, vec![QuestionInput {
            text: "When should the release ship?".into(),
            options: vec![
                OptionInput { label: "Ship today".into(), description: "Deliver the fixes sooner.".into() },
                OptionInput { label: "Ship tomorrow".into(), description: "Allow another day for testing.".into() },
            ],
            multiple: false,
            free_answer: true,
            recommended: Some(1),
        }]).unwrap().remove(0);
        h.host.start();
        let waiting = h.host.register(&h.binding, &request).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        assert_eq!(assignment::read(&h.state, "depot", "thread-one", &waiting.id).unwrap().state,
            AssignmentState::Blocked, "a task report must not close its unanswered question");

        let independent = h.host.register(&h.binding, &Registration {
            obligation_id: "obligation-8".into(), ..request
        }).unwrap();
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)));
        assert_eq!(assignment::read(&h.state, "depot", "thread-one", &independent.id).unwrap().state,
            AssignmentState::Settled, "another task can finish in the same conversation");
        assert_eq!(assignment::read(&h.state, "depot", "thread-one", &waiting.id).unwrap().state,
            AssignmentState::Blocked);
        let questions = store.list("depot", "thread-one").unwrap();
        let saved = questions.iter().find(|q| q.id == question.id).unwrap();
        assert_eq!(saved.state, State::Open);
        assert!(!saved.delivered);
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn unverifiable_original_request_never_opens_a_work_lease() {
        for bad in ["digest", "entity", "thread", "internal", "missing", "duplicate"] {
            let h = harness(1);
            let mut request = registration(&h);
            let mut row = serde_json::json!({"event":"PromptReceived", "turn_id":"turn-7",
                "thread_id":"thread-one", "entity_id":"depot", "source":"text",
                "text":"landing the three branches"});
            match bad {
                "digest" => request.instruction_sha256 = "0".repeat(64),
                "entity" => row["entity_id"] = "another-company".into(),
                "thread" => row["thread_id"] = "another-thread".into(),
                "internal" => row["source"] = "internal".into(),
                _ => (),
            }
            let lines = if bad == "missing" { String::new() } else {
                (row.to_string()+"\n").repeat(if bad == "duplicate" {2} else {1})
            };
            std::fs::write(h.root.join("conversation-ledger.jsonl"), lines).unwrap();
            h.host.start();
            let receipt = h.host.register(&h.binding, &request).unwrap();
            assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)), "{bad}");
            assert_eq!(h.spawns.load(Ordering::SeqCst), 0, "{bad}");
            assert!(h.work_prompts.lock().unwrap().is_empty(), "{bad}");
            assert_eq!(assignment::read(&h.state,"depot","thread-one",&receipt.id).unwrap().state,
                AssignmentState::Failed, "{bad}");
            h.host.shutdown();
            std::fs::remove_dir_all(h.root).unwrap();
        }
    }

    /// The same registration, declaring that its work needs the Mac's screen — the CEO's
    /// ruling §56's one input.
    fn screen_registration(harness: &Harness) -> Registration {
        Registration { needs_screen: true, ..registration(harness) }
    }

    /// Give one back-end session a readable, empty evidence file: every worker it started
    /// was observed ending. Without this the settle check answers "still running", which is
    /// the correct honest answer and not the one these tests are about.
    fn witnessed(state: &Path, session: &str) {
        let folder = state.join("evidence").join(session);
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(".lock"), "").unwrap();
        std::fs::write(folder.join("callbacks.jsonl"), "").unwrap();
    }

    fn until_live(host: &Arc<WorkHost>) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while host.live_on("thread-one").is_none() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(host.live_on("thread-one").is_some(), "the assignment never reached its back end");
    }

    /// **HIS STOP ENDS THE BACK END, AND HE IS TOLD HE STOPPED IT** — the reap walk, guest
    /// walk-0ddfdbf8ff00, 2026-09-27. On the real work lease, Stop kills the child inside
    /// `cancel()` (`native.rs`, `settle_workers_on_stop`), so the turn returns `Err(Closed)`
    /// and the `Err` arm wrote `failed` with "It stopped before it finished. Claude channel
    /// closed (child exited?)." on his timeline, twice for two Stops he pressed. The sentence
    /// for a stop already exists (`says::interrupted`); it was reachable only by `Ok`.
    #[test]
    fn his_stop_that_ends_the_back_end_is_told_as_a_stop_never_as_a_closed_channel() {
        let h = harness(5_000);
        h.fence.stop_ends_child.store(true, Ordering::SeqCst);
        h.host.start();
        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();
        until_live(&h.host);
        h.host.stop_assignment("depot", "thread-one", &receipt.id).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let row = loop {
            let row = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
            if !row.state.is_open() || std::time::Instant::now() > deadline {
                break row;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert_eq!(row.state, AssignmentState::Interrupted, "{:?}: {}", row.state, row.detail);
        // The state is written before the notice is raised, so wait for the notice itself.
        while h.notices.0.lock().unwrap().is_empty() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let told: Vec<_> = h.notices.0.lock().unwrap().iter().map(|(_, n)| (n.kind, n.text.clone())).collect();
        assert_eq!(told, vec![(NoticeKind::Interrupted, assignment::says::interrupted(&receipt.title))]);
        // The child is gone, so the lease is still retired: the next assignment opens a fresh one.
        let next = h.host.register(&h.binding, &Registration { obligation_id: "obligation-8".into(), ..registration(&h) }).unwrap();
        until_live(&h.host);
        assert_eq!(h.spawns.load(Ordering::SeqCst), 2, "the stopped back end was handed the next assignment");
        h.host.stop_assignment("depot", "thread-one", &next.id).unwrap();
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// The same for a quit that ends a turn in flight (`closing`): the sweep writes
    /// `interrupted`, and the runner's own `Err` must not write `failed` over it. Observed on
    /// the same guest: the assignment Quit ended read "failed ... Claude channel closed".
    #[test]
    fn a_quit_that_ends_the_back_end_mid_turn_leaves_the_assignment_stopped_not_failed() {
        let h = harness(5_000);
        h.fence.stop_ends_child.store(true, Ordering::SeqCst);
        h.host.start();
        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();
        until_live(&h.host);
        h.host.shutdown();
        let row = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(row.state, AssignmentState::Interrupted, "{:?}: {}", row.state, row.detail);
        assert!(
            h.notices.0.lock().unwrap().iter().all(|(_, n)| n.kind != NoticeKind::Failed),
            "a quit was told as a failure: {:?}",
            h.notices.0.lock().unwrap()
        );
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **Spec §0 row 2 and §7.1. Registration returns while the work is still to come.**
    ///
    /// **THE ASSERTION IS AN ORDERING, NOT A DURATION — changed 2026-09-20, and here is
    /// what forced it.** This test used to read `assert!(took < 100ms)`. A wall clock does
    /// not measure the property; it measures the machine, and on a loaded one the two
    /// diverge. zach-opus-cache1 measured **100.107 ms** — over by a tenth of one percent —
    /// and on pristine `049d8790` the whole `richos-core` lib suite under 56 CPU hogs gave
    /// *"registration took 129.283042ms"* on run 5 of 12. The same binary, run alone under
    /// 48 hogs at load 133, took **8.3–10.8 ms** across twenty runs: the bound was never
    /// measuring registration, it was measuring how busy the Mac was.
    ///
    /// A test that needs a timeout to pass is wrong, and a wider bound would only move the
    /// day it fires. So the thing the number stood for is asserted directly: **the work's
    /// first observable step is held shut, and `register` returns anyway.**
    ///
    ///   * With the gate closed, `spawn_work` — the runner opening this conversation's
    ///     back-end lease, the first thing that happens to a queued assignment — cannot
    ///     proceed. `spawns == 0` is then a fact the gate guarantees, not a race won.
    ///   * A build that prepared the work on the CALLER's thread would be inside that gate
    ///     when `register` was supposed to return, so it could not return at all. That is
    ///     the defect, caught structurally, at any speed, on any machine.
    ///   * `had_to_force` turns the hang such a build would otherwise cause into a named
    ///     failure. On the passing path the gate is released explicitly and it is never set.
    ///
    /// The second half is unchanged and is the positive control: released, the work really
    /// does run, really does reach the lease, and really does take the 400 ms that
    /// registration did not wait for. That is a LOWER bound, which load can only help.
    #[test]
    fn registering_returns_before_the_work_starts_and_the_work_still_runs() {
        let h = harness(400);
        let _runner = h.host.start();

        h.start_gate.shut();
        let started = std::time::Instant::now();
        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();

        // THE ASSERTION. `register` has returned while the work is provably still to come.
        //
        // `had_to_force` is read FIRST, and the order is the diagnosis. A build that
        // prepared the work on the caller's thread gets out of the gate only when the gate
        // lets go of itself — and by then `spawns` is 1, so the count would fire too, with
        // the wrong sentence. Verified 2026-09-20 by injecting exactly that defect into
        // `register_kind`: with the count first it read "the gate is in the wrong place",
        // which is false; with this order it names what actually happened.
        assert!(
            !h.start_gate.had_to_force(),
            "registration was waiting at the work's own first step, so it prepared the work on the \
             caller's thread instead of queueing it"
        );
        assert_eq!(
            h.spawns.load(Ordering::SeqCst),
            0,
            "the work's first step ran even though it was held shut — the gate is in the wrong place"
        );

        // Positive control: released, the thing it did NOT wait for really does happen, and
        // really does take longer than the registration did.
        h.start_gate.release();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        assert!(started.elapsed() >= std::time::Duration::from_millis(400));
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1, "the back-end lease was never opened");
        let row = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        assert_ne!(row.state, AssignmentState::Registered, "the runner never picked it up");
        assert_eq!(h.bound.lock().unwrap().len(), 1, "the assignment never reached the work lease");
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// The settled run a held job ends in once it is started: its obligation closed and its
    /// back end's evidence readable, for each back end the test can open.
    fn every_job_settles(h: &Harness, sessions: &[&str]) {
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Settled);
        for session in sessions {
            let folder = h.state.join("evidence").join(session);
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join(".lock"), "").unwrap();
            std::fs::write(folder.join("callbacks.jsonl"), "").unwrap();
        }
    }

    fn held_rows(h: &Harness, ids: &[String], detail: &str) -> bool {
        ids.iter().all(|id| {
            let row = assignment::read(&h.state, "depot", "thread-one", id).unwrap();
            row.state == AssignmentState::Registered && row.detail == detail
        })
    }

    fn settled_rows(h: &Harness, ids: &[String]) -> bool {
        ids.iter().all(|id| assignment::read(&h.state, "depot", "thread-one", id).unwrap().state == AssignmentState::Settled)
    }

    fn bound_ids(h: &Harness) -> Vec<String> {
        h.bound.lock().unwrap().iter().map(|work| work.assignment_id.clone()).collect()
    }

    /// **Three jobs queued while nobody is signed in: one attempt, three held, none failed;
    /// when the sign-in comes back the host itself starts the first held job, then the rest in
    /// order, and starts nothing else** (T3 idea 1, finished: richos-hq
    /// `docs/plans/2026-10-09-automatic-second-review-and-t3-ideas.md` §4 row 5, and Sage's
    /// check of it §1.1). The gate holds the first job at its first step so three are queued
    /// behind one back end. The fake refuses a turn the way Claude Code 2.1.295 does with
    /// nobody signed in. A clearance whose first job still finds nobody signed in tries that
    /// job alone and holds the queue again. No model turn is spent on any clearance: every
    /// prompt the back end receives is a job's own.
    #[test]
    fn a_signed_out_queue_is_held_and_the_host_resumes_it_in_order_when_the_sign_in_returns() {
        let h = harness(5);
        every_job_settles(&h, &["work-session-one", "work-session-rotated-1", "work-session-rotated-2"]);
        let account_signed_in = Arc::new(AtomicBool::new(false));
        let readings = Arc::new(AtomicUsize::new(0));
        let (signed, counted) = (account_signed_in.clone(), readings.clone());
        h.host.set_sign_in(Arc::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
            Some(signed.load(Ordering::SeqCst))
        }));
        h.host.set_hold_check_every(std::time::Duration::from_millis(20));
        h.host.start();
        h.signed_out.store(true, Ordering::SeqCst);
        h.start_gate.shut();
        let ids: Vec<String> = (0..3)
            .map(|n| {
                let reg = Registration { obligation_id: format!("obligation-q{n}"), ..registration(&h) };
                h.host.register(&h.binding, &reg).unwrap().id
            })
            .collect();
        h.start_gate.release();
        until("three jobs queued while signed out were not held", || held_rows(&h, &ids, HELD_FOR_SIGN_IN));
        // The host keeps looking at the sign-in, and starts nothing while it says no.
        let seen = readings.load(Ordering::SeqCst);
        until("the host never looked at the sign-in again", || readings.load(Ordering::SeqCst) >= seen + 3);
        assert_eq!(h.work_prompts.lock().unwrap().len(), 1, "one attempt for the three, not one per job");
        assert!(held_rows(&h, &ids, HELD_FOR_SIGN_IN), "none of them failed: all three are held");
        // What the window is told is held (nightly 48's walk, D26), and only on this conversation.
        assert_eq!(h.host.held_on("thread-one"), ids.iter().cloned().collect(), "the window is not told all three are held");
        assert!(h.host.held_on("another-thread").is_empty(), "another conversation is told this one's hold");
        let told = |h: &Harness| h.notices.0.lock().unwrap().iter().map(|(_, n)| (n.kind, n.text.clone())).collect::<Vec<_>>();
        assert_eq!(told(&h), vec![(NoticeKind::Held, "I haven't started \"landing the three branches\" or the 2 requests \
            waiting behind it yet, because nobody is signed in to Claude on this Mac. Sign in from Settings, under \
            Claude accounts, and they will start by themselves.".to_string())], "told once, and nothing announced as ended");
        // A clearance whose first job still finds nobody signed in: that job alone is tried,
        // the queue is held again, and nothing else is started.
        h.host.signed_in();
        until("the shell's word did not try the first held job", || h.work_prompts.lock().unwrap().len() == 2);
        until("the queue was not held again", || held_rows(&h, &ids, HELD_FOR_SIGN_IN));
        let seen = readings.load(Ordering::SeqCst);
        until("the host stopped looking at the sign-in", || readings.load(Ordering::SeqCst) >= seen + 3);
        assert_eq!(h.work_prompts.lock().unwrap().len(), 2, "a failed trial starts nothing else");
        assert_eq!(bound_ids(&h), vec![ids[0].clone(), ids[0].clone()], "the trial is the first held job");
        assert_eq!(told(&h).len(), 1, "a trial held again is not told again");
        // The sign-in comes back. The host's own look sees it, with no turn of anyone's.
        h.signed_out.store(false, Ordering::SeqCst);
        account_signed_in.store(true, Ordering::SeqCst);
        until("the held jobs were never started", || settled_rows(&h, &ids));
        assert_eq!(
            bound_ids(&h),
            vec![ids[0].clone(), ids[0].clone(), ids[0].clone(), ids[1].clone(), ids[2].clone()],
            "the first held job first, then the rest, in order"
        );
        assert_eq!(h.work_prompts.lock().unwrap().len(), 5,
            "two refused first turns, then one turn per job: no turn spent on a clearance");
        assert!(h.notices.0.lock().unwrap().iter().all(|(_, n)| n.kind != NoticeKind::Failed), "none of them failed");
        assert!(h.host.held_on("thread-one").is_empty(), "jobs that ran are still told as held");
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **A back end that will not open holds the queue the same way** (hunt part 1 finding 22
    /// kept: it is opened once for the jobs already waiting, not once per job). Every look
    /// afterwards is one attempt to open it and touches no job; a job registered meanwhile
    /// waits with the others; when it opens, the jobs run on that back end in order.
    #[test]
    fn a_back_end_that_will_not_open_holds_the_queue_and_the_host_resumes_it_when_it_opens() {
        let h = harness(5);
        every_job_settles(&h, &["work-session-one"]);
        h.host.set_hold_check_every(std::time::Duration::from_millis(20));
        h.host.start();
        h.broken.store(true, Ordering::SeqCst);
        h.start_gate.shut();
        let mut ids: Vec<String> = (0..3)
            .map(|n| {
                let reg = Registration { obligation_id: format!("obligation-q{n}"), ..registration(&h) };
                h.host.register(&h.binding, &reg).unwrap().id
            })
            .collect();
        h.start_gate.release();
        let held = format!("The provider is down. {HELD_FOR_BACK_END}");
        until("three jobs queued behind a broken back end were not held", || held_rows(&h, &ids, &held));
        let seen = h.attempts.load(Ordering::SeqCst);
        until("the host never looked at the back end again", || h.attempts.load(Ordering::SeqCst) >= seen + 3);
        assert!(held_rows(&h, &ids, &held), "none of them failed: all three are held");
        assert!(h.work_prompts.lock().unwrap().is_empty() && h.bound.lock().unwrap().is_empty(), "no job was started");
        let kinds: Vec<NoticeKind> = h.notices.0.lock().unwrap().iter().map(|(_, n)| n.kind).collect();
        assert_eq!(kinds, vec![NoticeKind::Held], "told once, and nothing announced as ended");
        let late = Registration { obligation_id: "obligation-late".into(), ..registration(&h) };
        ids.push(h.host.register(&h.binding, &late).unwrap().id);
        until("a job registered while held did not wait with the others", || held_rows(&h, &ids, &held));
        h.broken.store(false, Ordering::SeqCst);
        until("the held jobs were never started", || settled_rows(&h, &ids));
        assert_eq!(bound_ids(&h), ids, "in the order they were registered");
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1, "the back end the host's look opened is the one they ran on");
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **QUIT IS BOUNDED EVEN WHILE A BACK END IS STILL BEING OPENED** (hunt 2026-09-29 part 1,
    /// finding 28). `ensure_lease` holds the lease lock across `spawn_work`, so a quit that
    /// reached its last cleanup while the back end was still opening used to wait on that lock
    /// for as long as the opening took, after the two-second bound had already expired.
    ///
    /// The gate holds `spawn_work` shut, which is exactly that window, and it is released only
    /// after the quit has been given 8 s: four times the quit's own 2 s bound. A build with the
    /// unbounded wait cannot return inside it, on any machine; a bounded one returns in about
    /// 2 s. Then, released, the back end that finished opening after the quit is not kept and
    /// does not write over the sweep: the row stays `interrupted` and nothing was bound.
    #[test]
    fn quit_is_bounded_while_a_back_end_is_still_being_opened() {
        let h = harness(5);
        h.host.start();
        h.start_gate.shut();
        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !h.host.lease_locked("thread-one") {
            assert!(std::time::Instant::now() < deadline, "the runner never began opening its back end");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let host = Arc::clone(&h.host);
        let (done, quit_returned) = std::sync::mpsc::channel();
        let began = std::time::Instant::now();
        std::thread::spawn(move || {
            host.shutdown();
            done.send(()).ok();
        });
        let returned = quit_returned.recv_timeout(std::time::Duration::from_secs(8)).is_ok();
        let took = began.elapsed();
        h.start_gate.release();
        assert!(returned, "quit was still waiting after {took:?} for a back end that was still opening");
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)), "the runner never finished");
        let row = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(row.state, AssignmentState::Interrupted, "the late opening wrote over the quit: {row:?}");
        assert!(h.bound.lock().unwrap().is_empty(), "a back end opened after quit was given the assignment");
        assert!(!h.host.lease_locked("thread-one"));
        assert!(
            h.host.backend("thread-one").is_some_and(|backend| backend.lease.lock().unwrap().is_none()),
            "a back end opened after quit was kept"
        );
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn quota_hold_does_not_start_a_lease_and_disabling_it_continues_the_assignment() {
        let h = harness(5);
        let quota = Arc::new(crate::quota::Service::open(&h.root).unwrap());
        quota.set_policy(crate::quota::Policy { enabled: true, pause_percent: 93 }).unwrap();
        h.host.set_quota(quota.clone());
        h.host.start();
        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let row = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
            if row.state == AssignmentState::WaitingForQuota { break; }
            assert!(std::time::Instant::now() < deadline, "assignment did not enter the quota hold");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(h.spawns.load(Ordering::SeqCst), 0);
        quota.set_policy(crate::quota::Policy::default()).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(5)));
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1);
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **Plan §15 answer 3: background work cut by a limit continues on the next account.**
    /// Two accounts, both read by the fake `claude` (Account 1 at 40%, Work at 5%). The back
    /// end is spawned under Account 1, whose work turn is refused for a usage limit (the error
    /// `native.rs` writes for the three real signals). The run is started again, on a NEW lease
    /// spawned under Work, which takes the work: the cut run raises nothing to him.
    #[test]
    #[cfg(unix)]
    fn a_background_lease_cut_by_a_usage_limit_is_started_again_under_the_next_account() {
        let h = harness(5);
        let quota = Arc::new(crate::quota::Service::open(&h.root).unwrap());
        let work = quota.accounts.add("Work").unwrap();
        let bin = crate::quota::tests::fake_claude(&h.root);
        crate::quota::tests::usage(&h.root.join("usage-1.json"), 40., 40., "2099-01-05T00:00:00Z");
        crate::quota::tests::usage(&work.folder.clone().unwrap().join("usage.json"), 5., 10., "2099-01-06T00:00:00Z");
        quota.refresh(&bin, true);
        {
            let mut fill = h.fill.lock().unwrap();
            fill.quota = Some(quota.clone());
            fill.limited = Some("1".into());
        }
        h.host.set_quota(quota.clone());
        h.host.start();
        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();
        // Two runs: the one the limit cut, and the one started again under Work.
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(30)), "the run never finished");
        assert_eq!(h.fill.lock().unwrap().spawned, vec!["1".to_string(), "2".to_string()],
            "the second lease was spawned under the next account");
        assert_eq!(quota.lease_account().id, "2");
        assert!(h.work_prompts.lock().unwrap().len() >= 2, "the work was sent again");
        // **Weekly-switch plan §3**: the run started again reads its receipts, handed-off
        // workers included, before redoing anything; the first run was told nothing of it.
        let prompts = h.work_prompts.lock().unwrap().clone();
        assert!(!prompts[0].starts_with(PICKED_UP_NOTE), "{}", prompts[0]);
        assert!(prompts[1].starts_with(PICKED_UP_NOTE), "the run started again was not told to check first: {}", prompts[1]);
        // The row's last word is the SECOND run's, on the successor; the cut run said nothing
        // to him at all. (How the second run settles is this harness's own business: with no
        // worker receipts it reports that nothing was landed, as every harness run does.)
        let row = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(row.work_session.as_deref(), Some("work-session-rotated-1"), "{row:?}");
        assert!(!row.detail.contains("usage limit"), "{}", row.detail);
        let told = h.notices.0.lock().unwrap().clone();
        assert_eq!(told.len(), 1, "only the second run's ending reached him: {told:?}");
        assert!(told.iter().all(|(_, n)| !n.text.contains("usage limit")), "{told:?}");
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// A clock that waits no time and writes down every wait it was asked for.
    #[derive(Default)]
    struct FakeRetryClock(Mutex<Vec<std::time::Duration>>);
    impl crate::upstream::RetryClock for FakeRetryClock {
        fn wait(&self, wait: std::time::Duration, stop_asked: &dyn Fn() -> bool) -> bool {
            self.0.lock().unwrap().push(wait);
            !stop_asked()
        }
    }

    /// **The CEO, 2026-10-07: *"There needs to be an exponential backoff ... first retry after 1
    /// minute; second retry: after another 2-minute wait; third retry: after another 5-minute
    /// wait; 4th retry: after another 10-minute wait; 5th retry: after another 20-minute wait;
    /// 6th retry: after another 40-minute wait; 7th retry: after another 80-minute wait."***
    ///
    /// A work turn meets seven `529`s in a row: the host waits 1, 2, 5, 10, 20, 40 and 80
    /// minutes and sends the SAME turn again on the SAME back end each time (one spawn, eight
    /// identical prompts, the row's session the first one), and the eighth attempt answers. A
    /// second assignment then meets one `529`: its wait is 1 minute again, because the answered
    /// turn reset the schedule.
    ///
    /// RED before this change: the work host had no overload handling at all, so the first
    /// `529` failed the run and no wait was ever asked for.
    #[test]
    fn an_overloaded_work_turn_waits_out_his_schedule_on_the_same_session_and_a_success_resets_it() {
        const CAPTURED_529: &str =
            include_str!("../../../../../docs/verification/upstream-failure-2026-09-05/captured-529.txt");
        let overloaded = CAPTURED_529.lines().next().unwrap().to_string();
        let minutes = |m: u64| std::time::Duration::from_secs(m * 60);
        let h = harness(5);
        let clock = Arc::new(FakeRetryClock::default());
        h.host.set_retry_clock(clock.clone());
        h.fail_next.lock().unwrap().extend(std::iter::repeat_n(overloaded.clone(), 7));
        h.host.start();
        let first = h.host.register(&h.binding, &registration(&h)).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)), "the run never finished");

        assert_eq!(*clock.0.lock().unwrap(), [1, 2, 5, 10, 20, 40, 80].map(minutes).to_vec(), "his schedule");
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1, "every retry went to the back end that met the 529");
        let prompts = h.work_prompts.lock().unwrap().clone();
        assert!(prompts.len() >= 8, "{prompts:?}");
        assert!(prompts[..8].iter().all(|p| *p == prompts[0]), "the same turn, sent again: {prompts:?}");
        let row = assignment::read(&h.state, "depot", "thread-one", &first.id).unwrap();
        assert_eq!(row.work_session.as_deref(), Some("work-session-one"), "{row:?}");
        // The eighth attempt answered, so the run reached this harness's ordinary settle (with no
        // worker receipts it reports that nothing was landed, as every harness run does), never
        // the overload.
        assert!(!row.detail.contains("529") && !row.detail.contains("capacity"), "{}", row.detail);
        assert!(told(&h).iter().all(|t| !t.contains("529") && !t.contains("capacity")), "{:?}", told(&h));

        // The answered turn reset the schedule: the next overload waits 1 minute again.
        h.fail_next.lock().unwrap().push_back(overloaded);
        h.host
            .register(&h.binding, &Registration { obligation_id: "obligation-8".into(), ..registration(&h) })
            .unwrap();
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)), "the second run never finished");
        assert_eq!(*clock.0.lock().unwrap(), [1, 2, 5, 10, 20, 40, 80, 1].map(minutes).to_vec());
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1, "still the one back end");
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **The CEO, 2026-10-04: *"How can 'work cut off by a limit' happen if THE WHOLE ... JOB
    /// OF THIS ENTIRE ... FEATURE IS TO PREVENT THAT FROM HAPPENING????"*** — his case is
    /// background work: a long run of several turns on one back end.
    ///
    /// One run, three turns on one seat: the brief (it starts `npm test`), a continuation
    /// reporting it (which starts `npm run e2e`), and a continuation reporting that. Between
    /// the two continuation turns Account 1 crosses its check point — its lease streams 93% of
    /// the five-hour window, setting on Switch — and would refuse its next turn for a usage
    /// limit (the wall). The second continuation must be sent on a lease spawned under Work,
    /// primed with the assignment, bound to the same seat, and the run must end on its report:
    /// never cut off, never started again.
    ///
    /// RED at `d837b5204`: the account is read once per run (step 0c), so the second
    /// continuation goes to Account 1, is refused, and the backstop starts the run again.
    #[test]
    #[cfg(unix)]
    fn a_long_background_run_moves_to_the_next_account_between_two_continuation_turns_and_carries_on() {
        use crate::cognition::ObligationState;
        const STARTED: &str = "I've started the unit tests. I'll tell you when they finish.";
        const SECOND: &str = "The unit tests passed. I've started the end-to-end suite now.";
        const FINISHED: &str = "The end-to-end suite has finished: all 48 scenarios passed.";
        const HANDOFF: &str = "The end-to-end suite writes its output to /fictional/project/e2e.log.";
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        *h.background.lock().unwrap() = Some(Vec::new());
        *h.handoff_reply.lock().unwrap() = HANDOFF.into();
        h.replies.lock().unwrap().extend([STARTED.to_string(), SECOND.to_string(), FINISHED.to_string()]);
        h.start_in_turn.lock().unwrap().extend([
            vec![background_command("bunit", "npm test")],
            vec![background_command("be2e", "npm run e2e")],
        ]);
        // Two accounts, read by the fake `claude`: Account 1 at 60% of its five-hour window,
        // Work at 5%. Account 1 is in use.
        let quota = Arc::new(crate::quota::Service::open(&h.root).unwrap());
        let work = quota.accounts.add("Work").unwrap();
        let bin = crate::quota::tests::fake_claude(&h.root);
        crate::quota::tests::usage(&h.root.join("usage-1.json"), 60., 40., "2099-01-05T00:00:00Z");
        crate::quota::tests::usage(&work.folder.clone().unwrap().join("usage.json"), 5., 10., "2099-01-06T00:00:00Z");
        quota.refresh(&bin, true);
        // The five-hour switch acts only while the automatic switch is on (e1ac24d27).
        quota.set_policy(crate::quota::Policy { enabled: true, pause_percent: 93 }).unwrap();
        quota.set_at_threshold(crate::claude_accounts::AtThreshold::Switch).unwrap();
        assert_eq!(quota.lease_account().id, "1");
        h.fill.lock().unwrap().quota = Some(quota.clone());
        h.host.set_quota(quota.clone());
        h.host.start();
        let job = h.host
            .register(&h.binding, &Registration { title: "run both test suites and tell me how they end".into(), ..registration(&h) })
            .unwrap();

        // ---- turn 1, the brief, and turn 2, the first continuation: both on Account 1 ------
        assert_eq!(until_notices(&h, 1)[0].text, STARTED);
        background_command_ends(&h, "bunit", false);
        assert_eq!(until_notices(&h, 2)[1].text, SECOND);
        assert_eq!(h.fill.lock().unwrap().turns, vec![Some("1".to_string()); 2]);

        // ---- between two continuation turns, Account 1 crosses its check point ------------
        // 2099-01-01T00:00:00Z, the fake's five-hour reset, so it is the same window.
        let crossed = (vec![crate::quota::Window { id: "five_hour".into(), label: "Five-hour".into(), used_percent: 93.,
            resets_at: Some(4_070_908_800_000), duration_ms: 5 * 3_600_000 }], crate::util::now_millis() + 1_000);
        {
            let mut fill = h.fill.lock().unwrap();
            fill.streamed.insert("1".into(), crossed);
            fill.limited = Some("1".into());
        }
        background_command_ends(&h, "be2e", false);
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)), "the run never finished");

        // ---- the second continuation ran on Work, on a successor that carries the job ------
        let fill = h.fill.lock().unwrap();
        assert_eq!(fill.spawned, vec!["1".to_string(), "2".to_string()], "the successor was spawned under Work");
        assert_eq!(fill.turns, vec![Some("1".to_string()), Some("1".to_string()), Some("2".to_string())],
            "the second continuation turn ran ONLY under Work");
        drop(fill);
        assert_eq!(quota.lease_account().id, "2");
        assert_eq!(h.host.rotations("thread-one"), (1, Some(crate::spine::ACCOUNT_SWITCH.to_string())));
        let prompts = h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(), 3, "never started again: {prompts:?}");
        assert_eq!(prompts.iter().filter(|p| p.contains("\"user_request\"")).count(), 1, "the brief was sent once");
        assert!(prompts[2].contains("npm run e2e") && prompts[2].starts_with(COMMAND_ENDED_HEAD), "{}", prompts[2]);
        let handoffs = h.handoffs.lock().unwrap().clone();
        assert_eq!(handoffs.len(), 1);
        assert!(handoffs[0].contains(HANDOFF_ASK_MID_RUN), "{}", handoffs[0]);
        let primed = h.reprimes.lock().unwrap().clone();
        assert_eq!(primed.len(), 1);
        assert!(primed[0].starts_with(MID_RUN_HANDOVER_OPENING), "{}", primed[0]);
        for carried in ["run both test suites and tell me how they end", "\"user_request\"", SECOND, HANDOFF] {
            assert!(primed[0].contains(carried), "the successor was not told {carried:?}: {}", primed[0]);
        }
        let bound = h.bound.lock().unwrap().clone();
        assert_eq!(bound.len(), 2, "the successor took the same seat");
        assert_eq!(bound[0], bound[1]);
        // ---- and the run ended on its report, on the successor ------------------------------
        let row = assignment::read(&h.state, "depot", "thread-one", &job.id).unwrap();
        assert!(row.was_answered(), "{:?}: {}", row.state, row.detail);
        assert_eq!(row.work_session.as_deref(), Some("work-session-rotated-1"), "{row:?}");
        let notices = until_notices(&h, 3);
        let said: Vec<&str> = notices.iter().map(|n| n.text.as_str()).collect();
        assert_eq!(said, vec![STARTED, SECOND, FINISHED], "never cut off: {notices:?}");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **Background work: "Switched to" only once a job really runs under the new account.**
    /// The back end's lease on Account 1 streams 93% of its five-hour window (setting on
    /// Switch), so before the second job the switch is decided; but a command the back end
    /// started is still running, so that job runs on Account 1 (finding 50) and no notice may
    /// be pending for the conversation. Once the command ends, the third job runs under Work
    /// and the notice is there to be said.
    ///
    /// RED on main at `442b60f9e`: the notice was written when the switch was decided.
    #[test]
    #[cfg(unix)]
    fn background_work_says_the_switch_only_once_a_job_runs_under_the_new_account() {
        use crate::lease_commands::CommandReading;
        let h = harness(5);
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Settled);
        let quota = Arc::new(crate::quota::Service::open(&h.root).unwrap());
        let work = quota.accounts.add("Work").unwrap();
        let bin = crate::quota::tests::fake_claude(&h.root);
        crate::quota::tests::usage(&h.root.join("usage-1.json"), 60., 40., "2099-01-05T00:00:00Z");
        crate::quota::tests::usage(&work.folder.clone().unwrap().join("usage.json"), 5., 10., "2099-01-06T00:00:00Z");
        quota.refresh(&bin, true);
        // The five-hour switch acts only while the automatic switch is on (e1ac24d27).
        quota.set_policy(crate::quota::Policy { enabled: true, pause_percent: 93 }).unwrap();
        quota.set_at_threshold(crate::claude_accounts::AtThreshold::Switch).unwrap();
        h.fill.lock().unwrap().quota = Some(quota.clone());
        h.host.set_quota(quota.clone());
        h.host.start();
        let run = |n: u64| {
            h.host
                .register(&h.binding, &Registration { obligation_id: format!("obligation-notice-{n}"), ..registration(&h) })
                .unwrap();
            assert!(h.host.wait_for_completed(n, std::time::Duration::from_secs(10)));
        };
        run(1);
        assert_eq!(h.fill.lock().unwrap().turns, vec![Some("1".to_string())]);

        // Account 1 crosses its check point (2099-01-01T00:00:00Z is the fake's five-hour
        // reset, so it is the same window) while a command the back end started is running.
        let crossed = (vec![crate::quota::Window { id: "five_hour".into(), label: "Five-hour".into(), used_percent: 93.,
            resets_at: Some(4_070_908_800_000), duration_ms: 5 * 3_600_000 }], crate::util::now_millis() + 1_000);
        h.fill.lock().unwrap().streamed.insert("1".into(), crossed);
        *h.commands.lock().unwrap() = Some(CommandReading::Running(1));
        run(2);
        assert_eq!(quota.lease_account().id, "2", "the switch was decided");
        assert_eq!(h.fill.lock().unwrap().turns.last(), Some(&Some("1".to_string())), "the job ran on Account 1");
        assert_eq!(quota.accounts.take_notice(), None, "\"Switched to\" was pending while the job ran on Account 1");

        // The command ends: the next job runs under Work, and the switch is now real.
        *h.commands.lock().unwrap() = Some(CommandReading::Clear);
        run(3);
        assert_eq!(h.fill.lock().unwrap().turns.last(), Some(&Some("2".to_string())), "the job ran under Work");
        assert!(quota.accounts.take_notice().is_some_and(|line| line.starts_with("I switched the team to your **Work** account. Account 1 had used 93% of its 5-hour limit")));
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    #[test]
    fn stopping_a_quota_held_assignment_never_opens_a_lease() {
        let h = harness(5);
        let quota = Arc::new(crate::quota::Service::open(&h.root).unwrap());
        quota.set_policy(crate::quota::Policy { enabled: true, pause_percent: 93 }).unwrap();
        h.host.set_quota(quota);
        h.host.start();
        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();
        h.host.stop_assignment("depot", "thread-one", &receipt.id).unwrap();
        h.host.shutdown();
        assert_eq!(h.spawns.load(Ordering::SeqCst), 0);
        assert_eq!(assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap().state, AssignmentState::Interrupted);
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// Spec §5.8c and §5.4: one seat per assignment, carried to the lease, and the grant
    /// revoked per assignment rather than left standing.
    #[test]
    fn each_assignment_reaches_the_work_lease_on_its_own_seat_and_gives_the_grant_back() {
        let h = harness(5);
        let _runner = h.host.start();
        let first = h.host.register(&h.binding, &registration(&h)).unwrap();
        let second = h
            .host
            .register(&h.binding, &Registration { obligation_id: "obligation-8".into(), ..registration(&h) })
            .unwrap();
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)));
        let bound = h.bound.lock().unwrap().clone();
        assert_eq!(bound.len(), 2);
        assert_ne!(bound[0].seat, bound[1].seat);
        // The seat's `turn_id` on the wire is the OBLIGATION, which is what the engine's
        // reconciler reads back (`mega-lander/app.py:704`).
        assert_eq!(bound[0].obligation_id, "obligation-7");
        assert_eq!(bound[1].obligation_id, "obligation-8");
        assert_eq!(bound[0].assignment_id, first.id);
        assert_eq!(bound[1].assignment_id, second.id);
        for one in &bound {
            assert!(one.seat.starts_with("work-seat:"));
            assert_ne!(one.seat, "ceo-default");
            // §3.6: the instruction reference is the turn he gave it in, frozen.
            assert_eq!(one.instruction_ledger_ref, "ledger:thread-one:turn-7");
        }
        assert_eq!(h.revoked.load(Ordering::SeqCst), 2, "a grant was left standing");
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    // ----------------------------------------------------------------------------------
    // THE CEO's RULING §56 — "WAITING FOR THE SCREEN"
    //
    // `richos-hq/wiki/ceo-decisions.md` §56 (2026-09-18): *"I like that "Watch for the Mac's
    // screen to unlock" feature that you had running here. Should be added to the RichOS app
    // for automatic use when Rich needs it."*
    //
    // The acceptance is his sentence, so the first test below is his sentence and nothing
    // else: a job that needs the screen, on a locked screen, waits — and carries on by itself
    // when the screen comes back, with nobody touching anything.
    // ----------------------------------------------------------------------------------

    /// Read the record until it reaches `want`, or give up loudly. Never a bare sleep on a
    /// guess: the same reasoning `wait_for_completed` carries.
    fn await_state(h: &Harness, id: &str, want: AssignmentState) -> Assignment {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let row = assignment::read(&h.state, "depot", "thread-one", id).unwrap();
            if row.state == want {
                return row;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the assignment never reached {want:?}; it is {:?} ({})",
                row.state,
                row.detail
            );
            std::thread::yield_now();
        }
    }

    /// **§56, END TO END, AND IT IS THE WHOLE ACCEPTANCE.**
    ///
    /// A screen-bound assignment on a locked screen reaches `waiting-for-screen`, spends
    /// NOTHING while it is there, and runs to the lease by itself the moment the screen
    /// unlocks. No approval, no notice, no retry, nobody touching anything.
    #[test]
    fn a_screen_bound_assignment_waits_for_a_locked_screen_and_carries_on_when_it_unlocks() {
        let h = harness(5);
        let screen = crate::screen::FakeScreen::locked();
        h.host.set_screen(screen.clone());
        h.host.set_screen_poll(std::time::Duration::from_millis(2));
        let _runner = h.host.start();
        let receipt = h.host.register(&h.binding, &screen_registration(&h)).unwrap();

        // 1. IT WAITS, and the durable record says so in the app's own words.
        let waiting = await_state(&h, &receipt.id, AssignmentState::WaitingForScreen);
        assert_eq!(waiting.detail, crate::screen::says::detail());

        // 2. IT IS WAITING ON THE SCREEN AND NOT ON HIM. Each of these three is a different
        //    surface reading the same state, and a `Blocked`-shaped answer to any of them
        //    would put a decision in front of him that is not his.
        assert!(waiting.state.is_open(), "an update must not install over it");
        assert!(!waiting.state.awaits_his_word(), "he has nothing to decide about a lock");
        assert!(waiting.state.waits_for_the_world());

        // 3. NOTHING HAS BEEN SPENT. The gate is before `ensure_lease`, so no provider
        //    connection is sitting on a locked screen and no seat has been bound. This is the
        //    assertion that makes `recovery.rs`'s "had not started" truthful.
        assert_eq!(h.spawns.load(Ordering::SeqCst), 0, "a locked screen must not open a lease");
        assert!(h.bound.lock().unwrap().is_empty(), "a seat was bound while the screen was locked");

        // 4. HE IS TOLD NOTHING. §56 is a wait that looks after itself; a push about it would
        //    be the nag. (The state above is the visibility.)
        assert!(
            h.notices.0.lock().unwrap().is_empty(),
            "a screen wait must raise no notice: {:?}",
            h.notices.0.lock().unwrap()
        );

        // 5. THE SCREEN COMES BACK, AND IT CARRIES ON BY ITSELF.
        screen.set(crate::screen::ScreenReading::unlocked());
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        assert_eq!(h.bound.lock().unwrap().len(), 1, "it never reached the lease after the unlock");
        let done = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        assert_ne!(done.state, AssignmentState::WaitingForScreen, "it is still claiming to wait");
        assert!(!done.state.waits_for_the_world());
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **The negative control for the test above**, and the reason the whole feature costs
    /// ordinary work nothing: an assignment that did not declare a screen need never reads the
    /// screen at all, even while it is locked.
    #[test]
    fn work_that_does_not_need_the_screen_never_looks_at_it() {
        let h = harness(5);
        let screen = crate::screen::FakeScreen::locked();
        h.host.set_screen(screen.clone());
        h.host.set_screen_poll(std::time::Duration::from_millis(2));
        let _runner = h.host.start();
        // `registration`, not `screen_registration` — needs_screen is false.
        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        assert_eq!(screen.reads(), 0, "the screen was read for work that does not need it");
        let row = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        assert_ne!(row.state, AssignmentState::WaitingForScreen);
        assert_eq!(h.bound.lock().unwrap().len(), 1);
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **THE POLARITY RULE AT THE HOST LEVEL, and it is the inverse of the update gate's.**
    /// A build that cannot read the screen runs screen-bound work immediately rather than
    /// parking it forever — because §56's wait has no timeout, so an unbounded wait on an
    /// unestablished reading is the worse failure. A host nobody installed a reader on is
    /// exactly this case, so the default is asserted too.
    #[test]
    fn a_screen_that_cannot_be_read_runs_the_work_rather_than_waiting_forever() {
        let h = harness(5);
        // No `set_screen` call at all: the host's default is `UnknownScreen`.
        h.host.set_screen_poll(std::time::Duration::from_millis(2));
        let _runner = h.host.start();
        let receipt = h.host.register(&h.binding, &screen_registration(&h)).unwrap();
        assert!(
            h.host.wait_for_completed(1, std::time::Duration::from_secs(10)),
            "an unreadable screen stranded the work, which is the one thing it must never do"
        );
        let row = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        assert_ne!(row.state, AssignmentState::WaitingForScreen);
        assert_eq!(h.bound.lock().unwrap().len(), 1);
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **WORK PARKED ON A LOCKED SCREEN MUST NOT COME BACK FROM THE DEAD AFTER A QUIT.**
    ///
    /// **This test was WRONG on its first draft and the mistake is worth keeping in writing,
    /// because it is the kind that leaves a green suite guarding nothing.** The first version
    /// asserted that `shutdown()` RETURNS QUICKLY and that the receipt says `interrupted`. It
    /// passed with `shutdown`'s `watch.stop()` deleted — so it proved nothing. `shutdown` never
    /// blocks on a screen wait in the first place: its drain loop waits on `inner.live`, and a
    /// screen wait happens strictly BEFORE the lease, so `live` is `None` and the drain returns
    /// at once whether or not the wait was ever reached.
    ///
    /// **The real defect is the opposite of a hang, and it is much worse than one.** Without
    /// the stop, the runner thread stays parked on its own condition variable after the app has
    /// quit, the sweep writes `interrupted`, and then — when the screen is unlocked, minutes or
    /// hours later — that thread WAKES UP, returns from the wait and carries on: it advances the
    /// receipt to `preparing`, opens a provider lease and binds a seat, all on behalf of an
    /// assignment the app already told him was stopped. Work resurrecting over an `interrupted`
    /// receipt is exactly the class of lie `recovery.rs` exists to refuse.
    ///
    /// So the test quits, THEN unlocks the screen, then asserts nothing moved. Proven to fail
    /// with the `watch.stop()` removed from `shutdown`.
    #[test]
    fn a_quit_leaves_an_assignment_parked_on_a_locked_screen_stopped_for_good() {
        let h = harness(5);
        let screen = crate::screen::FakeScreen::locked();
        h.host.set_screen(screen.clone());
        // Short, so that IF the wait survived the quit it would certainly notice the unlock
        // below. A long poll here would hide the defect rather than expose it.
        h.host.set_screen_poll(std::time::Duration::from_millis(2));
        let _runner = h.host.start();
        let receipt = h.host.register(&h.binding, &screen_registration(&h)).unwrap();
        let waiting = await_state(&h, &receipt.id, AssignmentState::WaitingForScreen);
        assert_eq!(waiting.detail, crate::screen::says::detail());

        // `shutdown` returning at all is the "quit returns" half; a quit that never came back
        // would hang here and libtest names the test after 60 s. The `< 10 s` clock that
        // stood after this call could not catch that hang (it only runs once the call has
        // returned); it could only fail a quit that returned slowly on a busy Mac (audit R9,
        // work_host `< 10 s`). What this test proves is what the quit LEAVES, below.
        h.host.shutdown();

        // Witnessed, never invented: a quit is a stop somebody made.
        let stopped = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(stopped.state, AssignmentState::Interrupted, "detail was: {}", stopped.detail);

        // **THE ASSERTIONS THAT ACTUALLY GUARD SOMETHING.** The screen comes back AFTER the
        // quit. Nothing may stir.
        screen.set(crate::screen::ScreenReading::unlocked());
        std::thread::sleep(std::time::Duration::from_millis(150));
        let after = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();

        // **`updated_at_ms` IS THE WITNESS, AND THE FINAL STATE IS NOT.** Measured while
        // proving this test can fail: with the stop removed, the resurrected runner advanced
        // the receipt and something downstream settled it back to `interrupted`, so the state
        // check alone stayed green while the record had been rewritten 51-59 ms after the app
        // had quit. A "did anything touch this at all" assertion catches a resurrection that
        // a "what does it say now" assertion cannot, so it goes first.
        assert_eq!(
            after.updated_at_ms, stopped.updated_at_ms,
            "the receipt was written {} ms AFTER the app quit: a runner parked on the locked \
             screen woke up on the unlock and carried on with work that had been stopped",
            after.updated_at_ms.saturating_sub(stopped.updated_at_ms)
        );
        assert_eq!(h.spawns.load(Ordering::SeqCst), 0, "a lease was opened after the app quit");
        assert!(h.bound.lock().unwrap().is_empty(), "a seat was bound after the app quit");
        assert_eq!(after.state, AssignmentState::Interrupted, "{:?} ({})", after.state, after.detail);
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **His stop reaches a parked assignment too**, and the state it writes is the one that
    /// is exactly true of a screen wait: nothing was prepared.
    #[test]
    fn his_stop_reaches_an_assignment_parked_on_a_locked_screen() {
        let h = harness(5);
        let screen = crate::screen::FakeScreen::locked();
        h.host.set_screen(screen.clone());
        h.host.set_screen_poll(std::time::Duration::from_secs(3600));
        let _runner = h.host.start();
        let receipt = h.host.register(&h.binding, &screen_registration(&h)).unwrap();
        await_state(&h, &receipt.id, AssignmentState::WaitingForScreen);

        h.host.stop_assignment("depot", "thread-one", &receipt.id).unwrap();
        let row = await_state(&h, &receipt.id, AssignmentState::Interrupted);
        assert!(
            row.detail.contains("Nothing was prepared"),
            "a screen wait that is stopped had nothing prepared: {}",
            row.detail
        );
        assert!(h.bound.lock().unwrap().is_empty());
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **Spec §4.2's structural half.** The work lease's cancel handle is the host's, and
    /// it is never the one in `TurnControl`'s slot — so `stop_turn` cannot reach it.
    ///
    /// The negative assertion has a positive control: the same `TurnControl` DOES carry a
    /// conversation lease's handle when one is attached, so an empty slot here is evidence
    /// rather than an artifact of a control nobody ever populates.
    #[test]
    fn stop_turn_never_cancels_the_work_lease() {
        use crate::steering::TurnControl;
        let h = harness(3000);
        let control = TurnControl::open(h.root.join("steering")).unwrap();
        let _runner = h.host.start();
        h.host.register(&h.binding, &registration(&h)).unwrap();
        until_live(&h.host);

        // Positive control: the control CAN hold a handle, so its emptiness means something.
        let conversation = Arc::new(Fence::default());
        control.set_cancel(Some(conversation.clone()));
        control.shutdown_lease();
        assert_eq!(conversation.shutdowns.load(Ordering::SeqCst), 1);
        // The work lease's fence was never handed to that control and was not touched.
        assert!(!h.fence.stop_seen.load(Ordering::SeqCst), "Stop reached the work lease");
        assert!(h.host.live_on("thread-one").is_some(), "the work stopped when the conversation did");

        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// Spec §4.2's second case and §7.3: the per-assignment stop DOES stop it, and what it
    /// produces is `interrupted` — never `settled`.
    #[test]
    fn the_per_assignment_stop_interrupts_it_and_never_settles_it() {
        let h = harness(3000);
        let _runner = h.host.start();
        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();
        until_live(&h.host);
        h.host.stop_assignment("depot", "thread-one", &receipt.id).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(row.state, AssignmentState::Interrupted);
        assert_ne!(row.state, AssignmentState::Settled);
        let notices = h.notices.0.lock().unwrap();
        assert_eq!(notices.last().unwrap().1.kind, NoticeKind::Interrupted);
        drop(notices);
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// Spec §2.9. The settle check reads the WORK lease's evidence directory. With no
    /// readable evidence the answer is "still running", never "settled" — and the positive
    /// control is the same call against a directory that IS readable and empty.
    #[test]
    fn background_work_is_settled_against_the_work_lease_session_not_the_conversation() {
        let h = harness(5);
        // Before a lease exists there is no session at all: unattributed, so still running.
        assert!(matches!(h.host.settlement("thread-one"), Settlement::StillRunning(_)));
        let _runner = h.host.start();
        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        assert_eq!(h.host.lease_sessions(), vec!["work-session-one".to_string()]);
        // The work lease's own evidence directory is missing, so the honest answer is
        // "still running" and the assignment is NOT settled.
        //
        // **THIS LINE USED TO ASSERT `Running` AND WAS ASSERTING THE DEFECT.** The reading
        // above is unchanged and still correct — nothing about these workers was witnessed —
        // but the assignment's own turn has ENDED by the time the runner reads it, and a job
        // whose turn has ended is not running. See the `Outcome::StillRunning` arm in
        // `run_one`. The settle reading and the assignment's state are two different
        // questions, and this test now asserts both rather than conflating them.
        let row = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(row.state, AssignmentState::Failed);
        assert_ne!(row.state, AssignmentState::Running, "a job whose turn has ended is reported as running");
        assert!(matches!(h.host.settlement("thread-one"), Settlement::StillRunning(_)));

        // Positive control: give the WORK session a readable, empty evidence file and the
        // same call settles. The conversation's session id would not reach this directory.
        let folder = h.state.join("evidence").join("work-session-one");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(".lock"), "").unwrap();
        std::fs::write(folder.join("callbacks.jsonl"), "").unwrap();
        assert_eq!(h.host.settlement("thread-one"), Settlement::Settled);
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **FOUR ENDINGS, AND THE THIRD IS THE ONE THE CEO'S RULING §52 CREATED.**
    ///
    /// The engine's rule is unchanged and still the foundation
    /// (`mega-lander/app.py:664-669`): *"An assignment whose workers have all stopped is NOT
    /// settled"* — settled is read from the OBLIGATION, never from the workers. Every case
    /// below runs against identical worker evidence (readable, empty: every worker this lease
    /// started was observed ending), so the workers' verdict is held constant throughout and
    /// the only things that differ are the obligation and the DESK.
    ///
    /// **What changed on 2026-09-18.** Its predecessor was
    /// `workers_all_ending_means_ready_to_approve_and_only_the_obligation_can_settle_it` and
    /// had three cases, because an open obligation with the workers done could mean only one
    /// thing: the run had stopped at its land, waiting for him. §52 — *"There's nothing that
    /// ever not lands on its own here in the terminal … So, yes, always land on its own"* —
    /// makes that impossible, so the open-obligation case splits in two and only the desk can
    /// say which it is:
    ///
    ///   1. obligation CLOSED                                → `settled`, and the sentence
    ///      names what landed
    ///   2. obligation OPEN, a real question of his waiting   → `blocked`, still his
    ///   3. obligation OPEN, nothing waiting for him          → `failed`: a job that did not
    ///      land, reported as one
    ///   4. obligation UNREADABLE                             → `running`, never settled
    ///
    /// **Case 3 against case 2 is the whole assertion.** Either alone passes for the wrong
    /// reason: a build that called every unfinished job `failed` would satisfy 3 and destroy
    /// the approval queue, and a build that called every unfinished job `blocked` would
    /// satisfy 2 and be the old behavior §52 overruled. They differ in one input — whether a
    /// question of his is waiting — and in nothing else.
    #[test]
    fn a_job_that_did_not_land_is_failed_and_only_a_question_of_his_makes_it_blocked() {
        use crate::cognition::ObligationState;
        let h = harness(5);
        // Identical, readable, empty worker evidence for every case below.
        let folder = h.state.join("evidence").join("work-session-one");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(".lock"), "").unwrap();
        std::fs::write(folder.join("callbacks.jsonl"), "").unwrap();
        let _runner = h.host.start();

        // 1. The obligation is OPEN and NOTHING is waiting for him: the job did not land, and
        //    that is what he is told — never "ready for you to approve", which would invite a
        //    press that would do nothing, and never "done".
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        let unlanded = h.host.register(&h.binding, &registration(&h)).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &unlanded.id).unwrap();
        assert_eq!(row.state, AssignmentState::Failed, "a job that did not land was not reported as one");
        assert_ne!(row.state, AssignmentState::Settled);
        assert_ne!(row.state, AssignmentState::Blocked, "a job with no question for him was left waiting on him");
        // The reason is in his terms and it is read off the receipts: there are none here, so
        // no worker ever ran, which is a different sentence from work that ran and failed.
        assert!(row.detail.contains("No work was started, so nothing was landed."), "{}", row.detail);
        let failure = h.notices.0.lock().unwrap().last().unwrap().1.clone();
        assert_eq!(failure.kind, NoticeKind::Failed);
        assert!(failure.text.contains("stopped before it finished"), "{}", failure.text);
        assert!(failure.text.contains("nothing was landed"), "{}", failure.text);
        assert!(!failure.text.contains("ready for you to approve"), "{}", failure.text);

        // 1b. THE SAME OBLIGATION STATE, THE SAME WORKER EVIDENCE, one question of his on the
        //     desk — and it is `blocked` instead. This is the control that keeps case 1 from
        //     passing because everything unfinished is called failed.
        a_question_for_him(&h, "obligation-q");
        let asked = h
            .host
            .register(&h.binding, &Registration { obligation_id: "obligation-q".into(), ..registration(&h) })
            .unwrap();
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &asked.id).unwrap();
        assert_eq!(row.state, AssignmentState::Blocked, "a real question of his did not leave the job waiting on him");
        assert!(row.detail.contains("Waiting for your answer"), "{}", row.detail);

        // 2. The obligation is CLOSED: settled, with the same worker evidence — and what he
        //    hears is the OUTCOME (§52). There are no receipts in this harness, so the honest
        //    outcome is that nothing was landed, said beside "is finished" rather than instead
        //    of it: the obligation really is closed, and an assignment can close with nothing
        //    to land.
        *h.obligation.lock().unwrap() = Some(ObligationState::Settled);
        let closed = h
            .host
            .register(&h.binding, &Registration { obligation_id: "obligation-8".into(), ..registration(&h) })
            .unwrap();
        assert!(h.host.wait_for_completed(3, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &closed.id).unwrap();
        assert_eq!(row.state, AssignmentState::Settled);
        let settled = h.notices.0.lock().unwrap().last().unwrap().1.clone();
        assert_eq!(settled.kind, NoticeKind::Settled);
        assert!(settled.text.contains("is finished."), "{}", settled.text);
        assert!(settled.text.contains("nothing was landed"), "the settled sentence claimed a land it has no record of: {}", settled.text);

        // 3. The obligation cannot be read. **Never settled, never approved, and never
        //    described as a land — and since 2026-09-18 never left sitting in `Running`
        //    either.**
        //
        //    This arm used to assert `Running`, and the concern it was written to protect is
        //    intact: the SENTENCE must not claim a land the app has no record of, and it
        //    does not — it is `what_happened`, read off the receipts, which here says no work
        //    was started. What changed is the state word. The turn has ended by the time this
        //    is read, nothing re-reads a `Running` row, and the first real run of the whole
        //    flow spent 1,150 s in exactly this state with `notices: []`
        //    (`a_turn_that_has_ended_with_nothing_witnessed_fails_loudly_and_never_sits_in_running`).
        //    An assignment that cannot be confirmed finished and is not running is a failure
        //    he is told about, not a state with no watcher.
        *h.obligation.lock().unwrap() = None;
        let unknown = h
            .host
            .register(&h.binding, &Registration { obligation_id: "obligation-9".into(), ..registration(&h) })
            .unwrap();
        assert!(h.host.wait_for_completed(4, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &unknown.id).unwrap();
        assert_eq!(row.state, AssignmentState::Failed);
        assert_ne!(row.state, AssignmentState::Settled, "an unreadable record was called finished");
        assert!(!row.detail.contains("It landed"), "an unreadable record was called a land: {}", row.detail);
        assert!(!row.detail.contains("still running"), "an ended turn was called running: {}", row.detail);
        assert!(row.detail.contains("No work was started"), "{}", row.detail);

        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// Spec §1.4 and §2.1: a lease that refuses to exist is reported as what it is, not a
    /// worker that never speaks. **Since slice 5 of the 2026-10-09 plan (T3 idea 1, finished)
    /// what it is, is a job held until the back end opens**, said to him once, and never one
    /// that started: it was a failure he had to ask for again by hand.
    #[test]
    fn a_work_lease_that_refuses_to_open_is_reported_as_held_not_as_started() {
        let h = harness(5);
        struct Refusing;
        impl LeaseFactory for Refusing {
            fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
                Err(CognitionError::Io("no".into()))
            }
            fn spawn_work(&self, _b: &ThreadBinding) -> Result<Box<dyn Cognition>, CognitionError> {
                Err(CognitionError::Io("the engine release gate refused this lease".into()))
            }
        }
        let host = WorkHost::new(&h.state, Box::new(Refusing), h.notices.clone(), Arc::clone(&h.desk));
        let _runner = host.start();
        let receipt = host.register(&h.binding, &registration(&h)).unwrap();
        assert!(host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(row.state, AssignmentState::Registered);
        assert_eq!(row.detail, format!("The engine release gate refused this lease. {HELD_FOR_BACK_END}"));
        let notices = h.notices.0.lock().unwrap();
        let (thread, notice) = notices.last().unwrap();
        assert_eq!(thread, "thread-one");
        assert_eq!(notice.kind, NoticeKind::Held);
        // **This test's own name was already the invariant: "not as started".** It used to
        // assert *"stopped before it finished"*, which claims there was something to stop — the
        // lease never opened, so nothing was ever asked of the back end. Ray's candidate-.7
        // failures were every one of them this shape and every one reported the other way.
        assert!(notice.text.contains("I haven't started"), "{}", notice.text);
        assert!(notice.text.contains("it will start by itself"), "{}", notice.text);
        assert!(!notice.text.contains("stopped before it finished"), "{}", notice.text);
        // Capitalized, because `honest` now opens the card like the sentence it is — the seam
        // label it replaced used to be the first thing on the line.
        assert!(notice.text.contains("The engine release gate refused this lease"), "{}", notice.text);
        assert!(!notice.text.contains("cognition"), "a seam label reached his card: {}", notice.text);
        drop(notices);
        host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **A queue held because the back end cannot open is not open work for the update check;
    /// a queue held for sign-in still is** (an update fixes a broken build, not a sign-in).
    /// Positive control: the held row is open in the register either way.
    #[test]
    fn a_queue_held_for_a_broken_back_end_does_not_block_an_update_but_one_held_for_sign_in_does() {
        use crate::work_gate::{self, Liveness};
        let h = harness(5);
        struct Refusing;
        impl LeaseFactory for Refusing {
            fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
                Err(CognitionError::Io("no".into()))
            }
            fn spawn_work(&self, _b: &ThreadBinding) -> Result<Box<dyn Cognition>, CognitionError> {
                Err(CognitionError::Io("the build is broken".into()))
            }
        }
        let host = WorkHost::new(&h.state, Box::new(Refusing), h.notices.clone(), Arc::clone(&h.desk));
        host.start();
        let receipt = host.register(&h.binding, &registration(&h)).unwrap();
        assert!(host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(row.state, AssignmentState::Registered, "the job is held");
        assert_eq!(host.open_assignments().unwrap().len(), 1, "positive control: the row is open");
        assert_eq!(work_gate::background(&host.background_work(), &[]).0, Liveness::Clear,
            "a queue held for a broken back end blocked the update that fixes it");
        host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();

        let h = harness(5);
        every_job_settles(&h, &["work-session-one"]);
        h.host.start();
        h.signed_out.store(true, Ordering::SeqCst);
        let ids = vec![h.host.register(&h.binding, &registration(&h)).unwrap().id];
        until("the job was not held for sign-in", || held_rows(&h, &ids, HELD_FOR_SIGN_IN));
        assert_eq!(work_gate::background(&h.host.background_work(), &[]).0, Liveness::Busy,
            "a queue held for sign-in must still count as open work");
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// Spec §2.5: quit stops the work lease BY NAME, and everything open becomes
    /// `interrupted`. Nothing becomes `settled` on the way out.
    #[test]
    fn quit_stops_the_work_lease_by_name_and_settles_nothing() {
        let h = harness(3000);
        let _runner = h.host.start();
        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();
        until_live(&h.host);
        h.host.shutdown();
        assert_eq!(h.fence.shutdowns.load(Ordering::SeqCst), 1, "quit did not reach the work lease");
        let row = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(row.state, AssignmentState::Interrupted);
        // And a registration arriving after quit is refused rather than queued forever.
        assert!(h.host.register(&h.binding, &registration(&h)).is_err());
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn a_queued_assignment_that_is_stopped_never_starts() {
        let h = harness(3000);
        let _runner = h.host.start();
        let first = h.host.register(&h.binding, &registration(&h)).unwrap();
        let queued = h
            .host
            .register(&h.binding, &Registration { obligation_id: "obligation-8".into(), ..registration(&h) })
            .unwrap();
        until_live(&h.host);
        h.host.stop_assignment("depot", "thread-one", &queued.id).unwrap();
        let row = assignment::read(&h.state, "depot", "thread-one", &queued.id).unwrap();
        assert_eq!(row.state, AssignmentState::Interrupted);
        // The live one is untouched: stopping one leaves every other one alone (§5.4).
        assert_eq!(h.host.live_on("thread-one").map(|live| live.id), Some(first.id));
        assert!(!h.fence.stop_seen.load(Ordering::SeqCst));
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn the_work_lease_is_told_the_assignment_and_never_an_identifier() {
        let record = Assignment {
            schema: 1,
            id: "assignment-id-that-must-not-appear".into(),
            obligation_id: "obligation-that-must-not-appear".into(),
            seat: "work-seat:obligation-that-must-not-appear".into(),
            entity_id: "depot".into(),
            thread_id: "thread-one".into(),
            instruction_ledger_ref: "ledger:thread-one:turn-7".into(),
            instruction_sha256: "abc".into(),
            title: "landing the three branches".into(),
            handoff: String::new(),
            kind: assignment::AssignmentKind::Task,
            repositories: vec!["/fictional/project".into()],
            state: AssignmentState::Registered,
            detail: String::new(),
            registered_at_ms: 0,
            updated_at_ms: 0,
            user_stop_requested_at_ms: None,
            notices: Vec::new(),
            work_session: None,
            repository_pins: Vec::new(),
            needs_screen: false,
            waiting_sentence: false,
        };
        let brief = brief_for(&record, false, &record.title);
        assert!(brief.contains("landing the three branches"));
        assert!(brief.contains("/fictional/project"));
        assert!(!brief.contains("assignment-id-that-must-not-appear"));
        assert!(!brief.contains("work-seat:"));
        assert!(!brief.contains("obligation-that-must-not-appear"));
        assert!(!brief.contains("ledger:"));
        let input: serde_json::Value = serde_json::from_str(&brief).unwrap();
        assert_eq!(input["from"], "rich");
        assert_eq!(input["assignment"]["message"], record.title);
        assert_eq!(input["assignment"]["question"], false);
        let resumed: serde_json::Value = serde_json::from_str(&brief_for(&record, true, &record.title)).unwrap();
        assert_eq!(resumed["assignment"]["resumed_after_approval"], true);
        assert_eq!(resumed["assignment"]["user_request"], record.title);

    }

    // ---- §5.2/§5.7: his decision, and the assignment it belongs to ----------------------

    /// Readable, empty worker evidence for the work lease's own session: every worker this
    /// lease started was observed ending. Without it the settle check answers *"still
    /// running"*, which is correct and is not the state these tests are about.
    fn every_worker_observed_ending(h: &Harness) {
        let folder = h.state.join("evidence").join("work-session-one");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(".lock"), "").unwrap();
        std::fs::write(folder.join("callbacks.jsonl"), "").unwrap();
    }

    /// A work lease's scoped permissions against this harness's desk, with the standing
    /// grant `bind_work_assignment` writes: `actions_allowed: true`, `audience: "worker"`,
    /// and `turn_id` = the OBLIGATION (`ecs.rs`'s `bind_work_seat`).
    fn work_lease_desk(h: &Harness, obligation: &str) -> (PathBuf, crate::permissions::ScopedPermissions) {
        let path = h.root.join(format!("work-grant-{obligation}.json"));
        std::fs::write(
            &path,
            serde_json::json!({"version":1,"actions_allowed":true,"binding":{
                "entity_id":"depot","thread_id":"thread-one","session_id":"work-session-one",
                "turn_id":obligation,"audience":"worker","revision":1}})
            .to_string(),
        )
        .unwrap();
        (path.clone(), crate::permissions::ScopedPermissions { desk: Arc::clone(&h.desk), scope: path })
    }

    /// **THE TURN THAT LANDS THE WORK — the leg that survived both slices before it.**
    ///
    /// The green `work_lease_roundtrip` run of 2026-09-18 got a worker to commit and a
    /// reviewer to pass the commit byte for byte, and the job still did not land
    /// (`docs/verification/worker-turn-grant-2026-09-18.md` §5). Two things were wrong on
    /// this side of it and both are driven here:
    ///
    /// 1. **The continuation named the wrong next step.** One sentence served every wait,
    ///    so at the end of a REVIEW it told the back end to prepare a reviewer — for work a
    ///    reviewer had just passed. The choice is now read off the receipts.
    /// 2. **A turn that produced nothing was accepted as the answer.** The host's second
    ///    continuation came back in 1.847 s with zero tool calls because a turn the platform
    ///    injected was answered in its place. `native.rs` refuses that result outright when
    ///    the child names its turns; here the host asks once more when a turn delivered no
    ///    item at all, which no turn that prepares, reviews or lands anything can do.
    ///
    /// The third case is the negative control: with no reviewer receipt the sentence is the
    /// worker one, unchanged, so case 1 is not this function saying the same thing twice.
    #[test]
    fn after_a_passing_review_the_continuation_asks_for_the_land_and_a_silent_turn_is_asked_once_more() {
        for (reviewed, answers, asks, expected) in [
            (true, false, 2usize, REVIEW_PASSED_CONTINUATION),
            (true, true, 1usize, REVIEW_PASSED_CONTINUATION),
            (false, true, 1usize, WORKER_ENDED_CONTINUATION),
        ] {
            let h = harness(5);
            // The receipts, in the engine's own shape: a worker whose run ended with no land,
            // and (for the first two cases) a reviewer whose own receipt says it passed.
            engine_receipt(&h, "worker-1", "obligation-7", "worker", None, None);
            if reviewed {
                engine_receipt(&h, "reviewer-1", "obligation-7", "reviewer", None, Some("passed"));
            }
            // The back end's own journal: one helper the platform answered `async_launched`
            // for, still open. This is what makes the host wait rather than settle.
            witnessed(&h.state, "work-session-one");
            let journal = h.state.join("evidence").join("work-session-one").join("callbacks.jsonl");
            let row = |body: serde_json::Value| {
                serde_json::json!({"schema": 1, "callback": body}).to_string() + "\n"
            };
            std::fs::write(
                &journal,
                row(serde_json::json!({"session_id":"work-session-one","hook_event_name":"SubagentStart","agent_id":"helper-1"}))
                    + &row(serde_json::json!({"session_id":"work-session-one","hook_event_name":"PostToolUse",
                        "tool_name":"Agent","tool_response":{"isAsync":true,"status":"async_launched","agentId":"helper-1"}})),
            )
            .unwrap();
            if answers {
                *h.answer_reply.lock().unwrap() = "carrying on".to_string();
            } else {
                // The degraded case under test: a turn that streams nothing at all (C11).
                h.silent.store(true, Ordering::SeqCst);
            }
            let _runner = h.host.start();
            let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();

            // **The helper is witnessed ending only once the host is actually waiting for
            // it.** Witnessing it earlier would have the loop break before it ever asked,
            // and the test would pass by testing nothing.
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            loop {
                let row = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
                if row.detail == "A helper is doing the work." {
                    break;
                }
                assert!(std::time::Instant::now() < deadline, "the host never waited for the helper");
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            let stop = row(serde_json::json!({"session_id":"work-session-one",
                "hook_event_name":"SubagentStop","agent_id":"helper-1"}));
            let mut file = std::fs::OpenOptions::new().append(true).open(&journal).unwrap();
            std::io::Write::write_all(&mut file, stop.as_bytes()).unwrap();
            drop(file);

            assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(60)));
            let prompts = h.work_prompts.lock().unwrap().clone();
            assert_eq!(
                prompts.len(),
                1 + asks,
                "reviewed={reviewed} answers={answers}: the back end was given {} turns",
                prompts.len(),
            );
            assert!(prompts[0].contains("landing the three branches"), "the first turn is the brief");
            for ask in &prompts[1..] {
                assert_eq!(ask, expected, "reviewed={reviewed} answers={answers}");
            }
            h.host.shutdown();
            let _ = std::fs::remove_dir_all(&h.root);
        }
    }

    /// **Hunt part 1 finding 06** (richos-hq `docs/audits/2026-09-29-hunt/part-1-codex.md`): a
    /// helper that is still witnessed open when the wait's bound is reached is not a failure.
    /// The bound used to end the wait, revoke the grant the helper works under and fail the
    /// job on "nothing could be witnessed finishing it". Now the job stays `Running`, the row
    /// says the helper is taking longer than usual and that Stop ends it, the grant stays, and
    /// the helper's own `SubagentStop` carries the job on exactly as a quick helper's does.
    #[test]
    fn a_helper_still_working_past_the_wait_bound_is_waited_for_and_never_failed() {
        use crate::cognition::ObligationState;
        let h = harness(5);
        *h.obligation.lock().unwrap() = Some(ObligationState::Settled);
        h.host.set_worker_wait_budget(std::time::Duration::from_millis(50));
        // A worker's receipt, so the job is one a helper works on rather than one the back end
        // answered itself; before this fix it therefore ended in the "nothing could be
        // witnessed finishing it" failure the report names.
        engine_receipt(&h, "worker-1", "obligation-7", "worker", None, None);
        witnessed(&h.state, "work-session-one");
        let journal = h.state.join("evidence").join("work-session-one").join("callbacks.jsonl");
        let row = |body: serde_json::Value| serde_json::json!({"schema": 1, "callback": body}).to_string() + "\n";
        std::fs::write(
            &journal,
            row(serde_json::json!({"session_id":"work-session-one","hook_event_name":"SubagentStart","agent_id":"helper-1"}))
                + &row(serde_json::json!({"session_id":"work-session-one","hook_event_name":"PostToolUse",
                    "tool_name":"Agent","tool_response":{"isAsync":true,"status":"async_launched","agentId":"helper-1"}})),
        )
        .unwrap();
        *h.answer_reply.lock().unwrap() = "carrying on".to_string();
        h.host.start();
        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();

        // The host is waiting for the helper; then the bound passes with the helper still open.
        let read = || assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while read().detail != "A helper is doing the work." {
            assert!(std::time::Instant::now() < deadline, "the host never waited for the helper");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // One poll of the journal is 2 s; the bound is 50 ms. The row moves once the bound is
        // passed: to a failure before this fix, to the longer wait now.
        while read().detail == "A helper is doing the work." {
            assert!(std::time::Instant::now() < deadline, "nothing happened at the bound");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let waiting = read();
        assert_eq!(waiting.state, AssignmentState::Running, "a helper still at work failed the job: {}", waiting.detail);
        assert!(waiting.detail.contains("still doing the work"), "{}", waiting.detail);
        assert!(waiting.detail.contains("Stop"), "the way out is not named: {}", waiting.detail);
        assert_eq!(h.revoked.load(Ordering::SeqCst), 0, "the grant the helper works under was taken away");
        assert!(h.notices.0.lock().unwrap().iter().all(|(_, n)| n.kind != NoticeKind::Failed));

        // The helper finishes: the ordinary continuation, and the job ends on its evidence.
        let stop = row(serde_json::json!({"session_id":"work-session-one","hook_event_name":"SubagentStop","agent_id":"helper-1"}));
        let mut file = std::fs::OpenOptions::new().append(true).open(&journal).unwrap();
        std::io::Write::write_all(&mut file, stop.as_bytes()).unwrap();
        drop(file);
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(60)));
        let prompts = h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(), 2, "{prompts:?}");
        assert_eq!(prompts[1], WORKER_ENDED_CONTINUATION);
        let done = read();
        assert_eq!(done.state, AssignmentState::Settled, "{}", done.detail);
        assert!(h.notices.0.lock().unwrap().iter().all(|(_, n)| n.kind != NoticeKind::Failed));
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **Walk 6 (2026-10-05, `walk-10608166d53f`, esc-20261005T151403Z-2f5c3737): a job was
    /// written `failed` while its back end was still working on it.** The back end had
    /// messaged its reviewer and ended its turn with the engine's declared stop
    /// `stop-declared: waiting-on-teammate`. The reviewer's last `SubagentStop` was already in
    /// the journal, so nothing read as open, the loop ended, the grant the reviewer works under
    /// was revoked and the job failed ("The work ran and nothing was landed.") — and every
    /// hand-back of the reviewer's verdict after that was refused, because its grant was gone.
    ///
    /// Now the declaration holds the job open: it stays `Running` with its grant, the next
    /// helper run to end brings the back end its continuation, and the job ends on its
    /// evidence. On the unfixed host this test fails at the first assertion: the job is
    /// `Failed` before any helper could answer.
    #[test]
    fn a_back_end_that_declares_it_waits_on_its_helper_is_waited_for_and_never_failed() {
        use crate::cognition::ObligationState;
        let h = harness(5);
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        engine_receipt(&h, "worker-1", "obligation-7", "worker", None, None);
        engine_receipt(&h, "reviewer-1", "obligation-7", "reviewer", None, Some("changes-requested"));
        // The journal as walk 6 left it at that turn's end: the reviewer launched in the
        // background and its run already ended. Nothing is open.
        witnessed(&h.state, "work-session-one");
        let journal = h.state.join("evidence").join("work-session-one").join("callbacks.jsonl");
        let row = |body: serde_json::Value| serde_json::json!({"schema": 1, "callback": body}).to_string() + "\n";
        std::fs::write(
            &journal,
            row(serde_json::json!({"session_id":"work-session-one","hook_event_name":"SubagentStart","agent_id":"reviewer-a"}))
                + &row(serde_json::json!({"session_id":"work-session-one","hook_event_name":"PostToolUse",
                    "tool_name":"Agent","tool_response":{"isAsync":true,"status":"async_launched","agentId":"reviewer-a"}}))
                + &row(serde_json::json!({"session_id":"work-session-one","hook_event_name":"SubagentStop","agent_id":"reviewer-a"})),
        )
        .unwrap();
        // The back end's two turns: walk 6's own last words, then its continuation.
        h.replies.lock().unwrap().extend([
            "Nothing has landed yet. I've asked the reviewer again for the specific defect, and I'll act \
             as soon as it answers.\n\nstop-declared: waiting-on-teammate — The reviewer has been asked to \
             state its blocking defect, and the land depends on its answer."
                .to_string(),
            "The reviewer passed it; landed.".to_string(),
        ]);
        h.host.start();
        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();

        let read = || assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            let now = read();
            if now.detail == "A helper is doing the work."
                || !matches!(now.state, AssignmentState::Registered | AssignmentState::Preparing | AssignmentState::Running)
            {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "nothing happened: {:?} {}", now.state, now.detail);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let waiting = read();
        assert_eq!(waiting.state, AssignmentState::Running,
            "the job was ended while its back end waited on its helper: {}", waiting.detail);
        assert_eq!(h.revoked.load(Ordering::SeqCst), 0, "the grant its helper works under was taken away");
        assert!(h.notices.0.lock().unwrap().iter().all(|(_, n)| n.kind != NoticeKind::Failed));

        // The reviewer answers: its next run ends, the back end is handed its continuation, and
        // the job ends on what is then witnessed.
        *h.obligation.lock().unwrap() = Some(ObligationState::Settled);
        let stop = row(serde_json::json!({"session_id":"work-session-one","hook_event_name":"SubagentStop","agent_id":"reviewer-a"}));
        let mut file = std::fs::OpenOptions::new().append(true).open(&journal).unwrap();
        std::io::Write::write_all(&mut file, stop.as_bytes()).unwrap();
        drop(file);
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(60)));
        let prompts = h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(), 2, "{prompts:?}");
        assert_eq!(prompts[1], WORKER_ENDED_CONTINUATION);
        let done = read();
        assert_eq!(done.state, AssignmentState::Settled, "{}", done.detail);
        assert!(h.notices.0.lock().unwrap().iter().all(|(_, n)| n.kind != NoticeKind::Failed));
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **The declared wait is bounded, and only the declaration opens it** — the two controls
    /// for the test above. A turn that ends WITHOUT the declaration is settled at once, exactly
    /// as before; one that declares it and sees no helper run end within the bound is settled
    /// on what can be witnessed, with its grant given back. Neither waits forever on a word.
    #[test]
    fn a_declared_wait_with_no_helper_answer_ends_at_its_bound_and_no_declaration_never_waits() {
        use crate::cognition::ObligationState;
        for declared in [true, false] {
            let h = harness(5);
            *h.obligation.lock().unwrap() = Some(ObligationState::Open);
            h.host.set_worker_wait_budget(std::time::Duration::from_millis(50));
            engine_receipt(&h, "worker-1", "obligation-7", "worker", None, None);
            witnessed(&h.state, "work-session-one");
            let reply = if declared {
                "I asked the reviewer again.\n\nstop-declared: waiting-on-teammate — The reviewer has been \
                 asked to state its blocking defect, and the land depends on its answer."
            } else {
                "I asked the reviewer again and will act when it answers."
            };
            h.replies.lock().unwrap().push_back(reply.to_string());
            h.host.start();
            let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();
            assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(60)), "declared={declared}");
            let row = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
            assert_eq!(row.state, AssignmentState::Failed, "declared={declared}: {}", row.detail);
            assert_eq!(h.work_prompts.lock().unwrap().len(), 1, "declared={declared}");
            assert!(h.revoked.load(Ordering::SeqCst) >= 1, "declared={declared}: the grant was kept");
            h.host.shutdown();
            std::fs::remove_dir_all(&h.root).unwrap();
        }
    }

    /// **Nightly 56's hire walk (2026-10-14, `walk-1606409b3615`, esc-20261014T202736Z-38bde06b):
    /// the hiring settled while Beth was still being written.** Dean's consult hit a
    /// `SubagentStop` without handing his answer back, the provider ran him on, and the back
    /// end's next turn ended "Dean, who is still writing Beth's profile" with the provider's own
    /// `Stop` listing Dean `running`. The host read the first `SubagentStop` as his end, settled
    /// the job "Answered." with only Will saved, and Dean's hand-back was refused.
    ///
    /// Now that `Stop` keeps Dean open: the job stays `Running` with its grant, and his real
    /// end brings the back end the turn in which Beth is saved. On the unfixed host this fails
    /// at the first assertion: the job is `Settled` before Dean ends.
    #[test]
    fn a_helper_the_provider_still_runs_after_its_first_subagent_stop_holds_the_job_open() {
        let h = harness(5);
        witnessed(&h.state, "work-session-one");
        let still_running = serde_json::json!({"schema": 1, "callback": {"session_id":"work-session-one",
            "hook_event_name":"Stop","stop_hook_active":false,
            "background_tasks":[{"id":"dean-1","type":"subagent","status":"running","description":"Write Beth, Bookkeeper"}]}})
            .to_string() + "\n";
        // The journal as the walk left it at that turn's end, in the provider's order.
        journal(&h, &(helper_started("dean-1") + &helper_stopped("dean-1") + &still_running));
        h.replies.lock().unwrap().extend([
            "Clark has finished researching the Bookkeeper role. I've passed that research to Dean, who is \
             still writing Beth's profile. Beth isn't saved yet, so she isn't on your team yet."
                .to_string(),
            "Beth saved. She is on your team now as your Bookkeeper.".to_string(),
        ]);
        h.host.start();
        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();

        let read = || assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            let now = read();
            if now.detail == "A helper is doing the work."
                || !matches!(now.state, AssignmentState::Registered | AssignmentState::Preparing | AssignmentState::Running)
            {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "nothing happened: {:?} {}", now.state, now.detail);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let waiting = read();
        assert_eq!(waiting.state, AssignmentState::Running,
            "the hiring was settled while a teammate was still being written: {}", waiting.detail);
        assert_eq!(h.revoked.load(Ordering::SeqCst), 0, "the grant Dean hands back under was taken away");

        // Dean's real end: the back end gets its turn, and the job ends on what it then says.
        journal(&h, &helper_stopped("dean-1"));
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(60)));
        assert_eq!(h.work_prompts.lock().unwrap().len(), 2);
        let done = read();
        assert_eq!(done.state, AssignmentState::Settled, "{}", done.detail);
        let told: Vec<_> = h.notices.0.lock().unwrap().iter().map(|(_, n)| n.text.clone()).collect();
        assert!(told.iter().any(|t| t.contains("Beth saved.")), "{told:?}");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// A long sequence stays open while its helper is still active. Beth is
    /// still being fitted** (handover review rv-20261014T210558Z-b7ebe6ad-571c: nightly 56's
    /// hire needed exactly six). Seven fitting attempts, each launched in the turn after the
    /// last one ended; the seventh is launched in the sixth continuation. The host used to stop
    /// there, revoke the grant and close the job with Beth unsaved; now it waits for that
    /// attempt too, and the job ends on the turn after it, in which Beth is saved.
    #[test]
    fn a_hiring_still_fitting_a_teammate_after_six_waits_stays_open_until_she_is_saved() {
        let attempts = 8; // Beyond the former six-continuation cutoff.
        let h = harness(5);
        witnessed(&h.state, "work-session-one");
        h.replies.lock().unwrap().extend(
            (0..attempts).map(|n| format!("Dean is fitting Beth, your Bookkeeper (attempt {}).", n + 1))
                .chain(["Beth saved. She is on your team now as your Bookkeeper.".to_string()]),
        );
        let fitting = |n: usize| format!("dean-fit-{n}");
        journal(&h, &helper_started(&fitting(0)));
        h.host.start();
        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();
        let read = || assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        let turns_reach = |n: usize| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            while h.work_prompts.lock().unwrap().len() < n {
                assert!(std::time::Instant::now() < deadline, "turn {n} never came: {:?} {}", read().state, read().detail);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        };
        for n in 0..attempts {
            turns_reach(n + 1);
            if n > 0 {
                // The turn after the last attempt ended launches the next one.
                journal(&h, &helper_started(&fitting(n)));
                h.first_item_gate.release();
            }
            // The host reads the journal at once when a turn ends and then every
            // WORKER_WAIT_POLL (2 s); one second in, it has seen this attempt open.
            std::thread::sleep(std::time::Duration::from_secs(1));
            let now = read();
            assert_eq!(now.state, AssignmentState::Running,
                "the hiring closed while fitting attempt {} was still running: {}", n + 1, now.detail);
            assert_eq!(h.revoked.load(Ordering::SeqCst), 0,
                "the grant fitting attempt {} saves under was taken away", n + 1);
            h.first_item_gate.shut();
            journal(&h, &helper_stopped(&fitting(n)));
        }
        // The turn after the last attempt: Beth is saved, nothing is open, and the job ends.
        turns_reach(attempts + 1);
        h.first_item_gate.release();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(60)));
        assert_eq!(h.work_prompts.lock().unwrap().len(), attempts + 1);
        let done = read();
        assert_eq!(done.state, AssignmentState::Settled, "{}", done.detail);
        let told: Vec<_> = h.notices.0.lock().unwrap().iter().map(|(_, n)| n.text.clone()).collect();
        assert!(told.iter().any(|t| t.contains("Beth saved.")), "{told:?}");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    // ---- the weekly switch handoff (richos-hq docs/plans/2026-10-07-weekly-switch-handoff.md) --

    const DECLARED_WAIT: &str = "I've asked the helper to go on.\n\nstop-declared: waiting-on-teammate — \
        The helper is working, and the land depends on it.";

    /// The back end's own journal rows for a helper run: started in the background, or ended.
    fn helper_started(agent: &str) -> String {
        let row = |body: serde_json::Value| serde_json::json!({"schema": 1, "callback": body}).to_string() + "\n";
        row(serde_json::json!({"session_id":"work-session-one","hook_event_name":"SubagentStart","agent_id":agent}))
            + &row(serde_json::json!({"session_id":"work-session-one","hook_event_name":"PostToolUse",
                "tool_name":"Agent","tool_response":{"isAsync":true,"status":"async_launched","agentId":agent}}))
    }
    fn helper_stopped(agent: &str) -> String {
        serde_json::json!({"schema": 1, "callback": {"session_id":"work-session-one",
            "hook_event_name":"SubagentStop","agent_id":agent}}).to_string() + "\n"
    }
    /// Append rows to the journal in one write, as the hook appends one callback.
    fn journal(h: &Harness, rows: &str) {
        let path = h.state.join("evidence").join("work-session-one").join("callbacks.jsonl");
        let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
        std::io::Write::write_all(&mut file, rows.as_bytes()).unwrap();
    }
    /// The marker the quota gate writes when it orders `agent` on `account` (`quota::gate`).
    fn ordered(h: &Harness, agent: &str, account: &str) {
        let folder = h.state.join(crate::quota::gate::HANDOFFS_DIR);
        std::fs::create_dir_all(&folder).unwrap();
        let marker = crate::quota::gate::Handoff { agent: agent.into(), session: "work-session-one".into(),
            account: account.into(), at: crate::util::now_millis(), continued_at: None, committed_at: None, points: None };
        std::fs::write(folder.join(format!("{agent}.json")), serde_json::to_vec(&marker).unwrap()).unwrap();
    }
    fn continued(h: &Harness, agent: &str) -> Option<u64> {
        let path = h.state.join(crate::quota::gate::HANDOFFS_DIR).join(format!("{agent}.json"));
        serde_json::from_slice::<crate::quota::gate::Handoff>(&std::fs::read(path).unwrap()).unwrap().continued_at
    }
    /// A worker's receipt whose run was `agent` (the engine joins the agent id to it).
    fn worker_run(h: &Harness, id: &str, agent: &str) {
        engine_receipt(h, id, "obligation-7", "worker", None, None);
        use sha2::{Digest, Sha256};
        let path = h.state.join("work-receipts").join(format!("{:x}", Sha256::digest(b"[\"depot\",\"thread-one\"]")))
            .join(format!("{id}.json"));
        let mut row: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        row["agent_id"] = agent.into();
        std::fs::write(path, serde_json::to_vec(&row).unwrap()).unwrap();
    }
    /// A weekly reading of `account`, as its lease streams one (`rate_limit_event`), taken now
    /// and merged before the next turn (`quota::Service::before_turn`), which decides the
    /// switch on it. The probe's own refresh would not read again inside its five-second
    /// double-click cooldown.
    fn weekly_streamed(quota: &crate::quota::Service, account: &str, used: f64, resets_in_hours: u64) {
        std::thread::sleep(std::time::Duration::from_millis(2));
        let now = crate::util::now_millis();
        quota.before_turn(account, Some((vec![crate::quota::Window { id: "seven_day".into(), label: "Weekly".into(),
            used_percent: used, resets_at: Some(now + resets_in_hours * 3_600_000), duration_ms: 168 * 3_600_000 }], now)));
    }
    fn until(what: &str, done: impl Fn() -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !done() {
            assert!(std::time::Instant::now() < deadline, "{what}");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// **Weekly-switch plan §2 and §7, slice 3: the handoff continuation, on both paths, sent
    /// only after the switch.** Two accounts. The back end runs on Account 1 and a helper of
    /// its (`helper-1`, the worker on receipt `worker-1`) is out. Then Account 1 reaches its
    /// weekly point: Work is put in use, Account 1 is leaving, the gate orders `helper-1` (its
    /// marker), and a command the back end started defers the switch. `helper-1` ends:
    /// - path 1: it was open, and `wait_for_owned_workers` sees it end;
    /// - path 2: nothing was open and the back end said it is waiting on a helper, and
    ///   `wait_for_a_helper_run_to_end` sees its run end.
    ///
    /// Either way the continuation is the handoff one, naming the worker's receipt, its marker
    /// is used when it is chosen, and it is NOT sent while the switch waits on the command.
    /// Once the command ends, the run moves to Work and the handoff is sent there.
    #[test]
    #[cfg(unix)]
    fn a_handoff_continuation_is_chosen_on_both_paths_and_sent_only_after_the_switch() {
        use crate::lease_commands::CommandReading;
        for declared in [false, true] {
            let h = harness(5);
            *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
            witnessed(&h.state, "work-session-one");
            worker_run(&h, "worker-1", "helper-1");
            let quota = Arc::new(crate::quota::Service::open(&h.root).unwrap());
            let work = quota.accounts.add("Work").unwrap();
            let bin = crate::quota::tests::fake_claude(&h.root);
            crate::quota::tests::usage(&h.root.join("usage-1.json"), 10., 50., "2099-01-05T00:00:00Z");
            crate::quota::tests::usage(&work.folder.clone().unwrap().join("usage.json"), 5., 10., "2099-01-06T00:00:00Z");
            quota.refresh(&bin, true);
            h.fill.lock().unwrap().quota = Some(quota.clone());
            h.host.set_quota(quota.clone());
            if declared {
                // A helper run that has already ended, and a back end that says it waits.
                journal(&h, &(helper_started("helper-0") + &helper_stopped("helper-0")));
                h.replies.lock().unwrap().push_back(DECLARED_WAIT.into());
            } else {
                journal(&h, &helper_started("helper-1"));
            }
            h.host.start();
            let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();
            let read = || assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
            until("the host never waited for a helper", || read().detail == "A helper is doing the work.");

            weekly_streamed(&quota, "1", 99., 48);
            assert_eq!(quota.lease_account().id, "2");
            assert_eq!(quota.view().leaving, vec!["1".to_string()]);
            ordered(&h, "helper-1", "1");
            *h.commands.lock().unwrap() = Some(CommandReading::Running(1));
            journal(&h, &if declared { helper_started("helper-1") + &helper_stopped("helper-1") } else { helper_stopped("helper-1") });

            until("the handoff was never chosen", || continued(&h, "helper-1").is_some());
            // The host has checked the deferred switch twice since the choice: once before the
            // turn, as before every continuation, and again while it waits for the switch.
            let reads = h.fill.lock().unwrap().command_reads;
            until("the host never checked the switch again", || h.fill.lock().unwrap().command_reads >= reads + 2);
            assert_eq!(h.work_prompts.lock().unwrap().len(), 1, "declared={declared}: sent on the account being left");
            *h.commands.lock().unwrap() = Some(CommandReading::Clear);
            assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(60)), "declared={declared}");
            let prompts = h.work_prompts.lock().unwrap().clone();
            assert_eq!(prompts.len(), 2, "declared={declared}: {prompts:?}");
            assert!(prompts[1].starts_with(HANDOFF_CONTINUATION_HEAD), "declared={declared}: {}", prompts[1]);
            assert!(prompts[1].contains("- the worker on receipt `worker-1`"), "{}", prompts[1]);
            assert!(prompts[1].ends_with(HANDOFF_CONTINUATION_TAIL), "{}", prompts[1]);
            assert_eq!(h.fill.lock().unwrap().turns, vec![Some("1".to_string()), Some("2".to_string())],
                "declared={declared}: the handoff went out only on Work");
            h.host.shutdown();
            std::fs::remove_dir_all(&h.root).unwrap();
        }
    }

    /// **Plan §2, step 1: on the path where one run's end is the signal, a handed-off helper
    /// still open is not named.** The back end said it waits on a helper; then the ordered
    /// `helper-1` starts again (still committing) while another run ends. The continuation is
    /// the worker one and `helper-1`'s marker stays unused. When `helper-1`'s own run ends,
    /// the next continuation is the handoff one, naming it.
    #[test]
    fn a_handed_off_helper_still_open_is_named_only_at_its_own_end() {
        let h = harness(5);
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
        witnessed(&h.state, "work-session-one");
        worker_run(&h, "worker-1", "helper-1");
        journal(&h, &(helper_started("helper-0") + &helper_stopped("helper-0")));
        h.replies.lock().unwrap().extend([DECLARED_WAIT.to_string(), DECLARED_WAIT.to_string()]);
        h.host.start();
        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();
        let read = || assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        until("the host never waited for a helper", || read().detail == "A helper is doing the work.");

        ordered(&h, "helper-1", "1");
        journal(&h, &(helper_started("helper-1") + &helper_stopped("helper-0")));
        until("no continuation after the run that ended", || h.work_prompts.lock().unwrap().len() == 2);
        assert_eq!(h.work_prompts.lock().unwrap()[1], WORKER_ENDED_CONTINUATION);
        assert_eq!(continued(&h, "helper-1"), None, "a helper still open was named before its end");

        journal(&h, &helper_stopped("helper-1"));
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(60)));
        let prompts = h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(), 3, "{prompts:?}");
        assert!(prompts[2].starts_with(HANDOFF_CONTINUATION_HEAD) && prompts[2].contains("`worker-1`"), "{}", prompts[2]);
        assert!(continued(&h, "helper-1").is_some());
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **Plan §2, step 5 and Frank's M2: with one account, the handoff waits for the reset and
    /// is announced once.** The one account reaches its weekly point while `helper-1` works:
    /// it is ordered and ends, and the run waits in the quota hold, nothing sent. At the reset
    /// the handoff continuation goes out, and the back end starts a successor (`helper-2`) in
    /// that turn. When the successor ends, the next continuation is the worker one (a reviewer
    /// next), not the handoff again: the marker was used when the handoff was chosen.
    #[test]
    #[cfg(unix)]
    fn with_one_account_the_handoff_waits_for_the_reset_and_the_successor_s_end_is_a_worker_end() {
        let h = harness(5);
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
        witnessed(&h.state, "work-session-one");
        worker_run(&h, "worker-1", "helper-1");
        let quota = Arc::new(crate::quota::Service::open(&h.root).unwrap());
        let bin = crate::quota::tests::fake_claude(&h.root);
        crate::quota::tests::usage(&h.root.join("usage-1.json"), 10., 50., "2099-01-05T00:00:00Z");
        quota.refresh(&bin, true);
        h.fill.lock().unwrap().quota = Some(quota.clone());
        h.host.set_quota(quota.clone());
        journal(&h, &helper_started("helper-1"));
        h.replies.lock().unwrap().extend(["A worker is on it.".to_string(), DECLARED_WAIT.to_string()]);
        h.host.start();
        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();
        let read = || assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        until("the host never waited for a helper", || read().detail == "A helper is doing the work.");

        weekly_streamed(&quota, "1", 99., 48);
        assert_eq!(quota.view().leaving, vec!["1".to_string()]);
        assert!(matches!(quota.view().admission, crate::quota::Admission::Held { .. }));
        ordered(&h, "helper-1", "1");
        journal(&h, &helper_stopped("helper-1"));
        until("the run never waited for the allowance", || read().state == AssignmentState::WaitingForQuota);
        assert_eq!(h.work_prompts.lock().unwrap().len(), 1, "nothing is sent while the week is held");

        h.first_item_gate.shut();
        // The reset: a new week, read at 5%.
        weekly_streamed(&quota, "1", 5., 7 * 24);
        until("the handoff never went out after the reset", || h.work_prompts.lock().unwrap().len() == 2);
        assert!(h.work_prompts.lock().unwrap()[1].starts_with(HANDOFF_CONTINUATION_HEAD));
        journal(&h, &helper_started("helper-2"));
        h.first_item_gate.release();
        journal(&h, &helper_stopped("helper-2"));

        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(60)));
        let prompts = h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(), 3, "{prompts:?}");
        assert_eq!(prompts[2], WORKER_ENDED_CONTINUATION, "the handoff was announced again");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    #[test]
    fn the_declaration_is_read_where_the_engine_reads_it() {
        assert!(declares_waiting_on_a_helper("Done for now.\nstop-declared: waiting-on-teammate — reason here"));
        assert!(declares_waiting_on_a_helper("> **stop-declared:  Waiting-On-Teammate — reason"));
        assert!(!declares_waiting_on_a_helper("I am waiting on a teammate."));
        assert!(!declares_waiting_on_a_helper("stop-declared: nothing-unblocked — all done here"));
        assert!(!declares_waiting_on_a_helper("he wrote stop-declared: waiting-on-teammate mid-line"));
    }

    /// **After a consult ends, the back end is told to read its answer, not to review it**
    /// (plan §3, slice 3) — and an assignment answered with a consult's help is still one no
    /// helper worked on, so it closes on the back end's report. The newest end decides the
    /// sentence: a worker that ends after the consult gets the worker one.
    #[test]
    fn a_consult_that_ended_is_read_not_reviewed_and_leaves_the_answer_close_open() {
        let h = harness(5);
        let receipt = assignment::register_kind(&h.state, &registration(&h), assignment::AssignmentKind::Task).unwrap();
        let record = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        let ended = |id: &str, role: &str, at: f64| {
            engine_receipt(&h, id, "obligation-7", role, None, None);
            use sha2::{Digest, Sha256};
            let path = h.state.join("work-receipts")
                .join(format!("{:x}", Sha256::digest(b"[\"depot\",\"thread-one\"]"))).join(format!("{id}.json"));
            let mut row: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            row["end_observation"] = serde_json::json!({"at": at, "signal": "SubagentStop"});
            if role == "consult" {
                row["request"]["repo"] = serde_json::Value::Null;
                row["consult_answer"] = serde_json::json!({"message": "The answer.", "answered": true});
            }
            std::fs::write(&path, serde_json::to_vec(&row).unwrap()).unwrap();
        };
        ended("consult-1", "consult", 100.0);
        assert_eq!(h.host.continuation_after_a_helper_ended(&record, "work-session-one"), CONSULT_ENDED_CONTINUATION);
        assert!(h.host.no_helper_ever_ran(&record), "a consult was counted as a helper that worked on it");
        ended("worker-1", "worker", 200.0);
        assert_eq!(h.host.continuation_after_a_helper_ended(&record, "work-session-one"), WORKER_ENDED_CONTINUATION);
        assert!(!h.host.no_helper_ever_ran(&record));
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **Rich's waiting sentence is shown while the teammate works and is never part of his
    /// answer, and the answer is delivered exactly as the back end wrote it** (second review
    /// rv-20261015T100221Z-67c05812-8b55, both of its fixtures: two consults one after the
    /// other, and an answer that opens with "Will recommends working on this"). The back end
    /// says who is working on what in the engine's own `stop-declared: waiting-on-teammate`
    /// line (`DESKTOP.md`, the consult paragraph); the row shows that sentence while the
    /// teammate works, and his notice is every other word it wrote, paragraph for paragraph.
    #[test]
    fn the_waiting_sentence_shows_while_the_teammate_works_and_the_answer_is_delivered_as_written() {
        const WILL_WORKS: &str = "I've asked Will for advice, and he's working on it now.";
        const BETH_WORKS: &str = "I've asked Beth for the budget, and she's working on it now.";
        const WILL_SAID: &str = "Will recommends working on this in two stages: interview five clients, then test \
                                 the draft with ten prospects.\n\nHe would start with the clients you know best.";
        const BETH_SAID: &str = "Beth recommends a budget of 500.";
        let h = harness(5);
        witnessed(&h.state, "work-session-one");
        let consult = |id: &str, at: f64| {
            engine_receipt(&h, id, "obligation-7", "consult", None, None);
            use sha2::{Digest, Sha256};
            let path = h.state.join("work-receipts")
                .join(format!("{:x}", Sha256::digest(b"[\"depot\",\"thread-one\"]"))).join(format!("{id}.json"));
            let mut row: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            row["request"]["repo"] = serde_json::Value::Null;
            row["end_observation"] = serde_json::json!({"at": at, "signal": "SubagentStop"});
            row["consult_answer"] = serde_json::json!({"message": "answered", "answered": true});
            std::fs::write(&path, serde_json::to_vec(&row).unwrap()).unwrap();
        };
        consult("consult-will", 100.0);
        journal(&h, &helper_started("will-1"));
        h.replies.lock().unwrap().extend([
            format!("stop-declared: waiting-on-teammate — {WILL_WORKS}"),
            format!("{WILL_SAID}\n\nstop-declared: waiting-on-teammate — {BETH_WORKS}"),
            BETH_SAID.to_string(),
        ]);
        h.host.start();
        let receipt = h.host
            .register(&h.binding, &Registration { repositories: Vec::new(), ..registration(&h) })
            .unwrap();
        let read = || assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        // What his row says while a teammate works: the sentence, or after ten seconds whatever
        // it says instead, read once the host is waiting either way and judged at the end.
        // His row shows that sentence in both views, for a task and a question alike: the
        // shell's `forYou` is [`for_him`] (the review of 3b91f1223, finding 1, whose fixture
        // read `waitingSentenceShown:false`).
        let shown_while_working = |sentence: &str| {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                let row = read();
                if row.detail == sentence || std::time::Instant::now() > deadline {
                    return (row.detail.clone(), for_him(&row));
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        };
        let while_will = shown_while_working(WILL_WORKS);
        h.first_item_gate.shut();
        journal(&h, &helper_stopped("will-1"));
        until("no turn after Will's answer", || h.work_prompts.lock().unwrap().len() == 2);
        consult("consult-beth", 200.0);
        journal(&h, &helper_started("beth-1"));
        h.first_item_gate.release();
        let while_beth = shown_while_working(BETH_WORKS);
        journal(&h, &helper_stopped("beth-1"));
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(60)));
        let prompts = h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(), 3, "{prompts:?}");
        assert_eq!(&prompts[1..], [CONSULT_ENDED_CONTINUATION, CONSULT_ENDED_CONTINUATION]);
        let told: Vec<_> = h.notices.0.lock().unwrap().iter().map(|(_, n)| n.text.clone()).collect();
        assert!(told.iter().any(|t| t == WILL_SAID), "the first result was not passed to Rich while work continued: {told:?}");
        assert!(told.iter().any(|t| t == BETH_SAID), "the last result was not passed to Rich: {told:?}");
        let all = told.join("\n");
        assert!(!all.contains("working on it now") && !all.contains("stop-declared"),
            "a waiting sentence is in his answer: {told:?}");
        assert_eq!(while_will, (WILL_WORKS.to_string(), true), "Rich's sentence was not shown while Will worked");
        assert_eq!(while_beth, (BETH_WORKS.to_string(), true), "Rich's sentence was not shown while Beth worked");
        let done = read();
        assert!(!for_him(&done), "the settled row's own record is shown as his: {}", done.detail);
        assert_eq!(done.state, AssignmentState::Settled, "{}", done.detail);
        assert!(done.detail != WILL_WORKS && done.detail != BETH_WORKS, "the waiting sentence outlived the answer: {}", done.detail);
        // And when a background command's report turn starts the consult, that turn REPLACES the
        // account ("it has started"), and its declared wait stays out of his answer all the same
        // (second review rv-20261015T110858Z-a5f8a28b-a1ab, fixture command_then_consult.py).
        let mut account = "The analysis is running.".to_string();
        fold_turn(&mut account, "The analysis found 12 qualified leads.\n\nstop-declared: waiting-on-teammate — \
            Will is working on the outreach recommendation now.");
        fold_turn(&mut account, "Will recommends starting with the three largest leads.");
        assert_eq!(account, "Will recommends starting with the three largest leads.",
            "earlier progress was concatenated into a newer report");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **A command's report that holds ONLY its declared wait still replaces "it has started"**
    /// (handover review rv-20261015T120013Z-7ade25c4-de05, fixture declaration_only_report.py:
    /// his answer read "The analysis is running." above Will's answer). A turn that said
    /// nothing at all still leaves the account as it was.
    #[test]
    fn a_report_that_only_declares_its_wait_replaces_the_command_kickoff() {
        let mut account = "The analysis is running.".to_string();
        fold_turn(&mut account, "stop-declared: waiting-on-teammate — Will is working on the outreach recommendation now.");
        fold_turn(&mut account, "Will recommends starting with the three largest leads.");
        assert_eq!(account, "Will recommends starting with the three largest leads.",
            "a finished command is still reported as running above the answer");
        let mut account = "The analysis is running.".to_string();
        fold_turn(&mut account, "  \n");
        assert_eq!(account, "The analysis is running.", "a turn that said nothing emptied the account");
    }

    /// **A work receipt as the ENGINE writes it**, in the partition the engine writes it to
    /// (`mega-lander/app.py:81`), so `what_happened` reads the real shape rather than one
    /// invented here. `landed` is the branch the ref moved on, or `None` for a run that ended
    /// without a land.
    fn engine_receipt(h: &Harness, id: &str, obligation: &str, role: &str, landed: Option<&str>, verdict: Option<&str>) {
        use sha2::{Digest, Sha256};
        let partition = format!("{:x}", Sha256::digest(b"[\"depot\",\"thread-one\"]"));
        let folder = h.state.join("work-receipts").join(partition);
        std::fs::create_dir_all(&folder).unwrap();
        let mut row = serde_json::json!({"schema":1,"id":id,
            "binding":{"entity_id":"depot","thread_id":"thread-one"},
            "request":{"title":"landing the three branches","repo":"/fictional/project",
                       "role":role,"obligation_id":obligation},
            "status":"run-ended"});
        if let Some(branch) = landed {
            row["integration"] = serde_json::json!({"verified":true,"reviewer_id":"reviewer-1",
                "branch":branch,"commit":"deadbeef","cleanup_pending":[]});
            row["status"] = "integrated".into();
        }
        if let Some(verdict) = verdict {
            row["review_observation"] = serde_json::json!({"valid":true,"report":{"verdict":verdict}});
        }
        std::fs::write(folder.join(format!("{id}.json")), serde_json::to_vec(&row).unwrap()).unwrap();
    }

    /// **WHAT HE HEARS IS THE OUTCOME — the CEO's ruling §52, end to end through the host.**
    ///
    /// *"There's nothing that ever not lands on its own here in the terminal … So, yes,
    /// always land on its own."* So a finished job's sentence names the branch, the repository
    /// and the reviewer's verdict, read off the receipt the engine wrote — and the receipt's
    /// row carries the same words, so the surface and the spoken notice can never describe one
    /// land two ways.
    ///
    /// **The negative half is in the same test and it is the half that can go wrong quietly**:
    /// the identical assignment with an identical closed obligation and NO land on its
    /// receipts says nothing was landed. Without it, a sentence that always claimed a land
    /// would pass.
    /// **THE MOVED HALF OF THE READINESS GATE, AT THE LEVEL HE ACTUALLY SEES (step 3a).**
    ///
    /// Until 2026-09-18 `bind_work_assignment` asserted the engine plugin, the work tools and
    /// automatic permission checks from the child's `system/init` frame — which arrives with
    /// the first TURN — on a lease that had not taken one. Every background job therefore
    /// failed at the bind, before running: candidate .7 failed two, 6.545 s and 7.486 s after
    /// registration, `work_session: null` on both. The bind now refuses only a REPORTED
    /// absence, so the assignment reaches its turn; this is where a lease that turns out not
    /// to have been equipped is failed instead, by name.
    ///
    /// **The obligation is deliberately `Settled` here**, which is the most favorable reading
    /// the settle check can return. A build that consulted readiness after the settle reading,
    /// or not at all, would tell him this job finished. The sentence has to win over that, or
    /// the loudness cell K4 exists to preserve has not actually moved anywhere.
    #[test]
    fn a_lease_that_reports_one_turn_late_that_it_had_no_engine_plugin_fails_the_job_by_name() {
        use crate::cognition::ObligationState;
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(ObligationState::Settled);
        *h.readiness.lock().unwrap() = Some("The desktop engine plugin did not load".to_string());
        let _runner = h.host.start();

        let job = h.host.register(&h.binding, &registration(&h)).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));

        // It REACHED its back end — the whole point of the fix. A bind that refused would
        // never have bound anything.
        assert_eq!(h.bound.lock().unwrap().len(), 1, "the assignment never reached its lease");

        let row = assignment::read(&h.state, "depot", "thread-one", &job.id).unwrap();
        assert_eq!(row.state, AssignmentState::Failed, "a lease with no engine plugin must not settle");
        assert!(
            row.detail.contains("The desktop engine plugin did not load"),
            "the row does not name what was missing: {}",
            row.detail
        );
        assert!(!row.detail.contains("It landed"), "a land was claimed: {}", row.detail);
        let notice = h.notices.0.lock().unwrap().last().unwrap().1.clone();
        assert_eq!(notice.kind, NoticeKind::Failed);
        assert!(
            notice.text.contains("The desktop engine plugin did not load"),
            "he was not told which piece was missing: {}",
            notice.text
        );

        // **THE CONTROL, on the same harness and the same settled obligation**: an equipped
        // lease still settles. Without this the assertion above would pass for a build that
        // failed every job, which is the defect wearing the fix's clothes.
        *h.readiness.lock().unwrap() = None;
        let fine = h
            .host
            .register(&h.binding, &Registration { obligation_id: "obligation-8".into(), ..registration(&h) })
            .unwrap();
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &fine.id).unwrap();
        assert_eq!(row.state, AssignmentState::Settled, "an equipped lease must still be able to finish");

        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn a_finished_job_says_what_it_landed_and_never_claims_a_land_it_has_no_record_of() {
        use crate::cognition::ObligationState;
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(ObligationState::Settled);
        let _runner = h.host.start();

        // The land the engine performed, and the reviewer that passed it.
        engine_receipt(&h, "worker-1", "obligation-7", "worker", Some("cc/echo-1"), None);
        engine_receipt(&h, "reviewer-1", "obligation-7", "reviewer", None, Some("passed"));
        let landed = h.host.register(&h.binding, &registration(&h)).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &landed.id).unwrap();
        assert_eq!(row.state, AssignmentState::Settled);
        // The branch, the repository BY NAME rather than by path, and the verdict.
        assert!(row.detail.contains("It landed on cc/echo-1 in project."), "{}", row.detail);
        assert!(row.detail.contains("An independent review passed it first."), "{}", row.detail);
        assert!(!row.detail.contains("/fictional/"), "he was read an absolute path: {}", row.detail);
        let notice = h.notices.0.lock().unwrap().last().unwrap().1.clone();
        assert_eq!(notice.kind, NoticeKind::Settled);
        assert!(notice.text.contains("cc/echo-1 in project"), "the notice does not say what landed: {}", notice.text);
        assert!(notice.text.contains("An independent review passed it first."), "{}", notice.text);
        // ONE DESCRIPTION OF ONE LAND: the row the surface renders and the sentence he hears
        // carry the same clause, because both come from `what_happened`.
        assert!(notice.text.contains(&row.detail), "the row and the notice describe the land differently");

        // NEGATIVE HALF: same closed obligation, same worker evidence, no land on record.
        let bare = h
            .host
            .register(&h.binding, &Registration { obligation_id: "obligation-8".into(), ..registration(&h) })
            .unwrap();
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &bare.id).unwrap();
        assert_eq!(row.state, AssignmentState::Settled, "an assignment with nothing to land must still be able to close");
        assert!(row.detail.contains("nothing was landed"), "{}", row.detail);
        assert!(!row.detail.contains("It landed"), "a land was claimed with no record of one: {}", row.detail);

        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **A TURN THAT HAS ENDED IS NEVER "STILL RUNNING" — the fourth way a background job
    /// stalled, and the worst of the four because he was told nothing at all.**
    ///
    /// The first real run of the whole flow (`esc-20260918T115854Z-852c5f9c`, and the record
    /// at `docs/verification/2026-09-18-the-third-way-a-background-job-died.md` §5) reached
    /// the back end, ran a turn, prepared nothing, and ended. The settle reading then found
    /// no worker evidence it could attribute and the runner wrote `Running` with the reading's
    /// own sentence at **t+350.207 s**. The row sat there for the remaining **1,150 s** with
    /// `notices: []`. Nothing re-reads a `Running` row, so that state had no watcher and no
    /// exit: a silent hang, which is worse for him than a failure card because a card at
    /// least ends the wait.
    ///
    /// **Arm A is that run, in a harness.** Arms B and C are the controls, and both are
    /// needed for different reasons:
    ///
    /// - **B** — a run whose work IS witnessed still finishes and still says what it landed.
    ///   Without it, a build that failed every assignment would pass arm A.
    /// - **C** — the identical assignment with READABLE worker evidence and the same open
    ///   obligation produces the SAME sentence through the `NotSettled` arm. That is the
    ///   assertion that arm A reports a missing witness with the reading the rest of this
    ///   module already uses, rather than inventing a claim of its own.
    #[test]
    fn a_turn_that_has_ended_with_nothing_witnessed_fails_loudly_and_never_sits_in_running() {
        use crate::cognition::ObligationState;
        let h = harness(5);
        // The obligation is OPEN and there is no worker evidence directory at all — the
        // exact pair the real run ended on.
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        let _runner = h.host.start();

        // ---- A. THE RUN THAT HUNG -------------------------------------------------------
        let stalled = h.host.register(&h.binding, &registration(&h)).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &stalled.id).unwrap();
        assert_ne!(row.state, AssignmentState::Running, "a job whose turn has ended still reads as running");
        assert_eq!(row.state, AssignmentState::Failed);
        // The settle reading's sentence is for an open turn and must not be what he is left
        // with when the turn has ended.
        assert!(
            !row.detail.contains("counts as still running"),
            "the ended turn kept the settle reading's sentence: {}",
            row.detail
        );
        // What he IS told is read off the receipts, and with no receipts that is the truth
        // about this run: nothing got as far as being prepared.
        assert_eq!(row.detail, "No work was started, so nothing was landed.");
        // **AND HE IS TOLD.** The real run's `notices` was `[]`; that is the half that made
        // it a silent hang rather than a wrong card.
        let notice = h.notices.0.lock().unwrap().last().unwrap().1.clone();
        assert_eq!(notice.kind, NoticeKind::Failed);
        assert!(
            notice.text.contains("No work was started, so nothing was landed."),
            "he was not told what happened: {}",
            notice.text
        );
        assert!(!notice.text.contains("still running"), "the notice says it is running: {}", notice.text);

        // ---- B. CONTROL: a witnessed run with a receipt still finishes -------------------
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(ObligationState::Settled);
        engine_receipt(&h, "worker-b", "obligation-8", "worker", Some("cc/echo-b"), None);
        engine_receipt(&h, "reviewer-b", "obligation-8", "reviewer", None, Some("passed"));
        let fine = h
            .host
            .register(&h.binding, &Registration { obligation_id: "obligation-8".into(), ..registration(&h) })
            .unwrap();
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &fine.id).unwrap();
        assert_eq!(row.state, AssignmentState::Settled, "a witnessed run must still be able to finish");
        assert!(row.detail.contains("It landed on cc/echo-b in project."), "{}", row.detail);

        // ---- C. CONTROL: the witness is what changed, not the verdict --------------------
        // Readable worker evidence, the same open obligation, no receipts. This goes through
        // `NotSettled` — the arm that was already correct — and must produce arm A's sentence
        // word for word.
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        let witnessed_open = h
            .host
            .register(&h.binding, &Registration { obligation_id: "obligation-9".into(), ..registration(&h) })
            .unwrap();
        assert!(h.host.wait_for_completed(3, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &witnessed_open.id).unwrap();
        assert_eq!(row.state, AssignmentState::Failed);
        assert_eq!(row.detail, "No work was started, so nothing was landed.");

        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **A LAND THAT FAILED, reported as one, with the reviewer's refusal as the reason** —
    /// §52's third ending. The obligation is open, nothing is waiting for him, and the receipts
    /// say why: the reviewer asked for changes.
    ///
    /// Its control is the arm above it in `a_job_that_did_not_land_is_failed_and_only_a_question_of_his_makes_it_blocked`,
    /// where there are no receipts at all and the sentence is the different one — so "the
    /// reason came from the receipts" is a fact rather than one sentence for every failure.
    #[test]
    fn a_land_the_reviewer_refused_is_reported_as_a_failed_land_with_that_reason() {
        use crate::cognition::ObligationState;
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        let _runner = h.host.start();
        engine_receipt(&h, "worker-1", "obligation-7", "worker", None, None);
        engine_receipt(&h, "reviewer-1", "obligation-7", "reviewer", None, Some("changes-requested"));
        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(row.state, AssignmentState::Failed);
        assert_eq!(row.detail, "The review asked for changes, so nothing was landed.");
        let notice = h.notices.0.lock().unwrap().last().unwrap().1.clone();
        assert_eq!(notice.kind, NoticeKind::Failed);
        assert!(notice.text.contains("stopped before it finished"), "{}", notice.text);
        assert!(notice.text.contains("The review asked for changes"), "the reason was softened away: {}", notice.text);
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **A LAND WHOSE ASSIGNMENT WAS NEVER CLOSED is not called finished, and is not called
    /// unlanded either.** The one case where §52's two honesty rules pull in opposite
    /// directions: the branch really did move, and the obligation really is open. Both go in
    /// the sentence.
    #[test]
    fn a_land_with_an_open_assignment_is_neither_finished_nor_reported_as_unlanded() {
        use crate::cognition::ObligationState;
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        let _runner = h.host.start();
        engine_receipt(&h, "worker-1", "obligation-7", "worker", Some("cc/echo-1"), None);
        engine_receipt(&h, "reviewer-1", "obligation-7", "reviewer", None, Some("passed"));
        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(row.state, AssignmentState::Failed, "an open obligation was called finished");
        assert!(row.detail.contains("It landed on cc/echo-1 in project."), "his branch moved and he was not told: {}", row.detail);
        let notice = h.notices.0.lock().unwrap().last().unwrap().1.clone();
        assert!(notice.text.contains("stopped before it finished"), "{}", notice.text);
        assert!(notice.text.contains("It landed on cc/echo-1 in project."), "{}", notice.text);
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **HIS ANSWER ARRIVES AS AN ANSWER, WITH THE OBLIGATION STILL OPEN** — the CEO's
    /// ruling §58, 2026-09-18: *"The answer arrives on the timeline as an answer, never as
    /// 'done'."*
    ///
    /// **The open obligation is the whole point of this test, not a detail of its setup.**
    /// Answering a question gives a back end no reason to close anything, so this is the
    /// ORDINARY shape of an answered question — and it is the shape that, before the question
    /// arm was placed ahead of the obligation reading, produced `Failed` and *"stopped before
    /// it finished"* with his answer sitting unread in a local variable.
    ///
    /// The comparison that makes it a test rather than an assertion is in the same function:
    /// an identical run with `kind` left as a task, same open obligation, same words off the
    /// stream, still reports the failure it should.
    #[test]
    fn an_answered_question_is_delivered_as_the_answer_and_never_as_a_job_that_finished() {
        use crate::cognition::ObligationState;
        let h = harness(5);
        every_worker_observed_ending(&h);
        // Open, and it stays open: nothing about answering him closes it.
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        *h.answer_reply.lock().unwrap() =
            "The nightly has been red since Tuesday because the speech model download times out on a slow connection.".into();
        let _runner = h.host.start();
        let receipt = h
            .host
            .register_kind(
                &h.binding,
                &Registration { title: "why the nightly has been red since Tuesday".into(), ..registration(&h) },
                assignment::AssignmentKind::Investigate,
            )
            .unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(row.state, AssignmentState::Settled, "an answered question was not closed: {}", row.detail);
        assert_eq!(row.kind, assignment::AssignmentKind::Investigate);
        let notice = h.notices.0.lock().unwrap().last().unwrap().1.clone();
        assert_eq!(notice.kind, NoticeKind::Answer, "his answer was filed as a work result");
        // **The answer, and nothing but the answer.**
        assert_eq!(
            notice.text,
            "The nightly has been red since Tuesday because the speech model download times out on a slow connection."
        );
        for wrapper in ["is finished", "landed", "stopped before", "your saved work", "On it"] {
            assert!(!notice.text.contains(wrapper), "a work receipt's framing reached his answer: {}", notice.text);
        }
        // §58 names the one framing it refuses, so it is asserted by name in both cases.
        assert!(!notice.text.to_lowercase().contains("done"), "{}", notice.text);
        // Nothing an identifier could ride out on.
        assert!(!notice.text.contains(&receipt.id));
        assert!(!notice.text.contains("obligation-7"));
        assert!(!notice.text.contains("work-seat"));

        // **THE COMPARISON.** Same open obligation, same words off the stream, registered as
        // a TASK that a helper worked on — and it still reports the failure an open
        // obligation means for work. So the arm above is reached by the KIND and not by
        // anything else that changed here. (The helper's receipt is what keeps this a WORK
        // failure: a task no helper ever touched is reported in the back end's own words, see
        // `a_task_the_back_end_did_itself_is_reported_in_its_own_words_not_as_no_work_started`.)
        engine_receipt(&h, "worker-q", "obligation-8", "worker", None, None);
        let task = h
            .host
            .register(&h.binding, &Registration { obligation_id: "obligation-8".into(), ..registration(&h) })
            .unwrap();
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)));
        let task_row = assignment::read(&h.state, "depot", "thread-one", &task.id).unwrap();
        assert_eq!(task_row.state, AssignmentState::Failed, "an open obligation stopped failing work");
        let task_notice = h.notices.0.lock().unwrap().last().unwrap().1.clone();
        assert_eq!(task_notice.kind, NoticeKind::Failed);
        assert!(task_notice.text.contains("stopped before it finished"), "{}", task_notice.text);
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **A TASK THE BACK END CARRIED OUT ITSELF IS REPORTED IN ITS OWN WORDS** —
    /// esc-20260927T093052Z-85f3303f, measured in the test VM on 2026-09-27.
    ///
    /// He asked for a command to be run. The back end ran it (the process started within 4.3 s of
    /// his approval, `sleep 90` under the back end's own `claude`), said what it had done, and
    /// ended its turn 3.6-4.1 s after the command started. No helper was ever prepared — there was nothing to land —
    /// so the engine's `complete` (which needs at least one worker) could never close the
    /// obligation, and the host read the receipts, found none, and told him *"It stopped
    /// before it finished. No work was started, so nothing was landed."* The back end's own
    /// report was in `answer`'s place and was thrown away because the kind was not a
    /// question. The same arm swallowed the first-run case — *"The Acme repository isn't
    /// connected to me … connect it in the app under Connected repositories"* — and left him
    /// with no idea what to do.
    ///
    /// Controls, in the same test: a task a helper DID work on, with the same words and the
    /// same open obligation, is still the work failure the receipts say it is; a back end
    /// that said nothing is still told as "no work was started".
    #[test]
    fn a_task_the_back_end_did_itself_is_reported_in_its_own_words_not_as_no_work_started() {
        use crate::cognition::ObligationState;
        let h = harness(5);
        every_worker_observed_ending(&h);
        // Open, and it stays open: nothing a back end does in its own turn can close it.
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        const REPORT: &str = "I ran git rev-list --count HEAD in the Acme repository. It has 3 commits.";
        *h.answer_reply.lock().unwrap() = REPORT.into();
        h.host.start();

        // ---- the task no helper touched ------------------------------------------------
        let itself = h
            .host
            .register(&h.binding, &Registration { title: "count the commits in Acme".into(), ..registration(&h) })
            .unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &itself.id).unwrap();
        assert_eq!(row.state, AssignmentState::Settled, "his task was reported as a failure: {}", row.detail);
        assert_eq!(row.detail, "Answered.");
        assert!(row.was_answered(), "the pane would call a report it never witnessed \"Finished.\"");
        let notice = h.notices.0.lock().unwrap().last().unwrap().1.clone();
        assert_eq!(notice.kind, NoticeKind::Answer, "the back end's report was filed as a work result");
        assert_eq!(notice.text, REPORT, "the result he asked for did not reach him");
        for wrapper in ["No work was started", "stopped before", "is finished", "your saved work"] {
            assert!(!notice.text.contains(wrapper), "a receipt sentence replaced the report: {}", notice.text);
        }

        // ---- CONTROL: a helper worked on it — the receipts still decide ------------------
        engine_receipt(&h, "worker-c", "obligation-8", "worker", None, None);
        let helped = h
            .host
            .register(&h.binding, &Registration { obligation_id: "obligation-8".into(), ..registration(&h) })
            .unwrap();
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &helped.id).unwrap();
        assert_eq!(row.state, AssignmentState::Failed, "a helper's unlanded work was reported as an answer");
        assert!(!row.was_answered());
        assert!(row.detail.contains("nothing was landed"), "{}", row.detail);
        let notice = h.notices.0.lock().unwrap().last().unwrap().1.clone();
        assert_eq!(notice.kind, NoticeKind::Failed);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();

        // ---- CONTROL: a back end that said nothing is still "no work was started" --------
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        h.host.start();
        let silent = h.host.register(&h.binding, &registration(&h)).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &silent.id).unwrap();
        assert_eq!(row.state, AssignmentState::Failed);
        assert_eq!(row.detail, "No work was started, so nothing was landed.");
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// Non-repository work can close without a code worker. Independent Stu must
    /// select its existing obligation; a legacy assignment-bound host can still
    /// close from the observed native report. The loaded instructions and schema
    /// must describe both paths rather than contradicting each other.
    #[test]
    fn a_job_that_changes_no_repository_is_reported_without_the_machinery() {
        let h = harness(5);
        let receipt = assignment::register_kind(&h.state, &registration(&h), assignment::AssignmentKind::Task).unwrap();
        let record = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        let brief = brief_for(&record, false, &record.title);
        let input: serde_json::Value = serde_json::from_str(&brief).unwrap();
        assert_eq!(input["assignment"]["message"], record.title);
        assert_eq!(input["assignment"]["question"], false);
        let engine = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().join("engine/mega-lander");
        let desktop = std::fs::read_to_string(engine.join("DESKTOP.md")).unwrap().split_whitespace().collect::<Vec<_>>().join(" ");
        assert!(desktop.contains("Work that changes no repository needs no code worker."),
                "the tool contract wrongly requires code work for a non-repository assignment");
        assert!(desktop.contains("Close its obligation with `complete` and your verified `answer`"));
        assert!(desktop.contains("A legacy assignment-bound host can also record an established outcome from its native report."));
        assert!(desktop.contains("A consult needs no reviewer or `integrate`."));
        assert!(desktop.contains("Supply the selected real `obligation_id`"));
        assert!(!desktop.contains("leave it out"), "the shared contract must not tell independent Stu to omit his obligation");
        let tools = std::fs::read_to_string(engine.join("app.py")).unwrap();
        assert!(tools.contains("For an assignment without implementation work, supply answer with your verified outcome instead of worker_ids."),
                "the complete tool does not explain non-repository completion");
        assert!(tools.contains("Completion updates the operational record but never sends Rich a message."));
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// Records every answer close the host asks for, and fails them on demand.
    #[derive(Default)]
    struct Closes {
        asked: Mutex<Vec<(String, String)>>,
        refuse: Mutex<Option<String>>,
    }

    impl AnsweredClose for Closes {
        fn close_answered(&self, record: &Assignment, answer: &str) -> Result<(), String> {
            self.asked.lock().unwrap().push((record.obligation_id.clone(), answer.to_string()));
            match &*self.refuse.lock().unwrap() {
                Some(why) => Err(why.clone()),
                None => Ok(()),
            }
        }
    }

    /// A closer that does not return until the test lets it: the engine's close is three
    /// subprocess calls, and on first use the runtime is verified too (VM run 5, 2026-09-27).
    struct SlowClose {
        open: Mutex<bool>,
        released: std::sync::Condvar,
        asked: AtomicUsize,
    }

    impl AnsweredClose for SlowClose {
        fn close_answered(&self, _record: &Assignment, _answer: &str) -> Result<(), String> {
            self.asked.fetch_add(1, Ordering::SeqCst);
            let open = self.open.lock().unwrap();
            drop(self.released.wait_timeout_while(open, std::time::Duration::from_secs(10), |open| !*open).unwrap());
            Ok(())
        }
    }

    /// **His report reaches him before the bookkeeping, never after it** (VM runs 1 and 5,
    /// 2026-09-27, bundle 1.2.0-dev.0d3dadf8): the back end's report turn had happened, and the
    /// walk read the row `settled` with one notice and the obligation open, because the arm
    /// closed the obligation between writing `Settled` and raising his notice. A quit in that
    /// window loses the report outright.
    #[test]
    fn his_report_is_raised_before_the_obligation_is_closed() {
        use crate::cognition::ObligationState;
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        const REPORT: &str = "It ran and printed one line: 4b578fe init.";
        *h.answer_reply.lock().unwrap() = REPORT.into();
        let close = Arc::new(SlowClose { open: Mutex::new(false), released: std::sync::Condvar::new(), asked: AtomicUsize::new(0) });
        h.host.set_answered_close(close.clone());
        h.host.start();
        h.host.register(&h.binding, &registration(&h)).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while close.asked.load(Ordering::SeqCst) == 0 {
            assert!(std::time::Instant::now() < deadline, "the close was never asked for");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        // The close is still running: his report must already be his.
        let told: Vec<String> = h.notices.0.lock().unwrap().iter().map(|(_, n)| n.text.clone()).collect();
        *close.open.lock().unwrap() = true;
        close.released.notify_all();
        assert_eq!(told, [REPORT], "his report waited on the engine's bookkeeping");
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **An assignment the back end handled itself closes its engine obligation, on the words
    /// he was given** (richos-hq `2026-09-27-background-command-finish`, "Not done": *"It's
    /// still open there"*). One close per such assignment, question or task, with the report
    /// as its evidence; none for an assignment a helper worked on, which still closes only
    /// through `complete` with its workers; and a close the engine refuses is never spoken.
    #[test]
    fn an_assignment_the_back_end_handled_itself_closes_its_obligation_on_the_report_he_was_given() {
        use crate::cognition::ObligationState;
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        const REPORT: &str = "I ran git rev-list --count HEAD in the Acme repository. It has 3 commits.";
        *h.answer_reply.lock().unwrap() = REPORT.into();
        let closes = Arc::new(Closes::default());
        h.host.set_answered_close(closes.clone());
        h.host.start();

        let itself = h.host.register(&h.binding, &registration(&h)).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        assert_eq!(assignment::read(&h.state, "depot", "thread-one", &itself.id).unwrap().state, AssignmentState::Settled);
        assert_eq!(*closes.asked.lock().unwrap(), vec![("obligation-7".to_string(), REPORT.to_string())],
                   "the task it handled itself was left open in the engine");

        // A question is handled by the back end itself by definition.
        h.host
            .register_kind(&h.binding, &Registration { obligation_id: "obligation-q".into(), ..registration(&h) },
                           assignment::AssignmentKind::Investigate)
            .unwrap();
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)));
        assert_eq!(closes.asked.lock().unwrap().last().unwrap().0, "obligation-q");

        // CONTROL: a helper worked on it — never closed on an answer.
        engine_receipt(&h, "worker-c", "obligation-8", "worker", None, None);
        h.host.register(&h.binding, &Registration { obligation_id: "obligation-8".into(), ..registration(&h) }).unwrap();
        assert!(h.host.wait_for_completed(3, std::time::Duration::from_secs(10)));
        assert_eq!(closes.asked.lock().unwrap().len(), 2, "work a helper did was closed on the back end's words");

        // A refused close changes nothing he hears.
        *closes.refuse.lock().unwrap() = Some("the engine's store could not be reached".into());
        let refused = h.host.register(&h.binding, &Registration { obligation_id: "obligation-9".into(), ..registration(&h) }).unwrap();
        assert!(h.host.wait_for_completed(4, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &refused.id).unwrap();
        assert_eq!((row.state, row.detail.as_str()), (AssignmentState::Settled, "Answered."));
        let notice = h.notices.0.lock().unwrap().last().unwrap().1.clone();
        assert_eq!((notice.kind, notice.text.as_str()), (NoticeKind::Answer, REPORT));
        assert_eq!(closes.asked.lock().unwrap().len(), 3);
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    fn background_command(id: &str, what: &str) -> crate::cognition::BackgroundCommand {
        crate::cognition::BackgroundCommand { task_id: id.into(), description: what.into(), ended: None }
    }

    /// The provider's `task_notification` for `id`, as the reader records it.
    fn background_command_ends(h: &Harness, id: &str, during_a_turn_of_ours: bool) {
        let mut reading = h.background.lock().unwrap();
        for command in reading.as_mut().expect("this lease reports commands").iter_mut().filter(|c| c.task_id == id) {
            command.ended = Some(crate::cognition::CommandEnded {
                status: "completed".into(),
                summary: format!("Background command \"{}\" completed (exit code 0)", command.description),
                during_a_turn_of_ours,
            });
        }
    }

    /// Wait until `n` notices have been raised, on the record of them rather than a sleep.
    fn until_notices(h: &Harness, n: usize) -> Vec<PendingNotice> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let notices: Vec<PendingNotice> = h.notices.0.lock().unwrap().iter().map(|(_, n)| n.clone()).collect();
            if notices.len() >= n {
                return notices;
            }
            assert!(std::time::Instant::now() < deadline, "{n} notice(s) never came: {notices:?}");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    /// **"START IT AND TELL ME WHEN IT FINISHES": THE FINISH REACHES HIM, AS ITS OWN REPORT,
    /// WHEN THE COMMAND ENDS** — the leftover of esc-20260927T093052Z-85f3303f.
    ///
    /// In the test VM on 2026-09-27 the back end started `sleep 90 && date > ...` in the
    /// background and ended its turn with *"I've started your test command ... I'll tell you
    /// as soon as it does"*; that was the last he heard. The provider's own
    /// `<task-notification>` turn said *"Your first test command has finished"* (session
    /// `52e71a95`, rows 19-20) to nobody: it ran with no prompt parked, so its words went to
    /// the between-turn lane, which this host never reads, and the assignment had already
    /// been settled on the words "it has started".
    ///
    /// Now: his assignment stays open while the command runs, and he hears the back end's
    /// words at once (the interim notice, the report he got before); when the provider says
    /// the command ended, the back end is asked for its report, and THAT is the answer the
    /// assignment settles on.
    ///
    /// RED at `24d9930c`: the assignment settled on the first turn's words, one prompt, one
    /// notice.
    #[test]
    fn a_background_command_s_finish_reaches_him_as_its_own_report_when_it_ends() {
        use crate::cognition::ObligationState;
        const STARTED: &str = "I've started the test suite in the Acme folder. It takes a few minutes; I'll tell you when it finishes.";
        const FINISHED: &str = "The test suite has finished: all 212 tests passed.";
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        *h.background.lock().unwrap() = Some(Vec::new());
        h.replies.lock().unwrap().extend([STARTED.to_string(), FINISHED.to_string()]);
        h.start_in_turn.lock().unwrap().push_back(vec![background_command("bk1c0clka", "npm test")]);
        h.host.start();
        let job = h
            .host
            .register(&h.binding, &Registration { title: "run the tests and tell me when they finish".into(), ..registration(&h) })
            .unwrap();

        // ---- while it runs: he has the back end's words, and the job is still open -------
        let notices = until_notices(&h, 1);
        assert_eq!(notices[0].kind, NoticeKind::Answer, "{notices:?}");
        assert_eq!(notices[0].text, STARTED);
        let row = assignment::read(&h.state, "depot", "thread-one", &job.id).unwrap();
        assert_eq!(row.state, AssignmentState::Running, "settled while its command was still running: {}", row.detail);
        assert_eq!(row.detail, COMMAND_STILL_RUNNING_DETAIL);
        assert!(!h.host.wait_for_completed(1, std::time::Duration::from_millis(300)), "it stopped waiting on its own");
        assert_eq!(h.work_prompts.lock().unwrap().len(), 1);

        // ---- the provider says it ended, outside any turn of ours ---------------------------
        background_command_ends(&h, "bk1c0clka", false);
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)), "the ending was never acted on");
        let prompts = h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(), 2, "the back end was never asked for its report: {prompts:?}");
        assert!(prompts[1].contains("npm test") && prompts[1].contains("exit code 0"), "{}", prompts[1]);
        // VM run 6 (2026-09-27): he asked "what it printed" and the report described the output
        // ("one entry, labeled init") instead of quoting it. His words are what the report owes.
        assert!(!prompts[1].contains("Give him your report"), "a factual event still supplies a workflow script");
        let row = assignment::read(&h.state, "depot", "thread-one", &job.id).unwrap();
        assert!(row.was_answered(), "{:?}: {}", row.state, row.detail);
        let notices = until_notices(&h, 2);
        assert_eq!(notices.len(), 2, "{notices:?}");
        assert_eq!(notices[1].kind, NoticeKind::Answer);
        assert_eq!(notices[1].text, FINISHED, "the finish did not reach him as the report");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **The three ways the wait must NOT happen, or must end.**
    ///
    /// 1. A command whose ending the provider folded into the back end's own turn
    ///    (`cap-fold.jsonl`: the model's answer already describes it) is not asked about
    ///    again — that would be a second model turn to repeat what he was told.
    /// 2. When he gives this thread another job while a command still runs, the back end is
    ///    not held for it: the first job closes on the words he already has (no second copy
    ///    of them), and the second job runs.
    /// 3. His Stop reaches a job that is waiting on its command.
    #[test]
    fn a_folded_ending_a_new_job_and_his_stop_each_end_the_wait_for_a_background_command() {
        use crate::cognition::ObligationState;
        const SAID: &str = "I ran the build; it finished while I was checking the log, and it passed.";

        // ---- 1. folded into the turn: one prompt, one notice -------------------------------
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        *h.background.lock().unwrap() = Some(Vec::new());
        *h.answer_reply.lock().unwrap() = SAID.into();
        let mut folded = background_command("b50y3od71", "make");
        folded.ended = Some(crate::cognition::CommandEnded {
            status: "completed".into(), summary: "make completed (exit code 0)".into(), during_a_turn_of_ours: true,
        });
        h.start_in_turn.lock().unwrap().push_back(vec![folded]);
        h.host.start();
        let job = h.host.register(&h.binding, &registration(&h)).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        assert_eq!(h.work_prompts.lock().unwrap().len(), 1, "a folded ending was asked about again");
        let row = assignment::read(&h.state, "depot", "thread-one", &job.id).unwrap();
        assert!(row.was_answered(), "{:?}: {}", row.state, row.detail);
        let notices = until_notices(&h, 1);
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].text, SAID);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();

        // ---- 2. another job arrives: the wait yields, nothing is said twice, and the finish
        //         still reaches him later as the first job's own report --------------------
        //
        // Until bgdone2 the first job settled here on the words he already had, and the
        // command's finish was lost (richos-hq 2026-09-27-background-command-finish, "Not
        // done"). It now stays open, "still running", while the second job runs.
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        *h.background.lock().unwrap() = Some(Vec::new());
        const RUNNING: &str = "The dev server is starting at localhost:5173.";
        const SECOND: &str = "Your Acme folder has 3 saved changes.";
        const STOPPED: &str = "The dev server stopped on its own: it printed \"port 5173 in use\".";
        h.replies.lock().unwrap().extend([RUNNING.to_string(), SECOND.to_string(), STOPPED.to_string()]);
        h.start_in_turn.lock().unwrap().push_back(vec![background_command("bserver1", "npm run dev")]);
        h.host.start();
        let server = h.host.register(&h.binding, &Registration { title: "start the dev server".into(), ..registration(&h) }).unwrap();
        until_notices(&h, 1);
        let next = h
            .host
            .register(&h.binding, &Registration { obligation_id: "obligation-8".into(), ..registration(&h) })
            .unwrap();
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)), "the second job waited on the first's command");
        let row = assignment::read(&h.state, "depot", "thread-one", &server.id).unwrap();
        assert_eq!((row.state, row.detail.as_str()), (AssignmentState::Running, COMMAND_STILL_RUNNING_DETAIL),
                   "the first job was closed while its command still ran, and its finish would be lost");
        let for_server = |h: &Harness| -> Vec<PendingNotice> {
            h.notices.0.lock().unwrap().iter().filter(|(_, n)| n.assignment_id == server.id).map(|(_, n)| n.clone()).collect()
        };
        assert_eq!(for_server(&h).len(), 1, "he was told the same words twice: {:?}", for_server(&h));
        let prompts = h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(), 2);
        assert!(!prompts[1].contains(COMMAND_ENDED_HEAD), "the second job's turn was spent on the first's command");
        assert!(prompts[1].contains("\"npm run dev\" (for \"start the dev server\")") && prompts[1].contains("Other assignments' background commands (observed)"),
                "the second job's back end was not told to keep the first job's finish out of its answer: {}", prompts[1]);
        let row = assignment::read(&h.state, "depot", "thread-one", &next.id).unwrap();
        assert!(!row.state.is_open(), "{:?}", row.state);

        background_command_ends(&h, "bserver1", false);
        assert!(h.host.wait_for_completed(3, std::time::Duration::from_secs(10)), "the finish was never acted on");
        let prompts = h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(), 3);
        assert!(prompts[2].contains(COMMAND_ENDED_HEAD) && prompts[2].contains("start the dev server") && prompts[2].contains("npm run dev"),
                "the back end was not asked for the first job's report: {}", prompts[2]);
        assert_eq!(h.bound.lock().unwrap().last().unwrap().assignment_id, server.id, "the report turn ran on another job's seat");
        let row = assignment::read(&h.state, "depot", "thread-one", &server.id).unwrap();
        assert!(row.was_answered(), "{:?}: {}", row.state, row.detail);
        let told = for_server(&h);
        assert_eq!(told.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(), [RUNNING, STOPPED], "{told:?}");
        let for_next: Vec<_> = h.notices.0.lock().unwrap().iter().filter(|(_, n)| n.assignment_id == next.id).map(|(_, n)| n.text.clone()).collect();
        assert_eq!(for_next, [SECOND], "the first job's finish was folded into the second job's answer");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();

        // ---- 3. his Stop reaches a job that waits on its command ---------------------------
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        *h.background.lock().unwrap() = Some(Vec::new());
        *h.answer_reply.lock().unwrap() = "Started.".into();
        h.start_in_turn.lock().unwrap().push_back(vec![background_command("blong", "sleep 3600")]);
        h.host.start();
        let job = h.host.register(&h.binding, &registration(&h)).unwrap();
        until_notices(&h, 1);
        h.host.stop_assignment("depot", "thread-one", &job.id).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)), "his Stop did not reach the wait");
        let row = assignment::read(&h.state, "depot", "thread-one", &job.id).unwrap();
        assert_eq!(row.state, AssignmentState::Interrupted, "{}", row.detail);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **His Stop of a watched job tells its command to stop** (hunt 2026-09-29 part 1,
    /// finding 11). A job whose background command outlived its wait is watched between jobs;
    /// Stop used to take it off the watch and mark it `interrupted` without ever telling the
    /// command, which kept running and changing the workspace after he saw it stopped. The
    /// command is now told, by the provider's own task id, on the lease it runs on; and the
    /// rest is as before: `interrupted`, the same words, and its later ending asks nothing.
    #[test]
    fn a_stop_of_a_watched_job_tells_its_command_to_stop() {
        use crate::cognition::ObligationState;
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        *h.background.lock().unwrap() = Some(Vec::new());
        h.host.set_command_wait_budget(std::time::Duration::from_millis(100));
        *h.answer_reply.lock().unwrap() = "Started.".into();
        h.start_in_turn.lock().unwrap().push_back(vec![background_command("blong", "sleep 3600")]);
        h.host.start();
        let job = h.host.register(&h.binding, &registration(&h)).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        assert!(h.fence.commands_stopped.lock().unwrap().is_empty(), "nothing is stopped before he asks");
        h.host.stop_assignment("depot", "thread-one", &job.id).unwrap();
        assert_eq!(*h.fence.commands_stopped.lock().unwrap(), ["blong"],
                   "Stop took the job off the watch and never told its command to stop");
        let row = assignment::read(&h.state, "depot", "thread-one", &job.id).unwrap();
        assert_eq!((row.state, row.detail.as_str()), (AssignmentState::Interrupted, "Stopped. The workspace and the receipts are kept."));
        background_command_ends(&h, "blong", false);
        assert!(!h.host.wait_for_completed(2, std::time::Duration::from_millis(400)), "a stopped job was asked for a report");
        assert_eq!(h.work_prompts.lock().unwrap().len(), 1);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **A stop the provider refused, or whose ending was never seen, is never reported as a
    /// stopped command** (hunt 2026-09-29 part 1 v2, finding 11: the request being written
    /// was taken as the command having stopped). Each of the provider's answers gets its own
    /// true sentence; only a witnessed ending gets the plain "Stopped."
    #[test]
    fn a_stop_the_provider_refused_or_never_confirmed_is_not_reported_as_stopped() {
        use crate::cognition::ObligationState;
        for (answer, said) in [
            (CommandStop::Refused("fixture refuses task stop".into()), WATCHED_STOP_REFUSED),
            (CommandStop::Unconfirmed, WATCHED_STOP_UNCONFIRMED),
            (CommandStop::NotDelivered, WATCHED_STOP_NOT_DELIVERED),
            (CommandStop::Ended, "Stopped. The workspace and the receipts are kept."),
        ] {
            let h = harness(5);
            every_worker_observed_ending(&h);
            *h.obligation.lock().unwrap() = Some(ObligationState::Open);
            *h.background.lock().unwrap() = Some(Vec::new());
            h.host.set_command_wait_budget(std::time::Duration::from_millis(100));
            *h.answer_reply.lock().unwrap() = "Started.".into();
            *h.fence.stop_answer.lock().unwrap() = Some(answer.clone());
            h.start_in_turn.lock().unwrap().push_back(vec![background_command("blong", "sleep 3600")]);
            h.host.start();
            let job = h.host.register(&h.binding, &registration(&h)).unwrap();
            assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
            h.host.stop_assignment("depot", "thread-one", &job.id).unwrap();
            let row = assignment::read(&h.state, "depot", "thread-one", &job.id).unwrap();
            assert_eq!((row.state, row.detail.as_str()), (AssignmentState::Interrupted, said),
                       "the provider answered {answer:?} and he was told something else");
            h.host.shutdown();
            std::fs::remove_dir_all(&h.root).unwrap();
        }
    }

    /// **A background command that outlives the wait still reaches him once, as its own
    /// job's report** (richos-hq 2026-09-27-background-command-finish, "Not done": *"If he
    /// gives the thread another job while it runs, or it runs past 60 min, the host stops
    /// waiting and claims nothing"*).
    ///
    /// 1. Its ending folded into ANOTHER job's turn: that job's answer is that job's, and the
    ///    first job's report is asked for on its own seat afterwards.
    /// 2. The bound: the job is handed to the watch rather than closed, and its finish is
    ///    reported when it comes.
    /// 3. His Stop reaches a watched job: interrupted, and a later ending asks nothing.
    /// 4. The connection it ran on is gone: he is told the result could not be read, never
    ///    nothing.
    #[test]
    fn a_command_that_outlives_the_wait_is_still_reported_once_as_its_own_job_s_report() {
        use crate::cognition::ObligationState;
        let only = |h: &Harness, id: &str| -> Vec<String> {
            h.notices.0.lock().unwrap().iter().filter(|(_, n)| n.assignment_id == id).map(|(_, n)| n.text.clone()).collect()
        };

        // ---- 1. folded into the next job's turn ---------------------------------------------
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        *h.background.lock().unwrap() = Some(Vec::new());
        h.replies.lock().unwrap().extend(["Started the build.", "Acme has 3 saved changes.", "The build finished and passed."].map(String::from));
        let mut folded = background_command("bbuild", "make");
        folded.ended = Some(crate::cognition::CommandEnded {
            status: "completed".into(), summary: "make completed (exit code 0)".into(), during_a_turn_of_ours: true,
        });
        h.start_in_turn.lock().unwrap().extend([vec![background_command("bbuild", "make")], vec![folded]]);
        h.host.start();
        let build = h.host.register(&h.binding, &Registration { title: "run the build".into(), ..registration(&h) }).unwrap();
        until_notices(&h, 1);
        let count = h.host.register(&h.binding, &Registration { obligation_id: "obligation-8".into(), ..registration(&h) }).unwrap();
        assert!(h.host.wait_for_completed(3, std::time::Duration::from_secs(10)), "the folded finish was never reported on its own");
        assert_eq!(only(&h, &count.id), ["Acme has 3 saved changes."]);
        assert_eq!(only(&h, &build.id), ["Started the build.", "The build finished and passed."]);
        assert!(assignment::read(&h.state, "depot", "thread-one", &build.id).unwrap().was_answered());
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();

        // ---- 2. the bound hands it to the watch -----------------------------------------------
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        *h.background.lock().unwrap() = Some(Vec::new());
        h.host.set_command_wait_budget(std::time::Duration::from_millis(100));
        h.replies.lock().unwrap().extend(["Started the backup.", "The backup finished: 4 GB copied."].map(String::from));
        h.start_in_turn.lock().unwrap().push_back(vec![background_command("bbackup", "rsync -a")]);
        h.host.start();
        let backup = h.host.register(&h.binding, &registration(&h)).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)), "the bound was never reached");
        let row = assignment::read(&h.state, "depot", "thread-one", &backup.id).unwrap();
        assert_eq!((row.state, row.detail.as_str()), (AssignmentState::Running, COMMAND_STILL_RUNNING_DETAIL),
                   "reaching the bound closed the job and lost its finish");
        background_command_ends(&h, "bbackup", false);
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)));
        assert_eq!(only(&h, &backup.id), ["Started the backup.", "The backup finished: 4 GB copied."]);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();

        // ---- 3. his Stop reaches a watched job --------------------------------------------------
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        *h.background.lock().unwrap() = Some(Vec::new());
        h.host.set_command_wait_budget(std::time::Duration::from_millis(100));
        *h.answer_reply.lock().unwrap() = "Started.".into();
        h.start_in_turn.lock().unwrap().push_back(vec![background_command("blong", "sleep 3600")]);
        h.host.start();
        let job = h.host.register(&h.binding, &registration(&h)).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        h.host.stop_assignment("depot", "thread-one", &job.id).unwrap();
        let row = assignment::read(&h.state, "depot", "thread-one", &job.id).unwrap();
        assert_eq!((row.state, row.detail.as_str()), (AssignmentState::Interrupted, "Stopped. The workspace and the receipts are kept."));
        background_command_ends(&h, "blong", false);
        assert!(!h.host.wait_for_completed(2, std::time::Duration::from_millis(400)), "a stopped job was asked for a report");
        assert_eq!(h.work_prompts.lock().unwrap().len(), 1);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();

        // ---- 4. the connection it ran on is gone ---------------------------------------------
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        *h.background.lock().unwrap() = Some(Vec::new());
        h.host.set_command_wait_budget(std::time::Duration::from_millis(100));
        *h.answer_reply.lock().unwrap() = "Started the import.".into();
        h.start_in_turn.lock().unwrap().push_back(vec![background_command("bimport", "import.sh")]);
        h.host.start();
        let import = h.host.register(&h.binding, &registration(&h)).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        // The next job's turn fails, and a back end that failed a turn is retired.
        *h.turn_error.lock().unwrap() = Some("the child exited".into());
        h.host.register(&h.binding, &Registration { obligation_id: "obligation-8".into(), ..registration(&h) }).unwrap();
        assert!(h.host.wait_for_completed(3, std::time::Duration::from_secs(10)), "a watch on a retired connection never ended");
        let row = assignment::read(&h.state, "depot", "thread-one", &import.id).unwrap();
        assert_eq!(row.state, AssignmentState::Failed, "{}", row.detail);
        let told = only(&h, &import.id);
        assert_eq!(told.len(), 2, "{told:?}");
        assert!(told[1].contains("could not be read"), "{}", told[1]);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    // =======================================================================================
    // HIS ANSWER ON THE WORK PATH: TAKEN AT THE FIRST ITEM, NEVER LOST, NEVER DOUBLED
    // (richos-hq `docs/plans/2026-09-27-work-path-answer-delivery-design.md` §4.1)
    // =======================================================================================

    /// One saved answer of his for `record`, in the inbox, and handed to the host the way the
    /// question worker hands it (reader 2).
    fn his_answer(h: &Harness, record: &Assignment, id: &str, text: &str) -> crate::questions::Delivery {
        let answer = crate::questions::Delivery { obligation_id:None,
            id: id.into(), entity_id: "depot".into(), thread_id: "thread-one".into(),
            asker: record.obligation_id.clone(), set_id: None, text: text.into(), receipt: None,
        };
        crate::question_work::enqueue(&h.state, &answer).unwrap();
        h.host.queue_question_answer(&h.binding, &answer).unwrap();
        answer
    }

    /// A job waiting for his answer: registered and blocked, with nothing queued.
    fn waiting_job(h: &Harness) -> Assignment {
        let receipt = assignment::register(&h.state, &registration(h)).unwrap();
        assignment::advance(&h.state, "depot", "thread-one", &receipt.id, AssignmentState::Blocked,
                            "Waiting for your answer. Independent work can continue.").unwrap();
        assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap()
    }

    /// A question of his the job asked and he has not answered, so its runs end blocked on it.
    fn open_question(h: &Harness, record: &Assignment) {
        use crate::questions::{AskScope, OptionInput, QuestionInput, Store};
        Store::new(&h.state).ask(&AskScope { obligation_id:None,
            root: h.state.clone(), entity_id: "depot".into(), thread_id: "thread-one".into(),
            turn_id: "turn-7".into(), asker: record.obligation_id.clone(), session_id: "work-session-one".into(),
            engine: None, entity_root: None, app_run: None,
        }, vec![QuestionInput {
            text: "Who should review it?".into(),
            options: vec![
                OptionInput { label: "Dana reviews".into(), description: "Faster, knows the code.".into() },
                OptionInput { label: "Sam reviews".into(), description: "Slower, fresh eyes.".into() },
            ],
            multiple: false, free_answer: true, recommended: None,
        }]).unwrap();
    }

    fn until_prompts(h: &Harness, n: usize) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while h.work_prompts.lock().unwrap().len() < n {
            assert!(std::time::Instant::now() < deadline, "prompt {n} was never sent");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    fn told(h: &Harness) -> Vec<String> {
        h.notices.0.lock().unwrap().iter().map(|(_, n)| n.text.clone()).collect()
    }

    /// §4.1 tests 1 and 4 (D1, D2). While the prompt carrying his answer waits for its first
    /// item, the answer is still pending on disk, and the question worker handing it over again
    /// queues nothing: the live run carries it. At the first item it is taken, in the lease's own
    /// session, and exactly one prompt carried it.
    #[test]
    fn his_answer_stays_pending_until_the_first_item_and_the_live_run_is_never_queued_again() {
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
        let record = waiting_job(&h);
        h.first_item_gate.shut();
        h.host.start();
        let answer = his_answer(&h, &record, "answer-1", "When should it ship? You answered: tomorrow");
        until_prompts(&h, 1);
        assert_eq!(crate::question_work::pending(&h.state).unwrap().len(), 1, "let go before the back end had it");
        // Reader 2 on a store wake, while the answer is in flight.
        h.host.queue_question_answer(&h.binding, &answer).unwrap();
        let backend = h.host.backend("thread-one").unwrap();
        {
            let inner = backend.inner.lock().unwrap();
            assert!(inner.queue.is_empty(), "the live job was queued again for the answer it carries");
            assert_eq!(inner.carrying, ["answer-1"]);
        }
        h.first_item_gate.release();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        assert!(crate::question_work::pending(&h.state).unwrap().is_empty(), "taken at the first item");
        assert!(crate::question_work::peek(&h.state, "depot", "thread-one", &record.obligation_id, "work-session-one")
            .unwrap().is_empty(), "taken in this lease's own session");
        assert!(backend.inner.lock().unwrap().carrying.is_empty());
        let prompts = h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.iter().filter(|p| p.contains("You answered: tomorrow")).count(), 1, "{prompts:?}");
        assert!(!h.host.wait_for_completed(2, std::time::Duration::from_millis(300)), "a second run of the job");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// A second host over the same engine state: the relaunch. Its own open first-item gate, so
    /// a first host held at its gate does not hold this one.
    fn relaunch(h: &Harness) -> Arc<WorkHost> {
        let factory = WorkFactory { first_item_gate: StartGate::open_now(), ..factory_over(h, 5) };
        let host = WorkHost::new(&h.state, Box::new(factory), h.notices.clone(), Arc::clone(&h.desk));
        host.start();
        host
    }

    /// §4.1 test 2 (P4, D6). The host dies after carrying his answer and before the back end's
    /// first item. On disk the answer is still pending, and recovery puts the job back to
    /// waiting on it with NO "was running when RichOS closed" notice. The relaunch sends it once.
    #[test]
    fn a_crash_before_the_first_item_leaves_his_answer_pending_and_the_relaunch_sends_it_once() {
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
        let record = waiting_job(&h);
        h.first_item_gate.shut();
        h.host.start();
        let answer = his_answer(&h, &record, "answer-1", "When should it ship? You answered: tomorrow");
        until_prompts(&h, 1);
        // The crash: this host is gone from here on. What a relaunch finds is on disk.
        let row = assignment::read(&h.state, "depot", "thread-one", &record.id).unwrap();
        assert_eq!(row.state, AssignmentState::Preparing, "{}", row.detail);
        assert_eq!(crate::question_work::pending(&h.state).unwrap().len(), 1, "lost before the back end had it");
        let report = crate::recovery::reconcile(&h.state, &crate::recovery::UnreadableRepositories);
        assert!(report.unknown.is_empty(), "{report:?}");
        assert_eq!(report.answers_waiting.len(), 1, "{report:?}");
        let row = assignment::read(&h.state, "depot", "thread-one", &record.id).unwrap();
        assert_eq!((row.state, row.detail.as_str()), (AssignmentState::Blocked, crate::recovery::ANSWER_SAVED_AT_RELAUNCH));
        assert!(row.notices.is_empty(), "he was told a job that never started was running: {:?}", row.notices);

        let after = relaunch(&h);
        after.queue_question_answer(&h.binding, &answer).unwrap(); // the launch wake
        assert!(after.wait_for_completed(1, std::time::Duration::from_secs(10)));
        let prompts = h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.iter().filter(|p| p.contains("You answered: tomorrow")).count(), 2,
                   "one prompt from the host that died, one from the relaunch: {prompts:?}");
        assert!(crate::question_work::pending(&h.state).unwrap().is_empty());
        assert!(!after.wait_for_completed(2, std::time::Duration::from_millis(300)), "sent twice after the relaunch");
        after.shutdown();
        h.first_item_gate.release();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)), "the host that died never let go");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// §4.1 test 3 (P5). The host dies after the back end's first item. The answer is taken, the
    /// job is `Unknown` with the ordinary notice, and nothing re-runs it: §6.3, and his notice
    /// says nothing is running.
    #[test]
    fn a_crash_after_the_first_item_leaves_the_job_unknown_and_nothing_re_runs_it() {
        let h = harness(3000);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
        *h.answer_reply.lock().unwrap() = "Shipping tomorrow.".into();
        let record = waiting_job(&h);
        h.host.start();
        let answer = his_answer(&h, &record, "answer-1", "When should it ship? You answered: tomorrow");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while assignment::read(&h.state, "depot", "thread-one", &record.id).unwrap().state != AssignmentState::Running {
            assert!(std::time::Instant::now() < deadline, "the first item never came");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        // The crash, mid-turn.
        assert!(crate::question_work::pending(&h.state).unwrap().is_empty(), "taken at the first item");
        let report = crate::recovery::reconcile(&h.state, &crate::recovery::UnreadableRepositories);
        assert_eq!(report.unknown.len(), 1, "{report:?}");
        assert!(report.answers_waiting.is_empty());
        let after = relaunch(&h);
        after.queue_question_answer(&h.binding, &answer).unwrap(); // the launch wake
        assert!(!after.wait_for_completed(1, std::time::Duration::from_millis(400)), "a started job re-ran by itself");
        assert_eq!(h.work_prompts.lock().unwrap().len(), 1);
        after.shutdown();
        h.host.shutdown();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)), "the host that died never let go");
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **A job RichOS closed on while it ran is never re-run by his answer, and his word picks
    /// it back up WITH every answer he gave it** (the work-path design's C6, option B; Rich's
    /// call). He answered a second question while its turn ran; RichOS closed. After the
    /// relaunch the answer is saved and nothing runs. When he says to pick it back up, the same
    /// assignment runs once: the note first, the answer the dead back end had as an earlier one,
    /// and the answer it never saw as new. He is never asked twice.
    #[test]
    fn a_started_job_waits_for_his_word_and_the_pick_up_carries_every_answer_he_gave() {
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
        witnessed(&h.state, "work-session-one");
        let record = waiting_job(&h);
        open_question(&h, &record);
        h.host.start();
        his_answer(&h, &record, "answer-1", "Which branch? You answered: integration");
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        // Its next run is going when he answers again and RichOS closes: the crash, on disk.
        assignment::advance(&h.state, "depot", "thread-one", &record.id, AssignmentState::Running, "The back end has started on it.").unwrap();
        let second = crate::questions::Delivery { obligation_id:None,
            id: "answer-2".into(), entity_id: "depot".into(), thread_id: "thread-one".into(),
            asker: record.obligation_id.clone(), set_id: None, text: "Who reviews? You answered: Dana".into(), receipt: None,
        };
        crate::question_work::enqueue(&h.state, &second).unwrap();
        // A crash, not a quit: nothing of the first host runs again, and it is not shut down (a
        // quit would mark the job Interrupted, which is a witnessed stop).
        let report = crate::recovery::reconcile(&h.state, &crate::recovery::UnreadableRepositories);
        assert_eq!(report.unknown.len(), 1, "{report:?}");

        let after = relaunch(&h);
        after.queue_question_answer(&h.binding, &second).unwrap(); // the launch wake
        assert!(!after.wait_for_completed(1, std::time::Duration::from_millis(400)), "a started job re-ran by itself");
        assert_eq!(h.work_prompts.lock().unwrap().len(), 1);
        assert_eq!(crate::question_work::pending(&h.state).unwrap().len(), 1, "his answer was not kept");

        // His word.
        assignment::pick_up(&h.state, "depot", "thread-one", &record.title).unwrap();
        // Explicit legacy intake resumes this saved assignment. Desktop peer
        // delivery never dispatches an operational record at the boundary.
        let picked = assignment::read(&h.state, "depot", "thread-one", &record.id).unwrap();
        assert!(after.enqueue(&h.binding, picked));
        assert!(after.wait_for_completed(1, std::time::Duration::from_secs(10)));
        let prompts = h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(), 2, "{prompts:?}");
        let picked = &prompts[1];
        assert!(picked.starts_with(PICKED_UP_NOTE), "{picked}");
        assert!(picked.contains("Answers he gave earlier on this assignment") && picked.contains("You answered: integration"), "{picked}");
        assert!(picked.contains("You answered: Dana"), "{picked}");
        assert!(crate::question_work::pending(&h.state).unwrap().is_empty(), "the pick-up's back end took it");
        after.shutdown();
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// §4.1 test 12 (D7) and §3's lock order. His answer goes through the real store: while
    /// the back end has not taken it, his card does not say "Rich has your answer"; once it
    /// has, it does. And the back end taking it (which writes the store) never waits on the
    /// host's `inner` while a delivery holds the store lock and asks for `inner` (the sink,
    /// `queue_question_answer`): a second answer is delivered at exactly that moment, and both
    /// finish.
    #[test]
    fn his_card_says_rich_has_it_only_once_taken_and_the_store_lock_never_waits_on_inner() {
        use crate::questions::{AnswerRequest, Store};
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
        let record = waiting_job(&h);
        let store = Store::new(&h.state);
        let answer = |set: usize, key: &str| {
            let q = store.list("depot", "thread-one").unwrap().into_iter()
                .filter(|q| q.state == crate::questions::State::Open).nth(set).unwrap();
            store.answer("depot", "thread-one", AnswerRequest { question_id: q.id.clone(), client_id: key.into(),
                option_ids: vec![q.options[1].id.clone()], text: String::new(), expected_revision: None }, "click", "mac").unwrap();
            q.id
        };
        open_question(&h, &record);
        open_question(&h, &record);
        let first = answer(0, "first");
        h.first_item_gate.shut();
        h.host.start();
        store.deliver("depot", "thread-one", &record.obligation_id, |d| h.host.queue_question_answer(&h.binding, d)).unwrap();
        until_prompts(&h, 1);
        let card = |id: &str| store.list("depot", "thread-one").unwrap().into_iter().find(|q| q.id == id).unwrap().public_value();
        assert_eq!(card(&first)["delivered"], false, "\"Rich has your answer\" before the back end had it");

        // The second answer is delivered while the first one's taking waits for the store lock.
        let second = answer(0, "second");
        let (done, finished) = std::sync::mpsc::channel();
        let (host, binding, gate, state, asker) =
            (h.host.clone(), h.binding.clone(), h.first_item_gate.clone(), h.state.clone(), record.obligation_id.clone());
        std::thread::spawn(move || {
            let result = Store::new(&state).deliver("depot", "thread-one", &asker, |d| {
                gate.release();
                // The runner reaches the store's lock (the card write) and waits on it here.
                std::thread::sleep(std::time::Duration::from_millis(200));
                host.queue_question_answer(&binding, d)
            });
            drop(done.send(result.map_err(|e| e.to_string())));
        });
        let delivered = finished.recv_timeout(std::time::Duration::from_secs(20));
        assert!(matches!(delivered, Ok(Ok(1))), "deadlock or failure: {delivered:?}");
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(20)));
        assert_eq!(card(&first)["delivered"], true);
        assert_eq!(card(&second)["delivered"], true);
        assert!(crate::question_work::pending(&h.state).unwrap().is_empty());
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **C10: a job that ends on an early exit lets go of his answers, and an answer run whose
    /// back end will not open is D4's retry.** (1) The back end refuses to open once: the job
    /// waits on his answer again and the second run carries it, never "did not start". (2) His
    /// original request can no longer be verified, so the run ends before any lease: his open
    /// question is withdrawn and his saved answer let go, rather than re-queued and refused on
    /// every wake for ever.
    #[test]
    fn an_early_exit_lets_his_answer_go_and_a_back_end_that_will_not_open_is_retried() {
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
        h.refuse_next.store(true, Ordering::SeqCst);
        let record = waiting_job(&h);
        h.host.start();
        his_answer(&h, &record, "answer-1", "When should it ship? You answered: tomorrow");
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)), "no retry");
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1, "the refused open, then one lease");
        assert_eq!(h.work_prompts.lock().unwrap().iter().filter(|p| p.contains("You answered: tomorrow")).count(), 1);
        assert!(crate::question_work::pending(&h.state).unwrap().is_empty());
        assert!(told(&h).iter().all(|t| !t.contains("did not start")), "{:?}", told(&h));
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();

        let h = harness(5);
        let receipt = assignment::register(&h.state, &Registration { instruction_sha256: "0".repeat(64), ..registration(&h) }).unwrap();
        assignment::advance(&h.state, "depot", "thread-one", &receipt.id, AssignmentState::Blocked, "Waiting.").unwrap();
        let record = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        open_question(&h, &record);
        h.host.start();
        his_answer(&h, &record, "answer-1", "When should it ship? You answered: tomorrow");
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &record.id).unwrap();
        assert_eq!(row.state, AssignmentState::Failed, "{}", row.detail);
        assert!(crate::question_work::pending(&h.state).unwrap().is_empty(), "left to be re-queued and refused on every wake");
        let open: Vec<_> = crate::questions::Store::new(&h.state).list("depot", "thread-one").unwrap()
            .into_iter().filter(|q| q.state == crate::questions::State::Open).collect();
        assert!(open.is_empty(), "a question left open for a job that ended");
        assert_eq!(h.spawns.load(Ordering::SeqCst), 0, "nothing was asked of a back end");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **D8: the question worker's pass, as it ships.** His answer in the durable store reaches
    /// the job through `deliver_team_answers` alone (the pass the app's worker thread now
    /// calls), once; a second pass, the launch wake, sends nothing more. An answer to his team
    /// with no team sink stays saved, never handed to a work job.
    #[test]
    fn the_question_workers_pass_delivers_his_answer_once_and_leaves_his_teams_saved() {
        use crate::questions::{AnswerRequest, Store};
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
        let record = waiting_job(&h);
        open_question(&h, &record);
        let store = Store::new(&h.state);
        let q = store.list("depot", "thread-one").unwrap().remove(0);
        store.answer("depot", "thread-one", AnswerRequest { question_id: q.id.clone(), client_id: "tap".into(),
            option_ids: vec![q.options[0].id.clone()], text: String::new(), expected_revision: None }, "click", "mac").unwrap();
        crate::question_work::enqueue(&h.state, &crate::questions::Delivery { obligation_id:None,
            id: "operator-answer".into(), entity_id: "depot".into(), thread_id: "thread-one".into(),
            asker: "operator:conversation".into(), set_id: None, text: "for his team".into(), receipt: None,
        }).unwrap();
        h.host.start();
        let binding = h.binding.clone();
        let binding_for = move |thread: &str| (thread == "thread-one").then(|| binding.clone());
        h.host.deliver_team_answers(&binding_for, None);
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        h.host.deliver_team_answers(&binding_for, None); // the launch wake, again
        assert!(!h.host.wait_for_completed(2, std::time::Duration::from_millis(300)), "sent twice");
        let prompts = h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.iter().filter(|p| p.contains("Dana reviews")).count(), 1, "{prompts:?}");
        assert!(prompts.iter().all(|p| !p.contains("for his team")), "his team's answer reached a work job");
        let pending = crate::question_work::pending(&h.state).unwrap();
        assert_eq!(pending.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(), ["operator-answer"]);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// §4.1 test 6 (D3). The run that carried his answer starts a command that outlives its
    /// wait: the job is WATCHED, and never left blocked on the answer it already took.
    #[test]
    fn a_taken_answer_never_blocks_its_own_run_and_a_long_command_is_still_watched() {
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
        *h.background.lock().unwrap() = Some(Vec::new());
        h.host.set_command_wait_budget(std::time::Duration::from_millis(100));
        *h.answer_reply.lock().unwrap() = "Shipping tomorrow; the import is running.".into();
        h.start_in_turn.lock().unwrap().push_back(vec![background_command("bimport", "import.sh")]);
        let record = waiting_job(&h);
        h.host.start();
        his_answer(&h, &record, "answer-1", "When should it ship? You answered: tomorrow");
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &record.id).unwrap();
        assert_eq!((row.state, row.detail.as_str()), (AssignmentState::Running, COMMAND_STILL_RUNNING_DETAIL),
                   "a job blocked on the answer it already took");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// §4.1 test 7 (D4, a dead lease). The prompt carrying his answer fails before streaming: the
    /// job goes back to waiting on it, ONE fresh lease is opened, and the answer reaches it. On a
    /// second failure the job fails, with exactly one notice, and never as "stopped before it
    /// finished" about a turn that never ran (C4).
    #[test]
    fn a_dead_lease_is_a_delivery_error_retried_once_on_a_fresh_lease() {
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
        *h.answer_reply.lock().unwrap() = "Noted, shipping tomorrow.".into();
        h.fail_next.lock().unwrap().push_back("claude channel closed (child exited?)".into());
        let record = waiting_job(&h);
        h.host.start();
        his_answer(&h, &record, "answer-1", "When should it ship? You answered: tomorrow");
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)), "no retry");
        assert_eq!(h.spawns.load(Ordering::SeqCst), 2, "one fresh lease for the retry");
        assert!(crate::question_work::pending(&h.state).unwrap().is_empty(), "the retry's back end took it");
        assert!(crate::question_work::peek(&h.state, "depot", "thread-one", &record.obligation_id, "work-session-rotated-1")
            .unwrap().is_empty(), "taken by the fresh lease");
        let row = assignment::read(&h.state, "depot", "thread-one", &record.id).unwrap();
        assert_ne!(row.state, AssignmentState::Blocked, "{}", row.detail);
        assert!(told(&h).iter().all(|t| !t.contains("did not take your answer")), "{:?}", told(&h));
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();

        // Past the bound: two failures.
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
        h.fail_next.lock().unwrap().extend(["claude channel closed (child exited?)".to_string(), "claude channel closed (child exited?)".to_string()]);
        let record = waiting_job(&h);
        h.host.start();
        his_answer(&h, &record, "answer-1", "When should it ship? You answered: tomorrow");
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)));
        assert!(!h.host.wait_for_completed(3, std::time::Duration::from_millis(300)), "retried past the bound");
        let row = assignment::read(&h.state, "depot", "thread-one", &record.id).unwrap();
        assert_eq!(row.state, AssignmentState::Failed, "{}", row.detail);
        let notices = told(&h);
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert!(!notices[0].contains("stopped before it finished"), "{notices:?}");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// §4.1 test 8 (D4, refused). The child refuses the message: nothing ran, the same lease is
    /// asked again (a fresh uuid per prompt is `native.rs`'s), and he never hears that a job
    /// "stopped before it finished". Past the bound, one notice that says what happened.
    #[test]
    fn a_refused_answer_turn_is_asked_again_and_never_reported_as_a_job_that_stopped() {
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
        *h.answer_reply.lock().unwrap() = "Noted, shipping tomorrow.".into();
        h.unrun.lock().unwrap().push_back("refused_before_it_ran".into());
        let record = waiting_job(&h);
        h.host.start();
        his_answer(&h, &record, "answer-1", "When should it ship? You answered: tomorrow");
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)));
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1, "a refusal does not retire the lease");
        assert!(crate::question_work::pending(&h.state).unwrap().is_empty());
        assert_eq!(h.work_prompts.lock().unwrap().iter().filter(|p| p.contains("You answered: tomorrow")).count(), 2);
        assert!(told(&h).iter().all(|t| !t.contains("stopped before it finished")), "{:?}", told(&h));
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();

        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
        h.unrun.lock().unwrap().extend(["discarded_before_it_ran".to_string(), "refused_before_it_ran".to_string()]);
        let record = waiting_job(&h);
        h.host.start();
        his_answer(&h, &record, "answer-1", "When should it ship? You answered: tomorrow");
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)));
        assert!(!h.host.wait_for_completed(3, std::time::Duration::from_millis(300)), "retried past the bound");
        let row = assignment::read(&h.state, "depot", "thread-one", &record.id).unwrap();
        assert_eq!((row.state, row.detail.as_str()), (AssignmentState::Failed, ANSWER_NOT_TAKEN));
        let notices = told(&h);
        assert_eq!(notices.len(), 1, "{notices:?}");
        assert!(notices[0].contains(ANSWER_NOT_TAKEN) && !notices[0].contains("stopped before it finished"), "{notices:?}");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// §4.1 test 9 (D4 and Stop). His Stop while the answer is on its way ends the job as his
    /// stop, as today, and nothing is retried.
    #[test]
    fn his_stop_outranks_the_retry_of_an_untaken_answer() {
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
        h.fence.stop_ends_child.store(true, Ordering::SeqCst);
        let record = waiting_job(&h);
        h.first_item_gate.shut();
        h.host.start();
        his_answer(&h, &record, "answer-1", "When should it ship? You answered: tomorrow");
        until_prompts(&h, 1);
        h.host.stop_assignment("depot", "thread-one", &record.id).unwrap();
        h.first_item_gate.release();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        assert!(!h.host.wait_for_completed(2, std::time::Duration::from_millis(300)), "retried after his Stop");
        let row = assignment::read(&h.state, "depot", "thread-one", &record.id).unwrap();
        assert_eq!(row.state, AssignmentState::Interrupted, "{}", row.detail);
        assert_eq!(told(&h), [assignment::says::interrupted(&record.title)]);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// §4.1 test 10 (D5). His first answer was taken by a back end that has since been retired;
    /// he answers a second question. The fresh back end is told both, the first under its own
    /// heading. On the SAME back end only the new one is sent.
    #[test]
    fn a_fresh_back_end_is_told_the_answers_a_retired_one_had() {
        for retire in [true, false] {
            let h = harness(5);
            every_worker_observed_ending(&h);
            *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
            let record = waiting_job(&h);
            // It asks him a second question, so its runs end waiting on him rather than ending.
            open_question(&h, &record);
            h.host.start();
            his_answer(&h, &record, "answer-1", "Which branch? You answered: integration");
            assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
            assert_eq!(assignment::read(&h.state, "depot", "thread-one", &record.id).unwrap().state, AssignmentState::Blocked);
            if retire {
                let backend = h.host.backend("thread-one").unwrap();
                *backend.lease.lock().unwrap() = None;
                backend.inner.lock().unwrap().lease_session = None;
            }
            his_answer(&h, &record, "answer-2", "When should it ship? You answered: tomorrow");
            assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)));
            let prompts = h.work_prompts.lock().unwrap().clone();
            let second = &prompts[1];
            assert!(second.contains("You answered: tomorrow"), "{second}");
            let earlier = second.find("Answers he gave earlier on this assignment");
            if retire {
                let at = earlier.expect("the fresh back end was not told his earlier answer");
                assert!(second[at..].contains("You answered: integration"), "{second}");
                assert!(second.find("You answered: integration") < second.find("You answered: tomorrow"));
            } else {
                assert!(earlier.is_none() && !second.contains("You answered: integration"),
                        "the same back end was told an answer it already had: {second}");
            }
            h.host.shutdown();
            std::fs::remove_dir_all(&h.root).unwrap();
        }
    }

    /// §4.1 test 11. An inbox file written before `taken_in` existed (`handed: true` and nothing
    /// else) is an answer a session that is gone took: told once to the back end running now,
    /// never counted as pending, and not told to that back end again.
    #[test]
    fn a_legacy_taken_answer_is_told_once_to_a_fresh_back_end() {
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
        let record = waiting_job(&h);
        let legacy = crate::questions::Delivery { obligation_id:None,
            id: "legacy-1".into(), entity_id: "depot".into(), thread_id: "thread-one".into(),
            asker: record.obligation_id.clone(), set_id: None, text: "Which branch? You answered: integration".into(), receipt: None,
        };
        crate::question_work::enqueue(&h.state, &legacy).unwrap();
        let file = std::fs::read_dir(h.state.join("questions/work-inputs")).unwrap().next().unwrap().unwrap().path();
        std::fs::write(&file, serde_json::json!({"delivery": legacy, "handed": true}).to_string() + "\n").unwrap();
        assert!(crate::question_work::pending(&h.state).unwrap().is_empty());
        open_question(&h, &record);
        h.host.start();
        his_answer(&h, &record, "answer-2", "When should it ship? You answered: tomorrow");
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        his_answer(&h, &record, "answer-3", "Who reviews? You answered: Dana");
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)));
        let prompts = h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.iter().filter(|p| p.contains("You answered: integration")).count(), 1, "{prompts:?}");
        assert!(prompts[0].contains("Answers he gave earlier on this assignment"));
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **A watched job that comes back for his answer keeps its command** (bgdone2, after the
    /// questions branch landed). `queue_question_answer` puts the asking assignment back on the
    /// queue, and it may be on the watch at that moment. The re-run must treat the command as
    /// its own — wait for it, hand it to the watch again — rather than count it as already
    /// seen, settle without it, and leave the watch to report on a closed job.
    #[test]
    fn a_watched_job_rerun_for_his_answer_still_reports_its_command_once() {
        use crate::cognition::ObligationState;
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        *h.background.lock().unwrap() = Some(Vec::new());
        h.host.set_command_wait_budget(std::time::Duration::from_millis(100));
        const STARTED: &str = "Started the import.";
        const NOTED: &str = "Noted: ship tomorrow. The import is still running.";
        const DONE: &str = "The import finished: 12 rows.";
        h.replies.lock().unwrap().extend([STARTED, NOTED, DONE].map(String::from));
        h.start_in_turn.lock().unwrap().push_back(vec![background_command("bimport", "import.sh")]);
        h.host.start();
        let job = h.host.register(&h.binding, &registration(&h)).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        let record = assignment::read(&h.state, "depot", "thread-one", &job.id).unwrap();
        assert_eq!(record.detail, COMMAND_STILL_RUNNING_DETAIL);

        let answer = crate::questions::Delivery { obligation_id:None,
            id: "answer-1".into(), entity_id: "depot".into(), thread_id: "thread-one".into(),
            asker: record.obligation_id.clone(), set_id: None,
            text: "When should it ship? You answered: tomorrow".into(), receipt: None,
        };
        crate::question_work::enqueue(&h.state, &answer).unwrap();
        h.host.queue_question_answer(&h.binding, &answer).unwrap();
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &job.id).unwrap();
        assert_eq!((row.state, row.detail.as_str()), (AssignmentState::Running, COMMAND_STILL_RUNNING_DETAIL),
                   "the re-run settled without its own command");

        background_command_ends(&h, "bimport", false);
        assert!(h.host.wait_for_completed(3, std::time::Duration::from_secs(10)));
        let prompts = h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(), 3, "{prompts:?}");
        assert!(prompts[1].contains("You answered: tomorrow"), "{}", prompts[1]);
        assert!(!prompts[1].contains("\"import.sh\" (for"), "its own command was listed as another job's");
        assert!(prompts[2].contains(COMMAND_ENDED_HEAD), "{}", prompts[2]);
        let told: Vec<String> = h.notices.0.lock().unwrap().iter().map(|(_, n)| n.text.clone()).collect();
        assert_eq!(told, [STARTED, NOTED, DONE], "{told:?}");
        assert!(assignment::read(&h.state, "depot", "thread-one", &job.id).unwrap().was_answered());
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **A QUESTION THE BACK END NEVER ANSWERED is told to him as that, not as a job that
    /// stopped part way.**
    ///
    /// `says::failed` and `says::did_not_start` both put the title where a job's name goes,
    /// so for a question they produce *"why the nightly is red stopped before it finished"* —
    /// which is not English and not a thing anybody would say to him. The reason still
    /// travels in full; only the shape of the sentence changes.
    #[test]
    fn a_question_that_got_no_answer_is_said_as_that_and_not_as_a_job_that_stopped() {
        use crate::cognition::ObligationState;
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        // The back end said nothing at all — the fall-through the question arm deliberately
        // does not catch, because there is no positive evidence of an answer.
        assert!(h.answer_reply.lock().unwrap().is_empty());
        let _runner = h.host.start();
        let receipt = h
            .host
            .register_kind(
                &h.binding,
                &Registration { title: "why the nightly has been red since Tuesday".into(), ..registration(&h) },
                assignment::AssignmentKind::Check,
            )
            .unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        let row = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(row.state, AssignmentState::Failed, "silence was read as an answer");
        let notice = h.notices.0.lock().unwrap().last().unwrap().1.clone();
        assert!(
            notice.text.starts_with("I couldn't get you an answer on why the nightly has been red since Tuesday."),
            "{}",
            notice.text
        );
        // The job-shaped sentences are gone from a question, in both directions.
        assert!(!notice.text.contains("stopped before it finished"), "{}", notice.text);
        assert!(!notice.text.contains("did not start"), "{}", notice.text);
        // The reason was not softened away with the sentence.
        assert!(notice.text.len() > 80, "the reason was dropped: {}", notice.text);
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **THE BRIEF A QUESTION IS SENT WITH, and the three claims it must not invite.**
    #[test]
    fn a_question_is_asked_as_a_question_and_never_told_to_land_anything() {
        let record = Assignment {
            schema: 1,
            id: "assignment-id-that-must-not-appear".into(),
            obligation_id: "obligation-that-must-not-appear".into(),
            seat: "work-seat:obligation-that-must-not-appear".into(),
            entity_id: "depot".into(),
            thread_id: "thread-one".into(),
            instruction_ledger_ref: "ledger:thread-one:turn-7".into(),
            instruction_sha256: "abc".into(),
            title: "why the nightly has been red since Tuesday".into(),
            handoff: String::new(),
            kind: assignment::AssignmentKind::Check,
            // §56's field, false here: answering a question does not need the Mac's screen,
            // and this test is about what the brief says rather than about the screen.
            needs_screen: false,
            repositories: vec!["/fictional/project".into()],
            state: AssignmentState::Registered,
            detail: String::new(),
            registered_at_ms: 0,
            updated_at_ms: 0,
            user_stop_requested_at_ms: None,
            notices: Vec::new(),
            work_session: None,
            repository_pins: Vec::new(),
            waiting_sentence: false,
        };
        let brief = brief_for(&record, false, &record.title);
        let input: serde_json::Value = serde_json::from_str(&brief).unwrap();
        assert_eq!(input["assignment"]["question"], true);
        assert_eq!(input["assignment"]["user_request"], record.title);
        assert_eq!(input["assignment"]["repositories"][0], "/fictional/project");
        assert!(!brief.contains("check") && !brief.contains("investigate"));
        // No identifier travels, exactly as with a task.
        assert!(!brief.contains("assignment-id-that-must-not-appear"));
        assert!(!brief.contains("obligation-that-must-not-appear"));
        assert!(!brief.contains("work-seat:"));
        assert!(!brief.contains("ledger:"));
    }

    /// `and_list` is the spoken form: one, two, or several. A background job can land in more
    /// than one repository, and a comma-separated list read aloud is one he cannot parse.
    #[test]
    fn a_list_of_lands_is_readable_aloud() {
        let of = |items: &[&str]| and_list(&items.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(of(&[]), "");
        assert_eq!(of(&["one"]), "one");
        assert_eq!(of(&["one", "two"]), "one and two");
        assert_eq!(of(&["one", "two", "three"]), "one, two and three");
    }

    /// **A real question of his for this obligation, saved and unanswered.** Since 2026-10-13
    /// it is the one thing that leaves background work waiting on him: a STEP of the work never
    /// does (`permissions::BACKGROUND_STEP_REFUSED`). Raised before the work turn settles,
    /// which is the real order: the job asks, then its turn ends around the question.
    fn a_question_for_him(h: &Harness, obligation: &str) {
        use crate::questions::{AskScope, OptionInput, QuestionInput, Store};
        Store::new(&h.state).ask(&AskScope { obligation_id:None,
            root: h.state.clone(), entity_id: "depot".into(), thread_id: "thread-one".into(),
            turn_id: "turn-7".into(), asker: obligation.into(), session_id: "work-session-one".into(),
            engine: None, entity_root: None, app_run: None,
        }, vec![QuestionInput {
            text: "Which client should this go to first?".into(),
            options: vec![
                OptionInput { label: "Acme first".into(), description: "The larger account.".into() },
                OptionInput { label: "Birch first".into(), description: "The earlier deadline.".into() },
            ],
            multiple: false, free_answer: true, recommended: None,
        }]).unwrap();
    }

    /// He answers it some other way (or it is no longer needed): the question is withdrawn,
    /// so the next run of the job is not held on it again.
    fn the_question_is_settled(h: &Harness, obligation: &str) {
        let store = crate::questions::Store::new(&h.state);
        for question in store.list("depot", "thread-one").unwrap().into_iter().filter(|q| q.asker == obligation) {
            store.withdraw("depot", "thread-one", obligation, &question.id, "answered in the fixture").unwrap();
        }
    }

    /// **A WORKER's step, raised between the lease's turns, is refused and never waits on him**
    /// (the CEO, 2026-10-13: *"Is the CEO user supposed to keep babysitting Rich like this all
    /// the time OR WHAT???"*, over nightly 54's "Approve this" on a job of reading his website).
    ///
    /// The back end's `prepare` launches its worker in the background and the lead's turn ends
    /// at once, so the worker's `Bash` arrives with the scope as an ordinary turn end leaves
    /// it: `actions_allowed: false`, `background_work_allowed: true`. It used to wait on the
    /// job's row as an Approve (esc-20261005T150541Z-ee581ad7); now it is refused at once with
    /// the sentence that tells the model to find another way or report, the row has nothing
    /// awaiting him, and the update gate counts the job as running, not as waiting on him.
    ///
    /// The control: once the work is STOPPED (`ecs::revoke` clears both flags), the step is
    /// refused too, by the closed grant, and still nothing reaches his screen.
    #[test]
    fn a_workers_step_between_the_leases_turns_is_refused_and_never_waits_on_him() {
        let h = harness(5);
        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();
        let row = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        let scope = |actions: bool, workers: bool| {
            serde_json::json!({"version":1,"actions_allowed":actions,"background_work_allowed":workers,
                "binding":{"entity_id":"depot","thread_id":"thread-one","session_id":"work-session-one",
                    "turn_id":row.obligation_id,"audience":"worker","revision":1}})
            .to_string()
        };
        let path = h.root.join("work-grant-between-turns.json");
        std::fs::write(&path, scope(false, true)).unwrap();
        let lease = crate::permissions::ScopedPermissions { desk: Arc::clone(&h.desk), scope: path.clone() };
        let dismissal = serde_json::json!({"tool_name":"Bash","input":{
            "command":"'/fictional/runtime/python3' '/fictional/engine/scripts/lib/pierce.py' --dismiss 0f --reason broken"}});
        // A hang guard, never a verdict: a desk that queued this would hold the call for the
        // whole minute and then fail every assertion below.
        let worker = {
            let (lease, dismissal) = (lease.clone(), dismissal.clone());
            std::thread::spawn(move || lease.decide_within(&dismissal, std::time::Duration::from_secs(60)))
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while !worker.is_finished() && std::time::Instant::now() < deadline {
            assert!(h.host.pending_decision(&row).is_none(), "a worker's step was put on its job's row for his approval");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        match worker.join().unwrap() {
            crate::native::PermissionDecision::Deny { message } => {
                assert_eq!(message, crate::permissions::BACKGROUND_STEP_REFUSED, "the step was not refused as background work")
            }
            other => panic!("a worker's step was approved on his behalf: {other:?}"),
        }
        assert!(h.host.pending_decision(&row).is_none(), "a worker's step is waiting on him");
        assert!(h.desk.background_queue().is_empty(), "a worker's step is in his queue");
        assert_eq!(h.host.background_work().awaiting_you, 0, "the job is counted as waiting on him");

        // The control: a stopped job's worker gets nothing either, and nothing reaches him.
        std::fs::write(&path, scope(false, false)).unwrap();
        assert_eq!(lease.decide_within(&dismissal, std::time::Duration::from_secs(5)).behavior(), "deny");
        assert!(h.desk.background_queue().is_empty(), "a stopped job's worker put a question in front of him");

        h.host.shutdown();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    // ---- §6.4: what the update gate sees -------------------------------------------------

    /// **The hole §6.4 names, closed end to end** — the same reading the shell makes, made
    /// against a real work host with a real assignment on its lease and the CONVERSATION
    /// IDLE, which is the exact state the old gate called clear.
    ///
    /// It is one function rather than a re-implementation of the shell's counting: the shell
    /// calls `background_work()` and passes the answer to `work_gate::background`, and so
    /// does this test. A test that counted the rows itself would prove that two copies of the
    /// arithmetic agree.
    #[test]
    fn a_running_assignment_blocks_an_update_with_the_conversation_idle() {
        use crate::work_gate::{self, Liveness, WorkSources};
        let h = harness(3000);
        let _runner = h.host.start();

        // POSITIVE CONTROL FIRST, and it is the half that matters most: with nothing
        // registered the gate is CLEAR and the app updates exactly as it did before §6.4.
        // Without it, "everything now blocks" would pass this test.
        let quiet = work_gate::background(&h.host.background_work(), &[]);
        assert_eq!(quiet.0, Liveness::Clear);
        let idle = work_gate::decide(&WorkSources {
            background: quiet.0,
            background_gap: quiet.1,
            ..WorkSources::all_clear()
        });
        assert!(!idle.busy, "an app with no background work could never update");

        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();
        until_live(&h.host);
        let reading = work_gate::background(&h.host.background_work(), &[]);
        assert_eq!(reading.0, Liveness::Busy, "a live background assignment was invisible to the gate");
        let verdict = work_gate::decide(&WorkSources {
            // Every conversation source clear: no turn, the spine free, no workers of its
            // own. This is the state §6.4 says an update would have installed in.
            background: reading.0,
            background_gap: reading.1,
            ..WorkSources::all_clear()
        });
        assert!(verdict.busy, "an update could have installed over live background work");
        assert_eq!(verdict.reason.as_deref(), Some("1 assignment is still running in the background."));

        // A step of that work which the provider would not approve on its own is refused
        // (2026-10-13) and changes nothing the gate reads: still running, nothing awaiting him.
        let (grant, lease) = work_lease_desk(&h, "obligation-7");
        assert_eq!(
            lease.decide_within(&serde_json::json!({"tool_name":"Bash","input":{}}), std::time::Duration::from_secs(60)).behavior(),
            "deny"
        );
        assert_eq!(h.host.background_work().awaiting_you, 0, "a refused step left the job waiting on him");
        assert_eq!(h.host.background_work().running, 1);

        h.host.stop_assignment("depot", "thread-one", &receipt.id).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while h.host.background_work().running + h.host.background_work().awaiting_you > 0
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(
            work_gate::background(&h.host.background_work(), &[]).0,
            Liveness::Clear,
            "the gate never clears once an assignment has been registered"
        );
        h.host.shutdown();
        std::fs::remove_file(grant).unwrap();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **THE CEO's SHAPE, PINNED** — the Two Riches spec (richos-hq
    /// `docs/plans/two-riches-spec-2026-09-17.md`), in his own words: *"each conversation
    /// thread always holds one front desk Rich and one back-end Rich. Regardless of the
    /// number of assignments within a given conversation thread."*
    ///
    /// **Both halves of that sentence are asserted, and the second half is the correction.**
    /// §51 said one standing back end; the page says one PER CONVERSATION THREAD. So: three
    /// assignments' worth of work on one thread — two registrations and a resume — ask the
    /// factory for a back end exactly once, and a second THREAD gets its own rather than
    /// queueing behind the first.
    ///
    /// **The first half was already true and that is exactly why it needs a test.** It held
    /// by `ensure_lease` returning early when a lease exists — a shape nothing would have
    /// gone red for changing, in a file whose own doc says "per assignment" about the seat
    /// and the grant (which the page keeps: an assignment is a bookkeeping unit INSIDE the
    /// back end).
    #[test]
    fn one_back_end_per_conversation_thread_and_one_only() {
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
        let _runner = h.host.start();
        // A question of his on the FIRST assignment, raised before its turn ends, so it is
        // `blocked` and there is something to resume below (§52: a land asks him nothing, so
        // an open obligation on its own no longer leaves an assignment resumable).
        a_question_for_him(&h, "obligation-7");
        let first = h.host.register(&h.binding, &registration(&h)).unwrap();
        h.host
            .register(&h.binding, &Registration { obligation_id: "obligation-8".into(), ..registration(&h) })
            .unwrap();
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)));
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1, "a second assignment opened a second back end");

        // ...and a resume, which is the third time work goes onto the lease, does not open
        // one either.
        the_question_is_settled(&h, "obligation-7");
        let record = assignment::read(&h.state, "depot", "thread-one", &first.id).unwrap();
        h.host
            .apply_decision(
                &crate::ecs::Binding {
                    entity_id: "depot".into(),
                    thread_id: "thread-one".into(),
                    session_id: "work-session-one".into(),
                    turn_id: record.obligation_id.clone(),
                    audience: "worker".into(),
                    revision: 1,
                },
                true,
            )
            .unwrap();
        assert!(h.host.wait_for_completed(3, std::time::Duration::from_secs(10)));
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1, "a resume opened a second back end");

        // **A SECOND CONVERSATION IS A SECOND BACK END**, which is the page's correction to
        // §51 — and it doubles as the positive control for the "1" above: the counter can
        // move, so the reuse is a fact about one thread rather than about a counter that
        // never increments.
        let second_thread = ThreadBinding::new(
            PersonId::default_ceo(),
            EntityId::parse("depot").unwrap(),
            "thread-two",
            1,
        );
        h.host
            .register(
                &second_thread,
                &Registration {
                    thread_id: "thread-two".into(),
                    obligation_id: "obligation-9".into(),
                    // The ledger reference names the turn it came from, and the turn is on
                    // THAT conversation (`assignment::register` checks the two agree).
                    instruction_ledger_ref: "ledger:thread-two:turn-8".into(),
                    ..registration(&h)
                },
            )
            .unwrap();
        assert!(h.host.wait_for_completed(4, std::time::Duration::from_secs(10)));
        assert_eq!(h.spawns.load(Ordering::SeqCst), 2, "the second conversation shared the first's back end");
        assert_eq!(first.id.is_empty(), false);
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **A BACK END WHOSE TURN FAILED IS RETIRED, AND THE NEXT ASSIGNMENT GETS A FRESH
    /// ONE** — the second half of Ray's candidate-.10 walk, which nothing in this file could
    /// produce until `turn_error` existed.
    ///
    /// `ensure_lease` stands the back end up once and reuses it, which is right. But several
    /// of the errors that reach `run_one`'s `Err` arm have already killed the child on their
    /// way there — the three grant revocations and the worker-settlement check in
    /// `native.rs` all call `child.kill()` before returning — so without this, the slot holds
    /// a corpse and every later assignment on the thread is handed it.
    ///
    /// On the shipped candidate that is exactly what he saw: attempt 1 died at the
    /// settlement check; attempt 2, which he accepted from Rich's own offer, came back
    /// *"did not start"*, because `prompt` on the dead child returned before anything
    /// streamed and `took_the_turn` was therefore false. Two different failures, one cause,
    /// and the second told him nothing true about itself. The state root has ONE work-lease
    /// evidence session for those two attempts, which is the same fact from the other side.
    ///
    /// The assertion is the spawn count, not a message: 1 before, 2 after, with the second
    /// assignment settling on the fresh back end.
    #[test]
    fn a_work_turn_that_failed_retires_its_back_end_instead_of_handing_on_a_dead_one() {
        let h = harness(5);
        every_worker_observed_ending(&h);
        // The successor's session has its own evidence directory, so the second assignment
        // is settled by the same witnessed-ending rule and not by an unreadable view.
        let folder = h.state.join("evidence").join("work-session-rotated-1");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(".lock"), "").unwrap();
        std::fs::write(folder.join("callbacks.jsonl"), "").unwrap();
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Settled);
        let _runner = h.host.start();

        *h.turn_error.lock().unwrap() =
            Some("Worker settlement could not be verified at turn end.".into());
        let first = h.host.register(&h.binding, &registration(&h)).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1, "the first assignment did not open a back end");
        assert_eq!(
            assignment::read(&h.state, "depot", "thread-one", &first.id).unwrap().state,
            AssignmentState::Failed,
        );
        assert!(
            h.host.lease_sessions().is_empty(),
            "the failed back end is still standing, so the next assignment would be handed it",
        );

        // The next assignment opens a NEW connection and runs to a proper end on it.
        *h.turn_error.lock().unwrap() = None;
        let second = h
            .host
            .register(&h.binding, &Registration { obligation_id: "obligation-8".into(), ..registration(&h) })
            .unwrap();
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)));
        assert_eq!(h.spawns.load(Ordering::SeqCst), 2, "the second assignment reused the failed back end");
        assert_eq!(
            assignment::read(&h.state, "depot", "thread-one", &second.id).unwrap().state,
            AssignmentState::Settled,
            "the fresh back end could not settle a perfectly ordinary assignment",
        );
        assert_eq!(h.host.lease_sessions(), vec!["work-session-rotated-1".to_string()]);
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **TWO CONVERSATIONS RUN AT ONCE, AND NEITHER WAITS FOR THE OTHER.** The CEO's own
    /// reason for the shape: *"the CEO could open and run multiple things in parallel"*, and
    /// *"the CEO's conversation with Rich is never blocked by any work that's going on in the
    /// background."* One back end per thread is what makes that true — with one shared back
    /// end the second thread's assignment would sit in a queue behind the first's turn.
    ///
    /// The scripted turn here is 400 ms. Both threads are given an assignment while the
    /// first's turn is still running, and BOTH are observed live at the same moment.
    #[test]
    fn two_conversations_run_at_the_same_time_and_neither_queues_behind_the_other() {
        let h = harness(400);
        let _runner = h.host.start();
        let second_thread = ThreadBinding::new(
            PersonId::default_ceo(),
            EntityId::parse("depot").unwrap(),
            "thread-two",
            1,
        );
        h.host.register(&h.binding, &registration(&h)).unwrap();
        h.host
            .register(
                &second_thread,
                &Registration {
                    thread_id: "thread-two".into(),
                    obligation_id: "obligation-9".into(),
                    // The ledger reference names the turn it came from, and the turn is on
                    // THAT conversation (`assignment::register` checks the two agree).
                    instruction_ledger_ref: "ledger:thread-two:turn-8".into(),
                    ..registration(&h)
                },
            )
            .unwrap();
        // Both live AT ONCE — not one after the other. A shared back end cannot produce this
        // state at all: its second assignment is still in the queue while the first runs.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while (h.host.live_on("thread-one").is_none() || h.host.live_on("thread-two").is_none())
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let one = h.host.live_on("thread-one").expect("the first conversation's work never started");
        let two = h.host.live_on("thread-two").expect("the second conversation waited for the first");
        assert_eq!(one.thread_id, "thread-one");
        assert_eq!(two.thread_id, "thread-two");
        assert_ne!(one.id, two.id, "one assignment was reported as live on both conversations");
        // And a stop on one conversation leaves the other alone — the same rule the
        // per-assignment stop follows, one level up.
        h.host.stop_assignment("depot", "thread-two", &two.id).unwrap();
        assert!(h.host.live_on("thread-one").is_some(), "stopping one conversation stopped the other");
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **Sage's finding 11, walked: the back end that runs for weeks renews itself, and an
    /// assignment that is still open lives across the renewal without losing its binding,
    /// its seat or its grant.**
    ///
    /// The first assignment crosses the watermark and stops at a step of his (`blocked`).
    /// The rotation happens at the boundary AFTER it — never inside its turn. Then he
    /// approves, and the SAME assignment is bound again, on the NEW back end, with the same
    /// seat and a fresh grant. That is the whole of "without losing an assignment's
    /// binding, seat or grant across a rotation".
    #[test]
    fn the_back_end_renews_itself_at_a_boundary_and_an_open_assignment_survives_it() {
        let h = harness(5);
        h.host.start();
        // The MEASURED branch: the adapter reports a window it has nearly filled, which is
        // the primary trigger (`spine.rs:1288-1298`), estimate only as fallback.
        *h.usage.lock().unwrap() = Some(crate::machinery::ContextUsage { used: 800_000, size: 1_000_000 });
        *h.handoff_reply.lock().unwrap() =
            "I had read the three branches and was about to ask him about the second one.".into();
        // It runs, its workers end, and its obligation is still open: ready for him.
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
        // Both back ends' workers are witnessed ending — the incumbent's and the
        // successor's — so the only thing deciding an outcome here is the obligation.
        witnessed(&h.state, "work-session-one");
        witnessed(&h.state, "work-session-rotated-1");
        // A question of his, raised before the turn ends: since §52 a land asks him nothing,
        // so this is what makes the assignment wait on him and stay open across the renewal —
        // which is the whole subject of this test.
        a_question_for_him(&h, "obligation-7");
        let receipt = h.host.register(&h.binding, &registration(&h)).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        // Its state was written before the rotation and is untouched by it.
        assert_eq!(
            assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap().state,
            AssignmentState::Blocked
        );

        // THE ROTATION happened at the boundary, once, and it is observable rather than
        // inferred: the back end's session id moved.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while h.host.rotations("thread-one").0 == 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let (rotations, reason) = h.host.rotations("thread-one");
        assert_eq!(rotations, 1, "the back end never renewed itself");
        assert_eq!(reason.as_deref(), Some("context-watermark-measured"));
        assert_eq!(h.host.lease_sessions(), vec!["work-session-rotated-1".to_string()]);
        assert_eq!(h.spawns.load(Ordering::SeqCst), 2, "a second back end was opened, and only one");

        // THE COMPACTION: the successor was primed from the durable register — his own
        // words for what is open — plus the outgoing back end's five sentences. No
        // identifiers travel.
        let priming = h.reprimes.lock().unwrap().clone();
        assert_eq!(priming.len(), 1, "the successor was not primed");
        let priming = &priming[0];
        assert!(priming.contains("landing the three branches"), "{priming}");
        assert!(priming.contains("blocked"), "{priming}");
        assert!(priming.contains("about to ask him about the second one"), "{priming}");
        assert!(!priming.contains(&receipt.id), "an identifier reached the back end: {priming}");
        assert!(!priming.contains("work-seat:"), "a seat reached the back end: {priming}");
        // **THE CEO'S RULING §52 REACHES THE SUCCESSOR TOO** (operator spec r1 §8, r3 §8: the
        // renewal handover still said "Stop at any step that would change his repository and
        // wait for him to approve it"). A job lands on its own — *"So, yes, always land on its
        // own"* — so the successor must not be told to hold at a land; it takes its orders
        // from the assignment's own brief, which says to land the reviewed result. The old
        // sentence must be GONE, not merely joined, or the successor is left to pick.
        assert!(!priming.contains("wait for him to approve"), "the handover still holds work at his approval: {priming}");
        assert!(!priming.contains("Stop at any step"), "the handover still tells it to stop before a land: {priming}");
        assert!(priming.contains("\"event\":\"session_replaced\""), "{priming}");
        assert!(priming.contains("\"carrying_assignment\":false"), "{priming}");
        assert_eq!(h.handoffs.lock().unwrap().len(), 1, "the outgoing back end was not asked for a handoff");

        // HIS ANSWER, AFTER THE ROTATION. The same assignment goes back on the NEW back
        // end and binds the same seat again — the grant and the seat are per assignment and
        // are rebuilt by the same call its first run used.
        the_question_is_settled(&h, "obligation-7");
        let bound_before = h.bound.lock().unwrap().len();
        let record = assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Settled);
        h.host.apply_decision(
            &crate::ecs::Binding {
                entity_id: "depot".into(),
                thread_id: "thread-one".into(),
                session_id: "work-session-rotated-1".into(),
                turn_id: record.obligation_id.clone(),
                audience: "worker".into(),
                revision: 1,
            },
            true,
        )
        .unwrap();
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)));
        let bound = h.bound.lock().unwrap().clone();
        assert_eq!(bound.len(), bound_before + 1, "the approved assignment never reached the new back end");
        let last = bound.last().unwrap();
        assert_eq!(last.assignment_id, receipt.id);
        assert_eq!(last.seat, format!("work-seat:{}", record.obligation_id), "the seat changed across a rotation");
        assert_eq!(last.entity_id, "depot");
        // And it finished on the successor.
        assert_eq!(
            assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap().state,
            AssignmentState::Settled
        );
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **The three refusals that keep a renewal from becoming a new failure mode.**
    ///
    /// 1. A back end under its watermark is never rotated — the positive control for the
    ///    test above, and the state every back end is in almost always.
    /// 2. A successor that cannot be opened leaves the incumbent working, rather than
    ///    stranding the conversation lease-less (the spine's own ordering rule).
    /// 3. Rotation happens only between assignments: it is asked for once per completed
    ///    assignment and never while one is live.
    #[test]
    fn a_back_end_under_its_watermark_is_left_alone_and_a_failed_renewal_keeps_the_one_that_works() {
        let h = harness(5);
        h.host.start();
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Settled);
        // Nothing reported, and the estimate nowhere near a 200_000-token window.
        h.host.register(&h.binding, &registration(&h)).unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        assert_eq!(h.host.rotations("thread-one").0, 0, "a quiet back end was rotated");
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1);
        assert_eq!(h.host.lease_sessions(), vec!["work-session-one".to_string()]);

        // Now make the watermark reachable on the ESTIMATE alone — no measurement at all —
        // and refuse the successor. The incumbent must still be in the chair.
        h.host.set_context_budget(1, 0.0);
        h.refuse_next.store(true, Ordering::SeqCst);
        h.host
            .register(&h.binding, &Registration { obligation_id: "obligation-8".into(), ..registration(&h) })
            .unwrap();
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)));
        // The rotation was attempted and failed; nothing was swapped and nothing is lost.
        assert_eq!(h.host.rotations("thread-one").0, 0, "a failed renewal was counted as one");
        assert_eq!(h.host.lease_sessions(), vec!["work-session-one".to_string()]);

        // And the next boundary tries again, successfully — so a refusal delays a renewal
        // rather than ending them.
        h.host
            .register(&h.binding, &Registration { obligation_id: "obligation-9".into(), ..registration(&h) })
            .unwrap();
        assert!(h.host.wait_for_completed(3, std::time::Duration::from_secs(10)));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while h.host.rotations("thread-one").0 == 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(h.host.rotations("thread-one").0, 1);
        assert_eq!(h.host.rotations("thread-one").1.as_deref(), Some("context-watermark-estimated"));
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **Reap gap C6: a renewal is invisible, so it waits for a command the back end started,
    /// and the incumbent ends with no lock held.** Retiring a lease now ends its tool commands
    /// (the supervisor's reap), so a due renewal is deferred while the lease's state file
    /// shows one running, or cannot be read, and goes ahead once it clears or the bound
    /// passes. The retired lease is dropped after the back end's lease lock is released.
    /// The company registry beside the harness's state root, with `connected` connected to
    /// the thread's company — the file `connect_repository` writes and `EngineProfile::configure`
    /// reads when it opens a back end.
    fn connected(h: &Harness, connected: &[&str]) {
        let id = crate::entity::EntityId::parse("depot").unwrap();
        let mut registry = crate::entity::EntityRegistry::empty();
        registry.register(crate::entity::Entity::new("depot", "Depot", &[]).unwrap()).unwrap();
        for path in connected {
            registry.connect_repository(&id, PathBuf::from(path)).unwrap();
        }
        registry.save(&crate::entity::entity_registry_path(&h.root)).unwrap();
    }

    /// **A REPOSITORY HE CONNECTS AFTER HIS BACK END OPENED IS ITS REPOSITORY FROM THE NEXT
    /// ASSIGNMENT ON** — the leftover of esc-20260927T093052Z-85f3303f.
    ///
    /// `EngineProfile::configure` fixes the back end's `--add-dir` roots and the auto mode's
    /// "Trusted local task repositories" list from the registry when the provider is spawned,
    /// and `connect_repository` (`main.rs`) updates only the registry and the conversation's
    /// spine. Meanwhile the back end's own `repositories` tool reads the registry live, so it
    /// was told the repository was his to work in and was held outside it. Connecting IS his
    /// consent (`entity.rs`: "Exact roots explicitly connected for app execution"), and the
    /// app applies it without a relaunch everywhere else; so the back end is renewed — at an
    /// assignment boundary, never inside one, primed from the register like any renewal —
    /// before the first assignment that would otherwise run without it.
    ///
    /// Controls: nothing changed, nothing renewed; and a command the back end is still
    /// running defers the renewal (it would end with the incumbent), exactly as a context
    /// renewal is deferred.
    ///
    /// RED at `fb4e1545`: no renewal; the second assignment ran on the back end opened with
    /// one repository.
    #[test]
    fn a_repository_connected_after_the_back_end_opened_renews_it_before_the_next_assignment() {
        use crate::lease_commands::CommandReading;
        let h = harness(5);
        h.host.start();
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Settled);
        let run = |n: u64| {
            h.host
                .register(&h.binding, &Registration { obligation_id: format!("obligation-repo-{n}"), ..registration(&h) })
                .unwrap();
            assert!(h.host.wait_for_completed(n, std::time::Duration::from_secs(10)));
        };
        connected(&h, &["/fictional/acme"]);
        run(1);
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1);

        // Nothing changed: the same back end.
        run(2);
        assert_eq!(h.host.rotations("thread-one").0, 0, "a back end was renewed with nothing changed");

        // He connects a second repository: the next assignment gets a renewed back end.
        connected(&h, &["/fictional/acme", "/fictional/billing"]);
        run(3);
        assert_eq!(h.spawns.load(Ordering::SeqCst), 2, "the assignment ran on the back end opened before he connected it");
        assert_eq!(h.host.rotations("thread-one"), (1, Some(REPOSITORIES_CHANGED.to_string())));
        assert_eq!(h.reprimes.lock().unwrap().len(), 1, "the successor was not primed from the register");

        // A command still running defers it; once it ends, the next boundary renews.
        connected(&h, &["/fictional/acme", "/fictional/billing", "/fictional/web"]);
        *h.commands.lock().unwrap() = Some(CommandReading::Running(1));
        run(4);
        assert_eq!(h.host.rotations("thread-one").0, 1, "a renewal ended a command the back end was running");
        *h.commands.lock().unwrap() = Some(CommandReading::Clear);
        run(5);
        assert_eq!(h.host.rotations("thread-one").0, 2);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **A TEAMMATE SAVED SINCE THE BACK END OPENED IS ITS TEAMMATE FROM THE NEXT ASSIGNMENT
    /// ON** (proto-teammate shelf plan §1, slice 5, the first seam). A lease registers its
    /// team once, when it opens (`EngineProfile::prepare`), so the host keeps the digest of the
    /// team it opened with and renews the back end at the start of an assignment when the team
    /// on disk has moved, beside a connected repository and an account switch.
    ///
    /// Controls: nothing changed, nothing renewed; a command still running defers it to the
    /// next boundary; a factory that cannot say what the team is renews nothing.
    #[test]
    fn the_model_chosen_for_directing_the_team_is_the_model_of_the_next_job() {
        // §119 and its addendum: the choice is saved as Preferences closes and implemented
        // from there — for the team's back end, at the next assignment boundary.
        use crate::config::ModelChoice;
        use crate::lease_commands::CommandReading;
        let h = harness(5);
        h.team.lock().unwrap().model = Some(ModelChoice::Opus);
        h.host.start();
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Settled);
        let run = |n: u64| {
            h.host
                .register(&h.binding, &Registration { obligation_id: format!("obligation-model-{n}"), ..registration(&h) })
                .unwrap();
            assert!(h.host.wait_for_completed(n, std::time::Duration::from_secs(10)));
        };
        run(1);
        run(2);
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1);
        assert_eq!(h.host.rotations("thread-one").0, 0, "a back end was renewed with its model unchanged");

        // Preferences closed with the team row on Fable: the next job runs on a Fable back end.
        h.team.lock().unwrap().model = Some(ModelChoice::Fable);
        run(3);
        assert_eq!(h.spawns.load(Ordering::SeqCst), 2, "the next job ran on the back end opened on Opus");
        assert_eq!(h.host.rotations("thread-one"), (1, Some(crate::spine::MODEL_CHANGED.to_string())));
        run(4);
        assert_eq!(h.spawns.load(Ordering::SeqCst), 2, "a back end already on Fable was renewed again");

        // Back to Opus while a command it started runs: deferred, then renewed once it ends.
        h.team.lock().unwrap().model = Some(ModelChoice::Opus);
        *h.commands.lock().unwrap() = Some(CommandReading::Running(1));
        run(5);
        assert_eq!(h.host.rotations("thread-one").0, 1, "a model renewal ended a command the back end was running");
        *h.commands.lock().unwrap() = Some(CommandReading::Clear);
        run(6);
        assert_eq!(h.host.rotations("thread-one"), (2, Some(crate::spine::MODEL_CHANGED.to_string())));

        // A setting that cannot be read is not a changed setting.
        h.team.lock().unwrap().model = None;
        run(7);
        assert_eq!(h.host.rotations("thread-one").0, 2);
    }

    #[test]
    fn a_team_changed_since_the_back_end_opened_renews_it_before_the_next_assignment() {
        use crate::lease_commands::CommandReading;
        let h = harness(5);
        h.team.lock().unwrap().digest = Some("stock five".into());
        h.host.start();
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Settled);
        let run = |n: u64| {
            h.host
                .register(&h.binding, &Registration { obligation_id: format!("obligation-team-{n}"), ..registration(&h) })
                .unwrap();
            assert!(h.host.wait_for_completed(n, std::time::Duration::from_secs(10)));
        };
        run(1);
        run(2);
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1);
        assert_eq!(h.host.rotations("thread-one").0, 0, "a back end was renewed with its team unchanged");

        // Dean's fitted Mark is saved: the next assignment gets a back end that registers him.
        h.team.lock().unwrap().digest = Some("stock five and mark".into());
        run(3);
        assert_eq!(h.spawns.load(Ordering::SeqCst), 2, "the assignment ran on a back end that never registered Mark");
        assert_eq!(h.host.rotations("thread-one"), (1, Some(TEAM_CHANGED.to_string())));
        assert_eq!(h.reprimes.lock().unwrap().len(), 1, "the successor was not primed from the register");

        // A command still running defers it; once it ends, the next boundary renews.
        h.team.lock().unwrap().digest = Some("stock five and mark, refitted".into());
        *h.commands.lock().unwrap() = Some(CommandReading::Running(1));
        run(4);
        assert_eq!(h.host.rotations("thread-one").0, 1, "a renewal ended a command the back end was running");
        *h.commands.lock().unwrap() = Some(CommandReading::Clear);
        run(5);
        assert_eq!(h.host.rotations("thread-one"), (2, Some(TEAM_CHANGED.to_string())));

        // A team that cannot be read is not a changed team.
        h.team.lock().unwrap().digest = None;
        run(6);
        assert_eq!(h.host.rotations("thread-one").0, 2, "an unreadable team renewed the back end");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **THE WALK'S SHAPE: DEAN FITS AN ENGINEER, THE BACK END SAVES HIM, AND HE DOES THE JOB IN
    /// THE SAME ASSIGNMENT** (plan §1, slice 5, the second seam: *"a teammate Dean activates is
    /// usable on the back end's next turn after Dean's run ends, in the same assignment, with no
    /// restart of the job"*).
    ///
    /// The back end saves the teammate during a turn and ends it, because it cannot name him
    /// until its connection is renewed; nothing is open and the job is not over. The host renews
    /// the back end there, between two turns of the one run, carrying the assignment (the
    /// account switch's road: handoff asked mid-run, successor primed with the job and bound to
    /// the same seat), and gives the successor the next turn, saying the team is registered.
    /// The brief is sent once: the job is not started again.
    ///
    /// Control: a command the back end started that may still be running defers the renewal,
    /// and the run ends as it did before; the next assignment's start renews it.
    #[test]
    fn a_teammate_saved_during_a_run_is_registered_for_its_next_turn_in_the_same_assignment() {
        use crate::cognition::ObligationState;
        use crate::lease_commands::CommandReading;
        const SAVED: &str = "Dean fitted Mark to the Acme repository and I saved him to the team. Mark makes the change next.";
        const CARRIED_ON: &str = "Mark made the change and Frank reviewed it.";
        const HANDOFF: &str = "Mark is saved as team/mark.md; the change is still to be made.";
        let h = harness(5);
        every_worker_observed_ending(&h);
        *h.obligation.lock().unwrap() = Some(ObligationState::Open);
        *h.background.lock().unwrap() = Some(Vec::new());
        *h.handoff_reply.lock().unwrap() = HANDOFF.into();
        h.replies.lock().unwrap().extend([SAVED.to_string(), CARRIED_ON.to_string()]);
        {
            let mut team = h.team.lock().unwrap();
            team.digest = Some("stock five".into());
            // The first turn saves Mark; the second changes nothing.
            team.in_turn.extend([Some("stock five and mark".to_string()), None]);
        }
        h.host.start();
        let job = h.host
            .register(&h.binding, &Registration { title: "add a health check endpoint".into(), ..registration(&h) })
            .unwrap();
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)), "the run never finished");

        assert_eq!(h.host.rotations("thread-one"), (1, Some(TEAM_CHANGED.to_string())),
            "the back end was not renewed between the turn that saved Mark and the next one");
        assert_eq!(h.spawns.load(Ordering::SeqCst), 2);
        let prompts = h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(), 2, "the run did not go on after the turn that saved Mark: {prompts:?}");
        assert_eq!(prompts.iter().filter(|p| p.contains("\"user_request\"")).count(), 1, "the brief was sent again");
        assert_eq!(prompts[1], TEAM_CHANGED_CONTINUATION);
        let handoffs = h.handoffs.lock().unwrap().clone();
        assert_eq!(handoffs.len(), 1);
        assert!(handoffs[0].contains(HANDOFF_ASK_MID_RUN), "{}", handoffs[0]);
        let primed = h.reprimes.lock().unwrap().clone();
        assert_eq!(primed.len(), 1);
        assert!(primed[0].starts_with(MID_RUN_TEAM_HANDOVER_OPENING), "{}", primed[0]);
        for carried in ["add a health check endpoint", "\"user_request\"", SAVED, HANDOFF] {
            assert!(primed[0].contains(carried), "the successor was not told {carried:?}: {}", primed[0]);
        }
        let bound = h.bound.lock().unwrap().clone();
        assert_eq!(bound.len(), 2, "the successor did not take the assignment's seat");
        assert_eq!(bound[0], bound[1]);
        let row = assignment::read(&h.state, "depot", "thread-one", &job.id).unwrap();
        assert_eq!(row.work_session.as_deref(), Some("work-session-rotated-1"), "{row:?}");

        // ---- control: a command it started may still be running --------------------------
        *h.commands.lock().unwrap() = Some(CommandReading::Running(1));
        h.team.lock().unwrap().in_turn.push_back(Some("stock five, mark and norm".to_string()));
        h.host
            .register(&h.binding, &Registration { obligation_id: "obligation-team-2".into(), ..registration(&h) })
            .unwrap();
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)));
        assert_eq!(h.host.rotations("thread-one").0, 1, "a renewal ended a command the back end was running");
        assert_eq!(h.work_prompts.lock().unwrap().len(), 3, "a deferred renewal still gave the run another turn");
        *h.commands.lock().unwrap() = Some(CommandReading::Clear);
        h.host
            .register(&h.binding, &Registration { obligation_id: "obligation-team-3".into(), ..registration(&h) })
            .unwrap();
        assert!(h.host.wait_for_completed(3, std::time::Duration::from_secs(10)));
        assert_eq!(h.host.rotations("thread-one"), (2, Some(TEAM_CHANGED.to_string())),
            "the deferred renewal did not happen at the next assignment's start");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    #[test]
    fn a_renewal_waits_for_a_running_command_and_retires_the_incumbent_with_no_lock_held() {
        use crate::lease_commands::CommandReading;
        let h = harness(5);
        *h.drop_probe.lock().unwrap() = Some(Arc::downgrade(&h.host));
        h.host.start();
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Settled);
        h.host.set_context_budget(1, 0.0);                 // every boundary is due a renewal
        let run = |n: u64| {
            h.host
                .register(&h.binding, &Registration { obligation_id: format!("obligation-c6-{n}"), ..registration(&h) })
                .unwrap();
            assert!(h.host.wait_for_completed(n, std::time::Duration::from_secs(10)));
        };

        // A background command is running: two boundaries pass with no renewal. The second
        // assignment's completion proves the first boundary is behind it (one runner).
        *h.commands.lock().unwrap() = Some(CommandReading::Running(1));
        run(1);
        run(2);
        // Unreadable is never read as "nothing running".
        *h.commands.lock().unwrap() = Some(CommandReading::Unreadable);
        run(3);
        assert_eq!(h.host.rotations("thread-one").0, 0, "a renewal ended a command it had started");
        assert_eq!(h.host.lease_sessions(), vec!["work-session-one".to_string()]);

        // The command ends: the next boundary renews.
        *h.commands.lock().unwrap() = Some(CommandReading::Clear);
        run(4);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while h.host.rotations("thread-one").0 == 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(h.host.rotations("thread-one").0, 1, "the renewal never happened once the command ended");
        let drops = h.drops.lock().unwrap().clone();
        assert_eq!(drops, vec![("work-session-one".to_string(), false)],
                   "the incumbent was not dropped exactly once, with no lock held: {drops:?}");

        // The bound: a command that never ends does not hold the renewal forever.
        *h.commands.lock().unwrap() = Some(CommandReading::Running(1));
        h.host.set_rotation_command_wait(std::time::Duration::ZERO);
        run(5);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while h.host.rotations("thread-one").0 < 2 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!(h.host.rotations("thread-one").0, 2, "a command held the renewal past its bound");

        *h.drop_probe.lock().unwrap() = None;
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **§2.4a's trigger, and the distinction §7.8 spends a paragraph on.**
    ///
    /// The host reports "nothing is registered any more" when an assignment ends and the
    /// register is empty — and does NOT report it when the assignment ended by stopping at
    /// a step of his, because that assignment is still registered and he is coming back to
    /// it. The shell turns the first into a quit and never sees the second.
    #[test]
    fn nothing_left_to_do_fires_on_an_empty_register_and_never_on_one_waiting_for_him() {
        struct Counter {
            idle: AtomicUsize,
        }
        impl WorkNotifier for Counter {
            fn raised(&self, _thread: &str, _notice: &PendingNotice) {}
            fn nothing_left_to_do(&self) {
                self.idle.fetch_add(1, Ordering::SeqCst);
            }
        }
        let h = harness(5);
        let counter = Arc::new(Counter { idle: AtomicUsize::new(0) });
        // A second host over the same state, with a notifier that counts.
        let host = WorkHost::new(
            &h.state,
            // The same gate as the harness's, which is open and stays open here.
            Box::new(factory_over(&h, 5)),
            counter.clone(),
            Arc::clone(&h.desk),
        );
        host.start();

        // 1. It waits on a question of his: still registered, so the app must stay up. Since
        // §52 a land asks him nothing, so the thing that stops it has to be a real question of
        // his — raised before the turn ends.
        witnessed(&h.state, "work-session-one");
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Open);
        a_question_for_him(&h, "obligation-7");
        host.register(&h.binding, &registration(&h)).unwrap();
        assert!(host.wait_for_completed(1, std::time::Duration::from_secs(10)));
        assert_eq!(counter.idle.load(Ordering::SeqCst), 0, "the app would have quit on work waiting for him");
        the_question_is_settled(&h, "obligation-7");

        // 2. It settles, and nothing is left open anywhere.
        *h.obligation.lock().unwrap() = Some(crate::cognition::ObligationState::Settled);
        let record = assignment::read_all(&h.state, "depot", "thread-one").unwrap().pop().unwrap();
        host.stop_assignment("depot", "thread-one", &record.id).unwrap();
        host.register(&h.binding, &Registration { obligation_id: "obligation-8".into(), ..registration(&h) })
            .unwrap();
        assert!(host.wait_for_completed(2, std::time::Duration::from_secs(10)));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while counter.idle.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(counter.idle.load(Ordering::SeqCst) >= 1, "the last assignment settled and nothing said so");
        assert!(host.open_assignments().unwrap().is_empty());
        host.shutdown();
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn a_failure_sentence_carries_no_stack_trace_and_is_never_empty() {
        // Flattened, bounded, never empty — and opened like a sentence, which it did not used to
        // be: the seam label `honest` now strips (row 8) was supplying the first word.
        assert_eq!(honest("  a\nb  "), "A b.");
        assert_eq!(honest("   "), "No reason was recorded.");
        assert_eq!(honest("ended."), "Ended.");
        assert!(honest(&"x".repeat(500)).chars().count() <= 201);
    }

    /// **Only the lines that tell him what to do reach the ordinary view** (hunt 57, finding
    /// 11; the review of 25b540804): sign in, a helper he may stop, an answer he owes. The back
    /// end's own record stays behind Technical view, including a hold whose reason it relayed.
    #[test]
    fn only_the_lines_that_tell_him_what_to_do_are_for_him() {
        assert!(tells_him_what_to_do(&HoldCause::SignIn.detail()));
        assert!(tells_him_what_to_do(HELPER_TAKING_LONGER_DETAIL));
        assert!(tells_him_what_to_do(WAITING_FOR_YOUR_ANSWER));
        let relayed = HoldCause::BackEnd("spawn failed: no such file".into()).detail();
        for record in [
            relayed.as_str(),
            ANSWER_RETRY_DETAIL,
            STARTED_DETAIL,
            COMMAND_STILL_RUNNING_DETAIL,
            LIMIT_SWITCH_DETAIL,
            WATCHED_STOP_REFUSED,
            "Opening the work connection.",
            "",
        ] {
            assert!(!tells_him_what_to_do(record), "a back-end record is shown as his to act on: {record:?}");
        }
    }    #[test]
    fn peer_handoff_with_operational_record_executes_once() {
        let h=harness(0);
        h.team.lock().unwrap().digest=Some("stock".into());
        every_worker_observed_ending(&h);
        for n in 1..=4 {
            let folder=h.state.join("evidence").join(format!("work-session-rotated-{n}"));
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("callbacks.jsonl"),"").unwrap();
            std::fs::write(folder.join(".lock"),"").unwrap();
        }
        let scope=h.host.message_scope(&h.binding);
        let path=h.state.join("rich-status.json");
        crate::session_tools::bind(&path,&scope,"native-rich","rich",None).unwrap();
        let receipt=assignment::register(&h.state,&registration(&h)).unwrap();
        assert_eq!(h.host.adopt_registered(&h.binding),0,"an operational record must never dispatch a second execution");
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"Inspect the current evidence"})).unwrap();
        h.host.wake_messages(&h.binding);
        // load-bound: hang guard only; the verdict is one consumed prompt and an empty inbox, not elapsed time.
        assert!(h.host.wait_for_completed(1,std::time::Duration::from_secs(10)),"hang guard: wait for peer consumption");
        assert_eq!(h.host.adopt_registered(&h.binding),0);
        assert_eq!(assignment::read(&h.state,"depot","thread-one",&receipt.id).unwrap().state,AssignmentState::Registered);
        assert_eq!(h.work_prompts.lock().unwrap().len(),1);
        assert!(h.bound.lock().unwrap().is_empty(),"Stu was rebound to a formal assignment");
        assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_none());
        let backend=h.host.backend_for("thread-one").unwrap();
        let message=crate::session_tools::Message { handoff:true, from:"rich".into(),
            text:"Use the saved teammate for this authorized work".into(), instruction:None };
        let run=|n| h.host.run_message(&backend,&h.binding,&message,&format!("peer:{n}")).unwrap();
        run(2);
        assert_eq!(h.spawns.load(Ordering::SeqCst),1,"unchanged team renewed unnecessarily");
        h.team.lock().unwrap().digest=Some("stock and mark".into());
        let evidence=h.state.join("evidence/work-session-one/callbacks.jsonl");
        let start=serde_json::json!({"schema":1,"callback":{"session_id":"work-session-one","hook_event_name":"SubagentStart","agent_id":"live-worker"}}).to_string()+"\n";
        std::fs::write(&evidence,&start).unwrap();
        run(3);
        assert_eq!(h.spawns.load(Ordering::SeqCst),1,"renewal killed a live native worker");
        let end=serde_json::json!({"schema":1,"callback":{"session_id":"work-session-one","hook_event_name":"SubagentStop","agent_id":"live-worker"}}).to_string()+"\n";
        std::fs::write(&evidence,start+&end).unwrap();
        run(4);
        assert_eq!(h.spawns.load(Ordering::SeqCst),2,"next peer handoff kept an obsolete roster");
        *h.commands.lock().unwrap()=Some(crate::lease_commands::CommandReading::Running(1));
        h.team.lock().unwrap().digest=Some("stock, mark and norm".into());
        run(5);
        assert_eq!(h.spawns.load(Ordering::SeqCst),2,"renewal ended an unrelated running command");
        *h.commands.lock().unwrap()=Some(crate::lease_commands::CommandReading::Clear);
        run(6);
        assert_eq!(h.spawns.load(Ordering::SeqCst),3,"deferred roster was never registered");
        h.team.lock().unwrap().in_turn.push_back(Some("stock, mark, norm and echo".into()));
        run(7);
        assert_eq!(h.spawns.load(Ordering::SeqCst),4,"a teammate saved during the handoff was not registered");
        let prompts=h.work_prompts.lock().unwrap();
        assert_eq!(prompts.len(),8,"same handoff did not continue after registering the new teammate");
        assert!(prompts.last().unwrap().starts_with(TEAM_CHANGED_CONTINUATION));
        assert!(prompts.last().unwrap().contains(&message.text),"the successor lost the original handoff");
        drop(prompts);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    #[test]
    fn independent_stu_stop_is_durable_while_a_dispatch_writer_is_busy() {
        struct Noop;
        impl TurnCancel for Noop { fn cancel(&self) -> bool { false } }
        let h = harness(0);
        let receipt = assignment::register(&h.state, &registration(&h)).unwrap();
        let backend = Backend::new();
        backend.inner.lock().unwrap().peer_stopper = Some(("independent-stu".into(), Arc::new(Noop)));
        h.host.backends.lock().unwrap().insert("thread-one".into(), backend);
        let writer = crate::work_status::lock_receipts(&h.state, "depot", "thread-one").unwrap();
        let host = h.host.clone();
        let id = receipt.id.clone();
        let stop = std::thread::spawn(move || host.stop_assignment("depot", "thread-one", &id));
        // The existing bounded wait is a hang guard. The verdict requires the
        // actual Stop fact while the dispatch lock is still held, never elapsed time.
        let durable = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| until(
            "the Stop fact waited for the dispatch writer", || assignment::read(
                &h.state, "depot", "thread-one", &receipt.id).unwrap().user_stop_requested_at_ms.is_some()
        ))).is_ok();
        drop(writer);
        stop.join().unwrap().unwrap();
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
        assert!(durable, "the user's Stop must be durable before waiting for a busy dispatch writer");
    }

    #[test]
    fn independent_stu_stop_reaches_only_the_selected_assignments_worker() {
        struct Stops(Mutex<Vec<String>>);
        impl TurnCancel for Stops {
            fn cancel(&self)->bool { panic!("per-assignment Stop must not cancel Stu") }
            fn stop_worker(&self, tool:&str)->CommandStop {
                self.0.lock().unwrap().push(tool.into()); CommandStop::Ended
            }
        }
        let h=harness(0);
        let first=assignment::register(&h.state,&registration(&h)).unwrap();
        let second=assignment::register(&h.state,&Registration {obligation_id:"another-obligation".into(),..registration(&h)}).unwrap();
        let backend=Backend::new();let stopper=Arc::new(Stops(Mutex::new(vec![])));
        {let mut inner=backend.inner.lock().unwrap();inner.lease_session=Some("independent-stu".into());inner.peer_stopper=Some(("independent-stu".into(),stopper.clone()));}
        h.host.backends.lock().unwrap().insert("thread-one".into(),backend);
        let root=h.state.join("work-receipts").join({use sha2::Digest;format!("{:x}",sha2::Sha256::digest(b"[\"depot\",\"thread-one\"]"))});
        std::fs::create_dir_all(&root).unwrap();
        for (id,obligation,tool) in [("first","obligation-7","call-first"),("second","another-obligation","call-second")] {
            std::fs::write(root.join(format!("{id}.json")),serde_json::json!({"schema":1,"id":id,
                "binding":{"entity_id":"depot","thread_id":"thread-one","session_id":"independent-stu"},
                "request":{"obligation_id":obligation,"role":"worker"},"status":"running","tool_use_id":tool}).to_string()).unwrap();
        }
        let evidence=h.state.join("evidence/independent-stu");
        std::fs::create_dir_all(&evidence).unwrap();
        let selected=assignment::read(&h.state,"depot","thread-one",&first.id).unwrap();
        let callbacks=[serde_json::json!({"schema":1,"instruction":{"ledger_ref":selected.instruction_ledger_ref,"sha256":selected.instruction_sha256},
            "callback":{"session_id":"independent-stu","hook_event_name":"PreToolUse","tool_name":"Bash","tool_use_id":"direct-first"}}),
            serde_json::json!({"schema":1,"instruction":{"ledger_ref":"ledger:thread-one:another-request","sha256":"other"},
            "callback":{"session_id":"independent-stu","hook_event_name":"PreToolUse","tool_name":"Bash","tool_use_id":"direct-second"}})];
        // A standing session can have a long history and large unrelated tool output.
        let history=serde_json::json!({"schema":1,"callback":{"session_id":"independent-stu", "hook_event_name":"PostToolUse",
            "tool_name":"Read","tool_response":"x".repeat(1024 * 1024 + 1)}}).to_string()+"\n";
        std::fs::write(evidence.join("callbacks.jsonl"),history.repeat(17)+&callbacks.iter().map(|row|row.to_string()+"\n").collect::<String>()).unwrap();
        h.host.stop_assignment("depot","thread-one",&first.id).unwrap();
        assert_eq!(*stopper.0.lock().unwrap(),vec!["call-first","direct-first"]);
        let stopped=assignment::read(&h.state,"depot","thread-one",&first.id).unwrap();
        assert!(stopped.user_stop_requested_at_ms.is_some());
        let control=crate::session_tools::pending(&h.host.message_scope(&h.binding),"stu").unwrap().unwrap().0;
        assert_eq!(control.from,"user-control");
        let facts:serde_json::Value=serde_json::from_str(&control.text).unwrap();
        assert_eq!(facts["event"],"user_stop");assert_eq!(facts["assignment_id"],first.id);
        assert_eq!(facts["requested_at_ms"],stopped.user_stop_requested_at_ms.unwrap());
        assert!(crate::session_tools::pending(&h.host.message_scope(&h.binding),"rich").unwrap().is_none());
        assert_eq!(stopped.state,AssignmentState::Interrupted);
        assert_eq!(assignment::read(&h.state,"depot","thread-one",&second.id).unwrap().state,AssignmentState::Registered);
        h.host.shutdown();std::fs::remove_dir_all(&h.root).unwrap();
    }
   #[test]
   fn user_stop_provenance_survives_state_changes_and_reload() {
       let h = harness(0);
       let receipt = assignment::register(&h.state, &registration(&h)).unwrap();
       let before = serde_json::to_value(assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap()).unwrap();
       assert!(before.get("user_stop_requested_at_ms").is_none_or(|value|value.is_null()));
       h.host.stop_assignment("depot", "thread-one", &receipt.id).unwrap();
       let read = || assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap();
       let stopped = serde_json::to_value(read()).unwrap();
       let at = stopped["user_stop_requested_at_ms"].as_u64().expect("the user's Stop is a durable source fact");
       assert!(at > 0);
       assert_eq!(read().state, AssignmentState::Interrupted);
       assignment::advance(&h.state, "depot", "thread-one", &receipt.id, AssignmentState::Interrupted, "Provider cancellation outcome recorded separately").unwrap();
       assert_eq!(serde_json::to_value(read()).unwrap()["user_stop_requested_at_ms"], at);
       h.host.shutdown();
       std::fs::remove_dir_all(&h.root).unwrap();
   }

}

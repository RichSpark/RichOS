//! The COGNITION seam — the swappable compute lease behind the durable spine.
//!
//! `Cognition` is the one narrow interface the Spine talks to. The real
//! implementation (`native::NativeCognition`) drives a `claude` child over its
//! stream-json stdio;
//! `MockCognition` (below) lets the entire spine — ledger crash-safety,
//! queue-not-interrupt, thread switching, re-prime injection — be unit-tested with
//! ZERO live Claude / network. Structuring the session as a trait object IS the
//! swappable-lease foundation: a later rotation just drops in a fresh Cognition.

use crate::machinery::MachineryRecord;
use crate::steering::TurnCancel;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, thiserror::Error)]
pub enum CognitionError {
    #[error("Preparation stopped with {0}")]
    PrimingStopped(String),
    #[error("cognition io: {0}")]
    Io(String),
    #[error("cognition protocol: {0}")]
    Protocol(String),
}

/// The factory that spawns a FRESH compute lease at rotation time (continuity design
/// §3.3 step 4, which wrote "Spawn fresh claude-agent-acp child" and now means "spawn a
/// fresh `claude` child" — `ceo-decisions.md` §16). Injected into the `Spine` so
/// richos-core stays IO-agnostic — the real implementation (Tauri shell) wraps
/// `NativeCognition::start`; tests inject a `MockLeaseFactory`. This is what makes the
/// swappable-lease design real: the Spine can rotate WITHOUT knowing how a lease is
/// actually constructed.
pub trait LeaseFactory: Send + Sync {
    /// Spawn + initialize a fresh, un-primed lease. Fallible — e.g. the `claude` binary
    /// is missing, or Claude isn't signed in. A failure here means rotation/recovery
    /// cannot proceed and must surface honestly rather than silently keep the dead lease.
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError>;

    /// The host fixes the company/thread before the provider or its hooks start.
    fn spawn_scoped(&self, _binding: &crate::entity::ThreadBinding, control: &crate::steering::ConversationControl) -> Result<Box<dyn Cognition>, CognitionError> {
        self.spawn_cancellable(control)
    }

    /// Stop may be recorded before a child has completed its initialize handshake.
    fn spawn_cancellable(&self, _control: &crate::steering::ConversationControl) -> Result<Box<dyn Cognition>, CognitionError> {
        self.spawn()
    }

    /// **Fill-first** (`claude_accounts.rs`): the quota service that knows the accounts, the
    /// account in use and the freshest readings. `None` — the default — means this factory
    /// spawns every lease under the one account it has, and nothing is ever switched.
    fn quota(&self) -> Option<Arc<crate::quota::Service>> {
        None
    }

    /// **The digest of the team the next lease would register**
    /// (`engine_profile::team_digest`; proto-teammate shelf plan §1, slice 5). A lease reads its
    /// team once, when it starts, so the work host records this when it opens one and renews the
    /// back end at the next boundary when it has moved: a teammate saved mid-assignment is
    /// usable on the back end's next turn. `None` — the default, and any reading that fails —
    /// means this factory cannot say, and nothing is renewed on the strength of it.
    fn team_digest(&self) -> Option<String> {
        None
    }

    /// **The models the next leases would start on** (the CEO's §119): Rich's for a
    /// conversation lease and Stu's for a work lease, as the stored setting says now. Read at
    /// each boundary and compared with the lease in the chair's own
    /// [`Cognition::lease_model`]; a difference renews the lease there, so a choice saved
    /// when Preferences closes is the model of the next reply and the next job. `None` — the
    /// default — means this factory cannot say, and nothing is renewed on the strength of it.
    fn lease_models(&self) -> Option<crate::config::LeaseModels> {
        None
    }

    /// Spawn the SECOND lease — the background-work spec's work lease (§2.1).
    ///
    /// **It is a separate method rather than an argument, because the two leases differ in
    /// what the CHILD is given, not in how the host holds it.** The work lease's per-lease
    /// MCP config omits `richos_continuity` (spec §5.8a-ii seam 1: *"the tool is not there
    /// to call"*) and its preparation never calls `brief` (seam 2). A boolean on
    /// `spawn_scoped` would have let a caller ask for a conversation lease and get a work
    /// one, which is the shape of a mistake that is invisible until his continuity
    /// checkpoints have a worker's records in them.
    ///
    /// **No `TurnControl`.** Spec §4.2: *"The work lease is never attached to the
    /// conversation's `TurnControl`."* The signature is the enforcement — there is no
    /// argument here to pass one through, so the one-line convenience of reusing the
    /// attach path cannot be taken by accident.
    ///
    /// The default is an honest refusal: a build with no second lease says so rather than
    /// silently handing back a conversation lease that would take the spine's turn gate
    /// with it.
    fn spawn_work(&self, _binding: &crate::entity::ThreadBinding) -> Result<Box<dyn Cognition>, CognitionError> {
        Err(CognitionError::Protocol(
            "This RichOS cannot open a second connection for background work.".into(),
        ))
    }
}

/// Everything a work lease needs to bind ONE background assignment.
///
/// Spec §5.8c — **one seat per assignment**, created with it and revoked with it, named by
/// the assignment's own identity and never chosen by the model. Spec §3.6 — the
/// instruction reference is the one from the turn in which he gave the assignment, frozen
/// for its life, so the engine's completion gate checks a durable instruction rather than
/// a live one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkAssignment {
    pub entity_id: String,
    pub thread_id: String,
    pub assignment_id: String,
    /// The ECS obligation. **It is what the seat's `turn_id` carries**, because the
    /// engine's seat reconciler maps a seat to its assignment through exactly that field
    /// (`richos/engine/mega-lander/app.py:704`).
    pub obligation_id: String,
    pub seat: String,
    pub instruction_ledger_ref: String,
    pub instruction_sha256: String,
    /// The words that reference names, already checked against `instruction_sha256` by the
    /// work host (`work_host.rs`'s `instruction_for`). They reach the scope beside the hash so
    /// a reviewer is briefed with the user's own turn (second review, slice 4). `None` when the
    /// host could not read them back (a report turn), never a guess.
    pub instruction_text: Option<String>,
}

/// What the OBLIGATION says, which is the only thing allowed to say an assignment is
/// finished.
///
/// **Not the workers.** The engine states the rule in its own voice: *"An assignment whose
/// workers have all stopped is NOT settled: that is exactly the state where it has run,
/// stopped at integrate and is waiting for him"*
/// (`richos/engine/mega-lander/app.py:664-669`). The app built the opposite of this before
/// that engine landed — all workers observed ending, therefore settled — which would have
/// told the CEO a thing was finished at the precise moment it was waiting for him.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObligationState {
    /// `candidate`, `accepted`, `active`, `pending` or `blocked` (`app.py:660`).
    Open,
    /// The obligation itself is closed.
    Settled,
    /// No such obligation. Never read as finished.
    Absent,
}

/// ONE item leaving a turn's drain loop, in the order it actually happened.
///
/// The seam widening the techy-mode design forces (§1.6): machinery cannot travel through
/// a `&str` callback, so `prompt`'s sink takes a tagged item instead.
///
/// **One deviation from §1.6's literal signature, and the reason.** §1.6 writes
/// `Text(&'a str)`. `seq` is carried on the text arm as well, because §1.4 **G1** — *"the
/// single most important guarantee in the design"* — requires ONE counter per turn shared
/// by text and machinery: *"You cannot reconstruct 'he said X, then ran Y, then said Z'
/// from two independent counters."* With a bare `Text(&str)` the spine would have to
/// count text items itself, which is two counters again. So the counter is assigned once,
/// at the drain point (§1.4's feasibility argument: `native.rs`'s mpsc drain is
/// single-threaded, so assigning there is sound), and both arms carry it.
pub enum TurnItem<'a> {
    /// Assistant-message text. The clean-output path — unchanged in every other respect.
    Text { seq: u64, text: &'a str },
    /// Everything else the session emitted, normalized. `thread_id` / `turn_id` /
    /// `internal` are still unset here; only the caller that knows the turn can stamp them.
    Machinery(MachineryRecord),
    /// **His answer is complete, and the turn is still running** (product hunt 2026-10-15,
    /// finding 21). Nothing after this in the turn will reach him as words: the turn's
    /// checkpoint was accepted after his answer, and the lease withholds what follows it. The
    /// turn still ends at its own end; this only lets the screen stop saying "Rich is working".
    /// A lease that cannot tell never sends it, and the turn shows as working to its end.
    AnswerComplete,
}

/// **One command the provider is running, or ran, in the background** — the thing a back end
/// starts when he says "start X and tell me when it finishes".
///
/// Read off three frames of the provider's own stream, measured on `claude` 2.1.283 on
/// 2026-09-27 (richos-hq `docs/verification/2026-09-27-background-command-finish/`):
///
/// - `system/task_started` `{task_id, task_type: "local_bash", is_backgrounded: true,
///   description}` when the command goes to the background. A FOREGROUND command gets the
///   same frame with `is_backgrounded: false`, and is not one of these (`cap-fold.jsonl`).
/// - `system/background_tasks_changed` `{tasks: [{task_id, task_type, description}]}`, the
///   provider's whole list, sent whenever it changes.
/// - `system/task_notification` `{task_id, status, summary}` when it ends, with
///   `status: "completed"` and a summary such as `... completed (exit code 0)`.
///
/// Only `local_bash` is tracked. A helper (`local_agent`) is waited for on its own positive
/// signal, `SubagentStop` in the app's journal (`work_host.rs` step 3b), and no other task
/// type has been measured ending on this wire.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackgroundCommand {
    pub task_id: String,
    /// The provider's own description of the command (the model's or the command line).
    pub description: String,
    /// `None` while no `task_notification` has named it.
    pub ended: Option<CommandEnded>,
}

/// How a background command ended, from its `task_notification`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandEnded {
    pub status: String,
    pub summary: String,
    /// **The notification arrived while a turn this client sent was running.** The provider
    /// folds it into that turn, so the model had it before it answered: measured in
    /// `cap-fold.jsonl`, where the notification at 6.925 s is inside the turn that ended at
    /// 18.802 s and that turn's answer describes it. When `false` the provider reported it
    /// in a turn of its own, whose words reach no caller (`native.rs`'s between-turn lane).
    pub during_a_turn_of_ours: bool,
}

/// A consumer of the existing between-turn lane before presentation normalization.
pub type BetweenTurnSink = Arc<dyn Fn(Vec<serde_json::Value>) + Send + Sync>;

/// A disposable compute lease. One implementation per backing session.
/// `Send` so the durable `Spine` can be held behind a `Mutex` as Tauri managed state.
pub trait Cognition: Send {
    /// A stable identifier for the backing session (for the ledger's rotation record).
    fn session_id(&self) -> &str;

    /// **Fill-first: the Claude account this lease runs under** (`claude_accounts.rs`). `None`
    /// for a lease that cannot say, which is never rotated for an account switch.
    fn account(&self) -> Option<&str> { None }
    /// **The model this lease was started on** (§119: `--model` in its argument vector).
    /// `None` for a lease that cannot say, which is never renewed for a model change.
    fn lease_model(&self) -> Option<crate::config::ModelChoice> { None }
    /// The latest usage reading this lease's own child streamed (`rate_limit_event`), and when.
    /// Read before every turn so the switch decision uses the freshest figure there is.
    fn streamed_usage(&self) -> Option<crate::quota::StreamedReading> { None }

    /// Scoped operational sessions cannot be reused for another thread.
    fn requires_thread_isolation(&self) -> bool { false }
    fn worker_status(&self) -> Option<crate::worker_status::WorkerStatusView> { None }
    /// Tool commands this lease has running outside its provider's group, which retiring the
    /// lease would end (the product reap, `lease_commands.rs`). `None`: nothing supervises
    /// this lease's commands, so there is nothing to read.
    fn running_commands(&self) -> Option<crate::lease_commands::CommandReading> { None }
    /// **The commands this lease's provider started in the background, and how each ended**,
    /// read off the provider's own task frames ([`BackgroundCommand`]). `None`: this lease
    /// cannot say, which is never the same as "none are running".
    fn background_commands(&self) -> Option<Vec<BackgroundCommand>> { None }

    /// Bind app-owned onboarding tools before a priming turn. Adapters without these tools
    /// keep the default no-op; the native chat lease atomically updates its private scope.
    fn set_onboarding_scope(
        &mut self, _entity: &crate::entity::EntityId, _central_root: &std::path::Path,
        _record_path: &std::path::Path,
    ) -> Result<(), CognitionError> { Ok(()) }

    /// Stage current host-owned company material as data for the next visible request.
    /// Returning false keeps the separate priming fallback for adapters that cannot carry
    /// context separately from the original user instruction.
    fn set_company_context(&mut self, _context: Option<&str>) -> Result<bool, CognitionError> {
        Ok(false)
    }

    /// Host-issued visible-turn scope. Hidden preparation records are retained as
    /// internal machinery and never receive a mutation grant.
    fn set_input_channel(&mut self, _channel: Option<&str>) -> Result<(), CognitionError> { Ok(()) }
    fn prepare_work_turn(&mut self, _binding: &crate::entity::ThreadBinding, _turn: &str, _original_turn: &str,
        _source: crate::ledger::Source, _text: &str, _on_item: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> { Ok(()) }

    /// The host's current visible conversation, separate from instructions and hidden context.
    fn set_conversation_context(&mut self, _context: &str) {}

    /// Previous delivery facts, separate from the unchanged user instruction.
    fn set_previous_delivery_attempt(&mut self, _context: &str) -> Result<(), CognitionError> {
        Err(CognitionError::Protocol("this adapter cannot reconcile an uncertain delivery".into()))
    }

    /// Prepare ordinary peer input without manufacturing an assignment.
    fn prepare_agent_message(&mut self,_binding:&crate::entity::ThreadBinding,_turn:&str,
        _message:&crate::session_tools::Message)->Result<(),CognitionError> {Ok(())}

    /// Reload the existing native roster without replacing this session or its workers.
    fn refresh_team(&mut self)->Result<bool,CognitionError> {Ok(false)}

    /// Give the session the context brief his next turn would otherwise supply first, while
    /// nobody is waiting: called right after the priming turn, under that turn's own ledger
    /// id. His turn's [`Self::prepare_work_turn`] then finds the same brief already held and
    /// sends his words straight away. Adapters without a context brief keep the no-op.
    fn supply_context_ahead(&mut self, _binding: &crate::entity::ThreadBinding, _turn: &str,
        _on_item: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> { Ok(()) }

    /// Prepare session history and continuity together before the visible request.
    /// Adapters can combine both into one hidden request. The fallback preserves the
    /// older two-step adapter contract and leaves a failed prefetch to visible preparation.
    fn prime_with_continuity(&mut self, binding: &crate::entity::ThreadBinding, turn: &str,
        priming: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
        self.reprime(priming, on_item)?;
        if let Err(error) = self.supply_context_ahead(binding, turn, on_item) {
            eprintln!("[richos] continuity prefetch failed ({error}); visible preparation will retry");
        }
        Ok(())
    }

    /// Inject the re-prime payload as an INTERNAL, non-rendered priming turn.
    /// Called once when the lease is (re)spawned, before any CEO-visible turn.
    ///
    /// It runs a REAL turn, so it produces real machinery — which techy-mode §1.5 says
    /// must be recorded and never rendered (`internal: true`, `turn_id: None`). That is
    /// why this now takes a sink at all: before, the real `reprime` ran into a
    /// `|_| {}` and the machinery had nowhere to go. Its assistant TEXT is still
    /// discarded by every caller — the priming turn is never rendered.
    fn reprime(
        &mut self,
        priming_text: &str,
        on_item: &mut dyn FnMut(TurnItem),
    ) -> Result<(), CognitionError>;

    /// Run one CEO turn. Streams items to `on_item` in the order they actually happened,
    /// each stamped with the ONE shared per-turn `seq` (§1.4 G1).
    ///
    /// CLEAN OUTPUT IS UNCHANGED: only agent-message text is ever `TurnItem::Text`. Tool
    /// calls, shell and hook output arrive as `TurnItem::Machinery` — a different arm, a
    /// different event family, and no path into `StreamEvent::Chunk`. Returns the turn's
    /// stop reason.
    fn prompt(
        &mut self,
        text: &str,
        on_item: &mut dyn FnMut(TurnItem),
    ) -> Result<String, CognitionError>;

    /// Run hidden context maintenance without granting tools or onboarding writes.
    /// Adapters with side-effecting tools must override this method.
    fn prompt_context_only(
        &mut self, text: &str, on_item: &mut dyn FnMut(TurnItem),
    ) -> Result<String, CognitionError> { self.prompt(text, on_item) }

    /// A handle that interrupts the CURRENTLY RUNNING `prompt` from another thread
    /// (UX §9.3 step 2).
    ///
    /// It has to be obtainable WITHOUT `&mut self`, because while a turn is running the
    /// `&mut` is held by `prompt` itself and the whole spine is behind one `Mutex`. That
    /// constraint is the entire shape of this method: `&self`, `Arc`, `Send + Sync`.
    ///
    /// The default is `None` — "this lease cannot be interrupted" — so a lease with no
    /// cancel story reports one honestly instead of silently accepting a stop that will
    /// never arrive. `StopOutcome::reached_lease` carries that fact to the UI.
    fn cancel_handle(&self) -> Option<Arc<dyn TurnCancel>> {
        None
    }

    /// Take everything the backing session emitted while NO turn was in flight (techy-mode
    /// §1.5, gap #1): session-start traffic, and whatever arrives after a prompt response
    /// has already been returned.
    ///
    /// The records come back with `turn_id: None` and an unstamped thread — only the spine
    /// knows the thread, and NOTHING knows a turn, because there isn't one (§1.4 G4).
    ///
    /// **The default is an empty vec, and it is a statement rather than a stub:** a lease
    /// with no independent machinery channel emitted nothing between turns as far as it can
    /// witness, and saying "none" is the true answer for it. `NativeCognition` overrides
    /// this with the real buffer; the test doubles below override it when a test drives the
    /// lane.
    fn drain_between_turn(&mut self) -> Vec<MachineryRecord> {
        Vec::new()
    }

    /// Deliver the existing between-turn lane without waiting for another prompt.
    /// Native Stu uses this to report turns started by its own workers as well.
    fn set_between_turn_sink(&mut self, _sink: BetweenTurnSink) {}


    /// Bind this lease's ECS seat for ONE background assignment and write its tool scope.
    ///
    /// **This is the work lease's sibling of `prepare_work_turn`, and the whole reason it
    /// is a different method is what it does NOT do.** `prepare_work_turn` calls
    /// `bridge.brief(&scope)` on its way to writing the scope (`native.rs:1972`). Spec
    /// §5.8a-ii names that as the seam that fires first — the HOST calling `brief` on the
    /// work lease's behalf, which no allow-list touches — and §5.8a shows it raising on a
    /// work seat (`RevisionConflict: expected 1, actual 2`, the third review's probe 2
    /// line G). So the work lease binds its seat, writes its scope, and stops. His
    /// executive continuity is his.
    ///
    /// It also turns the standing action grant ON (spec §5.4): the engine's work adapter
    /// refuses every work tool without it (`mega-lander/app.py:41-42`), so a background
    /// lease can only use them by holding a grant with no visible turn. That grant is
    /// revoked by [`Cognition::revoke_work_assignment`], per assignment, by name.
    ///
    /// The default is a refusal, not a no-op: a lease that is not a work lease must not
    /// quietly accept an assignment binding and then behave like a conversation.
    fn bind_work_assignment(&mut self, _assignment: &WorkAssignment) -> Result<(), CognitionError> {
        Err(CognitionError::Protocol(
            "This connection cannot take a background assignment.".into(),
        ))
    }

    /// What this lease's own `system/init` frame said about the three facts a background
    /// assignment depends on — the engine plugin, the work tools, and automatic permission
    /// checks. **Asked after its first turn, because that is when the frame arrives.**
    ///
    /// [`Cognition::bind_work_assignment`] cannot ask: it runs before the lease has ever
    /// taken a turn (`work_host.rs`'s `run_one` — `ensure_lease`, bind, then the turn), so
    /// the child has reported nothing at that point. A bind that treated an unreported fact
    /// as a refusal refused **every** background job this product was given, on every
    /// machine, for the whole life of the feature: candidate .7, 2026-09-18, two assignments
    /// failed 6.545 s and 7.486 s after registration with `work_session: null`.
    ///
    /// So the bind refuses only a reported absence and this is where the rest is settled. It
    /// is a MOVE and not a relaxation: the check exists because `--plugin-dir` accepts a path
    /// it cannot use and still reports success (`native.rs`'s module doc, measured cell K4),
    /// so a genuinely absent plugin must still fail the job and still say so in the same
    /// words. Anything short of a reported `Yes` is a refusal on this path.
    ///
    /// **The default is `Ok(())`**, and that is not a soft default: a cognition with no init
    /// frame of its own has nothing to report, and manufacturing a refusal from an absence of
    /// machinery is the same mistake one level up.
    fn work_readiness_after_turn(&self) -> Result<(), CognitionError> {
        Ok(())
    }

    /// Revoke the standing action grant this lease is holding (spec §5.4: a grant and its
    /// seat are created together and revoked together, per assignment). Idempotent, and
    /// safe to call on a lease that never held one.
    fn revoke_work_assignment(&mut self) -> Result<(), CognitionError> {
        Ok(())
    }

    /// Read the obligation's own state on this lease's seat — the one answer allowed to
    /// settle an assignment (see [`ObligationState`]).
    ///
    /// The default is an error, and the caller must treat an error as *still running*:
    /// spec §6.1's rule that anything unwitnessed counts as open, never as zero.
    fn obligation_state(&self, _obligation: &str) -> Result<ObligationState, CognitionError> {
        Err(CognitionError::Protocol(
            "This connection cannot read an obligation.".into(),
        ))
    }
}

/// A scripted Cognition for tests. Records every call so tests can assert that
/// re-prime happened and that the right prompts were delivered in the right order.
pub struct MockCognition {
    session_id: String,
    /// Canned replies, consumed FIFO. If exhausted, echoes a default.
    replies: Arc<Mutex<VecDeque<String>>>,
    /// Shared handles so tests can inspect calls after the mock is boxed into the Spine.
    pub reprimes: Arc<Mutex<Vec<String>>>,
    pub prompts: Arc<Mutex<Vec<String>>>,
    pub recovery_contexts: Arc<Mutex<Vec<String>>>,
    pub request_origins: Arc<Mutex<Vec<(String,String)>>>,
    pub conversation_contexts: Arc<Mutex<Vec<String>>>,
    /// RAW native frames a test parked as between-turn traffic (§1.5 gap #1).
    ///
    /// Raw wire JSON, NOT pre-built records, deliberately: a test that pushed a finished
    /// `MachineryRecord` would prove the spine can carry a record it was handed and nothing
    /// about whether a `system/init` frame normalizes into one. This runs the same
    /// `MachineryRecord::from_native_between_turn` the real client's drain runs.
    pub between_updates: Arc<Mutex<VecDeque<serde_json::Value>>>,
    /// The lane's counter, mirroring `native::BetweenTurn::next_seq`.
    between_seq: Arc<Mutex<u64>>,
}

impl MockCognition {
    pub fn new(session_id: &str, replies: Vec<&str>) -> Self {
        MockCognition {
            session_id: session_id.to_string(),
            replies: Arc::new(Mutex::new(replies.into_iter().map(|s| s.to_string()).collect())),
            reprimes: Arc::new(Mutex::new(Vec::new())),
            prompts: Arc::new(Mutex::new(Vec::new())),
            recovery_contexts: Arc::new(Mutex::new(Vec::new())),
            request_origins: Arc::new(Mutex::new(Vec::new())),
            conversation_contexts: Arc::new(Mutex::new(Vec::new())),
            between_updates: Arc::new(Mutex::new(VecDeque::new())),
            between_seq: Arc::new(Mutex::new(0)),
        }
    }

    /// Like `new`, but shares an existing reply queue rather than owning a fresh one —
    /// lets a `LeaseFactory` hand successive rotated leases ONE continuous script, so a
    /// rotation test can assert "the conversation continues" across a lease swap.
    fn new_with_shared_replies(session_id: &str, replies: Arc<Mutex<VecDeque<String>>>) -> Self {
        MockCognition {
            session_id: session_id.to_string(),
            replies,
            reprimes: Arc::new(Mutex::new(Vec::new())),
            prompts: Arc::new(Mutex::new(Vec::new())),
            recovery_contexts: Arc::new(Mutex::new(Vec::new())),
            request_origins: Arc::new(Mutex::new(Vec::new())),
            conversation_contexts: Arc::new(Mutex::new(Vec::new())),
            between_updates: Arc::new(Mutex::new(VecDeque::new())),
            between_seq: Arc::new(Mutex::new(0)),
        }
    }

    /// Park one raw native frame as if the agent had emitted it with no turn in flight.
    pub fn emit_between_turn(&self, update: serde_json::Value) {
        self.between_updates.lock().unwrap().push_back(update);
    }
}

impl Cognition for MockCognition {
    fn set_conversation_context(&mut self, context:&str) {self.conversation_contexts.lock().unwrap().push(context.into());}

    fn prepare_work_turn(&mut self, _: &crate::entity::ThreadBinding, turn: &str, original_turn: &str,
        _: crate::ledger::Source, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
        self.request_origins.lock().unwrap().push((turn.into(),original_turn.into())); Ok(())
    }

    fn set_previous_delivery_attempt(&mut self, context: &str) -> Result<(), CognitionError> {
        self.recovery_contexts.lock().unwrap().push(context.into()); Ok(())
    }

    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn reprime(&mut self, priming_text: &str, _on_item: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
        self.reprimes.lock().unwrap().push(priming_text.to_string());
        Ok(())
    }

    fn prompt(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        self.prompts.lock().unwrap().push(text.to_string());
        let reply = self.replies.lock().unwrap().pop_front().unwrap_or_else(|| format!("ack: {text}"));
        // Stream in two chunks to exercise incremental delta persistence. `seq` is
        // assigned HERE, by the lease, exactly as the real client does at its drain
        // point — so the shared-counter contract is exercised by every spine test.
        let mut seq = 0u64;
        let mut emit = |s: &str| {
            on_item(TurnItem::Text { seq, text: s });
            seq += 1;
        };
        // Split at a CHAR boundary: `&reply[..len/2]` panics mid-codepoint on any
        // non-ASCII reply, and a test fixture with an em-dash in it is enough.
        let mid = reply
            .char_indices()
            .nth(reply.chars().count() / 2)
            .map(|(i, _)| i)
            .unwrap_or(0);
        if mid > 0 {
            emit(&reply[..mid]);
            emit(&reply[mid..]);
        } else {
            emit(&reply);
        }
        Ok("end_turn".to_string())
    }

    fn drain_between_turn(&mut self) -> Vec<MachineryRecord> {
        let mut seq = self.between_seq.lock().unwrap();
        let mut out = Vec::new();
        for frame in std::mem::take(&mut *self.between_updates.lock().unwrap()) {
            let records = MachineryRecord::from_native_between_turn(&frame, &self.session_id, *seq);
            *seq += records.len() as u64;
            out.extend(records);
        }
        out
    }
}

/// A scripted `LeaseFactory` for tests — hands out fresh `MockCognition`s (one per
/// `spawn()` call, each with its own session id so rotation is independently
/// verifiable) with a shared canned-reply script, OR fails on demand to exercise the
/// "recovery/rotation itself can't proceed" honest-failure path.
pub struct MockLeaseFactory {
    next_session_suffix: Arc<Mutex<u64>>,
    replies: Arc<Mutex<VecDeque<String>>>,
    /// Every `Cognition` this factory has ever spawned, in spawn order — so a test can
    /// assert on re-prime calls / prompts made to EACH successive lease.
    pub spawned: Arc<Mutex<Vec<Arc<Mutex<Vec<String>>>>>>, // per-spawn reprimes log
    pub spawned_prompts: Arc<Mutex<Vec<Arc<Mutex<Vec<String>>>>>>, // per-spawn prompts log
    pub fail_next: Arc<Mutex<bool>>,
    /// **`(entity_id, thread_id)` of every SCOPED spawn, in spawn order.**
    ///
    /// It exists because the thing a spare front desk must get right cannot be seen any other
    /// way: `EngineProfile::scope_to` pins those two values into the child's environment and
    /// workspace partition BEFORE it is spawned, so "the spare was spawned under the id the
    /// thread ends up with" is a statement about the ARGUMENT to this call and about nothing
    /// observable afterwards. A mock that discarded the binding could not tell a correctly
    /// scoped spare from one adopted across threads.
    pub scoped_to: Arc<Mutex<Vec<(String, String)>>>,
    /// **A frame every spawned lease has already parked on its between-turn lane**, as a real
    /// child does: `NativeClient` re-announces its commands at session start, with no turn in
    /// flight to own them.
    ///
    /// It exists so that "the spare's session-start chatter never reaches a turn he is watching"
    /// is a test with something to lose rather than a negative that passes because the double
    /// emits nothing. `MockCognition::emit_between_turn` cannot be used for it: the spare's lease
    /// is built inside the factory and primed before any test can reach it, which is exactly when
    /// a real child's announcement arrives.
    pub emits_at_spawn: Arc<Mutex<Option<serde_json::Value>>>,
}

impl MockLeaseFactory {
    pub fn new(replies: Vec<&str>) -> Self {
        MockLeaseFactory {
            next_session_suffix: Arc::new(Mutex::new(1)),
            replies: Arc::new(Mutex::new(replies.into_iter().map(|s| s.to_string()).collect())),
            spawned: Arc::new(Mutex::new(Vec::new())),
            spawned_prompts: Arc::new(Mutex::new(Vec::new())),
            fail_next: Arc::new(Mutex::new(false)),
            scoped_to: Arc::new(Mutex::new(Vec::new())),
            emits_at_spawn: Arc::new(Mutex::new(None)),
        }
    }

    /// The next `spawn()` call fails once (simulating e.g. Claude not signed in),
    /// resetting itself after firing.
    pub fn fail_next_spawn(&self) {
        *self.fail_next.lock().unwrap() = true;
    }

    pub fn spawn_count(&self) -> usize {
        self.spawned.lock().unwrap().len()
    }

    /// Every lease this factory spawns from now on comes up with `frame` already parked on its
    /// between-turn lane. See [`MockLeaseFactory::emits_at_spawn`].
    pub fn every_child_announces_itself(&self, frame: serde_json::Value) {
        *self.emits_at_spawn.lock().unwrap() = Some(frame);
    }
}

impl LeaseFactory for MockLeaseFactory {
    /// Records the binding and then spawns exactly as the un-scoped path does — the real
    /// factory's own shape (`EngineLeaseFactory::spawn_scoped` is `spawn_chat` with the
    /// binding), so nothing about the lease differs and only the observation is added.
    fn spawn_scoped(&self, binding: &crate::entity::ThreadBinding, control: &crate::steering::ConversationControl)
        -> Result<Box<dyn Cognition>, CognitionError> {
        self.scoped_to.lock().unwrap()
            .push((binding.entity_id().to_string(), binding.thread_id().to_string()));
        self.spawn_cancellable(control)
    }

    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        if std::mem::take(&mut *self.fail_next.lock().unwrap()) {
            return Err(CognitionError::Io("mock factory: forced spawn failure".into()));
        }
        let mut suffix = self.next_session_suffix.lock().unwrap();
        let session_id = format!("sess-rotated-{suffix}");
        *suffix += 1;
        drop(suffix);

        // SHARE the reply queue (not drain-and-copy) — every successor pulls from the
        // SAME script, one continuous conversation across rotations.
        let mock = MockCognition::new_with_shared_replies(&session_id, self.replies.clone());
        if let Some(frame) = self.emits_at_spawn.lock().unwrap().clone() {
            mock.emit_between_turn(frame);
        }
        self.spawned.lock().unwrap().push(mock.reprimes.clone());
        self.spawned_prompts.lock().unwrap().push(mock.prompts.clone());
        Ok(Box::new(mock))
    }
}

// ---------------------------------------------------------------------------------------
// A CANCELLABLE test double
// ---------------------------------------------------------------------------------------

/// A `Cognition` that streams slowly and can actually be stopped — the double the stop
/// path needs, because `MockCognition` returns before a stop could possibly race it.
///
/// It emits `chunks` pieces of text, sleeping `step` between them, and checks its cancel
/// flag before each one. So a test can start a turn on one thread, press stop on another,
/// and assert on what the LEDGER holds afterwards: the partial text that arrived before
/// the stop, and a terminal state that says the CEO ended it.
pub struct CancellableMockCognition {
    session_id: String,
    chunks: Vec<String>,
    step: std::time::Duration,
    cancel: Arc<CancelFlag>,
    /// Every prompt this lease was handed, for ordering assertions.
    pub prompts: Arc<Mutex<Vec<String>>>,
    pub recovery_contexts: Arc<Mutex<Vec<String>>>,
    /// How many chunks were actually delivered before the cancel landed.
    pub delivered: Arc<Mutex<usize>>,
}

/// The shared flag behind [`CancellableMockCognition`]'s cancel seam.
pub struct CancelFlag {
    flag: AtomicBool,
    /// Whether a cancel was ever requested — separate from `flag`, which the lease clears
    /// at the start of each accepted operation, so tests can distinguish asking from acting.
    pub requested: AtomicBool,
}

impl TurnCancel for CancelFlag {
    fn begin_operation(&self) { self.flag.store(false, Ordering::SeqCst); }
    fn end_operation(&self) { self.flag.store(false, Ordering::SeqCst); }

    fn cancel(&self) -> bool {
        self.requested.store(true, Ordering::SeqCst);
        self.flag.store(true, Ordering::SeqCst);
        true
    }
}

impl CancellableMockCognition {
    pub fn new(session_id: &str, chunks: Vec<&str>, step: std::time::Duration) -> Self {
        CancellableMockCognition {
            session_id: session_id.to_string(),
            chunks: chunks.into_iter().map(|s| s.to_string()).collect(),
            step,
            cancel: Arc::new(CancelFlag { flag: AtomicBool::new(false), requested: AtomicBool::new(false) }),
            prompts: Arc::new(Mutex::new(Vec::new())),
            recovery_contexts: Arc::new(Mutex::new(Vec::new())),
            delivered: Arc::new(Mutex::new(0)),
        }
    }

    pub fn flag(&self) -> Arc<CancelFlag> {
        Arc::clone(&self.cancel)
    }
}

impl Cognition for CancellableMockCognition {
    fn set_previous_delivery_attempt(&mut self, context: &str) -> Result<(), CognitionError> {
        self.recovery_contexts.lock().unwrap().push(context.into()); Ok(())
    }

    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn reprime(&mut self, _priming_text: &str, _on_item: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
        Ok(())
    }

    fn prompt(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        self.prompts.lock().unwrap().push(text.to_string());
        let mut seq = 0u64;
        for chunk in &self.chunks {
            if self.cancel.flag.load(Ordering::SeqCst) {
                // Exactly what the real client does: stop delivering, return the cancelled
                // stopReason, and leave everything already persisted alone (§9.3 step 4).
                return Ok(crate::native::STOP_REASON_CANCELLED.to_string());
            }
            on_item(TurnItem::Text { seq, text: chunk });
            seq += 1;
            *self.delivered.lock().unwrap() += 1;
            std::thread::sleep(self.step);
        }
        Ok("end_turn".to_string())
    }

    fn cancel_handle(&self) -> Option<Arc<dyn TurnCancel>> {
        Some(self.flag() as Arc<dyn TurnCancel>)
    }
}

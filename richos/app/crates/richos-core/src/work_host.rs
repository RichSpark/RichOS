//! One independent Stu session per conversation, driven only by its durable inbox.
//! Job reports belong to Stu and the existing engine work tools. This host manages
//! provider connections and transports facts; process lifecycle never changes job state.
use crate::assignment::{self, Assignment, AssignmentState, PendingNotice};
use crate::cognition::{Cognition, CognitionError, LeaseFactory, TurnItem};
use crate::entity::ThreadBinding;
use crate::steering::{CommandStop, TurnCancel};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

pub trait WorkNotifier: Send + Sync {
    fn raised(&self, thread_id: &str, notice: &PendingNotice);
    fn nothing_left_to_do(&self) {}
    /// Stu's work met an overload and waits out the CEO's schedule: `sentence` is
    /// `RetryBudget::ceo_message`, the same "RichOS will try again in N minutes" line his
    /// conversation shows for Rich, for the conversation `thread_id`. A turn of his that a usage
    /// limit cut, held until the reset with no account left, is shown here too
    /// (`usage_limit_hold_sentence`).
    fn overload_retry(&self, _thread_id: &str, _sentence: &str) {}
}
pub const ROTATION_WAITS_FOR_COMMANDS: std::time::Duration = std::time::Duration::from_secs(600);
/// **How often an idle back end checks that Stu is on the account in use** (review
/// rv-20261019T005929Z-e3d9ad03-3e7b, finding 2). A switch decided while Stu is idle, by Rich's
/// turn or the quota service's own check, otherwise left his session on the old account until
/// the next message the app sent him. The check reads two ids; the move itself waits for a
/// boundary (`WorkHost::move_to_account_in_use`).
pub const ACCOUNT_CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(30);

/// What renewing Stu's lease for the account in use came to.
#[derive(Debug, PartialEq)]
enum Renewal {
    /// No switch is due, or there is no lease to move.
    NotDue,
    /// Something of Stu's still runs on the old account: a turn (his own or an automatic one),
    /// input of ours, a command or a helper. Ending his session now would end it.
    Waits { from: Option<String> },
    /// He is on a new session on the account in use.
    Moved { from: Option<String>, to: Option<String> },
    /// The old session ended and no new one could be opened; the next boundary opens one.
    Failed,
}

#[derive(Clone)]
enum InputReceipt { Peer(u64), Answer(String) }
impl InputReceipt {
    fn input_id(&self)->String {match self {Self::Peer(through)=>format!("peer:{through}"),Self::Answer(id)=>format!("answer:{id}")}}
    /// The receipt of a run-loop turn named `turn` (`peer:<offset>` or `answer:<id>`).
    fn of_turn(turn: &str) -> Option<Self> {
        turn.strip_prefix("peer:").and_then(|through| through.parse().ok()).map(Self::Peer)
            .or_else(|| turn.strip_prefix("answer:").map(|id| Self::Answer(id.to_string())))
    }
}

/// A wait of the workers' overload clock on one Stu session: the retry it is and how long it
/// is, and the `worker_overload` fact for each worker it will restart.
struct WorkerWait {
    session: String,
    retry: u32,
    wait: std::time::Duration,
    facts: Vec<serde_json::Value>,
}

/// The hold before the one retry of a turn of Stu's that a usage limit cut
/// ([`WorkHost::usage_limit_hold`]): how long its first wait is, the session the cut turn ran
/// on (for the Stop checks) and the account that refused it.
struct UsageLimitHold {
    wait: std::time::Duration,
    session: String,
    account: String,
}

#[derive(Default)]
struct Inner {
    peer_input: Option<Arc<dyn TurnCancel>>,
    overload: crate::upstream::RetryBudget,
    /// ONE overload clock for all of Stu's WORKERS (follow-up 11), counted apart from his own
    /// so a turn of his that succeeds does not restart theirs. An overload is Anthropic's
    /// servers being overloaded and hits every running worker at once, so it is one schedule,
    /// as Rich's and Stu's own are: the overloads that arrive during a wait join it
    /// ([`Inner::worker_wait`]), the next one after it moves one step, a success resets it.
    workers: crate::upstream::RetryBudget,
    /// The wait the workers' clock is running, and every worker the overload stopped, each told
    /// to Stu when it ends.
    worker_wait: Option<WorkerWait>,
    /// The description each worker run was started under, by the tool call that started it
    /// (Claude Code's `task_started`), told to Stu as the worker's task.
    worker_names: std::collections::HashMap<String, String>,
    /// Inputs accepted by this live backend. Unfinished handoffs remain durable for restart.
    // Input already delivered to this session stays delivered even when a turn fails.
    peer_active_through: Option<u64>,
    peer_stopper: Option<(String, Arc<dyn TurnCancel>)>,
    processing: bool,
    cancel: Option<Arc<dyn TurnCancel>>,
    lease_session: Option<String>,
    binding: Option<ThreadBinding>,
    closing: bool,
    completed: u64,
    rotations: u64,
    last_rotation_reason: Option<String>,
    lease_team: Option<String>,
    /// **An automatic turn of Stu's is running on the live lease** (Claude Code started it
    /// itself: a helper's hand-back, a task notification): begun on the between-turn lane and
    /// not yet ended by its `result`. Nightly 65's whole website job ran as such turns.
    automatic_turn: bool,
    /// The run loop is to check that Stu is on the account in use: `Some(true)` when one of his
    /// automatic turns has ended (the full decision, as before every turn), `Some(false)` at the
    /// idle check ([`ACCOUNT_CHECK_EVERY`]).
    move_check: Option<bool>,
    /// The account Stu was told he is being moved to (`account_switch`), until he has moved.
    switch_told: Option<String>,
    /// **Recoveries of a cut automatic turn of Stu's now running**
    /// ([`WorkHost::recover_automatic_turn`]). An `account_moved` sent meanwhile is the one
    /// message that carries the cut turn on (the recovery reads any move off the refused
    /// account as such), so it is marked as having had the one retry
    /// ([`WorkHost::usage_limit_retry_spent`]). Every other `account_moved` is not.
    automatic_recoveries: u32,
    /// Inbox offsets of handoffs whose screen wait ended with the screen back (§56,
    /// [`WorkHost::held_for_screen`]). Later input may have been delivered past them meanwhile,
    /// so they are delivered from here rather than found again by reading forward.
    screen_released: Vec<u64>,
    /// **Inbox offsets each provider session was given in this process**, by session id. A
    /// handoff stays in Stu's inbox until its job closes (`session_tools::pending_after`), and
    /// [`Inner::peer_active_through`] is cleared whenever his lease is renewed or its
    /// connection fails, which read every open handoff out to the SAME resumed session again
    /// (2026-10-28 ~23:00Z: Stu was handed the nightly-78 handoff three times within minutes,
    /// two of them with no `previous_delivery_attempt`). What a session was given here is never
    /// given to it again here; the next process hands it back once, as a reconciliation that
    /// names the earlier attempt.
    delivered: std::collections::HashMap<String, std::collections::HashSet<u64>>,
    /// **Input each provider session was given in a turn that ended without `end_turn`**, by
    /// session id, that nothing will send again (Otto's trace of 2026-10-29: a message delivered
    /// into a turn a 429 cut stayed only received; this process never sent it again, its sent
    /// set lives here, but the next one did, after Stu had acted on it, at 11:44Z). Once a later
    /// turn of the same session ends with `end_turn`, he has had it: it is marked done
    /// ([`WorkHost::settle_cut`]), as its own turn ending so would have done.
    cut: std::collections::HashMap<String, Vec<InputReceipt>>,
}
impl Inner {
    /// What `session` was already given in this process, for [`WorkHost::next_for_stu`].
    fn given_to(&self, session: Option<&str>) -> std::collections::HashSet<u64> {
        session.and_then(|session| self.delivered.get(session)).cloned().unwrap_or_default()
    }
    fn note_given(&mut self, session: &str, through: u64) {
        self.delivered.entry(session.to_string()).or_default().insert(through);
    }
}
struct Backend {
    live_delivery: Mutex<()>,
    /// Held across a move's decision and the renewal it makes ([`WorkHost::move_to_account_in_use`]),
    /// so a move decided off the run loop (the usage-limit recovery of a live input or of an
    /// automatic turn) and the run loop's own never both renew Stu's lease for one switch.
    moving: Mutex<()>,
    lease: Mutex<Option<Box<dyn Cognition>>>,
    inner: Mutex<Inner>,
    wake: Condvar,
}
impl Backend {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            live_delivery: Mutex::new(()),
            moving: Mutex::new(()),
            lease: Mutex::new(None),
            inner: Mutex::new(Inner::default()),
            wake: Condvar::new(),
        })
    }
}
pub struct WorkHost {
    state: PathBuf,
    factory: Arc<dyn LeaseFactory>,
    backends: Mutex<std::collections::HashMap<String, Arc<Backend>>>,
    closing: Mutex<bool>,
    notifier: Arc<dyn WorkNotifier>,
    screen: Mutex<Arc<dyn crate::screen::ScreenSource>>,
    /// Every job waiting for the screen (§56), by assignment id: the wait a Stop or a quit ends.
    screen_waits: Mutex<std::collections::HashMap<String, Arc<crate::screen::ScreenWatch>>>,
    screen_poll: Mutex<std::time::Duration>,
    quota: Mutex<Option<Arc<crate::quota::Service>>>,
    output: Mutex<Option<crate::output::OutputStore>>,
    /// The app's own cleanup (CEO §54 addendum 2), run when nothing is left to do.
    cleanup: Mutex<Option<crate::app_cleanup::Hook>>,
    retry_clock: Mutex<Arc<dyn crate::upstream::RetryClock>>,
}
impl WorkHost {
    pub fn new(
        state: &Path,
        factory: Box<dyn LeaseFactory>,
        notifier: Arc<dyn WorkNotifier>,
    ) -> Arc<Self> {
        Arc::new(Self {
            state: state.into(),
            factory: Arc::from(factory),
            backends: Mutex::new(Default::default()),
            closing: Mutex::new(false),
            notifier,
            screen: Mutex::new(Arc::new(crate::screen::UnknownScreen)),
            screen_waits: Mutex::new(Default::default()),
            screen_poll: Mutex::new(crate::screen::SCREEN_POLL),
            quota: Mutex::new(None),
            output: Mutex::new(None),
            cleanup: Mutex::new(None),
            retry_clock: Mutex::new(Arc::new(crate::upstream::SystemRetryClock)),
        })
    }
    pub fn set_retry_clock(&self, clock: Arc<dyn crate::upstream::RetryClock>) {
        *self.retry_clock.lock().unwrap() = clock;
    }

    fn overload_wait(&self, backend: &Backend, binding: &ThreadBinding, result: &Result<String, String>) -> Option<std::time::Duration> {
        let overloaded = result.as_ref().err()
            .and_then(|error| crate::upstream::UpstreamFailure::classify_lines(error))
            .is_some_and(|failure| failure.fault == crate::upstream::UpstreamFault::Overloaded);
        let mut inner = backend.inner.lock().unwrap();
        if !overloaded {
            if result.is_ok() { inner.overload.succeeded(); }
            return None;
        }
        if !inner.overload.charge(crate::upstream::UpstreamFault::Overloaded) { return None; }
        let wait = inner.overload.wait_before_retry();
        let said = inner.overload.ceo_message();
        drop(inner);
        // The wait is shown as Rich's is: RichOS's own retry sentence, never Stu's words.
        if let Some(said) = said { self.notifier.overload_retry(binding.thread_id(), &said); }
        wait
    }

    fn retry_stopped(&self, backend: &Backend, binding: &ThreadBinding,
        message: Option<&crate::session_tools::Message>, session: &str) -> bool {
        let inner = backend.inner.lock().unwrap();
        let ended = inner.closing || inner.lease_session.as_deref() != Some(session);
        drop(inner);
        ended || message.is_some_and(|message| self.handoff_stopped(binding, message).unwrap_or(true))
    }

    /// **A turn that ended with `reason` ran the input it was given**, so that input is in the
    /// session's transcript and a later `end_turn` there means Stu had it ([`Inner::cut`]). Not a
    /// Stop (`STOP_REASON_CANCELLED`, which is also what input canceled before it ran says, or
    /// its unacknowledged form) and not input the child threw away before any turn ran it: those
    /// keep the reconciliation a restart gives them, as before.
    fn reason_ran(reason: &str) -> bool {
        use crate::native::{STOP_REASON_CANCELLED, STOP_REASON_CANCEL_UNACKNOWLEDGED, STOP_REASON_DISCARDED, STOP_REASON_REFUSED};
        !matches!(reason, STOP_REASON_CANCELLED | STOP_REASON_CANCEL_UNACKNOWLEDGED | STOP_REASON_DISCARDED | STOP_REASON_REFUSED)
    }

    /// [`Self::reason_ran`] for live input's outcome, whose error is the native turn's
    /// (`native.rs`, `PendingTurn::finish`): a child that went away mid-turn
    /// (`NativeError::Exited`, `Closed`) may never have read the input, as `run_message`'s lost
    /// connection may not, so it is not taken as run.
    fn live_turn_ran(result: &Result<String, String>) -> bool {
        use crate::native::NativeError;
        match result {
            Ok(reason) => Self::reason_ran(reason),
            Err(error) => {
                let exited = NativeError::Exited { said: String::new() }.to_string();
                !error.starts_with(exited.trim_end()) && *error != NativeError::Closed.to_string()
            }
        }
    }

    /// `input` was given to a turn of `session` that ended without `end_turn`, and nothing sends
    /// it again ([`Inner::cut`]).
    fn note_cut(&self, backend: &Backend, session: Option<&str>, input: InputReceipt) {
        let Some(session) = session.filter(|session| !session.is_empty()) else { return };
        backend.inner.lock().unwrap().cut.entry(session.to_string()).or_default().push(input);
    }

    /// **A turn of `session` ended with `end_turn`**: the input that session was given in an
    /// earlier turn that did not ([`Inner::cut`]) is done now, as it would have been had its own
    /// turn ended so. A handoff whose job is still open stays open on its record, as at any
    /// `end_turn` (`session_tools::acknowledge`).
    fn settle_cut(&self, backend: &Backend, binding: &ThreadBinding, session: &str) {
        let cut = backend.inner.lock().unwrap().cut.remove(session).unwrap_or_default();
        if cut.is_empty() { return; }
        let scope = self.message_scope(binding);
        for input in cut {
            let done = match &input {
                InputReceipt::Peer(through) => crate::session_tools::acknowledge(&scope, "stu", *through),
                InputReceipt::Answer(id) => crate::question_work::taken(&self.state, std::slice::from_ref(id), session)
                    .and_then(|_| crate::session_tools::complete_attempt(&scope, "stu", &input.input_id())),
            };
            if let Err(error) = done {
                eprintln!("[richos] input a cut turn was given stays open: {error}");
            }
        }
    }

    /// **A turn of Stu's that a usage limit cut** (`error` is the lease's
    /// [`crate::claude_accounts::usage_limit_error`]): the backstop Rich's turns already have
    /// (`spine.rs`, "FILL-FIRST'S BACKSTOP"). The account is marked gone and the next with room
    /// put in use ([`crate::quota::Service::limit_reached`]), and Stu is moved there now when
    /// nothing of his still runs on the refused account (otherwise he is told the switch is
    /// coming, as for any switch). Returns the hold before the one retry: none once he is off
    /// the refused account; otherwise until that account's reset, or the soonest reset across
    /// the accounts when none has room. The hold ends early when [`Self::usage_limit_lifted`].
    ///
    /// `None` (no retry, the turn ends as before) when it is not a usage limit, or there is no
    /// quota service or no account to mark, as with Rich's.
    fn usage_limit_hold(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding, error: &str) -> Option<UsageLimitHold> {
        let resets_at = crate::claude_accounts::parse_usage_limit(error)?;
        let quota = self.quota.lock().unwrap().clone()?;
        let (account, session) = {
            let lease = backend.lease.lock().unwrap();
            let lease = lease.as_ref()?;
            (lease.account()?.to_string(), lease.session_id().to_string())
        };
        let continued = quota.limit_reached(&account, resets_at) == crate::claude_accounts::AfterLimit::Continue;
        // `Continue` says only that another account is in use, which it already is when another
        // conversation's turn moved there first; that account may since have been refused too
        // (review of e58c2c825, P2), or be used up by its readings with no refusal yet (second
        // review rv-20261029T111217Z-71fb5f11-8f6b, P2). It has room only when it is neither.
        let next = continued.then(|| quota.accounts.in_use().id);
        let room = next.as_deref().is_some_and(|id| Self::gone_until(&quota, id).is_none());
        if room {
            // **Never back to the refused account** (review of a6bd17fa5, P1): a command, helper
            // or automatic turn of his keeps his session there (`renew_lease`), and the retry
            // would only be refused again. So he moves now, or the hold lasts until he can.
            self.move_to_account_in_use(backend, binding, false);
        }
        // The Stop checks follow the session he is on now: a move just made is not his session
        // ending (`retry_stopped`).
        let session = backend.inner.lock().unwrap().lease_session.clone().unwrap_or(session);
        let hold = UsageLimitHold {wait: std::time::Duration::ZERO, session, account};
        if self.usage_limit_lifted(backend, &quota, &hold.account) {
            return Some(hold);
        }
        let now = crate::util::now_millis();
        let limited = |id: &str| Self::gone_until(&quota, id);
        // With room he waits only for his own session to come free, at most to this account's
        // reset; without, for the soonest reset of the accounts.
        let soonest = limited(&hold.account).into_iter().chain(next.as_deref().and_then(limited)).min();
        let held = (!room).then(|| quota.view().held_until).flatten();
        let until = held.or(soonest).unwrap_or(now);
        let wait = std::time::Duration::from_millis(until.saturating_sub(now));
        if !room {
            // Shown as the overload wait is: RichOS's own retry sentence, never Stu's words.
            self.notifier.overload_retry(binding.thread_id(), &usage_limit_hold_sentence(wait));
        }
        Some(UsageLimitHold {wait, ..hold})
    }

    /// **The hold before the one retry**, on the existing retry clock with the overload retry's
    /// Stop checks; it ends early once the refusal is lifted ([`Self::usage_limit_lifted`]).
    /// True when the retry is to go.
    ///
    /// **Each time a wait runs out, availability is decided again before the input goes** (review
    /// rv-20261029T140028Z-3e883526-10b9, finding 2): a refusal mark is read before an account's
    /// readings ([`Self::gone_until`]), so the reset it was held to can arrive while the
    /// readings still show that account used up, its week for days. And a hold that ran out at
    /// another account's sooner reset may find his session still busy on the refused account
    /// (second review of 71fb5f113). So at each such moment the switch is decided as before any
    /// turn and he moves if nothing of his still runs; the input goes when that lifted the
    /// refusal, or when the refused account has room again by what is known of it at the moment
    /// the hold has reached. Otherwise the hold goes on, to the soonest moment that can change:
    /// the refused account's own return, or the soonest reset across the accounts when none has
    /// room. Each wait reaches a later moment than the one before, so the hold ends. Still one
    /// retry, never a loop of sends.
    fn usage_limit_waited(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding,
        message: Option<&crate::session_tools::Message>, hold: &UsageLimitHold) -> bool {
        let quota = self.quota.lock().unwrap().clone();
        let lifted = || quota.as_deref().is_some_and(|quota| self.usage_limit_lifted(backend, quota, &hold.account));
        // The Stop checks follow the session he is on: a move this hold makes is not his session
        // ending (as in `usage_limit_hold`).
        let session = std::cell::RefCell::new(hold.session.clone());
        let stopped = || self.retry_stopped(backend, binding, message, &session.borrow());
        let clock = self.retry_clock.lock().unwrap().clone();
        let millis = |wait: std::time::Duration| u64::try_from(wait.as_millis()).unwrap_or(u64::MAX);
        let mut wait = hold.wait;
        // The moment the hold has reached: what ends by then has ended.
        let mut reached = crate::util::now_millis();
        loop {
            let target = crate::util::now_millis().saturating_add(millis(wait));
            let ran_out = clock.wait(wait, &|| stopped() || lifted());
            if stopped() { return false; }
            if lifted() { return true; }
            if !ran_out { return false; }
            reached = reached.max(target).max(crate::util::now_millis());
            self.move_to_account_in_use(backend, binding, true);
            let now_on = backend.inner.lock().unwrap().lease_session.clone();
            if let Some(now_on) = now_on { *session.borrow_mut() = now_on; }
            if lifted() { return true; }
            let Some(quota) = quota.as_deref() else { return true };
            let Some(back) = Self::gone_until_at(quota, &hold.account, reached) else { return true };
            let held = quota.view_at(reached).held_until.filter(|until| *until > reached);
            let until = held.map_or(back, |held| held.min(back));
            wait = std::time::Duration::from_millis(until.saturating_sub(crate::util::now_millis()));
        }
    }

    /// **`message` carries on an automatic turn a usage limit cut, after its hold**
    /// ([`Self::recover_automatic_turn`]'s `turn_usage_limit`): that hold was the one retry, so
    /// the turn it starts gets none of its own and whatever it comes to is final (review
    /// rv-20261029T140028Z-3e883526-10b9, finding 1: it was given a fresh one, so a second
    /// refusal was held and sent a third time). Read on both ways it reaches Stu: the run loop's
    /// (`run_message`) and live (`wake_messages`).
    ///
    /// Moved off the refused account, that one message is the `account_moved` he was told on
    /// arrival, marked `after_usage_limit` by the move ([`Inner::automatic_recoveries`]; review
    /// rv-20261029T150350Z-fc3e4c65-03ea: unmarked, it had a retry of its own). An ordinary
    /// `account_moved` carries no mark and keeps its one retry.
    fn usage_limit_retry_spent(message: &crate::session_tools::Message) -> bool {
        if message.handoff { return false; }
        let Ok(fact) = serde_json::from_str::<serde_json::Value>(&message.text) else { return false };
        match message.from.as_str() {
            "overload-retry" => fact["event"] == "turn_usage_limit",
            "account-switch" => fact["event"] == "account_moved" && fact["after_usage_limit"] == true,
            _ => false,
        }
    }

    /// A usage limit cut this turn and there is a quota service to answer it with
    /// ([`Self::usage_limit_hold`]). Reads no lease, so the native reader may ask it.
    fn usage_limit_answerable(&self, result: &Result<String, String>) -> bool {
        result.as_ref().err().is_some_and(|error| crate::claude_accounts::parse_usage_limit(error).is_some())
            && self.quota.lock().unwrap().is_some()
    }

    /// **Live input whose turn a usage limit cut** (second review
    /// rv-20261029T111217Z-71fb5f11-8f6b, P1: the recovery was on `run_message` only, while
    /// Rich's messages, answers and corrections reach a Stu who is working live). The same
    /// recovery, off the native reader, which must keep reading: the account is marked gone and
    /// Stu moved to one with room, or the input is held until the refusal is lifted
    /// ([`Self::usage_limit_waited`]); then the same input goes once more, live, naming the cut
    /// attempt. Its completion has no retry of its own, so whatever it comes to is final. When it
    /// cannot be sent, the cut turn's failure is reported as before.
    fn retry_live_after_usage_limit(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding,
        message: crate::session_tools::Message, delivered: (InputReceipt, Option<String>), text: String,
        original: Result<String, String>) -> std::io::Result<()> {
        let (host, receiving, binding) = (Arc::downgrade(self), Arc::downgrade(backend), binding.clone());
        std::thread::Builder::new().name("richos-stu-usage-limit".into()).spawn(move || {
            let (Some(host), Some(backend)) = (host.upgrade(), receiving.upgrade()) else { return };
            let (input, cut_session) = delivered;
            let input_id = input.input_id();
            let failed = || {
                if let Err(error) = host.observe_turn_result(&binding, Some(&message), Some(&input_id),
                    cut_session.as_deref().unwrap_or(""), &original) {
                    eprintln!("[richos] native input status: {error}");
                }
                host.note_cut(&backend, cut_session.as_deref(), input.clone());
            };
            let error = original.as_ref().err().cloned().unwrap_or_default();
            let Some(hold) = host.usage_limit_hold(&backend, &binding, &error) else { failed(); return };
            if !host.usage_limit_waited(&backend, &binding, Some(&message), &hold) { return; }
            // A hold that ended because another account has room and his session came free: he
            // goes there before the input does, never back to the refused account.
            host.move_to_account_in_use(&backend, &binding, false);
            let resent = {
                let _delivery = backend.live_delivery.lock().unwrap();
                let (session, live) = {
                    let inner = backend.inner.lock().unwrap();
                    let session = inner.lease_session.clone().filter(|_| !inner.closing);
                    let live = inner.peer_stopper.clone().filter(|(owner, _)| Some(owner) == session.as_ref()).map(|(_, live)| live);
                    (session, live)
                };
                if host.handoff_stopped(&binding, &message).unwrap_or(true) { return; }
                let scope = host.message_scope(&binding);
                (|| -> Result<bool, String> {
                    let (Some(session), Some(live)) = (session, live) else { return Ok(false) };
                    let mut text = text.clone();
                    if let InputReceipt::Peer(through) = &input {
                        // Named as `run_message`'s second delivery is, read before `begin_attempt`
                        // replaces the cut attempt with this one.
                        let mut payload: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
                        payload["previous_delivery_attempt"] = serde_json::to_value(
                            crate::session_tools::previous_attempt(&scope, "stu", &input_id)?).map_err(|e| e.to_string())?;
                        text = payload.to_string();
                        backend.inner.lock().unwrap().note_given(&session, *through);
                    }
                    crate::session_tools::begin_attempt(&scope, "stu", &input_id, &session)?;
                    let completed = host.live_completion_with(&backend, &binding, message.clone(),
                        (input.clone(), Some(session.clone())), text.clone(), false);
                    live.send_input(&uuid::Uuid::new_v4().to_string(), &text, completed)
                })()
            };
            match resent {
                Ok(true) => host.wake_messages(&binding),
                Ok(false) => failed(),
                Err(error) => {
                    eprintln!("[richos] a live input cut by a usage limit was not sent again: {error}");
                    failed();
                }
            }
        }).map(|_| ())
    }

    /// **An automatic turn of Stu's that a usage limit cut** (second review
    /// rv-20261029T111217Z-71fb5f11-8f6b, P1: helper hand-backs and task notifications are
    /// normal Stu paths). Claude Code started it, so no input of ours can go again: the account
    /// is marked gone and Stu is moved to one with room, where `account_moved` tells him to carry
    /// on; or the hold lasts until the refusal is lifted and he is told once, through his own
    /// inbox, to carry on (`turn_usage_limit`, stu.md). Never more than that one message, and
    /// the turn it starts has no retry of its own ([`Self::usage_limit_retry_spent`]): the
    /// `account_moved` sent while this runs is marked so ([`Inner::automatic_recoveries`]).
    fn recover_automatic_turn(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding,
        session: &str, result: &Result<String, String>) -> std::io::Result<()> {
        let (host, receiving, binding) = (Arc::downgrade(self), Arc::downgrade(backend), binding.clone());
        let (session, result) = (session.to_string(), result.clone());
        /// Counts this recovery in [`Inner::automatic_recoveries`] until it ends, however it ends.
        struct Recovering(Arc<Backend>);
        impl Drop for Recovering {
            fn drop(&mut self) { self.0.inner.lock().unwrap().automatic_recoveries -= 1; }
        }
        std::thread::Builder::new().name("richos-stu-usage-limit".into()).spawn(move || {
            let (Some(host), Some(backend)) = (host.upgrade(), receiving.upgrade()) else { return };
            // From here on, a move off the refused account is this recovery's one message.
            backend.inner.lock().unwrap().automatic_recoveries += 1;
            let recovering = Recovering(Arc::clone(&backend));
            let error = result.as_ref().err().cloned().unwrap_or_default();
            let Some(hold) = host.usage_limit_hold(&backend, &binding, &error) else {
                if let Err(error) = host.observe_turn_result(&binding, None, None, &session, &result) {
                    eprintln!("[richos] native turn status: {error}");
                }
                return;
            };
            if !host.usage_limit_waited(&backend, &binding, None, &hold) { return; }
            host.move_to_account_in_use(&backend, &binding, false);
            // Moved off the refused account, he was told so there (`account_moved`), which carries
            // the work on: that is his one message.
            let moved = backend.lease.lock().unwrap().as_ref()
                .and_then(|lease| lease.account().map(str::to_string)).is_some_and(|account| account != hold.account);
            if !moved {
                let fact = serde_json::json!({"event":"turn_usage_limit","error":error});
                if let Err(error) = crate::session_tools::observe_overload_retry(&host.message_scope(&binding), &fact) {
                    eprintln!("[richos] a usage limit could not be passed to Stu: {error}");
                    return;
                }
            }
            // Its one message is written: a move from here on is an ordinary one.
            drop(recovering);
            host.wake_messages(&binding);
        }).map(|_| ())
    }

    /// **Stu's input can go again without meeting the same refusal**: the refused `account` is
    /// no longer gone (its reset passed, or a reset was used on it), or another account is in
    /// use, is not gone itself, and nothing of his still holds his session on the refused one.
    /// Read by the hold before the retry, so it ends as soon as either is true rather than at
    /// the time it began with. "Gone" is [`Self::gone_until`].
    fn usage_limit_lifted(&self, backend: &Backend, quota: &crate::quota::Service, account: &str) -> bool {
        if Self::gone_until(quota, account).is_none() { return true; }
        let in_use = quota.accounts.in_use().id;
        if in_use == account || Self::gone_until(quota, &in_use).is_some() { return false; }
        let lease = backend.lease.lock().unwrap();
        lease.as_ref().is_none_or(|current| current.account() != Some(account) || !Self::session_in_use(backend, current.as_ref()))
    }

    /// **`id` has no room for Stu's input, and until when**: a usage-limit refusal that has not
    /// reset (the backstop's mark, [`crate::claude_accounts::Accounts::limited_until`]), or used
    /// up by its readings with no refusal yet: its row in the quota view says so
    /// (`exhausted_until`, the same `claude_accounts::gone` rule the switch decides on), or every
    /// account is held (`held_until`). Second review rv-20261029T111217Z-71fb5f11-8f6b, P2: an
    /// account the readings showed used up counted as available, so the one retry was spent on
    /// it. With one account the view has no rows and no hold, so only the refusal mark counts,
    /// as before.
    fn gone_until(quota: &crate::quota::Service, id: &str) -> Option<u64> {
        Self::gone_until_at(quota, id, crate::util::now_millis())
    }

    /// [`Self::gone_until`] at the moment `at`, which a hold has reached
    /// ([`Self::usage_limit_waited`]): a refusal mark or window that ends by then has ended.
    fn gone_until_at(quota: &crate::quota::Service, id: &str, at: u64) -> Option<u64> {
        if let Some(until) = quota.accounts.limited_until(id).filter(|until| *until > at) {
            return Some(until);
        }
        let view = quota.view_at(at);
        view.accounts.iter().find(|row| row.id == id).and_then(|row| row.exhausted_until)
            .or(view.held_until).filter(|until| *until > at)
    }

    fn live_completion(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding,
        message: crate::session_tools::Message, delivered: (InputReceipt, Option<String>), text: String,
    ) -> Box<dyn FnOnce(crate::steering::InputOutcome) + Send> {
        self.live_completion_with(backend, binding, message, delivered, text, true)
    }

    /// The completion of live input. `limit_retry`: a usage limit that cuts its turn gets the one
    /// retry ([`Self::retry_live_after_usage_limit`]); false on that retry's own completion, so
    /// whatever it comes to is final, never a loop.
    fn live_completion_with(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding,
        message: crate::session_tools::Message, delivered: (InputReceipt, Option<String>), text: String,
        limit_retry: bool,
    ) -> Box<dyn FnOnce(crate::steering::InputOutcome) + Send> {
        let host = Arc::downgrade(self);
        let receiving = Arc::downgrade(backend);
        let binding = binding.clone();
        Box::new(move |outcome| {
            let Some(host) = host.upgrade() else { return };
            let Some(backend) = receiving.upgrade() else { return };
            let (input, session) = delivered;
            let input_id=input.input_id();
            let scope = host.message_scope(&binding);
            let succeeded=outcome.result.as_ref().is_ok_and(|reason| reason=="end_turn");
            let receipt = match &input {
                InputReceipt::Peer(through) if succeeded=>crate::session_tools::acknowledge(&scope,"stu",*through),
                InputReceipt::Peer(through)=>crate::session_tools::accept_live_handoff(&scope,"stu",*through),
                InputReceipt::Answer(id) if succeeded=>crate::question_work::taken(&host.state,std::slice::from_ref(id),session.as_deref().unwrap_or(""))
                    .and_then(|_|crate::session_tools::complete_attempt(&scope,"stu",&input_id)),
                InputReceipt::Answer(_)=>Ok(()),
            };
            if let Err(error) = receipt { eprintln!("[richos] native input receipt remains pending: {error}"); }
            if succeeded {
                if let Some(session) = session.as_deref() { host.settle_cut(&backend, &binding, session); }
            }
            if outcome.report_failure {
                if let Some(wait) = host.overload_wait(&backend, &binding, &outcome.result) {
                    // The native reader must keep reading while the existing retry clock waits.
                    let retry_host = Arc::downgrade(&host);
                    let retry_backend = Arc::downgrade(&backend);
                    let retry_binding = binding.clone();
                    let retry_message = message.clone();
                    let retry_session = session.clone();
                    let original_error = outcome.result.clone();
                    let retry_receipt=input.clone();
                    let retry_input_id=input_id.clone();
                    let retry_text=text.clone();
                    let retry = std::thread::Builder::new().name("richos-stu-overload".into()).spawn(move || {
                        let (Some(host), Some(backend)) = (retry_host.upgrade(), retry_backend.upgrade()) else { return };
                        let owner = retry_session.as_deref().unwrap_or("");
                        let clock = host.retry_clock.lock().unwrap().clone();
                        if !clock.wait(wait, &||host.retry_stopped(&backend, &retry_binding, Some(&retry_message), owner)) { return; }
                        let _delivery = backend.live_delivery.lock().unwrap();
                        if host.retry_stopped(&backend, &retry_binding, Some(&retry_message), owner) { return; }
                        let input = backend.inner.lock().unwrap().peer_stopper.clone()
                            .filter(|(session, _)| session == owner).map(|(_, input)| input);
                        let completed = host.live_completion_with(&backend, &retry_binding, retry_message.clone(),
                            (retry_receipt.clone(), retry_session.clone()), retry_text.clone(), limit_retry);
                        let sent = input.map(|input|input.send_input(&uuid::Uuid::new_v4().to_string(), &retry_text, completed));
                        if !matches!(sent, Some(Ok(true))) {
                            if let Err(error) = host.observe_turn_result(&retry_binding, Some(&retry_message),
                                Some(&retry_input_id), owner, &original_error) {
                                eprintln!("[richos] native input status: {error}");
                            }
                            host.note_cut(&backend, retry_session.as_deref(), retry_receipt);
                        }
                    });
                    if retry.is_ok() { backend.wake.notify_all(); return; }
                }
                if limit_retry && host.usage_limit_answerable(&outcome.result) {
                    match host.retry_live_after_usage_limit(&backend, &binding, message.clone(),
                        (input.clone(), session.clone()), text.clone(), outcome.result.clone()) {
                        Ok(()) => { backend.wake.notify_all(); return; }
                        Err(error) => eprintln!("[richos] a live input's usage-limit retry could not start: {error}"),
                    }
                }
                if let Err(error) = host.observe_turn_result(&binding, Some(&message),
                    Some(&input_id), session.as_deref().unwrap_or(""), &outcome.result) {
                    eprintln!("[richos] native input status: {error}");
                }
            }
            // Given to a turn that ran and did not end normally, and nothing sends it again.
            if !succeeded && Self::live_turn_ran(&outcome.result) { host.note_cut(&backend, session.as_deref(), input); }
            backend.wake.notify_all();
        })
    }

    /// **A worker stopped by an overload gets the CEO's waits too** (follow-up 11; his order of
    /// 2026-10-07: 1, 2, 5, 10, 20, 40 and 80 minutes). A worker belongs to Claude Code, which
    /// cannot be set to those waits, so this host waits, on the same budget, clock and Stop
    /// checks as Stu's own retry above, and then tells Stu, who restarts the worker (stu.md).
    /// All workers share ONE clock ([`Inner::workers`]): an overload during a running wait joins
    /// it, and when it ends Stu gets one `worker_overload` message per worker it stopped. Any
    /// other answer a worker gets from the API starts the schedule again.
    fn observe_worker(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding, session: &str,
        frame: &serde_json::Value) {
        if frame["type"] == "system" && frame["subtype"] == "task_started" && frame["task_type"] == "local_agent" {
            if let (Some(call), Some(name)) = (frame["tool_use_id"].as_str(), frame["description"].as_str().filter(|name| !name.is_empty())) {
                backend.inner.lock().unwrap().worker_names.insert(call.to_string(), name.to_string());
            }
            return;
        }
        if frame["type"] != "assistant" { return; }
        let call = &frame["parent_tool_use_id"];
        let mut inner = backend.inner.lock().unwrap();
        let Some(failure) = crate::native::overloaded_api_error_message(frame) else {
            if frame.get("error").is_none_or(serde_json::Value::is_null) { inner.workers.succeeded(); }
            return;
        };
        let task = call.as_str().and_then(|call| inner.worker_names.get(call).cloned()).map(serde_json::Value::from)
            .or_else(|| frame.get("task_description").cloned());
        let fact = |wait: Option<(u32, std::time::Duration)>| serde_json::json!({"event":"worker_overload",
            "worker":{"tool_use_id":call,"agent_id":frame.get("agent_id"),"task":task},
            "error":failure.raw,"retry":wait.map(|(retry, _)| retry),
            "retries_left":wait.map_or(0, |(retry, _)| crate::upstream::MAX_OVERLOAD_RETRIES - retry),
            "waited_minutes":wait.map(|(_, wait)| wait.as_secs() / 60),
            // The end is the overload granted no retry, never the seventh, whose retry still runs.
            "retries_used_up":wait.is_none()});
        if let Some(running) = inner.worker_wait.as_mut().filter(|running| running.session == session) {
            // The same overload, stopping another worker: it joins the wait, the step stays.
            if !running.facts.iter().any(|told| &told["worker"]["tool_use_id"] == call) {
                let joined = fact(Some((running.retry, running.wait)));
                running.facts.push(joined);
            }
            return;
        }
        let granted = inner.workers.charge(crate::upstream::UpstreamFault::Overloaded);
        let wait = granted.then(|| inner.workers.wait_before_retry()).flatten();
        let retry = inner.workers.retries_spent();
        let told = fact(wait.map(|wait| (retry, wait)));
        let started = match wait {
            Some(wait) => {
                inner.worker_wait = Some(WorkerWait {session: session.to_string(), retry, wait, facts: vec![told]});
                drop(inner);
                let ours = session.to_string();
                // Every worker it stopped, taken when the wait ends; nobody when it was cut short.
                self.tell_stu_after(backend, binding, session, Some(wait), move |backend, elapsed| {
                    let taken = backend.inner.lock().unwrap().worker_wait.take_if(|running| running.session == ours);
                    taken.filter(|_| elapsed).map_or_else(Vec::new, |running| running.facts)
                })
            }
            None => {
                drop(inner);
                self.tell_stu_after(backend, binding, session, None, move |_, _| vec![told])
            }
        };
        if let Err(error) = started {
            backend.inner.lock().unwrap().worker_wait.take_if(|running| running.session == session);
            eprintln!("[richos] a worker's overload wait could not start: {error}");
        }
    }

    /// Off the native reader: wait `wait` on the retry clock unless the session ends first, then
    /// put the facts `told` hands back in Stu's own inbox, one message each, and wake him on the
    /// same session. `told` is asked once either way, with whether the wait elapsed.
    fn tell_stu_after(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding, session: &str,
        wait: Option<std::time::Duration>,
        told: impl FnOnce(&Backend, bool) -> Vec<serde_json::Value> + Send + 'static) -> std::io::Result<()> {
        let (host, receiving, binding, session) = (Arc::downgrade(self), Arc::downgrade(backend), binding.clone(), session.to_string());
        std::thread::Builder::new().name("richos-overload-retry".into()).spawn(move || {
            let (Some(host), Some(backend)) = (host.upgrade(), receiving.upgrade()) else { return };
            let elapsed = wait.is_none_or(|wait| {
                let ended = || { let inner = backend.inner.lock().unwrap(); inner.closing || inner.lease_session.as_deref() != Some(session.as_str()) };
                let clock = host.retry_clock.lock().unwrap().clone();
                clock.wait(wait, &ended)
            });
            let facts = told(&backend, elapsed);
            if facts.is_empty() { return; }
            for fact in &facts {
                if let Err(error) = crate::session_tools::observe_overload_retry(&host.message_scope(&binding), fact) {
                    eprintln!("[richos] an overload could not be passed to Stu: {error}");
                    return;
                }
            }
            host.wake_messages(&binding);
        }).map(|_| ())
    }

    /// **An automatic turn of Stu's that Claude Code gave up on with an overload waits the same
    /// schedule as his other turns** (review rv-20261018T200657Z-585c24df-f9d6, finding 3).
    /// Claude Code starts these turns itself, after a helper hands back or a task notification,
    /// so no input of ours can be sent again: after the wait RichOS wakes Stu through his own
    /// inbox to carry on. When the schedule grants no retry, the failure reaches Rich as before.
    fn observe_automatic_turn(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding, session: &str,
        result: &Result<String, String>) {
        if result.as_ref().is_ok_and(|reason| reason == "end_turn") {
            self.settle_cut(backend, binding, session);
        }
        if let Some(wait) = self.overload_wait(backend, binding, result) {
            let fact = serde_json::json!({"event":"turn_overload","error":result.as_ref().err(),"waited_minutes":wait.as_secs()/60});
            match self.tell_stu_after(backend, binding, session, Some(wait), move |_, elapsed| if elapsed { vec![fact] } else { vec![] }) {
                Ok(()) => return,
                Err(error) => eprintln!("[richos] an automatic turn's overload wait could not start: {error}"),
            }
        }
        if self.usage_limit_answerable(result) {
            // The cut turn has ended, so it no longer holds his session on the refused account
            // (`session_in_use`): the recovery may move him at once.
            {
                let mut inner = backend.inner.lock().unwrap();
                if inner.lease_session.as_deref() == Some(session) { inner.automatic_turn = false; }
            }
            match self.recover_automatic_turn(backend, binding, session, result) {
                Ok(()) => return,
                Err(error) => eprintln!("[richos] an automatic turn's usage-limit recovery could not start: {error}"),
            }
        }
        if let Err(error) = self.observe_turn_result(binding, None, None, session, result) {
            eprintln!("[richos] native turn status: {error}");
        }
    }

    fn project_output(&self, binding: &ThreadBinding, session: &str) {
        let Some(store) = self.output.lock().unwrap().clone() else {
            return;
        };
        let join = crate::output::SessionJoin::from_parts(
            Vec::new(),
            assignment::read_all(
                &self.state,
                binding.entity_id().as_str(),
                binding.thread_id(),
            )
            .unwrap_or_default()
            .iter()
            .filter(|record| record.work_session.as_deref() == Some(session))
            .map(|record| crate::output::BackendWork::for_assignment(record, session))
            .collect(),
        );
        if let Err(e) = store.project_session(
            binding.thread_id(),
            &self.state.join("evidence"),
            session,
            &join,
        ) {
            eprintln!("[richos] output: the back end's written files could not be recorded: {e}");
        }
    }
    pub fn signed_in(&self) {
        for backend in self.backends.lock().unwrap().values() {
            backend.wake.notify_all();
        }
    }
    pub fn set_output_store(&self, store: crate::output::OutputStore) {
        *self.output.lock().unwrap() = Some(store);
    }
    /// The sweep run when Stu's jobs have settled: after a turn of his, once none is open.
    pub fn set_cleanup(&self, cleanup: crate::app_cleanup::Hook) {
        *self.cleanup.lock().unwrap() = Some(cleanup);
    }
    pub fn set_quota(&self, quota: Arc<crate::quota::Service>) {
        *self.quota.lock().unwrap() = Some(quota);
    }

    pub fn set_screen(&self, screen: Arc<dyn crate::screen::ScreenSource>) {
        *self.screen.lock().unwrap() = screen;
    }

    pub fn screen_reading(&self) -> crate::screen::ScreenReading {
        self.screen.lock().unwrap().read()
    }

    #[cfg(test)]
    fn set_screen_poll(&self, poll: std::time::Duration) {
        *self.screen_poll.lock().unwrap() = poll;
    }

    /// **Does this input wait for the screen?** The CEO's ruling §56: a job that declared
    /// [`Assignment::needs_screen`] and has not started, on a screen read as locked, is not
    /// handed to Stu. Its record says `WaitingForScreen`, and a [`crate::screen::ScreenWatch`]
    /// on its own thread hands it over when the screen is usable: nothing asks him to unlock.
    /// Anything else (other input, an unmarked job, a started or stopped one, an unknown or
    /// unlocked screen, a record that cannot be read) is never held.
    fn held_for_screen(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding,
        message: &crate::session_tools::Message, through: u64) -> bool {
        if !message.handoff {
            return false;
        }
        let Ok(Some(work)) = crate::session_tools::work_reference(&self.message_scope(binding), message) else {
            return false;
        };
        let id = work.assignment_id;
        if self.screen_waits.lock().unwrap().contains_key(&id) {
            return true;
        }
        let (entity, thread) = (binding.entity_id().to_string(), binding.thread_id().to_string());
        let Ok(record) = assignment::read(&self.state, &entity, &thread, &id) else { return false };
        if !record.needs_screen || !record.accepts_input()
            || !matches!(record.state, AssignmentState::Registered | AssignmentState::WaitingForScreen) {
            return false;
        }
        let source = self.screen.lock().unwrap().clone();
        let watch = crate::screen::ScreenWatch::with_poll(source, *self.screen_poll.lock().unwrap());
        if watch.read().usable() {
            // A wait that outlived a relaunch, on a screen that is back.
            if record.state == AssignmentState::WaitingForScreen {
                if let Err(error) = assignment::record_screen_wait(&self.state, &entity, &thread, &id, false) {
                    eprintln!("[richos] the screen wait could not be closed: {error}");
                }
            }
            return false;
        }
        // In the map before the record says it waits, so a Stop that sees the record reaches it.
        self.screen_waits.lock().unwrap().insert(id.clone(), watch.clone());
        if let Err(error) = assignment::record_screen_wait(&self.state, &entity, &thread, &id, true) {
            eprintln!("[richos] the screen wait could not be recorded, so the job goes ahead: {error}");
            self.screen_waits.lock().unwrap().remove(&id);
            return false;
        }
        let waiter = std::thread::Builder::new().name("richos-screen-wait".into()).spawn({
            let (host, backend, binding) = (Arc::clone(self), Arc::clone(backend), binding.clone());
            let (watch, id, entity, thread) = (watch.clone(), id.clone(), entity.clone(), thread.clone());
            move || {
                let outcome = watch.wait_quietly();
                host.screen_waits.lock().unwrap().retain(|key, held| key != &id || !Arc::ptr_eq(held, &watch));
                if !outcome.is_available() {
                    return;
                }
                if let Err(error) = assignment::record_screen_wait(&host.state, &entity, &thread, &id, false) {
                    eprintln!("[richos] the screen is back but the job's record could not say so: {error}");
                }
                backend.inner.lock().unwrap().screen_released.push(through);
                host.wake_messages(&binding);
            }
        });
        if let Err(error) = waiter {
            // Waiting is the defect here (screen.rs, the polarity rule): without a waiter the job goes ahead.
            eprintln!("[richos] the screen wait could not start, so the job goes ahead: {error}");
            self.screen_waits.lock().unwrap().remove(&id);
            if let Err(error) = assignment::record_screen_wait(&self.state, &entity, &thread, &id, false) {
                eprintln!("[richos] the job's record still says it waits for the screen: {error}");
            }
            return false;
        }
        true
    }

    /// End one job's screen wait, or every one when `id` is `None`. A Stop or a quit must
    /// reach a waiter, because §56 gives the wait no timeout.
    fn end_screen_waits(&self, id: Option<&str>) {
        let ended: Vec<_> = {
            let mut waits = self.screen_waits.lock().unwrap();
            match id {
                Some(id) => waits.remove(id).into_iter().collect(),
                None => waits.drain().map(|(_, watch)| watch).collect(),
            }
        };
        for watch in ended {
            watch.stop();
        }
    }

    /// The next input for Stu: a handoff whose screen wait just ended first (`released`, the
    /// back end's [`Inner::screen_released`]), then the inbox in order from `after`, passing
    /// over every handoff still waiting for the screen. Never locks the back end itself: the
    /// run loop calls it holding that lock.
    /// `given` is what the live session was already given in this process ([`Inner::delivered`]):
    /// passed over, never handed to it twice.
    fn next_for_stu(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding, mut after: u64,
        released: &mut Vec<u64>, given: &std::collections::HashSet<u64>) -> Result<Option<(crate::session_tools::Message, u64)>, String> {
        let scope = self.message_scope(binding);
        while let Some(through) = released.pop() {
            let message = crate::session_tools::saved_input(&scope, "stu", through)
                .inspect_err(|_| released.push(through))?;
            if !self.held_for_screen(backend, binding, &message, through) {
                return Ok(Some((message, through)));
            }
        }
        loop {
            let Some((message, through)) = crate::session_tools::pending_after(&scope, "stu", after)? else {
                return Ok(None);
            };
            if given.contains(&through) {
                after = through;
                continue;
            }
            if !self.held_for_screen(backend, binding, &message, through) {
                released.retain(|offset| *offset != through);
                return Ok(Some((message, through)));
            }
            after = through;
        }
    }

    pub fn remember_binding(self: &Arc<Self>, binding: &ThreadBinding) {
        if let Some(backend) = self.backend_for(binding.thread_id()) {
            backend.inner.lock().unwrap().binding = Some(binding.clone());
        }
    }

    pub fn wake_messages(self: &Arc<Self>, binding: &ThreadBinding) {
        let Some(backend) = self.backend_for(binding.thread_id()) else {
            return;
        };
        let _delivery = backend.live_delivery.lock().unwrap();
        let (input, after, session) = {
            let mut inner = backend.inner.lock().unwrap();
            inner.binding = Some(binding.clone());
            if inner.closing {
                return;
            }
            (
                inner.peer_input.clone(),
                inner.peer_active_through.unwrap_or(0),
                inner.lease_session.clone(),
            )
        };
        backend.wake.notify_all();
        let Some(input) = input else { return };
        let scope = self.message_scope(binding);
        let result = (|| {
            if let Some(session)=session.as_deref() {
                for answer in crate::question_work::pending(&self.state)?.into_iter().filter(|answer|
                    answer.entity_id==binding.entity_id().as_str() && answer.thread_id==binding.thread_id()) {
                    let input_id=format!("answer:{}",answer.id);
                    if crate::session_tools::previous_attempt(&scope,"stu",&input_id)?.as_ref()
                        .and_then(|attempt|attempt["session_id"].as_str())==Some(session) {continue}
                    let instruction=self.question_answer_instruction(&answer)?;
                    let message=crate::session_tools::Message {handoff:false,from:"user".into(),text:answer.text.clone(),instruction,work:None};
                    let text=serde_json::to_string(&message).map_err(|e|e.to_string())?;
                    use sha2::Digest;
                    let digest=sha2::Sha256::digest(format!("{}\0{}\0{input_id}",scope.entity_id,scope.thread_id));
                    let id=uuid::Uuid::from_slice(&digest[..16]).unwrap().to_string();
                    crate::session_tools::begin_attempt(&scope,"stu",&input_id,session)?;
                    let completed=self.live_completion(&backend,binding,message.clone(),
                        (InputReceipt::Answer(answer.id.clone()),Some(session.to_string())),text.clone());
                    match input.send_input(&id,&text,completed) {
                        Ok(true)=>{},
                        Ok(false)=>{crate::session_tools::complete_attempt(&scope,"stu",&input_id)?;return Ok(())},
                        Err(error)=>{
                            self.observe_turn_result(binding,Some(&message),Some(&input_id),session,&Err(error.clone()))?;
                            return Err(error);
                        }
                    }
                }
            }
            let mut after = after;
            loop {
            // A handoff whose screen wait ended goes to Stu now, live, even when later input has
            // already gone past it (§56; review rv-20261025T001333Z-ed81e5e8-29df, finding 1).
            // Taken out of the back end for the selection and put back unless it was delivered.
            let (mut message, through, released_now) = loop {
                let (mut released, given) = {
                    let mut inner = backend.inner.lock().unwrap();
                    (std::mem::take(&mut inner.screen_released), inner.given_to(session.as_deref()))
                };
                let taken = released.clone();
                let next = self.next_for_stu(&backend, binding, after, &mut released, &given);
                backend.inner.lock().unwrap().screen_released.extend(released);
                let Some((message, through)) = next? else {
                    return Ok::<_, String>(());
                };
                if !self.handoff_stopped(binding, &message)? {
                    backend.inner.lock().unwrap().screen_released.retain(|offset| *offset != through);
                    break (message, through, taken.contains(&through).then_some(through));
                }
                crate::session_tools::acknowledge(&scope, "stu", through)?;
                after = after.max(through);
            };
            let delivered = (|| -> Result<bool, String> {
            if message.handoff {
                message.work = crate::session_tools::work_reference(&scope, &message)?;
            }
            use sha2::Digest;
            let digest = sha2::Sha256::digest(format!(
                "{}\0{}\0{through}",
                scope.entity_id, scope.thread_id
            ));
            let id = uuid::Uuid::from_slice(&digest[..16]).unwrap().to_string();
            let mut payload = serde_json::to_value(&message).map_err(|e| e.to_string())?;
            payload["previous_delivery_attempt"] = serde_json::to_value(
                crate::session_tools::previous_attempt(&scope, "stu", &format!("peer:{through}"))?,
            )
            .map_err(|e| e.to_string())?;
            let text = payload.to_string();
            if let Some(session) = &session {
                crate::session_tools::begin_attempt(&scope,"stu",&format!("peer:{through}"),session)?;
            }
            let completed = self.live_completion_with(&backend, binding, message.clone(),
                (InputReceipt::Peer(through), session.clone()), text.clone(), !Self::usage_limit_retry_spent(&message));
            match input.send_input(&id, &text, completed) {
                Ok(delivered)=>Ok(delivered),
                Err(error)=>{
                    self.observe_turn_result(binding,Some(&message),Some(&format!("peer:{through}")),
                        session.as_deref().unwrap_or(""),&Err(error.clone()))?;
                    Err(error)
                }
            }
            })();
            // Not taken live (no turn now, or the send failed): the run loop delivers it.
            if !matches!(delivered, Ok(true)) {
                if let Some(through) = released_now {
                    backend.inner.lock().unwrap().screen_released.push(through);
                }
            }
            if !delivered? { return Ok(()); }
            {
                let mut inner = backend.inner.lock().unwrap();
                if inner.lease_session == session {
                    inner.peer_active_through=Some(inner.peer_active_through.unwrap_or(0).max(through));
                }
                if let Some(session) = session.as_deref() {
                    inner.note_given(session, through);
                }
            }
            // A released handoff sits behind input already delivered: never read back over that.
            after = after.max(through);
            }
        })();
        if let Err(error) = result {
            eprintln!("[richos] operator live input remains pending: {error}");
        }
    }

    fn observe_turn_result(&self, binding: &ThreadBinding, message: Option<&crate::session_tools::Message>,
        input: Option<&str>, session: &str, result: &Result<String,String>) -> Result<(),String> {
        if result.as_ref().is_ok_and(|reason| matches!(reason.as_str(), "end_turn" | crate::native::STOP_REASON_CANCELLED | crate::native::STOP_REASON_CANCEL_UNACKNOWLEDGED))
            || *self.closing.lock().unwrap() || match message {Some(message)=>self.handoff_stopped(binding,message)?,None=>false} { return Ok(()); }
        crate::session_tools::observe_work_status(&self.message_scope(binding), serde_json::json!({
            "event":"upstream_turn","input_id":input,"session_id":session,"work":message.and_then(|message|message.work.as_ref()),
            "stop_reason":result.as_ref().ok(),"error":result.as_ref().err()
        }))
    }

    fn publish_peer_input(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding) {
        {
            let mut inner = backend.inner.lock().unwrap();
            inner.peer_input = inner.cancel.clone();
        }
        self.wake_messages(binding);
    }

    fn message_scope(&self, binding: &ThreadBinding) -> crate::status_tools::StatusToolScope {
        crate::status_tools::StatusToolScope {
            version: 1,
            state_root: self.state.clone(),
            entity_id: binding.entity_id().to_string(),
            thread_id: binding.thread_id().to_string(),
        }
    }

    fn refresh_peer_team(
        &self,
        backend: &Arc<Backend>,
        lease: &mut dyn Cognition,
    ) -> Result<bool, String> {
        let team = self.factory.team_digest();
        if team.is_none() || team == backend.inner.lock().unwrap().lease_team {
            return Ok(false);
        }
        let changed = lease.refresh_team().map_err(|e| e.to_string())?;
        backend.inner.lock().unwrap().lease_team = team;
        Ok(changed)
    }

    /// At the end of a turn of Stu's, his own or an automatic one: a teammate he saved in it is
    /// usable only from his next turn, which nothing else would start, so a changed team is
    /// registered and he gets one turn per change, through his own inbox.
    fn reload_team_after_turn(&self, backend: &Arc<Backend>, binding: &ThreadBinding,
        lease: &mut dyn Cognition) -> Result<(), String> {
        if self.refresh_peer_team(backend, lease)? {
            if let Err(error) = crate::session_tools::observe_team_reloaded(&self.message_scope(binding)) {
                eprintln!("[richos] the team reload could not be passed to Stu: {error}");
            }
        }
        Ok(())
    }

    fn handoff_stopped(
        &self,
        binding: &ThreadBinding,
        message: &crate::session_tools::Message,
    ) -> Result<bool, String> {
        let Some(instruction) = message.instruction.as_ref().filter(|_| message.handoff) else {
            return Ok(false);
        };
        Ok(assignment::read_all(
            &self.state,
            binding.entity_id().as_str(),
            binding.thread_id(),
        )
        .map_err(|e| e.to_string())?
        .iter()
        .any(|record| {
            record.instruction_ledger_ref == instruction.ledger_ref
                && record.instruction_sha256 == instruction.sha256
                && record.user_stop_requested_at_ms.is_some()
        }))
    }

    fn run_message(
        self: &Arc<Self>,
        backend: &Arc<Backend>,
        binding: &ThreadBinding,
        message: &crate::session_tools::Message,
        turn: &str,
    ) -> Result<String, String> {
        // One delivery of `message` as a turn of Stu's. Called once, and a second time only
        // after a usage limit (below), so it is never a retry loop.
        // The session a delivery's turn ran to a result on (its input is in that transcript).
        let ran: std::cell::RefCell<Option<String>> = Default::default();
        let deliver=|| {
        // This is queue cancellation, not per-tool authority on Stu's session.
        let stopped = || self.handoff_stopped(binding, message);
        self.move_to_account_in_use(backend, binding, true);
        let _ = self.renew_for_user_settings(backend, binding);
        // **Every delivery of saved input names its earlier attempt, on this path as on the
        // live one** (P0, 2026-10-28: two of the three copies of the nightly-78 handoff Stu was
        // given after a restart came this way and carried none, so they read as fresh orders).
        // Read before `begin_attempt` below replaces it with this one; `null` on a first one.
        let mut payload = serde_json::to_value(message).map_err(|e| e.to_string())?;
        if turn.starts_with("peer:") {
            payload["previous_delivery_attempt"] = serde_json::to_value(
                crate::session_tools::previous_attempt(&self.message_scope(binding), "stu", turn)?,
            )
            .map_err(|e| e.to_string())?;
        }
        let input = payload.to_string();
        {
            self.ensure_lease(backend, binding)?;
            let mut guard = backend.lease.lock().unwrap();
            let lease = guard.as_mut().ok_or("The operator connection closed")?;
            let session = lease.session_id().to_string();
            if stopped()? {
                return Ok(session);
            }
            self.refresh_peer_team(backend, lease.as_mut())?;
            lease
                .prepare_agent_message(binding, turn, message)
                .map_err(|e| e.to_string())?;
            {
                let mut inner = backend.inner.lock().unwrap();
                inner.cancel = lease.cancel_handle();
                inner.peer_stopper = inner
                    .cancel
                    .clone()
                    .map(|handle| (lease.session_id().to_string(), handle));
                if let Some(through)=turn.strip_prefix("peer:").and_then(|n| n.parse::<u64>().ok()) {
                    inner.peer_active_through=Some(inner.peer_active_through.unwrap_or(0).max(through));
                }
            }
            crate::session_tools::begin_attempt(
                &self.message_scope(binding),
                "stu",
                turn,
                &session,
            )?;
            // Stop may have arrived during connection/scope preparation, before a
            // native stop handle existed. Never deliver that saved handoff afterward.
            if stopped()? {
                return Ok(session);
            }
            let mut published = false;
            let result = loop {
                let mut said = String::new();
                let mut sent=lease.enqueue_prompt(&input,Box::new(||Ok(()))).and_then(|reply| {
                    if !published {
                        published=true;
                        self.publish_peer_input(backend,binding);
                    }
                    reply.wait(&mut |item| {
                        if let TurnItem::Text {text,..}=item {
                            if said.len()<64*1024 {said.push_str(text);}
                        }
                    })
                });
                if let Some(failure) = crate::upstream::UpstreamFailure::classify_lines(&said)
                    .filter(|failure|failure.fault == crate::upstream::UpstreamFault::Overloaded) {
                    sent = Err(CognitionError::Protocol(failure.raw));
                }
                let observed = sent.as_ref().map(Clone::clone).map_err(ToString::to_string);
                let Some(wait) = self.overload_wait(backend, binding, &observed) else { break sent };
                let clock = self.retry_clock.lock().unwrap().clone();
                if !clock.wait(wait, &||self.retry_stopped(backend, binding, Some(message), &session)) { break sent; }
            };
            {
                let _delivery=backend.live_delivery.lock().unwrap();
                let mut inner=backend.inner.lock().unwrap();
                inner.peer_input=None;
                inner.cancel=None;
                // Input of ours sent during an automatic turn is folded into it, and that turn's
                // result is then ours: it has ended with this one.
                inner.automatic_turn=false;
                // The turn ran to a result, so this session has the input whatever that result
                // was: it is not given to it again in this process ([`Inner::delivered`]).
                if let (Ok(_), Some(through)) = (&result, turn.strip_prefix("peer:").and_then(|n| n.parse::<u64>().ok())) {
                    inner.note_given(&session, through);
                }
            }
            if matches!(&result, Err(CognitionError::Io(_))) {
                drop(guard.take());
                self.note_back_end(binding, None);
                backend.inner.lock().unwrap().peer_active_through=None;
            }
            // Not a lost connection, a Stop or input thrown away before it ran (`reason_ran`).
            if match &result { Ok(reason) => Self::reason_ran(reason), Err(error) => !matches!(error, CognitionError::Io(_)) } {
                *ran.borrow_mut() = Some(session.clone());
            }
            let reason = result.map_err(|e| e.to_string())?;
            if reason != "end_turn" {
                return Err(format!("Operator input did not complete: {reason}"));
            }
            self.settle_cut(backend, binding, &session);
            let lease = guard
                .as_mut()
                .ok_or("The operator connection closed during team reload")?;
            self.project_output(binding, &session);
            self.reload_team_after_turn(backend, binding, lease.as_mut())?;
            Ok(session)
        }
        };
        let result = deliver();
        // A TURN OF STU'S CUT BY A USAGE LIMIT is sent once more, as Rich's is (`spine.rs`,
        // "FILL-FIRST'S BACKSTOP"): at once on the next account with room, or after the hold
        // until the refusal is lifted when it cannot go there yet. The wait is the existing retry
        // clock's, with the overload retry's Stop checks, and it ends early once the refusal is
        // lifted. Whatever the second delivery comes to is final, and so is the first of a
        // message that carries on after such a hold (`usage_limit_retry_spent`).
        let retry = !Self::usage_limit_retry_spent(message);
        let result = match result.as_ref().err().filter(|_| retry).and_then(|error| self.usage_limit_hold(backend, binding, error)) {
            Some(hold) => if self.usage_limit_waited(backend, binding, Some(message), &hold) { deliver() } else { result },
            None => result,
        };
        if result.is_err() {
            if let (Some(cut), Some(input)) = (ran.borrow_mut().take(), InputReceipt::of_turn(turn)) {
                self.note_cut(backend, Some(&cut), input);
            }
            let session=backend.inner.lock().unwrap().lease_session.clone().unwrap_or_default();
            self.observe_turn_result(binding,Some(message),Some(turn),&session,&result)?;
        }
        result

    }

    pub fn adopt_registered(self: &Arc<Self>, binding: &ThreadBinding) -> usize {
        self.wake_messages(binding);
        // Desktop work is dispatched only through Stu's durable peer input.
        0
    }

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
            .name(format!(
                "richos-back-end:{}",
                &thread[..thread.len().min(24)]
            ))
            .spawn(move || host.run(runner))
            .expect("a back end could not be started");
        Some(backend)
    }

    pub fn start(self: &Arc<Self>) {
        *self.closing.lock().unwrap() = false;
    }

    pub fn deliver_team_answers(
        self: &Arc<Self>,
        binding_for: &dyn Fn(&str) -> Option<ThreadBinding>,
    ) {
        for delivery in crate::question_work::pending(&self.state).unwrap_or_default() {
            if let Some(binding) = binding_for(&delivery.thread_id) {
                drop(self.queue_question_answer(&binding, &delivery));
            }
        }
        let store = crate::questions::Store::new(&self.state);
        for (entity, thread, asker) in store.pending_threads().unwrap_or_default() {
            if asker == "front_desk" {
                continue;
            }
            let Some(binding) = binding_for(&thread) else {
                continue;
            };
            if let Err(error) = store.deliver(&entity, &thread, &asker, |d| {
                self.queue_question_answer(&binding, d)
            }) {
                eprintln!("[richos] team answer remains saved: {error}");
            }
        }
    }

    fn question_answer_instruction(
        &self,
        delivery: &crate::questions::Delivery,
    ) -> Result<Option<crate::ecs::UserInstruction>, String> {
        let Some(owner) = delivery.obligation_id.as_deref() else {
            return Ok(None);
        };
        let record = assignment::read_all(&self.state, &delivery.entity_id, &delivery.thread_id)
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|record| record.obligation_id == owner)
            .ok_or("The asking assignment is unavailable")?;
        if !record.accepts_input() {
            crate::question_work::discard(
                &self.state,
                &delivery.entity_id,
                &delivery.thread_id,
                owner,
            )?;
            return Err("The asking assignment has stopped".into());
        }
        let text = verified_instruction(
            &self.state,
            &record.entity_id,
            &record.thread_id,
            &record.instruction_ledger_ref,
            &record.instruction_sha256,
        )
        .ok()
        .filter(|text| text.len() <= crate::ecs::INSTRUCTION_TEXT_LIMIT);
        Ok(Some(crate::ecs::UserInstruction {
            ledger_ref: record.instruction_ledger_ref,
            sha256: record.instruction_sha256,
            text,
        }))
    }

    fn observe_lease(self: &Arc<Self>, backend: &Arc<Backend>, lease: &mut dyn Cognition, binding: &ThreadBinding) {
        let host = Arc::downgrade(self);
        let receiving = Arc::downgrade(backend);
        let binding = binding.clone();
        let session = lease.session_id().to_string();
        // An automatic turn (a helper's hand-back, a task notification) reaches this sink whole,
        // no input of ours being parked: its last message says whether Claude Code gave up on an
        // overload before the turn's ordinary end_turn, as a live input's sink reads it.
        let said: Mutex<Option<String>> = Mutex::new(None);
        // A refusal reported before the turn's result (a rejected `rate_limit_event`, with its
        // reset) is that turn's usage limit, as a live input's sink reads it (`native.rs`).
        let limited: Mutex<Option<Option<u64>>> = Mutex::new(None);
        lease.set_between_turn_sink(Arc::new(move |records| {
            let Some(host) = host.upgrade() else { return };
            for frame in records {
                if frame.get("parent_tool_use_id").is_some_and(|parent| !parent.is_null()) {
                    if let Some(backend) = receiving.upgrade() { host.observe_worker(&backend, &binding, &session, &frame); }
                    continue;
                }
                // A worker's start names its task, told to Stu with its overload.
                if frame["type"] == "system" && frame["subtype"] == "task_started" {
                    if let Some(backend) = receiving.upgrade() { host.observe_worker(&backend, &binding, &session, &frame); }
                    continue;
                }
                if frame["type"] == "rate_limit_event" {
                    if let Some(reset) = crate::native::usage_limit_signal(&frame) { *limited.lock().unwrap() = Some(reset); }
                    continue;
                }
                // Stu's own frames between turns of ours are an automatic turn under way; a task
                // notification starts one.
                if matches!(frame["type"].as_str(), Some("assistant" | "user" | "stream_event"))
                    || (frame["type"] == "system" && frame["subtype"] == "task_notification") {
                    if let Some(backend) = receiving.upgrade() { host.note_automatic_turn(&backend, &session, true); }
                }
                if frame["type"] == "assistant" {
                    *said.lock().unwrap() = crate::native::overloaded_api_error_message(&frame).map(|failure| failure.raw);
                    continue;
                }
                if frame["type"] != "result" { continue }
                let overload = said.lock().unwrap().take();
                let limit = limited.lock().unwrap().take();
                let result = match limit {
                    Some(reset) => Err(crate::claude_accounts::usage_limit_error(reset, &frame.to_string())),
                    None => crate::native::native_result(&frame).map_err(|error| error.to_string())
                        .and_then(|reason| match overload {
                            Some(raw) if reason == "end_turn" => Err(raw),
                            _ => Ok(reason),
                        }),
                };
                match receiving.upgrade() {
                    Some(backend) => {
                        host.observe_automatic_turn(&backend, &binding, &session, &result);
                        host.note_automatic_turn(&backend, &session, false);
                    }
                    None => if let Err(error) = host.observe_turn_result(&binding,None,None,&session,&result) {
                        eprintln!("[richos] native turn status: {error}");
                    },
                }
            }
        }));
    }

    fn ensure_lease(
        self: &Arc<Self>,
        backend: &Arc<Backend>,
        binding: &ThreadBinding,
    ) -> Result<(), String> {
        let mut lease = backend.lease.lock().unwrap();
        if lease.as_ref().is_some_and(|lease|lease.is_closed()) {drop(lease.take());self.note_back_end(binding,None);}
        if lease.is_some() {
            return Ok(());
        }
        // The team it registers, read just before it is spawned (see `Inner::lease_team`).
        let (team, opened) = {
            let factory = &self.factory;
            (factory.team_digest(), factory.spawn_work(binding))
        };
        let mut opened = opened.map_err(|e| e.to_string())?;
        self.observe_lease(backend, opened.as_mut(), binding);
        // A provider opened after shutdown started is never installed.
        let closing = {
            let mut inner = backend.inner.lock().unwrap();
            if !inner.closing {
                inner.automatic_turn = false;
                inner.lease_session = Some(opened.session_id().to_string());
                inner.peer_stopper=opened.cancel_handle().map(|handle|(opened.session_id().to_string(),handle));
                inner.lease_team = team;
            }
            inner.closing
        };
        if closing {
            drop(lease);
            drop(opened);
            return Err("RichOS is closing down. Nothing was started.".into());
        }
        let account = opened.account().map(str::to_string);
        *lease = Some(opened);
        drop(lease);
        self.note_back_end(binding, account.as_deref());
        Ok(())
    }

    /// **Where this conversation's back end runs now** (`None`: it has no live lease), for the
    /// switch line: "I switched the team" is said only once Stu is on the new account too
    /// (`claude_accounts::Accounts::back_end_on`).
    fn note_back_end(&self, binding: &ThreadBinding, account: Option<&str>) {
        if let Some(quota) = self.quota.lock().unwrap().clone() {
            quota.accounts.back_end_on(binding.thread_id(), account);
        }
    }

    /// **An automatic turn of Stu's began (`running`) or ended**, on the lease of `session`. Its
    /// end is a boundary of his work: the run loop checks there that he is on the account in use.
    /// Its start is checked too, so a switch already decided reaches him while the turn runs
    /// rather than after it (the idle check covers one decided during the turn).
    fn note_automatic_turn(&self, backend: &Backend, session: &str, running: bool) {
        let mut inner = backend.inner.lock().unwrap();
        if inner.lease_session.as_deref() != Some(session) { return; }
        let began = running && !inner.automatic_turn;
        inner.automatic_turn = running;
        if !running {
            inner.move_check = Some(true);
            backend.wake.notify_all();
        } else if began {
            inner.move_check.get_or_insert(false);
            backend.wake.notify_all();
        }
    }

    /// **Stu moves to the account in use at the next boundary of his work** (review
    /// rv-20261019T005929Z-e3d9ad03-3e7b, finding 2; nightly 65: his whole website job ran as
    /// automatic turns on Work after the switch to Home, because he was moved only before a
    /// message the app sent him). Called before every message the app sends him, when one of his
    /// automatic turns ends (`at_turn_end`: the full decision, as before every turn) and at the
    /// idle check.
    ///
    /// When nothing of his is running, his session ends and the normal opener resumes it on the
    /// account in use. When something is (a turn, a command, a helper), ending it would end that
    /// work, so he is told once (`account_switch`) and moves at a later boundary; every time he
    /// has moved, he is told so (`account_moved`) on the new session. What each
    /// message asks of him is in his instructions (`stu.md`), never here.
    fn move_to_account_in_use(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding, at_turn_end: bool) {
        let _moving = backend.moving.lock().unwrap();
        let due = if at_turn_end { self.account_switch_due(backend) } else { self.account_left_behind(backend) };
        if !due { return; }
        let outcome = self.renew_lease(backend, binding);
        let Some(quota) = self.quota.lock().unwrap().clone() else { return };
        let label = |id: &Option<String>| id.as_deref().and_then(|id| quota.accounts.list().into_iter().find(|a| a.id == id)).map(|a| a.label);
        let fact = match outcome {
            Renewal::Waits { from } => {
                // The accounts screen says the move cannot happen yet, until he moves.
                quota.accounts.move_waits(binding.thread_id());
                let to = quota.accounts.in_use();
                {
                    let mut inner = backend.inner.lock().unwrap();
                    if inner.switch_told.as_deref() == Some(to.id.as_str()) { return; }
                    inner.switch_told = Some(to.id.clone());
                }
                serde_json::json!({"event":"account_switch","from":label(&from),"to":to.label})
            }
            // Told or not, his new session has nothing else telling it to carry on (review
            // rv-20261019T030516Z-0a1a8728-e798, finding 1).
            Renewal::Moved { from, to } => {
                let carries_on = {
                    let mut inner = backend.inner.lock().unwrap();
                    inner.switch_told = None;
                    inner.automatic_recoveries > 0
                };
                let mut fact = serde_json::json!({"event":"account_moved","from":label(&from),"to":label(&to)});
                // The one message that carries a cut automatic turn on: it has had the retry
                // (`usage_limit_retry_spent`).
                if carries_on { fact["after_usage_limit"] = true.into(); }
                fact
            }
            Renewal::NotDue | Renewal::Failed => return,
        };
        if let Err(error) = crate::session_tools::observe_account_switch(&self.message_scope(binding), &fact) {
            eprintln!("[richos] the account switch could not be passed to Stu: {error}");
        }
    }

    /// **The user's own settings files changed since Stu's session read them** (build plan
    /// item 0): his session ends and the normal opener resumes it under the current settings,
    /// unless something of his still runs ([`Renewal::Waits`]), when the next message asks again.
    fn renew_for_user_settings(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding) -> Renewal {
        let changed = backend.lease.lock().unwrap().as_ref()
            .is_some_and(|lease| crate::cognition::user_settings_changed(Some(self.factory.as_ref()), lease.as_ref()));
        if !changed { return Renewal::NotDue; }
        let _moving = backend.moving.lock().unwrap();
        self.renew_lease(backend, binding)
    }

    /// The renewal for a switch, decided on the lease's own freshest reading. Kept for the
    /// tests that drive the boundary rule itself.
    #[cfg(test)]
    fn renew_if_account_changed(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding) -> Renewal {
        if !self.account_switch_due(backend) {
            return Renewal::NotDue;
        }
        self.renew_lease(backend, binding)
    }

    /// Something of Stu's still runs in `current`'s session, which ending it would end: a
    /// command, a helper, pending input or an automatic turn ([`Renewal::Waits`]).
    fn session_in_use(backend: &Backend, current: &dyn Cognition) -> bool {
        crate::cognition::account_resume_blocked(current)
            || current.worker_status().is_some_and(|workers| workers.active > 0)
            || backend.inner.lock().unwrap().automatic_turn
    }

    /// End Stu's session and resume it on the account in use, unless something of his still
    /// runs on it ([`Renewal::Waits`]).
    fn renew_lease(self: &Arc<Self>, backend: &Arc<Backend>, binding: &ThreadBinding) -> Renewal {
        // Finish the previous process before the normal opener resumes its transcript.
        // Both processes have the same native session ID and must never overlap.
        let from = {
            let mut lease = backend.lease.lock().unwrap();
            let Some(current) = lease.as_ref() else { return Renewal::NotDue };
            let from = current.account().map(str::to_string);
            if Self::session_in_use(backend, current.as_ref()) {
                return Renewal::Waits { from };
            }
            let mut inner = backend.inner.lock().unwrap();
            inner.cancel = None;
            inner.peer_input = None;
            inner.peer_stopper = None;
            inner.peer_active_through = None;
            drop(inner);
            drop(lease.take());
            from
        };
        self.note_back_end(binding, None);
        if let Err(why) = self.ensure_lease(backend, binding) {
            eprintln!("[richos] back end: not moved to the next Claude account ({why})");
            return Renewal::Failed;
        }
        let to = backend.lease.lock().unwrap().as_ref().and_then(|lease| lease.account().map(str::to_string));
        Renewal::Moved { from, to }
    }

    pub fn set_model(&self,model:crate::config::ModelChoice)->Result<(),String> {
        let backends:Vec<_>=self.backends.lock().unwrap().values().cloned().collect();
        for backend in backends {Self::set_backend_model(&backend,model)?;}
        Ok(())
    }
    fn set_backend_model(backend:&Backend,model:crate::config::ModelChoice)->Result<(),String> {
        let handle={let inner=backend.inner.lock().unwrap();inner.cancel.clone().or_else(||inner.peer_stopper.as_ref()
            .filter(|(session,_)|inner.lease_session.as_ref()==Some(session)).map(|(_,handle)|handle.clone()))};
        if let Some(handle)=handle {handle.set_model(model)?;}
        Ok(())
    }
    /// Is Stu's lease on another account than the one in use? Two ids, no decision: the idle
    /// check's question ([`ACCOUNT_CHECK_EVERY`]).
    fn account_left_behind(&self, backend: &Arc<Backend>) -> bool {
        let Some(quota)=self.quota.lock().unwrap().clone() else {return false};
        let lease=backend.lease.lock().unwrap();
        lease.as_ref().and_then(|lease|lease.account().map(str::to_string))
            .is_some_and(|account|quota.accounts.in_use().id!=account)
    }
    fn account_switch_due(&self, backend: &Arc<Backend>) -> bool {
        let Some(quota)=self.quota.lock().unwrap().clone() else {return false};
        let lease=backend.lease.lock().unwrap();
        let Some(lease)=lease.as_ref() else {return false};
        let Some(account)=lease.account() else {return false};
        quota.before_turn(account,lease.streamed_usage())!=account
    }

    pub fn rotations(&self, thread: &str) -> (u64, Option<String>) {
        match self.backend(thread) {
            None => (0, None),
            Some(backend) => {
                let inner = backend.inner.lock().unwrap();
                (inner.rotations, inner.last_rotation_reason.clone())
            }
        }
    }

    pub fn open_assignments(&self) -> Result<Vec<Assignment>, String> {
        assignment::open(&self.state).map_err(|e| e.to_string())
    }

    /// At the end of a turn of Stu's, a message's or an automatic one: when nothing is open his
    /// jobs have settled, and what the app left behind goes now (§54 addendum 2), on its own
    /// thread, so the next input never waits for it. True when nothing is open.
    ///
    /// "Settled" is [`Self::background_work`]'s answer, the same one the update gate and the
    /// quit question read: nothing running and nothing waiting for him. An open record whose
    /// work has all ended does not hold this back; the reaper proves liveness for itself.
    fn sweep_if_settled(&self) -> bool {
        let work = self.background_work();
        if !work.readable || work.running > 0 || work.awaiting_you > 0 {
            return false;
        }
        if let Some(cleanup) = self.cleanup.lock().unwrap().clone() {
            cleanup(crate::app_cleanup::Moment::Settled);
        }
        true
    }

    /// A back end is in a turn, or has saved input it has not been given. **A handoff Stu was
    /// already given is not pending input** (P0, 2026-10-28): its job stays open on its own
    /// record until his report, which [`Self::background_work`] reads, and counting it here too
    /// held the quit question and the windowless close for as long as any job record stayed open.
    pub fn peer_work_pending(&self) -> bool {
        let backends: Vec<_> = self.backends.lock().unwrap().values().cloned().collect();
        backends.into_iter().any(|backend| {
            let inner = backend.inner.lock().unwrap();
            if inner.processing || inner.automatic_turn {
                return true;
            }
            inner.binding.as_ref().is_some_and(|binding| {
                let scope = self.message_scope(binding);
                crate::session_tools::pending(&scope, "rich").map(|m| m.is_some()).unwrap_or(true)
                    || crate::session_tools::undelivered(&scope, "stu").unwrap_or(true)
            })
        })
    }

    fn backend(&self, thread: &str) -> Option<Arc<Backend>> {
        self.backends.lock().unwrap().get(thread).map(Arc::clone)
    }

    /// **The provider sessions this host holds open now**, one per back end: a lease that exists
    /// and has not reached provider EOF ([`Cognition::is_closed`]). Read off the lease itself,
    /// never off the remembered [`Inner::lease_session`], which outlives the lease: after provider
    /// EOF or shutdown it keeps naming the session until another input opens a replacement
    /// (second review rv-20261029T062537Z-21f7f26e-d081 of `21f7f26e5`: that remembered id kept
    /// "1 assignment is still running in the background." after Stu's provider exited, where a
    /// fresh host over the same disk read clear). A lease whose mutex is held is in use (a turn,
    /// an opening, a renewal) and counts by its remembered session: never blocked on, because a
    /// turn holds it for its whole length, and in use is open (fail toward waiting).
    pub fn lease_sessions(&self) -> Vec<String> {
        let backends: Vec<_> = self.backends.lock().unwrap().values().cloned().collect();
        backends
            .iter()
            .filter_map(|backend| match backend.lease.try_lock() {
                Ok(lease) => Self::open_session(&lease),
                Err(std::sync::TryLockError::Poisoned(poisoned)) => {
                    Self::open_session(&poisoned.into_inner())
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    backend.inner.lock().unwrap().lease_session.clone()
                }
            })
            .collect()
    }

    fn open_session(lease: &Option<Box<dyn Cognition>>) -> Option<String> {
        lease
            .as_ref()
            .filter(|lease| !lease.is_closed())
            .map(|lease| lease.session_id().to_string())
    }

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
    fn run(self: Arc<Self>, backend: Arc<Backend>) {
        enum Next {
            Message(ThreadBinding, crate::session_tools::Message, u64),
            Answer(ThreadBinding, crate::questions::Delivery),
            /// A boundary of Stu's work, or the idle check: is he on the account in use?
            Move(ThreadBinding, bool),
        }
        loop {
            let live_session = backend.lease.lock().unwrap().as_ref().map(|lease| lease.session_id().to_string());
            let next = {
                let mut inner = backend.inner.lock().unwrap();
                loop {
                    if inner.closing {
                        return;
                    }
                    // During an automatic turn of Stu's too (review
                    // rv-20261019T030516Z-0a1a8728-e798, finding 1): he is told there that the
                    // switch is coming, and the renewal never ends a running turn.
                    if let (Some(binding), Some(at_turn_end)) = (inner.binding.clone(), inner.move_check) {
                        inner.move_check = None;
                        inner.processing = true;
                        break Next::Move(binding, at_turn_end);
                    }
                    if let Some(binding) = inner.binding.clone() {
                        if let Some(answer) = crate::question_work::pending(&self.state)
                            .unwrap_or_default()
                            .into_iter()
                            .find(|d| {
                                d.entity_id == binding.entity_id().as_str()
                                    && d.thread_id == binding.thread_id()
                                    && live_session.as_deref().is_none_or(|session| {
                                        match crate::session_tools::previous_attempt(&self.message_scope(&binding), "stu", &format!("answer:{}", d.id)) {
                                            Ok(attempt) => attempt.as_ref().and_then(|a| a["session_id"].as_str()) != Some(session),
                                            Err(error) => { eprintln!("[richos] answer receipt remains unreadable: {error}"); false }
                                        }
                                    })
                            })
                        {
                            inner.processing = true;
                            break Next::Answer(binding, answer);
                        }
                        let after = inner.peer_active_through.unwrap_or(0);
                        // Keyed by the OPEN lease: a renewed lease resumes the same session and
                        // is passed over; after a lost connection there is no open lease, and
                        // the next one is handed unfinished input once, naming the attempt.
                        let given = inner.given_to(live_session.as_deref());
                        match self.next_for_stu(&backend, &binding, after, &mut inner.screen_released, &given) {
                            Ok(Some((message, through))) => {
                                inner.processing = true;
                                break Next::Message(binding, message, through);
                            }
                            Ok(None) => {}
                            Err(error) => eprintln!("[richos] operator inbox: {error}"),
                        }
                    }
                    let (woken, waited) = backend.wake.wait_timeout(inner, ACCOUNT_CHECK_EVERY).unwrap();
                    inner = woken;
                    if waited.timed_out() && inner.move_check.is_none() {
                        inner.move_check = Some(false);
                    }
                }
            };
            let result = match next {
                Next::Move(binding, at_turn_end) => {
                    self.move_to_account_in_use(&backend, &binding, at_turn_end);
                    // An automatic turn's end (the only `at_turn_end`) is a turn's end too
                    // (nightly 73 walk, check 5: Stu saved norm in one and was never told).
                    if at_turn_end {
                        let mut lease = backend.lease.lock().unwrap();
                        if let Some(lease) = lease.as_mut() {
                            if let Err(error) = self.reload_team_after_turn(&backend, &binding, lease.as_mut()) {
                                eprintln!("[richos] the team could not be reloaded after an automatic turn: {error}");
                            }
                        }
                        drop(lease);
                    }
                    backend.inner.lock().unwrap().processing = false;
                    // A job's last steps run as automatic turns (a teammate's hand-back), so
                    // its end is where his jobs settle too (the 2026-10-25 garbage walk). Read
                    // once this check is no longer processing, or it would count itself as work.
                    if at_turn_end {
                        self.sweep_if_settled();
                    }
                    backend.wake.notify_all();
                    continue;
                }
                Next::Message(binding, message, through) => self
                    .run_message(&backend, &binding, &message, &format!("peer:{through}"))
                    .and_then(|_| {
                        crate::session_tools::acknowledge(
                            &self.message_scope(&binding),
                            "stu",
                            through,
                        )
                    }),
                Next::Answer(binding, delivery) => self
                    .question_answer_instruction(&delivery)
                    .and_then(|instruction| {
                        let message = crate::session_tools::Message {
                            handoff: false,
                            from: "user".into(),
                            text: delivery.text.clone(),
                            instruction,
                            work: None,
                        };
                        self.run_message(
                            &backend,
                            &binding,
                            &message,
                            &format!("answer:{}", delivery.id),
                        )
                    })
                    .and_then(|session| {
                        crate::question_work::taken(
                            &self.state,
                            std::slice::from_ref(&delivery.id),
                            &session,
                        )?;
                        crate::session_tools::complete_attempt(
                            &self.message_scope(&binding),
                            "stu",
                            &format!("answer:{}", delivery.id),
                        )
                    }),
            };
            let mut inner = backend.inner.lock().unwrap();
            inner.peer_input = None;
            inner.processing = false;
            inner.completed += 1;
            // The end of this turn is a boundary of Stu's work too.
            inner.move_check.get_or_insert(false);
            backend.wake.notify_all();
            if let Err(error) = result {
                eprintln!("[richos] operator input remains saved: {error}");
                if inner.closing {
                    return;
                }
                drop(backend.wake.wait(inner).unwrap());
            } else {
                drop(inner);
                if self.sweep_if_settled() {
                    self.notifier.nothing_left_to_do();
                }
            }
        }
    }

    /// An answer is input to Stu, never an app instruction to execute a job again.
    pub fn queue_question_answer(
        self: &Arc<Self>,
        binding: &ThreadBinding,
        delivery: &crate::questions::Delivery,
    ) -> Result<String, String> {
        if delivery.entity_id != binding.entity_id().as_str()
            || delivery.thread_id != binding.thread_id()
        {
            return Err("The answer belongs to another conversation".into());
        }
        self.question_answer_instruction(delivery)?;
        crate::question_work::enqueue(&self.state, delivery)?;
        let backend = self
            .backend_for(binding.thread_id())
            .ok_or("RichOS is closing")?;
        backend.inner.lock().unwrap().binding = Some(binding.clone());
        self.wake_messages(binding);
        Ok(format!("work-input:{}", delivery.id))
    }

    /// **Is there background work, and is any of it running NOW?** The one answer the update
    /// gate, the quit question and the settled sweep all read (`lifecycle.rs`: "is there work"
    /// must not have two answers in one process).
    ///
    /// **An open record is not, by itself, running work** (P0, 2026-10-28 23:00Z: the update
    /// panel said "6 assignments still running in the background" with no worker, turn or
    /// command of any of them alive, because every open record that was not waiting for him was
    /// counted as running). A job record closes only on Stu's own report or his Stop, so one
    /// whose work all ended without that report stayed "running" for ever. Such a record stays
    /// open and is handed back to Stu at the next start (`pending_after`), so installing or
    /// quitting over it loses nothing. It counts here only while something of it is live
    /// ([`Self::live_assignments`]). Waiting for his approval is read exactly as before.
    pub fn background_work(&self) -> crate::work_gate::BackgroundWork {
        match self.open_assignments() {
            Ok(open) => {
                let (awaiting, others): (Vec<_>, Vec<_>) = open
                    .into_iter()
                    .partition(|row| row.reported_state() == Some(AssignmentState::Blocked));
                crate::work_gate::BackgroundWork {
                    running: self.live_assignments(&others),
                    awaiting_you: awaiting.len(),
                    readable: true,
                }
            }
            Err(_) => crate::work_gate::BackgroundWork {
                running: 0,
                awaiting_you: 0,
                readable: false,
            },
        }
    }

    /// **How many of these open records have work live now.** A record is live when any of
    /// these holds, and every reading that cannot be taken counts as live (fail toward waiting):
    /// - it waits for the screen or for an allowance, which end by themselves and start it;
    /// - no work of it has been witnessed yet: its handoff has not been given to a turn of
    ///   Stu's and no helper of it was dispatched. That is an assignment between its
    ///   registration and its first worker (`work_gate.rs`, §6.4), which still blocks;
    /// - one of its receipts may still have a run in flight, that session's own worker reading
    ///   is not an observed clear (`app_workers::status`), and the session itself has not
    ///   demonstrably ended ([`Self::provider_ended`]).
    ///
    /// Stu's back end is per conversation, not per job: while it is in a turn (ours or an
    /// automatic one) or has saved input it has not been given, that conversation counts at
    /// least one live assignment even when each of its records alone reads idle.
    fn live_assignments(&self, open: &[Assignment]) -> usize {
        let mut threads: std::collections::BTreeMap<(&str, &str), Vec<&Assignment>> = Default::default();
        for record in open {
            threads.entry((record.entity_id.as_str(), record.thread_id.as_str())).or_default().push(record);
        }
        let hosted = self.lease_sessions();
        let mut sessions: std::collections::HashMap<String, bool> = Default::default();
        let mut live = 0;
        for ((entity, thread), records) in threads {
            let scope = crate::status_tools::StatusToolScope {
                version: 1,
                state_root: self.state.clone(),
                entity_id: entity.to_string(),
                thread_id: thread.to_string(),
            };
            let launches = crate::work_status::launches(&self.state, entity, thread);
            let mut session_live = |session: &str| {
                // A receipt naming no session has no worker reading that could clear it.
                *sessions.entry(session.to_string()).or_insert_with(|| {
                    session.is_empty() || (crate::work_gate::workers(&crate::app_workers::status(&self.state, Some(session))).0
                        != crate::work_gate::Liveness::Clear
                        && !self.provider_ended(entity, thread, session, &hosted))
                })
            };
            let own = records.iter().filter(|record| {
                if record.reported_state().is_some_and(AssignmentState::waits_for_the_world) {
                    return true;
                }
                let Ok(launches) = launches.as_ref() else { return true };
                let launched = launches.get(&record.obligation_id).cloned().unwrap_or_default();
                let instruction = crate::ecs::UserInstruction {
                    ledger_ref: record.instruction_ledger_ref.clone(),
                    sha256: record.instruction_sha256.clone(),
                    text: None,
                };
                let given = match crate::session_tools::handoff_given(&scope, &instruction) {
                    Ok(given) => given,
                    Err(_) => return true,
                };
                if !given && !launched.dispatched {
                    return true;
                }
                launched.unresolved.iter().any(|session| session_live(session))
            }).count();
            live += own.max(usize::from(self.stu_busy(&scope)));
        }
        live
    }

    /// **This provider session has demonstrably ended, and no process of it still runs.** A
    /// provider that ends on a lost connection or a restart need not deliver its helpers'
    /// `SubagentStop` (second review rv-20261029T004310Z-6c4c304f-d238 of `6c4c304f4`: a
    /// receipt of such a session kept the update gate busy for as long as it stayed). The
    /// terminal evidence is the engine's own (`workspaces.py` `session_state`): its journal's
    /// last session row is `SessionEnd` (`app_workers::session_ended`), its canonical record
    /// carries `ended_at`, or the process recorded running it is gone or is now another
    /// process. Never ended while this host holds it as an open lease ([`Self::lease_sessions`]:
    /// the lease itself, not the session id remembered after it closed), while its recorded
    /// process still runs, or when either of those cannot be read (fail toward waiting).
    fn provider_ended(&self, entity: &str, thread: &str, session: &str, hosted: &[String]) -> bool {
        if hosted.iter().any(|live| live == session) {
            return false;
        }
        let Ok(record) = crate::work_status::session_record(&self.state, entity, thread, session) else {
            return false;
        };
        let record = record.unwrap_or_default();
        let process_gone = match &record.process {
            None => false,
            Some((pid, start)) => match crate::app_cleanup::process_start(*pid) {
                crate::app_cleanup::ProcessStart::Running(now) if now == *start => return false,
                crate::app_cleanup::ProcessStart::Running(_) | crate::app_cleanup::ProcessStart::Gone => true,
                crate::app_cleanup::ProcessStart::Unreadable => return false,
            },
        };
        process_gone || record.ended || crate::app_workers::session_ended(&self.state, session) == Some(true)
    }

    /// Stu's back end for this conversation is in a turn (ours or an automatic one), or has
    /// saved input it has not yet been given. An inbox that cannot be read counts as busy.
    fn stu_busy(&self, scope: &crate::status_tools::StatusToolScope) -> bool {
        if let Some(backend) = self.backend(&scope.thread_id) {
            let inner = backend.inner.lock().unwrap();
            if inner.processing || inner.automatic_turn {
                return true;
            }
        }
        crate::session_tools::undelivered(scope, "stu").unwrap_or(true)
    }

    /// Stop one user's job. Persist its provenance before waiting for dispatch or provider I/O.
    /// Stu remains alive to handle its other jobs and his own native background commands.
    pub fn stop_assignment(
        self: &Arc<Self>,
        entity: &str,
        thread: &str,
        id: &str,
    ) -> Result<(), String> {
        self.stop_assignment_and_front_desk(entity, thread, id, None)
    }

    /// The same Stop, also reaching the conversation's front desk (`ConversationControl::
    /// front_desk`). Reap walks walk-2bfb274d19d0 and walk-a78524f291dd: the job was handed to
    /// Stu AND Rich ran its commands himself, in the turn that gave it. While that turn still
    /// runs, the commands Rich's own model started after the job was registered are this job's.
    pub fn stop_assignment_and_front_desk(
        self: &Arc<Self>,
        entity: &str,
        thread: &str,
        id: &str,
        front_desk: Option<crate::steering::FrontDesk>,
    ) -> Result<(), String> {
        let record =
            assignment::read(&self.state, entity, thread, id).map_err(|e| e.to_string())?;
        if !record.accepts_input() {
            return Err("That assignment has already stopped.".into());
        }
        let stopped = assignment::record_user_stop(&self.state, entity, thread, id)
            .map_err(|e| e.to_string())?;
        self.end_screen_waits(Some(id));
        let stopper = self
            .backend(thread)
            .and_then(|b| b.inner.lock().unwrap().peer_stopper.clone());
        let dispatch_lock = crate::work_status::lock_receipts(&self.state, entity, thread)?;
        let targets =
            crate::work_status::trail(&self.state, entity, thread, &record.obligation_id)?
                .stop_targets;
        drop(dispatch_lock);
        // Stu's own commands for this job first, the one he waits on in the foreground among
        // them: they are his native tasks, so each ends by its own id and nothing else stops.
        let mut outcomes: Vec<_> = match &stopper {
            Some((_, handle)) => {
                let records = assignment::read_all(&self.state, entity, thread).map_err(|e| e.to_string())?;
                let commands = handle.own_commands();
                stop_own(handle.as_ref(), &commands, &job_commands(&records, id, &commands))
            }
            None => Vec::new(),
        };
        let job_turn = format!("ledger:{thread}:");
        if let Some(desk) = front_desk.as_ref().filter(|desk| desk.turn.as_ref().is_some_and(|turn|
            record.instruction_ledger_ref.strip_prefix(job_turn.as_str()) == Some(turn.turn_id.as_str()))) {
            let records = assignment::read_all(&self.state, entity, thread).map_err(|e| e.to_string())?;
            let commands = desk.handle.own_commands();
            outcomes.extend(stop_own(desk.handle.as_ref(), &commands, &desk_commands(&records, id, &commands)));
        }
        outcomes.extend(targets.iter().map(|(session, tool)| match &stopper {
            Some((owner, handle)) if owner == session => handle.stop_worker(tool),
            _ => CommandStop::NotDelivered,
        }));
        assignment::record_stop_observation(
            &self.state,
            entity,
            thread,
            id,
            stop_detail(&outcomes),
        )
        .map_err(|e| e.to_string())?;
        crate::questions::Store::new(&self.state).close(
            entity,
            thread,
            Some(&record.obligation_id),
            "The user stopped this assignment",
            false,
        )?;
        crate::question_work::discard(&self.state, entity, thread, &record.obligation_id)?;
        let scope = crate::status_tools::StatusToolScope {
            version: 1,
            state_root: self.state.clone(),
            entity_id: entity.into(),
            thread_id: thread.into(),
        };
        crate::session_tools::observe_user_stop(
            &scope,
            id,
            &record.obligation_id,
            stopped.user_stop_requested_at_ms.unwrap(),
        )?;
        if let Some(binding) = self
            .backend(thread)
            .and_then(|b| b.inner.lock().unwrap().binding.clone())
        {
            self.wake_messages(&binding);
        }
        Ok(())
    }

    /// **He chose "Quit and stop the work"** (§7.4a): every open job records his Stop, exactly
    /// as the Stop button records it ([`Self::stop_assignment_and_front_desk`]): his Stop on
    /// the record, its questions closed, its answers discarded and Stu told. This is what he
    /// did, not host inference, so it stands with 2002ec31b. The commands are not stopped here:
    /// the quit path ends every lease and its commands right after. Called only on his
    /// confirmed quit; an ordinary quit and a crash record nothing, and the job resumes.
    /// Returns how many jobs it stopped and why any could not be.
    pub fn record_quit_and_stop(&self) -> (usize, Vec<String>) {
        let scan = match assignment::scan_open(&self.state) {
            Ok(scan) => scan,
            Err(error) => return (0, vec![error.to_string()]),
        };
        let mut errors: Vec<String> = scan.unreadable.iter().map(ToString::to_string).collect();
        let mut stopped = 0;
        for record in scan.rows.iter().filter(|record| record.accepts_input()) {
            match self.record_one_quit_stop(record) {
                Ok(()) => stopped += 1,
                Err(error) => errors.push(error),
            }
        }
        (stopped, errors)
    }

    fn record_one_quit_stop(&self, record: &Assignment) -> Result<(), String> {
        let (entity, thread) = (record.entity_id.as_str(), record.thread_id.as_str());
        let stopped = assignment::record_user_stop(&self.state, entity, thread, &record.id)
            .map_err(|e| e.to_string())?;
        crate::questions::Store::new(&self.state).close(
            entity,
            thread,
            Some(&record.obligation_id),
            "The user stopped this assignment",
            false,
        )?;
        crate::question_work::discard(&self.state, entity, thread, &record.obligation_id)?;
        let scope = crate::status_tools::StatusToolScope {
            version: 1,
            state_root: self.state.clone(),
            entity_id: entity.into(),
            thread_id: thread.into(),
        };
        crate::session_tools::observe_user_stop(
            &scope,
            &record.id,
            &record.obligation_id,
            stopped.user_stop_requested_at_ms.unwrap(),
        )
    }

    /// Quit ends provider connections. Job reports, questions and inbox receipts remain unchanged.
    pub fn shutdown(&self) {
        *self.closing.lock().unwrap() = true;
        // A job waiting for the screen keeps its record; the next launch looks again.
        self.end_screen_waits(None);
        let backends: Vec<_> = self.backends.lock().unwrap().values().cloned().collect();
        for backend in &backends {
            let cancel = {
                let mut inner = backend.inner.lock().unwrap();
                inner.closing = true;
                inner.cancel.clone().or_else(|| {
                    inner
                        .peer_stopper
                        .as_ref()
                        .map(|(_, handle)| handle.clone())
                })
            };
            backend.wake.notify_all();
            if let Some(cancel) = cancel {
                cancel.shutdown();
            }
        }
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
            drop(inner);
            let Some(mut lease) = lock_until(&backend.lease, deadline) else {
                continue;
            };
            let retired = lease.take();
            drop(lease);
            drop(retired);
        }
    }
}

/// What his conversation shows while a turn of Stu's that a usage limit cut is held until the
/// reset ([`WorkHost::usage_limit_hold`]), `wait` from now.
fn usage_limit_hold_sentence(wait: std::time::Duration) -> String {
    let when = match wait.as_secs().div_ceil(60).max(1) {
        1 => "1 minute".to_string(),
        minutes if minutes < 90 => format!("{minutes} minutes"),
        minutes => format!("about {} hours", (minutes + 30) / 60),
    };
    format!("The team's work reached a Claude usage limit and no Claude account has room left. \
        RichOS will try that step again once, when the limit resets in {when}.")
}

fn lock_until<T>(
    lock: &Mutex<T>,
    deadline: std::time::Instant,
) -> Option<std::sync::MutexGuard<'_, T>> {
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

pub(crate) fn verified_instruction(
    state: &Path,
    entity: &str,
    thread: &str,
    ledger_ref: &str,
    sha256: &str,
) -> Result<String, &'static str> {
    use sha2::{Digest, Sha256};
    use std::io::BufRead;
    const UNAVAILABLE: &str = "The original request could not be verified. Nothing was started.";
    let prefix = format!("ledger:{thread}:");
    let turn = ledger_ref
        .strip_prefix(&prefix)
        .filter(|turn| !turn.is_empty())
        .ok_or(UNAVAILABLE)?;
    let path = state
        .parent()
        .ok_or(UNAVAILABLE)?
        .join("conversation-ledger.jsonl");
    let file = std::fs::File::open(path).map_err(|_| UNAVAILABLE)?;
    let mut reader = std::io::BufReader::new(file);
    let mut line = String::new();
    let mut found = None;
    loop {
        line.clear();
        if reader.read_line(&mut line).map_err(|_| UNAVAILABLE)? == 0 {
            break;
        }
        // A concurrent append may not have completed its last record yet.
        if !line.ends_with('\n') {
            break;
        }
        let Ok(row) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if row["event"] != "PromptReceived" || row["turn_id"] != turn {
            continue;
        }
        let text = row["text"].as_str().ok_or(UNAVAILABLE)?;
        if found.is_some()
            || row["thread_id"] != thread
            || row["entity_id"] != entity
            || !matches!(row["source"].as_str(), Some("text" | "jam"))
            || format!("{:x}", Sha256::digest(text.as_bytes())) != sha256
        {
            return Err(UNAVAILABLE);
        }
        found = Some(text.to_owned());
    }
    found.ok_or(UNAVAILABLE)
}

pub const STARTED_DETAIL: &str = "Rich has started on it.";
pub struct SilentNotifier;
impl WorkNotifier for SilentNotifier {
    fn raised(&self, _: &str, _: &PendingNotice) {}
}
pub fn for_him(row: &Assignment) -> bool {
    row.waiting_sentence || row.state.awaits_his_word()
}

/// How long a Stop waits for the task of a call whose `task_started` has not come yet: a
/// foreground one's arrives seconds after the call (repository recordings: about 3 s, review
/// rv-20261022T123741Z-7987beff-17a0 finding 2). load-bound: hang guard only; the call's own
/// task or its result ends the wait, and the deadline yields Unconfirmed, never Ended.
const PENDING_TASK_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// Stop the selected commands (by [`crate::steering::OwnCommand::key`]) by their own task ids.
/// A call still waiting for its task is waited for: its task is stopped when it comes, and a
/// call answered with no task had nothing left running.
fn stop_own(handle: &dyn TurnCancel, commands: &[crate::steering::OwnCommand], keys: &[String]) -> Vec<CommandStop> {
    let mut outcomes = Vec::new();
    for command in commands.iter().filter(|c| keys.iter().any(|k| k == c.key())) {
        if !command.task_id.is_empty() {
            outcomes.push(handle.stop_background_command(&command.task_id));
            continue;
        }
        let deadline = std::time::Instant::now() + PENDING_TASK_WAIT;
        loop {
            let now = handle.own_commands();
            match now.iter().find(|c| c.tool_use_id == command.tool_use_id) {
                Some(c) if !c.task_id.is_empty() => { outcomes.push(handle.stop_background_command(&c.task_id)); break; }
                None => break,
                Some(_) if std::time::Instant::now() >= deadline => { outcomes.push(CommandStop::Unconfirmed); break; }
                Some(_) => std::thread::sleep(std::time::Duration::from_millis(20)),
            }
        }
    }
    outcomes
}

/// Where the command's description first names this job's obligation_id: the whole id, never
/// a part of a longer one ("obligation-7" is not in "obligation-70").
fn first_mention(command: &crate::steering::OwnCommand, record: &assignment::Assignment) -> Option<usize> {
    let (text, wanted) = (command.description.as_str(), record.obligation_id.as_str());
    let part_of_id = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if wanted.is_empty() {
        return None;
    }
    text.match_indices(wanted).map(|(at, _)| at).find(|&at| {
        !part_of_id(text[..at].chars().next_back()) && !part_of_id(text[at + wanted.len()..].chars().next())
    })
}

/// The job whose obligation_id the command's description names FIRST, the one rich.md and
/// stu.md tell the model to begin it with; a job it mentions later ("check A's output") is
/// not its owner (review rv-20261022T155431Z-bc0b2065-3075#1). None when it names no job.
fn owner<'a>(command: &crate::steering::OwnCommand, records: &'a [assignment::Assignment]) -> Option<&'a assignment::Assignment> {
    records.iter().filter_map(|r| first_mention(command, r).map(|at| (at, r))).min_by_key(|(at, _)| *at).map(|(_, r)| r)
}

/// Is this job the command's owner (see [`owner`])?
fn names_job(command: &crate::steering::OwnCommand, records: &[assignment::Assignment], job: &assignment::Assignment) -> bool {
    owner(command, records).is_some_and(|r| r.obligation_id == job.obligation_id)
}

/// Does the command name another job than this one first?
fn names_other(command: &crate::steering::OwnCommand, records: &[assignment::Assignment], job: &assignment::Assignment) -> bool {
    owner(command, records).is_some_and(|r| r.obligation_id != job.obligation_id)
}

/// Which of Rich's own running commands are job `id`'s, while he runs the turn that gave it:
/// those started after it was registered, unless one names another job, or names none while
/// another job came from the same turn (`assignment_tools` records several from one turn).
fn desk_commands(records: &[assignment::Assignment], id: &str, commands: &[crate::steering::OwnCommand]) -> Vec<String> {
    let Some(job) = records.iter().find(|r| r.id == id) else { return Vec::new() };
    let others: Vec<_> = records.iter().filter(|r| r.id != id).collect();
    let same_turn = |at: u64| others.iter().any(|r| r.instruction_ledger_ref == job.instruction_ledger_ref && r.registered_at_ms <= at);
    commands
        .iter()
        .filter(|c| c.started_at_ms >= job.registered_at_ms)
        .filter(|c| names_job(c, records, job)
            || (!names_other(c, records, job) && !same_turn(c.started_at_ms)))
        .map(|c| c.key().to_string())
        .collect()
}

/// Which of Stu's own running commands are job `id`'s. **The job travels with the command:**
/// Stu names the job's `obligation_id` in the description of every command he runs for it
/// (stu.md), so a command naming this job is its, and one naming another job is never its.
/// A command that names no job is this job's only when it started after the job was
/// registered and no other job could be running it: another job counts when it is still
/// open, was registered before the command started, and was not already waiting (on the user,
/// the screen or the allowance) with no change since. Anything else is left to Stu, who is
/// told of the Stop, rather than guessed.
fn job_commands(records: &[assignment::Assignment], id: &str, commands: &[crate::steering::OwnCommand]) -> Vec<String> {
    use AssignmentState::{Blocked, WaitingForQuota, WaitingForScreen};
    let Some(job) = records.iter().find(|r| r.id == id) else { return Vec::new() };
    let another_could_run = |at: u64| {
        records.iter().any(|r| {
            let waiting_since_before = matches!(r.reported_state(), Some(Blocked | WaitingForScreen | WaitingForQuota))
                && r.updated_at_ms <= at;
            r.id != id && r.accepts_input() && r.registered_at_ms <= at && !waiting_since_before
        })
    };
    commands
        .iter()
        .filter(|command| {
            if names_job(command, records, job) {
                return true;
            }
            !names_other(command, records, job)
                && command.started_at_ms >= job.registered_at_ms
                && !another_could_run(command.started_at_ms)
        })
        .map(|command| command.key().to_string())
        .collect()
}

fn stop_detail(outcomes: &[CommandStop]) -> &'static str {
    if outcomes.is_empty() {
        "Stop requested by the user. Stu has been notified."
    } else if outcomes
        .iter()
        .any(|outcome| matches!(outcome, CommandStop::Refused(_)))
    {
        "Stop requested. A command refused to stop and is still running."
    } else if outcomes.contains(&CommandStop::NotDelivered) {
        "Stop requested. A command could not be reached."
    } else if outcomes.contains(&CommandStop::Unconfirmed) {
        "Stop requested. A command's ending has not been confirmed."
    } else {
        "The job's recorded work stopped. Stu has been notified."
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assignment::Registration;
    use crate::cognition::WorkAssignment;
    use crate::entity::{EntityId, PersonId};
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct StartGate(Mutex<bool>, Condvar, std::sync::atomic::AtomicBool);
    impl StartGate {
        fn open_now() -> Arc<Self> {
            Arc::new(Self(
                Mutex::new(true),
                Condvar::new(),
                std::sync::atomic::AtomicBool::new(false),
            ))
        }
        fn shut(&self) {
            self.2.store(false, Ordering::SeqCst);
            *self.0.lock().unwrap() = false;
        }
        fn release(&self) {
            *self.0.lock().unwrap() = true;
            self.1.notify_all();
        }
        fn wait_until_open(&self) {
            let mut open = self.0.lock().unwrap();
            while !*open {
                self.2.store(true, Ordering::SeqCst);
                open = self.1.wait(open).unwrap();
            }
        }
    }
    #[derive(Default)]
    struct TeamScript {
        digest: Option<String>,
        /// The user's settings digest the factory reports now; each lease keeps the one it
        /// was spawned under.
        settings: Option<String>,
        in_turn: VecDeque<Option<String>>,
        turn_results: VecDeque<&'static str>,
        /// Every lease opens the first session again, as `claude --resume` does in the app: a
        /// renewed or restarted Stu is the same provider session (`renew_lease`).
        resume_same_session: bool,
        /// Every lease reports provider EOF ([`Cognition::is_closed`]), as Stu's does once his
        /// provider exits on a lost connection.
        closed: bool,
    }
    struct Lease {
        session: String,
        prompts: Arc<Mutex<Vec<String>>>,
        commands: Arc<Mutex<Option<crate::lease_commands::CommandReading>>>,
        team: Arc<Mutex<TeamScript>>,
        settings: Option<String>,
    }
    impl Cognition for Lease {
        fn is_closed(&self) -> bool {
            self.team.lock().unwrap().closed
        }
        fn session_id(&self) -> &str {
            &self.session
        }
        fn user_settings_digest(&self) -> Option<String> {
            self.settings.clone()
        }
        fn refresh_team(&mut self) -> Result<bool, CognitionError> {
            Ok(true)
        }
        fn running_commands(&self) -> Option<crate::lease_commands::CommandReading> {
            *self.commands.lock().unwrap()
        }
        fn bind_work_assignment(&mut self, _: &WorkAssignment) -> Result<(), CognitionError> {
            panic!("Stu is never rebound to a job")
        }
        fn prompt(
            &mut self,
            text: &str,
            on: &mut dyn FnMut(TurnItem),
        ) -> Result<String, CognitionError> {
            self.prompts.lock().unwrap().push(text.into());
            let mut team = self.team.lock().unwrap();
            if let Some(digest) = team.in_turn.pop_front() {
                team.digest = digest;
            }
            let result=team.turn_results.pop_front().unwrap_or("end_turn");
            drop(team);
            if result=="retry_progress" {
                for record in crate::machinery::MachineryRecord::from_native_event(&serde_json::json!({"type":"system","subtype":"api_retry","attempt":1,"retry_delay_ms":1000,"error_status":529}),&self.session,0) {on(TurnItem::Machinery(record));}
                return Ok("end_turn".into());
            }
            if result=="native_quota" {return Err(CognitionError::Protocol(crate::claude_accounts::usage_limit_error(None,"native quota refusal")))}
            if result=="max_tokens_no_text" {return Ok("max_tokens".into())}
            if let Some(status)=result.strip_prefix("pre_model_") {
                if status=="protocol" {return Err(CognitionError::Protocol("API Error: 503 Server error.".into()))}
                let text=match status {"529"=>"API Error: 529 Overloaded.","429"=>"API Error: 429 Rate limited.",_=>"API Error: 503 Server error."};
                return Err(CognitionError::Protocol(text.into()));
            }
            if result=="tool_error" {
                for record in crate::machinery::MachineryRecord::from_native_event(&serde_json::json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"tool-one","name":"Bash","input":{"command":"fictional"}}]}}),&self.session,0) {on(TurnItem::Machinery(record));}
                return Err(CognitionError::Protocol("API Error: 503 Server error.".into()));
            }
            on(TurnItem::Text {
                seq: 0,
                text: "Recorded reply",
            });
            match result {
                "provider_error"=>Err(CognitionError::Protocol("API Error: 529 Overloaded.".into())),
                "io_error"=>Err(CognitionError::Io("connection ended".into())),
                reason=>Ok(reason.into()),
            }
        }
    }
    struct Factory {
        spawns: Arc<AtomicUsize>,
        prompts: Arc<Mutex<Vec<String>>>,
        commands: Arc<Mutex<Option<crate::lease_commands::CommandReading>>>,
        team: Arc<Mutex<TeamScript>>,
        gate: Arc<StartGate>,
    }
    impl LeaseFactory for Factory {
        fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
            panic!("Stu has an independent scoped session")
        }
        fn team_digest(&self) -> Option<String> {
            self.team.lock().unwrap().digest.clone()
        }
        fn user_settings_digest(&self) -> Option<String> {
            self.team.lock().unwrap().settings.clone()
        }
        fn spawn_work(&self, _: &ThreadBinding) -> Result<Box<dyn Cognition>, CognitionError> {
            self.gate.wait_until_open();
            let n = self.spawns.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(Lease {
                session: if n == 0 || self.team.lock().unwrap().resume_same_session {
                    "work-session-one".into()
                } else {
                    format!("work-session-rotated-{n}")
                },
                prompts: self.prompts.clone(),
                commands: self.commands.clone(),
                settings: self.team.lock().unwrap().settings.clone(),
                team: self.team.clone(),
            }))
        }
    }
    #[derive(Default)]
    struct RecordedRetryClock(Mutex<Vec<std::time::Duration>>);
    impl crate::upstream::RetryClock for RecordedRetryClock {
        fn wait(&self, wait: std::time::Duration, stop: &dyn Fn() -> bool) -> bool {
            self.0.lock().unwrap().push(wait);
            !stop()
        }
    }
    struct Harness {
        root: PathBuf,
        state: PathBuf,
        host: Arc<WorkHost>,
        binding: ThreadBinding,
        bound: Arc<Mutex<Vec<WorkAssignment>>>,
        spawns: Arc<AtomicUsize>,
        work_prompts: Arc<Mutex<Vec<String>>>,
        commands: Arc<Mutex<Option<crate::lease_commands::CommandReading>>>,
        team: Arc<Mutex<TeamScript>>,
        start_gate: Arc<StartGate>,
        notices: Arc<RecordingNotifier>,
    }
    /// The overload lines the host shows, in order, with the conversation each was for.
    #[derive(Default)]
    struct RecordingNotifier(Mutex<Vec<(String, String)>>);
    impl WorkNotifier for RecordingNotifier {
        fn raised(&self, _: &str, _: &PendingNotice) {}
        fn overload_retry(&self, thread_id: &str, sentence: &str) {
            self.0.lock().unwrap().push((thread_id.to_string(), sentence.to_string()));
        }
    }
    fn harness(_: u64) -> Harness {
        let root = std::env::temp_dir().join(format!("richos-peer-host-{}", uuid::Uuid::new_v4()));
        let state = root.join("engine-state");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(root.join("conversation-ledger.jsonl"),serde_json::json!({"event":"PromptReceived",
            "entity_id":"depot","thread_id":"thread-one","turn_id":"turn-7","source":"text","text":"landing the three branches"}).to_string()+"\n").unwrap();
        let binding = ThreadBinding::new(
            PersonId::default_ceo(),
            EntityId::parse("depot").unwrap(),
            "thread-one",
            1,
        );
        let spawns = Arc::new(AtomicUsize::new(0));
        let work_prompts = Arc::new(Mutex::new(vec![]));
        let commands = Arc::new(Mutex::new(None));
        let team = Arc::new(Mutex::new(TeamScript::default()));
        let start_gate = StartGate::open_now();
        let factory = Factory {
            spawns: spawns.clone(),
            prompts: work_prompts.clone(),
            commands: commands.clone(),
            team: team.clone(),
            gate: start_gate.clone(),
        };
        let notices = Arc::new(RecordingNotifier::default());
        let host = WorkHost::new(
            &state,
            Box::new(factory),
            notices.clone(),

        );
        host.set_retry_clock(Arc::new(RecordedRetryClock::default()));

        Harness {
            root,
            state,
            host,
            binding,
            bound: Arc::new(Mutex::new(vec![])),
            spawns,
            work_prompts,
            commands,
            team,
            start_gate,
            notices,
        }
    }
    fn every_worker_observed_ending(h: &Harness) {
        let folder = h.state.join("evidence/work-session-one");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("callbacks.jsonl"), "").unwrap();
        std::fs::write(folder.join(".lock"), "").unwrap();
    }

    fn registration(harness: &Harness) -> Registration {
        Registration {
            entity_id: harness.binding.entity_id().to_string(),
            thread_id: harness.binding.thread_id().to_string(),
            obligation_id: "obligation-7".into(),
            instruction_ledger_ref: "ledger:thread-one:turn-7".into(),
            instruction_sha256: {
                use sha2::Digest;
                format!("{:x}", sha2::Sha256::digest(b"landing the three branches"))
            },
            title: "landing the three branches".into(),
            repositories: vec!["/fictional/project".into()],
            needs_screen: false,
        }
    }

    fn until(what: &str, done: impl Fn() -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        while !done() {
            assert!(std::time::Instant::now() < deadline, "{what}");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// §56: a job of his on turn `turn`, registered (`needs_screen` as given) and handed to Stu
    /// by Rich, exactly as a conversation does it. Returns the record and the message scope.
    fn screen_job(h: &Harness, turn: &str, text: &str, needs_screen: bool) -> (Assignment, crate::status_tools::StatusToolScope) {
        use sha2::Digest;
        use std::io::Write;
        let sha = format!("{:x}", sha2::Sha256::digest(text.as_bytes()));
        let ledger = h.root.join("conversation-ledger.jsonl");
        if !std::fs::read_to_string(&ledger).unwrap().contains(&format!("\"{turn}\"")) {
            let line = serde_json::json!({"event":"PromptReceived","entity_id":"depot","thread_id":"thread-one",
                "turn_id":turn,"source":"text","text":text}).to_string() + "\n";
            std::fs::OpenOptions::new().append(true).open(&ledger).unwrap().write_all(line.as_bytes()).unwrap();
        }
        let receipt = assignment::register(&h.state, &Registration {
            obligation_id: format!("obligation-{turn}"),
            instruction_ledger_ref: format!("ledger:thread-one:{turn}"),
            instruction_sha256: sha.clone(),
            title: text.into(),
            needs_screen,
            ..registration(h)
        }).unwrap();
        let scope = h.host.message_scope(&h.binding);
        let path = h.state.join(format!("rich-status-{turn}.json"));
        let instruction = crate::ecs::UserInstruction::with_text(format!("ledger:thread-one:{turn}"), sha, Some(text));
        crate::session_tools::bind(&path, &scope, "native-rich", "rich", Some(instruction)).unwrap();
        crate::session_tools::delegate(&path, &scope, serde_json::json!({"text":format!("Handoff: {text}")})).unwrap();
        (assignment::read(&h.state, "depot", "thread-one", &receipt.id).unwrap(), scope)
    }

    fn job(h: &Harness, id: &str) -> Assignment {
        assignment::read(&h.state, "depot", "thread-one", id).unwrap()
    }

    fn handed_over(h: &Harness, text: &str) -> usize {
        h.work_prompts.lock().unwrap().iter().filter(|prompt| prompt.contains(text)).count()
    }

    /// **THE CEO'S §56, END TO END.** A job marked `needs_screen`, on a locked screen, is not
    /// handed to Stu: it says one plain line where he and Rich look, asks nobody for anything,
    /// and goes to Stu by itself the moment the screen is back.
    #[test]
    fn a_job_that_needs_the_screen_waits_while_it_is_locked_and_carries_on_at_unlock() {
        use crate::screen::{FakeScreen, ScreenReading};
        let h = harness(0);
        let screen = FakeScreen::locked();
        h.host.set_screen(screen.clone());
        h.host.set_screen_poll(std::time::Duration::from_millis(2));
        let (record, scope) = screen_job(&h, "turn-7", "landing the three branches", true);
        h.host.wake_messages(&h.binding);
        // load-bound: hang guards only; the verdicts are the record, the prompts and the reads.
        until("the job never went to waiting for the screen", || job(&h, &record.id).state == AssignmentState::WaitingForScreen);
        until("the wait never polled the screen", || screen.reads() >= 5);
        assert_eq!(handed_over(&h, "Handoff: landing"), 0, "a locked screen let the job through to Stu");
        assert_eq!(job(&h, &record.id).detail, crate::screen::says::detail());
        // The status read: one row under its own heading, with the one plain line.
        let status = h.state.join("status-look.json");
        crate::status_tools::write_scope(&status, &scope).unwrap();
        let look = crate::status_tools::call(&status, screen.as_ref(), crate::status_tools::LOOK_TOOL_NAME, serde_json::json!({})).unwrap();
        let rows = look["waiting_for_the_screen"].as_array().unwrap();
        assert_eq!(rows.len(), 1, "{look}");
        assert_eq!(rows[0]["detail"], crate::screen::says::detail());
        assert!(look["starting"].as_array().unwrap().is_empty() && look["waiting_for_you"].as_array().unwrap().is_empty(), "{look}");
        // Nothing asks him to unlock: no notice, nothing for Rich to pass on.
        assert!(job(&h, &record.id).notices.is_empty());
        assert!(crate::session_tools::pending(&scope, "rich").unwrap().is_none(), "the wait sent Rich something to say");
        screen.set(ScreenReading::unlocked());
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)), "hang guard: the job never carried on");
        assert_eq!(handed_over(&h, "Handoff: landing"), 1, "the job must go to Stu once, by itself");
        let resumed = job(&h, &record.id);
        assert_eq!(resumed.state, AssignmentState::Registered, "a resumed job must not keep claiming it is waiting");
        assert_eq!(resumed.detail, crate::screen::says::carrying_on());
        assert!(h.host.screen_waits.lock().unwrap().is_empty());
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// §56 is only for work that said it needs the screen. An unmarked job on a locked screen
    /// goes straight to Stu without the screen being read at all, and so does one given while
    /// a marked job is still waiting: nothing queues behind the wait.
    #[test]
    fn a_job_not_marked_never_waits_for_the_screen() {
        use crate::screen::{FakeScreen, ScreenReading};
        let h = harness(0);
        let screen = FakeScreen::locked();
        h.host.set_screen(screen.clone());
        h.host.set_screen_poll(std::time::Duration::from_millis(2));
        let (plain, _) = screen_job(&h, "turn-7", "landing the three branches", false);
        h.host.wake_messages(&h.binding);
        // load-bound: hang guards only; the verdicts are the prompts, the records and the reads.
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)), "hang guard: the unmarked job waited");
        assert_eq!(handed_over(&h, "Handoff: landing"), 1);
        assert_eq!(job(&h, &plain.id).state, AssignmentState::Registered);
        assert_eq!(screen.reads(), 0, "an unmarked job must not even look at the screen");
        let (marked, _) = screen_job(&h, "turn-8", "walking the build on screen", true);
        h.host.wake_messages(&h.binding);
        until("the marked job never waited", || job(&h, &marked.id).state == AssignmentState::WaitingForScreen);
        let (later, _) = screen_job(&h, "turn-9", "fixing the readme typo", false);
        h.host.wake_messages(&h.binding);
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(10)), "hang guard: the unmarked job queued behind the wait");
        assert_eq!(handed_over(&h, "Handoff: fixing"), 1);
        assert_eq!(handed_over(&h, "Handoff: walking"), 0);
        assert_eq!(job(&h, &later.id).state, AssignmentState::Registered);
        // The waiting job, released after later input went past it, still goes to Stu once.
        screen.set(ScreenReading::unlocked());
        assert!(h.host.wait_for_completed(3, std::time::Duration::from_secs(10)), "hang guard: the marked job never carried on");
        assert_eq!(handed_over(&h, "Handoff: walking"), 1);
        assert_eq!(h.work_prompts.lock().unwrap().len(), 3, "{:?}", h.work_prompts.lock().unwrap());
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **The unlock reaches Stu while his turn runs** (review rv-20261025T001333Z-ed81e5e8-29df,
    /// finding 1). A marked job waits; an unmarked one goes past it to Stu's live input, so the
    /// live frontier is beyond the marked handoff. At unlock the marked handoff must go to that
    /// same live input at once, not wait for his turn to end.
    #[test]
    fn at_unlock_a_passed_handoff_goes_to_stus_live_turn_at_once() {
        use crate::screen::{FakeScreen, ScreenReading};
        struct LiveInput(Mutex<Vec<String>>);
        impl TurnCancel for LiveInput {
            fn cancel(&self) -> bool { true }
            fn send_input(&self, _: &str, text: &str, _: Box<dyn FnOnce(crate::steering::InputOutcome) + Send>) -> Result<bool, String> {
                self.0.lock().unwrap().push(text.to_string());
                Ok(true)
            }
        }
        let h = harness(0);
        let screen = FakeScreen::locked();
        h.host.set_screen(screen.clone());
        h.host.set_screen_poll(std::time::Duration::from_millis(2));
        // Stu is in a turn: his live input is published, and no run loop is between turns.
        let live = Arc::new(LiveInput(Mutex::new(Vec::new())));
        let backend = Backend::new();
        {
            let mut inner = backend.inner.lock().unwrap();
            inner.binding = Some(h.binding.clone());
            inner.lease_session = Some("live-stu".into());
            inner.peer_input = Some(live.clone());
        }
        h.host.backends.lock().unwrap().insert("thread-one".into(), backend.clone());
        let given = |text: &str| live.0.lock().unwrap().iter().filter(|sent| sent.contains(text)).count();
        let (marked, _) = screen_job(&h, "turn-8", "walking the build on screen", true);
        h.host.wake_messages(&h.binding);
        // load-bound: hang guards only; the verdicts are what reached the live input.
        until("the marked job never waited", || job(&h, &marked.id).state == AssignmentState::WaitingForScreen);
        screen_job(&h, "turn-9", "fixing the readme typo", false);
        h.host.wake_messages(&h.binding);
        assert_eq!(given("Handoff: fixing"), 1, "the unmarked job must go to the live turn");
        assert_eq!(given("Handoff: walking"), 0, "a locked screen let the marked job through");
        screen.set(ScreenReading::unlocked());
        until("the released handoff waited behind the live turn", || given("Handoff: walking") == 1);
        assert!(backend.inner.lock().unwrap().screen_released.is_empty(), "the handoff is still queued for later");
        h.host.wake_messages(&h.binding);
        assert_eq!(given("Handoff: walking"), 1, "the released handoff was given twice");
        backend.inner.lock().unwrap().closing = true;
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// §56 gives the wait no timeout, so his Stop must reach it: the waiter ends at once, the
    /// job is stopped, and the screen coming back afterwards hands nothing to Stu.
    #[test]
    fn stopping_a_job_that_waits_for_the_screen_ends_the_wait() {
        use crate::screen::{FakeScreen, ScreenReading};
        let h = harness(0);
        let screen = FakeScreen::locked();
        h.host.set_screen(screen.clone());
        h.host.set_screen_poll(std::time::Duration::from_secs(3600));
        let (record, _) = screen_job(&h, "turn-7", "landing the three branches", true);
        h.host.wake_messages(&h.binding);
        // load-bound: hang guards only; the verdicts are the watch, the record and the prompts.
        until("the job never went to waiting for the screen", || job(&h, &record.id).state == AssignmentState::WaitingForScreen);
        let watch = h.host.screen_waits.lock().unwrap().get(&record.id).cloned().unwrap();
        h.host.stop_assignment("depot", "thread-one", &record.id).unwrap();
        assert!(watch.is_stopped(), "the Stop did not reach the wait");
        // An hour-long poll: only the Stop can have ended the waiter's thread this soon.
        until("the waiter's thread never ended", || Arc::strong_count(&watch) == 1);
        assert!(h.host.screen_waits.lock().unwrap().is_empty());
        assert_eq!(job(&h, &record.id).reported_state(), Some(AssignmentState::Interrupted));
        screen.set(ScreenReading::unlocked());
        h.host.wake_messages(&h.binding);
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)), "hang guard: Stu was not told of the Stop");
        assert_eq!(handed_over(&h, "Handoff: landing"), 0, "a stopped job reached Stu when the screen came back");
        assert_eq!(handed_over(&h, "user_stop"), 1);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    #[test]
    fn a_closed_stu_provider_reopens_at_the_next_normal_input_boundary() {
        struct Closed;
        impl Cognition for Closed {
            fn session_id(&self)->&str {"ended-stu-session"}
            fn is_closed(&self)->bool {true}
            fn prompt(&mut self,_:&str,_:&mut dyn FnMut(TurnItem))->Result<String,CognitionError> {
                panic!("a closed provider cannot accept input")
            }
        }
        let h=harness(0);
        let backend=h.host.backend_for(h.binding.thread_id()).unwrap();
        *backend.lease.lock().unwrap()=Some(Box::new(Closed));
        assert_eq!(h.spawns.load(Ordering::SeqCst),0);
        h.host.ensure_lease(&backend,&h.binding).unwrap();
        assert_eq!(h.spawns.load(Ordering::SeqCst),1,"an idle EOF left Stu on a closed provider");
        h.host.ensure_lease(&backend,&h.binding).unwrap();
        assert_eq!(h.spawns.load(Ordering::SeqCst),1,"a healthy connection was replaced");
        h.host.shutdown();std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn account_resume_preserves_background_commands_and_waits_for_the_input_boundary() {
        use crate::lease_commands::CommandReading;
        struct Busy {
            dropped:Arc<std::sync::atomic::AtomicBool>,
            commands:Arc<Mutex<CommandReading>>,
            pending:Arc<std::sync::atomic::AtomicBool>,
        }
        impl Cognition for Busy {
            fn session_id(&self)->&str {"old-account-session"}
            fn account(&self)->Option<&str> {Some(crate::claude_accounts::ACCOUNT_ONE)}
            fn has_pending_input(&self)->bool {self.pending.load(Ordering::SeqCst)}
            fn running_commands(&self)->Option<CommandReading> {Some(*self.commands.lock().unwrap())}
            fn prompt(&mut self,_:&str,_:&mut dyn FnMut(TurnItem))->Result<String,CognitionError> {Ok("end_turn".into())}
        }
        impl Drop for Busy {fn drop(&mut self){self.dropped.store(true,Ordering::SeqCst);}}
        let h=harness(0);
        let quota=Arc::new(crate::quota::Service::open(&h.root.join("quota")).unwrap());
        let next=quota.accounts.add("Other").unwrap();quota.accounts.use_first(&next.id).unwrap();
        h.host.set_quota(quota);
        let backend=h.host.backend_for(h.binding.thread_id()).unwrap();
        let dropped=Arc::new(std::sync::atomic::AtomicBool::new(false));
        let commands=Arc::new(Mutex::new(CommandReading::Running(1)));
        let pending=Arc::new(std::sync::atomic::AtomicBool::new(false));
        *backend.lease.lock().unwrap()=Some(Box::new(Busy {dropped:dropped.clone(),commands:commands.clone(),pending:pending.clone()}));
        for reading in [CommandReading::Running(1),CommandReading::Unreadable] {
            *commands.lock().unwrap()=reading;
            h.host.renew_if_account_changed(&backend,&h.binding);
            assert!(!dropped.load(Ordering::SeqCst),"resume killed a possibly running command");
            assert_eq!(h.spawns.load(Ordering::SeqCst),0);
        }
        *commands.lock().unwrap()=CommandReading::Clear;pending.store(true,Ordering::SeqCst);
        h.host.renew_if_account_changed(&backend,&h.binding);
        assert!(!dropped.load(Ordering::SeqCst));
        pending.store(false,Ordering::SeqCst);
        h.host.renew_if_account_changed(&backend,&h.binding);
        assert!(dropped.load(Ordering::SeqCst));
        assert_eq!(h.spawns.load(Ordering::SeqCst),1,"clear commands and finished input should use the common opener");
        h.host.shutdown();std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **Stu's lease tells the accounts where it runs** (nightly 65, 2026-10-18: "I switched
    /// the team to your Home account" was said while Stu's whole job ran on Work). The switch
    /// from Account 1 to Home is decided while Stu's lease is on Account 1: Rich's turn on Home
    /// says nothing yet and the panel shows no switch card; Stu's lease moving at its boundary
    /// releases the line and the card.
    #[test]
    fn the_switch_is_announced_only_once_stus_lease_has_moved_too() {
        struct OnAccount(String);
        impl Cognition for OnAccount {
            fn session_id(&self)->&str {"stu-on-an-account"}
            fn account(&self)->Option<&str> {Some(&self.0)}
            fn prompt(&mut self,_:&str,_:&mut dyn FnMut(TurnItem))->Result<String,CognitionError> {Ok("end_turn".into())}
        }
        struct Follows(Arc<crate::quota::Service>);
        impl LeaseFactory for Follows {
            fn spawn(&self)->Result<Box<dyn Cognition>,CognitionError> {unreachable!()}
            fn spawn_work(&self,_:&ThreadBinding)->Result<Box<dyn Cognition>,CognitionError> {
                Ok(Box::new(OnAccount(self.0.lease_account().id)))
            }
        }
        use crate::claude_accounts::tests::{reading, NOW};
        let h=harness(0);
        let quota=Arc::new(crate::quota::Service::open(&h.root.join("quota")).unwrap());
        let home=quota.accounts.add("Home").unwrap();
        let host=WorkHost::new(&h.state,Box::new(Follows(quota.clone())),Arc::new(SilentNotifier));
        host.set_quota(quota.clone());
        let backend=host.backend_for(h.binding.thread_id()).unwrap();
        host.ensure_lease(&backend,&h.binding).unwrap();
        let readings=[("1".to_string(),reading(10.,3_600_000,99.,48*3_600_000)),
            (home.id.clone(),reading(0.,3_600_000,0.,72*3_600_000))].into_iter().collect();
        assert!(quota.accounts.evaluate(&readings,None,NOW).unwrap(),"Account 1 at 99% of its week is left for Home");
        quota.accounts.ran_on("rich-reply",&home.id);
        assert_eq!(quota.accounts.take_notice(),None,"Stu is still on Account 1");
        assert!(quota.view().last_switch.is_none(),"no switch card while Stu is on Account 1");
        host.renew_if_account_changed(&backend,&h.binding);
        assert_eq!(backend.lease.lock().unwrap().as_ref().and_then(|lease|lease.account().map(str::to_string)),Some(home.id.clone()));
        assert!(quota.accounts.take_notice().unwrap().starts_with("I switched the team to your **Home** account."));
        assert_eq!(quota.view().last_switch.map(|s|s.to),Some(home.id.clone()));
        host.shutdown();h.host.shutdown();std::fs::remove_dir_all(h.root).unwrap();
    }

    /// A Stu whose turns can be driven as Claude Code drives an automatic one: the test hands
    /// frames to the between-turn sink the host installed, as the native reader does.
    struct Driven {
        session: String,
        account: String,
        commands: Arc<Mutex<crate::lease_commands::CommandReading>>,
        sink: Arc<Mutex<Option<crate::cognition::BetweenTurnSink>>>,
        prompts: Arc<Mutex<Vec<(String, String)>>>,
    }
    impl Cognition for Driven {
        fn session_id(&self)->&str {&self.session}
        fn account(&self)->Option<&str> {Some(&self.account)}
        fn running_commands(&self)->Option<crate::lease_commands::CommandReading> {Some(*self.commands.lock().unwrap())}
        fn set_between_turn_sink(&mut self,sink:crate::cognition::BetweenTurnSink) {*self.sink.lock().unwrap()=Some(sink);}
        fn refresh_team(&mut self)->Result<bool,CognitionError> {Ok(true)}
        fn prompt(&mut self,text:&str,_:&mut dyn FnMut(TurnItem))->Result<String,CognitionError> {
            self.prompts.lock().unwrap().push((self.account.clone(),text.to_string()));
            Ok("end_turn".into())
        }
    }
    /// Opens every lease on the account in use, as the production factory does
    /// (`lease_account`), and keeps the last one's sink.
    struct OnAccountInUse {
        quota: Arc<crate::quota::Service>,
        spawns: Arc<AtomicUsize>,
        commands: Arc<Mutex<crate::lease_commands::CommandReading>>,
        sink: Arc<Mutex<Option<crate::cognition::BetweenTurnSink>>>,
        prompts: Arc<Mutex<Vec<(String, String)>>>,
        team: Arc<Mutex<TeamScript>>,
    }
    impl LeaseFactory for OnAccountInUse {
        fn spawn(&self)->Result<Box<dyn Cognition>,CognitionError> {unreachable!()}
        fn team_digest(&self)->Option<String> {self.team.lock().unwrap().digest.clone()}
        fn spawn_work(&self,_:&ThreadBinding)->Result<Box<dyn Cognition>,CognitionError> {
            let n=self.spawns.fetch_add(1,Ordering::SeqCst);
            Ok(Box::new(Driven {session:format!("stu-session-{n}"),account:self.quota.lease_account().id,
                commands:self.commands.clone(),sink:self.sink.clone(),prompts:self.prompts.clone()}))
        }
    }
    struct Moving {
        h: Harness,
        quota: Arc<crate::quota::Service>,
        home: crate::claude_accounts::Account,
        host: Arc<WorkHost>,
        spawns: Arc<AtomicUsize>,
        commands: Arc<Mutex<crate::lease_commands::CommandReading>>,
        sink: Arc<Mutex<Option<crate::cognition::BetweenTurnSink>>>,
        prompts: Arc<Mutex<Vec<(String, String)>>>,
    }
    /// Stu started on Account 1 by a message in his inbox (the run loop opens his lease), then
    /// the switch from Account 1 (99% of its week) to Home is decided while a command of his runs.
    fn stu_on_account_one_then_a_switch() -> Moving {
        let m=stu_on_account_one();
        *m.commands.lock().unwrap()=crate::lease_commands::CommandReading::Running(1);
        switch_to_home(&m);
        m
    }
    /// The switch from Account 1 (99% of its week) to Home is decided.
    fn switch_to_home(m: &Moving) {
        use crate::claude_accounts::tests::{reading, NOW};
        let readings=[("1".to_string(),reading(10.,3_600_000,99.,48*3_600_000)),
            (m.home.id.clone(),reading(0.,3_600_000,0.,72*3_600_000))].into_iter().collect();
        assert!(m.quota.accounts.evaluate(&readings,None,NOW).unwrap(),"Account 1 at 99% of its week is left for Home");
    }
    /// Stu started on Account 1 by a message in his inbox (the run loop opens his lease).
    fn stu_on_account_one() -> Moving {
        stu_on_account_one_with_team(None)
    }
    /// The same, with the saved team `team` registered by the lease his first turn opens.
    fn stu_on_account_one_with_team(team: Option<&str>) -> Moving {
        let h=harness(0);
        h.team.lock().unwrap().digest=team.map(str::to_string);
        let quota=Arc::new(crate::quota::Service::open(&h.root.join("quota")).unwrap());
        let home=quota.accounts.add("Home").unwrap();
        let (spawns,prompts)=(Arc::new(AtomicUsize::new(0)),Arc::new(Mutex::new(Vec::new())));
        let commands=Arc::new(Mutex::new(crate::lease_commands::CommandReading::Clear));
        let sink=Arc::new(Mutex::new(None));
        let host=WorkHost::new(&h.state,Box::new(OnAccountInUse {quota:quota.clone(),spawns:spawns.clone(),
            commands:commands.clone(),sink:sink.clone(),prompts:prompts.clone(),team:h.team.clone()}),Arc::new(SilentNotifier));
        host.set_quota(quota.clone());
        crate::session_tools::observe_stall(&host.message_scope(&h.binding),"the website job").unwrap();
        host.wake_messages(&h.binding);
        until("Stu's first turn on Account 1",||prompts.lock().unwrap().len()==1);
        assert_eq!(prompts.lock().unwrap()[0].0,"1");
        Moving {h,quota,home,host,spawns,commands,sink,prompts}
    }
    /// Claude Code starts an automatic turn of Stu's on its own (a task notification) and he
    /// takes his first step in it; the turn's result comes later ([`automatic_turn_ends`]).
    fn automatic_turn_starts(m: &Moving) {
        let sink=m.sink.lock().unwrap().clone().expect("the host installs a between-turn sink");
        sink(vec![serde_json::json!({"type":"system","subtype":"task_notification","task_id":"build","status":"completed"}),
            serde_json::json!({"type":"assistant","message":{"content":[{"type":"text","text":"The build finished. Deploying next."}]}})]);
    }
    fn automatic_turn_ends(m: &Moving) {
        let sink=m.sink.lock().unwrap().clone().expect("the host installs a between-turn sink");
        sink(vec![serde_json::json!({"type":"result","subtype":"success","is_error":false,"stop_reason":"end_turn"})]);
    }
    /// Every message the app sent Stu with `event` in it, with the account it reached him on.
    fn told(m: &Moving, event: &str) -> Vec<(String, String)> {
        m.prompts.lock().unwrap().iter().filter(|(_,text)|text.contains(event)).cloned().collect()
    }
    /// One automatic turn of Stu's, as Claude Code reports it between turns of ours.
    fn automatic_turn(m: &Moving) {
        let sink=m.sink.lock().unwrap().clone().expect("the host installs a between-turn sink");
        sink(vec![serde_json::json!({"type":"system","subtype":"task_notification","task_id":"build","status":"completed"}),
            serde_json::json!({"type":"assistant","message":{"content":[{"type":"text","text":"The build finished."}]}}),
            serde_json::json!({"type":"result","subtype":"success","is_error":false,"stop_reason":"end_turn"})]);
    }

    /// **Stu really moves at the next boundary of his work, automatic turns included** (review
    /// rv-20261019T005929Z-e3d9ad03-3e7b, finding 2; nightly 65: after the switch to Home his whole
    /// website job ran as automatic turns, helper hand-backs and task notifications, on Work,
    /// because he was moved only before a message the app sent him). Nothing here calls the
    /// renewal: his turns end as Claude Code reports them.
    ///
    /// The switch comes while a command of his runs: ending his session would end it, so he is
    /// told once (`account_switch`, delivered on Account 1) and keeps his session. When an
    /// automatic turn ends with nothing of his running, he is on a new session on Home within
    /// seconds (well inside the idle check's 30 s, so it is the turn's end that moved him) and
    /// told so there (`account_moved`), and the panel shows the team on Home.
    #[test]
    fn stu_moves_to_the_new_account_when_an_automatic_turn_ends_and_is_told_while_a_command_runs() {
        use crate::lease_commands::CommandReading;
        let m=stu_on_account_one_then_a_switch();
        assert_eq!(m.quota.accounts.in_use().id,m.home.id,"the switch to Home is decided");
        automatic_turn(&m);
        until("Stu is told the switch is coming",||m.prompts.lock().unwrap().iter().any(|(_,text)|text.contains("account_switch")));
        assert_eq!(m.spawns.load(Ordering::SeqCst),1,"a running command was ended by a move");
        let told:Vec<_>=m.prompts.lock().unwrap().iter().filter(|(_,text)|text.contains("account_switch")).cloned().collect();
        assert_eq!(told.len(),1,"told once: {told:?}");
        assert_eq!(told[0].0,"1","told on the session that is being left");
        assert!(told[0].1.contains(r#"\"from\":\"Account 1\""#) && told[0].1.contains(r#"\"to\":\"Home\""#),"{}",told[0].1);

        *m.commands.lock().unwrap()=CommandReading::Clear;
        let ended=std::time::Instant::now();
        automatic_turn(&m);
        until("Stu is on Home",||m.spawns.load(Ordering::SeqCst)==2);
        // A third of the idle check's 30 s (`ACCOUNT_CHECK_EVERY`).
        assert!(ended.elapsed()<std::time::Duration::from_secs(10),"moved by the turn's end, not the idle check: {:?}",ended.elapsed());
        until("Stu is told he moved, on Home",||m.prompts.lock().unwrap().iter().any(|(account,text)|account==&m.home.id&&text.contains("account_moved")));
        let view=serde_json::to_value(m.quota.view()).unwrap();
        let working=|id:&str|view["accounts"].as_array().unwrap().iter().find(|a|a["id"]==id).map(|a|a["working"].clone());
        assert_eq!(working(&m.home.id),Some(serde_json::json!(["team"])));
        assert_eq!(working("1"),Some(serde_json::Value::Null),"nobody is left on Account 1");
        m.host.shutdown();m.h.host.shutdown();std::fs::remove_dir_all(&m.h.root).unwrap();
    }

    /// The run loop has nothing left to do for now: no input of ours and no account check.
    fn settled(m: &Moving) {
        let backend=m.host.backend(m.h.binding.thread_id()).unwrap();
        until("the run loop settles",||{let inner=backend.inner.lock().unwrap();!inner.processing&&inner.move_check.is_none()});
    }

    /// **Stu is told the switch is coming while an automatic turn of his runs** (review
    /// rv-20261019T030516Z-0a1a8728-e798, finding 1: the run loop skipped every account check
    /// while one ran, so through a job of many steps he kept spending the old account and was
    /// never asked to save and stop). The switch is decided while a command of his runs; then
    /// Claude Code starts an automatic turn and he takes a step. His turn's result has not come
    /// yet: he is told now, on Account 1, and his session is not ended under him. When the
    /// turn's result comes with nothing of his running, he is on Home and told to carry on.
    #[test]
    fn stu_is_told_to_save_and_stop_while_an_automatic_turn_runs() {
        use crate::lease_commands::CommandReading;
        let m=stu_on_account_one_then_a_switch();
        automatic_turn_starts(&m);
        until("Stu is told the switch is coming while his automatic turn runs",||!told(&m,"account_switch").is_empty());
        let switch=told(&m,"account_switch");
        assert_eq!(switch.len(),1,"told once: {switch:?}");
        assert_eq!(switch[0].0,"1","told on the session he is still on");
        assert_eq!(m.spawns.load(Ordering::SeqCst),1,"his running turn and command were ended by a move");

        *m.commands.lock().unwrap()=CommandReading::Clear;
        automatic_turn_ends(&m);
        until("Stu is on Home",||m.spawns.load(Ordering::SeqCst)==2);
        until("Stu is told to carry on, on Home",||told(&m,"account_moved").iter().any(|(account,_)|account==&m.home.id));
        assert_eq!(told(&m,"account_switch").len(),1,"told again after it was said: {:?}",told(&m,"account_switch"));
        m.host.shutdown();m.h.host.shutdown();std::fs::remove_dir_all(&m.h.root).unwrap();
    }

    /// **Stu is always told to carry on after a move** (review rv-20261019T030516Z-0a1a8728-e798,
    /// finding 1: a move he had not been warned of sent nothing, so his resumed session had
    /// nothing telling it to continue). An automatic turn of his starts before any switch; the
    /// switch is decided while it runs; its result comes with nothing of his running, so he is
    /// moved at once, and told so on Home.
    #[test]
    fn stu_is_told_to_carry_on_after_a_move_he_was_not_warned_of() {
        let m=stu_on_account_one();
        automatic_turn_starts(&m);
        settled(&m);
        switch_to_home(&m);
        automatic_turn_ends(&m);
        until("Stu is on Home",||m.spawns.load(Ordering::SeqCst)==2);
        assert_eq!(told(&m,"account_switch"),Vec::<(String,String)>::new(),"he was warned after all");
        until("Stu is told to carry on, on Home",||told(&m,"account_moved").iter().any(|(account,_)|account==&m.home.id));
        let moved=told(&m,"account_moved");
        assert!(moved[0].1.contains(r#"\"from\":\"Account 1\""#) && moved[0].1.contains(r#"\"to\":\"Home\""#),"{}",moved[0].1);
        m.host.shutdown();m.h.host.shutdown();std::fs::remove_dir_all(&m.h.root).unwrap();
    }

    /// **A teammate Stu saves in an automatic turn is reloaded when that turn ends, and Stu gets
    /// his one turn to take up the job** (nightly 73 walk, check 5, 2026-10-24: a task
    /// notification started his turn, he saved norm in it, `prepare norm` was refused, the turn
    /// ended, and nothing reloaded the team or told him; only an unrelated stall reminder moved
    /// the job on). No input of ours and no reminder here: the turn's end alone does it, on the
    /// same session, and an automatic turn that changes nothing adds nothing.
    /// **His jobs settle in automatic turns too, and the app's sweep runs there** (CEO §54
    /// addendum 2; the 2026-10-25 garbage walk, walk-d04d9bdd7d53: the job settled, its one
    /// assignment record said `settled`, and the app logged no "cleanup at settled" at all). A
    /// job's last steps run as automatic turns (a teammate's hand-back, a task notification), and
    /// only a turn the app sent him reached the settled check.
    #[test]
    fn an_automatic_turn_that_leaves_nothing_open_runs_the_settled_sweep() {
        let m=stu_on_account_one();
        settled(&m);
        let moments=Arc::new(Mutex::new(Vec::new()));
        let seen=moments.clone();
        m.host.set_cleanup(Arc::new(move |moment| seen.lock().unwrap().push(moment)));
        automatic_turn(&m);
        until("the sweep at the end of an automatic turn with nothing open",
              ||moments.lock().unwrap().contains(&crate::app_cleanup::Moment::Settled));
        m.host.shutdown();m.h.host.shutdown();std::fs::remove_dir_all(&m.h.root).unwrap();
    }

    #[test]
    fn a_teammate_saved_in_an_automatic_turn_is_reloaded_when_it_ends_and_stu_carries_on() {
        let m=stu_on_account_one_with_team(Some("stock"));
        settled(&m);
        automatic_turn_starts(&m);
        m.h.team.lock().unwrap().digest=Some("stock and norm".into());
        automatic_turn_ends(&m);
        until("Stu is told the team reloaded",||!told(&m,"team_reloaded").is_empty());
        settled(&m);
        let backend=m.host.backend(m.h.binding.thread_id()).unwrap();
        assert_eq!(backend.inner.lock().unwrap().lease_team.as_deref(),Some("stock and norm"),"norm is not registered");
        assert_eq!(m.spawns.load(Ordering::SeqCst),1,"the reload replaced Stu's session");
        let reloaded=told(&m,"team_reloaded");
        assert_eq!(reloaded.len(),1,"one turn per change: {reloaded:?}");
        assert_eq!(reloaded[0].0,"1","told on the session he saved norm in");
        assert_eq!(m.prompts.lock().unwrap().len(),2,"anything but his first turn and the reload: {:?}",m.prompts.lock().unwrap());
        automatic_turn(&m);
        settled(&m);
        assert_eq!(told(&m,"team_reloaded").len(),1,"an unchanged team got a continuation");
        m.host.shutdown();m.h.host.shutdown();std::fs::remove_dir_all(&m.h.root).unwrap();
    }

    /// **The panel shows the account each session is really on** (review
    /// rv-20261019T005929Z-e3d9ad03-3e7b, finding 3: Home was "In use now" while Stu still ran on
    /// Work). The switch to Home is decided and Rich's next turn runs on Home; Stu's session is
    /// still on Account 1 with a command running. The published view says so per account, and
    /// since Rich's decision of 2026-10-29 (the CEO: "I saw "switching to home" and the Home
    /// account was listed first in the list. But apparently it didn't actually switch.") the
    /// account marked in use is the one the work still runs on, Account 1, with the move to
    /// Home under way (`moving`) until Stu's session is on Home too.
    #[test]
    fn the_accounts_view_shows_where_rich_and_the_team_really_are_during_a_switch() {
        let m=stu_on_account_one_then_a_switch();
        m.quota.accounts.ran_on("rich-reply",&m.home.id);
        let view=serde_json::to_value(m.quota.view()).unwrap();
        let account=|id:&str|view["accounts"].as_array().unwrap().iter().find(|a|a["id"]==id).cloned().unwrap();
        assert_eq!((account(&m.home.id)["inUse"].clone(),account(&m.home.id)["working"].clone()),
            (serde_json::json!(false),serde_json::json!(["rich"])),"Home: switched to, not yet in use, and Rich is on it");
        assert_eq!((account("1")["inUse"].clone(),account("1")["working"].clone()),
            (serde_json::json!(true),serde_json::json!(["team"])),"Account 1: still in use, the team is still on it");
        assert_eq!((view["moving"]["from"].clone(),view["moving"]["to"].clone()),
            (serde_json::json!("1"),serde_json::json!(m.home.id)),"the move to Home is under way");
        m.host.shutdown();m.h.host.shutdown();std::fs::remove_dir_all(&m.h.root).unwrap();
    }

    #[test]
    fn a_finished_native_turn_keeps_its_unfinished_job_resumable_after_quit() {
        for terminal in [Some(AssignmentState::Settled),Some(AssignmentState::Failed),None] {
        for result in ["end_turn","max_tokens","provider_error"] {
        let h=harness(1);let record=assignment::register(&h.state,&registration(&h)).unwrap();
        assignment::advance(&h.state,"depot","thread-one",&record.id,AssignmentState::Running,"Worker start observed").unwrap();
        let scope=h.host.message_scope(&h.binding);let path=h.root.join("peer-status.json");
        crate::session_tools::bind(&path,&scope,"front-session","rich",Some(crate::ecs::UserInstruction{ledger_ref:registration(&h).instruction_ledger_ref,sha256:registration(&h).instruction_sha256,text:None})).unwrap();
        crate::session_tools::delegate(&path,&scope,serde_json::json!({"text":"Land the three branches"})).unwrap();
        let through=crate::session_tools::pending(&scope,"stu").unwrap().unwrap().1;
        h.host.wake_messages(&h.binding);
        assert!(h.host.wait_for_completed(1,std::time::Duration::from_secs(30)),"native turn did not end");
        assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_some(),"native end_turn erased the still-unfinished handoff");
        assert!(crate::session_tools::previous_attempt(&scope,"stu",&format!("peer:{through}")).unwrap().is_some(),"native end_turn erased Stu's recovery evidence");
        crate::session_tools::complete_attempt(&scope,"stu",&format!("peer:{through}")).unwrap();
        assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_some(),"generic attempt completion bypassed the job's primary report");
        // A later correction is independent input, not a request to execute the handoff again.
        h.team.lock().unwrap().turn_results.push_back(result);
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"Keep the original release date"})).unwrap();
        h.host.wake_messages(&h.binding);assert!(h.host.wait_for_completed(2,std::time::Duration::from_secs(30)));
        let prompts=h.work_prompts.lock().unwrap().clone();
        let retry = usize::from(result=="provider_error");
        assert_eq!(prompts.len(),2+retry,"unfinished work was repeatedly redispatched to the same Stu: {prompts:?}");
        assert!(prompts[1].contains("Keep the original release date"));
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"Use the connected repository"})).unwrap();
        h.host.wake_messages(&h.binding);assert!(h.host.wait_for_completed(3,std::time::Duration::from_secs(30)));
        let prompts=h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(),3+retry,"a failed turn made accepted work new again: {prompts:?}");
        assert!(prompts[2+retry].contains("Use the connected repository"),"old work was repeated after {result}: {prompts:?}");
        assert_eq!(h.spawns.load(Ordering::SeqCst),1,"a turn result replaced live Stu");
        h.host.shutdown();
        let resumed=crate::session_tools::pending(&scope,"stu").unwrap().unwrap();assert_eq!(resumed.1,through);
        let factory=Factory{spawns:h.spawns.clone(),prompts:h.work_prompts.clone(),commands:h.commands.clone(),team:h.team.clone(),gate:h.start_gate.clone()};
        let restarted=WorkHost::new(&h.state,Box::new(factory),Arc::new(SilentNotifier));
        // Only the open handoff comes back. The correction's own turn may have ended without
        // `end_turn`, but the next turn of the same session did, so Stu had it (Otto's trace of
        // 2026-10-29: such input came back after a restart although he had acted on it).
        let recovered=1;
        restarted.wake_messages(&h.binding);assert!(restarted.wait_for_completed(recovered,std::time::Duration::from_secs(30)));
        let prompts=h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(),3+retry+recovered as usize,"restart repeated input beyond its unfinished receipts: {prompts:?}");
        assert!(prompts[3+retry].contains("Land the three branches"));
        assert!(!prompts[3+retry..].iter().any(|p|p.contains("Keep the original release date")),
            "{result}: a correction a later normal turn of the same session followed was sent again: {prompts:?}");
        assert!(!prompts[3+retry..].iter().any(|p|p.contains("Use the connected repository")),"successfully completed input replayed");
        assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_some());
        // Only Stu's actual terminal report retires the original input.
        if let Some(terminal)=terminal {assignment::advance(&h.state,"depot","thread-one",&record.id,terminal,"Stu's terminal report").unwrap();}
        else {assignment::record_user_stop(&h.state,"depot","thread-one",&record.id).unwrap();}
        assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_none());
        assert!(crate::session_tools::previous_attempt(&scope,"stu",&format!("peer:{through}")).unwrap().is_none());
        restarted.shutdown();
        }
        }
    }

    #[test]
    fn scheduled_input_retries_only_overload_and_recovers_unfinished_work_after_restart() {
        for kind in ["handoff", "peer", "standing-answer", "job-answer"] {
            for result in ["end_turn", "max_tokens", "provider_error", "io_error", "pre_model_503", "pre_model_529", "pre_model_429", "pre_model_protocol", "tool_error", "native_quota", "max_tokens_no_text"] {
                let h = harness(0);
                let scope = h.host.message_scope(&h.binding);
                let path = h.root.join("peer-status.json");
                let instruction = if matches!(kind, "handoff" | "job-answer") {
                    assignment::register(&h.state, &registration(&h)).unwrap();
                    Some(crate::ecs::UserInstruction {
                        ledger_ref: registration(&h).instruction_ledger_ref,
                        sha256: registration(&h).instruction_sha256,
                        text: None,
                    })
                } else {
                    None
                };
                crate::session_tools::bind(&path, &scope, "front-session", "rich", instruction).unwrap();
                h.team.lock().unwrap().turn_results.push_back(result);
                match kind {
                    "handoff" => { crate::session_tools::delegate(&path, &scope, serde_json::json!({"text":"Input under test"})).unwrap(); }
                    "peer" => { crate::session_tools::send(&path, &scope, serde_json::json!({"text":"Input under test"})).unwrap(); }
                    _ => {
                        let answer = crate::questions::Delivery {
                            obligation_id: (kind == "job-answer").then(|| registration(&h).obligation_id),
                            id: "answer-under-test".into(), entity_id: "depot".into(), thread_id: "thread-one".into(),
                            asker: "stu".into(), set_id: None, text: "Input under test".into(), receipt: None, order: None,
                        };
                        h.host.queue_question_answer(&h.binding, &answer).unwrap();
                    }
                }
                h.host.wake_messages(&h.binding);
                assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(30)));
                crate::session_tools::send(&path, &scope, serde_json::json!({"text":"Independent later input"})).unwrap();
                h.host.wake_messages(&h.binding);
                until("later input must pass an answer already received by live Stu", || h.work_prompts.lock().unwrap().iter().any(|p| p.contains("Independent later input")));
                // Its turn has ended, so what it settles is settled before the restart below.
                assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(30)), "{kind}/{result}: the later input's turn did not end");
                h.host.shutdown();
                let prompts = h.work_prompts.lock().unwrap().clone();
                let recovered_overload = matches!(result,"provider_error"|"pre_model_529");
                let expected = if result == "io_error" || recovered_overload {2} else {1};
                assert_eq!(prompts.iter().filter(|p|p.contains("Input under test")).count(),expected,
                    "{kind}/{result}: input repeated in the same live session: {prompts:?}");
                assert_eq!(h.spawns.load(Ordering::SeqCst), if result=="io_error" {2}else{1});
                let mut facts=Vec::new();
                while let Some((message,through))=crate::session_tools::pending(&scope,"rich").unwrap() {
                    assert_eq!(message.from,"work-status");assert!(message.instruction.is_none());
                    facts.push(serde_json::from_str::<serde_json::Value>(&message.text).unwrap());
                    crate::session_tools::acknowledge(&scope,"rich",through).unwrap();
                }
                assert_eq!(facts.len(),usize::from(result!="end_turn" && !recovered_overload),"{kind}/{result}: wrong failure-report count");
                let before = prompts.len();
                crate::session_tools::send(&path, &scope, serde_json::json!({"text":"After restart"})).unwrap();
                let restarted = WorkHost::new(&h.state, Box::new(Factory {
                    spawns: h.spawns.clone(), prompts: h.work_prompts.clone(), commands: h.commands.clone(),
                    team: h.team.clone(), gate: h.start_gate.clone(),
                }), Arc::new(SilentNotifier));
                restarted.wake_messages(&h.binding);
                until("restart must deliver its new input", || h.work_prompts.lock().unwrap().iter().any(|p| p.contains("After restart")));
                restarted.shutdown();
                let prompts = h.work_prompts.lock().unwrap().clone();
                // Only an open handoff comes back: whatever the input's own turn came to, the later
                // input's turn on the same session ended `end_turn`, so Stu had it (Otto's trace of
                // 2026-10-29); after `io_error` that later turn is on a new session, and the input
                // was sent again there.
                let unfinished = kind == "handoff";
                assert_eq!(prompts[before..].iter().filter(|p| p.contains("Input under test")).count(), usize::from(unfinished),
                    "{kind}/{result}: new session lost or repeated recovery input: {prompts:?}");
                assert!(!prompts[before..].iter().any(|p| p.contains("Independent later input")), "completed input replayed");
                std::fs::remove_dir_all(h.root).unwrap();
            }
        }
    }

    #[test]
    fn stu_overload_uses_the_ceos_seven_waits_on_one_session_and_success_resets_them() {
        let h = harness(0);
        let clock = Arc::new(RecordedRetryClock::default());
        h.host.set_retry_clock(clock.clone());
        let scope = h.host.message_scope(&h.binding);
        let path = h.root.join("peer-status.json");
        crate::session_tools::bind(&path, &scope, "front", "rich", None).unwrap();
        h.team.lock().unwrap().turn_results.extend(std::iter::repeat_n("pre_model_529", 7).chain(["end_turn"]));
        crate::session_tools::send(&path, &scope, serde_json::json!({"text":"Original request"})).unwrap();
        h.host.wake_messages(&h.binding);
        // Hang guard: the clock is controlled; actual input completion is the verdict.
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(30)), "retry fixture did not finish");
        let prompts = h.work_prompts.lock().unwrap().clone();
        assert_eq!(prompts.len(), 8);
        assert!(prompts.iter().all(|prompt| prompt == &prompts[0]), "retry changed the original input");
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1);
        assert_eq!(*clock.0.lock().unwrap(), crate::upstream::OVERLOAD_RETRY_WAITS);
        assert!(crate::session_tools::pending(&scope, "rich").unwrap().is_none(), "recovered failure was announced as terminal");
        // Each wait is shown in his conversation as RichOS's own retry line, as Rich's is.
        let shown = h.notices.0.lock().unwrap().clone();
        let expected: Vec<(String, String)> = [1, 2, 5, 10, 20, 40, 80].iter().enumerate().map(|(k, minutes)| {
            let when = if *minutes == 1 { "1 minute".to_string() } else { format!("{minutes} minutes") };
            ("thread-one".to_string(), format!("{} RichOS will try again in {when}, in the same conversation, so nothing is \
                lost. This is retry {} of 7.", crate::upstream::UpstreamFault::Overloaded.ceo_message(), k + 1))
        }).collect();
        assert_eq!(shown, expected, "Stu's overload waits were not shown as RichOS's retry line");
        h.team.lock().unwrap().turn_results.extend(["pre_model_529", "end_turn"]);
        crate::session_tools::send(&path, &scope, serde_json::json!({"text":"Independent later request"})).unwrap();
        h.host.wake_messages(&h.binding);
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(30)));
        assert_eq!(clock.0.lock().unwrap().as_slice(), [crate::upstream::OVERLOAD_RETRY_WAITS.as_slice(), &[std::time::Duration::from_secs(60)]].concat());
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1);
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// A Stu lease on the account it was opened under that is refused by a usage limit, as
    /// `native.rs` reports it, for the first `cut` prompts given to any lease.
    struct LimitedLease {
        session: String,
        account: String,
        resets_at: u64,
        cut: Arc<AtomicUsize>,
        prompts: Arc<Mutex<Vec<(String, String)>>>,
        commands: Arc<Mutex<crate::lease_commands::CommandReading>>,
        before_cut: BeforeCut,
    }
    /// What happens elsewhere while Stu's turn runs, just before the limit cuts it (run once).
    type BeforeCut = Arc<Mutex<Option<Box<dyn FnOnce() + Send>>>>;
    impl Cognition for LimitedLease {
        fn session_id(&self)->&str {&self.session}
        fn account(&self)->Option<&str> {Some(&self.account)}
        fn running_commands(&self)->Option<crate::lease_commands::CommandReading> {Some(*self.commands.lock().unwrap())}
        fn refresh_team(&mut self)->Result<bool,CognitionError> {Ok(true)}
        fn prompt(&mut self,text:&str,_:&mut dyn FnMut(TurnItem))->Result<String,CognitionError> {
            self.prompts.lock().unwrap().push((self.account.clone(),text.to_string()));
            if self.cut.fetch_update(Ordering::SeqCst,Ordering::SeqCst,|left|left.checked_sub(1)).is_ok() {
                let elsewhere=self.before_cut.lock().unwrap().take();
                if let Some(elsewhere)=elsewhere {elsewhere();}
                return Err(CognitionError::Protocol(crate::claude_accounts::usage_limit_error(Some(self.resets_at),
                    "[\"You've hit your session limit\"]")));
            }
            Ok("end_turn".into())
        }
    }
    struct LimitedFactory { quota: Arc<crate::quota::Service>, spawns: Arc<AtomicUsize>, resets_at: u64,
        cut: Arc<AtomicUsize>, prompts: Arc<Mutex<Vec<(String, String)>>>,
        commands: Arc<Mutex<crate::lease_commands::CommandReading>>, before_cut: BeforeCut }
    impl LeaseFactory for LimitedFactory {
        fn spawn(&self)->Result<Box<dyn Cognition>,CognitionError> {unreachable!()}
        fn spawn_work(&self,_:&ThreadBinding)->Result<Box<dyn Cognition>,CognitionError> {
            let n=self.spawns.fetch_add(1,Ordering::SeqCst);
            Ok(Box::new(LimitedLease {session:format!("stu-limited-{n}"),account:self.quota.lease_account().id,
                resets_at:self.resets_at,cut:self.cut.clone(),prompts:self.prompts.clone(),commands:self.commands.clone(),
                before_cut:self.before_cut.clone()}))
        }
    }
    struct Limited {
        h: Harness,
        host: Arc<WorkHost>,
        quota: Arc<crate::quota::Service>,
        clock: Arc<RecordedRetryClock>,
        notices: Arc<RecordingNotifier>,
        cut: Arc<AtomicUsize>,
        prompts: Arc<Mutex<Vec<(String, String)>>>,
        /// What Stu's leases report running (`Clear` unless a test says otherwise).
        commands: Arc<Mutex<crate::lease_commands::CommandReading>>,
        before_cut: BeforeCut,
        resets_at: u64,
    }
    /// A quota service with Account 1 in use and Home, both read with room (the probe fixture),
    /// under `h`'s root.
    #[cfg(unix)]
    fn account_one_and_home_with_room(h: &Harness) -> (Arc<crate::quota::Service>, crate::claude_accounts::Account) {
        let root=h.root.join("quota");
        std::fs::create_dir_all(&root).unwrap();
        let bin=crate::quota::tests::fake_claude(&root);
        let quota=Arc::new(crate::quota::Service::open(&root).unwrap());
        let home=quota.accounts.add("Home").unwrap();
        crate::quota::tests::usage(&root.join("usage-1.json"),20.,40.,"2099-01-05T00:00:00Z");
        crate::quota::tests::usage(&home.folder.clone().unwrap().join("usage.json"),10.,20.,"2099-01-06T00:00:00Z");
        quota.refresh(&bin,true);
        assert_eq!(quota.accounts.in_use().id,"1","Account 1 has room and is in use");
        (quota,home)
    }
    /// Stu with the quota service `quota`, whose leases are refused by a usage limit resetting in
    /// an hour for the first `cut` prompts, on a recorded retry clock.
    fn stu_with_a_usage_limit(h: Harness, quota: Arc<crate::quota::Service>, cut: usize) -> Limited {
        let resets_at=crate::util::now_millis()+3_600_000;
        let (cut,prompts)=(Arc::new(AtomicUsize::new(cut)),Arc::new(Mutex::new(Vec::new())));
        let commands=Arc::new(Mutex::new(crate::lease_commands::CommandReading::Clear));
        let before_cut: BeforeCut=Arc::new(Mutex::new(None));
        let notices=Arc::new(RecordingNotifier::default());
        let host=WorkHost::new(&h.state,Box::new(LimitedFactory {quota:quota.clone(),spawns:Arc::new(AtomicUsize::new(0)),
            resets_at,cut:cut.clone(),prompts:prompts.clone(),commands:commands.clone(),before_cut:before_cut.clone()}),notices.clone());
        let clock=Arc::new(RecordedRetryClock::default());
        host.set_retry_clock(clock.clone());
        host.set_quota(quota.clone());
        crate::session_tools::bind(&h.root.join("peer-status.json"),&host.message_scope(&h.binding),"front","rich",None).unwrap();
        Limited {h,host,quota,clock,notices,cut,prompts,commands,before_cut,resets_at}
    }
    impl Limited {
        /// Rich hands Stu `text`; returns once the run loop has finished `completed` turns.
        fn send(&self, text: &str, completed: u64) {
            crate::session_tools::send(&self.h.root.join("peer-status.json"),&self.host.message_scope(&self.h.binding),
                serde_json::json!({"text":text})).unwrap();
            self.host.wake_messages(&self.h.binding);
            // Hang guard: the clock is recorded, so nothing here waits for real time.
            assert!(self.host.wait_for_completed(completed,std::time::Duration::from_secs(30)),"Stu's turn did not finish");
        }
        /// The account each delivery of `text` reached Stu on, in order.
        fn deliveries(&self, text: &str) -> Vec<String> {
            self.prompts.lock().unwrap().iter().filter(|(_,prompt)|prompt.contains(text)).map(|(account,_)|account.clone()).collect()
        }
        /// The failure facts Rich was given about Stu's turns.
        fn failures(&self) -> Vec<serde_json::Value> {
            let scope=self.host.message_scope(&self.h.binding);
            let mut facts=Vec::new();
            while let Some((message,through))=crate::session_tools::pending(&scope,"rich").unwrap() {
                facts.push(serde_json::from_str::<serde_json::Value>(&message.text).unwrap());
                crate::session_tools::acknowledge(&scope,"rich",through).unwrap();
            }
            facts
        }
        fn end(self) {
            self.host.shutdown();
            std::fs::remove_dir_all(&self.h.root).unwrap();
        }
    }

    /// **A turn of Stu's cut by a usage limit goes on at once on the next account with room**,
    /// as Rich's does (`spine.rs`, "FILL-FIRST'S BACKSTOP"). Account 1 and Home both have room
    /// by their readings; Stu's turn on Account 1 is refused by a usage limit. Account 1 is
    /// marked gone until its reset and Home put in use, and the same input reaches Stu once more,
    /// on Home, with no wait and naming the cut attempt. Rich is told of no failure.
    ///
    /// Before this, the input was given once and the run loop stopped there until someone
    /// happened to message Stu (2026-10-28).
    #[test]
    #[cfg(unix)]
    fn a_stu_turn_cut_by_a_usage_limit_is_sent_once_more_on_the_next_account_with_room() {
        let h=harness(0);
        let (quota,home)=account_one_and_home_with_room(&h);
        let stu=stu_with_a_usage_limit(h,quota,1);

        stu.send("Input under test",1);

        assert_eq!(stu.deliveries("Input under test"),["1".to_string(),home.id.clone()],
            "the input was not sent once more on the account with room: {:?}",stu.prompts.lock().unwrap());
        assert_eq!(stu.quota.accounts.in_use().id,home.id,"Home is in use");
        assert_eq!(stu.quota.accounts.limited_until("1"),Some(stu.resets_at),"Account 1 is gone until its reset");
        assert_eq!(*stu.clock.0.lock().unwrap(),[std::time::Duration::ZERO],"another account had room: no hold");
        let retried=stu.prompts.lock().unwrap().iter().filter(|(_,p)|p.contains("Input under test")).nth(1).cloned().unwrap().1;
        let retried: serde_json::Value=serde_json::from_str(&retried).unwrap();
        assert!(retried["previous_delivery_attempt"]["session_id"].as_str()==Some("stu-limited-0"),
            "the second delivery does not name the attempt the limit cut: {retried}");
        assert!(stu.failures().is_empty(),"a turn that went on on Home was reported as failed");
        assert!(stu.notices.0.lock().unwrap().is_empty(),"nothing is held, so no hold is shown");
        assert_eq!(stu.cut.load(Ordering::SeqCst),0);
        stu.end();
    }

    /// A retry clock whose hold never elapses: it records each wait, checks the hold is not
    /// already over, does `lift` and asks again. So a retry after it is one the hold's own early
    /// end allowed, never the time running out.
    struct LiftedDuringHold { waits: Mutex<Vec<std::time::Duration>>, lift: Box<dyn Fn() + Send + Sync> }
    impl LiftedDuringHold {
        fn new(lift: impl Fn() + Send + Sync + 'static) -> Self { Self {waits:Mutex::new(Vec::new()),lift:Box::new(lift)} }
        fn waits(&self) -> Vec<std::time::Duration> { self.waits.lock().unwrap().clone() }
    }
    impl crate::upstream::RetryClock for LiftedDuringHold {
        fn wait(&self, wait: std::time::Duration, stop: &dyn Fn() -> bool) -> bool {
            self.waits.lock().unwrap().push(wait);
            assert!(!stop(), "the hold was over before anything lifted the refusal");
            (self.lift)();
            assert!(stop(), "lifting the refusal did not end the hold");
            false
        }
    }

    /// **Never sent back to the account that refused it** (review of a6bd17fa5, P1). Home has
    /// room, but a command of Stu's is running when his turn on Account 1 is cut, so his session
    /// cannot leave Account 1 yet (`renew_lease` waits). The retry is held, not sent to Account 1
    /// at once to be refused again: the hold is to Account 1's reset an hour away, and it ends
    /// as soon as the command does, when the input goes once more, on Home.
    #[test]
    #[cfg(unix)]
    fn a_stu_turn_cut_by_a_usage_limit_waits_for_his_running_command_and_then_goes_on_the_next_account() {
        let h=harness(0);
        let (quota,home)=account_one_and_home_with_room(&h);
        let stu=stu_with_a_usage_limit(h,quota,1);
        *stu.commands.lock().unwrap()=crate::lease_commands::CommandReading::Running(1);
        let commands=stu.commands.clone();
        let clock=Arc::new(LiftedDuringHold::new(move || *commands.lock().unwrap()=crate::lease_commands::CommandReading::Clear));
        stu.host.set_retry_clock(clock.clone());

        stu.send("Input under test",1);

        assert_eq!(stu.deliveries("Input under test"),["1".to_string(),home.id.clone()],
            "the retry went back to the refused account: {:?}",stu.prompts.lock().unwrap());
        let waits=clock.waits();
        let (hour,minute)=(std::time::Duration::from_secs(3600),std::time::Duration::from_secs(60));
        assert!(waits.len()==1 && waits[0]<=hour && waits[0]>hour-minute,"not one hold to Account 1's reset: {waits:?}");
        assert!(stu.failures().is_empty(),"a turn that went on on Home was reported as failed");
        assert!(stu.notices.0.lock().unwrap().is_empty(),"Home has room: no line says no account has room");
        stu.end();
    }

    /// **Not spent on an account in use that is refused too** (review of e58c2c825, P2). While
    /// Stu's turn runs on Account 1, another conversation's turns are refused on Account 1 (Home
    /// is put in use) and then on Home. When Stu's turn is cut, `limit_reached` says only that
    /// another account is in use. The one retry is not sent to Home: it is held, to the soonest
    /// reset, with RichOS's line that no account has room, and it goes on Home once Home's
    /// refusal is lifted (a reset used on it).
    #[test]
    #[cfg(unix)]
    fn a_stu_turn_cut_by_a_usage_limit_is_held_when_the_account_in_use_was_refused_too() {
        let h=harness(0);
        let (quota,home)=account_one_and_home_with_room(&h);
        let stu=stu_with_a_usage_limit(h,quota.clone(),1);
        let elsewhere=(quota.clone(),home.id.clone());
        *stu.before_cut.lock().unwrap()=Some(Box::new(move || {
            let (quota,home)=elsewhere;
            let now=crate::util::now_millis();
            assert_eq!(quota.limit_reached("1",Some(now+3_600_000)),crate::claude_accounts::AfterLimit::Continue);
            assert_eq!(quota.limit_reached(&home,Some(now+2*3_600_000)),crate::claude_accounts::AfterLimit::NoRoom);
        }));
        let lifting=(quota.clone(),home.id.clone());
        let clock=Arc::new(LiftedDuringHold::new(move || lifting.0.accounts.reset_used(&lifting.1).unwrap()));
        stu.host.set_retry_clock(clock.clone());

        stu.send("Input under test",1);

        assert_eq!(stu.deliveries("Input under test"),["1".to_string(),home.id.clone()],
            "the one retry was spent while every account was refused: {:?}",stu.prompts.lock().unwrap());
        let waits=clock.waits();
        let (hour,minute)=(std::time::Duration::from_secs(3600),std::time::Duration::from_secs(60));
        assert!(waits.len()==1 && waits[0]<=hour && waits[0]>hour-minute,"not one hold to the soonest reset: {waits:?}");
        let shown=stu.notices.0.lock().unwrap().clone();
        assert!(shown.len()==1 && shown[0].1.contains("no Claude account has room"),"{shown:?}");
        assert!(stu.failures().is_empty());
        stu.end();
    }

    /// A retry clock that records each wait and, unless the hold is already over when it starts,
    /// does `lift` and returns as a hold cut short does. A retry after it is then one the hold's
    /// own early end allowed; a hold already over is sent at once, as before this clock.
    struct LiftOnWait { waits: Mutex<Vec<std::time::Duration>>, lift: Box<dyn Fn() + Send + Sync> }
    impl crate::upstream::RetryClock for LiftOnWait {
        fn wait(&self, wait: std::time::Duration, stop: &dyn Fn() -> bool) -> bool {
            self.waits.lock().unwrap().push(wait);
            if !stop() { (self.lift)(); }
            false
        }
    }

    /// **Not spent on an account its readings show used up** (second review
    /// rv-20261029T111217Z-71fb5f11-8f6b, P2). While Stu's turn runs on Account 1, another
    /// conversation is refused on Account 1 and Home is put in use; then Home's lease streams a
    /// reading of 100% of its week, with no refusal on Home. When Stu's turn is cut, `Continue`
    /// names Home, which has no refusal mark. The one retry is not sent to Home: it is held, to
    /// the soonest reset (Account 1's, an hour away), with RichOS's line that no account has
    /// room, and it goes once the hold is lifted (a reset used on Account 1), never on Home.
    #[test]
    #[cfg(unix)]
    fn a_stu_turn_cut_by_a_usage_limit_is_held_when_the_account_in_use_is_used_up_by_its_readings() {
        let h=harness(0);
        let (quota,home)=account_one_and_home_with_room(&h);
        let stu=stu_with_a_usage_limit(h,quota.clone(),1);
        let elsewhere=(quota.clone(),home.id.clone());
        *stu.before_cut.lock().unwrap()=Some(Box::new(move || {
            let (quota,home)=elsewhere;
            let now=crate::util::now_millis();
            assert_eq!(quota.limit_reached("1",Some(now+3_600_000)),crate::claude_accounts::AfterLimit::Continue);
            // What a lease on Home streams (`native::streamed_windows`): its week is used up.
            let week=crate::quota::Window {id:"seven_day".into(),label:"Weekly".into(),used_percent:100.,
                resets_at:Some(now+72*3_600_000),duration_ms:168*3_600_000};
            assert_eq!(quota.before_turn(&home,Some((vec![week],now+1))),home,"Home stays in use: no account has room to go to");
            assert_eq!(quota.accounts.limited_until(&home),None,"Home was never refused");
        }));
        let lifting=quota.clone();
        let clock=Arc::new(LiftOnWait {waits:Mutex::new(Vec::new()),lift:Box::new(move || lifting.accounts.reset_used("1").unwrap())});
        stu.host.set_retry_clock(clock.clone());

        stu.send("Input under test",1);

        let waits=clock.waits.lock().unwrap().clone();
        let (hour,minute)=(std::time::Duration::from_secs(3600),std::time::Duration::from_secs(60));
        assert!(waits.len()==1 && waits[0]<=hour && waits[0]>hour-minute,
            "the one retry was not held while Home was used up by its readings: {waits:?}");
        let shown=stu.notices.0.lock().unwrap().clone();
        assert!(shown.len()==1 && shown[0].1.contains("no Claude account has room"),"{shown:?}");
        assert_eq!(stu.deliveries("Input under test"),["1","1"],
            "the one retry went to Home, used up by its readings: {:?}",stu.prompts.lock().unwrap());
        assert!(stu.failures().is_empty());
        stu.end();
    }

    /// A retry clock that does the next of `steps` at each wait it is asked for, and records the
    /// wait. A step returns what the wait returns: true for a hold that ran out.
    /// One step of a [`StepClock`]: given the hold's stop check, what the wait returns.
    type Step = Box<dyn FnOnce(&dyn Fn() -> bool) -> bool + Send>;
    struct StepClock { waits: Mutex<Vec<std::time::Duration>>, steps: Mutex<VecDeque<Step>> }
    impl crate::upstream::RetryClock for StepClock {
        fn wait(&self, wait: std::time::Duration, stop: &dyn Fn() -> bool) -> bool {
            self.waits.lock().unwrap().push(wait);
            let step = self.steps.lock().unwrap().pop_front().expect("a wait the test did not expect");
            step(stop)
        }
    }

    /// **Not sent back to the refused account when another account resets first** (second
    /// review of 71fb5f113: the busy-session defect of e58c2c825 came back). While Stu's turn
    /// runs on Account 1 with a command of his running, another conversation is refused on
    /// Account 1 (Home put in use) and then on Home, whose limit resets in 30 minutes; Stu's own
    /// refusal on Account 1 resets in an hour. The hold is to Home's reset. When it runs out,
    /// Home is free but his command still holds his session on Account 1: the input is not sent
    /// there, the hold goes on (to Account 1's own reset), and it goes on Home once the command
    /// has ended.
    #[test]
    #[cfg(unix)]
    fn a_stu_turn_held_to_another_accounts_sooner_reset_waits_for_his_command_and_goes_on_that_account() {
        let h=harness(0);
        let (quota,home)=account_one_and_home_with_room(&h);
        let stu=stu_with_a_usage_limit(h,quota.clone(),1);
        *stu.commands.lock().unwrap()=crate::lease_commands::CommandReading::Running(1);
        let elsewhere=(quota.clone(),home.id.clone());
        *stu.before_cut.lock().unwrap()=Some(Box::new(move || {
            let (quota,home)=elsewhere;
            let now=crate::util::now_millis();
            assert_eq!(quota.limit_reached("1",Some(now+3_600_000)),crate::claude_accounts::AfterLimit::Continue);
            assert_eq!(quota.limit_reached(&home,Some(now+1_800_000)),crate::claude_accounts::AfterLimit::NoRoom);
        }));
        let (home_back,commands)=((quota.clone(),home.id.clone()),stu.commands.clone());
        let clock=Arc::new(StepClock {waits:Mutex::new(Vec::new()),steps:Mutex::new(VecDeque::from([
            // Home's reset arrives: the hold runs out.
            Box::new(move |_: &dyn Fn() -> bool| { home_back.0.accounts.reset_used(&home_back.1).unwrap(); true }) as Step,
            // Held on, his command ends: the hold is over.
            Box::new(move |stop: &dyn Fn() -> bool| {
                assert!(!stop(),"the hold was over while his command still held him on the refused account");
                *commands.lock().unwrap()=crate::lease_commands::CommandReading::Clear;
                assert!(stop(),"his command's end did not end the hold");
                false
            }),
        ]))});
        stu.host.set_retry_clock(clock.clone());

        stu.send("Input under test",1);

        assert_eq!(stu.deliveries("Input under test"),["1".to_string(),home.id.clone()],
            "the retry went back to the refused account: {:?}",stu.prompts.lock().unwrap());
        let waits=clock.waits.lock().unwrap().clone();
        let (half,hour,minute)=(std::time::Duration::from_secs(1800),std::time::Duration::from_secs(3600),std::time::Duration::from_secs(60));
        assert!(waits.len()==2 && waits[0]<=half && waits[0]>half-minute && waits[1]<=hour && waits[1]>hour-2*minute,
            "not a hold to Home's reset and then one to Account 1's own: {waits:?}");
        assert!(stu.failures().is_empty());
        stu.end();
    }

    /// **A refusal that resets does not release the one retry onto an account its readings still
    /// show used up** (review rv-20261029T140028Z-3e883526-10b9, finding 2). While Stu's turn
    /// runs on Account 1, another conversation is refused on Account 1 (Home put in use) and then
    /// on Home, for two hours; Account 1's lease has streamed a reading of 100% of its week,
    /// resetting in seven days. Stu's turn is refused on Account 1 for an hour, and the hold is
    /// to that hour. When it runs out Account 1's refusal is over, but its week is not: the input
    /// is not sent there. The hold goes on, to Home's reset, and the input then goes on Home.
    #[test]
    #[cfg(unix)]
    fn a_refusal_that_resets_does_not_release_the_retry_onto_an_account_used_up_by_its_readings() {
        let h=harness(0);
        let (quota,home)=account_one_and_home_with_room(&h);
        let stu=stu_with_a_usage_limit(h,quota.clone(),1);
        let elsewhere=(quota.clone(),home.id.clone());
        *stu.before_cut.lock().unwrap()=Some(Box::new(move || {
            let (quota,home)=elsewhere;
            let now=crate::util::now_millis();
            assert_eq!(quota.limit_reached("1",Some(now+3_600_000)),crate::claude_accounts::AfterLimit::Continue);
            assert_eq!(quota.limit_reached(&home,Some(now+2*3_600_000)),crate::claude_accounts::AfterLimit::NoRoom);
            // What Account 1's lease streams (`native::streamed_windows`): its week is used up.
            let week=crate::quota::Window {id:"seven_day".into(),label:"Weekly".into(),used_percent:100.,
                resets_at:Some(now+7*24*3_600_000),duration_ms:168*3_600_000};
            quota.before_turn("1",Some((vec![week],now+1)));
        }));
        let (refusal_over,home_back)=((quota.clone(),stu.resets_at),(quota.clone(),home.id.clone()));
        let clock=Arc::new(StepClock {waits:Mutex::new(Vec::new()),steps:Mutex::new(VecDeque::from([
            // Account 1's refusal resets: the hold runs out. Its week is still used up.
            Box::new(move |_: &dyn Fn() -> bool| {
                let (quota,resets_at)=refusal_over;
                assert_eq!(quota.accounts.limited_until("1"),Some(resets_at));
                quota.accounts.reset_used("1").unwrap();
                true
            }) as Step,
            // Held on, Home's refusal resets: the hold runs out again.
            Box::new(move |_: &dyn Fn() -> bool| { home_back.0.accounts.reset_used(&home_back.1).unwrap(); true }),
        ]))});
        stu.host.set_retry_clock(clock.clone());

        stu.send("Input under test",1);

        let waits=clock.waits.lock().unwrap().clone();
        let (hour,minute)=(std::time::Duration::from_secs(3600),std::time::Duration::from_secs(60));
        assert!(waits.len()==2 && waits[0]<=hour && waits[0]>hour-minute && waits[1]<=2*hour && waits[1]>2*hour-2*minute,
            "the retry was released when Account 1's refusal reset, its week still used up: {waits:?}");
        assert_eq!(stu.deliveries("Input under test"),["1".to_string(),home.id.clone()],
            "the one retry did not go on Home once Home had room: {:?}",stu.prompts.lock().unwrap());
        assert!(stu.failures().is_empty());
        stu.end();
    }

    /// **The hold ends when the refusal is lifted, not at the time it began with** (review of
    /// a6bd17fa5, P3). One account, held to its reset an hour away; a reset is used on it during
    /// the hold (`Accounts::reset_used`), and the input goes once more at that moment.
    #[test]
    fn a_reset_used_during_the_hold_sends_the_stu_turn_once_more_at_once() {
        let h=harness(0);
        let quota=Arc::new(crate::quota::Service::open(&h.root.join("quota")).unwrap());
        let stu=stu_with_a_usage_limit(h,quota.clone(),1);
        let clock=Arc::new(LiftedDuringHold::new(move || quota.accounts.reset_used("1").unwrap()));
        stu.host.set_retry_clock(clock.clone());

        stu.send("Input under test",1);

        assert_eq!(stu.deliveries("Input under test"),["1","1"],"the used reset did not end the hold");
        assert_eq!(clock.waits().len(),1);
        assert!(stu.failures().is_empty());
        stu.end();
    }

    /// **With no account that has room, the turn is held until the reset and sent once more**,
    /// on the existing retry clock, and never in a loop. One account: Stu's turn is refused by a
    /// usage limit that resets in an hour. The clock is asked for one wait, to that reset, his
    /// conversation is shown RichOS's own line about it, and the same input is then sent once.
    /// A second input refused both times is sent exactly twice and its failure reaches Rich once,
    /// as any failed turn's does.
    #[test]
    fn a_stu_turn_cut_by_a_usage_limit_with_no_account_left_is_held_until_the_reset_and_sent_once_more() {
        let h=harness(0);
        let quota=Arc::new(crate::quota::Service::open(&h.root.join("quota")).unwrap());
        let stu=stu_with_a_usage_limit(h,quota,1);

        stu.send("Input under test",1);

        assert_eq!(stu.deliveries("Input under test"),["1","1"],"held, then sent once more on the only account");
        let waits=stu.clock.0.lock().unwrap().clone();
        let (hour,minute)=(std::time::Duration::from_secs(3600),std::time::Duration::from_secs(60));
        assert!(waits.len()==1 && waits[0]<=hour && waits[0]>hour-minute,"not one wait until the reset an hour away: {waits:?}");
        assert!(stu.failures().is_empty(),"the retry after the reset succeeded, yet a failure was reported");
        let shown=stu.notices.0.lock().unwrap().clone();
        assert_eq!(shown.len(),1,"{shown:?}");
        assert_eq!(shown[0].0,"thread-one");
        assert!(shown[0].1.contains("Claude usage limit") && shown[0].1.contains("try that step again once"),"{shown:?}");

        // Refused both times: two deliveries, never a third, and one failure for Rich.
        stu.cut.store(2,Ordering::SeqCst);
        stu.send("Second input",2);
        assert_eq!(stu.deliveries("Second input"),["1","1"],"a usage limit after the retry was retried again");
        assert_eq!(stu.clock.0.lock().unwrap().len(),2,"one hold per input, never a loop");
        let failures=stu.failures();
        assert_eq!(failures.len(),1,"{failures:?}");
        assert!(failures[0].to_string().contains(crate::claude_accounts::USAGE_LIMIT),"{failures:?}");
        stu.end();
    }

    #[test]
    fn the_usage_limit_hold_is_said_in_minutes_then_hours() {
        let said=|seconds| usage_limit_hold_sentence(std::time::Duration::from_secs(seconds));
        assert!(said(0).ends_with("resets in 1 minute."));
        assert!(said(61).ends_with("resets in 2 minutes."));
        assert!(said(3600).ends_with("resets in 60 minutes."));
        assert!(said(90*60).ends_with("resets in about 2 hours."));
        assert!(said(5*3600).ends_with("resets in about 5 hours."));
    }

    /// Stu's lease as the native client is (`native.rs`): input of ours goes live through his
    /// session's handle (`send_input`, answered at once with the next scripted result, `end_turn`
    /// when none is left), Claude Code's own turns reach the between-turn sink, and every lease
    /// resumes the same session, as `claude --resume` does. Whatever reaches him is recorded with
    /// the account it reached him on.
    type Reached = Arc<Mutex<Vec<(String, String)>>>;
    type Answers = Arc<Mutex<VecDeque<Result<String, String>>>>;
    type SinkSlot = Arc<Mutex<Option<crate::cognition::BetweenTurnSink>>>;
    /// `prompted`: the next scripted result of a turn the run loop gives him (`prompt`),
    /// `end_turn` when none is left.
    struct NativeLease { account: String, sink: SinkSlot, reached: Reached, answers: Answers, prompted: Answers }
    impl Cognition for NativeLease {
        fn session_id(&self)->&str {"stu-native"}
        fn account(&self)->Option<&str> {Some(&self.account)}
        fn set_between_turn_sink(&mut self,sink:crate::cognition::BetweenTurnSink) {*self.sink.lock().unwrap()=Some(sink);}
        fn refresh_team(&mut self)->Result<bool,CognitionError> {Ok(true)}
        fn cancel_handle(&self)->Option<Arc<dyn TurnCancel>> {
            Some(Arc::new(NativeSend {account:self.account.clone(),reached:self.reached.clone(),answers:self.answers.clone()}))
        }
        fn prompt(&mut self,text:&str,_:&mut dyn FnMut(TurnItem))->Result<String,CognitionError> {
            self.reached.lock().unwrap().push((self.account.clone(),text.to_string()));
            self.prompted.lock().unwrap().pop_front().unwrap_or_else(|| Ok("end_turn".into())).map_err(CognitionError::Protocol)
        }
    }
    struct NativeSend { account: String, reached: Reached, answers: Answers }
    impl TurnCancel for NativeSend {
        fn cancel(&self)->bool {true}
        fn send_input(&self,_:&str,text:&str,completed:Box<dyn FnOnce(crate::steering::InputOutcome)+Send>)->Result<bool,String> {
            self.reached.lock().unwrap().push((self.account.clone(),text.to_string()));
            let result=self.answers.lock().unwrap().pop_front().unwrap_or_else(|| Ok("end_turn".into()));
            completed(crate::steering::InputOutcome {result,report_failure:true});
            Ok(true)
        }
    }
    struct NativeFactory { quota: Arc<crate::quota::Service>, sink: SinkSlot, reached: Reached, answers: Answers, prompted: Answers }
    impl LeaseFactory for NativeFactory {
        fn spawn(&self)->Result<Box<dyn Cognition>,CognitionError> {unreachable!()}
        fn spawn_work(&self,_:&ThreadBinding)->Result<Box<dyn Cognition>,CognitionError> {
            Ok(Box::new(NativeLease {account:self.quota.lease_account().id,sink:self.sink.clone(),
                reached:self.reached.clone(),answers:self.answers.clone(),prompted:self.prompted.clone()}))
        }
    }
    struct NativeStu { h: Harness, host: Arc<WorkHost>, quota: Arc<crate::quota::Service>, sink: SinkSlot,
        reached: Reached, answers: Answers, prompted: Answers, clock: Arc<RecordedRetryClock>, notices: Arc<RecordingNotifier> }
    /// Stu on the quota service `quota`, on a recorded retry clock, his lease opened on the
    /// account in use by a first message from Rich.
    fn native_stu(h: Harness, quota: Arc<crate::quota::Service>) -> NativeStu {
        let (sink,reached,answers,prompted): (SinkSlot,Reached,Answers,Answers)=Default::default();
        let notices=Arc::new(RecordingNotifier::default());
        let host=WorkHost::new(&h.state,Box::new(NativeFactory {quota:quota.clone(),sink:sink.clone(),
            reached:reached.clone(),answers:answers.clone(),prompted:prompted.clone()}),notices.clone());
        let clock=Arc::new(RecordedRetryClock::default());
        host.set_retry_clock(clock.clone());
        host.set_quota(quota.clone());
        let stu=NativeStu {h,host,quota,sink,reached,answers,prompted,clock,notices};
        crate::session_tools::bind(&stu.status(),&stu.scope(),"front","rich",None).unwrap();
        crate::session_tools::send(&stu.status(),&stu.scope(),serde_json::json!({"text":"First message"})).unwrap();
        stu.host.wake_messages(&stu.h.binding);
        assert!(stu.host.wait_for_completed(1,std::time::Duration::from_secs(30)),"Stu's first turn did not finish");
        stu
    }
    impl NativeStu {
        fn status(&self)->PathBuf {self.h.root.join("peer-status.json")}
        fn scope(&self)->crate::status_tools::StatusToolScope {self.host.message_scope(&self.h.binding)}
        fn backend(&self)->Arc<Backend> {self.host.backend(self.h.binding.thread_id()).expect("Stu's back end")}
        /// Rich hands Stu `text`, given live into a turn of his on `stu-native` as `wake_messages`
        /// gives it; returns the completion the native reader calls when that turn ends.
        fn live(&self,text:&str)->Box<dyn FnOnce(crate::steering::InputOutcome)+Send> {
            let backend=self.backend();
            // Held while the input is saved and marked given, so the run loop never takes it too.
            let mut inner=backend.inner.lock().unwrap();
            crate::session_tools::send(&self.status(),&self.scope(),serde_json::json!({"text":text})).unwrap();
            let (message,through)=crate::session_tools::pending_after(&self.scope(),"stu",0).unwrap().expect("the saved input");
            inner.note_given("stu-native",through);
            inner.peer_active_through=Some(through);
            drop(inner);
            crate::session_tools::begin_attempt(&self.scope(),"stu",&format!("peer:{through}"),"stu-native").unwrap();
            let mut payload=serde_json::to_value(&message).unwrap();
            payload["previous_delivery_attempt"]=serde_json::Value::Null;
            self.host.live_completion(&backend,&self.h.binding,message,(InputReceipt::Peer(through),Some("stu-native".into())),payload.to_string())
        }
        /// What reached Stu with `text` in it, as (account, what was sent), in order.
        fn reached(&self,text:&str)->Vec<(String,String)> {
            self.reached.lock().unwrap().iter().filter(|(_,said)|said.contains(text)).cloned().collect()
        }
        fn accounts(&self,text:&str)->Vec<String> {self.reached(text).into_iter().map(|(account,_)|account).collect()}
        /// A failure is waiting for Rich (read without taking it).
        fn rich_told(&self)->bool {crate::session_tools::pending(&self.scope(),"rich").unwrap().is_some()}
        fn failures(&self)->Vec<serde_json::Value> {
            let scope=self.scope();
            let mut facts=Vec::new();
            while let Some((message,through))=crate::session_tools::pending(&scope,"rich").unwrap() {
                facts.push(serde_json::from_str::<serde_json::Value>(&message.text).unwrap());
                crate::session_tools::acknowledge(&scope,"rich",through).unwrap();
            }
            facts
        }
        /// Claude Code runs an automatic turn of Stu's (a helper hands back) that a usage limit
        /// cuts, as the native reader reports it: a rejected `rate_limit_event` naming the reset
        /// (`resets_at`, in milliseconds; the wire says seconds), then the 429 result.
        fn automatic_turn_cut(&self,resets_at:u64) {
            let sink=self.sink.lock().unwrap().clone().expect("the host installs a between-turn sink");
            sink(vec![serde_json::json!({"type":"system","subtype":"task_notification","task_id":"helper","status":"completed"}),
                serde_json::json!({"type":"assistant","message":{"content":[{"type":"text","text":"The helper handed back. Reviewing its work."}]}}),
                serde_json::json!({"type":"rate_limit_event","rate_limit_info":{"status":"rejected","resetsAt":resets_at/1000}}),
                serde_json::json!({"type":"result","subtype":"success","is_error":true,"api_error_status":429,"errors":["You've hit your session limit"]})]);
        }
        fn end(self) {
            self.host.shutdown();
            std::fs::remove_dir_all(&self.h.root).unwrap();
        }
    }
    fn session_limit(resets_at: u64) -> Result<String, String> {
        Err(crate::claude_accounts::usage_limit_error(Some(resets_at),"[\"You've hit your session limit\"]"))
    }
    fn about_an_hour(waits: &[std::time::Duration]) -> bool {
        let (hour,minute)=(std::time::Duration::from_secs(3600),std::time::Duration::from_secs(60));
        waits.len()==1 && waits[0]<=hour && waits[0]>hour-minute
    }

    /// **Live input whose turn a usage limit cut is held until the reset and sent once more**
    /// (second review rv-20261029T111217Z-71fb5f11-8f6b, P1: the recovery was on `run_message`
    /// only). One account. Rich's message reaches Stu live, into a turn the limit cuts with a
    /// reset an hour away. The retry clock is asked for one wait, to that reset, his conversation
    /// shows RichOS's line about it, and the same input goes once more, live, naming the cut
    /// attempt; Rich is told of no failure. Cut again, it is not sent a third time: its failure
    /// reaches Rich once.
    #[test]
    fn a_live_input_cut_by_a_usage_limit_is_held_until_the_reset_and_sent_once_more() {
        let h=harness(0);
        let quota=Arc::new(crate::quota::Service::open(&h.root.join("quota")).unwrap());
        let stu=native_stu(h,quota);
        let in_an_hour=|| crate::util::now_millis()+3_600_000;

        stu.live("Live input under test")(crate::steering::InputOutcome {result:session_limit(in_an_hour()),report_failure:true});
        // Waits for the input to be closed, not for it to reach Stu: the retry's completion
        // closes it on the usage-limit thread after the send, so a read of `reached` alone could
        // come before that on a slow machine. It is open from `live` until then (`begin_attempt`).
        until("the cut live input was neither closed nor reported",
            ||crate::session_tools::pending_after(&stu.scope(),"stu",0).unwrap().is_none()||stu.rich_told());

        assert_eq!(stu.accounts("Live input under test"),["1"],"not sent once more, live, after the hold");
        let retried: serde_json::Value=serde_json::from_str(&stu.reached("Live input under test")[0].1).unwrap();
        assert_eq!(retried["previous_delivery_attempt"]["session_id"],"stu-native","the retry does not name the cut attempt: {retried}");
        assert!(about_an_hour(&stu.clock.0.lock().unwrap()),"not one hold to the reset: {:?}",stu.clock.0.lock().unwrap());
        let shown=stu.notices.0.lock().unwrap().clone();
        assert!(shown.len()==1 && shown[0].1.contains("Claude usage limit"),"{shown:?}");
        assert!(stu.failures().is_empty(),"a live input sent once more was reported as failed");
        assert!(crate::session_tools::pending_after(&stu.scope(),"stu",0).unwrap().is_none(),"the input that went once more is still open");

        // Cut both times: sent twice, never a third time, and its failure reaches Rich once.
        stu.answers.lock().unwrap().push_back(session_limit(in_an_hour()));
        stu.live("Second live input")(crate::steering::InputOutcome {result:session_limit(in_an_hour()),report_failure:true});
        until("the second input's failure did not reach Rich",||stu.rich_told());
        assert_eq!(stu.accounts("Second live input"),["1"],"a usage limit after the retry was retried again");
        assert_eq!(stu.clock.0.lock().unwrap().len(),2,"one hold per input, never a loop");
        let failures=stu.failures();
        assert_eq!(failures.len(),1,"{failures:?}");
        assert!(failures[0].to_string().contains(crate::claude_accounts::USAGE_LIMIT),"{failures:?}");
        stu.end();
    }

    /// **Live input whose turn a usage limit cut goes once more, at once, on the next account
    /// with room** (the same review, P1). Account 1 and Home both have room by their readings;
    /// the turn Rich's message went into on Account 1 is cut. Home is put in use, Stu's session
    /// moves there (nothing of his runs), and the same input reaches him once more there, live,
    /// with no hold.
    #[test]
    #[cfg(unix)]
    fn a_live_input_cut_by_a_usage_limit_goes_once_more_on_the_next_account_with_room() {
        let h=harness(0);
        let (quota,home)=account_one_and_home_with_room(&h);
        let stu=native_stu(h,quota);

        stu.live("Live input under test")(crate::steering::InputOutcome {
            result:session_limit(crate::util::now_millis()+3_600_000),report_failure:true});
        until("the cut live input was neither sent once more nor reported",||!stu.reached("Live input under test").is_empty()||stu.rich_told());

        assert_eq!(stu.accounts("Live input under test"),std::slice::from_ref(&home.id),"not sent once more on Home");
        assert_eq!(stu.quota.accounts.in_use().id,home.id,"Home is in use");
        assert!(stu.quota.accounts.limited_until("1").is_some(),"Account 1 is gone until its reset");
        assert_eq!(*stu.clock.0.lock().unwrap(),[std::time::Duration::ZERO],"Home had room: no hold");
        assert!(stu.notices.0.lock().unwrap().is_empty(),"nothing is held, so no hold is shown");
        assert!(stu.failures().is_empty());
        stu.end();
    }

    /// **An automatic turn of Stu's that a usage limit cut is held until the reset, and he is
    /// told once to carry on** (the same review, P1: helper hand-backs are automatic turns).
    /// One account. A helper hands back and the turn Claude Code starts for it is refused: a
    /// rejected `rate_limit_event` naming a reset an hour away, then the 429 result. Rich is not
    /// told of a failure; the retry clock is asked for one wait, to that reset, his conversation
    /// shows RichOS's line about it, and Stu then gets one `turn_usage_limit` message.
    #[test]
    fn an_automatic_turn_cut_by_a_usage_limit_is_held_until_the_reset_and_stu_is_told_once() {
        let h=harness(0);
        let quota=Arc::new(crate::quota::Service::open(&h.root.join("quota")).unwrap());
        let stu=native_stu(h,quota);

        stu.automatic_turn_cut(crate::util::now_millis()+3_600_000);
        until("the cut automatic turn was neither answered nor reported",||!stu.reached("turn_usage_limit").is_empty()||stu.rich_told());

        assert_eq!(stu.accounts("turn_usage_limit"),["1"],"Stu was not told once to carry on");
        assert!(about_an_hour(&stu.clock.0.lock().unwrap()),"not one hold to the reset: {:?}",stu.clock.0.lock().unwrap());
        let shown=stu.notices.0.lock().unwrap().clone();
        assert!(shown.len()==1 && shown[0].1.contains("Claude usage limit"),"{shown:?}");
        assert!(stu.failures().is_empty(),"an automatic turn that was answered was reported as failed");
        assert!(stu.quota.accounts.limited_until("1").is_some(),"Account 1 was not marked gone");
        stu.end();
    }

    /// **An automatic turn of Stu's that a usage limit cut moves him at once to the next account
    /// with room** (the same review, P1). Account 1 and Home both have room by their readings;
    /// the automatic turn on Account 1 is refused. Home is put in use and he is on Home, told
    /// so there once (`account_moved`, which tells him to carry on): no hold, no second message,
    /// no failure for Rich.
    #[test]
    #[cfg(unix)]
    fn an_automatic_turn_cut_by_a_usage_limit_moves_stu_to_the_next_account_with_room() {
        let h=harness(0);
        let (quota,home)=account_one_and_home_with_room(&h);
        let stu=native_stu(h,quota);

        stu.automatic_turn_cut(crate::util::now_millis()+3_600_000);
        until("Stu was neither moved nor the failure reported",||!stu.reached("account_moved").is_empty()||stu.rich_told());
        until("Stu's inbox was not emptied",||!crate::session_tools::undelivered(&stu.scope(),"stu").unwrap());

        assert_eq!(stu.accounts("account_moved"),std::slice::from_ref(&home.id),"not moved to Home, once");
        assert!(stu.reached("turn_usage_limit").is_empty(),"moved Stu was told a second time");
        assert_eq!(stu.quota.accounts.in_use().id,home.id);
        assert!(stu.quota.accounts.limited_until("1").is_some(),"Account 1 was not marked gone");
        assert_eq!(*stu.clock.0.lock().unwrap(),[std::time::Duration::ZERO],"Home had room: no hold");
        assert!(stu.failures().is_empty());
        stu.end();
    }

    /// **The message that carries an automatic turn on has had the one retry** (review
    /// rv-20261029T140028Z-3e883526-10b9, finding 1: `turn_usage_limit` reached `run_message`
    /// with a fresh retry of its own, so a second refusal was held and sent a third time). One
    /// account. A helper hands back and the automatic turn Claude Code starts for it is refused,
    /// with a reset an hour away: held to it, and Stu gets one `turn_usage_limit` message. The
    /// turn that message starts is refused too. It is not held or sent again: its failure
    /// reaches Rich, once.
    #[test]
    fn the_message_that_carries_a_cut_automatic_turn_on_is_not_retried_after_its_own_usage_limit() {
        let h=harness(0);
        let quota=Arc::new(crate::quota::Service::open(&h.root.join("quota")).unwrap());
        let stu=native_stu(h,quota);
        stu.prompted.lock().unwrap().push_back(session_limit(crate::util::now_millis()+3_600_000));

        stu.automatic_turn_cut(crate::util::now_millis()+3_600_000);
        until("the refused continuation was neither reported nor sent again",
            ||stu.rich_told()||stu.reached("turn_usage_limit").len()>1);

        assert_eq!(stu.accounts("turn_usage_limit"),["1"],"the continuation was held and sent again after its own refusal");
        assert!(about_an_hour(&stu.clock.0.lock().unwrap()),"not one hold, the automatic turn's: {:?}",stu.clock.0.lock().unwrap());
        let failures=stu.failures();
        assert_eq!(failures.len(),1,"{failures:?}");
        assert!(failures[0].to_string().contains(crate::claude_accounts::USAGE_LIMIT),"{failures:?}");
        stu.end();
    }

    /// **The `account_moved` that carries a cut automatic turn on has had the one retry too**
    /// (review rv-20261029T150350Z-fc3e4c65-03ea: moved to an account with room, the recovery's
    /// one message is `account_moved`, which `usage_limit_retry_spent` did not read, so its own
    /// refusal was held and it went a third time). Account 1 and Home both have room by their
    /// readings. A helper hands back and the automatic turn on Account 1 is refused: Stu is moved
    /// to Home and told so there once. The turn that message starts is refused too. It is not
    /// held or sent again: its failure reaches Rich, once.
    #[test]
    #[cfg(unix)]
    fn the_account_moved_that_carries_a_cut_automatic_turn_on_is_not_retried_after_its_own_usage_limit() {
        let h=harness(0);
        let (quota,home)=account_one_and_home_with_room(&h);
        let stu=native_stu(h,quota);
        stu.prompted.lock().unwrap().push_back(session_limit(crate::util::now_millis()+3_600_000));

        stu.automatic_turn_cut(crate::util::now_millis()+3_600_000);
        until("the refused continuation was neither reported nor sent again",
            ||stu.rich_told()||stu.reached("account_moved").len()>1);

        assert_eq!(stu.accounts("account_moved"),std::slice::from_ref(&home.id),
            "the continuation was held and sent again after its own refusal: {:?}",stu.reached("account_moved"));
        assert_eq!(*stu.clock.0.lock().unwrap(),[std::time::Duration::ZERO],"not one hold, the automatic turn's");
        let failures=stu.failures();
        assert_eq!(failures.len(),1,"{failures:?}");
        assert!(failures[0].to_string().contains(crate::claude_accounts::USAGE_LIMIT),"{failures:?}");
        stu.end();
    }

    /// **An ordinary `account_moved` still has its one retry** (the same review: only the one
    /// the usage-limit recovery sends has had it). Account 1 and Home both have room by their
    /// readings; Account 1 is then refused with no turn of Stu's involved, so Home is put in use
    /// and he moves there on the ordinary path, told so once. The turn that message starts is
    /// refused on Home, with Account 1 still gone: it is held, once, and the same message goes
    /// once more. Nothing reaches Rich.
    #[test]
    #[cfg(unix)]
    fn an_ordinary_account_moved_still_has_its_one_usage_limit_retry() {
        let h=harness(0);
        let (quota,home)=account_one_and_home_with_room(&h);
        let stu=native_stu(h,quota);
        // Both refusals reset at the same whole second, an hour away: one wait reaches both.
        let reset=crate::util::now_millis()/1000*1000+3_600_000;
        stu.quota.limit_reached("1",Some(reset));
        assert_eq!(stu.quota.accounts.in_use().id,home.id,"Home is in use");
        stu.prompted.lock().unwrap().push_back(session_limit(reset));

        // The run loop's idle check (`ACCOUNT_CHECK_EVERY`), brought forward.
        let backend=stu.backend();
        backend.inner.lock().unwrap().move_check=Some(false);
        backend.wake.notify_all();
        until("the ordinary move was neither sent once more nor reported",
            ||stu.rich_told()||stu.reached("account_moved").len()>1);

        let moved=stu.reached("account_moved");
        assert_eq!(moved.len(),2,"the ordinary account_moved was not sent once more after its refusal: {moved:?}");
        assert_eq!(moved[0].0,home.id,"not told on Home: {moved:?}");
        let fact=|said: &str| serde_json::from_str::<serde_json::Value>(said).ok()
            .and_then(|said| said["text"].as_str().map(str::to_string)).unwrap_or_else(|| said.to_string());
        assert_eq!(fact(&moved[1].1),fact(&moved[0].1),"the second delivery is not the same message: {moved:?}");
        assert!(about_an_hour(&stu.clock.0.lock().unwrap()),"not one hold to the reset: {:?}",stu.clock.0.lock().unwrap());
        assert!(stu.failures().is_empty(),"an ordinary move sent once more was reported as failed");
        stu.end();
    }

    /// **Input a cut turn was given is done once a later turn of the same session ends normally**
    /// (Otto's trace of 2026-10-29: messages Stu had already acted on came back at 11:44Z, after
    /// a restart). Message A goes live into a turn that a usage limit cuts (no quota service, so
    /// no retry: the case before this branch). The next turn on the same session, message B's,
    /// ends with `end_turn`. A fresh WorkHost on the same state, as after a restart, has nothing
    /// to hand Stu again: before this, `pending_after` gave A back, naming its earlier attempt.
    #[test]
    fn input_a_cut_turn_was_given_is_done_once_a_later_turn_of_the_session_ends_normally() {
        let h=harness(0);
        let scope=h.host.message_scope(&h.binding);
        let path=h.root.join("peer-status.json");
        crate::session_tools::bind(&path,&scope,"front","rich",None).unwrap();
        let backend=Backend::new();
        let session="stu-live".to_string();
        backend.inner.lock().unwrap().lease_session=Some(session.clone());
        let live=|text: &str, after: u64| {
            crate::session_tools::send(&path,&scope,serde_json::json!({"text":text})).unwrap();
            let (message,through)=crate::session_tools::pending_after(&scope,"stu",after).unwrap().unwrap();
            crate::session_tools::begin_attempt(&scope,"stu",&format!("peer:{through}"),&session).unwrap();
            let completed=h.host.live_completion(&backend,&h.binding,message,(InputReceipt::Peer(through),Some(session.clone())),text.into());
            (completed,through)
        };

        let (cut,a)=live("Message A",0);
        cut(crate::steering::InputOutcome {result:Err(crate::claude_accounts::usage_limit_error(None,"[\"You've hit your session limit\"]")),report_failure:true});
        assert!(crate::session_tools::previous_attempt(&scope,"stu",&format!("peer:{a}")).unwrap().is_some(),"A was not left received");
        let (next,b)=live("Message B",a);
        next(crate::steering::InputOutcome {result:Ok("end_turn".into()),report_failure:true});
        h.host.shutdown();

        let fresh=WorkHost::new(&h.state,Box::new(Factory {spawns:Arc::new(AtomicUsize::new(0)),prompts:Arc::new(Mutex::new(vec![])),
            commands:Arc::new(Mutex::new(None)),team:Arc::new(Mutex::new(TeamScript::default())),gate:StartGate::open_now()}),
            Arc::new(RecordingNotifier::default()));
        let fresh_scope=fresh.message_scope(&h.binding);
        let left=crate::session_tools::pending_after(&fresh_scope,"stu",0).unwrap();
        assert!(left.is_none(),"handed to Stu again after a restart: {:?}",left.map(|(message,through)| (message.text,
            crate::session_tools::previous_attempt(&fresh_scope,"stu",&format!("peer:{through}")).unwrap())));
        assert!(crate::session_tools::previous_attempt(&fresh_scope,"stu",&format!("peer:{a}")).unwrap().is_none());
        assert!(crate::session_tools::previous_attempt(&fresh_scope,"stu",&format!("peer:{b}")).unwrap().is_none());
        fresh.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **Input no turn ran is still handed back after a restart**, though a later turn of the
    /// same session ended normally: the child threw it away before it ran (`discarded`,
    /// `refused`), a Stop ended it, or the child went away mid-turn and may never have read it.
    /// Stu may not have it, so the restart's reconciliation stays ([`WorkHost::reason_ran`]).
    #[test]
    fn input_no_turn_ran_is_handed_back_after_a_restart_though_a_later_turn_ended_normally() {
        for outcome in [Ok(crate::native::STOP_REASON_DISCARDED.to_string()), Ok(crate::native::STOP_REASON_REFUSED.to_string()),
            Ok(crate::native::STOP_REASON_CANCELLED.to_string()),
            Err(crate::native::NativeError::Exited {said:"it wrote nothing on stderr".into()}.to_string()),
            Err(crate::native::NativeError::Closed.to_string())] {
            let h=harness(0);
            let scope=h.host.message_scope(&h.binding);
            let path=h.root.join("peer-status.json");
            crate::session_tools::bind(&path,&scope,"front","rich",None).unwrap();
            let backend=Backend::new();
            let session="stu-live".to_string();
            backend.inner.lock().unwrap().lease_session=Some(session.clone());
            let live=|text: &str, after: u64| {
                crate::session_tools::send(&path,&scope,serde_json::json!({"text":text})).unwrap();
                let (message,through)=crate::session_tools::pending_after(&scope,"stu",after).unwrap().unwrap();
                crate::session_tools::begin_attempt(&scope,"stu",&format!("peer:{through}"),&session).unwrap();
                let completed=h.host.live_completion(&backend,&h.binding,message,(InputReceipt::Peer(through),Some(session.clone())),text.into());
                (completed,through)
            };

            let (never,a)=live("Message A",0);
            never(crate::steering::InputOutcome {result:outcome.clone(),report_failure:true});
            let (next,_)=live("Message B",a);
            next(crate::steering::InputOutcome {result:Ok("end_turn".into()),report_failure:true});
            h.host.shutdown();

            let fresh=WorkHost::new(&h.state,Box::new(Factory {spawns:Arc::new(AtomicUsize::new(0)),prompts:Arc::new(Mutex::new(vec![])),
                commands:Arc::new(Mutex::new(None)),team:Arc::new(Mutex::new(TeamScript::default())),gate:StartGate::open_now()}),
                Arc::new(RecordingNotifier::default()));
            let fresh_scope=fresh.message_scope(&h.binding);
            let left=crate::session_tools::pending_after(&fresh_scope,"stu",0).unwrap();
            assert_eq!(left.map(|(message,through)| (message.text.contains("Message A"),through)),Some((true,a)),
                "{outcome:?}: input no turn ran was not handed back after a restart");
            fresh.shutdown();
            std::fs::remove_dir_all(h.root).unwrap();
        }
    }

    /// The `worker_overload` (and `turn_overload`) facts Stu has been sent, in order.
    fn overload_notices(h: &Harness) -> Vec<serde_json::Value> {
        h.work_prompts.lock().unwrap().iter().filter_map(|prompt| {
            let message: serde_json::Value = serde_json::from_str(prompt).ok()?;
            (message["from"] == "overload-retry").then(|| serde_json::from_str::<serde_json::Value>(message["text"].as_str()?).ok()).flatten()
        }).collect()
    }

    fn worker_overloaded(call: &str) -> serde_json::Value {
        serde_json::json!({"type":"assistant","parent_tool_use_id":call,"error":"overloaded",
            "message":{"content":[{"type":"text","text":"API Error: 529 Overloaded."}]}})
    }

    /// A retry clock that holds each wait open until the test ends it, so an overload can arrive
    /// DURING a wait. It writes down each wait it was asked for.
    #[derive(Default)]
    struct HeldRetryClock { waits: Mutex<Vec<std::time::Duration>>, ended: Mutex<usize>, turn: Condvar }
    impl HeldRetryClock {
        fn waits(&self) -> Vec<std::time::Duration> { self.waits.lock().unwrap().clone() }
        fn end_one(&self) { *self.ended.lock().unwrap() += 1; self.turn.notify_all(); }
    }
    impl crate::upstream::RetryClock for HeldRetryClock {
        fn wait(&self, wait: std::time::Duration, stop: &dyn Fn() -> bool) -> bool {
            let mine = { let mut waits = self.waits.lock().unwrap(); waits.push(wait); waits.len() };
            let mut ended = self.ended.lock().unwrap();
            while *ended < mine {
                if stop() { return false; }
                ended = self.turn.wait_timeout(ended, std::time::Duration::from_millis(10)).unwrap().0;
            }
            !stop()
        }
    }

    /// Follow-up 11, ONE clock: an overload hits every running worker at once. Eight workers
    /// overloaded together wait one 1-minute wait and each gets the restart message when it
    /// ends; a ninth overload during that wait joins it, and a worker reported twice is told
    /// once. The next overload after the wait moves one step, to 2 minutes.
    #[test]
    fn eight_workers_overloaded_together_wait_one_minute_and_all_are_told_to_restart() {
        let h = harness(0);
        let clock = Arc::new(HeldRetryClock::default());
        h.host.set_retry_clock(clock.clone());
        let backend = Backend::new();
        backend.inner.lock().unwrap().lease_session = Some("stu-session".into());
        let observe = |frame: serde_json::Value| h.host.observe_worker(&backend, &h.binding, "stu-session", &frame);
        let minute = std::time::Duration::from_secs(60);
        for n in 1..=8 { observe(worker_overloaded(&format!("W{n}"))); }
        until("the workers' overload did not start a wait", || clock.waits().len() == 1);
        observe(worker_overloaded("W9"));
        observe(worker_overloaded("W1"));
        assert_eq!(clock.waits(), [minute], "an overload during the wait started a wait of its own");
        assert!(overload_notices(&h).is_empty(), "Stu was told before the wait ended");
        clock.end_one();
        until("Stu was not told to restart every stopped worker", || overload_notices(&h).len() == 9);
        let mut told: Vec<_> = overload_notices(&h).iter().map(|fact| (fact["worker"]["tool_use_id"].as_str().unwrap_or("").to_string(),
            fact["event"].as_str().map(str::to_string), fact["retry"].as_u64(), fact["waited_minutes"].as_u64(), fact["retries_used_up"].as_bool())).collect();
        told.sort();
        let expected: Vec<_> = (1..=9).map(|n| (format!("W{n}"), Some("worker_overload".to_string()), Some(1), Some(1), Some(false))).collect();
        assert_eq!(told, expected, "the stopped workers did not all wait the one 1-minute wait");
        observe(worker_overloaded("W1"));
        until("the next overload did not start a wait", || clock.waits().len() == 2);
        assert_eq!(clock.waits(), [minute, 2 * minute], "the next overload after the wait did not move one step");
        clock.end_one();
        until("Stu was not told after the second wait", || overload_notices(&h).len() == 10);
        let last = overload_notices(&h).pop().unwrap();
        assert_eq!((&last["worker"]["tool_use_id"], &last["retry"], &last["waited_minutes"]),
            (&serde_json::json!("W1"), &serde_json::json!(2), &serde_json::json!(2)), "{last}");
        backend.inner.lock().unwrap().closing = true;
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// Follow-up 11, ONE clock: any worker's ordinary answer from the API resets it, so the next
    /// overload, whichever worker it stops, waits the first minute again.
    #[test]
    fn a_workers_success_resets_the_workers_overload_clock_to_one_minute() {
        let h = harness(0);
        let clock = Arc::new(RecordedRetryClock::default());
        h.host.set_retry_clock(clock.clone());
        let backend = Backend::new();
        backend.inner.lock().unwrap().lease_session = Some("stu-session".into());
        let observe = |frame: serde_json::Value| h.host.observe_worker(&backend, &h.binding, "stu-session", &frame);
        observe(worker_overloaded("A"));
        until("Stu was not told after the first wait", || overload_notices(&h).len() == 1);
        observe(worker_overloaded("B"));
        until("Stu was not told after the second wait", || overload_notices(&h).len() == 2);
        observe(serde_json::json!({"type":"assistant","parent_tool_use_id":"C",
            "message":{"content":[{"type":"text","text":"Reading the files again."}]}}));
        observe(worker_overloaded("A"));
        until("Stu was not told after the third wait", || overload_notices(&h).len() == 3);
        let minute = std::time::Duration::from_secs(60);
        assert_eq!(*clock.0.lock().unwrap(), [minute, 2 * minute, minute], "a worker's success did not reset the clock");
        let told: Vec<_> = overload_notices(&h).iter().map(|fact| (fact["worker"]["tool_use_id"].clone(), fact["retry"].clone())).collect();
        assert_eq!(told, [(serde_json::json!("A"), serde_json::json!(1)), (serde_json::json!("B"), serde_json::json!(2)),
            (serde_json::json!("A"), serde_json::json!(1))]);
        backend.inner.lock().unwrap().closing = true;
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// Follow-up 11: overloads one after another, whichever worker each stops (Stu restarts a
    /// worker as a new run), wait 1, 2, 5, 10, 20, 40 and 80 minutes and Stu is told after each
    /// wait; the eighth is told at once, with no retries left. A frame Claude Code did not mark
    /// as an API error is the worker's own words.
    #[test]
    fn a_workers_eighth_overload_in_a_row_tells_stu_its_retries_are_used_up() {
        let h = harness(0);
        let clock = Arc::new(RecordedRetryClock::default());
        h.host.set_retry_clock(clock.clone());
        let backend = Backend::new();
        backend.inner.lock().unwrap().lease_session = Some("stu-session".into());
        let quoted = serde_json::json!({"type":"assistant","parent_tool_use_id":"run-1",
            "message":{"content":[{"type":"text","text":"API Error: 529 Overloaded."}]}});
        h.host.observe_worker(&backend, &h.binding, "stu-session", &quoted);
        for n in 1..=8 {
            h.host.observe_worker(&backend, &h.binding, "stu-session", &worker_overloaded(&format!("run-{n}")));
            until("Stu was not told about his worker's overload", || overload_notices(&h).len() == n);
        }
        let told: Vec<_> = overload_notices(&h).iter().map(|fact| (fact["retry"].as_u64(), fact["retries_left"].clone(), fact["waited_minutes"].as_u64())).collect();
        let mut expected: Vec<_> = [1, 2, 5, 10, 20, 40, 80].iter().enumerate()
            .map(|(k, minutes)| (Some(k as u64 + 1), serde_json::json!(6 - k), Some(*minutes as u64))).collect();
        expected.push((None, serde_json::json!(0), None));
        assert_eq!(told, expected);
        assert_eq!(*clock.0.lock().unwrap(), crate::upstream::OVERLOAD_RETRY_WAITS, "the quoted line was taken for an overload, or a wait was skipped");
        backend.inner.lock().unwrap().closing = true;
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// Review rv-20261018T200657Z-585c24df-f9d6, finding 2: the seventh overload is granted the
    /// 80-minute wait and its retry still runs. Only the eighth, which is granted none, says the
    /// retries are used up, and that is what Stu's instruction reports to Rich.
    #[test]
    fn the_seventh_worker_retry_runs_and_only_the_eighth_overload_ends_the_schedule() {
        let h = harness(0);
        h.host.set_retry_clock(Arc::new(RecordedRetryClock::default()));
        let backend = Backend::new();
        backend.inner.lock().unwrap().lease_session = Some("stu-session".into());
        for n in 1..=8 {
            h.host.observe_worker(&backend, &h.binding, "stu-session", &worker_overloaded("worker-call"));
            until("Stu was not told about his worker's overload", || overload_notices(&h).len() == n);
        }
        let told = overload_notices(&h);
        let seventh = told.iter().find(|fact| fact["retry"] == 7).expect("no seventh retry");
        assert_eq!((&seventh["waited_minutes"], &seventh["retries_used_up"]), (&serde_json::json!(80), &serde_json::json!(false)),
            "the granted seventh retry was marked as the end: {seventh}");
        let eighth = told.iter().find(|fact| fact["retry"].is_null()).expect("no eighth overload");
        assert_eq!(eighth["retries_used_up"], true, "the eighth overload, granted no retry, did not end the schedule: {eighth}");
        let stu = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app/agents/stu.md")).unwrap();
        assert!(stu.contains("`retries_used_up`") && !stu.contains("with no retries left means"),
            "stu.md does not report the failure on the fact the host sets only when no retry is granted");
        backend.inner.lock().unwrap().closing = true;
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn a_live_overload_wait_releases_the_native_reader_and_does_not_block_other_input() {
        use std::sync::mpsc;
        struct HeldClock { entered: mpsc::Sender<()>, release: Mutex<mpsc::Receiver<()>> }
        impl crate::upstream::RetryClock for HeldClock {
            fn wait(&self, wait: std::time::Duration, stop: &dyn Fn() -> bool) -> bool {
                assert_eq!(wait, std::time::Duration::from_secs(60));
                self.entered.send(()).unwrap();
                // Hang guard only: the test releases the actual retry, never elapsed time.
                self.release.lock().unwrap().recv_timeout(std::time::Duration::from_secs(30)).unwrap();
                !stop()
            }
        }
        struct NativeInput { calls: Arc<Mutex<Vec<String>>>, finished: mpsc::Sender<()> }
        impl TurnCancel for NativeInput {
            fn cancel(&self) -> bool { true }
            fn send_input(&self, _: &str, text: &str, completed: Box<dyn FnOnce(crate::steering::InputOutcome) + Send>) -> Result<bool,String> {
                self.calls.lock().unwrap().push(text.to_string());
                completed(crate::steering::InputOutcome { result: Ok("end_turn".into()), report_failure:true });
                if text == "Original live input" { self.finished.send(()).unwrap(); }
                Ok(true)
            }
        }
        let h = harness(0);
        let scope = h.host.message_scope(&h.binding);
        let path = h.root.join("peer-status.json");
        crate::session_tools::bind(&path, &scope, "front", "rich", None).unwrap();
        crate::session_tools::send(&path, &scope, serde_json::json!({"text":"Original live input"})).unwrap();
        let (message, through) = crate::session_tools::pending(&scope, "stu").unwrap().unwrap();
        let backend = Backend::new();
        let session = "existing-live-stu".to_string();
        backend.inner.lock().unwrap().lease_session = Some(session.clone());
        crate::session_tools::begin_attempt(&scope, "stu", &format!("peer:{through}"), &session).unwrap();
        let (entered, waiting) = mpsc::channel();
        let (release, released) = mpsc::channel();
        h.host.set_retry_clock(Arc::new(HeldClock { entered, release:Mutex::new(released) }));
        let calls = Arc::new(Mutex::new(Vec::new()));
        let (finished, done) = mpsc::channel();
        let native = Arc::new(NativeInput { calls:calls.clone(), finished });
        backend.inner.lock().unwrap().peer_stopper = Some((session.clone(), native.clone()));
        let completed = h.host.live_completion(&backend, &h.binding, message,
            (InputReceipt::Peer(through), Some(session)), "Original live input".into());
        let (returned, reader_returned) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            completed(crate::steering::InputOutcome { result:Err("API Error: 529 Overloaded.".into()), report_failure:true });
            returned.send(()).unwrap();
        });
        // Hang guards: callback return and retry admission are separate positive events.
        waiting.recv_timeout(std::time::Duration::from_secs(30)).unwrap();
        reader_returned.recv_timeout(std::time::Duration::from_secs(30)).unwrap();
        reader.join().unwrap();
        assert!(calls.lock().unwrap().is_empty());
        assert!(native.send_input("independent", "Independent later input", Box::new(|outcome|assert!(outcome.result.is_ok()))).unwrap());
        release.send(()).unwrap();
        // Hang guard: completion comes from the retry's actual native callback.
        done.recv_timeout(std::time::Duration::from_secs(30)).unwrap();
        assert_eq!(*calls.lock().unwrap(), ["Independent later input", "Original live input"]);
        assert!(crate::session_tools::pending(&scope, "rich").unwrap().is_none());
        assert!(crate::session_tools::pending(&scope, "stu").unwrap().is_none());
        backend.inner.lock().unwrap().closing = true;
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn native_retry_activity_does_not_create_an_app_retry_or_failure_report() {
        let h=harness(0);let scope=h.host.message_scope(&h.binding);let path=h.root.join("peer-status.json");
        crate::session_tools::bind(&path,&scope,"front","rich",None).unwrap();
        h.team.lock().unwrap().turn_results.push_back("retry_progress");
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"One request"})).unwrap();
        h.host.wake_messages(&h.binding);assert!(h.host.wait_for_completed(1,std::time::Duration::from_secs(30)));
        h.host.shutdown();assert_eq!(h.work_prompts.lock().unwrap().len(),1);
        assert!(crate::session_tools::pending(&scope,"rich").unwrap().is_none());
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn a_disconnected_stu_reconciles_unfinished_input_in_a_new_session_once() {
        let h=harness(1);assignment::register(&h.state,&registration(&h)).unwrap();
        let scope=h.host.message_scope(&h.binding);let path=h.root.join("peer-status.json");
        crate::session_tools::bind(&path,&scope,"front-session","rich",Some(crate::ecs::UserInstruction{ledger_ref:registration(&h).instruction_ledger_ref,sha256:registration(&h).instruction_sha256,text:None})).unwrap();
        crate::session_tools::delegate(&path,&scope,serde_json::json!({"text":"Land the three branches"})).unwrap();
        h.host.wake_messages(&h.binding);assert!(h.host.wait_for_completed(1,std::time::Duration::from_secs(30)));
        h.team.lock().unwrap().turn_results.push_back("io_error");
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"Keep the original release date"})).unwrap();
        h.host.wake_messages(&h.binding);assert!(h.host.wait_for_completed(2,std::time::Duration::from_secs(30)));
        h.host.wake_messages(&h.binding);assert!(h.host.wait_for_completed(4,std::time::Duration::from_secs(30)));
        h.host.shutdown();
        assert_eq!(h.spawns.load(Ordering::SeqCst),2,"a disconnected connection was reused");
        let prompts=h.work_prompts.lock().unwrap().clone();assert_eq!(prompts.len(),4,"recovery input repeated: {prompts:?}");
        assert!(prompts[2].contains("Land the three branches"));assert!(prompts[3].contains("Keep the original release date"));
        assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_some(),"unfinished work was lost on disconnect");
    }

    #[test]
    fn one_stu_connection_can_open_while_another_is_still_starting() {
        struct IndependentFactory(Arc<StartGate>);
        impl LeaseFactory for IndependentFactory {
            fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
                panic!("scoped Stu only")
            }
            fn spawn_work(
                &self,
                binding: &ThreadBinding,
            ) -> Result<Box<dyn Cognition>, CognitionError> {
                if binding.thread_id() == "thread-one" {
                    self.0.wait_until_open();
                }
                Ok(Box::new(crate::cognition::MockCognition::new(
                    binding.thread_id(),
                    Vec::new(),
                )))
            }
        }
        let h = harness(1);
        let gate = StartGate::open_now();
        gate.shut();
        let host = WorkHost::new(
            &h.state,
            Box::new(IndependentFactory(gate.clone())),
            Arc::new(SilentNotifier),

        );
        let a = Backend::new();
        let b = Backend::new();
        let opening = host.clone();
        let binding = h.binding.clone();
        let worker = std::thread::spawn(move || opening.ensure_lease(&a, &binding));
        // load-bound: hang guard only; the gate's recorded entry proves A is still opening.
        until("A never reached its startup gate", || {
            gate.2.load(Ordering::SeqCst)
        });
        let binding = ThreadBinding::new(
            PersonId::default_ceo(),
            EntityId::parse("depot").unwrap(),
            "thread-two",
            1,
        );
        let opening = host.clone();
        let second = b.clone();
        let (sent, received) = std::sync::mpsc::channel();
        let other = std::thread::spawn(move || {
            let result = opening.ensure_lease(&second, &binding);
            sent.send(result).unwrap();
        });
        // load-bound: hang guard only; receiving B's result before releasing A proves independence.
        let result = received.recv_timeout(std::time::Duration::from_secs(60));
        let a_held = !worker.is_finished();
        gate.release();
        worker.join().unwrap().unwrap();
        other.join().unwrap();
        result
            .expect("B waited for A's connection startup")
            .unwrap();
        assert!(a_held);
        assert_eq!(
            b.lease.lock().unwrap().as_ref().unwrap().session_id(),
            "thread-two"
        );
    }

    #[test]
    fn a_correction_reaches_stus_live_input_while_his_job_owns_the_lease() {
        type Delivery = (String, String, Box<dyn FnOnce(crate::steering::InputOutcome) + Send>);
        struct Input(std::sync::mpsc::Sender<Delivery>);
        impl TurnCancel for Input {
            fn cancel(&self) -> bool {
                panic!("a correction must not cancel the job")
            }
            fn send_input(
                &self,
                id: &str,
                text: &str,
                taken: Box<dyn FnOnce(crate::steering::InputOutcome) + Send>,
            ) -> Result<bool, String> {
                self.0.send((id.into(), text.into(), taken)).unwrap();
                Ok(true)
            }
        }
        let h = harness(0);
        let scope = h.host.message_scope(&h.binding);
        let path = h.state.join("rich-status.json");
        crate::session_tools::bind(&path, &scope, "native-rich", "rich", None).unwrap();
        crate::session_tools::send(
            &path,
            &scope,
            serde_json::json!({"text":"Original request"}),
        )
        .unwrap();
        let original = crate::session_tools::pending(&scope, "stu")
            .unwrap()
            .unwrap()
            .1;
        let backend = Backend::new();
        let (tx, rx) = std::sync::mpsc::channel();
        {
            let mut inner = backend.inner.lock().unwrap();
            inner.processing = true;
            inner.binding = Some(h.binding.clone());
            inner.peer_active_through = Some(original);
            inner.peer_input = Some(Arc::new(Input(tx)));
            inner.lease_session = Some("same-stu".into());
        }
        h.host
            .backends
            .lock()
            .unwrap()
            .insert(h.binding.thread_id().into(), backend.clone());
        let running = backend.lease.lock().unwrap();
        crate::session_tools::send(
            &path,
            &scope,
            serde_json::json!({"text":"Correction: keep the existing release date"}),
        )
        .unwrap();
        h.host.wake_messages(&h.binding);
        let (_, text, taken) = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("hang guard: correction must reach input while the job still holds the lease");
        assert!(text.contains("keep the existing release date"));
        assert_eq!(
            crate::session_tools::pending(&scope, "stu")
                .unwrap()
                .unwrap()
                .1,
            original,
            "writing is not consumption"
        );
        crate::session_tools::send(
            &path,
            &scope,
            serde_json::json!({"text":"Then retain the current team"}),
        )
        .unwrap();
        taken(crate::steering::InputOutcome{result:Ok("end_turn".into()),report_failure:true});
        h.host.wake_messages(&h.binding);
        let (_, text, taken) = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("hang guard: consumption forwards the next durable message");
        assert!(text.contains("retain the current team"));
        taken(crate::steering::InputOutcome{result:Ok("end_turn".into()),report_failure:true});
        crate::session_tools::acknowledge(&scope, "stu", original).unwrap();
        assert!(
            crate::session_tools::pending(&scope, "stu")
                .unwrap()
                .is_none(),
            "finishing the original turn must not rewind the consumed cursor"
        );
        assert!(
            backend.inner.lock().unwrap().processing,
            "the running job was not ended to deliver input"
        );
        assert!(h.host.open_assignments().unwrap().is_empty());
        use sha2::Digest;
        let instruction = crate::ecs::UserInstruction::with_text(
            "ledger:thread-one:turn-7".into(),
            format!("{:x}", sha2::Sha256::digest("landing the three branches")),
            Some("landing the three branches"),
        );
        let mut request = registration(&h);
        request.instruction_ledger_ref = instruction.ledger_ref.clone();
        request.instruction_sha256 = instruction.sha256.clone();
        let receipt = assignment::register(&h.state, &request).unwrap();
        crate::session_tools::bind(&path, &scope, "native-rich", "rich", Some(instruction))
            .unwrap();
        crate::session_tools::delegate(
            &path,
            &scope,
            serde_json::json!({"text":"New execution request"}),
        )
        .unwrap();
        h.host.wake_messages(&h.binding);
        let (_,text,taken)=rx.recv_timeout(std::time::Duration::from_secs(5))
            .expect("hang guard: a new handoff must reach the same live input before Stu ends his current turn");
        let handoff: crate::session_tools::Message = serde_json::from_str(&text).unwrap();
        assert!(handoff.handoff);
        assert_eq!(handoff.text, "New execution request");
        assert_eq!(handoff.work.as_ref().unwrap().assignment_id, receipt.id);
        taken(crate::steering::InputOutcome{result:Ok("end_turn".into()),report_failure:true});
        crate::session_tools::send(
            &path,
            &scope,
            serde_json::json!({"text":"Correction to the current job: do not change the date"}),
        )
        .unwrap();
        h.host.wake_messages(&h.binding);
        let (_, text, taken) = rx.recv_timeout(std::time::Duration::from_secs(5)).expect(
            "hang guard: a queued new job must not block a quick correction to current work",
        );
        assert!(text.contains("do not change the date"));
        taken(crate::steering::InputOutcome{result:Ok("end_turn".into()),report_failure:true});
        let pending=crate::session_tools::pending(&scope,"stu").unwrap().expect("native turn completion erased the job");
        assert_eq!(pending.0.work.as_ref().unwrap().assignment_id,receipt.id);
        drop(running);
        *backend.lease.lock().unwrap()=Some(Box::new(Lease {
            session:"same-stu".into(),prompts:h.work_prompts.clone(),commands:h.commands.clone(),team:h.team.clone(),settings:None,
        }));
        h.team.lock().unwrap().turn_results.push_back("max_tokens");
        let message=crate::session_tools::Message {handoff:false,from:"rich".into(),text:"Receiving turn".into(),instruction:None,work:None};
        assert!(h.host.run_message(&backend,&h.binding,&message,"live-input-receiving-turn").is_err());
        let after=backend.inner.lock().unwrap().peer_active_through.unwrap();
        assert!(crate::session_tools::pending_after(&scope,"stu",after).unwrap().is_none(),"accepted live input became new after the receiving turn failed");
        assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_some(),"failed turn lost the unfinished handoff");
        assignment::advance(&h.state,"depot","thread-one",&receipt.id,AssignmentState::Settled,"Stu reported completion").unwrap();
        assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_none(),"completed job replayed after Stu's report");
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn unfinished_live_handoffs_reach_recovered_stu_before_newer_input() {
        use std::io::Write as _;
        type Delivery = (String, Box<dyn FnOnce(crate::steering::InputOutcome) + Send>);
        struct Input {
            seen: Mutex<std::collections::HashSet<String>>,
            tx: std::sync::mpsc::Sender<Delivery>,
        }
        impl TurnCancel for Input {
            fn cancel(&self) -> bool {
                panic!("live recovery must not interrupt Stu")
            }
            fn send_input(
                &self,
                id: &str,
                text: &str,
                taken: Box<dyn FnOnce(crate::steering::InputOutcome) + Send>,
            ) -> Result<bool, String> {
                if !self.seen.lock().unwrap().insert(id.into()) {
                    return Ok(false);
                }
                self.tx.send((text.into(), taken)).unwrap();
                Ok(true)
            }
        }
        for advanced_frontier in [false, true] {
            let h = harness(0);
            let scope = h.host.message_scope(&h.binding);
            let path = crate::session_tools::inbox(&scope, "stu").unwrap();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let add = |label: &str| {
                use sha2::Digest;
                let request=Registration{obligation_id:format!("work-{label}"),instruction_ledger_ref:format!("ledger:thread-one:{label}"),
                    instruction_sha256:format!("{:x}",sha2::Sha256::digest(label)),..registration(&h)};
                let receipt=assignment::register(&h.state,&request).unwrap();
                let instruction=crate::ecs::UserInstruction::with_text(request.instruction_ledger_ref,request.instruction_sha256,Some(label));
                let message = crate::session_tools::Message {
                    handoff: true,
                    from: "rich".into(),
                    text: label.into(),
                    instruction: Some(instruction),
                    work: Some(crate::session_tools::WorkReference{assignment_id:receipt.id,obligation_id:request.obligation_id}),
                };
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .unwrap();
                writeln!(file, "{}", serde_json::to_string(&message).unwrap()).unwrap();
                file.sync_all().unwrap();
                file.metadata().unwrap().len()
            };
            let a = add("A");
            crate::session_tools::begin_attempt(
                &scope,
                "stu",
                &format!("peer:{a}"),
                "before-relaunch",
            )
            .unwrap();
            let b = add("B");
            crate::session_tools::begin_attempt(
                &scope,
                "stu",
                &format!("peer:{b}"),
                "before-relaunch",
            )
            .unwrap();
            crate::session_tools::accept_live_handoff(&scope, "stu", b).unwrap();
            if advanced_frontier {
                crate::session_tools::accept_live_handoff(&scope, "stu", a).unwrap();
            }
            assert_eq!(
                crate::session_tools::pending(&scope, "stu")
                    .unwrap()
                    .unwrap()
                    .0
                    .text,
                "A"
            );
            crate::session_tools::begin_attempt(
                &scope,
                "stu",
                &format!("peer:{a}"),
                "after-relaunch",
            )
            .unwrap();
            let c = add("C");
            let backend = Backend::new();
            let (tx, rx) = std::sync::mpsc::channel();
            {
                let mut inner = backend.inner.lock().unwrap();
                inner.binding = Some(h.binding.clone());
                inner.peer_active_through = Some(a);
                inner.peer_input = Some(Arc::new(Input {
                    seen: Mutex::new(Default::default()),
                    tx,
                }));
                inner.lease_session = Some("after-relaunch".into());
            }
            h.host
                .backends
                .lock()
                .unwrap()
                .insert(h.binding.thread_id().into(), backend.clone());
            let lease = backend.lease.lock().unwrap();
            h.host.wake_messages(&h.binding);
            let (text, taken) = rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("hang guard: recovered B must reach Stu while A still holds the lease");
            let message: crate::session_tools::Message = serde_json::from_str(&text).unwrap();
            assert_eq!(message.text, "B", "new C overtook unfinished B");
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&text).unwrap()
                    ["previous_delivery_attempt"]["session_id"],
                "before-relaunch"
            );
            assert!(
                crate::session_tools::previous_attempt(&scope, "stu", &format!("peer:{b}"))
                    .unwrap()
                    .is_some(),
                "sending without a provider receipt was called complete"
            );
            h.host.wake_messages(&h.binding);
            let (text, complete_c) = rx.recv_timeout(std::time::Duration::from_secs(5))
                .expect("native queue must receive C after B without waiting for B's result");
            assert_eq!(serde_json::from_str::<crate::session_tools::Message>(&text).unwrap().text,"C");
            assert!(rx.try_recv().is_err(),"an input was submitted twice to the same native session");
            taken(crate::steering::InputOutcome{result:Ok("end_turn".into()),report_failure:true});
            complete_c(crate::steering::InputOutcome{result:Ok("end_turn".into()),report_failure:true});
            assert_eq!(backend.inner.lock().unwrap().peer_active_through, Some(c));
            crate::session_tools::acknowledge(&scope, "stu", a).unwrap();
            // The existing completion API is bounded by actual receipts in the fix.
            assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_some(),"end_turn erased unfinished jobs");
            for record in assignment::read_all(&h.state,"depot","thread-one").unwrap(){
                assignment::advance(&h.state,"depot","thread-one",&record.id,AssignmentState::Settled,"Stu's terminal report").unwrap();
            }
            assert!(crate::session_tools::pending(&scope,"stu").unwrap().is_none());
            drop(lease);
            h.host.shutdown();
            std::fs::remove_dir_all(h.root).unwrap();
        }
    }

    #[test]
    fn stopped_new_handoff_is_skipped_on_live_input_without_blocking_a_correction() {
        type Delivery = (String, Box<dyn FnOnce(crate::steering::InputOutcome) + Send>);
        struct Input(std::sync::mpsc::Sender<Delivery>);
        impl TurnCancel for Input {
            fn cancel(&self) -> bool {
                panic!("Stop of B must not cancel Stu's other work")
            }
            fn send_input(
                &self,
                _: &str,
                text: &str,
                taken: Box<dyn FnOnce(crate::steering::InputOutcome) + Send>,
            ) -> Result<bool, String> {
                self.0.send((text.into(), taken)).unwrap();
                Ok(true)
            }
        }
        let h = harness(0);
        let scope = h.host.message_scope(&h.binding);
        let path = h.state.join("rich-status.json");
        crate::session_tools::bind(&path, &scope, "native-rich", "rich", None).unwrap();
        crate::session_tools::send(&path, &scope, serde_json::json!({"text":"A is running"}))
            .unwrap();
        let a = crate::session_tools::pending(&scope, "stu")
            .unwrap()
            .unwrap()
            .1;
        let record = assignment::register(&h.state, &registration(&h)).unwrap();
        let instruction = crate::ecs::UserInstruction::with_text(
            "ledger:thread-one:turn-7".into(),
            registration(&h).instruction_sha256,
            Some("landing the three branches"),
        );
        crate::session_tools::bind(&path, &scope, "native-rich", "rich", Some(instruction))
            .unwrap();
        crate::session_tools::delegate(
            &path,
            &scope,
            serde_json::json!({"text":"B must never start"}),
        )
        .unwrap();
        assignment::record_user_stop(&h.state, "depot", "thread-one", &record.id).unwrap();
        crate::session_tools::send(&path, &scope, serde_json::json!({"text":"Correction to A"}))
            .unwrap();
        let backend = Backend::new();
        let (tx, rx) = std::sync::mpsc::channel();
        {
            let mut inner = backend.inner.lock().unwrap();
            inner.binding = Some(h.binding.clone());
            inner.peer_active_through = Some(a);
            inner.peer_input = Some(Arc::new(Input(tx)));
            inner.lease_session = Some("same-stu".into());
        }
        h.host
            .backends
            .lock()
            .unwrap()
            .insert(h.binding.thread_id().into(), backend.clone());
        let lease = backend.lease.lock().unwrap();
        h.host.wake_messages(&h.binding);
        let (text, taken) = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("hang guard: stopped B must not block A's correction");
        let message: crate::session_tools::Message = serde_json::from_str(&text).unwrap();
        assert!(!message.handoff);
        assert_eq!(message.text, "Correction to A");
        taken(crate::steering::InputOutcome{result:Ok("end_turn".into()),report_failure:true});
        assert!(rx.try_recv().is_err(), "a stopped handoff was sent");
        crate::session_tools::acknowledge(&scope, "stu", a).unwrap();
        assert!(crate::session_tools::pending(&scope, "stu")
            .unwrap()
            .is_none());
        drop(lease);
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn ordinary_peer_input_runs_without_an_assignment_and_is_not_replayed_after_ack() {
        let h = harness(0);
        let scope = h.host.message_scope(&h.binding);
        let path = h.state.join("rich-status.json");
        crate::session_tools::bind(&path, &scope, "native-rich", "rich", None).unwrap();
        crate::session_tools::send(
            &path,
            &scope,
            serde_json::json!({"text":"Inspect the current evidence"}),
        )
        .unwrap();
        h.host.wake_messages(&h.binding);
        assert!(
            h.host
                .wait_for_completed(1, std::time::Duration::from_secs(10)),
            "hang guard: wait for the receiving turn to complete"
        );
        assert!(
            h.bound.lock().unwrap().is_empty(),
            "communication manufactured an assignment"
        );
        assert!(h.host.open_assignments().unwrap().is_empty());
        assert_eq!(h.work_prompts.lock().unwrap().len(), 1);
        assert!(h.work_prompts.lock().unwrap()[0].contains("Inspect the current evidence"));
        assert!(crate::session_tools::pending(&scope, "stu")
            .unwrap()
            .is_none());
        h.host.shutdown();
    }

    /// **When Stu's jobs have settled, what the app left behind goes** (CEO §54 addendum 2): after
    /// his turn, with nothing open, the app's sweep runs through the real engine and removes an
    /// ended session's scratch, and never a live session's.
    #[test]
    fn a_turn_that_leaves_nothing_open_sweeps_the_apps_leftovers_and_never_a_live_one() {
        let h = harness(0);
        let mut s = crate::app_cleanup::sandbox::Sandbox::new(Some(&h.root));
        let (_, left) = s.hosted();
        let (live, running) = s.hosted();
        s.live(&live);
        let sweeper = Arc::new(s.sweeper());
        h.host.set_cleanup(Arc::new(move |moment| sweeper.spawn(moment)));
        let scope = h.host.message_scope(&h.binding);
        let path = h.state.join("rich-status.json");
        crate::session_tools::bind(&path, &scope, "native-rich", "rich", None).unwrap();
        crate::session_tools::send(&path, &scope, serde_json::json!({"text":"Inspect the current evidence"})).unwrap();
        assert!(left.exists(), "nothing is swept before his turn");
        h.host.wake_messages(&h.binding);
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(10)), "hang guard: the turn completes");
        // The sweep runs on its own thread, and the reaper writes its log once, after every
        // deletion (scratch-reaper.py `apply` -> `write_log`): wait for the recorded deletion,
        // not only for the folder to be gone, or a busy host reads the log before it is written.
        let deleted = format!("DELETED {}", left.display());
        until("the ended session's scratch is swept, and recorded, once nothing is open",
              || !left.exists() && s.log().contains(&deleted));
        assert!(running.exists(), "a live session's scratch is never touched");
        h.host.shutdown();
    }

    #[test]
    fn peer_input_and_question_answer_reach_stu_before_his_first_output_or_result() {
        type Delivery=(String,Box<dyn FnOnce(crate::steering::InputOutcome)+Send>);
        struct Input(std::sync::mpsc::Sender<Delivery>);
        impl TurnCancel for Input {
            fn cancel(&self)->bool {false}
            fn send_input(&self,_:&str,text:&str,completed:Box<dyn FnOnce(crate::steering::InputOutcome)+Send>)->Result<bool,String> {
                self.0.send((text.to_string(),completed)).map_err(|e|e.to_string())?;
                Ok(true)
            }
        }
        struct Early {
            input:Arc<Input>,
            entered:std::sync::mpsc::Sender<()>,
            release:Option<std::sync::mpsc::Receiver<()>>,
        }
        impl Cognition for Early {
            fn session_id(&self)->&str {"early-native-stu"}
            fn cancel_handle(&self)->Option<Arc<dyn TurnCancel>> {Some(self.input.clone())}
            fn prompt(&mut self,_:&str,_:&mut dyn FnMut(TurnItem))->Result<String,CognitionError> {panic!("submit must precede reply waiting")}
            fn enqueue_prompt(&mut self,_:&str,started:Box<dyn FnOnce()->Result<(),String>+Send>)->Result<crate::cognition::PendingReply,CognitionError> {
                started().map_err(CognitionError::Protocol)?;
                self.entered.send(()).unwrap();
                let release=self.release.take().unwrap();
                Ok(crate::cognition::PendingReply::new(move |_| {
                    // load-bound: a positive release owns the answer; this deadline only catches a hang.
                    release.recv_timeout(std::time::Duration::from_secs(30)).unwrap();
                    Ok("end_turn".into())
                }))
            }
        }
        struct Factory(Mutex<Option<Early>>);
        impl LeaseFactory for Factory {
            fn spawn(&self)->Result<Box<dyn Cognition>,CognitionError> {unreachable!()}
            fn spawn_work(&self,_:&ThreadBinding)->Result<Box<dyn Cognition>,CognitionError> {
                Ok(Box::new(self.0.lock().unwrap().take().expect("Stu must keep one native session")))
            }
        }
        let h=harness(0);
        let (sent,received)=std::sync::mpsc::channel();
        let (entered,ready)=std::sync::mpsc::channel();
        let (release,waiting)=std::sync::mpsc::channel();
        let host=WorkHost::new(&h.state,Box::new(Factory(Mutex::new(Some(Early {
            input:Arc::new(Input(sent)),entered,release:Some(waiting),
        })))),Arc::new(SilentNotifier));
        let scope=host.message_scope(&h.binding);
        let path=h.state.join("early-rich.json");
        crate::session_tools::bind(&path,&scope,"native-rich","rich",None).unwrap();
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"First input"})).unwrap();
        host.wake_messages(&h.binding);
        // load-bound: only a hang guard; the first reply cannot finish before release is sent.
        ready.recv_timeout(std::time::Duration::from_secs(30)).unwrap();
        until("Stu's input handle waited for output",||host.backend("thread-one").unwrap().inner.lock().unwrap().peer_input.is_some());
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"Correction before any output"})).unwrap();
        host.wake_messages(&h.binding);
        // load-bound: wait for the actual submitted input, with the first reply still held.
        let (peer,complete_peer)=received.recv_timeout(std::time::Duration::from_secs(30)).unwrap();
        assert!(peer.contains("Correction before any output"));
        let answer=crate::questions::Delivery {obligation_id:None,id:"early-answer".into(),entity_id:"depot".into(),
            thread_id:"thread-one".into(),asker:"stu".into(),set_id:None,text:"Use Friday".into(),receipt:None,order:None};
        host.queue_question_answer(&h.binding,&answer).unwrap();
        // load-bound: a submitted answer is the verdict; the first result remains impossible until release.
        let (text,complete_answer)=received.recv_timeout(std::time::Duration::from_secs(30)).unwrap();
        assert!(text.contains("Use Friday"));
        host.wake_messages(&h.binding);
        assert!(received.try_recv().is_err(),"a repeated wake duplicated native input");
        assert!(crate::question_work::waiting(&h.state,&answer.id),"stdin submission falsely marked the answer taken");
        complete_peer(crate::steering::InputOutcome{result:Ok("end_turn".into()),report_failure:true});
        complete_answer(crate::steering::InputOutcome{result:Ok("end_turn".into()),report_failure:true});
        assert!(!crate::question_work::waiting(&h.state,&answer.id));
        release.send(()).unwrap();
        assert!(host.wait_for_completed(1,std::time::Duration::from_secs(30)));
        host.shutdown();
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn a_standing_stu_question_answer_reaches_his_turn_without_an_assignment() {
        let h = harness(0);
        let answer = crate::questions::Delivery {
            obligation_id: None,
            id: "standing-answer".into(),
            entity_id: "depot".into(),
            thread_id: "thread-one".into(),
            asker: "stu".into(),
            set_id: Some("standing-choice".into()),
            text: "Use the existing launch date".into(),
            receipt: None,
            order: None,
        };
        h.host.queue_question_answer(&h.binding, &answer).unwrap();
        assert!(
            h.host
                .wait_for_completed(1, std::time::Duration::from_secs(10)),
            "hang guard: wait for the answer turn"
        );
        assert!(h.host.open_assignments().unwrap().is_empty());
        assert!(h.bound.lock().unwrap().is_empty());
        assert_eq!(h.work_prompts.lock().unwrap().len(), 1);
        assert!(h.work_prompts.lock().unwrap()[0].contains(&answer.text));
        assert!(crate::question_work::pending(&h.state).unwrap().is_empty());
        let wrong = ThreadBinding::new(
            PersonId::default_ceo(),
            EntityId::parse("depot").unwrap(),
            "thread-two",
            1,
        );
        assert!(h.host.queue_question_answer(&wrong, &answer).is_err());
        h.host.shutdown();
        // A's answer retains A's authority even when B is the latest session input.
        let a = assignment::register(&h.state, &registration(&h)).unwrap();
        let b = assignment::register(
            &h.state,
            &Registration {
                obligation_id: "work-B".into(),
                instruction_ledger_ref: "ledger:thread-one:turn-B".into(),
                instruction_sha256: "b".repeat(64),
                ..registration(&h)
            },
        )
        .unwrap();
        let record = assignment::read(&h.state, "depot", "thread-one", &a.id).unwrap();
        let selected = crate::questions::Delivery {
            obligation_id: Some(record.obligation_id.clone()),
            ..answer.clone()
        };
        assert_eq!(
            h.host
                .question_answer_instruction(&selected)
                .unwrap()
                .unwrap()
                .ledger_ref,
            record.instruction_ledger_ref
        );
        let b_record = assignment::read(&h.state, "depot", "thread-one", &b.id).unwrap();
        let status = h.state.join("stu-status.json");
        crate::session_tools::bind(
            &status,
            &h.host.message_scope(&h.binding),
            "question-stu",
            "stu",
            Some(crate::ecs::UserInstruction {
                ledger_ref: b_record.instruction_ledger_ref,
                sha256: b_record.instruction_sha256,
                text: None,
            }),
        )
        .unwrap();
        let qscope = h.state.join("questions-scope.json");
        crate::question_tools::write_scope(
            &qscope,
            &crate::question_tools::Scope {
                context: crate::questions::AskScope {
                    obligation_id: None,
                    root: h.state.clone(),
                    entity_id: "depot".into(),
                    thread_id: "thread-one".into(),
                    turn_id: "peer:latest-B".into(),
                    asker: "stu".into(),
                    session_id: "question-stu".into(),
                    engine: None,
                    entity_root: None,
                    app_run: None,
                },
                actions_allowed: true,
                answer_method: "typed".into(),
                surface: "mac".into(),
            },
        )
        .unwrap();
        let args = serde_json::json!({"questions":[{"text":"When should the release ship?", "options":[
            {"label":"Ship today","description":"Earlier fixes"},{"label":"Ship tomorrow","description":"More checks"}]}]});
        let current = crate::question_tools::call(&qscope, "ask", args.clone()).unwrap();
        assert!(
            current["questions"][0]["obligation_id"].is_null(),
            "a conversation question must not inherit the latest job"
        );
        let mut for_b = args.clone();
        for_b["obligation_id"] = "work-B".into();
        assert_eq!(
            crate::question_tools::call(&qscope, "ask", for_b.clone()).unwrap()["questions"][0]
                ["obligation_id"],
            "work-B"
        );
        let mut for_a = args.clone();
        for_a["obligation_id"] = record.obligation_id.clone().into();
        let owned = crate::question_tools::call(&qscope, "ask", for_a.clone()).unwrap();
        assert_eq!(owned["questions"][0]["obligation_id"], record.obligation_id);
        assignment::record_user_stop(&h.state, "depot", "thread-one", &b.id).unwrap();
        assert!(crate::question_tools::call(&qscope, "ask", for_b).is_err());
        assert!(
            crate::question_tools::call(&qscope, "ask", args).is_ok(),
            "job Stop must not close conversation questions"
        );
        assert!(
            crate::question_tools::call(&qscope, "ask", for_a).is_ok(),
            "B's Stop revoked questions about A"
        );
        assignment::record_user_stop(&h.state, "depot", "thread-one", &a.id).unwrap();
        assert!(
            h.host.question_answer_instruction(&selected).is_err(),
            "stale answer reauthorized a stopped job"
        );
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn a_handoff_refused_before_stu_starts_is_saved_and_reports_one_failure() {
        struct Refused;
        impl LeaseFactory for Refused {
            fn spawn(&self)->Result<Box<dyn Cognition>,CognitionError> {unreachable!()}
            fn spawn_work(&self,_:&ThreadBinding)->Result<Box<dyn Cognition>,CognitionError> {
                Err(CognitionError::Io("provider failed before opening".into()))
            }
        }
        let h=harness(0);
        let host=WorkHost::new(&h.state,Box::new(Refused),Arc::new(SilentNotifier));
        let scope=host.message_scope(&h.binding);
        let path=h.state.join("rich-status.json");
        crate::session_tools::bind(&path,&scope,"native-rich","rich",None).unwrap();
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"Prepare the report"})).unwrap();
        let (message,through)=crate::session_tools::pending(&scope,"stu").unwrap().unwrap();
        let error=host.run_message(&Backend::new(),&h.binding,&message,&format!("peer:{through}")).unwrap_err();
        assert!(error.contains("provider failed before opening"));
        let (saved,saved_through)=crate::session_tools::pending(&scope,"stu").unwrap().unwrap();
        assert_eq!(saved_through,through);
        assert_eq!(saved.text,"Prepare the report");
        let (notice,end)=crate::session_tools::pending(&scope,"rich").unwrap().unwrap();
        assert_eq!(notice.from,"work-status");
        let facts:serde_json::Value=serde_json::from_str(&notice.text).unwrap();
        assert_eq!(facts["input_id"],format!("peer:{through}"));
        assert!(facts["error"].as_str().unwrap().contains("provider failed before opening"));
        crate::session_tools::acknowledge(&scope,"rich",end).unwrap();
        assert!(crate::session_tools::pending(&scope,"rich").unwrap().is_none(),"one failure produced duplicate notices");
        for attempt in 0..3 {
            let reopened=WorkHost::new(&h.state,Box::new(Refused),Arc::new(SilentNotifier));
            assert!(reopened.run_message(&Backend::new(),&h.binding,&message,&format!("peer:{through}")).is_err());
            assert!(crate::session_tools::pending(&scope,"rich").unwrap().is_none(),"retry {attempt} reported the same failed input again");
            assert_eq!(crate::session_tools::pending(&scope,"stu").unwrap().unwrap().1,through,"unsent input was discarded");
            reopened.shutdown();
        }
        crate::session_tools::send(&path,&scope,serde_json::json!({"text":"A different request"})).unwrap();
        let (second,second_through)=crate::session_tools::pending_after(&scope,"stu",through).unwrap().unwrap();
        assert!(host.run_message(&Backend::new(),&h.binding,&second,&format!("peer:{second_through}")).is_err());
        let (notice,_)=crate::session_tools::pending(&scope,"rich").unwrap().unwrap();
        let facts:serde_json::Value=serde_json::from_str(&notice.text).unwrap();
        assert_eq!(facts["input_id"],format!("peer:{second_through}"),"dedup hid a different failed request");
        host.shutdown();
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    /// **Build plan 2026-10-21 item 0: a changed settings file renews Stu at his next message**,
    /// never while something of his runs, and an unchanged one renews nothing.
    #[test]
    fn a_change_to_the_users_settings_files_renews_stu_at_his_next_message() {
        let h = harness(0);
        h.team.lock().unwrap().settings = Some("as first read".into());
        every_worker_observed_ending(&h);
        for n in 1..=3 {
            let folder = h.state.join("evidence").join(format!("work-session-rotated-{n}"));
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("callbacks.jsonl"), "").unwrap();
            std::fs::write(folder.join(".lock"), "").unwrap();
        }
        let backend = Backend::new();
        let message = crate::session_tools::Message {
            handoff: false, from: "rich".into(), text: "Carry on".into(), instruction: None, work: None,
        };
        let run = |n| h.host.run_message(&backend, &h.binding, &message, &format!("settings-test:{n}")).unwrap();
        run(1);
        run(2);
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1, "unchanged settings renewed Stu");
        h.team.lock().unwrap().settings = Some("edited".into());
        *h.commands.lock().unwrap() = Some(crate::lease_commands::CommandReading::Running(1));
        run(3);
        assert_eq!(h.spawns.load(Ordering::SeqCst), 1, "a renewal ended a running command");
        *h.commands.lock().unwrap() = Some(crate::lease_commands::CommandReading::Clear);
        run(4);
        assert_eq!(h.spawns.load(Ordering::SeqCst), 2, "the edited settings did not reach Stu's next message");
        run(5);
        assert_eq!(h.spawns.load(Ordering::SeqCst), 2, "the renewed session was renewed again");
        h.host.shutdown();
        std::fs::remove_dir_all(h.root).unwrap();
    }

    #[test]
    fn peer_handoff_with_operational_record_executes_once() {
        let h = harness(0);
        h.team.lock().unwrap().digest = Some("stock".into());
        every_worker_observed_ending(&h);
        for n in 1..=4 {
            let folder = h
                .state
                .join("evidence")
                .join(format!("work-session-rotated-{n}"));
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("callbacks.jsonl"), "").unwrap();
            std::fs::write(folder.join(".lock"), "").unwrap();
        }
        let scope = h.host.message_scope(&h.binding);
        let path = h.state.join("rich-status.json");
        crate::session_tools::bind(&path, &scope, "native-rich", "rich", None).unwrap();
        let receipt = assignment::register(&h.state, &registration(&h)).unwrap();
        assert_eq!(
            h.host.adopt_registered(&h.binding),
            0,
            "an operational record must never dispatch a second execution"
        );
        crate::session_tools::send(
            &path,
            &scope,
            serde_json::json!({"text":"Inspect the current evidence"}),
        )
        .unwrap();
        h.host.wake_messages(&h.binding);
        // load-bound: hang guard only; the verdict is one consumed prompt and an empty inbox, not elapsed time.
        assert!(
            h.host
                .wait_for_completed(1, std::time::Duration::from_secs(10)),
            "hang guard: wait for peer consumption"
        );
        assert_eq!(h.host.adopt_registered(&h.binding), 0);
        assert_eq!(
            assignment::read(&h.state, "depot", "thread-one", &receipt.id)
                .unwrap()
                .state,
            AssignmentState::Registered
        );
        assert_eq!(h.work_prompts.lock().unwrap().len(), 1);
        assert!(
            h.bound.lock().unwrap().is_empty(),
            "Stu was rebound to a formal assignment"
        );
        assert!(crate::session_tools::pending(&scope, "stu")
            .unwrap()
            .is_none());
        let backend = h.host.backend_for("thread-one").unwrap();
        let message = crate::session_tools::Message {
            handoff: true,
            from: "rich".into(),
            text: "Use the saved teammate for this authorized work".into(),
            instruction: None,
            work: None,
        };
        let run = |n| {
            h.host
                .run_message(&backend, &h.binding, &message, &format!("team-test:{n}"))
                .unwrap()
        };
        run(2);
        assert_eq!(
            h.spawns.load(Ordering::SeqCst),
            1,
            "unchanged team renewed unnecessarily"
        );
        h.team.lock().unwrap().digest = Some("stock and mark".into());
        let evidence = h.state.join("evidence/work-session-one/callbacks.jsonl");
        let start=serde_json::json!({"schema":1,"callback":{"session_id":"work-session-one","hook_event_name":"SubagentStart","agent_id":"live-worker"}}).to_string()+"\n";
        std::fs::write(&evidence, &start).unwrap();
        run(3);
        assert_eq!(
            backend.inner.lock().unwrap().lease_team.as_deref(),
            Some("stock and mark")
        );
        assert_eq!(
            h.spawns.load(Ordering::SeqCst),
            1,
            "renewal killed a live native worker"
        );
        let end=serde_json::json!({"schema":1,"callback":{"session_id":"work-session-one","hook_event_name":"SubagentStop","agent_id":"live-worker"}}).to_string()+"\n";
        std::fs::write(&evidence, start + &end).unwrap();
        run(4);
        assert_eq!(
            h.spawns.load(Ordering::SeqCst),
            1,
            "team reload must retain Stu and his native descendants"
        );
        *h.commands.lock().unwrap() = Some(crate::lease_commands::CommandReading::Running(1));
        h.team.lock().unwrap().digest = Some("stock, mark and norm".into());
        run(5);
        assert_eq!(
            backend.inner.lock().unwrap().lease_team.as_deref(),
            Some("stock, mark and norm")
        );
        assert_eq!(
            h.spawns.load(Ordering::SeqCst),
            1,
            "team reload must retain Stu and his native descendants"
        );
        *h.commands.lock().unwrap() = Some(crate::lease_commands::CommandReading::Clear);
        run(6);
        assert_eq!(
            h.spawns.load(Ordering::SeqCst),
            1,
            "team reload must retain Stu and his native descendants"
        );
        h.team
            .lock()
            .unwrap()
            .in_turn
            .push_back(Some("stock, mark, norm and echo".into()));
        run(7);
        assert_eq!(
            h.spawns.load(Ordering::SeqCst),
            1,
            "team reload must retain Stu and his native descendants"
        );
        // The team changed during turn 7, so Stu gets his one continuation, and nothing more.
        h.host.wake_messages(&h.binding);
        assert!(
            h.host.wait_for_completed(2, std::time::Duration::from_secs(10)),
            "hang guard: wait for the continuation"
        );
        let prompts = h.work_prompts.lock().unwrap();
        assert_eq!(
            prompts.len(),
            8,
            "roster reload must add one continuation and never replay the accepted handoff"
        );
        assert!(prompts[7].contains("team_reloaded"), "{:?}", prompts[7]);
        drop(prompts);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// Walk 2 of 2026-10-24: Stu saved a teammate, ended his turn to use it on the next one,
    /// and no next input ever came. The reload after his turn registers it; a changed team
    /// then gets exactly one more turn through his own inbox, and an unchanged team none.
    #[test]
    fn a_turn_that_changes_the_team_gets_exactly_one_continuation() {
        let h = harness(0);
        h.team.lock().unwrap().digest = Some("stock".into());
        every_worker_observed_ending(&h);
        let scope = h.host.message_scope(&h.binding);
        let path = h.state.join("rich-status.json");
        crate::session_tools::bind(&path, &scope, "native-rich", "rich", None).unwrap();
        let continuations = |prompts: &[String]| {
            prompts.iter().filter(|text| text.contains("team_reloaded")).count()
        };
        // load-bound: hang guards only; the verdicts are the prompts and the empty inbox.
        let settled = |count| {
            assert!(h.host.wait_for_completed(count, std::time::Duration::from_secs(10)), "hang guard");
            assert!(crate::session_tools::pending(&scope, "stu").unwrap().is_none());
            h.work_prompts.lock().unwrap().clone()
        };
        h.team.lock().unwrap().in_turn.push_back(Some("stock and norm".into()));
        crate::session_tools::send(&path, &scope, serde_json::json!({"text":"Save norm and have him fix it"})).unwrap();
        h.host.wake_messages(&h.binding);
        let prompts = settled(2);
        assert_eq!(prompts.len(), 2, "the changed team got no continuation: {prompts:?}");
        assert_eq!(continuations(&prompts), 1);
        assert!(!prompts[1].contains("Save norm"), "the continuation replayed the input: {prompts:?}");
        crate::session_tools::send(&path, &scope, serde_json::json!({"text":"How is it going?"})).unwrap();
        h.host.wake_messages(&h.binding);
        let prompts = settled(3);
        assert_eq!(prompts.len(), 3, "an unchanged team got a continuation: {prompts:?}");
        assert_eq!(continuations(&prompts), 1);
        h.host.wake_messages(&h.binding);
        let prompts = settled(3);
        assert_eq!(prompts.len(), 3, "a second unchanged refresh added input: {prompts:?}");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    #[test]
    fn stopped_peer_handoff_is_not_delivered_after_first_connection_opens() {
        for during_open in [false, true] {
            let h = harness(0);
            let record = assignment::register(&h.state, &registration(&h)).unwrap();
            let scope = h.host.message_scope(&h.binding);
            let path = h.state.join("rich-status.json");
            let instruction = crate::ecs::UserInstruction::with_text(
                "ledger:thread-one:turn-7".into(),
                registration(&h).instruction_sha256,
                Some("landing the three branches"),
            );
            crate::session_tools::bind(&path, &scope, "native-rich", "rich", Some(instruction))
                .unwrap();
            crate::session_tools::delegate(
                &path,
                &scope,
                serde_json::json!({"text":"Land the three branches"}),
            )
            .unwrap();
            let (message, through) = crate::session_tools::pending(&scope, "stu")
                .unwrap()
                .unwrap();
            let backend = Backend::new();
            backend.inner.lock().unwrap().binding = Some(h.binding.clone());
            h.host
                .backends
                .lock()
                .unwrap()
                .insert("thread-one".into(), backend.clone());
            if during_open {
                h.start_gate.shut()
            } else {
                h.host
                    .stop_assignment("depot", "thread-one", &record.id)
                    .unwrap()
            }
            let host = h.host.clone();
            let binding = h.binding.clone();
            let run = std::thread::spawn(move || {
                host.run_message(&backend, &binding, &message, &format!("peer:{through}"))
                    .unwrap();
                crate::session_tools::acknowledge(&host.message_scope(&binding), "stu", through)
                    .unwrap();
            });
            if during_open {
                // load-bound: hang guard only; the gate records actual entry into
                // the blocked factory call before Stop.
                until(
                    "the first peer connection never reached its opening gate",
                    || h.start_gate.2.load(Ordering::SeqCst),
                );
                h.host
                    .stop_assignment("depot", "thread-one", &record.id)
                    .unwrap();
                h.start_gate.release();
            }
            run.join().unwrap();
            assert!(
                h.work_prompts.lock().unwrap().is_empty(),
                "the stopped handoff reached Stu"
            );
            assert!(
                crate::session_tools::pending(&scope, "stu")
                    .unwrap()
                    .is_none_or(|(message, _)| !message.handoff && message.from == "user-control"),
                "stopped execution must not replay; the user Stop fact may remain for Stu"
            );
            assert_eq!(
                assignment::read(&h.state, "depot", "thread-one", &record.id)
                    .unwrap()
                    .state,
                AssignmentState::Interrupted
            );
            h.host.shutdown();
            std::fs::remove_dir_all(h.root).unwrap();
        }
    }

    #[test]
    fn independent_stu_stop_is_durable_while_a_dispatch_writer_is_busy() {
        struct Noop;
        impl TurnCancel for Noop {
            fn cancel(&self) -> bool {
                false
            }
        }
        let h = harness(0);
        let receipt = assignment::register(&h.state, &registration(&h)).unwrap();
        let backend = Backend::new();
        backend.inner.lock().unwrap().peer_stopper =
            Some(("independent-stu".into(), Arc::new(Noop)));
        h.host
            .backends
            .lock()
            .unwrap()
            .insert("thread-one".into(), backend);
        let writer = crate::work_status::lock_receipts(&h.state, "depot", "thread-one").unwrap();
        let host = h.host.clone();
        let id = receipt.id.clone();
        let stop = std::thread::spawn(move || host.stop_assignment("depot", "thread-one", &id));
        // The existing bounded wait is a hang guard. The verdict requires the
        // actual Stop fact while the dispatch lock is still held, never elapsed time.
        let durable = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            until("the Stop fact waited for the dispatch writer", || {
                assignment::read(&h.state, "depot", "thread-one", &receipt.id)
                    .unwrap()
                    .user_stop_requested_at_ms
                    .is_some()
            })
        }))
        .is_ok();
        drop(writer);
        stop.join().unwrap().unwrap();
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
        assert!(
            durable,
            "the user's Stop must be durable before waiting for a busy dispatch writer"
        );
    }

    #[test]
    fn independent_stu_stop_reaches_only_the_selected_assignments_worker() {
        struct Stops(Mutex<Vec<String>>);
        impl TurnCancel for Stops {
            fn cancel(&self) -> bool {
                panic!("per-assignment Stop must not cancel Stu")
            }
            fn shutdown(&self) {}
            fn stop_worker(&self, tool: &str) -> CommandStop {
                self.0.lock().unwrap().push(tool.into());
                CommandStop::Ended
            }
        }
        let h = harness(0);
        let first = assignment::register(&h.state, &registration(&h)).unwrap();
        let second = assignment::register(
            &h.state,
            &Registration {
                obligation_id: "another-obligation".into(),
                ..registration(&h)
            },
        )
        .unwrap();
        let backend = Backend::new();
        let stopper = Arc::new(Stops(Mutex::new(vec![])));
        {
            let mut inner = backend.inner.lock().unwrap();
            inner.lease_session = Some("independent-stu".into());
            inner.peer_stopper = Some(("independent-stu".into(), stopper.clone()));
        }
        h.host
            .backends
            .lock()
            .unwrap()
            .insert("thread-one".into(), backend);
        let root = h.state.join("work-receipts").join({
            use sha2::Digest;
            format!("{:x}", sha2::Sha256::digest(b"[\"depot\",\"thread-one\"]"))
        });
        std::fs::create_dir_all(&root).unwrap();
        for (id, obligation, tool) in [
            ("first", "obligation-7", "call-first"),
            ("second", "another-obligation", "call-second"),
        ] {
            std::fs::write(root.join(format!("{id}.json")),serde_json::json!({"schema":1,"id":id,
                "binding":{"entity_id":"depot","thread_id":"thread-one","session_id":"independent-stu"},
                "request":{"obligation_id":obligation,"role":"worker"},"status":"running","tool_use_id":tool}).to_string()).unwrap();
        }
        h.host
            .stop_assignment("depot", "thread-one", &first.id)
            .unwrap();
        assert_eq!(*stopper.0.lock().unwrap(), vec!["call-first"]);
        let stopped = assignment::read(&h.state, "depot", "thread-one", &first.id).unwrap();
        assert!(stopped.user_stop_requested_at_ms.is_some());
        let control = crate::session_tools::pending(&h.host.message_scope(&h.binding), "stu")
            .unwrap()
            .unwrap()
            .0;
        assert_eq!(control.from, "user-control");
        let facts: serde_json::Value = serde_json::from_str(&control.text).unwrap();
        assert_eq!(facts["event"], "user_stop");
        assert_eq!(facts["assignment_id"], first.id);
        assert_eq!(
            facts["requested_at_ms"],
            stopped.user_stop_requested_at_ms.unwrap()
        );
        assert!(
            crate::session_tools::pending(&h.host.message_scope(&h.binding), "rich")
                .unwrap()
                .is_none()
        );
        assert_eq!(stopped.state, AssignmentState::Interrupted);
        assert_eq!(
            assignment::read(&h.state, "depot", "thread-one", &second.id)
                .unwrap()
                .state,
            AssignmentState::Registered
        );
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **A job's Stop ends the command Stu is running for it in the foreground** (reap walk 10,
    /// esc-20261021T224235Z-4a2d4e70: "stop-fg (pid 3294) was still running after the trigger").
    /// A real `sleep` stands in for the command; the stand-in `claude` ends it only on a
    /// `stop_task` for that command's own task, as Claude Code 2.1.307 does (measured: the
    /// foreground task's `stop_task` answered success and the process was gone 28 ms later, with
    /// the turn going on). A helper's command in the same stream is not Stu's and is not stopped.
    /// RED before the fix: Stop sent nothing for Stu's own command, so the `sleep` outlived the bound.
    #[test]
    fn a_jobs_stop_ends_the_foreground_command_stu_runs_for_it() {
        let h = harness(0);
        let job = assignment::register(&h.state, &registration(&h)).unwrap();
        stop_ends_the_foreground_command(&h, &job.id, "Run the heartbeat", false);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **Two jobs running at once: the job travels with its command.** Job B is running when
    /// A's foreground command starts, and the command's description names A's obligation_id
    /// (stu.md). Stop on A ends it; B is left as it was. RED at 9b7b70f64: with B open, the
    /// command was nobody's, and it outlived the bound.
    #[test]
    fn a_jobs_stop_ends_the_command_naming_it_while_another_job_runs() {
        let h = harness(0);
        let running = assignment::register(&h.state, &Registration { obligation_id: "obligation-b".into(), ..registration(&h) }).unwrap();
        assignment::advance(&h.state, "depot", "thread-one", &running.id, AssignmentState::Running, "Working.").unwrap();
        let job = assignment::register(&h.state, &registration(&h)).unwrap();
        stop_ends_the_foreground_command(&h, &job.id, "obligation-7: run the heartbeat", false);
        let other = assignment::read(&h.state, "depot", "thread-one", &running.id).unwrap();
        assert_eq!((other.state, other.user_stop_requested_at_ms), (AssignmentState::Running, None), "the other job is not touched");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **The job's commands on the front desk** (reap walk walk-2bfb274d19d0): the job was handed
    /// to Stu, and Rich ran its commands himself in that same turn, the foreground one included.
    /// Stop on the job ends Rich's command by its own task, and his turn goes on. RED at
    /// ae213107a: the Stop reached only Stu's session, so the `sleep` outlived the bound.
    /// Rich's commands in another turn are never the job's
    /// (`a_jobs_stop_leaves_the_front_desks_other_turns_alone`).
    #[test]
    fn a_jobs_stop_ends_the_foreground_command_rich_ran_for_it() {
        let h = harness(0);
        let job = assignment::register(&h.state, &registration(&h)).unwrap();
        stop_ends_the_foreground_command(&h, &job.id, "Run foreground heartbeat test for 300 seconds", true);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    #[test]
    fn a_jobs_stop_leaves_the_front_desks_other_turns_alone() {
        struct Desk(Mutex<Vec<String>>);
        impl TurnCancel for Desk {
            fn cancel(&self) -> bool { panic!("a job's Stop must not interrupt Rich") }
            fn shutdown(&self) {}
            fn own_commands(&self) -> Vec<crate::steering::OwnCommand> {
                vec![crate::steering::OwnCommand { tool_use_id: String::new(), task_id: "brich".into(), started_at_ms: u64::MAX, description: String::new() }]
            }
            fn stop_background_command(&self, task: &str) -> CommandStop {
                self.0.lock().unwrap().push(task.into());
                CommandStop::Ended
            }
        }
        let h = harness(0);
        let job = assignment::register(&h.state, &registration(&h)).unwrap();
        let desk = Arc::new(Desk(Mutex::new(vec![])));
        let turn = |id: &str| Some(crate::steering::ActiveTurn { turn_id: id.into(), thread_id: "thread-one".into(), entity_id: None, started_at: None });
        h.host.stop_assignment_and_front_desk("depot", "thread-one", &job.id,
            Some(crate::steering::FrontDesk { handle: desk.clone(), turn: turn("turn-8") })).unwrap();
        assert!(desk.0.lock().unwrap().is_empty(), "Rich's command in another turn is not this job's");
        let second = assignment::register(&h.state, &Registration { obligation_id: "obligation-8".into(), ..registration(&h) }).unwrap();
        h.host.stop_assignment_and_front_desk("depot", "thread-one", &second.id,
            Some(crate::steering::FrontDesk { handle: desk.clone(), turn: None })).unwrap();
        assert!(desk.0.lock().unwrap().is_empty(), "with no turn running, nothing of Rich's is this job's");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **Two jobs from one turn of Rich's** (review rv-20261022T113651Z-da8b9bfb-d4be): Stop on
    /// A ends the command naming A and never B's, which names B. RED at da8b9bfbf: every command
    /// Rich started after A was registered was stopped, b-background included.
    #[test]
    fn a_jobs_stop_on_the_front_desk_never_takes_the_other_jobs_command() {
        struct Desk(Mutex<Vec<String>>);
        impl TurnCancel for Desk {
            fn cancel(&self) -> bool { panic!("a job's Stop must not interrupt Rich") }
            fn shutdown(&self) {}
            fn own_commands(&self) -> Vec<crate::steering::OwnCommand> {
                [("a-foreground", "obligation-7: build the site"), ("b-background", "obligation-b: watch the logs")]
                    .into_iter().map(|(task, description)| crate::steering::OwnCommand { tool_use_id: String::new(),
                        task_id: task.into(), started_at_ms: u64::MAX, description: description.into() }).collect()
            }
            fn stop_background_command(&self, task: &str) -> CommandStop {
                self.0.lock().unwrap().push(task.into());
                CommandStop::Ended
            }
        }
        let h = harness(0);
        let a = assignment::register(&h.state, &registration(&h)).unwrap();
        let b = assignment::register(&h.state, &Registration { obligation_id: "obligation-b".into(), ..registration(&h) }).unwrap();
        let desk = Arc::new(Desk(Mutex::new(vec![])));
        let turn = Some(crate::steering::ActiveTurn { turn_id: "turn-7".into(), thread_id: "thread-one".into(), entity_id: None, started_at: None });
        h.host.stop_assignment_and_front_desk("depot", "thread-one", &a.id,
            Some(crate::steering::FrontDesk { handle: desk.clone(), turn })).unwrap();
        assert_eq!(*desk.0.lock().unwrap(), vec!["a-foreground".to_string()], "Stop on A took B's command");
        assert_eq!(assignment::read(&h.state, "depot", "thread-one", &b.id).unwrap().user_stop_requested_at_ms, None);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// Rich's selector: a command naming a job is that job's; one naming none is the job's only
    /// when no other job came from the same turn.
    #[test]
    fn the_front_desks_commands_go_to_the_job_they_name() {
        let h = harness(0);
        let a = assignment::register(&h.state, &registration(&h)).unwrap();
        let read = |id: &str| assignment::read(&h.state, "depot", "thread-one", id).unwrap();
        let command = |task: &str, description: &str| crate::steering::OwnCommand { tool_use_id: String::new(),
            task_id: task.into(), started_at_ms: u64::MAX, description: description.into() };
        let commands = vec![command("a-foreground", "obligation-7: build"), command("b-background", "obligation-b: watch"),
            command("unnamed", "List the files")];
        // A alone from this turn: its own command and the unnamed one.
        assert_eq!(desk_commands(&[read(&a.id)], &a.id, &[commands[0].clone(), commands[2].clone()]),
            vec!["a-foreground".to_string(), "unnamed".to_string()]);
        let b = assignment::register(&h.state, &Registration { obligation_id: "obligation-b".into(), ..registration(&h) }).unwrap();
        let all = assignment::read_all(&h.state, "depot", "thread-one").unwrap();
        assert_eq!(desk_commands(&all, &a.id, &commands), vec!["a-foreground".to_string()]);
        assert_eq!(desk_commands(&all, &b.id, &commands), vec!["b-background".to_string()]);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **A command is the job's whose ID its description names first** (review
    /// rv-20261022T155431Z-bc0b2065-3075#1): B's command, begun with B's ID as rich.md and
    /// stu.md say, that mentions A's ID later ("check A's output") is B's, in both selectors.
    #[test]
    fn a_command_mentioning_another_job_later_stays_with_the_job_it_begins_with() {
        let h = harness(0);
        let a = assignment::register(&h.state, &registration(&h)).unwrap();
        let b = assignment::register(&h.state, &Registration { obligation_id: "obligation-b".into(), ..registration(&h) }).unwrap();
        let all = assignment::read_all(&h.state, "depot", "thread-one").unwrap();
        let commands = vec![crate::steering::OwnCommand { tool_use_id: String::new(), task_id: "B-foreground".into(),
            started_at_ms: u64::MAX, description: "obligation-b: validate the output produced by obligation-7".into() }];
        for (who, selector) in [("Stu", job_commands as fn(&[assignment::Assignment], &str, &[crate::steering::OwnCommand]) -> Vec<String>),
            ("Rich", desk_commands)] {
            assert_eq!(selector(&all, &a.id, &commands), Vec::<String>::new(), "{who}: Stop A took B's command");
            assert_eq!(selector(&all, &b.id, &commands), vec!["B-foreground".to_string()], "{who}: Stop B missed its own command");
        }
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **Stop pressed between the call and its task** (review rv-20261022T123741Z-7987beff-17a0,
    /// finding 2: a foreground task_started comes about 3 s after the Bash call). The Stop waits
    /// for that call's task and ends it the moment the id arrives. RED at 5dc9485cb: the Stop
    /// saw no task, sent nothing, and the command outlived the bound.
    #[test]
    fn a_jobs_stop_pressed_before_the_commands_task_still_ends_it() {
        let h = harness(0);
        let job = assignment::register(&h.state, &registration(&h)).unwrap();
        stop_ends_the_command(&h, &job.id, "obligation-7: run the heartbeat", false, true);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// Rich and Stu are both told to name the job in each command's description: that is what
    /// lets Stop tell two jobs' commands apart (review rv-20261022T123741Z-7987beff-17a0,
    /// finding 1: Rich's two jobs from one turn, ordinary descriptions, stopped nothing).
    #[test]
    fn rich_and_stu_name_the_job_in_each_commands_description() {
        let line = "Begin the description of every shell command you run for a job with that job's obligation_id";
        for agent in ["rich", "stu"] {
            let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR"))
                .join(format!("../../../engine/app/agents/{agent}.md"))).unwrap();
            assert!(text.contains(line), "{agent}.md does not tell him to name the job in a command's description");
        }
    }

    /// The review of 9b7b70f64 (rv-20261022T093512Z-9b7b70f6-d496, finding 1): job B, open but
    /// only waiting for his decision since before A's command started, kept A's command out of
    /// A's Stop ("A foreground still running after 2015 ms: true"), even a command naming no
    /// job. B is left as it was. RED at 9b7b70f64: any other open job made the command nobody's.
    #[test]
    fn a_jobs_stop_ends_its_foreground_command_while_another_job_waits_for_a_decision() {
        let h = harness(0);
        let waiting = assignment::register(&h.state, &Registration { obligation_id: "waiting-for-him".into(), ..registration(&h) }).unwrap();
        assignment::advance(&h.state, "depot", "thread-one", &waiting.id, AssignmentState::Blocked, "Which branch first?").unwrap();
        let job = assignment::register(&h.state, &registration(&h)).unwrap();
        stop_ends_the_foreground_command(&h, &job.id, "Run the heartbeat", false);
        let other = assignment::read(&h.state, "depot", "thread-one", &waiting.id).unwrap();
        assert_eq!((other.state, other.user_stop_requested_at_ms), (AssignmentState::Blocked, None), "the waiting job is not touched");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// Stu runs one foreground command (a real `sleep`) beside a helper's; Stop on `job` must
    /// end Stu's within the reap walk's bound, with only that command's `stop_task`.
    /// With `front_desk`, the command is Rich's, made in the turn that gave the job and still
    /// running, and the Stop reaches his session instead of Stu's.
    fn stop_ends_the_foreground_command(h: &Harness, job: &str, description: &str, front_desk: bool) {
        stop_ends_the_command(h, job, description, front_desk, false)
    }

    /// With `before_its_task`, Stop is pressed after the call and before its `task_started`,
    /// which the stand-in sends 1 s later (Claude Code's foreground task comes seconds after
    /// the call). Without it the task still comes 0.2 s after the call: on a loaded Mac
    /// (2026-10-23) the test read the call before its task and failed with task_id "".
    fn stop_ends_the_command(h: &Harness, job: &str, description: &str, front_desk: bool, before_its_task: bool) {
        let (client, dir) = crate::native::native_driver_tests::fixture_client("stop-foreground-command", &r#"
here=$(dirname "$0")
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
sleep 300 &
command=$!
echo "$command" > "$here/command.pid"
printf '%s\n' '{"type":"assistant","message":{"id":"msg_stu","model":"claude-opus-5-5","role":"assistant","content":[{"type":"tool_use","id":"toolu_fg","name":"Bash","input":{"command":"python3 heartbeat.py beat stop-fg /tmp/reap-walk --seconds 300","description":"DESCRIPTION"}}],"usage":{"input_tokens":0,"output_tokens":1}},"parent_tool_use_id":null}'
printf '%s\n' '{"type":"assistant","message":{"id":"msg_helper","model":"claude-opus-5-5","role":"assistant","content":[{"type":"tool_use","id":"toolu_helper","name":"Bash","input":{"command":"sleep 1"}}],"usage":{"input_tokens":0,"output_tokens":1}},"parent_tool_use_id":"toolu_worker"}'
printf '%s\n' '{"type":"system","subtype":"task_started","task_type":"local_bash","task_id":"bhelper","tool_use_id":"toolu_helper","is_backgrounded":false}'
TASK_DELAY
printf '%s\n' '{"type":"system","subtype":"task_started","task_type":"local_bash","task_id":"bforeground","tool_use_id":"toolu_fg","is_backgrounded":false}'
read -r stop
case "$stop" in *'"subtype":"stop_task","task_id":"bforeground"'*) ;; *) exit 9;; esac
kill "$command"
wait "$command" 2>/dev/null
id=$(printf '%s' "$stop" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
printf '%s\n' "{\"type\":\"control_response\",\"response\":{\"subtype\":\"success\",\"request_id\":\"$id\",\"response\":{}}}"
printf '%s\n' '{"type":"system","subtype":"task_notification","task_id":"bforeground","tool_use_id":"toolu_fg","status":"stopped"}'
printf '%s\n' '{"type":"result","subtype":"success","stop_reason":"end_turn"}'
read -r keep_alive
"#.replace("DESCRIPTION", description).replace("TASK_DELAY", if before_its_task { "sleep 1" } else { "sleep 0.2" }));
        let handle = client.cancel_handle();
        let turn = std::thread::spawn(move || {
            let ended = client.prompt("run the job's commands", &mut |_| {});
            (client, ended)
        });
        // The call is seen before its task_started is read, so wait for the task too.
        let task = if before_its_task { "" } else { "bforeground" };
        until("Stu's foreground command never started", || {
            let own = handle.own_commands();
            own.len() == 1 && own[0].task_id == task
        });
        assert_eq!(handle.own_commands()[0].tool_use_id, "toolu_fg", "a helper's command is not Stu's own");
        assert_eq!(handle.own_commands()[0].description, description, "the job travels in the call's own description");
        let pid: i32 = std::fs::read_to_string(dir.join("command.pid")).unwrap().trim().parse().unwrap();
        let alive = || unsafe { libc::kill(pid, 0) } == 0;
        assert!(alive(), "the stand-in command must be running before Stop");
        let stopper: Arc<dyn TurnCancel> = handle.clone();
        let desk = if front_desk {
            // Rich's turn that gave the job is still running: the job's ledger ref names it.
            Some(crate::steering::FrontDesk { handle: stopper, turn: Some(crate::steering::ActiveTurn {
                turn_id: "turn-7".into(), thread_id: "thread-one".into(), entity_id: None, started_at: None }) })
        } else {
            let backend = Backend::new();
            {
                let mut inner = backend.inner.lock().unwrap();
                inner.lease_session = Some("stu-session".into());
                inner.peer_stopper = Some(("stu-session".into(), stopper));
            }
            h.host.backends.lock().unwrap().insert("thread-one".into(), backend);
            None
        };
        let pressed = std::time::Instant::now();
        h.host.stop_assignment_and_front_desk("depot", "thread-one", job, desk).unwrap();
        // The reap walk's bound (reap-walk.py BOUND_MS), from the press.
        while alive() && pressed.elapsed() < std::time::Duration::from_millis(2000) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let gone_after = pressed.elapsed();
        if alive() {
            unsafe { libc::kill(pid, libc::SIGKILL) };
            panic!("the job's foreground command (pid {pid}) was still running {gone_after:?} after Stop");
        }
        let (_client, ended) = turn.join().unwrap();
        assert_eq!(ended.unwrap(), "end_turn", "Stu's turn goes on past the stopped command; it is not interrupted");
        let stopped = assignment::read(&h.state, "depot", "thread-one", job).unwrap();
        assert_eq!(stopped.state, AssignmentState::Interrupted);
        assert_eq!(stopped.detail, "The job's recorded work stopped. Stu has been notified.");
    }

    /// **Stop on Stu's FOREGROUND command leaves an unrelated worker running** (§129 question,
    /// 2026-10-30). The stand-in `claude` replays what 2.1.314 did on the real provider
    /// (`scripts/qa/stop-probe.py`): Stu's Bash runs in the foreground, as it would with the
    /// §129 forced background switched off, and its own `local_bash` task arrives
    /// (`is_backgrounded: false`); `stop_task` for that task ends only that command; and an
    /// `interrupt` ends every task of the session, the other job's background worker included
    /// (measured: the worker was `stopped` 8 ms after the interrupt). So the worker survives
    /// only if the app's Stop reaches the command by its own task and never interrupts Stu.
    #[test]
    fn a_jobs_stop_of_stus_foreground_command_leaves_another_jobs_worker_running() {
        use sha2::Digest;
        let h = harness(0);
        let other = assignment::register(&h.state, &Registration { obligation_id: "obligation-70".into(), ..registration(&h) }).unwrap();
        assignment::advance(&h.state, "depot", "thread-one", &other.id, AssignmentState::Running, "Working.").unwrap();
        let job = assignment::register(&h.state, &registration(&h)).unwrap();
        // The other job's helper, as the engine's receipt records it, on Stu's own session.
        let folder = h.state.join("work-receipts").join(format!("{:x}", sha2::Sha256::digest(b"[\"depot\",\"thread-one\"]")));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("other-helper.json"), serde_json::json!({"schema":1,"id":"other-helper",
            "binding":{"entity_id":"depot","thread_id":"thread-one","session_id":"stu-session"},
            "request":{"obligation_id":"obligation-70","role":"worker","title":"Fictional","repo":"/fictional/project"},
            "status":"running","agent_id":"aworker","tool_use_id":"toolu_worker"}).to_string()).unwrap();
        let (client, dir) = crate::native::native_driver_tests::fixture_client("stop-fg-keeps-worker", r#"
here=$(dirname "$0")
read -r init
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}'
read -r prompt
sleep 300 &
worker=$!
echo "$worker" > "$here/worker.pid"
printf '%s\n' '{"type":"assistant","message":{"id":"msg_w","model":"claude-opus-5-5","role":"assistant","content":[{"type":"tool_use","id":"toolu_worker","name":"Agent","input":{"description":"obligation-70: build","prompt":"p","subagent_type":"general-purpose","run_in_background":true}}],"usage":{"input_tokens":0,"output_tokens":1}},"parent_tool_use_id":null}'
printf '%s\n' '{"type":"system","subtype":"task_started","task_id":"aworker","tool_use_id":"toolu_worker","description":"obligation-70: build","subagent_type":"general-purpose","is_backgrounded":true}'
printf '%s\n' '{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_worker","type":"tool_result","content":[{"type":"text","text":"Async agent launched successfully."}]}]},"parent_tool_use_id":null}'
sleep 300 &
command=$!
echo "$command" > "$here/command.pid"
printf '%s\n' '{"type":"assistant","message":{"id":"msg_fg","model":"claude-opus-5-5","role":"assistant","content":[{"type":"tool_use","id":"toolu_fg","name":"Bash","input":{"command":"bash ./long-task.sh 60","description":"obligation-7: run the long task","timeout":120000}}],"usage":{"input_tokens":0,"output_tokens":1}},"parent_tool_use_id":null}'
sleep 0.2
printf '%s\n' '{"type":"system","subtype":"task_started","task_id":"bforeground","tool_use_id":"toolu_fg","description":"obligation-7: run the long task","is_backgrounded":false,"task_type":"local_bash"}'
while read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"request_id":"\([^"]*\)".*/\1/p')
  case "$line" in
    *'"subtype":"interrupt"'*)
      kill "$command" "$worker"
      printf '%s\n' "{\"type\":\"control_response\",\"response\":{\"subtype\":\"success\",\"request_id\":\"$id\"}}"
      printf '%s\n' '{"type":"system","subtype":"task_notification","task_id":"aworker","tool_use_id":"toolu_worker","status":"stopped","summary":"obligation-70: build"}'
      printf '%s\n' '{"type":"system","subtype":"task_notification","task_id":"bforeground","tool_use_id":"toolu_fg","status":"stopped","summary":"obligation-7: run the long task"}'
      printf '%s\n' '{"type":"result","subtype":"error_during_execution","terminal_reason":"aborted_tools"}' ;;
    *'"subtype":"stop_task","task_id":"bforeground"'*)
      kill "$command"
      printf '%s\n' "{\"type\":\"control_response\",\"response\":{\"subtype\":\"success\",\"request_id\":\"$id\",\"response\":{}}}"
      printf '%s\n' '{"type":"system","subtype":"task_notification","task_id":"bforeground","tool_use_id":"toolu_fg","status":"stopped","summary":"obligation-7: run the long task"}'
      printf '%s\n' '{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"Exit code 137\n\n[killed]","is_error":true,"tool_use_id":"toolu_fg"}]},"parent_tool_use_id":null}'
      printf '%s\n' '{"type":"result","subtype":"success","stop_reason":"end_turn"}' ;;
    *'"subtype":"stop_task","task_id":"aworker"'*)
      kill "$worker"
      printf '%s\n' "{\"type\":\"control_response\",\"response\":{\"subtype\":\"success\",\"request_id\":\"$id\",\"response\":{}}}"
      printf '%s\n' '{"type":"system","subtype":"task_notification","task_id":"aworker","tool_use_id":"toolu_worker","status":"stopped","summary":"obligation-70: build"}' ;;
  esac
done
"#);
        let handle = client.cancel_handle();
        let turn = std::thread::spawn(move || {
            let ended = client.prompt("run the job's commands", &mut |_| {});
            (client, ended)
        });
        until("Stu's foreground command never got its task", || {
            let own = handle.own_commands();
            own.len() == 1 && own[0].task_id == "bforeground"
        });
        let pid = |name: &str| -> i32 { std::fs::read_to_string(dir.join(name)).unwrap().trim().parse().unwrap() };
        let (command, worker) = (pid("command.pid"), pid("worker.pid"));
        let alive = |pid: i32| unsafe { libc::kill(pid, 0) } == 0;
        assert!(alive(command) && alive(worker), "the stand-in command and worker must be running before Stop");
        assert_eq!(handle.worker_status().unwrap().active, 1, "the other job's worker is running before Stop");
        let stopper: Arc<dyn TurnCancel> = handle.clone();
        let backend = Backend::new();
        {
            let mut inner = backend.inner.lock().unwrap();
            inner.lease_session = Some("stu-session".into());
            inner.peer_stopper = Some(("stu-session".into(), stopper));
        }
        h.host.backends.lock().unwrap().insert("thread-one".into(), backend);
        let pressed = std::time::Instant::now();
        h.host.stop_assignment("depot", "thread-one", &job.id).unwrap();
        while alive(command) && pressed.elapsed() < std::time::Duration::from_millis(2000) {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let command_ran = alive(command);
        // Anything that would reach the worker has had its chance by now.
        std::thread::sleep(std::time::Duration::from_millis(500));
        let worker_ran = alive(worker);
        let (_client, ended) = turn.join().unwrap();
        for pid in [command, worker] { unsafe { libc::kill(pid, libc::SIGKILL) }; }
        assert!(!command_ran, "Stu's foreground command was still running {:?} after Stop", pressed.elapsed());
        assert!(worker_ran, "the other job's worker died when Stu's foreground command was stopped");
        assert_eq!(ended.unwrap(), "end_turn", "Stu's turn goes on past the stopped command; it is not interrupted");
        assert_eq!(assignment::read(&h.state, "depot", "thread-one", &job.id).unwrap().state, AssignmentState::Interrupted);
        assert_eq!(assignment::read(&h.state, "depot", "thread-one", &other.id).unwrap().state, AssignmentState::Running,
            "the other job is untouched by this job's Stop");
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// Which commands a job's Stop takes. A command naming the job is its, one naming another
    /// job never is, whatever else is open; one naming no job is its only when no other job
    /// could be running it.
    #[test]
    fn a_jobs_stop_takes_its_own_commands_and_never_another_jobs() {
        let h = harness(0);
        let other = assignment::register(&h.state, &Registration { obligation_id: "obligation-70".into(), ..registration(&h) }).unwrap();
        assignment::advance(&h.state, "depot", "thread-one", &other.id, AssignmentState::Running, "Working.").unwrap();
        let job = assignment::register(&h.state, &registration(&h)).unwrap();
        let read = |id: &str| assignment::read(&h.state, "depot", "thread-one", id).unwrap();
        let all = || assignment::read_all(&h.state, "depot", "thread-one").unwrap();
        let at = read(&job.id).registered_at_ms;
        let command = |task: &str, started_at_ms: u64, description: &str| crate::steering::OwnCommand { tool_use_id: String::new(),
            task_id: task.into(), started_at_ms, description: description.into() };
        let commands = vec![
            command("names-this-job", at, "obligation-7: build the site"),
            command("names-the-other-job", at, "Tests for obligation-70"),
            command("names-no-job", at, "List the files"),
            command("before-this-job", at.saturating_sub(1), "List the files"),
        ];
        let taken = |records: &[assignment::Assignment]| job_commands(records, &job.id, &commands);
        // The other job runs: only the command naming this job ("obligation-7" is not in "obligation-70").
        assert_eq!(taken(&all()), vec!["names-this-job".to_string()]);
        // Its Stop takes its own command, and the unnamed one from before this job existed,
        // when it was the only job open; none of this job's.
        assert_eq!(job_commands(&all(), &other.id, &commands), vec!["names-the-other-job".to_string(), "before-this-job".to_string()],
            "the other job's Stop takes its own commands and none of this job's");
        // The other job waits for his decision since before the commands: it runs none of them.
        assignment::advance(&h.state, "depot", "thread-one", &other.id, AssignmentState::Blocked, "Which branch first?").unwrap();
        let since = read(&other.id).updated_at_ms;
        let later = vec![command("names-no-job", since, "List the files"), command("names-the-other-job", since, "Tests for obligation-70")];
        assert_eq!(job_commands(&all(), &job.id, &later), vec!["names-no-job".to_string()]);
        // A command naming no job that started while the other job still ran stays with Stu.
        assert!(!taken(&all()).contains(&"names-no-job".to_string()));
        // The other job has closed: its command is still never this job's.
        assignment::record_user_stop(&h.state, "depot", "thread-one", &other.id).unwrap();
        assert_eq!(job_commands(&all(), &job.id, &later), vec!["names-no-job".to_string()]);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    // ---- P0, 2026-10-28: open records whose work has all ended -------------------------------

    /// A job of his handed to Stu exactly as a conversation does it, with nothing delivered yet.
    fn handed_job(h: &Harness) -> (crate::assignment::Receipt, crate::status_tools::StatusToolScope, PathBuf) {
        let record = assignment::register(&h.state, &registration(h)).unwrap();
        let scope = h.host.message_scope(&h.binding);
        let path = h.root.join("peer-status.json");
        crate::session_tools::bind(&path, &scope, "front-session", "rich", Some(crate::ecs::UserInstruction {
            ledger_ref: registration(h).instruction_ledger_ref, sha256: registration(h).instruction_sha256, text: None,
        })).unwrap();
        crate::session_tools::delegate(&path, &scope, serde_json::json!({"text":"Land the three branches"})).unwrap();
        (record, scope, path)
    }

    /// One of the job's helpers, as the engine's receipt records it.
    fn helper_receipt(h: &Harness, status: &str) {
        use sha2::Digest;
        let folder = h.state.join("work-receipts").join(format!("{:x}", sha2::Sha256::digest(b"[\"depot\",\"thread-one\"]")));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("helper.json"), serde_json::json!({"schema":1,"id":"helper",
            "binding":{"entity_id":"depot","thread_id":"thread-one","session_id":"work-session-one"},
            "request":{"obligation_id":registration(h).obligation_id,"role":"worker","title":"Fictional","repo":"/fictional/project"},
            "status":status,"agent_id":"agent-one","tool_use_id":"call-one"}).to_string()).unwrap();
    }

    fn gate(h: &Harness) -> (crate::work_gate::Liveness, Option<String>) {
        crate::work_gate::background(&h.host.background_work(), &[])
    }

    /// **(a) The update panel's "6 assignments still running in the background" at 23:00Z.** A
    /// job whose handoff Stu was given, whose turn ended and whose helper has ended is open on its
    /// own record and is NOT running: nothing of it is live, the record survives a relaunch and is
    /// handed back to Stu then. The same record reads running again the moment something of it is
    /// live: a helper whose session cannot be shown to have ended, or input Stu has not yet had.
    #[test]
    fn an_open_job_whose_work_has_all_ended_is_not_running_and_one_with_live_work_is() {
        let h = harness(0);
        let (record, scope, path) = handed_job(&h);
        assignment::advance(&h.state, "depot", "thread-one", &record.id, AssignmentState::Running, "Worker start observed").unwrap();
        h.host.wake_messages(&h.binding);
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(30)), "Stu was never given the job");
        // A helper ran and its session's own reading shows every run ended.
        helper_receipt(&h, "running");
        every_worker_observed_ending(&h);
        assert!(assignment::open(&h.state).unwrap().iter().any(|open| open.id == record.id), "the record must stay open");
        let work = h.host.background_work();
        assert_eq!((work.running, work.awaiting_you, work.readable), (0, 0, true),
            "an open record whose every turn and helper has ended was counted as running");
        assert_eq!(gate(&h), (crate::work_gate::Liveness::Clear, None));
        assert!(!h.host.peer_work_pending(), "a handoff Stu was given read as input still waiting for him");
        // The SAME record, with a helper whose end cannot be shown: running.
        std::fs::remove_dir_all(h.state.join("evidence/work-session-one")).unwrap();
        assert_eq!(h.host.background_work().running, 1, "a helper not witnessed ending was read as ended");
        every_worker_observed_ending(&h);
        // The SAME record, with input for Stu he has not been given yet: running.
        h.host.shutdown();
        crate::session_tools::send(&path, &scope, serde_json::json!({"text":"Keep the original release date"})).unwrap();
        let restarted = WorkHost::new(&h.state, Box::new(Factory {
            spawns: h.spawns.clone(), prompts: h.work_prompts.clone(), commands: h.commands.clone(),
            team: h.team.clone(), gate: h.start_gate.clone(),
        }), Arc::new(SilentNotifier));
        assert_eq!(restarted.background_work().running, 1, "input Stu has not had yet was read as nothing to do");
        assert_eq!(crate::work_gate::background(&restarted.background_work(), &[]).1.as_deref(),
            Some("1 assignment is still running in the background."));
        restarted.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **(b) Between its registration and its first worker it still blocks** (`work_gate.rs`,
    /// §6.4; the CEO's "ALL WORK"). Registered with no handoff yet, and handed over but not yet
    /// given to Stu, are both running.
    #[test]
    fn a_job_registered_with_no_worker_yet_still_blocks() {
        let h = harness(0);
        assignment::register(&h.state, &registration(&h)).unwrap();
        assert_eq!(h.host.background_work().running, 1, "a registered job with nothing started yet read as idle");
        assert_eq!(gate(&h).0, crate::work_gate::Liveness::Busy);
        assert_eq!(gate(&h).1.as_deref(), Some("1 assignment is still running in the background."));
        std::fs::remove_dir_all(&h.root).unwrap();
        let h = harness(0);
        let (_, _, _) = handed_job(&h);
        assert_eq!(h.host.background_work().running, 1, "a handoff not yet given to Stu read as idle");
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **(c) Waiting for his approval blocks, with its own sentence** (§6.5), exactly as before.
    #[test]
    fn a_job_awaiting_his_approval_still_blocks_with_its_own_sentence() {
        let h = harness(0);
        let record = assignment::register(&h.state, &registration(&h)).unwrap();
        assignment::advance(&h.state, "depot", "thread-one", &record.id, AssignmentState::Blocked, "Waiting for approval").unwrap();
        let work = h.host.background_work();
        assert_eq!((work.running, work.awaiting_you), (0, 1));
        assert_eq!(gate(&h), (crate::work_gate::Liveness::Busy, Some("An assignment is waiting for you to approve it.".into())));
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **(d) A helper's session that has demonstrably ended is not running** (second review
    /// rv-20261029T004310Z-6c4c304f-d238 of `6c4c304f4`, from its fixture `ended-session.rs`).
    /// A provider that ends on a lost connection or a restart need not deliver its helper's
    /// `SubagentStop`; it leaves `SessionEnd` in the session's callback journal and `ended_at`
    /// on its canonical session record (`workspaces.py` `record_session_end`), or its recorded
    /// process is gone. The helper's receipt still says `running`, and that session's worker
    /// reading still shows the helper launched and not stopped: on `6c4c304f4` the update gate
    /// read "1 assignment is still running in the background." for as long as the receipt
    /// stayed. It does not run now. A session with no terminal evidence, one that started
    /// again, one this host has live, and one whose recorded process still runs keep counting.
    #[test]
    fn a_helper_whose_provider_session_has_ended_is_not_running_and_a_live_session_still_is() {
        use sha2::Digest;
        let h = harness(0);
        assignment::register(&h.state, &registration(&h)).unwrap();
        helper_receipt(&h, "running");
        let partition = format!("{:x}", sha2::Sha256::digest(b"[\"depot\",\"thread-one\"]"));
        let sessions = h.state.join("workspaces").join(&partition).join("sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        let record = sessions.join("work-session-one.json");
        let evidence = h.state.join("evidence/work-session-one");
        std::fs::create_dir_all(&evidence).unwrap();
        std::fs::write(evidence.join(".lock"), "").unwrap();
        let row = |event: &str, more: serde_json::Value| {
            let mut callback = serde_json::json!({"session_id": "work-session-one", "hook_event_name": event});
            for (key, value) in more.as_object().unwrap() { callback[key] = value.clone(); }
            serde_json::json!({"schema": 1, "callback": callback}).to_string() + "\n"
        };
        let launched = row("SubagentStart", serde_json::json!({"agent_id": "agent-one"}))
            + &row("PostToolUse", serde_json::json!({"tool_name": "Agent",
                "tool_response": {"status": "async_launched", "agentId": "agent-one"}}));
        let ended = launched.clone() + &row("SessionEnd", serde_json::json!({"reason": "other"}));
        let journal = |text: &str| std::fs::write(evidence.join("callbacks.jsonl"), text).unwrap();
        let canonical = |value: serde_json::Value| std::fs::write(&record, value.to_string()).unwrap();
        let end_record = serde_json::json!({"session_id": "work-session-one",
            "ended_at": "2026-10-29T00:00:00Z", "end_reason": "SessionEnd"});
        let busy = (crate::work_gate::Liveness::Busy, Some("1 assignment is still running in the background.".to_string()));
        let clear = (crate::work_gate::Liveness::Clear, None);

        // Control: the session is live by every reading there is. Running.
        journal(&launched);
        assert_eq!((h.host.background_work().running, gate(&h)), (1, busy.clone()),
            "a helper of a session with no terminal evidence was read as ended");
        // The reviewer's case: SessionEnd and the canonical end, no provider running. Not running.
        journal(&ended);
        canonical(end_record.clone());
        assert_eq!((h.host.background_work().running, gate(&h)), (0, clear.clone()),
            "a helper of a provider session that has ended still blocked the update");
        // Either witness alone is terminal evidence.
        std::fs::remove_file(&record).unwrap();
        assert_eq!(h.host.background_work().running, 0, "SessionEnd in the journal was not read as the session's end");
        journal(&launched);
        canonical(end_record.clone());
        assert_eq!(h.host.background_work().running, 0, "the canonical ended_at record was not read as the session's end");
        // Its recorded process is gone: an identity that is no longer that process.
        let own = std::process::id();
        canonical(serde_json::json!({"session_id": "work-session-one", "pid": own, "pid_start": "Thu Jan  1 00:00:00 1970"}));
        assert_eq!(h.host.background_work().running, 0, "a session whose recorded process is gone was read as live");

        // Controls: each live session keeps counting.
        // It started again after its end (resumed): SessionStart follows, and the record drops ended_at.
        journal(&(ended.clone() + &row("SessionStart", serde_json::json!({"source": "resume"}))));
        std::fs::remove_file(&record).unwrap();
        assert_eq!(h.host.background_work().running, 1, "a session that started again after its end was read as ended");
        // Its recorded provider process still runs (this process, with its own start).
        journal(&ended);
        let start = crate::app_cleanup::own_start().expect("this process's start");
        canonical(serde_json::json!({"session_id": "work-session-one", "pid": own, "pid_start": start,
            "ended_at": "2026-10-29T00:00:00Z", "end_reason": "SessionEnd"}));
        assert_eq!((h.host.background_work().running, gate(&h)), (1, busy.clone()),
            "a session whose provider process still runs was read as ended");
        // This host holds it open as Stu's lease: the lease itself exists and is not closed.
        canonical(end_record);
        let backend = Backend::new();
        backend.inner.lock().unwrap().lease_session = Some("work-session-one".into());
        *backend.lease.lock().unwrap() = Some(Box::new(Lease { session: "work-session-one".into(),
            prompts: h.work_prompts.clone(), commands: h.commands.clone(), team: h.team.clone(), settings: None }));
        h.host.backends.lock().unwrap().insert("thread-one".into(), backend.clone());
        assert_eq!((h.host.background_work().running, gate(&h)), (1, busy.clone()),
            "a session this host has open was read as ended");
        // In use (a turn holds the lease for its whole length): open, by its remembered session.
        let in_use = backend.lease.lock().unwrap();
        assert_eq!((h.host.background_work().running, gate(&h)), (1, busy),
            "a lease in use was read as ended");
        drop(in_use);
        h.host.backends.lock().unwrap().clear();
        assert_eq!(gate(&h), clear);
        h.host.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **A session whose lease has closed, or been removed, has no veto** (second review
    /// rv-20261029T062537Z-21f7f26e-d081 of `21f7f26e5`, from its fixture `closed-lease.rs`).
    /// Stu is handed his job the normal way and his turn ends; his helper's receipt still says
    /// `running`. While his provider is open, even `SessionEnd` and `ended_at` do not end it.
    /// Then his provider exits (a lost connection: provider EOF), with no input waiting: on
    /// `21f7f26e5` the remembered session id kept "1 assignment is still running in the
    /// background." until another input opened a replacement, and still did after shutdown removed
    /// the lease, where a fresh host over the same disk read clear. The gate is read as
    /// `updates.rs` reads it, with every lease's own worker view.
    #[test]
    fn a_session_whose_lease_closed_or_was_removed_has_no_veto_and_an_open_lease_keeps_one() {
        use sha2::Digest;
        let h = harness(0);
        let _ = handed_job(&h);
        h.host.wake_messages(&h.binding);
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(30)), "Stu was never given the job");
        assert!(!h.host.peer_work_pending());
        helper_receipt(&h, "running");
        let evidence = h.state.join("evidence/work-session-one");
        std::fs::create_dir_all(&evidence).unwrap();
        std::fs::write(evidence.join(".lock"), "").unwrap();
        let row = |event: &str, more: serde_json::Value| {
            let mut callback = serde_json::json!({"session_id": "work-session-one", "hook_event_name": event});
            for (key, value) in more.as_object().unwrap() { callback[key] = value.clone(); }
            serde_json::json!({"schema": 1, "callback": callback}).to_string() + "\n"
        };
        let launched = row("SubagentStart", serde_json::json!({"agent_id": "agent-one"}))
            + &row("PostToolUse", serde_json::json!({"tool_name": "Agent",
                "tool_response": {"status": "async_launched", "agentId": "agent-one"}}));
        let read = |host: &WorkHost| {
            let work = host.background_work();
            let views: Vec<_> = host.lease_sessions().iter()
                .map(|session| crate::app_workers::status(&h.state, Some(session))).collect();
            (work.running, crate::work_gate::background(&work, &views))
        };
        let busy = (1, (crate::work_gate::Liveness::Busy, Some("1 assignment is still running in the background.".to_string())));
        let clear = (0, (crate::work_gate::Liveness::Clear, None));

        // Control: his provider is open and his helper launched. Running.
        std::fs::write(evidence.join("callbacks.jsonl"), &launched).unwrap();
        assert_eq!(read(&h.host), busy, "a helper of Stu's open session was read as ended");
        // Both terminal witnesses, his provider still open: the open lease keeps its veto.
        std::fs::write(evidence.join("callbacks.jsonl"), launched + &row("SessionEnd", serde_json::json!({"reason": "other"}))).unwrap();
        let sessions = h.state.join("workspaces")
            .join(format!("{:x}", sha2::Sha256::digest(b"[\"depot\",\"thread-one\"]"))).join("sessions");
        std::fs::create_dir_all(&sessions).unwrap();
        std::fs::write(sessions.join("work-session-one.json"), serde_json::json!({"session_id": "work-session-one",
            "ended_at": "2026-10-29T00:00:00Z", "end_reason": "SessionEnd"}).to_string()).unwrap();
        assert_eq!(crate::app_workers::session_ended(&h.state, "work-session-one"), Some(true));
        assert_eq!(h.host.lease_sessions(), vec!["work-session-one".to_string()]);
        assert_eq!(read(&h.host), busy, "a session this host holds open lost its veto");

        // The reviewer's case: provider EOF, nothing waiting for Stu. The ended session decides.
        h.team.lock().unwrap().closed = true;
        assert!(!h.host.peer_work_pending());
        assert_eq!(read(&h.host), clear, "a closed lease's remembered session kept the update blocked");
        assert_eq!(h.host.lease_sessions(), Vec::<String>::new(), "a closed lease was counted as open");
        // Shutdown removes the lease itself: still clear.
        h.host.shutdown();
        assert_eq!(read(&h.host), clear, "a removed lease's remembered session kept the update blocked");
        // The same disk state in a fresh host reads the same.
        let restarted = WorkHost::new(&h.state, Box::new(Factory {
            spawns: h.spawns.clone(), prompts: h.work_prompts.clone(), commands: h.commands.clone(),
            team: h.team.clone(), gate: h.start_gate.clone(),
        }), Arc::new(SilentNotifier));
        assert_eq!(read(&restarted), clear);
        restarted.shutdown();
        std::fs::remove_dir_all(&h.root).unwrap();
    }

    /// **A handoff the session already has is not handed to it again as new work** (2026-10-28
    /// ~23:00Z: after a restart Stu was given the nightly-78 handoff three times within minutes,
    /// two of them carrying no `previous_delivery_attempt`). His lease is renewed onto the same
    /// session, which clears the live cursor; the open handoff is not given to that session again
    /// in this process. After a restart it is given once more, naming the earlier attempt.
    #[test]
    fn a_handoff_stu_already_has_is_never_given_again_as_new_work() {
        let h = harness(0);
        h.team.lock().unwrap().resume_same_session = true;
        let (record, scope, path) = handed_job(&h);
        assignment::advance(&h.state, "depot", "thread-one", &record.id, AssignmentState::Running, "Worker start observed").unwrap();
        h.host.wake_messages(&h.binding);
        assert!(h.host.wait_for_completed(1, std::time::Duration::from_secs(30)));
        let backend = h.host.backend("thread-one").unwrap();
        assert!(matches!(h.host.renew_lease(&backend, &h.binding), Renewal::Moved { .. }));
        crate::session_tools::send(&path, &scope, serde_json::json!({"text":"Keep the original release date"})).unwrap();
        h.host.wake_messages(&h.binding);
        until("later input must reach Stu", || handed_over(&h, "Keep the original release date") == 1);
        assert!(h.host.wait_for_completed(2, std::time::Duration::from_secs(30)));
        h.host.shutdown();
        let handed = |h: &Harness| -> Vec<serde_json::Value> {
            h.work_prompts.lock().unwrap().iter().filter(|p| p.contains("Land the three branches"))
                .map(|p| serde_json::from_str(p).unwrap()).collect()
        };
        assert_eq!(handed(&h).len(), 1, "the same session was given the same handoff again: {:?}", h.work_prompts.lock().unwrap());
        assert!(handed(&h)[0].get("previous_delivery_attempt").is_some_and(serde_json::Value::is_null),
            "a first delivery must say it is a first one: {:?}", handed(&h)[0]);
        // Restart: the open handoff comes back once, as a reconciliation naming the attempt.
        let restarted = WorkHost::new(&h.state, Box::new(Factory {
            spawns: h.spawns.clone(), prompts: h.work_prompts.clone(), commands: h.commands.clone(),
            team: h.team.clone(), gate: h.start_gate.clone(),
        }), Arc::new(SilentNotifier));
        restarted.wake_messages(&h.binding);
        assert!(restarted.wait_for_completed(1, std::time::Duration::from_secs(30)));
        let backend = restarted.backend("thread-one").unwrap();
        assert!(matches!(restarted.renew_lease(&backend, &h.binding), Renewal::Moved { .. }));
        crate::session_tools::send(&path, &scope, serde_json::json!({"text":"Use the connected repository"})).unwrap();
        restarted.wake_messages(&h.binding);
        until("later input must reach Stu", || handed_over(&h, "Use the connected repository") == 1);
        restarted.shutdown();
        let all = handed(&h);
        assert_eq!(all.len(), 2, "after a restart the open handoff must come back exactly once: {all:?}");
        assert_eq!(all[1]["previous_delivery_attempt"]["session_id"], "work-session-one",
            "a redelivery read as a fresh order: {:?}", all[1]);
        std::fs::remove_dir_all(&h.root).unwrap();
    }
}

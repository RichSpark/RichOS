//! Ordered model/account changes and proactive messages survive native-session migration.
use richos_core::{cognition::{Cognition,CognitionError,LeaseFactory,MockCognition,TurnItem},entity::EntityId,ledger::{Ledger,Source,AttentionTier,TurnState},stream::{StreamEvent,TurnObserver}};
use std::sync::{Arc,Mutex};
mod support;
#[derive(Clone, Default)]
struct RecordingObserver {
    events: Arc<Mutex<Vec<StreamEvent>>>,
}
impl RecordingObserver {
    fn events(&self) -> Vec<StreamEvent> {
        self.events.lock().unwrap().clone()
    }
}
impl TurnObserver for RecordingObserver {
    fn on_event(&self, event: &StreamEvent) {
        self.events.lock().unwrap().push(event.clone());
    }
}

fn femcboost() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}

fn tmp_ledger(tag: &str) -> (std::path::PathBuf, Ledger) {
    let path = std::env::temp_dir().join(format!(
        "richos-rotation-test-{tag}-{}-{}.jsonl",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    let _ = std::fs::remove_file(&path);
    let ledger = Ledger::open(&path).unwrap();
    (path, ledger)
}

#[test]
fn proactive_tier1_and_tier2_render_as_rich_only_messages_no_preceding_ceo_prompt() {
    let (path, ledger) = tmp_ledger("proactive-render");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();

    spine
        .raise_proactive(None, AttentionTier::InterruptNow, "The Acme counter expires at noon — what's your floor?")
        .unwrap();
    spine
        .raise_proactive(None, AttentionTier::Digest, "Morning brief: three things need your attention.")
        .unwrap();

    let msgs = spine.messages(&thread).unwrap();
    assert_eq!(msgs.len(), 2, "both tiers render; NO paired user message (nothing was prompted)");
    assert!(msgs.iter().all(|m| m.role == "assistant"));
    assert_eq!(msgs[0].text, "The Acme counter expires at noon — what's your floor?");
    assert_eq!(msgs[1].text, "Morning brief: three things need your attention.");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn proactive_silent_tier_never_renders_but_stays_durable() {
    // UX §5.1 Tier 3: "Does not appear in the conversation and never notifies." But it
    // must still be durably logged (a CEO who goes looking, or a future activity view).
    let (path, ledger) = tmp_ledger("proactive-silent");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();

    let observer = RecordingObserver::default();
    spine.set_observer(Box::new(observer.clone()));

    spine.raise_proactive(None, AttentionTier::Silent, "FYI: renewed the Acme NDA, nothing needed from you.").unwrap();

    assert!(spine.messages(&thread).unwrap().is_empty(), "Silent tier has NO render path");
    assert!(observer.events().is_empty(), "Silent tier never notifies — no live UI event either");
    // But it's not lost — durable in the raw ledger.
    assert!(spine
        .ledger()
        .turns()
        .iter()
        .any(|t| t.assistant_text.contains("renewed the Acme NDA")));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn proactive_message_raised_mid_turn_is_durable_and_shown_at_once() {
    // Round 16 draws Rich's line beside the "working" row, so a message raised while a turn
    // is in flight is written AND sent to the window at once, exactly once — the next turn
    // boundary does not send it again. This spine is fully synchronous/single-threaded
    // (module doc), so the test uses the documented test-only seam to force the in-flight
    // state deterministically.
    let (path, ledger) = tmp_ledger("proactive-mid-turn");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.attach_lease(Box::new(MockCognition::new("sess-1", vec!["a reply"])));

    let observer = RecordingObserver::default();
    spine.set_observer(Box::new(observer.clone()));

    spine.debug_set_turn_in_progress(true);
    let proactive_turn_id = spine.raise_proactive(None, AttentionTier::Digest, "said while busy").unwrap();
    let sent = |events: &[StreamEvent]| events.iter().filter(|e| matches!(
        e, StreamEvent::ProactiveMessage { turn_id, .. } if turn_id == &proactive_turn_id)).count();
    assert_eq!(sent(&observer.events()), 1, "sent to the window while the turn is (simulated) in flight");
    assert_eq!(spine.messages(&thread).unwrap().len(), 1, "the message is already durable/readable");
    spine.debug_set_turn_in_progress(false);

    // A REAL turn boundary does not send it a second time.
    spine.submit_prompt("hi", Source::Text).unwrap();
    assert_eq!(sent(&observer.events()), 1, "the boundary sent the message again");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn proactive_message_defaults_to_active_thread_when_none_given() {
    let (path, ledger) = tmp_ledger("proactive-default-thread");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    spine.raise_proactive(None, AttentionTier::Digest, "hello").unwrap();
    assert_eq!(spine.messages(&thread).unwrap().len(), 1);
    let _ = std::fs::remove_file(&path);
}

struct AccountLease {
    session_id: String,
    account: String,
    streamed: Arc<Mutex<Option<richos_core::quota::StreamedReading>>>,
    fail_with: Option<String>,
    prompts: Arc<Mutex<Vec<(String, String)>>>,
}
impl Cognition for AccountLease {

    fn session_id(&self) -> &str { &self.session_id }
    fn account(&self) -> Option<&str> { Some(&self.account) }
    fn streamed_usage(&self) -> Option<richos_core::quota::StreamedReading> { self.streamed.lock().unwrap().clone() }
    fn prompt(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        self.prompts.lock().unwrap().push((self.account.clone(), text.to_string()));
        if let Some(error) = &self.fail_with {
            on_item(TurnItem::Text { seq: 0, text: "Partial answer before the limit" });
            return Err(CognitionError::Protocol(error.clone()));
        }
        on_item(TurnItem::Text { seq: 0, text: &format!("answered on account {}", self.account) });
        Ok("end_turn".into())
    }
}

/// The lease factory the desktop shell is, reduced to the fill-first part: every lease is
/// spawned under the account the quota service says is in use (`main.rs`, `spawn_chat`).
struct AccountFactory {
    quota: Arc<richos_core::quota::Service>,
    spawned: Arc<Mutex<Vec<String>>>,
    streamed: Arc<Mutex<Option<richos_core::quota::StreamedReading>>>,
    prompts: Arc<Mutex<Vec<(String, String)>>>,
}
impl LeaseFactory for AccountFactory {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        let account = self.quota.lease_account().id;
        let mut spawned = self.spawned.lock().unwrap();
        spawned.push(account.clone());
        Ok(Box::new(AccountLease {
            session_id: format!("sess-account-{account}-{}", spawned.len()),
            account,
            streamed: self.streamed.clone(),
            fail_with: None,
            prompts: self.prompts.clone(),
        }))
    }
    fn quota(&self) -> Option<Arc<richos_core::quota::Service>> { Some(self.quota.clone()) }
}

/// A quota service with Account 1 and "Work", both read once by a fake `claude` (the
/// `quota/probe.rs` style): Account 1's five-hour window at `one_five`%, Work's at 5%.
fn two_accounts(tag: &str, one_five: f64) -> (std::path::PathBuf, Arc<richos_core::quota::Service>) {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("richos-fill-first-{tag}-{}-{}", std::process::id(), richos_core::util::now_millis()));
    std::fs::create_dir_all(&dir).unwrap();
    let quota = Arc::new(richos_core::quota::Service::open(&dir).unwrap());
    quota.accounts.add("Work").unwrap();
    let bin = dir.join("claude-fixture");
    std::fs::write(&bin, format!(r#"#!/usr/bin/env python3
import json, os, sys
# Work's folder is under this data folder's claude-accounts/. Account 1 inherits the caller's
# CLAUDE_CONFIG_DIR, which may be any .../claude-accounts/<id> of the developer's own.
accounts = os.path.join(os.path.dirname(os.path.realpath(__file__)), "claude-accounts") + os.sep
folder = os.environ.get("CLAUDE_CONFIG_DIR", "")
five = 5 if folder and os.path.realpath(folder).startswith(accounts) else {one_five}
for line in sys.stdin:
    v = json.loads(line)
    payload = {{}} if v["request"]["subtype"] == "initialize" else {{"rate_limits_available": True, "rate_limits": {{
        "five_hour": {{"utilization": five, "resets_at": "2099-01-01T00:00:00Z"}},
        "seven_day": {{"utilization": 40, "resets_at": "2099-01-05T00:00:00Z"}}}}}}
    print(json.dumps({{"type": "control_response", "response": {{"subtype": "success", "request_id": v["request_id"], "response": payload}}}}), flush=True)
"#)).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o700)).unwrap();
    quota.refresh(&bin, true);
    (dir, quota)
}

/// 2099-01-01T00:00:00Z, the fake's five-hour reset, in ms — so a streamed reading is the same
/// window as the probe's.
const FIVE_HOUR_RESET: u64 = 4_070_908_800_000;
fn streamed_five_hour(used: f64, at: u64) -> richos_core::quota::StreamedReading {
    (vec![richos_core::quota::Window { id: "five_hour".into(), label: "Five-hour".into(), used_percent: used,
        resets_at: Some(FIVE_HOUR_RESET), duration_ms: 5 * 3_600_000 }], at)
}

/// **A turn that starts at 93% runs under the next account** (the CEO's correction and
/// ruling §108; setting on Switch). The probe read Account 1 at 60%; then the lease's OWN
/// streamed reading says 93%, newer than the probe. Before the next turn the spine decides on
/// that freshest reading, rotates at the boundary, and the turn is answered under Work. One
/// line in the conversation says so.
#[test]
fn a_turn_that_starts_at_93_percent_runs_under_the_next_account() {
    let (dir, quota) = two_accounts("boundary", 60.);
    quota.set_policy(richos_core::quota::Policy { enabled: true, pause_percent: 93 }).unwrap();
    quota.set_at_threshold(richos_core::claude_accounts::AtThreshold::Switch).unwrap();
    assert_eq!(quota.lease_account().id, "1", "60% has room: Account 1 stays");
    let (path, ledger) = tmp_ledger("fill-first-boundary");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    let streamed = Arc::new(Mutex::new(Some(streamed_five_hour(93., richos_core::util::now_millis() + 1_000))));
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let spawned = Arc::new(Mutex::new(Vec::new()));
    spine.attach_lease(Box::new(AccountLease { session_id: "sess-account-1".into(), account: "1".into(),
        streamed: streamed.clone(), fail_with: None, prompts: prompts.clone() }));
    spine.set_lease_factory(Box::new(AccountFactory { quota: quota.clone(), spawned: spawned.clone(),
        streamed: Arc::new(Mutex::new(None)), prompts: prompts.clone() }));

    spine.submit_prompt("What is on my plate?", Source::Text).unwrap();

    assert_eq!(*spawned.lock().unwrap(), vec!["2".to_string()], "the successor was spawned under Work");
    let his: Vec<_> = prompts.lock().unwrap().iter().filter(|(_, text)| text == "What is on my plate?").cloned().collect();
    assert_eq!(his, vec![("2".to_string(), "What is on my plate?".to_string())], "his turn ran ONLY under Work");
    let msgs = spine.messages(&thread).unwrap();
    assert!(msgs.iter().any(|m| m.text.starts_with("I switched the team to your **Work** account. Account 1 had used 93% of its 5-hour limit")), "{msgs:?}");
    assert!(msgs.iter().any(|m| m.text == "answered on account 2"));
    drop(std::fs::remove_file(&path));
    drop(std::fs::remove_dir_all(dir));
}

/// Native input ownership holds an account change until the current answer finishes.
struct Busy { lease: AccountLease, pending: Arc<std::sync::atomic::AtomicBool>, commands: Arc<Mutex<Option<richos_core::lease_commands::CommandReading>>> }
impl Cognition for Busy {
    fn session_id(&self) -> &str { self.lease.session_id() }
    fn has_pending_input(&self)->bool {self.pending.load(std::sync::atomic::Ordering::SeqCst)}
    fn account(&self) -> Option<&str> { self.lease.account() }
    fn streamed_usage(&self) -> Option<richos_core::quota::StreamedReading> { self.lease.streamed_usage() }
    fn running_commands(&self) -> Option<richos_core::lease_commands::CommandReading> { *self.commands.lock().unwrap() }
    fn prompt(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> { self.lease.prompt(text, on_item) }
}

#[test]
fn an_account_resumes_only_after_running_commands_and_the_answer_are_clear() {
    use richos_core::lease_commands::CommandReading;
    let (dir, quota) = two_accounts("switch-waits", 60.);
    // The five-hour switch acts only while the automatic switch is on (e1ac24d27).
    quota.set_policy(richos_core::quota::Policy { enabled: true, pause_percent: 93 }).unwrap();
    quota.set_at_threshold(richos_core::claude_accounts::AtThreshold::Switch).unwrap();
    let (path, ledger) = tmp_ledger("fill-first-switch-waits");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    let streamed = Arc::new(Mutex::new(Some(streamed_five_hour(93., richos_core::util::now_millis() + 1_000))));
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let spawned = Arc::new(Mutex::new(Vec::new()));
    let commands = Arc::new(Mutex::new(Some(CommandReading::Running(1))));
    spine.attach_lease(Box::new(Busy {
        lease: AccountLease { session_id: "sess-account-1".into(), account: "1".into(),
            streamed: streamed.clone(), fail_with: None, prompts: prompts.clone() },
        commands: commands.clone(), pending: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    }));
    spine.set_lease_factory(Box::new(AccountFactory { quota: quota.clone(), spawned: spawned.clone(),
        streamed: Arc::new(Mutex::new(None)), prompts: prompts.clone() }));

    spine.submit_prompt("Is the build done?", Source::Text).unwrap();
    assert!(spawned.lock().unwrap().is_empty(),"account resume killed the running command");
    *commands.lock().unwrap() = Some(CommandReading::Unreadable);
    spine.submit_prompt("And now?", Source::Text).unwrap();
    assert!(spawned.lock().unwrap().is_empty(),"unreadable commands must not be treated as clear");

    *commands.lock().unwrap() = Some(CommandReading::Clear);
    spine.submit_prompt("It finished", Source::Text).unwrap();
    assert_eq!(*spawned.lock().unwrap(), vec!["2".to_string()]);
    let ran: Vec<_> = prompts.lock().unwrap().iter().filter(|(_, t)| !t.is_empty()).cloned().collect();
    assert!(ran.contains(&("1".to_string(), "Is the build done?".to_string())), "{ran:?}");
    assert!(ran.contains(&("2".to_string(), "It finished".to_string())), "{ran:?}");
    drop(std::fs::remove_file(&path));
    drop(std::fs::remove_dir_all(dir));
}

/// A pending native answer keeps the old account. Its notice waits for a real new-account answer.
#[test]
fn the_switch_notice_waits_until_a_turn_really_runs_under_the_new_account() {
    use richos_core::lease_commands::CommandReading;
    // Round 18's words for the switch, held until a turn runs under Work. The rest of the line
    // says when Account 1 is fresh again, which moves with the clock.
    const NOTICE: &str = "I switched the team to your **Work** account. Account 1 had used 93% of its 5-hour limit";
    let (dir, quota) = two_accounts("notice-waits", 60.);
    // The five-hour switch acts only while the automatic switch is on (e1ac24d27).
    quota.set_policy(richos_core::quota::Policy { enabled: true, pause_percent: 93 }).unwrap();
    quota.set_at_threshold(richos_core::claude_accounts::AtThreshold::Switch).unwrap();
    let (path, ledger) = tmp_ledger("fill-first-notice-waits");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    let streamed = Arc::new(Mutex::new(Some(streamed_five_hour(93., richos_core::util::now_millis() + 1_000))));
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let commands = Arc::new(Mutex::new(Some(CommandReading::Clear)));
    let pending=Arc::new(std::sync::atomic::AtomicBool::new(true));
    spine.attach_lease(Box::new(Busy {
        lease: AccountLease { session_id: "sess-account-1".into(), account: "1".into(),
            streamed: streamed.clone(), fail_with: None, prompts: prompts.clone() },
        commands: commands.clone(), pending:pending.clone(),
    }));
    spine.set_lease_factory(Box::new(AccountFactory { quota: quota.clone(), spawned: Arc::new(Mutex::new(Vec::new())),
        streamed: Arc::new(Mutex::new(None)), prompts: prompts.clone() }));
    let notices = |spine: &richos_core::spine::Spine| spine.messages(&thread).unwrap().iter()
        .filter(|m| m.text.starts_with("I switched the team to")).map(|m| m.text.clone()).collect::<Vec<_>>();

    // ---- the switch is decided, and waits: the turn runs on Account 1 -------------------
    spine.submit_prompt("Is the build done?", Source::Text).unwrap();
    assert_eq!(quota.lease_account().id, "2", "the switch was decided");
    assert_eq!(spine.rotation_count(), 0, "the native answer has not finished");
    assert!(prompts.lock().unwrap().contains(&("1".to_string(), "Is the build done?".to_string())));
    assert_eq!(notices(&spine), Vec::<String>::new(), "\"Switched to\" was said while the turn ran on Account 1");
    // The desktop's quota monitor says pending notices whenever the spine is free.
    spine.raise_quota_notices(None);
    assert_eq!(notices(&spine), Vec::<String>::new(), "the monitor said \"Switched to\" before the switch happened");

    // ---- the command ends: the next turn runs under Work, and the notice is said --------
    pending.store(false,std::sync::atomic::Ordering::SeqCst);
    spine.submit_prompt("It finished", Source::Text).unwrap();
    assert!(prompts.lock().unwrap().contains(&("2".to_string(), "It finished".to_string())));
    assert_eq!(notices(&spine).iter().map(|t| t.starts_with(NOTICE)).collect::<Vec<_>>(), vec![true], "said once, once the turn ran under Work");
    drop(std::fs::remove_file(&path));
    drop(std::fs::remove_dir_all(dir));
}

/// **"Switched to" is drawn while the turn on the new account is still running** (round 16,
/// state `switched-line`: Rich's switch line, then "• 3 agents working on Work", a row that
/// exists only during a turn). The notice is raised inside his turn (`prepare_request`, once
/// the lease on Work is in the chair), so the window's signal to draw it,
/// `rich://proactive-message`, must leave the spine during that turn: after his turn started
/// (never before a turn runs on Work, `f2d3ab1c4`) and before it completed.
///
/// RED on `40de1962f` (main's deferral): the notice's event was held to the turn's boundary
/// and left after `turn-completed`, so the line appeared only once the turn was over.
#[test]
fn the_switch_notice_is_drawn_while_the_turn_on_the_new_account_runs() {
    let (dir, quota) = two_accounts("notice-during-turn", 60.);
    quota.set_policy(richos_core::quota::Policy { enabled: true, pause_percent: 93 }).unwrap();
    quota.set_at_threshold(richos_core::claude_accounts::AtThreshold::Switch).unwrap();
    let (path, ledger) = tmp_ledger("fill-first-notice-during-turn");
    let mut spine = support::spine(ledger);
    spine.create_thread("General", &femcboost()).unwrap();
    let streamed = Arc::new(Mutex::new(Some(streamed_five_hour(93., richos_core::util::now_millis() + 1_000))));
    let prompts = Arc::new(Mutex::new(Vec::new()));
    spine.attach_lease(Box::new(AccountLease { session_id: "sess-account-1".into(), account: "1".into(),
        streamed: streamed.clone(), fail_with: None, prompts: prompts.clone() }));
    spine.set_lease_factory(Box::new(AccountFactory { quota: quota.clone(), spawned: Arc::new(Mutex::new(Vec::new())),
        streamed: Arc::new(Mutex::new(None)), prompts: prompts.clone() }));
    let observer = RecordingObserver::default();
    spine.set_observer(Box::new(observer.clone()));

    spine.submit_prompt("What is on my plate?", Source::Text).unwrap();

    assert!(prompts.lock().unwrap().contains(&("2".to_string(), "What is on my plate?".to_string())), "his turn ran under Work");
    let turns = spine.ledger().turns();
    let his = turns.iter().find(|t| t.user_text == "What is on my plate?").expect("his turn").id.clone();
    let notice = turns.iter().find(|t| t.assistant_text.starts_with("I switched the team to your **Work** account")).expect("the switch notice").id.clone();
    let events = observer.events();
    let at = |wanted: &dyn Fn(&StreamEvent) -> bool| events.iter().position(wanted);
    let started = at(&|e| matches!(e, StreamEvent::TurnStarted { turn_id, .. } if *turn_id == his)).expect("his turn started");
    let completed = at(&|e| matches!(e, StreamEvent::TurnCompleted { turn_id, .. } if *turn_id == his)).expect("his turn completed");
    let drawn = at(&|e| matches!(e, StreamEvent::ProactiveMessage { turn_id, .. } if *turn_id == notice))
        .expect("the switch notice was never sent to the window");
    assert!(started < drawn, "the switch line was sent before a turn ran on Work: {events:?}");
    assert!(drawn < completed, "the switch line was held until his turn ended: {events:?}");
    drop(std::fs::remove_file(&path));
    drop(std::fs::remove_dir_all(dir));
}

/// A reply that runs until the test lets it end.
#[derive(Clone, Default)]
struct Gate(Arc<(Mutex<bool>, std::sync::Condvar)>);
impl Gate {
    fn open(&self) { *self.0.0.lock().unwrap() = true; self.0.1.notify_all(); }
    fn wait(&self) {
        let open = self.0.0.lock().unwrap();
        // load-bound: hang guard only; the test opens every gate itself.
        let (open, _) = self.0.1.wait_timeout_while(open, std::time::Duration::from_secs(30), |open| !*open).unwrap();
        assert!(*open, "the test never let the reply end");
    }
}
/// Every reply a lease has accepted: (account, text), in order.
#[derive(Clone, Default)]
struct Accepted(Arc<AcceptedReplies>);
#[derive(Default)]
struct AcceptedReplies { seen: Mutex<Vec<(String, String)>>, changed: std::sync::Condvar }
impl Accepted {
    fn push(&self, account: &str, text: &str) {
        self.0.seen.lock().unwrap().push((account.to_string(), text.to_string()));
        self.0.changed.notify_all();
    }
    fn wait_for(&self, count: usize) {
        let seen = self.0.seen.lock().unwrap();
        // load-bound: hang guard only; the replies are submitted by the drivers the test started.
        let (seen, _) = self.0.changed.wait_timeout_while(seen, std::time::Duration::from_secs(30), |seen| seen.len() < count).unwrap();
        assert!(seen.len() >= count, "only {} replies started: {:?}", seen.len(), *seen);
    }
    fn all(&self) -> Vec<(String, String)> { self.0.seen.lock().unwrap().clone() }
}
/// A conversation's lease whose reply is answered on another thread, as the app's are
/// (`drive_shared_turns`), and only once its gate opens.
struct HeldReply { session_id: String, account: String, gate: Gate, accepted: Accepted }
impl Cognition for HeldReply {
    fn session_id(&self) -> &str { &self.session_id }
    fn account(&self) -> Option<&str> { Some(&self.account) }
    fn prompt(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        unreachable!("replies are detached, as in the app")
    }
    fn enqueue_prompt(&mut self, text: &str, started: Box<dyn FnOnce() -> Result<(), String> + Send>)
        -> Result<richos_core::cognition::PendingReply, CognitionError> {
        started().map_err(CognitionError::Protocol)?;
        self.accepted.push(&self.account, text);
        let (gate, account) = (self.gate.clone(), self.account.clone());
        Ok(richos_core::cognition::PendingReply::new(move |on_item| {
            gate.wait();
            on_item(TurnItem::Text { seq: 0, text: &format!("answered on account {account}") });
            Ok("end_turn".into())
        }))
    }
}
/// Opens every lease on the account in use (`main.rs`, `spawn_chat`), each with the next gate.
struct HeldReplies { quota: Arc<richos_core::quota::Service>, gates: Mutex<std::collections::VecDeque<Gate>>, accepted: Accepted }
impl LeaseFactory for HeldReplies {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        let account = self.quota.lease_account().id;
        let gate = self.gates.lock().unwrap().pop_front().expect("a gate for every lease");
        Ok(Box::new(HeldReply { session_id: format!("sess-{account}-{}", uuid::Uuid::new_v4()), account,
            gate, accepted: self.accepted.clone() }))
    }
    fn quota(&self) -> Option<Arc<richos_core::quota::Service>> { Some(self.quota.clone()) }
}

/// **Two conversations at once: the switch is complete only when neither is still on the old
/// account** (review rv-20261019T030516Z-0a1a8728-e798, finding 2; the CEO: "that Home account
/// is deffinitely NOT in use"). Conversation A's reply runs on Account 1. The switch to Work is
/// decided at conversation B's turn, whose reply then runs on Work, side by side with A's. The
/// panel shows Rich on both accounts, and nothing says "I switched the team" until A's reply
/// has ended too; then Account 1 has nobody on it and the line is said.
#[test]
fn the_switch_is_complete_only_once_no_conversation_is_still_on_the_old_account() {
    use richos_core::spine::drive_shared_turns;
    const NOTICE: &str = "I switched the team to your **Work** account.";
    let (dir, quota) = two_accounts("two-conversations", 60.);
    quota.set_policy(richos_core::quota::Policy { enabled: true, pause_percent: 93 }).unwrap();
    quota.set_at_threshold(richos_core::claude_accounts::AtThreshold::Switch).unwrap();
    let (path, ledger) = tmp_ledger("two-conversations");
    let mut spine = support::spine(ledger);
    spine.set_turn_control(richos_core::steering::TurnControl::open(path.with_extension("intake.jsonl")).unwrap());
    let (a_ends, b_ends) = (Gate::default(), Gate::default());
    let accepted = Accepted::default();
    spine.set_lease_factory(Box::new(HeldReplies { quota: quota.clone(),
        gates: Mutex::new([a_ends.clone(), b_ends.clone()].into()), accepted: accepted.clone() }));
    spine.detach_conversation_io();
    let a = spine.create_thread("Pricing", &femcboost()).unwrap();
    let b = spine.create_thread("Hiring", &femcboost()).unwrap();
    // B already has a lease on Account 1, whose own reading says 93% of the five hours.
    spine.switch_thread(&b).unwrap();
    spine.attach_lease(Box::new(AccountLease { session_id: "sess-b-account-1".into(), account: "1".into(),
        streamed: Arc::new(Mutex::new(Some(streamed_five_hour(93., richos_core::util::now_millis() + 1_000)))),
        fail_with: None, prompts: Arc::new(Mutex::new(Vec::new())) }));
    let shared = Arc::new(Mutex::new(spine));
    let working = |id: &str| quota.view().accounts.iter().find(|x| x.id == id).map(|x| x.working.clone()).unwrap();
    let said = |shared: &Mutex<richos_core::spine::Spine>| shared.lock().unwrap().ledger().turns().iter()
        .filter(|t| t.assistant_text.starts_with(NOTICE)).count();

    // ---- A's reply runs on Account 1 ------------------------------------------------------
    shared.lock().unwrap().submit_prompt_to(&a, "Draft the pricing note", Source::Text).unwrap();
    let a_driver = { let shared = shared.clone(); std::thread::spawn(move || drive_shared_turns(&shared)) };
    accepted.wait_for(1);
    assert_eq!(accepted.all()[0], ("1".to_string(), "Draft the pricing note".to_string()));

    // ---- the switch is decided at B's turn, and B's reply runs on Work while A's runs on ----
    shared.lock().unwrap().submit_prompt_to(&b, "Who did we shortlist?", Source::Text).unwrap();
    let b_driver = { let shared = shared.clone(); std::thread::spawn(move || drive_shared_turns(&shared)) };
    accepted.wait_for(2);
    assert_eq!(accepted.all()[1], ("2".to_string(), "Who did we shortlist?".to_string()), "B's reply runs on Work");
    assert_eq!(quota.accounts.in_use().id, "2", "the switch to Work is decided");
    assert_eq!(working("1"), vec!["rich".to_string()], "A's reply still runs on Account 1");
    assert_eq!(working("2"), vec!["rich".to_string()], "B's reply runs on Work");
    assert!(quota.view().last_switch.is_none(), "the switch is not complete while A runs on Account 1");

    // ---- B's reply ends first: A is still on Account 1 -------------------------------------
    b_ends.open();
    b_driver.join().unwrap().unwrap();
    assert_eq!(working("1"), vec!["rich".to_string()], "A's reply still runs on Account 1 after B's ended");
    assert_eq!(said(&shared), 0, "\"I switched the team\" was said while A's reply ran on Account 1");
    assert!(quota.view().last_switch.is_none());

    // ---- A's reply ends: nobody is on Account 1, and the switch is complete -----------------
    a_ends.open();
    a_driver.join().unwrap().unwrap();
    assert_eq!(working("1"), Vec::<String>::new(), "nobody is left on Account 1");
    shared.lock().unwrap().raise_quota_notices(None);
    assert_eq!(said(&shared), 1, "the switch is said once it is complete");
    assert_eq!(quota.view().last_switch.map(|s| s.to), Some("2".to_string()));
    drop(shared);
    drop(std::fs::remove_file(&path));
    drop(std::fs::remove_file(path.with_extension("intake.jsonl")));
    drop(std::fs::remove_dir_all(dir));
}

/// **The backstop, only for one turn that by itself used up what remained.** The lease on
/// Account 1 is refused mid-turn with the error `native.rs` writes for any of the three real
/// signals. The account is marked gone, Work is put in use, and his prompt is re-served on a
/// fresh lease under Work: one clean exchange, the failed turn superseded and kept on disk.
#[test]
fn a_turn_cut_by_a_usage_limit_is_re_served_under_the_next_account() {
    let (dir, quota) = two_accounts("backstop", 40.);
    let (path, ledger) = tmp_ledger("fill-first-backstop");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let spawned = Arc::new(Mutex::new(Vec::new()));
    let refusal = richos_core::claude_accounts::usage_limit_error(Some(4_070_908_800_000), "[\"You've hit your session limit\"]");
    spine.attach_lease(Box::new(AccountLease { session_id: "sess-account-1".into(), account: "1".into(),
        streamed: Arc::new(Mutex::new(None)), fail_with: Some(refusal), prompts: prompts.clone() }));
    spine.set_lease_factory(Box::new(AccountFactory { quota: quota.clone(), spawned: spawned.clone(),
        streamed: Arc::new(Mutex::new(None)), prompts: prompts.clone() }));

    spine.submit_prompt("Draft the board update", Source::Text).unwrap();

    assert_eq!(quota.lease_account().id, "2");
    assert_eq!(*spawned.lock().unwrap(), vec!["2".to_string()]);
    let msgs = spine.messages(&thread).unwrap();
    let exchange: Vec<_> = msgs.iter().filter(|m| !m.text.starts_with("I switched the team to")).map(|m| m.text.as_str()).collect();
    assert_eq!(exchange, vec!["Draft the board update", "answered on account 2"], "one clean exchange");
    assert!(msgs.iter().any(|m| m.text == "I switched the team to your **Work** account. Account 1 reached a Claude usage limit, so the step it turned away runs again on Work."));
    let failed = spine.ledger().turns().iter().find(|t| t.state == TurnState::Interrupted).cloned().expect("kept on disk");
    assert!(failed.superseded_by.is_some());
    drop(std::fs::remove_file(&path));
    drop(std::fs::remove_dir_all(dir));
}

/// Fast and normal readings change meter cadence without posting into the conversation.
#[test]
fn fast_and_normal_usage_checks_do_not_interrupt_the_conversation() {
    let (dir, quota) = two_accounts("alert", 50.);
    let (path, ledger) = tmp_ledger("fill-first-alert");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    let start = richos_core::util::now_millis() + 1_000;
    let streamed = Arc::new(Mutex::new(Some(streamed_five_hour(50., start))));
    let prompts = Arc::new(Mutex::new(Vec::new()));
    spine.attach_lease(Box::new(AccountLease { session_id: "sess-account-1".into(), account: "1".into(),
        streamed: streamed.clone(), fail_with: None, prompts: prompts.clone() }));
    spine.set_lease_factory(Box::new(AccountFactory { quota: quota.clone(), spawned: Arc::new(Mutex::new(Vec::new())),
        streamed: Arc::new(Mutex::new(None)), prompts }));
    let alerts = |spine: &richos_core::spine::Spine| spine.messages(&thread).unwrap().iter()
        .filter(|m| m.text.starts_with("Usage is climbing fast")).map(|m| m.text.clone()).collect::<Vec<_>>();

    spine.submit_prompt("one", Source::Text).unwrap();
    assert!(alerts(&spine).is_empty(), "one reading is no speed");
    *streamed.lock().unwrap() = Some(streamed_five_hour(54., start + 60_000));
    spine.submit_prompt("two", Source::Text).unwrap();
    assert!(alerts(&spine).is_empty(), "fast usage stays out of the conversation");
    assert_eq!(quota.view().refresh_interval_ms, richos_core::quota::FAST_REFRESH_INTERVAL_MS);
    *streamed.lock().unwrap() = Some(streamed_five_hour(58., start + 120_000));
    spine.submit_prompt("three", Source::Text).unwrap();
    assert!(alerts(&spine).is_empty());
    *streamed.lock().unwrap() = Some(streamed_five_hour(58.1, start + 180_000));
    spine.submit_prompt("four", Source::Text).unwrap();
    assert_eq!(quota.view().refresh_interval_ms, richos_core::quota::REFRESH_INTERVAL_MS);
    assert!(spine.messages(&thread).unwrap().iter().all(|m|
        !m.text.starts_with("Usage is ")), "normal readings stay out of the conversation too");
    drop(std::fs::remove_file(&path));
    drop(std::fs::remove_dir_all(dir));
}

// ============================================================================
// §119 — the model he chose in Preferences is the model of the next reply
// ============================================================================

/// A lease that knows the model it was started on, as `NativeCognition::lease_model` does,
/// and says it in every answer so a test reads which model answered rather than assumes it.
struct ModelLease {
    session_id: String,
    model: Arc<Mutex<richos_core::config::ModelChoice>>,
    prompts: Arc<Mutex<Vec<(String, String)>>>,
    commands: Arc<Mutex<Option<richos_core::lease_commands::CommandReading>>>,
}
struct ModelControl(Arc<Mutex<richos_core::config::ModelChoice>>);
impl richos_core::steering::TurnCancel for ModelControl {
    fn cancel(&self)->bool {panic!("model change canceled a live session")}
    fn set_model(&self,model:richos_core::config::ModelChoice)->Result<bool,String> {*self.0.lock().unwrap()=model;Ok(true)}
}
impl Cognition for ModelLease {
    fn session_id(&self) -> &str { &self.session_id }
    fn lease_model(&self)->Option<richos_core::config::ModelChoice> {Some(*self.model.lock().unwrap())}
    fn cancel_handle(&self)->Option<Arc<dyn richos_core::steering::TurnCancel>> {Some(Arc::new(ModelControl(self.model.clone())))}
    fn running_commands(&self) -> Option<richos_core::lease_commands::CommandReading> { *self.commands.lock().unwrap() }
    fn prompt(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        self.prompts.lock().unwrap().push((self.model.lock().unwrap().as_str().to_string(), text.to_string()));
        on_item(TurnItem::Text { seq: 0, text: &format!("answered on {}", self.model.lock().unwrap().as_str()) });
        Ok("end_turn".into())
    }
}

/// The desktop's factory reduced to §119: every lease starts on the model the stored setting
/// names at that moment (`EngineProfile::prepare` reads `config.json` at every spawn), and
/// `lease_models` answers that same setting.
struct ModelFactory {
    stored: Arc<Mutex<richos_core::config::ModelChoice>>,
    spawned: Arc<Mutex<Vec<String>>>,
    prompts: Arc<Mutex<Vec<(String, String)>>>,
    commands: Arc<Mutex<Option<richos_core::lease_commands::CommandReading>>>,
}
impl LeaseFactory for ModelFactory {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        let model = *self.stored.lock().unwrap();
        let mut spawned = self.spawned.lock().unwrap();
        spawned.push(model.as_str().to_string());
        Ok(Box::new(ModelLease {
            session_id: format!("sess-{}-{}", model.as_str(), spawned.len()),
            model:Arc::new(Mutex::new(model)),
            prompts: self.prompts.clone(),
            commands: self.commands.clone(),
        }))
    }
    fn lease_models(&self) -> Option<richos_core::config::LeaseModels> {
        Some(richos_core::config::LeaseModels { rich: *self.stored.lock().unwrap(), stu: richos_core::config::ModelChoice::Opus })
    }
}

/// **§119 and the CEO's addendum:** *"Closing the preferences panel IS the confirmation that the
/// settings chosen there now need to be saved and implemented."* Saving is the shell's
/// `set_lease_models`; this is the implementing half. With the stored choice unchanged no turn
/// renews the lease; once it names Fable, his next reply comes from a Fable lease, renewed at
/// the boundary before his words are sent and never inside a turn; back to Opus, the same.
#[test]
fn the_model_he_chose_in_preferences_answers_his_next_reply() {
    use richos_core::config::ModelChoice;
    use richos_core::lease_commands::CommandReading;
    let (path, ledger) = tmp_ledger("model-changed");
    let mut spine = support::spine(ledger);
    let thread = spine.create_thread("General", &femcboost()).unwrap();
    let stored = Arc::new(Mutex::new(ModelChoice::Opus));
    let spawned = Arc::new(Mutex::new(Vec::new()));
    let prompts = Arc::new(Mutex::new(Vec::new()));
    let commands = Arc::new(Mutex::new(Some(CommandReading::Clear)));
    spine.attach_lease(Box::new(ModelLease {
        session_id: "sess-opus-0".into(), model: Arc::new(Mutex::new(ModelChoice::Opus)), prompts: prompts.clone(), commands: commands.clone(),
    }));
    spine.set_lease_factory(Box::new(ModelFactory {
        stored: stored.clone(), spawned: spawned.clone(), prompts: prompts.clone(), commands: commands.clone(),
    }));
    let answered_by = |text: &str| -> Vec<String> {
        prompts.lock().unwrap().iter().filter(|(_, t)| t == text).map(|(m, _)| m.clone()).collect()
    };

    spine.submit_prompt("one", Source::Text).unwrap();
    assert_eq!(spine.rotation_count(), 0, "the lease was renewed with the stored model unchanged");
    assert_eq!(answered_by("one"), vec!["opus"]);

    // Preferences closed with "talks with you" on Fable: the next reply is Fable's.
    *stored.lock().unwrap() = ModelChoice::Fable;
    spine.turn_control().set_model(ModelChoice::Fable).unwrap();
    spine.submit_prompt("two", Source::Text).unwrap();
    assert_eq!(answered_by("two"), vec!["fable"], "his next reply did not come from the model he chose");
    assert_eq!(spine.rotation_count(), 0);
    assert_eq!(spine.last_rotation_reason(), None);
    assert!(spawned.lock().unwrap().is_empty(),"changing model created a new session");
    spine.submit_prompt("three", Source::Text).unwrap();
    assert_eq!(spine.rotation_count(), 0, "a lease already on Fable was renewed again");

    // Back to Opus while a command the lease started is running: that turn stays on Fable,
    // and the switch happens at the first boundary after the command ends.
    *stored.lock().unwrap() = ModelChoice::Opus;
    spine.turn_control().set_model(ModelChoice::Opus).unwrap();
    *commands.lock().unwrap() = Some(CommandReading::Running(1));
    spine.submit_prompt("four", Source::Text).unwrap();
    assert_eq!(answered_by("four"), vec!["opus"], "a renewal retired a lease with a command running");
    *commands.lock().unwrap() = Some(CommandReading::Clear);
    spine.submit_prompt("five", Source::Text).unwrap();
    assert_eq!(answered_by("five"), vec!["opus"]);
    assert_eq!(spine.rotation_count(), 0);

    // One unbroken conversation across both renewals.
    let mine: Vec<_> = spine.messages(&thread).unwrap().into_iter().filter(|m| m.text.starts_with("answered on ")).collect();
    assert_eq!(mine.len(), 5, "{mine:?}");
    drop(std::fs::remove_file(&path));
}

//! **A CONVERSATION'S RICH IS ENDED WHEN IT HAS SAT IDLE PAST THE TIMEOUT, AND COMES BACK WHEN
//! HE SPEAKS** — richos-hq `docs/plans/follow-ups.md`, "a nightly's VM boot proof cannot get a
//! guest slot", decision 3: *"Retire idle Claude sessions of other conversations. … Retire a
//! session after an idle timeout and revive it on demand."*
//!
//! Before `Spine::retire_idle_front_desks` the only things that ended a parked desk were the
//! count cap (`MAX_RESIDENT_FRONT_DESKS`) and `run_setup`'s `retire_idle_lease`, which ends the
//! chair's desk alone. Nothing ended a desk because of TIME, so every conversation he had
//! spoken in kept its `claude` child until the app quit.
//!
//! The clock is an argument (`now_ms`), so these tests move time by passing a later instant
//! rather than by sleeping.
use richos_core::{
    cognition::{BackgroundCommand, Cognition, CognitionError, LeaseFactory, TurnItem},
    entity::{EntityId, ThreadBinding},
    ledger::{Ledger, Source},
    spine::Spine,
    steering::{ActiveTurn, ConversationControl, TurnControl},
    util::now_millis,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
mod support;

const THIRTY_MINUTES: Duration = Duration::from_secs(30 * 60);

fn femcboost() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}

/// What every desk the factory ever made did, so a test can ask about one since dropped.
#[derive(Clone, Default)]
struct Log {
    spawned: Arc<Mutex<Vec<String>>>,
    dropped: Arc<Mutex<Vec<String>>>,
    /// The provider's saved history, by session: what `claude --resume` reads back. A desk
    /// spawned for a session that already has one starts from it, as the native opener does.
    history: Arc<Mutex<HashMap<String, Vec<String>>>>,
    /// Per-session switches a test flips to make a desk busy.
    pending_input: Arc<Mutex<Vec<String>>>,
    running_command: Arc<Mutex<Vec<String>>>,
}

impl Log {
    fn spawned(&self) -> Vec<String> { self.spawned.lock().unwrap().clone() }
    fn dropped(&self) -> Vec<String> { self.dropped.lock().unwrap().clone() }
}

/// A front desk whose session is THE THREAD'S, the property the native conversation lease
/// has when continuity is configured: `native_resume` reads the thread's saved session and the
/// child is started with `--resume` on it. `requires_thread_isolation` is what puts it on the
/// resident path at all (see `resident_front_desk_tests.rs`).
struct Desk {
    session: String,
    log: Log,
}

impl Cognition for Desk {
    fn session_id(&self) -> &str { &self.session }
    fn requires_thread_isolation(&self) -> bool { true }
    fn has_pending_input(&self) -> bool { self.log.pending_input.lock().unwrap().contains(&self.session) }
    fn background_commands(&self) -> Option<Vec<BackgroundCommand>> {
        let running = self.log.running_command.lock().unwrap().contains(&self.session);
        Some(if running {
            vec![BackgroundCommand { task_id: "b1".into(), description: "npm test".into(), ended: None }]
        } else { Vec::new() })
    }
    fn prompt(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        let mut history = self.log.history.lock().unwrap();
        let earlier = history.entry(self.session.clone()).or_default();
        let reply = format!("{} remembers [{}] and hears: {text}", self.session, earlier.join(" | "));
        earlier.push(text.to_string());
        on_item(TurnItem::Text { seq: 0, text: &reply });
        Ok("end_turn".to_string())
    }
}

impl Drop for Desk {
    fn drop(&mut self) { self.log.dropped.lock().unwrap().push(self.session.clone()); }
}

struct Desks { log: Log }

impl LeaseFactory for Desks {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        unreachable!("a conversation's desk is always spawned for its thread")
    }
    fn spawn_scoped(&self, binding: &ThreadBinding, _: &ConversationControl) -> Result<Box<dyn Cognition>, CognitionError> {
        let session = format!("session-of-{}", binding.thread_id());
        self.log.spawned.lock().unwrap().push(session.clone());
        Ok(Box::new(Desk { session, log: self.log.clone() }))
    }
}

struct Fixture {
    spine: Spine,
    control: TurnControl,
    log: Log,
    root: std::path::PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.root) {
            eprintln!("fixture {} was not removed: {error}", self.root.display());
        }
    }
}

fn fixture() -> Fixture {
    let root = std::env::temp_dir().join(format!("richos-idle-retire-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let mut spine = support::spine(Ledger::open(root.join("ledger.jsonl")).unwrap());
    let control = TurnControl::open(root.join("intake.jsonl")).unwrap();
    spine.set_turn_control(control.clone());
    let log = Log::default();
    spine.set_lease_factory(Box::new(Desks { log: log.clone() }));
    Fixture { spine, control, log, root }
}

/// Two conversations, both spoken in; he is now looking at the second.
fn two_spoken_conversations(f: &mut Fixture) -> (String, String) {
    let one = f.spine.create_thread("Pricing", &femcboost()).unwrap();
    let two = f.spine.create_thread("Hiring", &femcboost()).unwrap();
    f.spine.submit_prompt_to(&one, "the pricing review is due Friday", Source::Text).unwrap();
    f.spine.submit_prompt_to(&two, "who did we shortlist", Source::Text).unwrap();
    assert_eq!(f.spine.active_thread(), Some(two.as_str()), "the second conversation is the one on screen");
    assert!(f.spine.front_desk_is_resident(&one) && f.spine.front_desk_is_resident(&two));
    (one, two)
}

fn later(by: Duration) -> u64 { now_millis() + by.as_millis() as u64 }

/// **THE HEADLINE.** Past the timeout, the parked desk is ended — its child is dropped — and the
/// desk on screen is not. Fails on main: there is no timed retirement there at all.
#[test]
fn an_idle_conversation_past_the_timeout_has_its_session_retired() {
    let mut f = fixture();
    let (one, two) = two_spoken_conversations(&mut f);

    // Not yet: a minute short of the timeout retires nothing.
    let early = f.spine.retire_idle_front_desks(later(THIRTY_MINUTES - Duration::from_secs(60)), THIRTY_MINUTES, &|_| false);
    assert!(early.is_empty(), "a desk was retired before its timeout: {early:?}");
    assert!(f.log.dropped().is_empty());

    let retired = f.spine.retire_idle_front_desks(later(THIRTY_MINUTES), THIRTY_MINUTES, &|_| false);
    assert_eq!(retired, vec![one.clone()], "the idle conversation was not the one retired");
    assert_eq!(f.log.dropped(), vec![format!("session-of-{one}")], "its claude child was not ended");
    assert!(!f.spine.front_desk_is_resident(&one));
    assert_eq!(f.control.for_thread(&one).lease_session(), None, "the retired session is still advertised");
    // The conversation on screen is never retired, however long it has been idle.
    assert!(f.spine.front_desk_is_resident(&two), "the conversation he is looking at was retired");
    let much_later = f.spine.retire_idle_front_desks(later(THIRTY_MINUTES * 10), THIRTY_MINUTES, &|_| false);
    assert!(much_later.is_empty(), "the chair's desk was retired: {much_later:?}");
}

/// Zero is the off switch: nothing is ever retired, as before this change.
#[test]
fn a_zero_timeout_retires_nothing() {
    let mut f = fixture();
    two_spoken_conversations(&mut f);
    assert!(f.spine.retire_idle_front_desks(later(THIRTY_MINUTES * 100), Duration::ZERO, &|_| false).is_empty());
    assert!(f.log.dropped().is_empty());
}

/// **A BUSY SESSION IS NOT RETIRED**, whatever it is busy with, and becomes retirable the moment
/// it is not. One conversation per hold, each with a positive control at the end, so the test
/// cannot pass because retirement had stopped working.
#[test]
fn a_busy_conversation_is_never_retired_by_the_idle_timer() {
    let mut f = fixture();
    let names = ["running turn", "pending input", "background command", "unanswered question", "queued message"];
    let threads: Vec<String> = names.iter().map(|n| f.spine.create_thread(n, &femcboost()).unwrap()).collect();
    for thread in &threads {
        f.spine.submit_prompt_to(thread, "hello", Source::Text).unwrap();
    }
    // He is looking at none of them.
    let chair = f.spine.create_thread("On screen", &femcboost()).unwrap();
    f.spine.switch_thread(&chair).unwrap();

    let session = |thread: &str| format!("session-of-{thread}");
    // 1. A turn is running (the shared control's mirror, as a delivery publishes it).
    f.control.for_thread(&threads[0]).begin_turn(ActiveTurn {
        turn_id: "turn-running".into(), thread_id: threads[0].clone(), entity_id: Some(femcboost()), started_at: None });
    // 2. Input is awaiting admission on its connection.
    f.log.pending_input.lock().unwrap().push(session(&threads[1]));
    // 3. A command it started in the background has not ended.
    f.log.running_command.lock().unwrap().push(session(&threads[2]));
    // 4. A question it asked him is unanswered (the shell's fact, through `held`).
    let unanswered = Mutex::new(Some(threads[3].clone()));
    // 5. A message for it is waiting in the durable intake.
    f.control.submit_from_channel(&threads[4], Some(femcboost()), "are you there", "phone").unwrap();

    let held = |binding: &ThreadBinding| unanswered.lock().unwrap().as_deref() == Some(binding.thread_id());
    let retired = f.spine.retire_idle_front_desks(later(THIRTY_MINUTES * 4), THIRTY_MINUTES, &held);
    assert!(retired.is_empty(), "a busy conversation was retired: {retired:?}");
    assert!(f.log.dropped().is_empty());
    for (thread, name) in threads.iter().zip(names) {
        assert!(f.spine.front_desk_is_resident(thread), "the conversation with a {name} lost its session");
    }

    // Positive control: each hold released, each desk goes — so the holds were what kept them.
    f.control.end_turn("turn-running");
    f.log.pending_input.lock().unwrap().clear();
    f.log.running_command.lock().unwrap().clear();
    *unanswered.lock().unwrap() = None;
    let mut retired = f.spine.retire_idle_front_desks(later(THIRTY_MINUTES * 4), THIRTY_MINUTES, &held);
    retired.sort();
    let mut expected = threads[..4].to_vec();
    expected.sort();
    assert_eq!(retired, expected, "only the queued message should still hold its conversation");
    assert!(f.spine.front_desk_is_resident(&threads[4]), "pending intake did not hold its conversation");
}

/// **A RETIRED CONVERSATION COMES BACK WHEN HE SPEAKS, AND IT REMEMBERS.** The next turn spawns
/// a desk scoped to the conversation's own binding — the call the native opener answers with
/// `--resume` on the thread's saved session — and the reply is made with what was said before.
#[test]
fn a_retired_conversation_revives_on_his_next_message_with_its_earlier_context() {
    let mut f = fixture();
    let (one, _two) = two_spoken_conversations(&mut f);
    assert_eq!(f.spine.retire_idle_front_desks(later(THIRTY_MINUTES), THIRTY_MINUTES, &|_| false), vec![one.clone()]);
    let spawns_before = f.log.spawned().len();

    f.spine.submit_prompt_to(&one, "when is it due again", Source::Text).unwrap();

    let spawned = f.log.spawned();
    assert_eq!(spawned.len(), spawns_before + 1, "speaking to a retired conversation did not revive it");
    assert_eq!(spawned.last().unwrap(), &format!("session-of-{one}"), "it was revived on another session");
    assert!(f.spine.front_desk_is_resident(&one));
    let reply = f.spine.messages(&one).unwrap().last().unwrap().text.clone();
    assert!(reply.contains("the pricing review is due Friday") && reply.contains("when is it due again"),
        "the revived conversation answered without its earlier context: {reply}");
}

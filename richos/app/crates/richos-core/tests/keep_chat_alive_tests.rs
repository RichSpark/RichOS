//! **KEEP CHAT ALIVE** (CEO, 2026-11-16, round 83): *"this checkbox would be in every chat but
//! off by default and in long-running threads where the user knows that he'll be back the next
//! morning, he could just check that checkbox to keep the cache for that particular thread. And
//! the cache heartbeat only needs to happen when Rich or Stu really have been idle for 55 minutes
//! i.e. the cache heartbeat needs to be smart, not dumb."*
//!
//! WHEN a keep-alive is due is `chat_cache::heartbeat_due`, proven beside it. These tests prove
//! what sending one to Rich's session does to the conversation: nothing he could see. It is one
//! message on the chat's own session, it is no turn in the ledger and no line in the chat, a
//! chat that is not idle gets none, and a chat whose session has ended is resumed for it.
use richos_core::{
    cognition::{Cognition, CognitionError, LeaseFactory, PendingReply, TurnItem},
    entity::{EntityId, ThreadBinding},
    ledger::{Ledger, Source},
    spine::{KeptAlive, Spine},
    steering::{ActiveTurn, ConversationControl, TurnControl},
    util::now_millis,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;
mod support;

fn femcboost() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}

#[derive(Clone, Default)]
struct Log {
    spawned: Arc<Mutex<Vec<String>>>,
    prompts: Arc<Mutex<Vec<(String, String)>>>,
    kept_alive: Arc<Mutex<Vec<String>>>,
}

/// A front desk on the thread's own session, as the native conversation lease is.
struct Desk {
    session: String,
    log: Log,
}

impl Cognition for Desk {
    fn session_id(&self) -> &str { &self.session }
    fn requires_thread_isolation(&self) -> bool { true }
    fn prompt(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        self.log.prompts.lock().unwrap().push((self.session.clone(), text.to_string()));
        on_item(TurnItem::Text { seq: 0, text: "Noted." });
        Ok("end_turn".to_string())
    }
    fn keep_chat_alive(&mut self) -> Result<PendingReply, CognitionError> {
        self.log.kept_alive.lock().unwrap().push(self.session.clone());
        // The model's answer to the keep-alive: text the app must never show.
        Ok(PendingReply::new(|on_item| {
            on_item(TurnItem::Text { seq: 0, text: "ok" });
            on_item(TurnItem::AnswerComplete);
            Ok("end_turn".to_string())
        }))
    }
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
    let root = std::env::temp_dir().join(format!("richos-keep-alive-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let mut spine = support::spine(Ledger::open(root.join("ledger.jsonl")).unwrap());
    let control = TurnControl::open(root.join("intake.jsonl")).unwrap();
    spine.set_turn_control(control.clone());
    let log = Log::default();
    spine.set_lease_factory(Box::new(Desks { log: log.clone() }));
    Fixture { spine, control, log, root }
}

/// A kept chat that has been spoken in, and another he is now looking at.
fn a_kept_chat_and_the_one_on_screen(f: &mut Fixture) -> (String, String) {
    let kept = f.spine.create_thread("Board update", &femcboost()).unwrap();
    let other = f.spine.create_thread("Hiring", &femcboost()).unwrap();
    f.spine.submit_prompt_to(&kept, "draft the board update", Source::Text).unwrap();
    f.spine.submit_prompt_to(&other, "who did we shortlist", Source::Text).unwrap();
    (kept, other)
}

/// **THE HEADLINE.** One keep-alive goes to the kept chat's own session and to no other, and
/// the conversation is exactly what it was: no turn, no message, nothing said.
#[test]
fn a_keep_alive_goes_to_the_chats_own_session_and_is_never_a_turn_or_a_line_in_the_chat() {
    let mut f = fixture();
    let (kept, other) = a_kept_chat_and_the_one_on_screen(&mut f);
    let turns = f.spine.ledger().turns().len();
    let said = f.spine.messages(&kept).unwrap().len();
    let prompts = f.log.prompts.lock().unwrap().len();

    let verdict = f.spine.keep_front_desk_alive(&kept);
    assert!(matches!(verdict, KeptAlive::Sent { spawned: false, .. }), "the live session was not used: {verdict:?}");
    assert_eq!(*f.log.kept_alive.lock().unwrap(), vec![format!("session-of-{kept}")]);
    assert_eq!(f.spine.ledger().turns().len(), turns, "the keep-alive became a turn");
    assert_eq!(f.spine.messages(&kept).unwrap().len(), said, "the keep-alive or its answer is in the chat");
    assert_eq!(f.log.prompts.lock().unwrap().len(), prompts, "the keep-alive was sent as a prompt of the user's");
    assert_eq!(f.spine.active_thread(), Some(other.as_str()), "the chat on screen changed");
    assert!(f.control.for_thread(&kept).front_desk_priming().is_none(), "the chat was left claimed");

    // And the chat carries on as before: his next message is a turn on the same session.
    f.spine.submit_prompt_to(&kept, "and the appendix", Source::Text).unwrap();
    assert_eq!(f.log.prompts.lock().unwrap().last().unwrap(), &(format!("session-of-{kept}"), "and the appendix".to_string()));
}

/// A chat that is not idle gets none: its turn is the request that renews the cache.
#[test]
fn a_chat_with_a_turn_running_gets_no_keep_alive() {
    let mut f = fixture();
    let (kept, _) = a_kept_chat_and_the_one_on_screen(&mut f);
    let entity = f.spine.ledger().thread_binding(&kept).unwrap().entity_id().clone();
    f.control.for_thread(&kept).begin_turn(ActiveTurn { turn_id: "turn-running".into(), thread_id: kept.clone(), entity_id: Some(entity), started_at: None });
    assert_eq!(f.spine.keep_front_desk_alive(&kept), KeptAlive::Busy);
    assert!(f.log.kept_alive.lock().unwrap().is_empty(), "a keep-alive was sent into a running turn");
}

/// The chat's session had ended (retired while parked, or the app was relaunched): it is
/// resumed for the keep-alive, on the thread's own session, the way his next message would.
#[test]
fn a_chat_whose_session_has_ended_is_resumed_for_its_keep_alive() {
    let mut f = fixture();
    let (kept, _) = a_kept_chat_and_the_one_on_screen(&mut f);
    let half_an_hour = Duration::from_secs(30 * 60);
    let retired = f.spine.retire_idle_front_desks(now_millis() + half_an_hour.as_millis() as u64, half_an_hour, &|_| false);
    assert_eq!(retired, vec![kept.clone()]);
    assert!(!f.spine.front_desk_is_resident(&kept));

    let verdict = f.spine.keep_front_desk_alive(&kept);
    assert!(matches!(verdict, KeptAlive::Sent { spawned: true, .. }), "the ended session was not resumed: {verdict:?}");
    let session = format!("session-of-{kept}");
    assert_eq!(f.log.spawned.lock().unwrap().iter().filter(|s| **s == session).count(), 2, "one start for his message, one for the keep-alive");
    assert_eq!(*f.log.kept_alive.lock().unwrap(), vec![session]);
    assert!(f.spine.front_desk_is_resident(&kept));
}

/// A chat that does not exist is refused, never created.
#[test]
fn a_chat_that_does_not_exist_gets_no_keep_alive() {
    let mut f = fixture();
    let before = f.spine.threads().len();
    assert!(matches!(f.spine.keep_front_desk_alive("thr_nothing_here"), KeptAlive::NotSent(_)));
    assert_eq!(f.spine.threads().len(), before);
}

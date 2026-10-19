//! **Haiku names a new conversation from its first message** (CEO, 2026-10-19: "As soon as the
//! user submits their first message in a new chat, Haiku needs to come up with the most fitting
//! title for that conversation and automatically change "New chat" to that title.").
//!
//! The spine's half: which message is asked about, that the answer reaches the shell's sink,
//! and that his reply never waits for it. The provider launch itself is tested against a stub
//! `claude` in `thread_title.rs`; the sidebar's half in the shell's `main.rs` and `nav.rs`.
use richos_core::{cognition::{MockCognition, TitleWriter}, entity::EntityId, ledger::{Ledger, Source}, spine::Spine};
use std::sync::{atomic::{AtomicUsize, Ordering}, mpsc, Arc, Mutex};
use std::time::Duration;
mod support;

/// The bound only stops a broken fixture hanging the suite; nothing here is a speed claim.
const GUARD: Duration = Duration::from_secs(60);

struct Fixture {
    spine: Spine,
    thread: String,
    titles: mpsc::Receiver<(String, String)>,
    asked: Arc<Mutex<Vec<String>>>,
    /// How often the spine asked the lease for its titler: counted as it asks, so "no second
    /// title" is a fact read at once rather than a wait that happened to stay quiet.
    requests: Arc<AtomicUsize>,
    root: std::path::PathBuf,
}

fn fixture(tag: &str, answer: impl Fn(&str) -> Result<String, String> + Send + Sync + 'static) -> Fixture {
    let root = std::env::temp_dir().join(format!("richos-title-{tag}-{}", uuid::Uuid::new_v4()));
    let mut ledger = Ledger::open(root.join("ledger.jsonl")).unwrap();
    let thread = ledger.create_thread("Can you work out what we should charge…", &EntityId::parse("femcboost").unwrap()).unwrap();
    let mut spine = support::spine(ledger);
    spine.switch_thread(&thread).unwrap();
    let (send, titles) = mpsc::channel();
    let send = Mutex::new(send);
    spine.set_title_observer(Arc::new(move |thread: &str, title: &str| {
        send.lock().unwrap().send((thread.to_string(), title.to_string())).unwrap();
    }));
    let asked = Arc::new(Mutex::new(Vec::new()));
    let seen = asked.clone();
    let writer: TitleWriter = Arc::new(move |first: &str| {
        seen.lock().unwrap().push(first.to_string());
        answer(first)
    });
    let mut mock = MockCognition::new("titled-native", vec!["On it!", "Beta is the same."]);
    mock.title_writer = Some(writer);
    let requests = mock.title_requests.clone();
    spine.attach_lease(Box::new(mock));
    Fixture { spine, thread, titles, asked, requests, root }
}

/// Item 1: his first message is what Haiku is asked about, whole (not the provisional title cut
/// at 48 characters), and the answer reaches the shell for that conversation.
#[test]
fn his_first_message_is_titled_and_the_title_reaches_the_shell() {
    let mut f = fixture("first", |_| Ok("Acme renewal pricing".into()));
    let first = "Can you work out what we should charge Acme at renewal?\nThey asked twice this week.";
    f.spine.submit_prompt(first, Source::Text).unwrap();
    let (thread, title) = f.titles.recv_timeout(GUARD).expect("Haiku's title reached the shell");
    assert_eq!((thread.as_str(), title.as_str()), (f.thread.as_str(), "Acme renewal pricing"));
    assert_eq!(*f.asked.lock().unwrap(), vec![first.to_string()]);
    assert_eq!(f.requests.load(Ordering::SeqCst), 1);

    // His second message is not a first message: no title is started for it.
    f.spine.submit_prompt("And what about Beta?", Source::Text).unwrap();
    assert_eq!(f.requests.load(Ordering::SeqCst), 1, "a later message started a second title");
    drop(f.spine);
    std::fs::remove_dir_all(f.root).unwrap();
}

/// Item 1, the second review's case (rv-20261019T125755Z-2ce0daa8-2bf1): his first message and
/// a quick follow-up both queue while the conversation is starting, and are delivered together
/// on the shell's own road (durable intake, `poll_intake`, then `drive_shared_turns`). Both are
/// in the ledger when the first is delivered; the first is still titled, once, from its own words.
#[test]
fn a_follow_up_queued_with_the_first_message_does_not_hide_it() {
    let f = fixture("batched", |_| Ok("Board update".into()));
    let Fixture { mut spine, thread, titles, asked, requests, root } = f;
    spine.detach_conversation_io();
    let control = richos_core::steering::TurnControl::open(root.join("intake.jsonl")).unwrap();
    spine.set_turn_control(control.clone());
    let entity = EntityId::parse("femcboost").unwrap();
    for words in ["Draft the board update", "Keep it to one page"] {
        control.submit_from_channel(&thread, Some(entity.clone()), words, "phone").unwrap();
    }
    spine.poll_intake().unwrap();
    let shared = Mutex::new(spine);
    richos_core::spine::drive_shared_turns(&shared).unwrap();
    let replies = shared.lock().unwrap().messages(&thread).unwrap().iter().filter(|m| m.role == "assistant").count();
    assert_eq!(replies, 2, "both messages were answered");
    assert_eq!(requests.load(Ordering::SeqCst), 1, "exactly one title, for the first message");
    let (titled, title) = titles.recv_timeout(GUARD).expect("the first message was titled");
    assert_eq!((titled.as_str(), title.as_str()), (thread.as_str(), "Board update"));
    assert_eq!(*asked.lock().unwrap(), vec!["Draft the board update".to_string()]);
    drop(shared);
    std::fs::remove_dir_all(root).unwrap();
}

/// Item 3: his first reply is never delayed by the title. The titler is held until his reply
/// has come back; if the spine waited for it, it would give up holding and no title would come.
#[test]
fn his_reply_never_waits_for_the_title() {
    let (release, held) = mpsc::channel::<()>();
    let held = Mutex::new(held);
    let mut f = fixture("unblocked", move |_| {
        held.lock().unwrap().recv_timeout(GUARD).map_err(|_| "the reply waited for the title".to_string())?;
        Ok("Offsite plan".into())
    });
    f.spine.submit_prompt("Plan the team offsite for March", Source::Text).unwrap();
    let reply = f.spine.messages(&f.thread).unwrap();
    assert!(reply.iter().any(|m| m.role == "assistant" && m.text == "On it!"), "his reply came back first: {reply:?}");
    release.send(()).unwrap();
    let (_, title) = f.titles.recv_timeout(GUARD).expect("the title still arrives after the reply");
    assert_eq!(title, "Offsite plan");
    drop(f.spine);
    std::fs::remove_dir_all(f.root).unwrap();
}

/// Item 3: Haiku cannot be reached, and the conversation goes on as normal. That a failure
/// sends no title (so the provisional one stays) is `spine.rs`'s `title_tests`, checked
/// synchronously on the titler thread's own body.
#[test]
fn an_unreachable_haiku_does_not_hold_up_the_conversation() {
    let (tried, attempts) = mpsc::channel();
    let tried = Mutex::new(tried);
    let mut f = fixture("unreachable", move |_| {
        tried.lock().unwrap().send(()).unwrap();
        Err("Classifier disconnected".into())
    });
    f.spine.submit_prompt("Draft the board update", Source::Text).unwrap();
    attempts.recv_timeout(GUARD).expect("Haiku was asked");
    f.spine.submit_prompt("Keep it short", Source::Text).unwrap();
    assert_eq!(f.spine.messages(&f.thread).unwrap().iter().filter(|m| m.role == "assistant").count(), 2);
    assert_eq!(f.requests.load(Ordering::SeqCst), 1, "asked once, never again after the failure");
    drop(f.spine);
    std::fs::remove_dir_all(f.root).unwrap();
}

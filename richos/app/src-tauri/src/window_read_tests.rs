use super::*;
use richos_core::cognition::TurnItem;
use std::sync::mpsc;
use std::time::Duration;

struct HeldTurn {
    release: mpsc::Receiver<()>,
}

// A provider callback only queues text. The published chunk proves it was written.
struct PublishedText(mpsc::Sender<()>);
impl TurnObserver for PublishedText {
    fn on_event(&self, event: &StreamEvent) {
        if matches!(event, StreamEvent::Chunk { .. }) {
            self.0.send(()).unwrap();
        }
    }
}

impl Cognition for HeldTurn {
    fn session_id(&self) -> &str { "read-test" }

    fn prompt(&mut self, _: &str, _: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        unreachable!("this fixture streams through the native pending-reply interface")
    }

    fn enqueue_prompt(&mut self, _: &str, started: Box<dyn FnOnce() -> Result<(), String> + Send>)
        -> Result<richos_core::cognition::PendingReply, CognitionError> {
        started().map_err(CognitionError::Protocol)?;
        let release = std::mem::replace(&mut self.release, mpsc::channel().1);
        Ok(richos_core::cognition::PendingReply::new(move |on_item| {
            on_item(TurnItem::Text { seq: 1, text: "First words" });
            release.recv().unwrap();
            Ok("end_turn".into())
        }))
    }
}

#[test]
fn window_reads_finish_before_an_unfinished_turn_releases() {
    let path = std::env::temp_dir().join(format!("window-read-gate-{}.jsonl", std::process::id()));
    if let Err(error) = std::fs::remove_file(&path) {
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    }
    let mut spine = Spine::new(Ledger::open(&path).unwrap());
    spine.set_entity_registry(EntityRegistry::new(vec![
        Entity::new("alpha", "Alpha", &["/fixture/alpha"]).unwrap(),
    ]).unwrap());
    let thread = spine.create_thread("A conversation", &EntityId::parse("alpha").unwrap()).unwrap();
    let (written_tx, written_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    spine.set_observer(Box::new(PublishedText(written_tx)));
    spine.attach_lease(Box::new(HeldTurn { release: release_rx }));
    let reader = spine.reader();
    let spine = Arc::new(Mutex::new(spine));
    let writer = spine.clone();
    let turn = std::thread::spawn(move || writer.lock().unwrap().submit_prompt("Hold the turn", Source::Text));
    let written = written_rx.recv_timeout(Duration::from_secs(5));
    let (read_tx, read_rx) = mpsc::channel();
    let read = std::thread::spawn(move || {
        let view = reader.snapshot();
        let tree = build_navigation_tree(&*view, &nav::NavState::default());
        let messages = view.messages(&thread).unwrap();
        let timeline = timeline_payload(&*view, &thread).unwrap();
        read_tx.send((tree.groups[0].threads[0].has_pending_turn,
            messages.iter().any(|m| m.text == "First words"), timeline)).unwrap();
    });
    let result = read_rx.recv_timeout(Duration::from_millis(500));
    // Always release and join, including the failing baseline, so a red test leaks no worker.
    release_tx.send(()).unwrap();
    turn.join().unwrap().unwrap();
    read.join().unwrap();
    std::fs::remove_file(path).unwrap();
    written.expect("the lease never published its first text");
    let (pending, has_text, _) = result.expect("window reads waited for the unfinished turn");
    assert!(pending, "the read must see an unfinished turn");
    assert!(has_text, "already written reply text must be available during the turn");
}
use crate::timeline_view::timeline_payload;

#[test]
fn corrections_follow_the_company_of_the_open_conversation_not_the_launch_selection() {
    let path = std::env::temp_dir().join(format!("correction-entity-{}.jsonl", std::process::id()));
    let empty_path = std::env::temp_dir().join(format!("correction-entity-empty-{}.jsonl", std::process::id()));
    for p in [&path, &empty_path] {
        if let Err(error) = std::fs::remove_file(p) {
            assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        }
    }
    let mut spine = Spine::new(Ledger::open(&path).unwrap());
    spine.set_entity_registry(EntityRegistry::new(vec![
        Entity::new("alpha", "Alpha", &["/fixture/alpha"]).unwrap(),
        Entity::new("beta", "Beta", &["/fixture/beta"]).unwrap(),
    ]).unwrap());
    let alpha = EntityId::parse("alpha").unwrap();
    let beta = EntityId::parse("beta").unwrap();
    let a_thread = spine.create_thread("A's work", &alpha).unwrap();
    let b_thread = spine.create_thread("B's work", &beta).unwrap();
    let reader = spine.reader();
    // Launch selection is company A; the conversation on screen is B's.
    spine.switch_thread(&a_thread).unwrap();
    assert_eq!(correction_entity(&*reader.snapshot(), Some(alpha.clone())).unwrap(), alpha);
    spine.switch_thread(&b_thread).unwrap();
    assert_eq!(correction_entity(&*reader.snapshot(), Some(alpha.clone())).unwrap(), beta,
        "a belief correction is filed under the open conversation's company; the queue and confirm must ask for the same one");
    // Nothing open: the launch selection is the scope; with neither, refuse rather than guess.
    let mut empty = Spine::new(Ledger::open(&empty_path).unwrap());
    assert!(correction_entity(&*empty.reader().snapshot(), None).is_err());
    assert_eq!(correction_entity(&*empty.reader().snapshot(), Some(alpha.clone())).unwrap(), alpha);
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_file(&empty_path).ok();
}

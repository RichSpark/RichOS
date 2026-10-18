//! Native session input provenance. App-driven rotation and automatic replay are removed.
mod support;
use richos_core::{cognition::{Cognition, CognitionError, LeaseFactory, MockCognition, TurnItem}, ledger::{Ledger, Source}};
use richos_core::entity::EntityId;
use std::sync::{Arc, Mutex};
fn femcboost() -> EntityId { EntityId::parse("femcboost").unwrap() }
fn tmp_ledger(tag: &str) -> (std::path::PathBuf, Ledger) {
    let path = std::env::temp_dir().join(format!("native-input-{tag}-{}.jsonl",uuid::Uuid::new_v4()));
    let ledger = Ledger::open(&path).unwrap();
    (path,ledger)
}
struct FailingCognition { session_id: String }
impl Cognition for FailingCognition {
    fn session_id(&self)->&str { &self.session_id }
    fn prompt(&mut self,_text:&str,on:&mut dyn FnMut(TurnItem))->Result<String,CognitionError> {
        on(TurnItem::Text{seq:0,text:"partial before disconnect"});
        Err(CognitionError::Io("adapter exited mid-turn".into()))
    }
}
#[test]
fn phone_answer_provenance_survives_a_failed_turn_without_replaying_it() {
    struct Capture {
        inner: Box<dyn Cognition>,
        channels: Arc<Mutex<Vec<Option<String>>>>,
    }
    impl Cognition for Capture {

        fn session_id(&self) -> &str { self.inner.session_id() }

        fn prompt(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String,CognitionError> {
            self.inner.prompt(text,on_item)
        }
        fn set_input_channel(&mut self, channel: Option<&str>) -> Result<(),CognitionError> {
            self.channels.lock().unwrap().push(channel.map(str::to_string)); Ok(())
        }
    }
    struct Factory(Arc<Mutex<Vec<Option<String>>>>);
    impl LeaseFactory for Factory {
        fn spawn(&self) -> Result<Box<dyn Cognition>,CognitionError> {
            Ok(Box::new(Capture { inner:Box::new(MockCognition::new("recovered",vec!["Got it", "Next reply"])),channels:self.0.clone() }))
        }
    }
    for keep in [false,true] {
        for channel in ["phone_typed","phone_voice"] {
            let (path, ledger)=tmp_ledger(&format!("phone-replay-{keep}-{channel}"));
            let mut spine=support::spine(ledger);
            let thread=spine.create_thread("General",&femcboost()).unwrap();
            spine.keep_intake_channel(keep);
            let channels=Arc::new(Mutex::new(Vec::new()));
            spine.attach_lease(Box::new(Capture {inner:Box::new(FailingCognition {session_id:"doomed".into()}),channels:channels.clone()}));
            spine.set_lease_factory(Box::new(Factory(channels.clone())));
            let intake=path.with_extension("intake");
            let control=richos_core::steering::TurnControl::open(&intake).unwrap();
            spine.set_turn_control(control.clone());
            control.submit_from_channel(&thread,Some(femcboost()),"Choose tomorrow",channel).unwrap();
            assert!(spine.poll_intake().is_err());
            assert_eq!(*channels.lock().unwrap(),vec![Some(channel.into())]);
            let binding=spine.ledger().thread_binding(&thread).unwrap();
            for turn in spine.ledger().thread_turns_scoped(&binding).unwrap().into_iter().filter(|t|t.source==Source::Text) {
                assert_eq!(turn.channel.as_deref(),keep.then_some(channel));
            }
            assert!(spine.submit_prompt("Next request from this Mac",Source::Text).is_err());
            assert_eq!(channels.lock().unwrap().last().cloned(),Some(keep.then(|| "desk".to_string())));
            drop(spine);
            drop(std::fs::remove_file(path));
            drop(std::fs::remove_file(intake));
        }
    }
}

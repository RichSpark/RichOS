//! **In voice mode, Rich's "On it!" is spoken the moment it is shown** (nightly 57's product
//! hunt, finding 3). Since a8208a465 the spine no longer writes "On it!" ahead of the model: on
//! his go Rich interprets it and the model's own reply carries the words. Voice mode speaks a reply by
//! feeding each `rich://chunk` delta to the sentence chunker (`ui/main.js` relays it to
//! `voice_speak_delta`), and the chunker holds a sentence until it sees past its full stop. So a
//! reply that opens "On it!" and goes on is spoken from "On it!" as it is shown, and the rest at
//! turn end; a reply that is only "On it!" is spoken when the turn completes.
//!
//! This runs the real spine with a mock model and the production chunker, relaying exactly as
//! `ui/main.js` does. No audio device, synthesizer or app is started.

use richos_core::cognition::MockCognition;
use richos_core::entity::{Entity, EntityId, EntityRegistry};
use richos_core::ledger::{Ledger, Source};
use richos_core::spine::Spine;
use richos_core::stream::{StreamEvent, TurnObserver};
use richos_voice::chunk::SentenceChunker;
use std::sync::{Arc, Mutex};

/// The voice relay: chunk deltas to the chunker as they arrive, the tail at turn end.
struct Speaker {
    said: Arc<Mutex<Vec<String>>>,
    chunker: Mutex<SentenceChunker>,
}

impl TurnObserver for Speaker {
    fn on_event(&self, event: &StreamEvent) {
        let mut chunker = self.chunker.lock().unwrap();
        match event {
            StreamEvent::Chunk { text_delta, .. } => {
                for sentence in chunker.push(text_delta) {
                    self.said.lock().unwrap().push(format!("as it is shown: {sentence}"));
                }
            }
            StreamEvent::TurnCompleted { .. } => {
                if let Some(sentence) = chunker.flush() {
                    self.said.lock().unwrap().push(format!("at turn end: {sentence}"));
                }
            }
            _ => {}
        }
    }
}

#[test]
fn on_it_is_spoken_the_moment_it_is_shown_on_his_spoken_go() {
    let path = std::env::temp_dir().join(format!("richos-voice-on-it-{}.jsonl", std::process::id()));
    if path.exists() {
        std::fs::remove_file(&path).unwrap();
    }
    let mut spine = Spine::new(Ledger::open(&path).unwrap());
    spine
        .set_entity_registry(EntityRegistry::new(vec![Entity::new("acme", "Acme", &["/fixture/acme"]).unwrap()]).unwrap());
    spine.create_thread("Onboarding", &EntityId::parse("acme").unwrap()).unwrap();
    // The proposal, then the register's receipt as the real lease streams it.
    spine.attach_lease(Box::new(MockCognition::new(
        "voice-on-it",
        vec!["Will (Marketing Director). Say go and I'll get them hired now.", "On it! Landing now."],
    )));
    spine.submit_prompt("That is my business.", Source::Text).unwrap();

    let said = Arc::new(Mutex::new(Vec::new()));
    spine.set_observer(Box::new(Speaker { said: said.clone(), chunker: Mutex::new(SentenceChunker::new()) }));
    let go = spine.submit_prompt("Go.", Source::Jam).unwrap();

    assert_eq!(
        *said.lock().unwrap(),
        ["as it is shown: On it!", "at turn end: Landing now."],
        "voice mode must say \"On it!\" when it is shown, once, and the rest at turn end"
    );
    assert_eq!(spine.ledger().turn(&go).unwrap().assistant_text, "On it! Landing now.", "the saved reply is unchanged");
    drop(spine);
    std::fs::remove_file(&path).unwrap();
}

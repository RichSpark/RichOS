//! Native handoff from an audio cut to the conversation ledger.

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Observation {
    Cutoff {
        confirmed_chunks: usize,
        last_confirmed: Option<String>,
        first_unconfirmed: Option<String>,
    },
    Withdrawn,
}

#[derive(Clone, Debug)]
pub struct ReplyObservation {
    pub thread_id: String,
    pub turn_id: String,
    pub observation: Observation,
}

#[derive(Clone, Default)]
pub struct Pending(Arc<Mutex<Vec<ReplyObservation>>>);

impl Pending {
    /// Bracket the cut and its native publication with the same lock used by
    /// request preparation. Never hold this lock while writing to disk.
    pub fn record(&self, observe: impl FnOnce() -> Vec<ReplyObservation>) -> bool {
        let mut pending = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let observed = observe();
        let added = !observed.is_empty();
        pending.extend(observed);
        added
    }

    pub(crate) fn take(&self) -> Vec<ReplyObservation> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(|p| p.into_inner()))
    }
}

pub(crate) fn context(turns: &[&crate::ledger::Turn], before: &str) -> Option<String> {
    use crate::ledger::Source;
    let reply = turns
        .iter()
        .take_while(|turn| turn.id != before)
        .filter(|turn| {
            matches!(turn.source, Source::Text | Source::Jam)
                && turn.superseded_by.is_none()
                && turn.voice_playback != Some(Observation::Withdrawn)
                && !turn.assistant_text.trim().is_empty()
        })
        .last()?;
    let Some(Observation::Cutoff {
        confirmed_chunks,
        last_confirmed,
        first_unconfirmed,
    }) = &reply.voice_playback
    else {
        return None;
    };
    Some(serde_json::json!({
        "richos_reply_playback": {
            "reply_turn_id": reply.id,
            "confirmed_spoken_chunks": confirmed_chunks,
            "last_confirmed_spoken_chunk": last_confirmed,
            "first_unconfirmed_spoken_chunk": first_unconfirmed,
            "status": "Playback stopped. The written reply is in the chat. These chunks describe audio delivery. Resume the prior answer only if the user asks to continue. When the user only acknowledges, acknowledge briefly and wait. Answer corrections, new questions and quick answers normally.",
        }
    }).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::EntityId;
    use crate::ledger::{Ledger, Source};

    #[test]
    fn the_first_cut_survives_reopen_and_ignores_other_and_withdrawn_turns() {
        let dir = std::env::temp_dir().join(format!("richos-playback-{}", uuid::Uuid::new_v4()));
        let path = dir.join("ledger.jsonl");
        let mut ledger = Ledger::open(&path).unwrap();
        let entity = EntityId::parse("desk").unwrap();
        let thread = ledger.create_thread("Talk", &entity).unwrap();
        let other = ledger.create_thread("Other", &entity).unwrap();
        let binding = ledger.thread_binding(&thread).unwrap();
        let first = ledger
            .record_prompt_received(&binding, "Tell me two things", Source::Text)
            .unwrap();
        ledger
            .append_assistant_delta(&first, "The build passed. The release is pending.", 0)
            .unwrap();
        let cutoff = Observation::Cutoff {
            confirmed_chunks: 1,
            last_confirmed: Some("The build passed.".into()),
            first_unconfirmed: Some("The release is pending.".into()),
        };
        let mut fact = ReplyObservation {
            thread_id: other.clone(),
            turn_id: first.clone(),
            observation: cutoff.clone(),
        };
        assert!(!ledger.record_voice_reply(&fact).unwrap());
        fact.thread_id = thread.clone();
        assert!(ledger.record_voice_reply(&fact).unwrap());
        fact.observation = Observation::Cutoff {
            confirmed_chunks: 2,
            last_confirmed: Some("Wrong later position".into()),
            first_unconfirmed: None,
        };
        assert!(!ledger.record_voice_reply(&fact).unwrap());

        let partial = ledger
            .record_prompt_received(&binding, "Wait", Source::Jam)
            .unwrap();
        ledger
            .append_assistant_delta(&partial, "This partial response does not stand.", 0)
            .unwrap();
        ledger
            .record_voice_reply(&ReplyObservation {
                thread_id: thread.clone(),
                turn_id: partial,
                observation: Observation::Withdrawn,
            })
            .unwrap();
        let replyless = ledger
            .record_prompt_received(&binding, "Current status?", Source::Text)
            .unwrap();
        ledger
            .record_voice_reply(&ReplyObservation {
                thread_id: thread.clone(),
                turn_id: replyless.clone(),
                observation: Observation::Cutoff {
                    confirmed_chunks: 0,
                    last_confirmed: None,
                    first_unconfirmed: None,
                },
            })
            .unwrap();
        ledger.complete_turn(&replyless, "end_turn").unwrap();
        let next = ledger
            .record_prompt_received(&binding, "Wait, keep going", Source::Jam)
            .unwrap();
        drop(ledger);

        let mut ledger = Ledger::open(&path).unwrap();
        let turns = ledger.thread_turns(&thread).unwrap();
        let context = context(&turns, &next).unwrap();
        let packet: serde_json::Value = serde_json::from_str(&context).unwrap();
        assert_eq!(packet["richos_reply_playback"]["reply_turn_id"], first);
        assert_eq!(
            packet["richos_reply_playback"]["confirmed_spoken_chunks"],
            1
        );
        assert_eq!(
            packet["richos_reply_playback"]["last_confirmed_spoken_chunk"],
            "The build passed."
        );
        assert_eq!(
            packet["richos_reply_playback"]["first_unconfirmed_spoken_chunk"],
            "The release is pending."
        );
        assert!(super::context(&ledger.thread_turns(&other).unwrap(), "none").is_none());
        assert_eq!(ledger.turn(&next).unwrap().user_text, "Wait, keep going");
        assert_eq!(
            ledger.turn(&first).unwrap().assistant_text,
            "The build passed. The release is pending."
        );
        ledger
            .append_assistant_delta(&next, "The release is pending.", 0)
            .unwrap();
        let later = ledger
            .record_prompt_received(&binding, "Another question", Source::Text)
            .unwrap();
        assert!(super::context(&ledger.thread_turns(&thread).unwrap(), &later).is_none());
        drop(ledger);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

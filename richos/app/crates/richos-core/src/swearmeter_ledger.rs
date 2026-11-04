//! Numeric projection of top-level durable ledger events, never generated turns.
use crate::{ledger::{Event, Source}, swearmeter};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Default)]
struct Entry { count: u64 }

#[derive(Clone, Default)]
pub struct Projection {
    entries: HashMap<String, Entry>,
    intakes: HashMap<u64, String>,
    aliases: HashMap<String, String>,
    count: u64,
    composites: HashSet<String>,
}

pub fn is_composite(id: &str) -> bool {
    id.starts_with("question-answer:") || id.starts_with("question-followup:")
}

impl Projection {
    pub fn count(&self) -> u64 { self.count }
    pub fn composites(&self) -> &HashSet<String> { &self.composites }

    fn root(&self, id: &str) -> String {
        let mut root = id;
        // Only our own root-to-root links enter this map, so no input can form a cycle.
        while let Some(parent) = self.aliases.get(root) { root = parent; }
        root.to_string()
    }

    pub(crate) fn observe(&mut self, event: &Event) {
        match event {
            Event::PromptReceived { turn_id, text, source, intake_id, pressed, .. } => {
                let key = self.root(turn_id);
                if self.entries.contains_key(&key) { return; }
                let eligible = matches!(source, Source::Text | Source::Jam)
                    && !pressed && !is_composite(&key);
                if eligible {
                    if let Some(existing) = intake_id.and_then(|id| self.intakes.get(&id)) {
                        let existing = self.root(existing);
                        self.aliases.insert(key, existing);
                        return;
                    }
                }
                let count = if eligible { swearmeter::count(text) } else { 0 };
                if eligible {
                    if let Some(id) = intake_id { self.intakes.insert(*id, key.clone()); }
                }
                if is_composite(&key) { self.composites.insert(key.clone()); }
                self.count += count;
                self.entries.insert(key, Entry { count });
            }
            Event::TurnReplayed { turn_id, by_turn_id, .. }
            | Event::TurnSuperseded { turn_id, by_turn_id, .. } => self.link(turn_id, by_turn_id),
            _ => {}
        }
    }

    fn link(&mut self, original: &str, continuation: &str) {
        let parent = self.root(original);
        let child = self.root(continuation);
        if parent == child { return; }
        if let Some(mut duplicate) = self.entries.remove(&child) {
            self.count -= duplicate.count;
            if !self.entries.contains_key(&parent) {
                if is_composite(&parent) { duplicate.count = 0; }
                self.count += duplicate.count;
                self.entries.insert(parent.clone(), duplicate);
            }
        }
        self.composites.remove(&child);
        if is_composite(&parent) { self.composites.insert(parent.clone()); }
        self.aliases.insert(child, parent);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn prompt(id: &str, source: &str, intake: Option<u64>, pressed: bool) -> Event {
        serde_json::from_value(json!({"event":"PromptReceived", "turn_id":id,
            "thread_id":"thread", "text":"fucking shit", "source":source, "at":1,
            "intake_id":intake,"pressed":pressed})).unwrap()
    }
    fn replay(original: &str, next: &str) -> Event {
        Event::TurnReplayed { turn_id:original.into(), by_turn_id:next.into(), at:2 }
    }

    #[test]
    fn actual_submissions_count_once_and_new_identical_submissions_still_count() {
        let mut p = Projection::default();
        p.observe(&prompt("first", "text", Some(1), false));
        p.observe(&prompt("retry", "text", Some(1), false));
        p.observe(&prompt("new-send", "text", Some(2), false));
        p.observe(&prompt("voice", "jam", Some(3), false));
        assert_eq!(p.count(), 6);
        p.observe(&replay("first", "continued"));
        p.observe(&prompt("continued", "text", None, false));
        assert_eq!(p.count(), 6);
    }

    #[test]
    fn generated_words_and_question_composites_never_count_as_human_input() {
        let mut p = Projection::default();
        for source in ["agent", "internal", "proactive"] {
            p.observe(&prompt(source, source, Some(1), false));
        }
        p.observe(&prompt("button", "text", Some(1), true));
        p.observe(&prompt("question-answer:set", "text", Some(1), false));
        p.observe(&replay("question-answer:set", "ordinary-looking-id"));
        p.observe(&prompt("ordinary-looking-id", "text", None, false));
        assert_eq!(p.count(), 0);
        assert!(p.composites().contains("question-answer:set"));
        // An internal copy cannot steal the intake identity of the actual message.
        p.observe(&prompt("actual", "text", Some(1), false));
        assert_eq!(p.count(), 2);
    }

    #[test]
    fn legacy_supersession_reconciles_separately_persisted_copies() {
        let mut p = Projection::default();
        p.observe(&prompt("original", "text", None, false));
        p.observe(&prompt("old-copy", "text", None, false));
        p.observe(&replay("original", "old-copy"));
        assert_eq!(p.count(), 2);
        p.observe(&prompt("question-followup:legacy", "text", None, false));
        p.observe(&prompt("old-plain-copy", "text", None, false));
        p.observe(&replay("question-followup:legacy", "old-plain-copy"));
        assert_eq!(p.count(), 2);
    }
}

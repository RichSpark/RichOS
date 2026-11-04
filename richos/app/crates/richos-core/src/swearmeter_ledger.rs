//! Numeric projection of top-level durable ledger events, never generated turns.
use crate::{ledger::{Event, Source}, swearmeter};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Default)]
struct Entry { count: u64, unknown: bool }

#[derive(Clone, Default)]
pub struct Projection {
    entries: HashMap<String, Entry>,
    intakes: HashMap<u64, String>,
    aliases: HashMap<String, String>,
    clients: HashSet<(String, String)>,
    count: u64,
    unknown_inputs: u64,
    revision: u64,
    composites: HashSet<String>,
}

pub fn is_composite(id: &str) -> bool {
    id.starts_with("question-answer:") || id.starts_with("question-followup:")
}

impl Projection {
    pub fn has_intake(&self, id: u64) -> bool { self.intakes.contains_key(&id) }
    pub fn has_client(&self, thread: &str, id: &str) -> bool { self.clients.contains(&(thread.into(), id.into())) }
    pub fn count(&self) -> u64 { self.count }
    pub fn unknown_inputs(&self) -> u64 { self.unknown_inputs }
    pub fn revision(&self) -> u64 { self.revision }
    pub fn composites(&self) -> &HashSet<String> { &self.composites }

    fn root(&self, id: &str) -> String {
        let mut root = id;
        // Only our own root-to-root links enter this map, so no input can form a cycle.
        while let Some(parent) = self.aliases.get(root) { root = parent; }
        root.to_string()
    }

    pub(crate) fn observe(&mut self, event: &Event) {
        match event {
            Event::PromptReceived { turn_id, text, source, intake_id, pressed, authored_bytes, thread_id, client_id, .. } => {
                self.revision += 1;
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
                let authored = match authored_bytes {
                    Some(bytes) => text.get(..*bytes),
                    None if ambiguous_legacy_attachment(text) => None,
                    None => Some(text.as_str()),
                };
                let unknown = eligible && authored.is_none();
                let count = if eligible { authored.map(swearmeter::count).unwrap_or(0) } else { 0 };
                if eligible {
                    if let Some(id) = intake_id { self.intakes.insert(*id, key.clone()); }
                    if let Some(id) = client_id { self.clients.insert((thread_id.clone(), id.clone())); }
                }
                if is_composite(&key) && authored_bytes.is_none() { self.composites.insert(key.clone()); }
                self.count += count;
                self.unknown_inputs += u64::from(unknown);
                self.entries.insert(key, Entry { count, unknown });
            }
            Event::TurnReplayed { turn_id, by_turn_id, .. }
            | Event::TurnSuperseded { turn_id, by_turn_id, .. } => {
                self.revision += 1;
                self.link(turn_id, by_turn_id);
            }
            _ => {}
        }
    }

    fn link(&mut self, original: &str, continuation: &str) {
        let parent = self.root(original);
        let child = self.root(continuation);
        if parent == child { return; }
        let legacy = self.composites.contains(&parent) || self.composites.contains(&child)
            || (!self.entries.contains_key(&parent) && is_composite(&parent));
        if let Some(mut duplicate) = self.entries.remove(&child) {
            self.count -= duplicate.count;
            self.unknown_inputs -= u64::from(duplicate.unknown);
            if !self.entries.contains_key(&parent) {
                if is_composite(&parent) { duplicate.count = 0; duplicate.unknown = false; }
                self.count += duplicate.count;
                self.unknown_inputs += u64::from(duplicate.unknown);
                self.entries.insert(parent.clone(), duplicate);
            }
        }
        self.composites.remove(&child);
        if legacy { self.composites.insert(parent.clone()); }
        self.aliases.insert(child, parent);
    }
}

// Older records did not retain the producer's text boundary. An exact attachment
// descriptor shape is ambiguous without it. Do not count its generated words or
// silently strip a possible quotation: qualify the whole old input as unknown.
pub(crate) fn ambiguous_legacy_attachment(text: &str) -> bool {
    text.lines().any(|line| (line.starts_with("Attached on this Mac (")
        || line.starts_with("Attached from the phone (") || line.starts_with("Folders on this Mac (")) && line.ends_with("):"))
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

    #[test]
    fn attachment_producer_boundary_preserves_quotes_and_excludes_generated_paths() {
        let quoted = "shit\n\nAttached on this Mac (1 file, saved by RichOS):\n- /fixture/wtf.png (image/png, 1 bytes)";
        let mut attached = serde_json::to_value(prompt("attached", "text", None, false)).unwrap();
        attached["text"] = quoted.into();
        attached["authored_bytes"] = 4.into();
        let mut p = Projection::default();
        p.observe(&serde_json::from_value(attached.clone()).unwrap());
        assert_eq!(p.count(), 1);
        // The exact same characters can be a genuine pasted quotation.
        attached["turn_id"] = "quotation".into();
        attached["authored_bytes"] = quoted.len().into();
        p.observe(&serde_json::from_value(attached.clone()).unwrap());
        assert_eq!(p.count(), 3);
        attached["turn_id"] = "legacy".into();
        attached.as_object_mut().unwrap().remove("authored_bytes");
        p.observe(&serde_json::from_value(attached).unwrap());
        assert_eq!(p.count(), 3);
        assert_eq!(p.unknown_inputs(), 1);
    }
    #[test]
    fn a_refused_writer_does_not_fold_an_unwritten_input() {
        use crate::{ledger::Ledger, entity::EntityId};
        let path = std::env::temp_dir().join(format!("swearmeter-write-{}", uuid::Uuid::new_v4()));
        let mut ledger = Ledger::open(&path).unwrap();
        let thread = ledger.create_thread("Fixture", &EntityId::parse("fixture").unwrap()).unwrap();
        let binding = ledger.thread_binding(&thread).unwrap();
        let mut reader = ledger.read_copy();
        assert!(reader.record_prompt_received(&binding, "shit", Source::Text).is_err());
        assert_eq!(reader.swearmeter().count(), 0);
        assert_eq!(Ledger::open(&path).unwrap().swearmeter().count(), 0);
        drop(ledger);
        std::fs::remove_file(path).unwrap();
    }

}

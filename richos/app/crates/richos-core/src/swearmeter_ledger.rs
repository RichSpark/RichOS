//! Numeric projection of top-level durable ledger events, never generated turns.
use crate::{ledger::{Event, Source}, swearmeter, swearmeter_periods::Dates};
use std::{collections::{HashMap, HashSet}, sync::Arc};
use crate::ledger::Turn;

/// Disposable numeric state, bound to the exact source by the checkpoint envelope.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Frozen {
    pub events: u64,
    counts: Vec<u64>,
    intakes: Vec<u64>,
    aliases: HashMap<String, String>,
    clients: Vec<(String, String)>,
    composites: HashSet<String>,
    orphans: HashMap<String, (u64, bool, Option<u64>)>,
    dates: Dates,
    timestamps: Vec<Option<u64>>,
    count: u64,
    unknown_inputs: u64,
}
impl Frozen {
    pub fn valid(&self) -> bool {
        let count = self.counts.iter().map(|v| v >> 1).chain(self.orphans.values().map(|v| v.0)).try_fold(0u64, u64::checked_add);
        let unknown: u64 = self.counts.iter().map(|v| v & 1).sum::<u64>() + self.orphans.values().filter(|v| v.1).count() as u64;
        count == Some(self.count) && unknown == self.unknown_inputs
            && self.counts.len() as u64 <= self.events
            && self.counts.len() == self.timestamps.len()
            && self.intakes.windows(2).all(|p| p[0] < p[1])
            && self.clients.windows(2).all(|p| p[0] < p[1])
            && self.aliases.keys().all(|id| {
                let mut key = id;
                for _ in 0..=self.aliases.len() {
                    match self.aliases.get(key) { Some(parent) => key = parent, None => return true }
                }
                false
            })
    }
}


#[derive(Clone, Default)]
struct Entry { count: u64, unknown: bool, at: Option<u64> }

#[derive(Clone, Default)]
pub struct Projection {
    entries: HashMap<String, Entry>,
    intakes: HashMap<u64, String>,
    aliases: HashMap<String, String>,
    clients: HashSet<(String, String)>,
    count: u64,
    unknown_inputs: u64,
    dates: Dates,
    revision: u64,
    composites: HashSet<String>,
    frozen: Option<Arc<Frozen>>,
    retired: HashSet<String>,
    known_roots: HashSet<String>,
}

pub fn is_composite(id: &str) -> bool {
    id.starts_with("question-answer:") || id.starts_with("question-followup:")
}

impl Projection {
    pub fn has_intake(&self, id: u64) -> bool { self.intakes.contains_key(&id) || self.frozen.as_ref().is_some_and(|f| f.intakes.binary_search(&id).is_ok()) }
    pub fn has_client(&self, thread: &str, id: &str) -> bool { self.clients.contains(&(thread.into(), id.into())) || self.frozen.as_ref().is_some_and(|f| f.clients.binary_search_by(|(t, c)| (t.as_str(), c.as_str()).cmp(&(thread, id))).is_ok()) }
    pub fn checkpoint_reused(&self) -> bool { self.frozen.is_some() }
    pub fn count(&self) -> u64 { self.count }
    pub fn dates(&self) -> &Dates { &self.dates }
    pub fn unknown_inputs(&self) -> u64 { self.unknown_inputs }
    pub fn revision(&self) -> u64 { self.revision }
    pub fn composites(&self) -> &HashSet<String> { &self.composites }

    pub fn from_frozen(frozen: Frozen) -> Self {
        Self { count: frozen.count, unknown_inputs: frozen.unknown_inputs, dates: frozen.dates.clone(),
            aliases: frozen.aliases.clone(), composites: frozen.composites.clone(),
            frozen: Some(Arc::new(frozen)), ..Self::default() }
    }

    pub fn freeze(&self, turns: &[Turn], events: u64) -> Frozen {
        let mut counts = Vec::with_capacity(turns.len());
        let mut timestamps = Vec::with_capacity(turns.len());
        let ids: HashSet<&str> = turns.iter().map(|t| t.id.as_str()).collect();
        for turn in turns {
            let packed = if let Some(entry) = self.entries.get(&turn.id) {
                (entry.count << 1) | u64::from(entry.unknown)
            } else if self.retired.contains(&turn.id) { 0 }
            else { turn.swearmeter_checkpoint_index.and_then(|index| self.frozen.as_ref().and_then(|f| f.counts.get(index))).copied().unwrap_or(0) };
            counts.push(packed);
            timestamps.push(self.entries.get(&turn.id).map(|entry| entry.at).unwrap_or_else(|| {
                turn.swearmeter_checkpoint_index.and_then(|index| self.frozen.as_ref()?.timestamps.get(index).copied().flatten())
            }));
        }
        let mut intakes = self.frozen.as_ref().map(|f| f.intakes.clone()).unwrap_or_default();
        intakes.extend(self.intakes.keys().copied());
        intakes.sort_unstable(); intakes.dedup();
        let mut clients = self.frozen.as_ref().map(|f| f.clients.clone()).unwrap_or_default();
        clients.extend(self.clients.iter().cloned());
        clients.sort_unstable(); clients.dedup();
        let mut orphans = self.frozen.as_ref().map(|f| f.orphans.clone()).unwrap_or_default();
        orphans.retain(|id, _| !self.retired.contains(id));
        for (id, entry) in &self.entries {
            if !ids.contains(id.as_str()) { orphans.insert(id.clone(), (entry.count, entry.unknown, entry.at)); }
        }
        Frozen { events, counts, intakes, clients, orphans, dates: self.dates.clone(), timestamps, count: self.count,
            unknown_inputs: self.unknown_inputs, aliases: self.aliases.clone(), composites: self.composites.clone() }
    }

    fn old_entry(&self, id: &str, turns: &[Turn]) -> Option<Entry> {
        if self.retired.contains(id) { return None; }
        let frozen = self.frozen.as_ref()?;
        if let Some((count, unknown, at)) = frozen.orphans.get(id) {
            return Some(Entry { count: *count, unknown: *unknown, at: *at });
        }
        let index = turns.iter().find(|t| t.id == id)?.swearmeter_checkpoint_index?;
        let packed = frozen.counts[index];
        Some(Entry { count: packed >> 1, unknown: packed & 1 != 0, at: frozen.timestamps[index] })
    }

    fn root(&self, id: &str) -> String {
        let mut root = id;
        // Only our own root-to-root links enter this map, so no input can form a cycle.
        while let Some(parent) = self.aliases.get(root) { root = parent; }
        root.to_string()
    }

    #[cfg(test)]
    pub(crate) fn observe(&mut self, event: &Event) { self.observe_in(event, &[], None); }

    pub(crate) fn observe_in(&mut self, event: &Event, turns: &[Turn], data_dir: Option<&std::path::Path>) {
        match event {
            Event::PromptReceived { turn_id, text, source, intake_id, pressed, authored_bytes, authored_ranges, thread_id, client_id, at, .. } => {
                self.revision += 1;
                let key = self.root(turn_id);
                if self.entries.contains_key(&key) || self.known_roots.contains(&key) { return; }
                let eligible = matches!(source, Source::Text | Source::Jam)
                    && !pressed && !is_composite(&key);
                if eligible {
                    if let Some(existing) = intake_id.and_then(|id| self.intakes.get(&id)) {
                        let existing = self.root(existing);
                        self.aliases.insert(key, existing);
                        return;
                    }
                }
                if eligible && intake_id.is_some_and(|id| self.frozen.as_ref().is_some_and(|f| f.intakes.binary_search(&id).is_ok())) {
                    self.known_roots.insert(key);
                    return;
                }
                let authored = if !eligible { Some(0) }
                    else if let Some(ranges) = authored_ranges { swearmeter::count_ranges(text, ranges) }
                    else { match authored_bytes {
                        Some(bytes) => text.get(..*bytes).map(swearmeter::count),
                        None if ambiguous_legacy_attachment(text) => data_dir.and_then(|dir| recovered_prefix(text, dir)).map(swearmeter::count),
                        None => Some(swearmeter::count(text)),
                    }};
                let unknown = eligible && authored.is_none();
                let count = if eligible { authored.unwrap_or(0) } else { 0 };
                if eligible {
                    if let Some(id) = intake_id { self.intakes.insert(*id, key.clone()); }
                    if let Some(id) = client_id { self.clients.insert((thread_id.clone(), id.clone())); }
                }
                if is_composite(&key) { self.composites.insert(key.clone()); }
                self.count += count;
                self.unknown_inputs += u64::from(unknown);
                let at = eligible.then_some(*at);
                if eligible { self.dates.add(at, count); }
                self.entries.insert(key, Entry { count, unknown, at });
            }
            Event::TurnReplayed { turn_id, by_turn_id, .. }
            | Event::TurnSuperseded { turn_id, by_turn_id, .. } => {
                self.revision += 1;
                self.link(turn_id, by_turn_id, turns);
            }
            _ => {}
        }
    }

    fn link(&mut self, original: &str, continuation: &str, turns: &[Turn]) {
        let parent = self.root(original);
        let child = self.root(continuation);
        if parent == child { return; }
        let parent_old = self.old_entry(&parent, turns);
        let parent_known = self.entries.contains_key(&parent) || parent_old.is_some();
        if parent_old.is_some() { self.known_roots.insert(parent.clone()); }
        let child_old = self.old_entry(&child, turns);
        let legacy = self.composites.contains(&parent) || self.composites.contains(&child)
            || (!self.entries.contains_key(&parent) && is_composite(&parent));
        if let Some(mut duplicate) = self.entries.remove(&child).or(child_old) {
            if self.frozen.is_some() { self.retired.insert(child.clone()); }
            self.count -= duplicate.count;
            self.dates.remove(duplicate.at, duplicate.count);
            self.unknown_inputs -= u64::from(duplicate.unknown);
            if !parent_known {
                if is_composite(&parent) { duplicate.count = 0; duplicate.unknown = false; }
                self.count += duplicate.count;
                if duplicate.at.is_some() { self.dates.add(duplicate.at, duplicate.count); }
                self.unknown_inputs += u64::from(duplicate.unknown);
                self.entries.insert(parent.clone(), duplicate);
            }
        }
        self.composites.remove(&child);
        if legacy { self.composites.insert(parent.clone()); }
        self.aliases.insert(child, parent);
    }
}

// Older app versions appended this exact descriptor after the entered text.
// Require both the full generated format and a matching RichOS-owned attachment.
// Merely quoting a heading is not sufficient evidence of a boundary.
fn recovered_prefix<'a>(text: &'a str, data_dir: &std::path::Path) -> Option<&'a str> {
    let (prefix, descriptor) = text.split_once("\n\nAttached on this Mac (")?;
    if ambiguous_legacy_attachment(prefix) { return None; }
    let (heading, files) = descriptor.split_once('\n')?;
    let expected = heading.strip_suffix(", saved by RichOS):")?;
    let (count, label) = expected.split_once(' ')?;
    let count = count.parse::<usize>().ok()?;
    if count == 0 || label != if count == 1 { "file" } else { "files" } || files.lines().count() != count { return None; }
    let root = data_dir.join("attachments").canonicalize().ok()?;
    for line in files.lines() {
        let (file, description) = line.strip_prefix("- ")?.rsplit_once(" (")?;
        let (media, bytes) = description.strip_suffix(" bytes)")?.rsplit_once(", ")?;
        if !media.contains('/') { return None; }
        let bytes = bytes.parse::<u64>().ok()?;
        let path = std::path::Path::new(file);
        if !path.is_absolute() || !path.starts_with(data_dir.join("attachments")) { return None; }
        let path = path.canonicalize().ok()?;
        if !path.starts_with(&root) || root != data_dir.canonicalize().ok()?.join("attachments") { return None; }
        let metadata = path.metadata().ok()?;
        if !metadata.is_file() || metadata.len() != bytes { return None; }
    }
    Some(prefix)
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
    fn modern_composite_source_coverage_survives_replay_and_checkpoint() {
        let mut p = Projection::default();
        let mut event = prompt("question-answer:source", "text", None, false);
        if let Event::PromptReceived { authored_bytes, .. } = &mut event { *authored_bytes = Some(0); }
        p.observe(&event);
        p.observe(&replay("question-answer:source", "continuation"));
        assert_eq!(p.count(), 0);
        assert!(p.composites().contains("question-answer:source"));
        let frozen = p.freeze(&[], 2);
        assert!(frozen.valid());
        assert!(Projection::from_frozen(frozen).composites().contains("question-answer:source"));
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

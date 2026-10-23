//! **ONE PIECE OF EVIDENCE, AND WHERE A DRAFT KEEPS THEM** (PRD §5).
//!
//! Every item carries an opaque ID, its source category, when it was observed, how it covers
//! the incident window, the parser version and a status: collected, absent, denied,
//! unreadable, unsupported or truncated. "A failed collector is not 'no errors found'": an item
//! that is not collected says so, and the report lists it under missing information.
//!
//! Items hold only SANITIZED text ([`crate::scrub`] ran before the item was made). They live in
//! the draft's private folder, one owner-only file each, and are read back by ID only: the
//! support tool can name nothing else ([`is_id`]).
use crate::collect::Category;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

/// The parser version every collector stamps on its items. Bumped when a reading changes.
pub const PARSER_VERSION: u32 = 1;

/// The whole report's local input cap (PRD §5: 10 MiB).
pub const REPORT_INPUT_CAP: u64 = 10 << 20;

/// One source's input cap (PRD §5: 2 MiB).
pub const SOURCE_INPUT_CAP: u64 = 2 << 20;

/// One source's read bound (PRD §5: 5 seconds).
pub const SOURCE_READ_BOUND: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Collected,
    Absent,
    Denied,
    Unreadable,
    Unsupported,
    Truncated,
}

impl Status {
    /// Whether the item holds evidence a report can quote.
    pub fn has_text(self) -> bool {
        matches!(self, Status::Collected | Status::Truncated)
    }
    pub fn from_io(error: &io::Error) -> Status {
        match error.kind() {
            io::ErrorKind::NotFound => Status::Absent,
            io::ErrorKind::PermissionDenied => Status::Denied,
            _ => Status::Unreadable,
        }
    }
}

/// How the item relates to the incident window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    /// Records from inside the window.
    InWindow,
    /// Nothing in the window; the most recent records are shown instead.
    MostRecent,
    /// A current state (versions, settings), not a record of the past.
    Current,
    /// Nothing could be placed in time.
    Unknown,
}

/// What the scrubber took out of an item, by kind, for the review's note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Removed {
    pub paths: u32,
    pub emails: u32,
    pub secrets: u32,
    pub names: u32,
}

impl Removed {
    pub fn add(&mut self, other: &Removed) {
        self.paths += other.paths;
        self.emails += other.emails;
        self.secrets += other.secrets;
        self.names += other.names;
    }
    pub fn total(&self) -> u32 {
        self.paths + self.emails + self.secrets + self.names
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub category: Category,
    /// The source in plain words: "Startup log".
    pub source: String,
    /// How the conversation names it: "the crash record from 8:43 AM".
    pub label: String,
    /// When the newest record in it happened, when known.
    pub observed_at: Option<u64>,
    pub coverage: Coverage,
    pub parser_version: u32,
    pub status: Status,
    /// One line for the tool's answer and the review's card header.
    pub summary: String,
    /// The sanitized excerpt. Empty unless [`Status::has_text`].
    pub text: String,
    /// Plain words on why it is missing or cut, when it is.
    pub note: String,
    pub removed: Removed,
    /// Bytes read from the source to make this item (counted against [`REPORT_INPUT_CAP`]).
    pub input_bytes: u64,
    /// Structured facts a host screen needs (the installation line, the versions section).
    #[serde(default)]
    pub facts: serde_json::Value,
}

impl Item {
    pub fn new(category: Category, source: &str) -> io::Result<Item> {
        Ok(Item {
            id: crate::util::new_id("ev")?,
            category,
            source: source.to_string(),
            label: source.to_lowercase(),
            observed_at: None,
            coverage: Coverage::Unknown,
            parser_version: PARSER_VERSION,
            status: Status::Absent,
            summary: String::new(),
            text: String::new(),
            note: String::new(),
            removed: Removed::default(),
            input_bytes: 0,
            facts: serde_json::Value::Null,
        })
    }

    /// What the support tool answers for this item in a list: no text, only what it is.
    pub fn listing(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id, "source": self.source, "status": self.status, "coverage": self.coverage,
            "observed_at": self.observed_at.map(crate::util::utc_stamp), "summary": self.summary, "note": self.note,
        })
    }

    /// What the support tool answers when asked to read it.
    pub fn reading(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id, "source": self.source, "status": self.status, "coverage": self.coverage,
            "observed_at": self.observed_at.map(crate::util::utc_stamp), "text": self.text, "note": self.note,
        })
    }
}

/// `ev-` and twelve lowercase hex digits, and nothing else.
pub fn is_id(s: &str) -> bool {
    s.len() == 15 && s.starts_with("ev-") && s[3..].bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The evidence of one draft: `<draft>/evidence/<id>.json`.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn at(draft_dir: &Path) -> Store {
        Store { dir: draft_dir.join("evidence") }
    }

    pub fn put(&self, item: &Item) -> io::Result<()> {
        if !is_id(&item.id) {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "not an evidence id"));
        }
        crate::util::write_private(&self.dir.join(format!("{}.json", item.id)), &serde_json::to_vec(item)?)
    }

    /// The item with this ID, or `None`. An ID that is not of the right shape names nothing.
    pub fn get(&self, id: &str) -> Option<Item> {
        if !is_id(id) {
            return None;
        }
        let bytes = std::fs::read(self.dir.join(format!("{id}.json"))).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Every item, oldest file first.
    pub fn all(&self) -> Vec<Item> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return Vec::new() };
        let mut items: Vec<(std::time::SystemTime, Item)> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_str().is_some_and(|n| n.ends_with(".json") && is_id(&n[..n.len() - 5])))
            .filter_map(|e| {
                let when = e.metadata().ok()?.modified().ok()?;
                let item: Item = serde_json::from_slice(&std::fs::read(e.path()).ok()?).ok()?;
                Some((when, item))
            })
            .collect();
        items.sort_by_key(|(w, i)| (*w, i.id.clone()));
        items.into_iter().map(|(_, i)| i).collect()
    }

    /// The input already read for this draft.
    pub fn input_bytes(&self) -> u64 {
        self.all().iter().map(|i| i.input_bytes).sum()
    }

    /// The items of one category, newest collection first replaced: a category collected again
    /// supersedes its earlier items.
    pub fn of(&self, category: Category) -> Vec<Item> {
        self.all().into_iter().filter(|i| i.category == category).collect()
    }

    /// Remove every item of `category` (before collecting it again).
    pub fn clear(&self, category: Category) {
        for item in self.of(category) {
            std::fs::remove_file(self.dir.join(format!("{}.json", item.id))).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_well_formed_ids_name_anything() {
        assert!(is_id("ev-0123456789ab"));
        for bad in ["ev-0123456789AB", "ev-0123456789a", "ev-../../etc/pa", "../ev-0123456789", "ev-0123456789abc", ""] {
            assert!(!is_id(bad), "{bad}");
        }
    }

    #[test]
    fn items_round_trip_and_a_bad_id_reads_nothing() {
        let dir = std::env::temp_dir().join(format!("rd-ev-{}", crate::util::hex(&crate::util::random_bytes(4).unwrap())));
        let store = Store::at(&dir);
        let mut item = Item::new(Category::StartupCrash, "Startup log").unwrap();
        item.status = Status::Collected;
        item.text = "failure: x".into();
        item.input_bytes = 10;
        store.put(&item).unwrap();
        assert_eq!(store.get(&item.id).unwrap(), item);
        assert!(store.get("../draft").is_none());
        assert_eq!(store.input_bytes(), 10);
        store.clear(Category::StartupCrash);
        assert!(store.all().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

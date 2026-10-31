//! Personal file catalog and narrow, durable filing recovery. The host supplies native
//! coordinated file operations. Accepted notes always precede mutation of an original.
mod catalog;
pub mod choice;
pub mod tools;
pub use catalog::{Catalog, Page};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
pub type Result<T> = std::result::Result<T, String>;
pub fn capture_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
pub const EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "tif", "tiff", "webp", "heic", "heif", "avif", "psd", "pdf", "docx",
    "xlsx", "pptx", "odt", "ods", "odp", "svg", "mp3", "flac", "ogg", "m4a", "mp4", "m4v", "mov",
    "zip",
];
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub volume: u64,
    pub object: u64,
    pub size: u64,
    pub modified: String,
    pub changed: String,
    pub bookmark: String,
}
impl Identity {
    pub fn same_object(&self, b: &Self) -> bool {
        self.volume == b.volume && self.object == b.object
    }
    pub fn same_content(&self, b: &Self) -> bool {
        self.same_object(b) && self.size == b.size && self.modified == b.modified
    }
    pub fn unchanged(&self, b: &Self) -> bool {
        self.same_content(b) && self.changed == b.changed
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    Draft,
    Choosing,
    Prepared,
    Replaced,
    Moving,
    Restoring,
    Filed,
    Unresolved,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Projection {
    pub mode: String,
    pub reason: String,
    pub previous: String,
    pub written: String,
}
impl Default for Projection {
    fn default() -> Self {
        Self {
            mode: "local".into(),
            reason: "Not yet written inside the file".into(),
            previous: String::new(),
            written: String::new(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Staged {
    pub path: PathBuf,
    pub identity: Identity,
    pub projection: Projection,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    pub original: PathBuf,
    pub path: PathBuf,
    pub identity: Identity,
    pub note: String,
    pub labels: Vec<String>,
    pub folder: String,
    pub phase: Phase,
    pub projection: Projection,
    pub intended: Option<PathBuf>,
    #[serde(default)]
    pub intended_folder: Option<String>,
    pub staged: Option<Staged>,
    pub undo: Option<PathBuf>,
    pub updated: u64,
    pub problem: Option<String>,
    pub inbox: bool,
    #[serde(default)]
    pub return_phase: Option<Phase>,
    #[serde(default)]
    pub pending_sort: bool,
}
impl Entry {
    pub fn new(path: PathBuf, identity: Identity, note: String) -> Result<Self> {
        if note.trim().is_empty()
            || note.len() > 32 * 1024
            || note
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        {
            return Err("The note is empty or too long".into());
        }
        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            original: path.clone(),
            path,
            identity,
            note,
            labels: vec![],
            folder: String::new(),
            phase: Phase::Draft,
            projection: Projection::default(),
            intended: None,
            intended_folder: None,
            staged: None,
            undo: None,
            updated: crate::util::now_millis(),
            problem: None,
            inbox: false,
            return_phase: None,
            pending_sort: false,
        })
    }
}
pub fn folder_parts(value: &str) -> Result<Vec<String>> {
    if value.is_empty() || value.len() > 512 {
        return Err("Invalid folder name".into());
    }
    let parts: Vec<_> = Path::new(value).components().collect();
    if parts.is_empty()
        || parts.len() > 2
        || parts.iter().any(|p| !matches!(p, Component::Normal(_)))
    {
        return Err("Choose a relative category with at most two folders".into());
    }
    parts
        .into_iter()
        .map(|p| {
            let text = p.as_os_str().to_str().ok_or("Invalid folder text")?;
            if text.starts_with('.')
                || text.trim() != text
                || text
                    .chars()
                    .any(|c| c.is_control() || matches!(c, ':' | '\\'))
                || text.chars().count() > 80
            {
                return Err("Invalid folder component".into());
            }
            Ok(text.into())
        })
        .collect()
}
/// Native operations must coordinate access, stay on one volume and never replace an occupant.
pub trait Files {
    fn inspect(&self, path: &Path) -> Result<Identity>;
    fn resolve(&self, path: &Path, identity: &Identity) -> Result<PathBuf>;
    fn destination(&self, root: &Path, parts: &[String], name: &std::ffi::OsStr)
        -> Result<PathBuf>;
    fn stage(&self, entry: &Entry) -> Result<Option<Staged>>;
    fn annotate(&self, entry: &Entry, staged: &Staged) -> Result<Staged>;
    fn replace(&self, entry: &Entry, staged: &Staged) -> Result<Identity>;
    fn rename(&self, source: &Path, destination: &Path, identity: &Identity) -> Result<Identity>;
    fn cleanup(&self, staged: &Staged) -> Result<()>;
}
pub fn file(
    db: &mut Catalog,
    fs: &impl Files,
    id: &str,
    root: &Path,
    folder: &str,
    labels: Vec<String>,
    inbox: bool,
) -> Result<Entry> {
    let mut e = db.get(id)?;
    if !matches!(e.phase, Phase::Draft | Phase::Choosing | Phase::Filed) {
        return Err("This file has an operation to recover first".into());
    }
    e.path = fs.resolve(&e.path, &e.identity)?;
    if !e.identity.unchanged(&fs.inspect(&e.path)?) {
        return Err("The file changed. Your saved note is still here".into());
    }
    let parts = folder_parts(folder)?;
    let folder_path = parts
        .iter()
        .fold(root.to_path_buf(), |parent, part| parent.join(part));
    let destination = if e.path.parent() == Some(folder_path.as_path()) {
        e.path.clone()
    } else {
        fs.destination(root, &parts, e.path.file_name().ok_or("Missing filename")?)?
    };
    if !e.pending_sort {
        e.undo = Some(e.path.clone());
    }
    e.intended = Some(destination);
    e.intended_folder = Some(folder.into());
    e.return_phase = Some(if e.phase == Phase::Filed {
        Phase::Filed
    } else {
        Phase::Draft
    });
    e.labels = labels;
    e.problem = None;
    e.phase = Phase::Prepared;
    db.save(&e)?;
    {
        match fs.stage(&e) {
            Ok(Some(staged)) => {
                e.staged = Some(staged.clone());
                db.save(&e)?;
                let staged = fs.annotate(&e, &staged)?;
                e.staged = Some(staged.clone());
                db.save(&e)?;
                if staged.projection.mode == "embedded" {
                    e.identity = fs.replace(&e, &staged)?;
                }
                e.projection = staged.projection;
                e.phase = Phase::Replaced;
                db.save(&e)?;
            }
            Ok(None) => {
                e.projection = Projection {
                    reason: "This file type has no supported in-file note field".into(),
                    ..Projection::default()
                };
            }
            Err(reason) => {
                e.phase = e.return_phase.take().unwrap_or(Phase::Draft);
                e.intended = None;
                e.intended_folder = None;
                e.problem = Some(reason.clone());
                db.save(&e)?;
                return Err(reason);
            }
        }
    }
    e.phase = Phase::Moving;
    db.save(&e)?;
    let destination = e.intended.clone().ok_or("Missing filing destination")?;
    if e.path != destination {
        relocate(db, fs, &mut e)?;
    }
    e.path = e.intended.clone().ok_or("Missing filing destination")?;
    e.folder = folder.into();
    e.inbox = inbox;
    e.return_phase = None;
    e.intended_folder = None;
    e.pending_sort = false;
    e.phase = Phase::Filed;
    e.intended = None;
    e.updated = crate::util::now_millis();
    db.save(&e)?;
    let resolved = fs.resolve(&e.path, &e.identity)?;
    if resolved != e.path || !e.identity.same_content(&fs.inspect(&resolved)?) {
        return Err("The moved file needs location recovery".into());
    }
    let mut saved = db.get(&e.id)?;
    if let Some(staged) = saved.staged.take() {
        fs.cleanup(&staged)?;
        db.save(&saved)?;
    }
    Ok(saved)
}
fn relocate(db: &mut Catalog, fs: &impl Files, e: &mut Entry) -> Result<()> {
    for _ in 0..16 {
        let destination = e.intended.as_ref().ok_or("Missing move intent")?;
        match fs.rename(&e.path, destination, &e.identity) {
            Ok(identity) => {
                e.identity = identity;
                return Ok(());
            }
            Err(reason) if reason == "destination-collision" => {
                e.intended = Some(fs.destination(
                    destination.parent().ok_or("Missing destination folder")?,
                    &[],
                    e.path.file_name().ok_or("Missing filename")?,
                )?);
                db.save(e)?;
            }
            Err(reason) => return Err(reason),
        }
    }
    Err("Another app kept taking the destination name. The original is still tracked".into())
}
/// Optional fast-clear mode: durable note and intent, then one no-replace move to Inbox.
/// No metadata rewrite or model answer can delay this first safe, searchable location.
pub fn store_inbox(db: &mut Catalog, fs: &impl Files, id: &str, root: &Path) -> Result<Entry> {
    let mut e = db.get(id)?;
    if e.phase != Phase::Draft {
        return Err("This file is already being handled".into());
    }
    if !e.identity.unchanged(&fs.inspect(&e.path)?) {
        return Err("The selected file changed. Its note is saved".into());
    }
    let destination = fs.destination(
        root,
        &["Inbox".into()],
        e.path.file_name().ok_or("Missing filename")?,
    )?;
    e.undo = Some(e.path.clone());
    e.intended = Some(destination.clone());
    e.pending_sort = true;
    e.phase = Phase::Moving;
    e.intended_folder = Some("Inbox".into());
    db.save(&e)?;
    relocate(db, fs, &mut e)?;
    e.path = e.intended.clone().ok_or("Missing move destination")?;
    e.intended = None;
    e.intended_folder = None;
    e.folder = "Inbox".into();
    e.inbox = true;
    e.phase = Phase::Filed;
    db.save(&e)?;
    fs.resolve(&e.path, &e.identity)?;
    db.get(id)
}
/// Settle only a canceled folder choice, reading the latest row after any Undo/Edit.
/// The host serializes this with actual mutations. Never pass an older Entry snapshot.
pub fn cancel_sort(db: &mut Catalog, id: &str) -> Result<Entry> {
    let mut entry = db.get(id)?;
    if entry.pending_sort {
        entry.pending_sort = false;
        entry.phase = Phase::Filed;
    } else if entry.phase == Phase::Choosing {
        entry.phase = Phase::Draft;
    }
    db.save(&entry)?;
    Ok(entry)
}
pub fn edit(db: &mut Catalog, fs: &impl Files, id: &str, note: String) -> Result<Entry> {
    if note.trim().is_empty()
        || note.len() > 32 * 1024
        || note
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err("The note is empty or too long".into());
    }
    let mut e = db.get(id)?;
    if !matches!(e.phase, Phase::Draft | Phase::Filed) {
        return Err("Recover this file operation before editing its note".into());
    }
    e.note = note;
    e.updated = crate::util::now_millis();
    let embedded = matches!(e.projection.mode.as_str(), "embedded" | "older");
    if embedded {
        e.projection.mode = "older".into();
    }
    db.save(&e)?;
    if !embedded {
        return Ok(e);
    }
    e.path = match fs.resolve(&e.path, &e.identity) {
        Ok(path) => path,
        Err(reason) => {
            e.problem = Some(reason);
            db.save(&e)?;
            return Ok(e);
        }
    };
    e.return_phase = Some(e.phase.clone());
    e.phase = Phase::Prepared;
    db.save(&e)?;
    let update = || -> Result<Staged> {
        let staged = fs
            .stage(&e)?
            .ok_or("There is no safe embedded note field")?;
        Ok(staged)
    };
    let result = update().and_then(|staged| {
        e.staged = Some(staged.clone());
        db.save(&e)?;
        let staged = fs.annotate(&e, &staged)?;
        e.staged = Some(staged.clone());
        db.save(&e)?;
        if staged.projection.mode != "embedded" {
            return Err(staged.projection.reason);
        }
        e.identity = fs.replace(&e, &staged)?;
        e.projection = staged.projection;
        Ok(())
    });
    e.phase = e.return_phase.take().unwrap_or(Phase::Draft);
    if let Err(reason) = result {
        e.problem = Some(format!(
            "Note saved in RichOS. The note inside the file is older: {reason}"
        ));
    }
    db.save(&e)?;
    if let Some(staged) = e.staged.take() {
        fs.cleanup(&staged)?;
        db.save(&e)?;
    }
    Ok(e)
}
pub fn undo(db: &mut Catalog, fs: &impl Files, id: &str) -> Result<Entry> {
    let mut e = db.get(id)?;
    if e.phase != Phase::Filed {
        return Err("This filing has not completed".into());
    }
    let previous = e.undo.clone().ok_or("There is no filing to undo")?;
    e.path = fs.resolve(&e.path, &e.identity)?;
    let now = fs.inspect(&e.path)?;
    if !e.identity.same_content(&now) {
        return Err("The filed object changed. Nothing was moved".into());
    }
    let destination = fs.destination(
        previous.parent().ok_or("Missing prior folder")?,
        &[],
        previous.file_name().ok_or("Missing filename")?,
    )?;
    e.identity = now;
    e.intended = Some(destination.clone());
    e.return_phase = Some(Phase::Filed);
    e.phase = Phase::Restoring;
    db.save(&e)?;
    relocate(db, fs, &mut e)?;
    e.path = e.intended.clone().ok_or("Missing move destination")?;
    e.intended = None;
    e.undo = None;
    e.return_phase = None;
    e.intended_folder = None;
    e.pending_sort = false;
    e.phase = Phase::Draft;
    e.folder.clear();
    e.inbox = false;
    db.save(&e)?;
    Ok(e)
}
pub fn recover(db: &mut Catalog, fs: &impl Files) -> Result<()> {
    for mut e in db.pending()? {
        if matches!(e.phase, Phase::Filed | Phase::Draft) && e.intended.is_none() {
            if let Some(staged) = e.staged.take() {
                fs.cleanup(&staged)?;
                db.save(&e)?;
            }
            continue;
        }

        if let Some(destination) = &e.intended {
            if let Ok(actual) = fs.inspect(destination) {
                let expected = e
                    .staged
                    .as_ref()
                    .map(|s| &s.identity)
                    .unwrap_or(&e.identity);
                if e.identity.same_content(&actual) || expected.same_content(&actual) {
                    e.path = destination.clone();
                    e.identity = actual;
                    if e.phase == Phase::Restoring {
                        e.phase = Phase::Draft;
                        e.undo = None;
                        e.pending_sort = false;
                        e.folder.clear();
                        e.inbox = false;
                    } else {
                        e.phase = Phase::Filed;
                        if let Some(folder) = e.intended_folder.take() {
                            e.inbox = folder == "Inbox";
                            e.folder = folder;
                        }
                        e.return_phase = None;
                    }
                    e.intended = None;
                    e.pending_sort = false;
                    if let Some(staged) = &e.staged {
                        e.projection = staged.projection.clone();
                    }
                    db.save(&e)?;
                    if let Some(staged) = e.staged.take() {
                        fs.cleanup(&staged)?;
                        db.save(&e)?;
                    }
                    continue;
                }
            }
        }
        if let Ok(actual) = fs.inspect(&e.path) {
            let staged = e
                .staged
                .as_ref()
                .filter(|s| s.identity.same_content(&actual));
            if e.identity.same_content(&actual) || staged.is_some() {
                if let Some(staged) = staged {
                    e.projection = staged.projection.clone();
                }
                e.identity = actual;
                e.phase = e.return_phase.take().unwrap_or(Phase::Draft);
                e.intended = None;
                e.intended_folder = None;
                e.problem = Some("Your note is saved. The file was not filed".into());
                if e.pending_sort {
                    e.phase = Phase::Filed;
                    e.folder = "Inbox".into();
                    e.inbox = true;
                    e.pending_sort = false;
                    e.problem = Some(
                        "Sorting was interrupted. Your file is in Inbox; use File again".into(),
                    );
                }
                db.save(&e)?;
                if let Some(staged) = e.staged.take() {
                    fs.cleanup(&staged)?;
                    db.save(&e)?;
                }
                continue;
            }
        }
        e.phase = Phase::Unresolved;
        e.problem = Some("Your note is saved. Locate the original before trying again".into());
        db.save(&e)?;
    }
    for mut e in db.interrupted_inbox()? {
        e.pending_sort = false;
        e.problem=Some("Sorting was interrupted. Your file is stored in Inbox; use File again to finish sorting".into());
        db.save(&e)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(std::path::PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("richcleaner-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir(&root).unwrap();
            Self(root)
        }
        fn db(&self) -> Catalog {
            Catalog::open(&self.0.join("library.sqlite")).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    fn entry(name: &str, note: &str, object: u64) -> Entry {
        Entry::new(
            PathBuf::from(name),
            Identity {
                volume: 1,
                object,
                size: 12,
                modified: "1".into(),
                changed: "1".into(),
                bookmark: "bookmark".into(),
            },
            note.into(),
        )
        .unwrap()
    }
    #[test]
    fn accepted_note_survives_restart_and_query_text_is_literal() {
        let f = Fixture::new();
        let mut db = f.db();
        let e = entry("O'Brien Q3-budget.png", "cash or card contract", 7);
        db.save(&e).unwrap();
        drop(db);
        let db = f.db();
        assert_eq!(db.get(&e.id).unwrap().note, e.note);
        for q in ["O'Brien", "Q3-budget", "cash or card", "\" OR \"", ""] {
            let page = db.search(q, 0, 65536).unwrap();
            assert!(page.entries.len() <= 1, "{q}");
        }
        assert_eq!(db.search("cash or card", 0, 65536).unwrap().total, 1);
        assert_eq!(db.search("nonexistent OR", 0, 65536).unwrap().total, 0);
    }
    #[test]
    fn duplicate_identity_is_refused_and_search_changes_with_saved_note() {
        let f = Fixture::new();
        let mut db = f.db();
        let mut a = entry("shot.png", "kitchen quote", 2);
        db.save(&a).unwrap();
        assert!(db
            .save(&entry("different.png", "different note", 2))
            .is_err());
        a.note = "garden wall".into();
        db.save(&a).unwrap();
        assert_eq!(db.search("kitchen", 0, 65536).unwrap().total, 0);
        assert_eq!(db.search("garden", 0, 65536).unwrap().total, 1);
        db.remove(&a.id).unwrap();
        assert_eq!(db.search("", 0, 65536).unwrap().total, 0);
    }
    #[test]
    fn corruption_is_not_replaced_and_newer_schema_is_not_downgraded() {
        let f = Fixture::new();
        let path = f.0.join("broken.sqlite");
        let bytes = b"not a sqlite database";
        std::fs::write(&path, bytes).unwrap();
        assert!(Catalog::open(&path).is_err());
        assert_eq!(std::fs::read(path).unwrap(), bytes);
        let db = f.db();
        db.db_for_test_version(99);
        drop(db);
        assert!(Catalog::open(&f.0.join("library.sqlite")).is_err());
    }
    #[test]
    fn folder_output_cannot_escape_or_create_sentence_trees() {
        for value in [
            "/etc",
            "../Elsewhere",
            "Home/../../escape",
            "Home/a/b",
            ".hidden",
            "Home/ bad",
            "Home/line\nfeed",
            "Home\\escape",
            "",
        ] {
            assert!(folder_parts(value).is_err(), "{value}");
        }
        assert_eq!(folder_parts("Home/Kitchen").unwrap(), ["Home", "Kitchen"]);
    }
    struct Moved {
        path: PathBuf,
        identity: Identity,
    }
    impl Files for Moved {
        fn inspect(&self, path: &Path) -> Result<Identity> {
            if path == self.path {
                Ok(self.identity.clone())
            } else {
                Err("Missing".into())
            }
        }
        fn resolve(&self, path: &Path, _: &Identity) -> Result<PathBuf> {
            Ok(path.into())
        }
        fn destination(&self, _: &Path, _: &[String], _: &std::ffi::OsStr) -> Result<PathBuf> {
            unreachable!()
        }
        fn stage(&self, _: &Entry) -> Result<Option<Staged>> {
            panic!("Recovery must not embed the note again")
        }
        fn annotate(&self, _: &Entry, _: &Staged) -> Result<Staged> {
            unreachable!()
        }
        fn replace(&self, _: &Entry, _: &Staged) -> Result<Identity> {
            unreachable!()
        }
        fn rename(&self, _: &Path, _: &Path, _: &Identity) -> Result<Identity> {
            panic!("Recovery must not repeat a move")
        }
        fn cleanup(&self, _: &Staged) -> Result<()> {
            Ok(())
        }
    }
    #[test]
    fn restart_after_rename_recovers_location_without_rewriting_or_refiling() {
        let f = Fixture::new();
        let mut db = f.db();
        let mut e = entry("/Desktop/shot.png", "compare the quote", 3);
        e.intended = Some(PathBuf::from("/Library/Home/shot.png"));
        e.undo = Some(e.path.clone());
        e.phase = Phase::Moving;
        db.save(&e).unwrap();
        drop(db);
        let mut db = f.db();
        let fs = Moved {
            path: e.intended.clone().unwrap(),
            identity: e.identity.clone(),
        };
        recover(&mut db, &fs).unwrap();
        let saved = db.get(&e.id).unwrap();
        assert_eq!(saved.path, fs.path);
        assert_eq!(saved.note, e.note);
        assert_eq!(saved.phase, Phase::Filed);
        assert_eq!(saved.undo, e.undo);
        recover(&mut db, &fs).unwrap();
    }
    #[test]
    fn restart_cannot_substitute_an_occupant_and_undo_recovery_keeps_note() {
        let f = Fixture::new();
        let mut db = f.db();
        let mut e = entry("/Library/shot.png", "keep this note", 3);
        e.intended = Some(PathBuf::from("/Desktop/shot.png"));
        e.return_phase = Some(Phase::Filed);
        e.phase = Phase::Restoring;
        db.save(&e).unwrap();
        let mut occupant = e.identity.clone();
        occupant.object = 9;
        recover(
            &mut db,
            &Moved {
                path: e.intended.clone().unwrap(),
                identity: occupant,
            },
        )
        .unwrap();
        assert_eq!(db.get(&e.id).unwrap().phase, Phase::Unresolved);
        e.return_phase = Some(Phase::Filed);
        e.phase = Phase::Restoring;
        db.save(&e).unwrap();
        recover(
            &mut db,
            &Moved {
                path: e.intended.clone().unwrap(),
                identity: e.identity.clone(),
            },
        )
        .unwrap();
        let saved = db.get(&e.id).unwrap();
        assert_eq!(saved.phase, Phase::Draft);
        assert_eq!(saved.note, e.note);
        assert!(saved.undo.is_none());
    }
    struct DiskFiles {
        db: PathBuf,
        id: String,
        interrupt: bool,
    }
    impl Files for DiskFiles {
        fn inspect(&self, p: &Path) -> Result<Identity> {
            use std::os::unix::fs::MetadataExt;
            let m = std::fs::symlink_metadata(p).map_err(|e| e.to_string())?;
            Ok(Identity {
                volume: m.dev(),
                object: m.ino(),
                size: m.len(),
                modified: format!("{}:{}", m.mtime(), m.mtime_nsec()),
                changed: format!("{}:{}", m.ctime(), m.ctime_nsec()),
                bookmark: String::new(),
            })
        }
        fn resolve(&self, p: &Path, i: &Identity) -> Result<PathBuf> {
            if self.inspect(p)?.same_object(i) {
                Ok(p.into())
            } else {
                Err("The original is missing".into())
            }
        }
        fn destination(
            &self,
            root: &Path,
            parts: &[String],
            name: &std::ffi::OsStr,
        ) -> Result<PathBuf> {
            let dir = parts.iter().fold(root.to_path_buf(), |p, n| p.join(n));
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let mut result = dir.join(name);
            if result.exists() {
                result = dir.join(format!("restored-{}", name.to_string_lossy()));
            }
            Ok(result)
        }
        fn stage(&self, _: &Entry) -> Result<Option<Staged>> {
            Ok(None)
        }
        fn annotate(&self, _: &Entry, _: &Staged) -> Result<Staged> {
            unreachable!()
        }
        fn replace(&self, _: &Entry, _: &Staged) -> Result<Identity> {
            unreachable!()
        }
        fn rename(&self, a: &Path, b: &Path, i: &Identity) -> Result<Identity> {
            let saved = Catalog::open(&self.db)?.get(&self.id)?;
            assert!(!saved.note.is_empty(), "the note must already be durable");
            assert_eq!(
                saved.intended.as_deref(),
                Some(b),
                "the destination must already be durable"
            );
            if b.exists() || !self.inspect(a)?.unchanged(i) {
                return Err("Refused to replace an occupant or changed source".into());
            }
            std::fs::rename(a, b).map_err(|e| e.to_string())?;
            if self.interrupt {
                return Err("Controlled interruption after rename".into());
            }
            self.inspect(b)
        }
        fn cleanup(&self, _: &Staged) -> Result<()> {
            Ok(())
        }
    }
    #[test]
    fn inbox_first_is_searchable_then_sorted_and_undo_still_targets_desktop() {
        let f = Fixture::new();
        let desktop = f.0.join("Desktop");
        let root = f.0.join("Library");
        std::fs::create_dir(&desktop).unwrap();
        std::fs::create_dir(&root).unwrap();
        let source = desktop.join("quote.txt");
        std::fs::write(&source, b"one original").unwrap();
        let mut db = f.db();
        let mut fs = DiskFiles {
            db: f.0.join("library.sqlite"),
            id: String::new(),
            interrupt: false,
        };
        let e = Entry::new(
            source.clone(),
            fs.inspect(&source).unwrap(),
            "kitchen quote".into(),
        )
        .unwrap();
        fs.id = e.id.clone();
        db.save(&e).unwrap();
        let stored = store_inbox(&mut db, &fs, &e.id, &root).unwrap();
        assert!(!source.exists());
        assert_eq!(std::fs::read(&stored.path).unwrap(), b"one original");
        assert!(stored.pending_sort);
        assert_eq!(db.search("kitchen", 0, 65536).unwrap().total, 1);
        let filed = file(&mut db, &fs, &e.id, &root, "Home/Kitchen", vec![], false).unwrap();
        assert_eq!(filed.undo.as_ref(), Some(&source));
        assert!(!stored.path.exists());
        assert!(filed.path.starts_with(root.join("Home/Kitchen")));
        assert!(!filed.pending_sort);
        drop(db);
        let mut db = f.db();
        std::fs::write(&source, b"new file").unwrap();
        let restored = undo(&mut db, &fs, &e.id).unwrap();
        assert_eq!(std::fs::read(&source).unwrap(), b"new file");
        assert_eq!(std::fs::read(&restored.path).unwrap(), b"one original");
        assert_eq!(restored.note, e.note);
        // A late canceled choice must reload the row after Undo and a correction,
        // rather than restore the older captured note/location snapshot.
        edit(&mut db, &fs, &e.id, "The corrected estimate".into()).unwrap();
        let settled = cancel_sort(&mut db, &e.id).unwrap();
        assert_eq!(settled.phase, Phase::Draft);
        assert_eq!(settled.path, restored.path);
        assert_eq!(settled.note, "The corrected estimate");
        let pending = store_inbox(&mut db, &fs, &e.id, &root).unwrap();
        edit(
            &mut db,
            &fs,
            &e.id,
            "A correction while stored in Inbox".into(),
        )
        .unwrap();
        let settled = cancel_sort(&mut db, &e.id).unwrap();
        assert_eq!(settled.phase, Phase::Filed);
        assert!(!settled.pending_sort);
        assert_eq!(settled.path, pending.path);
        assert_eq!(settled.undo, Some(restored.path));
        assert_eq!(settled.note, "A correction while stored in Inbox");
    }
    #[test]
    fn interrupted_inbox_move_recovers_its_original_and_settles_sorting_honestly() {
        let f = Fixture::new();
        let source = f.0.join("quote.txt");
        std::fs::write(&source, b"one original").unwrap();
        let root = f.0.join("Library");
        std::fs::create_dir(&root).unwrap();
        let mut db = f.db();
        let mut fs = DiskFiles {
            db: f.0.join("library.sqlite"),
            id: String::new(),
            interrupt: true,
        };
        let e = Entry::new(
            source.clone(),
            fs.inspect(&source).unwrap(),
            "saved note".into(),
        )
        .unwrap();
        fs.id = e.id.clone();
        db.save(&e).unwrap();
        assert!(store_inbox(&mut db, &fs, &e.id, &root).is_err());
        drop(db);
        let mut db = f.db();
        recover(&mut db, &fs).unwrap();
        let saved = db.get(&e.id).unwrap();
        assert_eq!(saved.phase, Phase::Filed);
        assert!(saved.path.starts_with(root.join("Inbox")));
        assert_eq!(saved.note, e.note);
        assert_eq!(std::fs::read(saved.path).unwrap(), b"one original");
        assert!(!saved.pending_sort);
    }
    #[test]
    fn library_cards_reference_the_catalog_and_disappear_without_deleting_originals() {
        let f = Fixture::new();
        let mut db = f.db();
        let e = entry("quote.png", "kitchen quote", 5);
        db.save(&e).unwrap();
        let first = db.show("acme", "chat", "turn", &e.id).unwrap();
        assert_eq!(first, db.show("acme", "chat", "turn", &e.id).unwrap());
        assert_eq!(db.cards().unwrap().len(), 1);
        assert_eq!(db.cards().unwrap()[0].file, e.id);
        db.remove(&e.id).unwrap();
        assert!(db.cards().unwrap().is_empty());
    }
}

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
pub const MAX_BATCH_FILES: usize = 20;
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<String>,
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
        validate_note(&note)?;
        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            batch_id: None,
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

    pub fn check_batch_eligibility(&self) -> Result<()> {
        if !matches!(self.phase, Phase::Draft | Phase::Filed)
            || self.staged.is_some()
            || self.intended.is_some()
            || self.intended_folder.is_some()
            || self.return_phase.is_some()
            || self.pending_sort
        {
            return Err("A selected file has unfinished RichCleaner work. Open Recent files to finish or recover it, then try again".into());
        }
        Ok(())
    }
}

fn validate_note(note: &str) -> Result<()> {
    if note.trim().is_empty()
        || note.len() > 32 * 1024
        || note
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err("The note is empty or too long".into());
    }
    Ok(())
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
    fn resolve_local(&self, path: &Path, identity: &Identity) -> Result<(PathBuf, Identity)> {
        let resolved = self.resolve(path, identity)?;
        let actual = self.inspect(&resolved)?;
        Ok((resolved, actual))
    }
    fn destination(&self, root: &Path, parts: &[String], name: &std::ffi::OsStr)
        -> Result<PathBuf>;
    fn destination_existing(
        &self,
        root: &Path,
        parts: &[String],
        name: &std::ffi::OsStr,
    ) -> Result<PathBuf> {
        let mut folder = root.to_path_buf();
        for part in parts {
            folder.push(part);
            if !std::fs::symlink_metadata(&folder).is_ok_and(|meta| meta.is_dir()) {
                return Err(DESTINATION_UNAVAILABLE.into());
            }
        }
        self.destination(&folder, &[], name)
    }
    fn stage(&self, entry: &Entry) -> Result<Option<Staged>>;
    fn annotate(&self, entry: &Entry, staged: &Staged) -> Result<Staged>;
    fn replace(&self, entry: &Entry, staged: &Staged) -> Result<Identity>;
    fn rename(&self, source: &Path, destination: &Path, identity: &Identity) -> Result<Identity>;
    fn cleanup(&self, staged: &Staged) -> Result<()>;
}
pub const DESTINATION_UNAVAILABLE: &str = "destination-folder-unavailable";

pub enum FilingChoice {
    Category {
        folder: String,
        labels: Vec<String>,
        existing_only: bool,
    },
    Inbox {
        reason: String,
    },
}

impl FilingChoice {
    pub fn category(folder: impl Into<String>, labels: Vec<String>, existing_only: bool) -> Self {
        Self::Category { folder: folder.into(), labels, existing_only }
    }

    pub fn folder(&self) -> &str {
        match self {
            Self::Category { folder, .. } => folder,
            Self::Inbox { .. } => "Inbox",
        }
    }
}

pub fn file(
    db: &mut Catalog,
    fs: &impl Files,
    id: &str,
    root: &Path,
    choice: &FilingChoice,
) -> Result<Entry> {
    let mut e = db.get(id)?;
    if !matches!(e.phase, Phase::Draft | Phase::Choosing | Phase::Filed) {
        return Err("This file has an operation to recover first".into());
    }
    let (path, actual) = fs.resolve_local(&e.path, &e.identity)?;
    if !e.identity.unchanged(&actual) {
        return Err("The file changed. Your saved note is still here".into());
    }
    e.path = path;
    let name = e.path.file_name().ok_or("Missing filename")?;
    let destination_for = |folder: &str, existing_only: bool| -> Result<PathBuf> {
        let parts = folder_parts(folder)?;
        let folder_path = parts.iter().fold(root.to_path_buf(), |parent, part| parent.join(part));
        if e.path.parent() == Some(folder_path.as_path()) {
            Ok(e.path.clone())
        } else if existing_only {
            fs.destination_existing(root, &parts, name)
        } else {
            fs.destination(root, &parts, name)
        }
    };
    let (folder, labels, reason, destination) = match choice {
        FilingChoice::Category { folder, labels, existing_only } => {
            match destination_for(folder, *existing_only) {
                Ok(path) => (folder.as_str(), labels.clone(), None, path),
                Err(error) if *existing_only && error == DESTINATION_UNAVAILABLE => (
                    "Inbox", vec![],
                    Some("The chosen folder is no longer available. Use File again to choose a folder".into()),
                    destination_for("Inbox", false)?,
                ),
                Err(error) => return Err(error),
            }
        }
        FilingChoice::Inbox { reason } => (
            "Inbox", vec![], Some(reason.clone()), destination_for("Inbox", false)?,
        ),
    };
    if !e.pending_sort && (!e.inbox || e.undo.is_none()) {
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
    // Keep the explanation in the same durable intent as the Inbox destination.
    e.problem = reason;
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
    e.inbox = folder == "Inbox";
    e.return_phase = None;
    e.intended_folder = None;
    e.pending_sort = false;
    e.phase = Phase::Filed;
    e.intended = None;
    e.updated = crate::util::now_millis();
    db.save(&e)?;
    let (resolved, actual) = fs.resolve_local(&e.path, &e.identity)?;
    if resolved != e.path || !e.identity.same_content(&actual) {
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
    validate_note(&note)?;
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
    if let Err(reason) = result {
        // A native replacement can finish before its process reports an error.
        // Keep the persisted staged identity until recovery reconciles that gap.
        let pending = db.get(id)?;
        recover_entries(db, fs, vec![pending])?;
        let mut recovered = db.get(id)?;
        recovered.problem = Some(if recovered.projection.mode == "embedded" {
            "Your note is saved inside the file. Its folder is unchanged".into()
        } else {
            format!("Note saved in RichOS. The note inside the file is older: {reason}")
        });
        db.save(&recovered)?;
        return Ok(recovered);
    }
    e.phase = e.return_phase.take().unwrap_or(Phase::Draft);
    e.problem = None;
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
fn recover_entries(
    db: &mut Catalog,
    fs: &impl Files,
    entries: impl IntoIterator<Item = Entry>,
) -> Result<()> {
    for mut e in entries {
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
                let failed_inbox_store = e.pending_sort
                    && !e.inbox
                    && e.phase == Phase::Moving
                    && e.return_phase.is_none();
                e.identity = actual;
                e.phase = e.return_phase.take().unwrap_or(Phase::Draft);
                e.intended = None;
                e.intended_folder = None;
                e.problem = Some("Your note is saved. The file was not filed".into());
                if e.pending_sort {
                    e.pending_sort = false;
                    if e.inbox {
                        e.phase = Phase::Filed;
                        e.problem = Some(
                            "Sorting was interrupted. Your file is in Inbox; use File again".into(),
                        );
                    } else if failed_inbox_store {
                        e.undo = None;
                    }
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
    Ok(())
}
pub fn recover(db: &mut Catalog, fs: &impl Files) -> Result<()> {
    let pending = db.pending()?;
    recover_entries(db, fs, pending)?;
    for e in db.interrupted_inbox()? {
        settle_interrupted_inbox(db, e)?;
    }
    Ok(())
}

/// Reconcile a failed mutation without settling other active files in its batch.
pub fn recover_entry(db: &mut Catalog, fs: &impl Files, id: &str) -> Result<()> {
    let mut e = db.get(id)?;
    if !matches!(e.phase, Phase::Filed | Phase::Draft) || e.staged.is_some() {
        recover_entries(db, fs, [e])?;
        e = db.get(id)?;
    }
    if e.phase == Phase::Filed && e.pending_sort {
        settle_interrupted_inbox(db, e)?;
    }
    Ok(())
}

fn settle_interrupted_inbox(db: &mut Catalog, mut e: Entry) -> Result<()> {
    e.pending_sort = false;
    e.problem = Some(if e.inbox {
        "Sorting was interrupted. Your file is stored in Inbox; use File again to finish sorting"
    } else {
        "Sorting was interrupted. Your file is at its saved location; use File again to sort it"
    }.into());
    db.save(&e)
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
    fn batch_note_and_search_rows_roll_back_on_duplicate_unsettled_or_oversized_member() {
        let f = Fixture::new();
        let mut db = f.db();
        let mut prior = entry("/Library/prior.png", "keep prior note", 1);
        prior.phase = Phase::Filed;
        db.save(&prior).unwrap();
        let long = entry("long.png", &"A".repeat(32 * 1024), 2);
        db.save(&long).unwrap();
        let mut pending = entry("moving.png", "keep the moving note", 3);
        pending.phase = Phase::Moving;
        db.save(&pending).unwrap();
        let watcher = Catalog::watch_filed_context(&f.0.join("library.sqlite"));
        let selected = (PathBuf::from("/Desktop/prior.png"), prior.identity.clone());
        let fresh = entry("/Desktop/model.png", "unused fixture note", 4);
        let new_file = (fresh.path, fresh.identity);

        for rejected in [selected.clone(), (long.path, long.identity), (pending.path, pending.identity)] {
            assert!(db
                .enroll_batch(
                    &capture_id(),
                    vec![selected.clone(), new_file.clone(), rejected],
                    "acquisition review",
                )
                .is_err());
            assert_eq!(db.search("acquisition", 0, 65536).unwrap().total, 0);
            assert!(db.by_identity(1, 4).unwrap().is_none());
            let unchanged = db.get(&prior.id).unwrap();
            assert_eq!(unchanged.note, prior.note);
            assert_eq!(unchanged.path, prior.path);
            assert_eq!(unchanged.phase, Phase::Filed);
            assert!(watcher.try_recv().is_err());
        }

        let batch_id = capture_id();
        let files = vec![selected, new_file];
        let enrolled = db
            .enroll_batch(&batch_id, files.clone(), "acquisition review")
            .unwrap();
        assert!(db.enroll_batch(&batch_id, files, "acquisition review").is_err());
        assert_eq!(watcher.try_recv().unwrap(), [prior.path]);
        drop(db);
        let db = f.db();
        assert_eq!(db.search("acquisition", 0, 65536).unwrap().total, 2);
        assert_eq!(enrolled[0].id, prior.id);
        assert_eq!(enrolled[0].note, "keep prior note\n\nacquisition review");
        assert_eq!(enrolled[1].note, "acquisition review");
        for e in enrolled {
            let saved = db.get(&e.id).unwrap();
            assert_eq!(saved.batch_id.as_deref(), Some(batch_id.as_str()));
            assert_eq!(saved.note, e.note);
            assert_eq!(saved.phase, Phase::Draft);
        }
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
    fn v2_catalogue_keeps_existing_notes_search_rows_and_cards() {
        let f = Fixture::new();
        let mut db = f.db();
        let prior = entry("ordinary.txt", "existing note", 2);
        db.save(&prior).unwrap();
        let card = db.show("company", "chat", "turn", &prior.id).unwrap();
        drop(db);
        rusqlite::Connection::open(f.0.join("library.sqlite"))
            .unwrap()
            .execute_batch(
                "DROP TABLE cleaner_batch_members;
                 DROP TABLE cleaner_batches;
                 PRAGMA user_version=2;",
            )
            .unwrap();

        let db = f.db();
        let saved = db.get(&prior.id).unwrap();
        assert_eq!(saved.note, "existing note");
        assert_eq!(saved.batch_id, None);
        assert_eq!(db.search("existing", 0, 65536).unwrap().total, 1);
        assert_eq!(db.cards().unwrap()[0].id, card);
    }
    #[test]
    fn v3_catalogue_recovers_latest_batch_memberships_without_inventing_a_capture_time() {
        let f = Fixture::new();
        let mut db = f.db();
        let batch_id = capture_id();
        let mut first = entry("slides.pdf", "Acquisition review", 1);
        first.batch_id = Some(batch_id.clone());
        db.save(&first).unwrap();
        let mut second = entry("model.xlsx", "Acquisition review", 2);
        second.batch_id = Some(batch_id.clone());
        db.save(&second).unwrap();
        let card = db.show("company", "chat", "turn", &first.id).unwrap();
        drop(db);
        let path = f.0.join("library.sqlite");
        rusqlite::Connection::open(&path)
            .unwrap()
            .execute_batch(
                "DROP TABLE cleaner_batch_members;
                 DROP TABLE cleaner_batches;
                 CREATE INDEX files_batch ON files(json_extract(record,'$.batchId'))
                 WHERE json_extract(record,'$.batchId') IS NOT NULL;
                 PRAGMA user_version=3;",
            )
            .unwrap();

        let mut db = f.db();
        let (batches, total) = db.batch_page(&first.id, 0).unwrap();
        assert_eq!(total, 1);
        assert_eq!(batches[0].id, batch_id);
        assert_eq!(batches[0].note.as_deref(), Some("Acquisition review"));
        assert_eq!(batches[0].created_at, None);
        assert_eq!(batches[0].file_count, 2);
        assert_eq!(db.cards().unwrap()[0].id, card);
        assert_eq!(db.search("Acquisition", 0, 65536).unwrap().total, 2);
        let shown = db.show_batch("company", "chat", "turn", &first.id, None).unwrap();
        assert!(shown.contains(&(first.id.clone(), card)));
        assert!(shown.iter().any(|(id, _)| id == &second.id));

        db.remove(&first.id).unwrap();
        assert_eq!(db.batch_page(&second.id, 0).unwrap().0[0].file_count, 1);
        db.remove(&second.id).unwrap();
        let headers: u64 = rusqlite::Connection::open(&path)
            .unwrap()
            .query_row("SELECT count(*) FROM cleaner_batches", [], |row| row.get(0))
            .unwrap();
        assert_eq!(headers, 0);
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
    fn search_refreshes_both_locations_when_a_file_move_settles() {
        let fixture = Fixture::new();
        let mut db = fixture.db();
        let mut file = entry("/Desktop/shot.png", "search note", 3);
        file.phase = Phase::Filed;
        db.save(&file).unwrap();
        let updates = Catalog::watch_filed_context(&fixture.0.join("library.sqlite"));
        for (moving, settled, destination) in [
            (Phase::Moving, Phase::Filed, "/Library/shot.png"),
            (Phase::Restoring, Phase::Draft, "/Desktop/shot.png"),
        ] {
            file.phase = moving;
            db.save(&file).unwrap();
            let original = file.path.clone();
            assert_eq!(updates.try_recv().unwrap().as_slice(), std::slice::from_ref(&original));

            file.phase = settled;
            file.path = destination.into();
            db.save(&file).unwrap();
            assert_eq!(updates.try_recv().unwrap(), [original, file.path.clone()]);
            assert!(updates.try_recv().is_err());
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
    fn an_undone_original_can_join_a_batch_and_keep_its_embedded_note_history() {
        let f = Fixture::new();
        let mut db = f.db();
        let desktop = f.0.join("Desktop");
        let folder = f.0.join("Library");
        std::fs::create_dir(&desktop).unwrap();
        let source = desktop.join("board.pdf");
        let other = desktop.join("model.xlsx");
        std::fs::write(&source, "board document").unwrap();
        std::fs::write(&other, "financial model").unwrap();
        let mut fs = DiskFiles {
            db: f.0.join("library.sqlite"),
            id: String::new(),
            interrupt: false,
        };
        let old = Entry::new(
            source.clone(),
            fs.inspect(&source).unwrap(),
            "Board's initial notes".into(),
        )
        .unwrap();
        fs.id = old.id.clone();
        db.save(&old).unwrap();
        let choice = FilingChoice::category("Board", vec![], false);
        let mut filed = file(&mut db, &fs, &old.id, &folder, &choice).unwrap();
        filed.projection = Projection {
            mode: "embedded".into(),
            reason: String::new(),
            previous: "Existing metadata".into(),
            written: "Existing metadata\n\nBoard's initial notes".into(),
        };
        db.save(&filed).unwrap();
        std::fs::write(&source, "new file at the original path").unwrap();
        let restored = undo(&mut db, &fs, &old.id).unwrap();
        assert_ne!(restored.path, source);
        let files = vec![
            (restored.path.clone(), fs.inspect(&restored.path).unwrap()),
            (other.clone(), fs.inspect(&other).unwrap()),
        ];
        let first = db
            .enroll_batch(&capture_id(), files.clone(), "Friday's acquisition review")
            .unwrap();
        let latest_batch = capture_id();
        let repeated = db
            .enroll_batch(&latest_batch, files, "Friday's acquisition review")
            .unwrap();
        let now = &repeated[0];
        assert_eq!(now.id, old.id);
        assert_eq!(now.original, source);
        assert_eq!(now.path, restored.path);
        assert_eq!(now.identity, fs.inspect(&restored.path).unwrap());
        assert_eq!(now.phase, Phase::Draft);
        assert_eq!(now.batch_id.as_deref(), Some(latest_batch.as_str()));
        assert!(now.undo.is_none());
        assert_eq!(now.note, "Board's initial notes\n\nFriday's acquisition review");
        assert_eq!(now.note, first[0].note);
        assert_eq!(now.projection.mode, "older");
        assert_eq!(now.projection.previous, filed.projection.previous);
        assert_eq!(now.projection.written, filed.projection.written);
        assert_eq!(repeated[1].id, first[1].id);
        assert_eq!(repeated[1].note, "Friday's acquisition review");
        assert_eq!(std::fs::read_to_string(&restored.path).unwrap(), "board document");
        assert_eq!(std::fs::read_to_string(&source).unwrap(), "new file at the original path");
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
        let choice = FilingChoice::category("Home/Kitchen", vec![], false);
        let filed = file(&mut db, &fs, &e.id, &root, &choice).unwrap();
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
        let before_retry = settled.undo.clone();
        drop(db);
        let mut db = f.db();
        let retried = file(&mut db, &fs, &e.id, &root, &choice).unwrap();
        assert_eq!(retried.undo, before_retry);
        assert!(retried.path.starts_with(root.join("Home/Kitchen")));
        let undone = undo(&mut db, &fs, &e.id).unwrap();
        assert_eq!(Some(undone.path), before_retry);
    }
    #[test]
    fn inbox_reason_survives_quiet_sort_and_recovery_of_a_missing_category() {
        let f = Fixture::new();
        let root = f.0.join("Library");
        std::fs::create_dir(&root).unwrap();
        let mut db = f.db();
        for quiet in [true, false] {
            let source = f.0.join(if quiet { "quiet.txt" } else { "direct.txt" });
            std::fs::write(&source, b"the original").unwrap();
            let mut fs = DiskFiles {
                db: f.0.join("library.sqlite"),
                id: String::new(),
                interrupt: false,
            };
            let entry = Entry::new(source.clone(), fs.inspect(&source).unwrap(), "Board quote".into()).unwrap();
            fs.id = entry.id.clone();
            db.save(&entry).unwrap();
            if quiet {
                store_inbox(&mut db, &fs, &entry.id, &root).unwrap();
                let reason = "The folder model did not answer in time. Use File again to retry";
                file(&mut db, &fs, &entry.id, &root, &FilingChoice::Inbox { reason: reason.into() }).unwrap();
                assert_eq!(f.db().get(&entry.id).unwrap().problem.as_deref(), Some(reason));
            } else {
                fs.interrupt = true;
                let choice = FilingChoice::category("Company/Board", vec![], true);
                assert!(file(&mut db, &fs, &entry.id, &root, &choice).is_err());
                drop(db);
                db = f.db();
                recover_entry(&mut db, &fs, &entry.id).unwrap();
                assert!(db.get(&entry.id).unwrap().problem.unwrap().contains("no longer available"));
                assert!(!root.join("Company").exists(), "a vanished listed category was recreated");
            }
            let settled = f.db().get(&entry.id).unwrap();
            assert_eq!(settled.phase, Phase::Filed);
            assert!(settled.inbox && !settled.pending_sort);
            assert_eq!(settled.undo.as_ref(), Some(&source));
            assert_eq!(std::fs::read(settled.path).unwrap(), b"the original");
        }
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
    fn recovering_one_batch_file_keeps_its_sibling_pending_sort() {
        let f = Fixture::new();
        let mut db = f.db();
        let files = [
            entry("/Desktop/first.txt", "shared note", 1),
            entry("/Desktop/second.txt", "shared note", 2),
        ]
        .into_iter()
        .map(|e| (e.path, e.identity))
        .collect();
        let mut entries = db.enroll_batch(&capture_id(), files, "shared note").unwrap();
        for e in &mut entries {
            e.undo = Some(e.path.clone());
            e.path = Path::new("/Library/Inbox").join(e.path.file_name().unwrap());
            e.folder = "Inbox".into();
            e.phase = Phase::Filed;
            e.inbox = true;
            e.pending_sort = true;
            db.save(e).unwrap();
        }
        let fs = Moved {
            path: entries[0].path.clone(),
            identity: entries[0].identity.clone(),
        };
        recover_entry(&mut db, &fs, &entries[0].id).unwrap();
        let recovered = db.get(&entries[0].id).unwrap();
        assert!(!recovered.pending_sort);
        assert_eq!(recovered.path, entries[0].path);
        assert_eq!(recovered.undo, entries[0].undo);
        assert!(recovered.problem.is_some());
        assert!(db.get(&entries[1].id).unwrap().pending_sort);
    }
    #[test]
    fn failed_initial_inbox_move_leaves_a_draft_at_the_source() {
        let f = Fixture::new();
        let mut db = f.db();
        let mut saved = entry("/Desktop/first.txt", "shared note", 1);
        saved.undo = Some(saved.path.clone());
        saved.intended = Some(PathBuf::from("/Library/Inbox/first.txt"));
        saved.intended_folder = Some("Inbox".into());
        saved.phase = Phase::Moving;
        saved.pending_sort = true;
        db.save(&saved).unwrap();
        let fs = Moved { path: saved.path.clone(), identity: saved.identity.clone() };
        recover_entry(&mut db, &fs, &saved.id).unwrap();
        let recovered = db.get(&saved.id).unwrap();
        assert_eq!(recovered.path, saved.path);
        assert_eq!(recovered.phase, Phase::Draft);
        assert_eq!(recovered.note, saved.note);
        assert!(!recovered.inbox);
        assert!(!recovered.pending_sort);
        assert!(recovered.undo.is_none());
    }
    #[test]
    fn failed_undo_keeps_the_desktop_restore_path_after_an_external_move() {
        let f = Fixture::new();
        let mut db = f.db();
        let mut saved = entry("/Library/Board/first.txt", "shared note", 1);
        let desktop = PathBuf::from("/Desktop/first.txt");
        saved.folder = "Board".into();
        saved.undo = Some(desktop.clone());
        saved.intended = Some(desktop.clone());
        saved.phase = Phase::Restoring;
        saved.return_phase = Some(Phase::Filed);
        saved.pending_sort = true;
        db.save(&saved).unwrap();
        let fs = Moved { path: saved.path.clone(), identity: saved.identity.clone() };
        recover_entry(&mut db, &fs, &saved.id).unwrap();
        let recovered = db.get(&saved.id).unwrap();
        assert_eq!(recovered.phase, Phase::Filed);
        assert_eq!(recovered.path, saved.path);
        assert_eq!(recovered.folder, "Board");
        assert_eq!(recovered.undo, Some(desktop));
        assert!(!recovered.inbox);
        assert!(!recovered.pending_sort);
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
    #[test]
    fn edit_recovers_a_completed_replacement_before_discarding_its_staged_identity() {
        use std::cell::{Cell, RefCell};
        struct Replaced {
            actual: RefCell<Identity>,
            cleaned: Cell<bool>,
        }
        impl Files for Replaced {
            fn inspect(&self, _: &Path) -> Result<Identity> { Ok(self.actual.borrow().clone()) }
            fn resolve(&self, p: &Path, i: &Identity) -> Result<PathBuf> {
                if self.actual.borrow().same_object(i) { Ok(p.into()) } else { Err("Wrong original".into()) }
            }
            fn destination(&self, _: &Path, _: &[String], _: &std::ffi::OsStr) -> Result<PathBuf> { unreachable!() }
            fn stage(&self, _: &Entry) -> Result<Option<Staged>> {
                Ok(Some(Staged { path: "/owned-stage/shot.png".into(), identity: self.actual.borrow().clone(), projection: Projection::default() }))
            }
            fn annotate(&self, e: &Entry, s: &Staged) -> Result<Staged> {
                let mut staged = s.clone();
                staged.identity.object = 99;
                staged.projection = Projection { mode: "embedded".into(), written: e.note.clone(), ..Projection::default() };
                Ok(staged)
            }
            fn replace(&self, _: &Entry, s: &Staged) -> Result<Identity> {
                *self.actual.borrow_mut() = s.identity.clone();
                Err("The replacement finished before its confirmation was interrupted".into())
            }
            fn rename(&self, _: &Path, _: &Path, _: &Identity) -> Result<Identity> { panic!("An edit must not move the file") }
            fn cleanup(&self, _: &Staged) -> Result<()> { self.cleaned.set(true); Ok(()) }
        }
        let f = Fixture::new();
        let mut db = f.db();
        let mut e = entry("/Library/Home/shot.png", "Initial note", 7);
        e.phase = Phase::Filed;
        e.folder = "Home".into();
        e.undo = Some("/Desktop/shot.png".into());
        e.projection = Projection { mode: "embedded".into(), written: e.note.clone(), ..Projection::default() };
        db.save(&e).unwrap();
        let mut pending = entry("/Library/Inbox/other.png", "Other file is still sorting", 111);
        pending.phase = Phase::Filed;
        pending.pending_sort = true;
        db.save(&pending).unwrap();
        let fs = Replaced { actual: RefCell::new(e.identity.clone()), cleaned: Cell::new(false) };
        let saved = edit(&mut db, &fs, &e.id, "Corrected kitchen quote".into()).unwrap();
        assert!(saved.identity.same_content(&fs.actual.borrow()));
        assert_eq!(saved.path, e.path);
        assert_eq!(saved.undo, e.undo);
        assert_eq!(saved.phase, Phase::Filed);
        assert_eq!(saved.projection.mode, "embedded");
        assert!(saved.staged.is_none() && fs.cleaned.get());
        assert!(db.get(&pending.id).unwrap().pending_sort, "An edit must not settle another file's live classifier");
        drop(db);
        let db = f.db();
        assert_eq!(db.get(&e.id).unwrap().note, "Corrected kitchen quote");
        assert_eq!(db.search("Corrected kitchen", 0, 65536).unwrap().total, 1);
    }

    #[test]
    fn simultaneous_first_openers_keep_every_accepted_note() {
        let f = Fixture::new();
        for round in 0..8 {
            let ready = std::sync::Arc::new(std::sync::Barrier::new(8));
            let mut workers = Vec::new();
            let path = f.0.join(format!("concurrent-{round}.sqlite"));
            for n in 0..8 {
                let ready = ready.clone();
                let path = path.clone();
                workers.push(std::thread::spawn(move || {
                    ready.wait();
                    let mut db = Catalog::open(&path).unwrap();
                    db.save(&entry(&format!("file-{n}.png"), &format!("Accepted note {n}"), n+1)).unwrap();
                }));
            }
            for worker in workers { worker.join().unwrap(); }
            let db = Catalog::open(&path).unwrap();
            assert_eq!(db.search("Accepted",0,65536).unwrap().total,8);
        }
    }

}

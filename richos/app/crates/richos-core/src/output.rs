//! THE OUTPUT RECORD — every file a thread produced, written down by a witness.
//!
//! The PRD is richos-hq `docs/prds/2026-10-05-output-side-panel.md` (§3, §4); this module is its
//! slice S1 (§12.1). The panel that shows the list is a view; the thing this module keeps is
//! **one append-only file per thread**, `<app-data>/output/<thread_id>.jsonl`, with one row per
//! WITNESSED WRITE ([`WriteRow`]), projected on read into one [`Entry`] per file.
//!
//! # Witnessed, never inferred
//!
//! A file is listed because something SAW the thread write it, never because Rich said so
//! (§1.6, §4.1). Four witnesses feed the record and nothing else may:
//!
//! - **(a)** the front desk's own completed `Write`/`Edit`/`MultiEdit`/`NotebookEdit` calls,
//!   merged per call from the machinery journal ([`witness_tool_calls`]);
//! - **(b)** the app hook's `PostToolUse` rows for the same four tools, in both leases and every
//!   worker (slice S2);
//! - **(c)** files a shell command made, seen on the disk by the hook (slice S2);
//! - **(d)** a worker's files landed into the connected repository, written by `integrate` itself
//!   (slice S2b). A land row retires the worktree copy it came from ([`project`]).
//!
//! # Identity
//!
//! A row is idempotent by its `key` (§4.4): appending a key the file already holds is a no-op,
//! so the live witness, the turn-end projection and convergence can all offer the same write
//! and it is one row. An entry is one CANONICAL path (§4.3): a file written in three turns is one
//! entry under the latest turn with `writes: 3`.
//!
//! # Every read re-stats (§4.6)
//!
//! `exists`, `bytes` and `modifiedAt` are read from the disk at projection time and never stored.
//! A deleted or moved file stays listed with `exists: false`: it is a file the thread produced,
//! and that it is gone is a fact about the Mac, not about the thread. A path that is now a
//! symbolic link, not a regular file, or a different file than the one witnessed is REFUSED at
//! projection ([`Refusal`]): it is listed, and nothing about it is read through the link.
//!
//! # Durability (§4.7)
//!
//! Append + flush, no fsync — the machinery journal's posture (`journal.rs:27`), for the same
//! reason: the sources stay the truth and the record is a converging cache of them, so a lost
//! line costs a re-projection, never a file. A write failure never fails a turn.

use crate::assignment::Assignment;
use crate::journal::MachineryJournal;
use crate::ledger::Turn;
use crate::machinery::{MachineryKind, MachineryRecord, ToolStatus};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// The directory under the app's data directory that holds one record per thread.
pub const OUTPUT_DIR: &str = "output";

/// The one row schema this build writes and reads. A row with another number is skipped and
/// counted, never half-understood.
pub const ROW_SCHEMA: u32 = 1;

/// The four write tools — the `Patch` class `timeline.rs` resolves from the same names
/// (`timeline::classify`). Named here rather than re-derived from an `ActivityType` because a
/// Tier-A record whose raw payload was evicted has no payload to classify, and the opening
/// record's title is where its name survives (`machinery.rs`, `content_block_start`).
pub const WRITE_TOOLS: [&str; 4] = ["Write", "Edit", "MultiEdit", "NotebookEdit"];

/// Who wrote it (§4.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Actor {
    /// The front-desk Rich, in the conversation.
    Rich,
    /// The back-end Rich, working an assignment for this thread.
    Backend,
    /// A worker under either of them; the row carries its name.
    Worker,
}

/// Which witness saw it (§4.2): (a), (b), (c) or (d).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WriteSource {
    /// (a) the front desk's own write-tool call, merged from the machinery journal.
    Tool,
    /// (b) a `PostToolUse` callback for a write tool, seen by the app hook.
    Hook,
    /// (c) a file a shell command made, seen on the disk by the app hook.
    Command,
    /// (d) a worker's file landed into the connected repository by `integrate` (slice S2b).
    /// The row carries the worktree copy it retires in [`WriteRow::landed_from`].
    Land,
}

/// One witnessed write (§4.2). No file content, no tool input, no command text — paths and
/// attribution only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteRow {
    pub schema: u32,
    /// The idempotency key (§4.4): `mach:<toolCallId>`, `hook:<session>:<toolUseId>`,
    /// `cmd:<session>:<canonical>:<mtimeNs>` or `land:<session>:<commit>:<path>`.
    pub key: String,
    pub thread_id: String,
    /// `None` for a write between turns or one no turn could be named for.
    pub turn_id: Option<String>,
    /// Absolute, as witnessed.
    pub path: String,
    /// Canonicalized at witness time when the file existed then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical: Option<String>,
    pub actor: Actor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    pub source: WriteSource,
    /// A `land` row only: the worker's worktree copy this row retires (§4.3), canonical when it
    /// still existed at witness time, else as written. Absent on every row written before S2b.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub landed_from: Option<String>,
    /// Epoch millis. A LABEL; order is append order.
    pub at: u64,
}

impl WriteRow {
    /// A row for a write just witnessed, canonicalizing the path NOW, which is the one moment
    /// the canonical form means "the file that was written" (§4.2).
    pub fn witnessed(
        key: String,
        thread_id: &str,
        turn_id: Option<&str>,
        path: &str,
        actor: Actor,
        source: WriteSource,
        at: u64,
    ) -> WriteRow {
        WriteRow {
            schema: ROW_SCHEMA,
            key,
            thread_id: thread_id.to_string(),
            turn_id: turn_id.map(str::to_string),
            path: path.to_string(),
            canonical: canonical_of(path),
            actor,
            worker_name: None,
            agent_id: None,
            source,
            landed_from: None,
            at,
        }
    }

    /// The projection key: the canonical path, or the path as witnessed when the file did not
    /// exist to canonicalize (§4.3).
    fn file_key(&self) -> &str {
        self.canonical.as_deref().unwrap_or(&self.path)
    }
}

fn canonical_of(path: &str) -> Option<String> {
    std::fs::canonicalize(path).ok().map(|p| p.to_string_lossy().into_owned())
}

/// Why a listed file is not read through its recorded path (§5.2 steps 2–3, applied at
/// projection). The entry stays listed; nothing is followed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Refusal {
    /// The recorded path is now a symbolic link.
    Link,
    /// The recorded path is a directory, a socket, a FIFO — not a regular file.
    NotAFile,
    /// The recorded path now resolves to a different file than the one witnessed.
    Swapped,
}

/// One file, as the panel shows it (§4.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// `out_` + the first 16 hex digits of `sha256(threadId + canonical)` — stable, not a counter.
    pub id: String,
    pub thread_id: String,
    /// The LATEST turn that wrote it: the group it sits in.
    pub turn_id: Option<String>,
    pub first_turn_id: Option<String>,
    /// How many witnessed writes.
    pub writes: usize,
    pub path: String,
    pub name: String,
    /// The folder, shown relative to home (`~/…`) when it is under it.
    pub folder: String,
    pub kind: String,
    pub actor: Actor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker_name: Option<String>,
    pub source: WriteSource,
    /// From `lstat` at read time (§4.6). `false` means nothing is at the recorded path now.
    pub exists: bool,
    pub bytes: Option<u64>,
    pub modified_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<Refusal>,
    /// The latest witnessed write.
    pub written_at: u64,
}

/// One turn's files, newest first (§6.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    /// `None` is the *Between turns* group, which is always last.
    pub turn_id: Option<String>,
    pub ids: Vec<String>,
}

/// A thread's projected record.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputList {
    /// Newest first: by the position of each file's latest write in append order.
    pub files: Vec<Entry>,
    /// The number the two buttons show: the number of entries (§4.3).
    pub count: usize,
    /// How many of them are no longer where they were written (§4.6's header line).
    pub missing: usize,
    /// Lines skipped because they would not parse, carried another schema, or named another
    /// thread. Counted, never fatal.
    pub corrupt: usize,
}

impl OutputList {
    /// The files grouped by the turn that last wrote them, groups newest first and the
    /// *Between turns* group last.
    pub fn groups(&self) -> Vec<Group> {
        let mut groups: Vec<Group> = Vec::new();
        let mut between: Option<Group> = None;
        for entry in &self.files {
            match &entry.turn_id {
                None => between.get_or_insert_with(|| Group { turn_id: None, ids: Vec::new() }).ids.push(entry.id.clone()),
                Some(turn) => match groups.iter_mut().find(|g| g.turn_id.as_deref() == Some(turn)) {
                    Some(group) => group.ids.push(entry.id.clone()),
                    None => groups.push(Group { turn_id: Some(turn.clone()), ids: vec![entry.id.clone()] }),
                },
            }
        }
        groups.extend(between);
        groups
    }
}

/// The rows a record file holds, in append order, and how many lines were skipped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordRead {
    pub rows: Vec<WriteRow>,
    pub corrupt: usize,
}

/// The kind (§4.5): from the extension, lowercased. Decides the preview and nothing else.
pub fn kind_of(path: &str) -> &'static str {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "md" | "markdown" => "md",
        "csv" => "csv",
        "xlsx" | "xls" | "numbers" => "xlsx",
        "docx" | "doc" | "pages" | "rtf" => "docx",
        "pptx" | "ppt" | "key" => "pptx",
        "pdf" => "pdf",
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "svg" => "png",
        "mp4" | "mov" | "m4v" | "webm" => "mp4",
        "m4a" | "mp3" | "wav" => "audio",
        "txt" | "text" | "log" | "json" | "jsonl" | "yaml" | "yml" | "toml" | "xml" | "html" | "htm" | "css"
        | "js" | "mjs" | "ts" | "tsx" | "jsx" | "rs" | "py" | "rb" | "go" | "java" | "kt" | "swift" | "c"
        | "h" | "cpp" | "hpp" | "sh" | "zsh" | "bash" | "sql" | "ini" | "cfg" | "conf" | "env" | "tsv" => "txt",
        _ => "other",
    }
}

/// The stable entry id (§4.3).
pub fn entry_id(thread_id: &str, file_key: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(format!("{thread_id}{file_key}").as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("out_{}", &hex[..16])
}

fn folder_label(path: &str) -> String {
    let parent = Path::new(path).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && (parent == home || parent.starts_with(&format!("{home}/"))) => {
            format!("~{}", &parent[home.len()..])
        }
        _ => parent,
    }
}

/// `lstat` the recorded path, never following it (§4.6, §5.2).
fn restat(path: &str, canonical: Option<&str>) -> (bool, Option<u64>, Option<u64>, Option<Refusal>) {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return (false, None, None, None);
    };
    if meta.file_type().is_symlink() {
        return (true, None, None, Some(Refusal::Link));
    }
    if !meta.is_file() {
        return (true, None, None, Some(Refusal::NotAFile));
    }
    if let Some(recorded) = canonical {
        if canonical_of(path).as_deref() != Some(recorded) {
            return (true, None, None, Some(Refusal::Swapped));
        }
    }
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64);
    (true, Some(meta.len()), modified, None)
}

/// The landed entry a worktree row is folded into, if a land row retired its path. A land row
/// itself is never folded.
fn retiring_key<'a>(row: &'a WriteRow, retired: &HashMap<&'a str, &'a str>) -> Option<&'a str> {
    if row.source == WriteSource::Land {
        return None;
    }
    retired
        .get(row.path.as_str())
        .or_else(|| row.canonical.as_deref().and_then(|c| retired.get(c)))
        .copied()
}

/// Fold rows (append order) into entries, newest first. Pure apart from the re-stat.
///
/// **A land row retires the entry at its `landedFrom`** (§4.3, slice S2b): every row whose path
/// or canonical path is a land row's `landedFrom` is folded into that land row's entry — counted
/// in its `writes`, eligible as its `firstTurnId` — and is never an entry of its own, so the
/// worker's deleted worktree copy is neither listed nor counted missing. The landed entry's
/// latest row is still its own (the land row, or a later write at the landed path), so its
/// actor, worker and group are the land's. The rule reads the whole slice first, so it holds
/// whatever order the land row and the worktree rows arrived in.
pub fn project(thread_id: &str, rows: &[WriteRow]) -> Vec<Entry> {
    // A worktree path -> the file key of the land row that retires it (the latest such land).
    let mut retired: HashMap<&str, &str> = HashMap::new();
    for row in rows.iter().filter(|r| r.source == WriteSource::Land) {
        if let Some(from) = row.landed_from.as_deref().filter(|f| *f != row.file_key()) {
            retired.insert(from, row.file_key());
        }
    }
    // file key -> (indexes of its rows, index of its latest OWN row, whether any row was folded in)
    let mut order: Vec<&str> = Vec::new();
    let mut spans: HashMap<&str, (Vec<usize>, Option<usize>, bool)> = HashMap::new();
    for (i, row) in rows.iter().enumerate() {
        let (key, own) = match retiring_key(row, &retired) {
            Some(landed) => (landed, false),
            None => (row.file_key(), true),
        };
        let span = spans.entry(key).or_insert_with(|| {
            order.push(key);
            (Vec::new(), None, false)
        });
        span.0.push(i);
        if own {
            span.1 = Some(i);
        } else {
            span.2 = true;
        }
    }
    let mut keyed: Vec<(usize, Entry)> = order
        .into_iter()
        .map(|key| {
            let (members, last, folded) = &spans[key];
            let writes = members.len();
            // The first write: append order, as S1 — except on a landed entry, whose worktree
            // rows may be appended after its land row by a later convergence. There the
            // earliest witnessed write is the earliest `at`, index breaking a tie, so the same
            // rows give the same entry in either order (§4.3).
            let first = if *folded {
                *members.iter().min_by_key(|&&i| (rows[i].at, i)).unwrap()
            } else {
                members[0]
            };
            // Every retiring key has its land row as an own row; the fallback is never taken.
            let last = last.unwrap_or(first);
            let latest = &rows[last];
            let (exists, bytes, modified_at, refused) = restat(&latest.path, latest.canonical.as_deref());
            let name = Path::new(&latest.path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| latest.path.clone());
            (
                last,
                Entry {
                    id: entry_id(thread_id, key),
                    thread_id: thread_id.to_string(),
                    turn_id: latest.turn_id.clone(),
                    first_turn_id: rows[first].turn_id.clone(),
                    writes,
                    path: latest.path.clone(),
                    name,
                    folder: folder_label(&latest.path),
                    kind: kind_of(&latest.path).to_string(),
                    actor: latest.actor,
                    worker_name: latest.worker_name.clone(),
                    source: latest.source,
                    exists,
                    bytes,
                    modified_at,
                    refused,
                    written_at: latest.at,
                },
            )
        })
        .collect();
    keyed.sort_by_key(|k| std::cmp::Reverse(k.0));
    keyed.into_iter().map(|(_, e)| e).collect()
}

fn usable_thread_id(thread_id: &str) -> bool {
    !thread_id.is_empty()
        && thread_id.len() <= 128
        && thread_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn invalid(what: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, what.to_string())
}

/// The Tauri event a thread's new files are announced on (§6.6): `{threadId, added, count}`.
pub const EVENT_OUTPUT: &str = "rich://output";

/// Where an append is announced (§6.6). MUST be non-blocking and infallible from the
/// writer's view: a UI that is not listening never fails a turn.
pub trait OutputObserver: Send + Sync {
    /// `added` is the entries the appended rows touched, projected and re-stated; `count` is
    /// the thread's entry count after the append.
    fn on_output(&self, thread_id: &str, added: &[Entry], count: usize);
}

/// The `rich://output` payload, camelCase: `{threadId, added, count}`.
pub fn event_payload(thread_id: &str, added: &[Entry], count: usize) -> serde_json::Value {
    serde_json::json!({ "threadId": thread_id, "added": added, "count": count })
}

/// The record store: `<app-data>/output/`.
#[derive(Clone)]
pub struct OutputStore {
    root: PathBuf,
    /// Announces every append that wrote something. One store, cloned into the spine and the
    /// work host, so every writer announces through the same chokepoint.
    observer: Option<std::sync::Arc<dyn OutputObserver>>,
    /// Folders whose files are recorded but never listed or counted (CEO 2026-10-06, PRD §13 Q3).
    /// `None` lists everything: the default, so a fixture under the system temp folder is listed.
    scratch: Option<ScratchRoots>,
}

/// THE SCRATCH SET — the one place that says which files the panel hides (CEO 2026-10-06, PRD
/// §13 Q3: "the panel lists only files written into your projects and folders"). The record keeps
/// every row; only the list, its counts and the "Wrote N files" links leave these out.
pub const SCRATCH_PREFIXES: &[&str] =
    &["/tmp", "/private/tmp", "/var/folders", "/private/var/folders"];
/// A path containing any of these folder sequences (slash-separated, consecutive) is scratch:
/// agents' worktree folders. A project's own `.claude/agents/` or `.claude/skills/` is not.
pub const SCRATCH_DIR_NAMES: &[&str] = &[".claude/worktrees"];

/// [`SCRATCH_PREFIXES`] plus this process's `TMPDIR` (and `temp_dir()`), with `/private` spellings
/// resolved, and [`SCRATCH_DIR_NAMES`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScratchRoots {
    prefixes: Vec<PathBuf>,
    dir_names: Vec<String>,
}

impl ScratchRoots {
    pub fn standard() -> Self {
        Self::with_machine_settings(&crate::machine_settings::Settings::current().unwrap_or_default())
    }

    pub fn with_machine_settings(settings: &crate::machine_settings::Settings) -> Self {
        let mut prefixes: Vec<PathBuf> = SCRATCH_PREFIXES.iter().map(PathBuf::from).collect();
        if let Some(t) = std::env::var_os("TMPDIR").filter(|t| !t.is_empty()) {
            prefixes.push(PathBuf::from(t));
        }
        prefixes.push(std::env::temp_dir());
        prefixes.push(settings.agent_root());
        let resolved: Vec<PathBuf> = prefixes.iter().filter_map(|p| std::fs::canonicalize(p).ok()).collect();
        prefixes.extend(resolved);
        prefixes.retain(|p| p.is_absolute() && p.components().count() > 1);
        prefixes.sort();
        prefixes.dedup();
        ScratchRoots { prefixes, dir_names: SCRATCH_DIR_NAMES.iter().map(|s| s.to_string()).collect() }
    }

    /// Roots given outright (tests).
    pub fn with(prefixes: Vec<PathBuf>, dir_names: Vec<String>) -> Self {
        ScratchRoots { prefixes, dir_names }
    }

    pub fn is_scratch(&self, path: &str) -> bool {
        let one = |p: &Path| {
            self.prefixes.iter().any(|r| p.starts_with(r))
                || self.dir_names.iter().any(|n| {
                    let seq: Vec<&str> = n.split('/').filter(|x| !x.is_empty()).collect();
                    let comps: Vec<_> = p.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
                    !seq.is_empty() && comps.windows(seq.len()).any(|w| w.iter().zip(&seq).all(|(a, b)| a == b))
                })
        };
        one(Path::new(path)) || canonical_of(path).is_some_and(|c| one(Path::new(&c)))
    }
}

impl std::fmt::Debug for OutputStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutputStore").field("root", &self.root).field("observed", &self.observer.is_some()).finish()
    }
}

impl OutputStore {
    /// A store rooted at `root` (normally `<app-data>/output`). Nothing is created until the
    /// first append.
    pub fn new(root: impl AsRef<Path>) -> Self {
        OutputStore { root: root.as_ref().to_path_buf(), observer: None, scratch: None }
    }

    /// The same store, announcing every append that wrote something.
    pub fn with_observer(mut self, observer: std::sync::Arc<dyn OutputObserver>) -> Self {
        self.observer = Some(observer);
        self
    }

    /// The same store, hiding files under `roots` from every list and count. Filtered where the
    /// list is built; the record is untouched.
    pub fn with_scratch(mut self, roots: ScratchRoots) -> Self {
        self.scratch = Some(roots);
        self
    }

    /// The store for an app data directory: `<data_dir>/output`.
    pub fn for_data_dir(data_dir: &Path) -> Self {
        Self::new(data_dir.join(OUTPUT_DIR))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `<root>/<thread_id>.jsonl`. A thread id that could name anything but one file in this
    /// directory is refused.
    pub fn record_path(&self, thread_id: &str) -> std::io::Result<PathBuf> {
        if !usable_thread_id(thread_id) {
            return Err(invalid("that thread cannot be named as an output record"));
        }
        Ok(self.root.join(format!("{thread_id}.jsonl")))
    }

    /// Append every row whose key the record does not already hold, and return the rows that
    /// were written. Idempotent by key (§4.4); a row for another thread is refused.
    ///
    /// The read of the keys and the append happen under one exclusive `flock` on the record,
    /// because two writers reach the same thread's record — the spine for the front desk and
    /// the work host for the back end — and a key checked by one and appended by both would
    /// be a duplicate row.
    pub fn append(&self, thread_id: &str, rows: &[WriteRow]) -> std::io::Result<Vec<WriteRow>> {
        if rows.iter().any(|r| r.thread_id != thread_id || r.schema != ROW_SCHEMA) {
            return Err(invalid("a row for another thread or schema cannot enter this record"));
        }
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let path = self.record_path(thread_id)?;
        std::fs::create_dir_all(&self.root)?;
        let mut file = std::fs::OpenOptions::new().create(true).read(true).append(true).open(&path)?;
        let _lock = Exclusive::take(&file)?;
        let mut existing = String::new();
        file.seek(SeekFrom::Start(0))?;
        file.read_to_string(&mut existing)?;
        let mut held: HashSet<String> = existing
            .lines()
            .filter_map(|l| serde_json::from_str::<WriteRow>(l).ok())
            .map(|r| r.key)
            .collect();
        let mut added = Vec::new();
        let mut text = String::new();
        for row in rows {
            if held.insert(row.key.clone()) {
                let line = serde_json::to_string(row)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                text.push_str(&line);
                text.push('\n');
                added.push(row.clone());
            }
        }
        if !text.is_empty() {
            crate::util::ensure_line_boundary(&mut file)?;
            file.write_all(text.as_bytes())?;
            // flush, NOT sync_data — the machinery journal's posture (§4.7).
            file.flush()?;
        }
        drop(_lock);
        if !added.is_empty() {
            self.announce(thread_id, &added);
        }
        Ok(added)
    }

    /// `rich://output` after rows are appended (§6.6): the entries the new rows touched, and
    /// the thread's count. A record that cannot be projected announces nothing.
    fn announce(&self, thread_id: &str, added: &[WriteRow]) {
        let Some(observer) = self.observer.as_ref() else { return };
        let Ok(list) = self.project(thread_id) else { return };
        let ids: HashSet<String> = added.iter().map(|r| entry_id(thread_id, r.file_key())).collect();
        let touched: Vec<Entry> = list.files.iter().filter(|e| ids.contains(&e.id)).cloned().collect();
        observer.on_output(thread_id, &touched, list.count);
    }

    /// Every row the record holds, in append order. A line that will not parse, carries
    /// another schema or names another thread is skipped and counted; a key seen twice keeps
    /// its first row. A record that does not exist yet is empty, not an error.
    pub fn read(&self, thread_id: &str) -> std::io::Result<RecordRead> {
        let path = self.record_path(thread_id)?;
        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(RecordRead::default()),
            Err(e) => return Err(e),
        };
        let mut out = RecordRead::default();
        let mut seen = HashSet::new();
        for line in BufReader::new(file).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<WriteRow>(&line) {
                Ok(row) if row.schema == ROW_SCHEMA && row.thread_id == thread_id => {
                    if seen.insert(row.key.clone()) {
                        out.rows.push(row);
                    }
                }
                _ => out.corrupt += 1,
            }
        }
        Ok(out)
    }

    /// The record, projected and re-stated (§4.3, §4.6). No convergence: this is exactly what
    /// the file holds.
    pub fn project(&self, thread_id: &str) -> std::io::Result<OutputList> {
        self.project_with(thread_id, true)
    }

    /// The whole record, scratch files included: for opening a file a link names directly.
    pub fn project_unfiltered(&self, thread_id: &str) -> std::io::Result<OutputList> {
        self.project_with(thread_id, false)
    }

    fn project_with(&self, thread_id: &str, hide_scratch: bool) -> std::io::Result<OutputList> {
        let read = self.read(thread_id)?;
        let mut files = project(thread_id, &read.rows);
        if let Some(roots) = self.scratch.as_ref().filter(|_| hide_scratch) {
            files.retain(|e| !roots.is_scratch(&e.path));
        }
        Ok(OutputList {
            count: files.len(),
            missing: files.iter().filter(|e| !e.exists).count(),
            files,
            corrupt: read.corrupt,
        })
    }

    /// Converge the record with its sources (§4.7): append every witnessed write the sources
    /// hold that the record does not. Returns the rows appended.
    pub fn converge(&self, thread_id: &str, sources: &Sources) -> std::io::Result<Vec<WriteRow>> {
        let mut rows = Vec::new();
        if let Some(journal) = sources.machinery {
            rows.extend(witness_tool_calls(&journal.read_thread(thread_id)));
        }
        rows.retain(|r| r.thread_id == thread_id);
        // Witnesses (b) and (c): the `writes.jsonl` of EVERY session the thread names — each
        // `TurnStarted` session and each assignment's `work_session` (§4.7). Witness (a)'s
        // rows go first and are offered as "already known", so a lead's own `Write` the hook
        // also saw is recorded once, by (a).
        if let Some(root) = sources.evidence_root {
            let join = SessionJoin::new(sources.turns, sources.assignments);
            let mut known = self.read(thread_id)?.rows;
            known.extend(rows.iter().cloned());
            for session in join.sessions_of(thread_id) {
                let found = evidence_rows(root, &session, &join, thread_id, &known);
                known.extend(found.iter().cloned());
                rows.extend(found);
            }
        }
        self.append(thread_id, &rows)
    }

    /// Project one session's `writes.jsonl` into this thread's record: the live trigger the
    /// spine runs at the end of each front-desk turn and the work host after each back-end
    /// turn (§4.1 (b), "who projects, and when"). Returns the rows appended.
    pub fn project_session(
        &self,
        thread_id: &str,
        evidence_root: &Path,
        session_id: &str,
        join: &SessionJoin,
    ) -> std::io::Result<Vec<WriteRow>> {
        let known = self.read(thread_id)?.rows;
        let rows = evidence_rows(evidence_root, session_id, join, thread_id, &known);
        self.append(thread_id, &rows)
    }

    /// `list(thread)`: converge, then project. What `list_output` answers (§5.4).
    pub fn list(&self, thread_id: &str, sources: &Sources) -> std::io::Result<OutputList> {
        // A convergence that could not write is not a list that cannot be read: the record
        // still says what it holds, and the next list tries again.
        if let Err(e) = self.converge(thread_id, sources) {
            eprintln!("[richos] output: could not converge this thread's record: {e}");
        }
        self.project(thread_id)
    }
}

/// An exclusive `flock` held for one append, released on drop.
struct Exclusive {
    #[cfg(unix)]
    fd: std::os::fd::RawFd,
}

impl Exclusive {
    fn take(file: &std::fs::File) -> std::io::Result<Exclusive> {
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            let fd = file.as_raw_fd();
            if unsafe { libc::flock(fd, libc::LOCK_EX) } != 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(Exclusive { fd })
        }
        #[cfg(not(unix))]
        {
            let _ = file;
            Ok(Exclusive {})
        }
    }
}

impl Drop for Exclusive {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::flock(self.fd, libc::LOCK_UN);
        }
    }
}

/// What convergence reads (§4.7). Every source is optional: a store with none attached
/// projects exactly what its file holds.
#[derive(Default, Clone, Copy)]
pub struct Sources<'a> {
    /// The machinery journal: witness (a), from Tier A, which is never evicted.
    pub machinery: Option<&'a MachineryJournal>,
    /// The ledger's turns (the thread's, or all of them: the join filters).
    pub turns: &'a [Turn],
    /// The thread's assignments.
    pub assignments: &'a [Assignment],
    /// `<app-data>/engine-state/evidence`: witnesses (b) and (c).
    pub evidence_root: Option<&'a Path>,
}

// ---- witness (a): the front desk's own write-tool calls ---------------------

/// Witness (a) over machinery records (§4.1 (a)): every MERGED tool call whose name is one of
/// the four write tools, whose status is `Completed` and whose merged locations are non-empty.
///
/// **Evaluated on the merged call, never on one record** (Frank's minor 1): on the native wire
/// the name is on the opening `content_block_start`, the arguments on the `assistant` frame and
/// `Completed` on the `tool_result`, so no single record is a completed write. The merge is
/// `machinery::merge_into`, the one the journal's projection and the live path use.
///
/// The name is read from a record's payload where the raw payload survives
/// (`timeline::tool_name_of`), else from the OPENING record's title, which is the tool name by
/// construction (`machinery.rs`, `content_block_start`) and is kept in Tier A after the raw
/// payload is evicted. Internal records (re-prime, rotation) are not the thread's work.
pub fn witness_tool_calls(records: &[MachineryRecord]) -> Vec<WriteRow> {
    let mut order: Vec<String> = Vec::new();
    let mut merged: HashMap<String, MachineryRecord> = HashMap::new();
    let mut names: HashMap<String, String> = HashMap::new();
    for record in records {
        if record.internal || record.kind != MachineryKind::ToolCall {
            continue;
        }
        let Some(id) = record.tool_call_id.clone() else { continue };
        if !names.contains_key(&id) {
            let from_payload = record.payload.as_ref().and_then(crate::timeline::tool_name_of);
            let from_open = (record.status == Some(ToolStatus::Pending) && !record.title.is_empty())
                .then_some(record.title.as_str());
            if let Some(name) = from_payload.or(from_open) {
                names.insert(id.clone(), name.to_string());
            }
        }
        match merged.get_mut(&id) {
            Some(base) => crate::machinery::merge_into(base, record.clone()),
            None => {
                order.push(id.clone());
                merged.insert(id, record.clone());
            }
        }
    }
    let mut rows = Vec::new();
    for id in order {
        let call = &merged[&id];
        let is_write = names.get(&id).is_some_and(|n| WRITE_TOOLS.contains(&n.as_str()));
        if !is_write || call.status != Some(ToolStatus::Completed) || call.locations.is_empty() {
            continue;
        }
        // A front-desk WORKER's call, nested under the lead's `Agent` call (§4.1 (a)): not
        // Rich's. Witness (b) records it under the worker's name. A row journaled before the
        // field existed reads `None` and is taken as the thread's (§4.7).
        if call.parent_tool_use_id.is_some() {
            continue;
        }
        rows.extend(tool_call_rows(call));
    }
    rows
}

/// The rows one completed write-tool call produces: one per location, keyed `mach:<id>` (and
/// `mach:<id>#<n>` for a second and later location, which no observed write tool produces).
pub(crate) fn tool_call_rows(call: &MachineryRecord) -> Vec<WriteRow> {
    let Some(id) = call.tool_call_id.as_deref() else { return Vec::new() };
    call.locations
        .iter()
        .filter(|p| Path::new(p).is_absolute())
        .enumerate()
        .map(|(n, path)| {
            let key = if n == 0 { format!("mach:{id}") } else { format!("mach:{id}#{n}") };
            WriteRow::witnessed(key, &call.thread_id, call.turn_id.as_deref(), path, Actor::Rich, WriteSource::Tool, call.at)
        })
        .collect()
}

// ---- the session join (§4.1 (b)) ------------------------------------------------

/// Which lease a session is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lease {
    /// A front-desk lease: one serves one thread (`spine.rs`, the pinned `(entity, thread)`).
    FrontDesk,
    /// A back-end lease: one per thread (`work_host.rs`, `backends`).
    Backend,
}

/// A session joined to a thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Joined {
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub lease: Lease,
}

/// One front-desk turn the ledger started on a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontDeskTurn {
    pub session_id: String,
    pub thread_id: String,
    pub turn_id: String,
    pub started_at: u64,
}

/// One assignment a back-end session worked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendWork {
    pub session_id: String,
    pub thread_id: String,
    /// The turn in which he gave it (`instruction_ledger_ref`), the group its files sit in.
    pub turn_id: Option<String>,
    pub registered_at: u64,
}

/// The session → thread join (§4.1 (b)): a pure function over the ledger's turns and the
/// assignments. A session no thread names, or one that names more than one thread, joins
/// nothing — a misattributed row is worse than a missing one (`worker_events.rs`'s rule).
#[derive(Debug, Clone, Default)]
pub struct SessionJoin {
    front: HashMap<String, Vec<FrontDeskTurn>>,
    back: HashMap<String, Vec<BackendWork>>,
}

impl SessionJoin {
    /// Built from the ledger's turns (every turn with a session is a `TurnStarted` the ledger
    /// recorded; a quarantined turn is excluded, as from every scoped projection) and the
    /// assignments' `work_session`s.
    pub fn new(turns: &[Turn], assignments: &[Assignment]) -> Self {
        // EVERY session a `TurnStarted` named, not only the turn's last one: one turn can be
        // started on two leases (a rotation before it ran, seen on the test VM 2026-10-05).
        let front = turns
            .iter()
            .filter(|t| !t.quarantined)
            .flat_map(|t| {
                let mut sessions = t.started_sessions.clone();
                if let Some(last) = t.session_id.as_ref().filter(|s| !s.is_empty() && !sessions.contains(s)) {
                    sessions.push(last.clone());
                }
                sessions.into_iter().map(move |session| FrontDeskTurn {
                    session_id: session,
                    thread_id: t.thread_id.clone(),
                    turn_id: t.id.clone(),
                    started_at: t.started_at.unwrap_or(t.created_at),
                })
            })
            .collect();
        let back = assignments
            .iter()
            .filter_map(|a| {
                let session = a.work_session.as_deref().filter(|s| !s.is_empty())?;
                let turn = a
                    .instruction_ledger_ref
                    .strip_prefix(&format!("ledger:{}:", a.thread_id))
                    .filter(|t| !t.is_empty())
                    .map(str::to_string);
                Some(BackendWork {
                    session_id: session.to_string(),
                    thread_id: a.thread_id.clone(),
                    turn_id: turn,
                    registered_at: a.registered_at_ms,
                })
            })
            .collect();
        Self::from_parts(front, back)
    }

    pub fn from_parts(front: Vec<FrontDeskTurn>, back: Vec<BackendWork>) -> Self {
        let mut join = SessionJoin::default();
        for t in front {
            join.front.entry(t.session_id.clone()).or_default().push(t);
        }
        for w in back {
            join.back.entry(w.session_id.clone()).or_default().push(w);
        }
        for turns in join.front.values_mut() {
            turns.sort_by_key(|t| t.started_at);
        }
        for works in join.back.values_mut() {
            works.sort_by_key(|w| w.registered_at);
        }
        join
    }

    /// Every session this join names.
    pub fn sessions(&self) -> Vec<String> {
        let mut out: Vec<String> = self.front.keys().chain(self.back.keys()).cloned().collect();
        out.sort();
        out.dedup();
        out
    }

    /// The thread (and turn) a write on `session_id` at `at` belongs to, or `None`.
    ///
    /// The THREAD is a ledger fact and needs no clock. The TURN is the latest one that started
    /// on this session at or before the write (front desk), or the latest assignment
    /// registered at or before it (back end) — a label for grouping, inside a thread that is
    /// already certain.
    pub fn resolve(&self, session_id: &str, at: u64) -> Option<Joined> {
        let front = self.front.get(session_id);
        let back = self.back.get(session_id);
        match (front, back) {
            (Some(_), Some(_)) | (None, None) => None,
            (Some(turns), None) => {
                let thread = single_thread(turns.iter().map(|t| t.thread_id.as_str()))?;
                let turn = turns.iter().rev().find(|t| t.started_at <= at).map(|t| t.turn_id.clone());
                Some(Joined { thread_id: thread, turn_id: turn, lease: Lease::FrontDesk })
            }
            (None, Some(works)) => {
                let thread = single_thread(works.iter().map(|w| w.thread_id.as_str()))?;
                let work = works.iter().rev().find(|w| w.registered_at <= at).or_else(|| works.first());
                Some(Joined { thread_id: thread, turn_id: work.and_then(|w| w.turn_id.clone()), lease: Lease::Backend })
            }
        }
    }
}

impl SessionJoin {
    /// Every session that joins `thread_id` and nothing else.
    pub fn sessions_of(&self, thread_id: &str) -> Vec<String> {
        self.sessions()
            .into_iter()
            .filter(|s| self.resolve(s, u64::MAX).is_some_and(|j| j.thread_id == thread_id))
            .collect()
    }
}

impl BackendWork {
    /// The one assignment a back-end turn is working, on `session_id` — what the work host
    /// knows at its per-turn reader, whether or not `work_session` was written down yet.
    pub fn for_assignment(assignment: &Assignment, session_id: &str) -> BackendWork {
        BackendWork {
            session_id: session_id.to_string(),
            thread_id: assignment.thread_id.clone(),
            turn_id: assignment
                .instruction_ledger_ref
                .strip_prefix(&format!("ledger:{}:", assignment.thread_id))
                .filter(|t| !t.is_empty())
                .map(str::to_string),
            registered_at: 0,
        }
    }
}

// ---- witnesses (b) and (c): the app hook's evidence ---------------------------------

/// The file `app-evidence.py` appends one line to per witnessed write (§4.8): paths only.
pub const WRITES_FILE: &str = "writes.jsonl";

/// One line of `evidence/<session>/writes.jsonl`, as `app-evidence.py` writes it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct EvidenceWrite {
    schema: u32,
    session_id: String,
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    tool_use_id: Option<String>,
    path: String,
    at: u64,
    /// `hook` (b), `command` (c) or `land` (d).
    source: String,
    /// (c) only: the write's mtime, which is half of its key.
    #[serde(default)]
    mtime_ns: Option<u64>,
    /// (d) only: the worker's worktree copy the landed file came from.
    #[serde(default, rename = "from")]
    landed_from: Option<String>,
    /// (d) only: the commit the land moved the ref to, which is half of its key.
    #[serde(default)]
    commit: Option<String>,
    /// (d) only: the worker, read off its own receipt by the land step.
    #[serde(default)]
    worker: Option<LandWorker>,
}

/// The worker a land row names (§4.1 (d)): the receipt's `name` and `agent_id`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct LandWorker {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    agent_id: Option<String>,
}

/// A commit id as Git prints it in full: 40 (SHA-1) or 64 (SHA-256) lowercase hex digits.
fn usable_commit(commit: &str) -> bool {
    matches!(commit.len(), 40 | 64) && commit.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A session id that can name exactly one evidence folder (`app_workers.rs`'s rule).
fn usable_session(session: &str) -> bool {
    !session.is_empty()
        && session.len() <= 128
        && session.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn read_writes(evidence_root: &Path, session: &str) -> Vec<EvidenceWrite> {
    let Ok(file) = std::fs::File::open(evidence_root.join(session).join(WRITES_FILE)) else {
        return Vec::new();
    };
    // A torn last line (the hook mid-append) does not parse and is read on the next pass.
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|l| serde_json::from_str::<EvidenceWrite>(&l).ok())
        .filter(|w| w.schema == 1 && w.session_id == session)
        .collect()
}

/// The `agentId` the platform's own response to an `Agent` call names: the structured field,
/// else `agentId: <id>` in the flattened text — the two shapes `app_workers.rs` and
/// `worker-created-handoff.sh` read.
fn agent_id_of(response: &serde_json::Value) -> Option<String> {
    if let Some(id) = response.get("agentId").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        return Some(id.to_string());
    }
    let text = match response {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let rest = &text[text.find("agentId:")? + "agentId:".len()..];
    let id: String = rest
        .trim_start_matches([' ', '\t'])
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    (!id.is_empty()).then_some(id)
}

/// The workers this session dispatched, by `agent_id`, with the name they were given.
///
/// **Read from the app's own callback journal, not from the team-dir `worker-events.jsonl`
/// the PRD names.** That stream is written by the engine's `worker-*-handoff.sh` emitters,
/// which the app does not register — its plugin registers only `app-engine-hook.py`
/// (`engine_profile.rs:222`), and the lease runs with `--setting-sources ""` (`native.rs`), so
/// no user-scope hook writes one either. The `created` row those emitters write IS a
/// projection of this callback — `PostToolUse[Agent]`, the `agentId` in the platform's
/// response, the name from `tool_input.name` (`worker-created-handoff.sh`) — and that callback
/// is already in `evidence/<session>/callbacks.jsonl`, whole. Same witness, the source the app
/// actually has. A worker with no such row joins nothing yet (§4.1 (b)).
fn worker_names(evidence_root: &Path, session: &str) -> HashMap<String, Option<String>> {
    let mut out = HashMap::new();
    let Ok(file) = std::fs::File::open(evidence_root.join(session).join("callbacks.jsonl")) else {
        return out;
    };
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        // Cheap filter first: a `Write` callback carries the whole file it wrote.
        if !line.contains("\"Agent\"") {
            continue;
        }
        let Ok(row) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
        let callback = &row["callback"];
        if row["schema"] != 1
            || callback["hook_event_name"] != "PostToolUse"
            || callback["tool_name"] != "Agent"
            || callback["session_id"].as_str() != Some(session)
        {
            continue;
        }
        if let Some(id) = agent_id_of(&callback["tool_response"]) {
            let name = callback["tool_input"]["name"].as_str().filter(|s| !s.is_empty()).map(str::to_string);
            out.insert(id, name);
        }
    }
    out
}

/// Witnesses (b) and (c) for one session, joined to `thread_id` (§4.1 (b), (c); §4.4 keys).
///
/// - A row whose session joins no thread, or another thread, is not written.
/// - A row with an `agent_id` is a worker's: its name comes from the `Agent` call that
///   created it in the same session; with no such call yet, it is not written — the next
///   projection or convergence takes it (a misattributed row is worse than a missing one).
/// - A row without one is the lease's own: `backend` on a back-end session, `rich` on a
///   front-desk one. A front-desk `hook` row is Rich's own write seen a second time and is
///   skipped when witness (a) already holds that file in that turn (`known`).
pub fn evidence_rows(
    evidence_root: &Path,
    session_id: &str,
    join: &SessionJoin,
    thread_id: &str,
    known: &[WriteRow],
) -> Vec<WriteRow> {
    if !usable_session(session_id) {
        return Vec::new();
    }
    let writes = read_writes(evidence_root, session_id);
    if writes.is_empty() {
        return Vec::new();
    }
    let names = if writes.iter().any(|w| w.agent_id.is_some()) {
        worker_names(evidence_root, session_id)
    } else {
        HashMap::new()
    };
    let mut out = Vec::new();
    for w in writes {
        if !Path::new(&w.path).is_absolute() {
            continue;
        }
        let Some(joined) = join.resolve(session_id, w.at) else { continue };
        if joined.thread_id != thread_id {
            continue;
        }
        let (source, key) = match (w.source.as_str(), &w.tool_use_id, w.mtime_ns, w.commit.as_deref()) {
            ("hook", Some(id), _, _) => (WriteSource::Hook, format!("hook:{session_id}:{id}")),
            ("command", _, Some(mtime), _) => (WriteSource::Command, format!("cmd:{session_id}:{}:{mtime}", w.path)),
            ("land", _, _, Some(commit)) if usable_commit(commit) => {
                (WriteSource::Land, format!("land:{session_id}:{commit}:{}", w.path))
            }
            _ => continue,
        };
        if source == WriteSource::Land {
            // (d): the worker is the one its own receipt names, so no `Agent` callback is
            // needed to name it — but a land row with no name or no worktree copy is not one.
            let Some(worker) = w.worker.as_ref() else { continue };
            let Some(name) = worker.name.as_deref().filter(|s| !s.is_empty()) else { continue };
            let Some(from) = w.landed_from.as_deref().filter(|f| Path::new(f).is_absolute()) else { continue };
            let mut row = WriteRow::witnessed(key, thread_id, joined.turn_id.as_deref(), &w.path, Actor::Worker, source, w.at);
            row.worker_name = Some(name.to_string());
            row.agent_id = worker.agent_id.clone().filter(|s| !s.is_empty());
            row.landed_from = Some(canonical_of(from).unwrap_or_else(|| from.to_string()));
            out.push(row);
            continue;
        }
        let (actor, worker_name) = match w.agent_id.as_deref().filter(|s| !s.is_empty()) {
            Some(agent) => match names.get(agent) {
                Some(name) => (Actor::Worker, name.clone()),
                None => continue,
            },
            None => match joined.lease {
                Lease::Backend => (Actor::Backend, None),
                Lease::FrontDesk => (Actor::Rich, None),
            },
        };
        let mut row = WriteRow::witnessed(key, thread_id, joined.turn_id.as_deref(), &w.path, actor, source, w.at);
        let held_in_turn = |by: &[WriteSource]| {
            known
                .iter()
                .chain(out.iter())
                .any(|k: &WriteRow| by.contains(&k.source) && k.file_key() == row.file_key() && k.turn_id == row.turn_id)
        };
        // Rich's own write tool, seen a second time by the hook: (a) owns it.
        if actor == Actor::Rich && source == WriteSource::Hook && held_in_turn(&[WriteSource::Tool]) {
            continue;
        }
        // A write tool's file seen again by the turn-end command pass (its mtime is after the
        // turn's first command): the write is already recorded by the witness that saw it.
        if source == WriteSource::Command && held_in_turn(&[WriteSource::Tool, WriteSource::Hook]) {
            continue;
        }
        row.worker_name = worker_name;
        row.agent_id = w.agent_id.filter(|s| !s.is_empty());
        out.push(row);
    }
    out
}

fn single_thread<'a>(mut threads: impl Iterator<Item = &'a str>) -> Option<String> {
    let first = threads.next()?;
    threads.all(|t| t == first).then(|| first.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Dir(PathBuf);
    impl Dir {
        fn new(tag: &str) -> Dir {
            let p = std::env::temp_dir().join(crate::util::new_id(tag));
            std::fs::create_dir_all(&p).unwrap();
            // Canonical from the start, so `/var` → `/private/var` on macOS never makes a
            // recorded path and a compared path two spellings of one file.
            Dir(std::fs::canonicalize(&p).unwrap())
        }
        fn file(&self, name: &str, body: &str) -> String {
            let p = self.0.join(name);
            std::fs::write(&p, body).unwrap();
            p.to_string_lossy().into_owned()
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            if let Err(e) = std::fs::remove_dir_all(&self.0) {
                eprintln!("test scratch {} was not removed: {e}", self.0.display());
            }
        }
    }

    fn row(key: &str, turn: Option<&str>, path: &str) -> WriteRow {
        WriteRow::witnessed(key.into(), "thr_out", turn, path, Actor::Rich, WriteSource::Tool, 1)
    }

    /// The three wire frames of one native `Write` call, in the shape `machinery.rs`'s own tests
    /// use (`run9-rust-driven.jsonl`): the streamed open, the `assistant` arguments, the
    /// `tool_result` close carrying `tool_use_result.filePath`.
    fn write_call(id: &str, path: &str, turn: &str) -> Vec<MachineryRecord> {
        let frames = [
            json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
                   "content_block":{"type":"tool_use","id":id,"name":"Write","input":{}}},
                   "session_id":"sess","parent_tool_use_id":null}),
            json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":id,"name":"Write",
                   "input":{"file_path":path,"content":"x"}}]},"session_id":"sess","parent_tool_use_id":null}),
            json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":id,
                   "content":"File created successfully"}]},
                   "tool_use_result":{"type":"create","filePath":path},"session_id":"sess","parent_tool_use_id":null}),
        ];
        frames
            .iter()
            .enumerate()
            .flat_map(|(i, f)| MachineryRecord::from_native_event(f, "sess", i as u64))
            .map(|r| r.stamp("thr_out", Some(turn), false))
            .collect()
    }

    #[test]
    fn output_scratch_files_are_recorded_but_not_listed_or_counted() {
        let proj = Dir::new("output-scratch-proj");
        let real = proj.file("notes.zip", "real");
        let scratch = Dir::new("output-scratch-tmp");
        let roots = ScratchRoots::with(vec![scratch.0.clone()], vec![".claude/worktrees".into()]);
        let claude = proj.0.join(".claude").join("worktrees").join("agent-1");
        std::fs::create_dir_all(&claude).unwrap();
        let in_claude = claude.join("notes.zip");
        std::fs::write(&in_claude, "x").unwrap();
        let in_scratch = scratch.file("notes.zip", "copy");
        let store = OutputStore::new(proj.0.join("output")).with_scratch(roots);
        let rows = vec![
            row("a", Some("t1"), &real),
            row("b", Some("t1"), &in_scratch),
            row("c", Some("t1"), &in_claude.to_string_lossy()),
        ];
        assert_eq!(store.append("thr_out", &rows).unwrap().len(), 3);
        assert_eq!(store.read("thr_out").unwrap().rows.len(), 3, "the record keeps all three");
        let list = store.project("thr_out").unwrap();
        assert_eq!(list.count, 1);
        assert_eq!(list.files.len(), 1);
        assert_eq!(list.files[0].path, real);
        assert_eq!(store.project_unfiltered("thr_out").unwrap().count, 3);
    }

    #[test]
    fn output_the_standard_scratch_set_names_every_kind() {
        let settings = crate::machine_settings::parse("RICHOS_BULK_ROOT=/external data").unwrap();
        let roots = ScratchRoots::with_machine_settings(&settings);
        for p in [
            "/tmp/x/a.md",
            "/private/tmp/rv-zip-a1/notes.zip",
            "/var/folders/ab/cd/T/a.md",
            "/external data/tmp/claude/echo/a.md",
            "/Users/alex/ab/richos-wt/x/.claude/worktrees/agent-1/a.md",
        ] {
            assert!(roots.is_scratch(p), "{p}");
        }
        let tmpdir = std::env::temp_dir().join("x.md");
        assert!(roots.is_scratch(&tmpdir.to_string_lossy()), "TMPDIR");
        assert!(!roots.is_scratch("/Users/alex/Documents/notes.zip"));
        assert!(!roots.is_scratch("/Users/alex/proj/.claude/agents/notes.txt"), "a project's own .claude stays listed");
        assert!(!roots.is_scratch("/Users/alex/proj/.claude/skills/x/SKILL.md"));
        assert!(roots.is_scratch("/Users/alex/proj/.claude/worktrees/agent-1/notes.txt"));
        assert!(!roots.is_scratch("/tmpfoo/a.md"), "prefix is by folder, not by letters");
    }

    #[test]
    fn output_append_is_idempotent_by_key() {
        let dir = Dir::new("output-idem");
        let file = dir.file("a.md", "a");
        let store = OutputStore::new(dir.0.join("output"));
        let r = row("mach:toolu_1", Some("turn_1"), &file);
        assert_eq!(store.append("thr_out", std::slice::from_ref(&r)).unwrap().len(), 1);
        assert_eq!(store.append("thr_out", &[r.clone(), r.clone()]).unwrap().len(), 0);
        let read = store.read("thr_out").unwrap();
        assert_eq!(read.rows, vec![r]);
        assert_eq!(read.corrupt, 0);
    }

    #[test]
    fn output_three_writes_to_one_path_across_two_turns_are_one_entry_under_the_latest_turn() {
        let dir = Dir::new("output-three");
        let file = dir.file("brief.md", "v3");
        let store = OutputStore::new(dir.0.join("output"));
        store
            .append(
                "thr_out",
                &[row("mach:a", Some("turn_1"), &file), row("mach:b", Some("turn_1"), &file), row("mach:c", Some("turn_2"), &file)],
            )
            .unwrap();
        let list = store.project("thr_out").unwrap();
        assert_eq!(list.count, 1);
        let entry = &list.files[0];
        assert_eq!(entry.writes, 3);
        assert_eq!(entry.turn_id.as_deref(), Some("turn_2"));
        assert_eq!(entry.first_turn_id.as_deref(), Some("turn_1"));
        assert_eq!(entry.name, "brief.md");
        assert_eq!(entry.kind, "md");
        assert_eq!(entry.bytes, Some(2));
        assert!(entry.exists);
        assert_eq!(entry.id, entry_id("thr_out", &file));
    }

    #[test]
    fn output_a_missing_file_stats_exists_false_and_stays_listed() {
        let dir = Dir::new("output-missing");
        let file = dir.file("gone.csv", "a,b");
        let store = OutputStore::new(dir.0.join("output"));
        store.append("thr_out", &[row("mach:a", Some("turn_1"), &file)]).unwrap();
        std::fs::remove_file(&file).unwrap();
        let list = store.project("thr_out").unwrap();
        assert_eq!((list.count, list.missing), (1, 1));
        assert!(!list.files[0].exists);
        assert_eq!(list.files[0].bytes, None);
        assert_eq!(list.files[0].path, file);
    }

    #[test]
    fn output_a_symlinked_path_is_refused_at_project_time() {
        let dir = Dir::new("output-link");
        let target = dir.file("secret.txt", "do not follow");
        let link = dir.0.join("innocent.md");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let link = link.to_string_lossy().into_owned();
        let store = OutputStore::new(dir.0.join("output"));
        // Witnessed as a regular file, then swapped for a link: both spellings refuse.
        let plain = dir.file("plain.md", "x");
        store.append("thr_out", &[row("mach:link", Some("turn_1"), &link), row("mach:plain", Some("turn_1"), &plain)]).unwrap();
        std::fs::remove_file(&plain).unwrap();
        std::os::unix::fs::symlink(&target, &plain).unwrap();
        let list = store.project("thr_out").unwrap();
        assert_eq!(list.count, 2);
        for entry in &list.files {
            assert_eq!(entry.refused, Some(Refusal::Link), "{entry:?}");
            assert_eq!(entry.bytes, None, "nothing is read through a link: {entry:?}");
        }
        // A directory where a file was is not a file.
        let was_file = dir.file("now-a-dir.md", "x");
        store.append("thr_out", &[row("mach:dir", Some("turn_1"), &was_file)]).unwrap();
        std::fs::remove_file(&was_file).unwrap();
        std::fs::create_dir(&was_file).unwrap();
        let list = store.project("thr_out").unwrap();
        assert_eq!(list.files[0].refused, Some(Refusal::NotAFile));
    }

    #[test]
    fn output_a_corrupt_line_is_skipped_and_counted() {
        let dir = Dir::new("output-corrupt");
        let file = dir.file("ok.md", "x");
        let store = OutputStore::new(dir.0.join("output"));
        store.append("thr_out", &[row("mach:1", Some("turn_1"), &file)]).unwrap();
        let path = store.record_path("thr_out").unwrap();
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        // A torn line, a row of a schema this build does not know, and a row for another thread.
        let mut other_schema = serde_json::to_value(row("mach:2", None, &file)).unwrap();
        other_schema["schema"] = json!(2);
        let other_thread = serde_json::to_string(&WriteRow { thread_id: "thr_else".into(), ..row("mach:3", None, &file) }).unwrap();
        write!(f, "{{\"schema\":1,\"key\":\n{other_schema}\n{other_thread}\n").unwrap();
        drop(f);
        // The next append starts on a line boundary and is read.
        let second = dir.file("two.md", "y");
        assert_eq!(store.append("thr_out", &[row("mach:4", Some("turn_2"), &second)]).unwrap().len(), 1);
        let list = store.project("thr_out").unwrap();
        assert_eq!(list.corrupt, 3);
        assert_eq!(list.count, 2);
    }

    #[test]
    fn output_a_row_for_another_thread_or_a_crafted_thread_id_is_refused() {
        let dir = Dir::new("output-refuse");
        let store = OutputStore::new(dir.0.join("output"));
        let r = WriteRow { thread_id: "thr_else".into(), ..row("mach:1", None, "/x") };
        assert!(store.append("thr_out", &[r]).is_err());
        assert!(store.record_path("../escape").is_err());
        assert!(store.record_path("").is_err());
    }

    #[test]
    fn output_kind_follows_the_extension() {
        for (path, kind) in [
            ("/a/b.MD", "md"), ("/a/b.numbers", "xlsx"), ("/a/b.pages", "docx"), ("/a/b.rtf", "docx"),
            ("/a/b.key", "pptx"), ("/a/b.pdf", "pdf"), ("/a/b.HEIC", "png"), ("/a/b.mov", "mp4"),
            ("/a/b.m4a", "audio"), ("/a/b.json", "txt"), ("/a/b.csv", "csv"), ("/a/b", "other"), ("/a/b.zip", "other"),
        ] {
            assert_eq!(kind_of(path), kind, "{path}");
        }
    }

    #[test]
    fn output_the_join_maps_a_work_session_and_a_turn_started_session_and_refuses_the_rest() {
        let join = SessionJoin::from_parts(
            vec![
                FrontDeskTurn { session_id: "desk-1".into(), thread_id: "thr_a".into(), turn_id: "turn_1".into(), started_at: 100 },
                FrontDeskTurn { session_id: "desk-1".into(), thread_id: "thr_a".into(), turn_id: "turn_2".into(), started_at: 200 },
                // One session claimed by two threads is ambiguous, and joins nothing.
                FrontDeskTurn { session_id: "desk-x".into(), thread_id: "thr_a".into(), turn_id: "turn_3".into(), started_at: 100 },
                FrontDeskTurn { session_id: "desk-x".into(), thread_id: "thr_b".into(), turn_id: "turn_4".into(), started_at: 100 },
            ],
            vec![BackendWork { session_id: "work-1".into(), thread_id: "thr_a".into(), turn_id: Some("turn_1".into()), registered_at: 150 }],
        );
        assert_eq!(
            join.resolve("desk-1", 250),
            Some(Joined { thread_id: "thr_a".into(), turn_id: Some("turn_2".into()), lease: Lease::FrontDesk })
        );
        assert_eq!(join.resolve("desk-1", 150).unwrap().turn_id.as_deref(), Some("turn_1"));
        assert_eq!(join.resolve("desk-1", 50).unwrap().turn_id, None, "before any turn is between turns");
        assert_eq!(
            join.resolve("work-1", 400),
            Some(Joined { thread_id: "thr_a".into(), turn_id: Some("turn_1".into()), lease: Lease::Backend })
        );
        assert_eq!(join.resolve("desk-x", 150), None);
        assert_eq!(join.resolve("nobody", 150), None);
    }

    #[test]
    fn output_the_join_reads_the_real_ledger_and_assignment_records() {
        let dir = Dir::new("output-join");
        let mut ledger = crate::ledger::Ledger::open(dir.0.join("ledger.jsonl")).unwrap();
        let entity = crate::entity::EntityId::parse("femcboost").unwrap();
        let thread = ledger.create_thread("acme", &entity).unwrap();
        let binding = ledger.thread_binding(&thread).unwrap();
        let turn = ledger.record_prompt_received(&binding, "write the brief", crate::ledger::Source::Text).unwrap();
        ledger.mark_turn_started(&turn, "desk-session").unwrap();
        let assignment: Assignment = serde_json::from_value(json!({
            "schema":1,"id":"asg-1","obligation_id":"obl-1","seat":"work-seat:obl-1","entity_id":"femcboost",
            "thread_id":thread,"instruction_ledger_ref":format!("ledger:{thread}:{turn}"),"instruction_sha256":"a",
            "title":"the brief","repositories":[],"state":"registered","detail":"","registered_at_ms":5,
            "updated_at_ms":5,"work_session":"work-session"}))
        .unwrap();
        let join = SessionJoin::new(ledger.turns(), &[assignment]);
        let desk = join.resolve("desk-session", u64::MAX).unwrap();
        assert_eq!((desk.thread_id.as_str(), desk.turn_id.as_deref(), desk.lease), (thread.as_str(), Some(turn.as_str()), Lease::FrontDesk));
        let work = join.resolve("work-session", u64::MAX).unwrap();
        assert_eq!((work.thread_id.as_str(), work.turn_id.as_deref(), work.lease), (thread.as_str(), Some(turn.as_str()), Lease::Backend));
        assert_eq!(join.resolve("someone-else", u64::MAX), None);
    }

    /// S1's "Done when" (§12.1): a fixture Tier A shard with three write calls across two turns
    /// — three frames each, the wire's shape — lists as three entries in two groups, newest
    /// first; the record file exists at `<app-data>/output/<thread>.jsonl`; and a fresh store
    /// over the same directory (the process restarted) replays to the same list.
    #[test]
    fn output_done_when_a_tier_a_shard_lists_three_entries_in_two_groups_and_replays() {
        let dir = Dir::new("output-done");
        let (a, b, c) = (dir.file("plan.md", "a"), dir.file("sheet.csv", "b,c"), dir.file("deck.pdf", "%PDF"));
        let journal = MachineryJournal::new(dir.0.join("machinery"));
        let mut records = write_call("toolu_A", &a, "turn_1");
        records.extend(write_call("toolu_B", &b, "turn_1"));
        records.extend(write_call("toolu_C", &c, "turn_2"));
        for r in &records {
            journal.append(r).unwrap();
        }
        // Tier B evicted: the names must survive on Tier A alone.
        for shard in std::fs::read_dir(dir.0.join("machinery/thr_out")).unwrap() {
            let p = shard.unwrap().path();
            if p.to_string_lossy().ends_with(".raw.jsonl") {
                std::fs::remove_file(p).unwrap();
            }
        }

        let data = dir.0.clone();
        let store = OutputStore::for_data_dir(&data);
        let sources = Sources { machinery: Some(&journal), ..Sources::default() };
        let list = store.list("thr_out", &sources).unwrap();
        let names: Vec<&str> = list.files.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["deck.pdf", "sheet.csv", "plan.md"], "newest first");
        let groups = list.groups();
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].turn_id.as_deref(), Some("turn_2"));
        assert_eq!(groups[0].ids.len(), 1);
        assert_eq!(groups[1].turn_id.as_deref(), Some("turn_1"));
        assert_eq!(groups[1].ids.len(), 2);
        assert!(list.files.iter().all(|e| e.actor == Actor::Rich && e.source == WriteSource::Tool));
        assert!(data.join("output/thr_out.jsonl").is_file());

        // Converging again adds nothing; a fresh store replays the same list.
        assert!(store.converge("thr_out", &sources).unwrap().is_empty());
        let replayed = OutputStore::for_data_dir(&data).project("thr_out").unwrap();
        assert_eq!(replayed, list);
    }

    /// Seen on the test VM (2026-10-05): one turn started on one lease, then again on the lease
    /// a rotation put in the chair before it ran. `Turn::session_id` keeps only the second; a
    /// write the first lease's hook saw must still join the thread.
    #[test]
    fn output_the_join_takes_every_session_a_turn_was_started_on() {
        let dir = Dir::new("output-two-starts");
        let mut ledger = crate::ledger::Ledger::open(dir.0.join("ledger.jsonl")).unwrap();
        let entity = crate::entity::EntityId::parse("femcboost").unwrap();
        let thread = ledger.create_thread("acme", &entity).unwrap();
        let binding = ledger.thread_binding(&thread).unwrap();
        let turn = ledger.record_prompt_received(&binding, "add the notes", crate::ledger::Source::Text).unwrap();
        ledger.mark_turn_started(&turn, "before-rotation").unwrap();
        ledger.mark_turn_started(&turn, "after-rotation").unwrap();
        assert_eq!(ledger.turn(&turn).unwrap().session_id.as_deref(), Some("after-rotation"));
        let join = SessionJoin::new(ledger.turns(), &[]);
        for session in ["before-rotation", "after-rotation"] {
            let joined = join.resolve(session, u64::MAX).unwrap_or_else(|| panic!("{session} joined nothing"));
            assert_eq!((joined.thread_id.as_str(), joined.turn_id.as_deref()), (thread.as_str(), Some(turn.as_str())));
        }
        // Reopened from the log, the same.
        drop(ledger);
        let reopened = crate::ledger::Ledger::open(dir.0.join("ledger.jsonl")).unwrap();
        assert!(SessionJoin::new(reopened.turns(), &[]).resolve("before-rotation", u64::MAX).is_some());
    }

    // ---- S2: witnesses (b) and (c), the join, and the nested worker ---------------------

    /// A thread with one front-desk turn on session `desk-1` and one assignment worked on
    /// session `work-1`, in the REAL ledger and assignment shapes.
    struct World {
        dir: Dir,
        ledger: crate::ledger::Ledger,
        thread: String,
        turn: String,
        assignment: Assignment,
        evidence: PathBuf,
    }

    fn world(tag: &str) -> World {
        let dir = Dir::new(tag);
        let mut ledger = crate::ledger::Ledger::open(dir.0.join("ledger.jsonl")).unwrap();
        let entity = crate::entity::EntityId::parse("femcboost").unwrap();
        let thread = ledger.create_thread("acme", &entity).unwrap();
        let binding = ledger.thread_binding(&thread).unwrap();
        let turn = ledger.record_prompt_received(&binding, "make the brief", crate::ledger::Source::Text).unwrap();
        ledger.mark_turn_started(&turn, "desk-1").unwrap();
        let assignment: Assignment = serde_json::from_value(json!({
            "schema":1,"id":"asg-1","obligation_id":"obl-1","seat":"work-seat:obl-1","entity_id":"femcboost",
            "thread_id":thread,"instruction_ledger_ref":format!("ledger:{thread}:{turn}"),"instruction_sha256":"a",
            "title":"the brief","repositories":[],"state":"registered","detail":"","registered_at_ms":5,
            "updated_at_ms":5,"work_session":"work-1"}))
        .unwrap();
        let evidence = dir.0.join("engine-state/evidence");
        World { dir, ledger, thread, turn, assignment, evidence }
    }

    impl World {
        fn line(&self, session: &str, file: &str, value: serde_json::Value) {
            let folder = self.evidence.join(session);
            std::fs::create_dir_all(&folder).unwrap();
            let mut f = std::fs::OpenOptions::new().create(true).append(true).open(folder.join(file)).unwrap();
            writeln!(f, "{value}").unwrap();
        }
        /// `PostToolUse[Agent]` as `app-evidence.py` keeps it: the whole callback, with the
        /// platform's async-launch answer naming the worker's id.
        fn created(&self, session: &str, agent: &str, name: &str) {
            self.line(session, "callbacks.jsonl", json!({"schema":1,"callback":{
                "hook_event_name":"PostToolUse","session_id":session,"tool_name":"Agent","tool_use_id":format!("tu-agent-{agent}"),
                "tool_input":{"name":name,"subagent_type":"worker","prompt":"write it"},
                "tool_response":{"isAsync":true,"status":"async_launched","agentId":agent}}}));
        }
        fn wrote(&self, session: &str, agent: Option<&str>, tool_use: &str, path: &str) {
            self.line(session, WRITES_FILE, json!({"schema":1,"session_id":session,"agent_id":agent,
                "tool_use_id":tool_use,"path":path,"at":crate::util::now_millis(),"source":"hook"}));
        }
        fn command_made(&self, session: &str, agent: Option<&str>, path: &str, mtime_ns: u64) {
            self.line(session, WRITES_FILE, json!({"schema":1,"session_id":session,"agent_id":agent,
                "tool_use_id":"tu-bash","path":path,"at":crate::util::now_millis(),"source":"command","mtime_ns":mtime_ns}));
        }
        /// A land row as `app-evidence.py`'s `append_land_rows` writes it (§4.1 (d)).
        fn landed(&self, session: &str, path: &str, from: &str, commit: Option<&str>, name: &str, agent: Option<&str>) {
            self.line(session, WRITES_FILE, json!({"schema":1,"session_id":session,"agent_id":null,"tool_use_id":null,
                "path":path,"from":from,"commit":commit,"worker":{"name":name,"agent_id":agent},
                "at":crate::util::now_millis(),"source":"land"}));
        }
        fn sources(&self) -> (Vec<Assignment>, PathBuf) {
            (vec![self.assignment.clone()], self.evidence.clone())
        }
        fn join(&self) -> SessionJoin {
            SessionJoin::new(self.ledger.turns(), std::slice::from_ref(&self.assignment))
        }
    }

    fn by_name<'a>(list: &'a OutputList, name: &str) -> &'a Entry {
        list.files.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("{name} not listed: {list:?}"))
    }

    /// S2's "Done when" (§12.2): a worker's `Write` in fixture `callbacks.jsonl` + `writes.jsonl`
    /// — once under a `work_session`, once under a `TurnStarted.session_id` — appears in
    /// `list(thread)` with `actor: worker` and its name; a command-made file appears with
    /// `source: command`; an unjoinable row does not appear.
    #[test]
    fn output_done_when_worker_writes_under_both_leases_and_a_command_file_are_listed_and_strays_are_not() {
        let w = world("output-s2-done");
        let (back, front, pdf) = (w.dir.file("back.md", "b"), w.dir.file("front.md", "f"), w.dir.file("brief.pdf", "%PDF"));
        let stray = w.dir.file("stray.md", "s");
        w.created("work-1", "agent-back", "mark-sonnet-f1");
        w.wrote("work-1", Some("agent-back"), "tu-1", &back);
        w.created("desk-1", "agent-front", "norm-sonnet-a");
        w.wrote("desk-1", Some("agent-front"), "tu-2", &front);
        w.command_made("desk-1", None, &pdf, 1_759_660_800_123_000_000);
        // A session this thread never names, and a line in desk-1's folder claiming another session.
        w.wrote("nobody", None, "tu-4", &stray);
        w.line("desk-1", WRITES_FILE, json!({"schema":1,"session_id":"intruder","path":stray,"at":1,"source":"hook","tool_use_id":"tu-5"}));

        let (assignments, evidence) = w.sources();
        let store = OutputStore::for_data_dir(&w.dir.0);
        let sources = Sources { turns: w.ledger.turns(), assignments: &assignments, evidence_root: Some(&evidence), machinery: None };
        let list = store.list(&w.thread, &sources).unwrap();
        assert_eq!(list.count, 3, "{list:?}");

        let b = by_name(&list, "back.md");
        assert_eq!((b.actor, b.worker_name.as_deref(), b.source), (Actor::Worker, Some("mark-sonnet-f1"), WriteSource::Hook));
        assert_eq!(b.turn_id.as_deref(), Some(w.turn.as_str()), "a back-end file sits in the turn he gave the work");
        let f = by_name(&list, "front.md");
        assert_eq!((f.actor, f.worker_name.as_deref()), (Actor::Worker, Some("norm-sonnet-a")), "not Rich");
        let p = by_name(&list, "brief.pdf");
        assert_eq!((p.actor, p.source), (Actor::Rich, WriteSource::Command));
        assert!(list.files.iter().all(|e| e.name != "stray.md"));
        assert!(evidence_rows(&evidence, "nobody", &w.join(), &w.thread, &[]).is_empty(), "an unjoinable session writes nothing");

        // Convergence is idempotent: the second list appends nothing.
        assert!(store.converge(&w.thread, &sources).unwrap().is_empty());
    }

    #[test]
    fn output_a_nested_worker_write_on_a_fixture_wire_produces_no_rich_row() {
        let dir = Dir::new("output-nested");
        let file = dir.file("worker.md", "w");
        let records: Vec<MachineryRecord> = write_call("toolu_W", &file, "turn_1")
            .into_iter()
            .map(|mut r| {
                r.parent_tool_use_id = Some("toolu_AGENT".into());
                r
            })
            .collect();
        assert!(witness_tool_calls(&records).is_empty());
        // The lead's own call beside it still is one.
        let mut both = records;
        both.extend(write_call("toolu_L", &file, "turn_1"));
        let rows = witness_tool_calls(&both);
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].key.as_str(), rows[0].actor), ("mach:toolu_L", Actor::Rich));
    }

    #[test]
    fn output_a_lead_write_the_hook_also_saw_is_recorded_once_by_witness_a() {
        let w = world("output-lead-twice");
        let (seen_twice, hook_only) = (w.dir.file("plan.md", "p"), w.dir.file("notes.md", "n"));
        let store = OutputStore::for_data_dir(&w.dir.0);
        store.append(&w.thread, &[WriteRow::witnessed("mach:toolu_P".into(), &w.thread, Some(&w.turn), &seen_twice, Actor::Rich, WriteSource::Tool, 1)]).unwrap();
        w.wrote("desk-1", None, "toolu_P", &seen_twice);
        w.wrote("desk-1", None, "toolu_N", &hook_only);
        let added = store.project_session(&w.thread, &w.evidence, "desk-1", &w.join()).unwrap();
        assert_eq!(added.len(), 1, "{added:?}");
        assert_eq!((added[0].path.as_str(), added[0].actor, added[0].source), (hook_only.as_str(), Actor::Rich, WriteSource::Hook));
        let list = store.project(&w.thread).unwrap();
        assert_eq!(by_name(&list, "plan.md").writes, 1, "one write, not two");
    }

    #[test]
    fn output_a_worker_row_before_its_created_row_waits_and_is_written_at_convergence() {
        let w = world("output-late-created");
        let file = w.dir.file("late.md", "l");
        let store = OutputStore::for_data_dir(&w.dir.0);
        w.wrote("work-1", Some("agent-late"), "tu-late", &file);
        assert!(store.project_session(&w.thread, &w.evidence, "work-1", &w.join()).unwrap().is_empty());
        w.created("work-1", "agent-late", "zach-opus-x");
        let (assignments, evidence) = w.sources();
        let sources = Sources { turns: w.ledger.turns(), assignments: &assignments, evidence_root: Some(&evidence), machinery: None };
        let list = store.list(&w.thread, &sources).unwrap();
        assert_eq!(by_name(&list, "late.md").worker_name.as_deref(), Some("zach-opus-x"));
    }

    #[test]
    fn output_the_turn_end_command_pass_does_not_count_a_tool_write_twice() {
        let w = world("output-cmd-dup");
        let (written, made) = (w.dir.file("w.md", "w"), w.dir.file("chart.png", "png"));
        w.created("work-1", "agent-1", "mark-sonnet-f2");
        w.wrote("work-1", Some("agent-1"), "tu-w", &written);
        w.command_made("work-1", Some("agent-1"), &written, 42);
        w.command_made("work-1", Some("agent-1"), &made, 43);
        w.command_made("work-1", Some("agent-1"), &made, 43); // the per-call and turn-end passes: one key
        let store = OutputStore::for_data_dir(&w.dir.0);
        let added = store.project_session(&w.thread, &w.evidence, "work-1", &w.join()).unwrap();
        assert_eq!(added.len(), 2, "{added:?}");
        let list = store.project(&w.thread).unwrap();
        assert_eq!(by_name(&list, "w.md").writes, 1);
        let chart = by_name(&list, "chart.png");
        assert_eq!((chart.source, chart.actor, chart.kind.as_str()), (WriteSource::Command, Actor::Worker, "png"));
    }

    #[test]
    fn output_an_append_that_wrote_something_is_announced_once_with_the_count() {
        #[derive(Default)]
        struct Heard(std::sync::Mutex<Vec<(String, Vec<String>, usize)>>);
        impl OutputObserver for Heard {
            fn on_output(&self, thread_id: &str, added: &[Entry], count: usize) {
                self.0.lock().unwrap().push((thread_id.into(), added.iter().map(|e| e.name.clone()).collect(), count));
            }
        }
        let dir = Dir::new("output-announce");
        let (a, b) = (dir.file("a.md", "a"), dir.file("b.md", "b"));
        let heard = std::sync::Arc::new(Heard::default());
        let store = OutputStore::new(dir.0.join("output")).with_observer(heard.clone());
        store.append("thr_out", &[row("mach:a", Some("t1"), &a)]).unwrap();
        store.append("thr_out", &[row("mach:a", Some("t1"), &a)]).unwrap(); // nothing new: silent
        store.append("thr_out", &[row("mach:b", Some("t2"), &b)]).unwrap();
        let heard = heard.0.lock().unwrap();
        assert_eq!(*heard, vec![("thr_out".to_string(), vec!["a.md".to_string()], 1), ("thr_out".to_string(), vec!["b.md".to_string()], 2)]);
        let payload = event_payload("thr_out", &[], 2);
        assert_eq!((payload["threadId"].as_str(), payload["count"].as_u64()), (Some("thr_out"), Some(2)));
    }

    #[test]
    fn output_the_agent_id_is_read_from_either_response_shape() {
        assert_eq!(agent_id_of(&json!({"status":"async_launched","agentId":"a1b2"})).as_deref(), Some("a1b2"));
        assert_eq!(agent_id_of(&json!("Async agent launched successfully.\nagentId: a7f-9_x (internal)")).as_deref(), Some("a7f-9_x"));
        assert_eq!(agent_id_of(&json!([{"type":"text","text":"done\nagentId: zz9"}])).as_deref(), Some("zz9"));
        assert_eq!(agent_id_of(&json!({"status":"completed"})), None);
    }

    #[test]
    fn output_a_write_that_did_not_complete_or_a_read_is_not_a_witnessed_write() {
        let dir = Dir::new("output-incomplete");
        let a = dir.file("a.md", "a");
        let mut records = write_call("toolu_A", &a, "turn_1");
        records.pop(); // no tool_result: never completed
        let read: Vec<MachineryRecord> = [
            json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
                   "content_block":{"type":"tool_use","id":"toolu_R","name":"Read","input":{}}}}),
            json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_R","name":"Read","input":{"file_path":a}}]}}),
            json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_R","content":"a"}]},
                   "tool_use_result":{"type":"text","file":{"filePath":a}}}),
        ]
        .iter()
        .flat_map(|f| MachineryRecord::from_native_event(f, "sess", 0))
        .map(|r| r.stamp("thr_out", Some("turn_1"), false))
        .collect();
        records.extend(read);
        assert!(witness_tool_calls(&records).is_empty());
    }

    // ---- S2b: witness (d), the land ---------------------------------------------------------

    const LANDED: &str = "0123456789abcdef0123456789abcdef01234567";

    /// A worker's worktree copy and the connected repository's landed copy of one file.
    fn worktree_and_landed(w: &World, name: &str) -> (String, String) {
        let worktree = w.dir.0.join("engine-state/target-worktrees/scope/worker-sonnet-f9");
        let acme = w.dir.0.join("Acme");
        std::fs::create_dir_all(&worktree).unwrap();
        std::fs::create_dir_all(&acme).unwrap();
        let (from, path) = (worktree.join(name), acme.join(name));
        std::fs::write(&from, "Notes for the walk test.").unwrap();
        std::fs::write(&path, "Notes for the walk test.").unwrap();
        (from.to_string_lossy().into_owned(), path.to_string_lossy().into_owned())
    }

    /// §12.2b: a `hook` row at a worktree path followed by a `land` row with that `landedFrom` is
    /// one entry at the landed path, `writes: 2`, the worker's, not counted twice, and nothing
    /// is missing after the worktree file is deleted — the walk-3 record's third row (a worker
    /// file at a worktree path, nothing at the landed path) cannot be produced again.
    #[test]
    fn output_a_land_row_retires_the_worktree_entry_and_lists_the_file_once_at_the_landed_path() {
        let w = world("output-s2b-land");
        let (from, landed) = worktree_and_landed(&w, "notes.md");
        w.created("work-1", "agent-w", "worker-sonnet-f9");
        w.wrote("work-1", Some("agent-w"), "tu-1", &from);
        let (assignments, evidence) = w.sources();
        let store = OutputStore::for_data_dir(&w.dir.0);
        let sources = Sources { turns: w.ledger.turns(), assignments: &assignments, evidence_root: Some(&evidence), machinery: None };
        // Before the land: listed at the worktree path, by the worker.
        let before = store.list(&w.thread, &sources).unwrap();
        assert_eq!((before.count, before.files[0].path.as_str()), (1, from.as_str()));

        w.landed("work-1", &landed, &from, Some(LANDED), "worker-sonnet-f9", Some("agent-w"));
        let list = store.list(&w.thread, &sources).unwrap();
        assert_eq!((list.count, list.missing), (1, 0), "{list:?}");
        let e = &list.files[0];
        assert_eq!(e.path, landed, "listed at the copy that opens");
        assert_eq!((e.actor, e.worker_name.as_deref(), e.source, e.writes), (Actor::Worker, Some("worker-sonnet-f9"), WriteSource::Land, 2));
        assert_eq!(e.turn_id.as_deref(), Some(w.turn.as_str()), "the group of his request");
        let key = store.read(&w.thread).unwrap().rows.iter().find(|r| r.source == WriteSource::Land).unwrap().key.clone();
        assert_eq!(key, format!("land:work-1:{LANDED}:{landed}"));

        // The app deletes the worktree after the land: still one entry, nothing missing.
        std::fs::remove_file(&from).unwrap();
        let after = store.list(&w.thread, &sources).unwrap();
        assert_eq!((after.count, after.missing, after.files[0].id.as_str()), (1, 0, e.id.as_str()));
        // A repeated projection appends nothing: the land row's key is the commit and the path.
        assert_eq!(store.read(&w.thread).unwrap().rows.len(), 2);
    }

    /// §12.2b: the same two rows in the other order (a convergence that appends the worktree row
    /// after its land row) project to the same list.
    #[test]
    fn output_a_land_row_and_its_worktree_row_project_the_same_in_either_order() {
        let w = world("output-s2b-order");
        let (from, landed) = worktree_and_landed(&w, "notes.pdf");
        let mut hook = WriteRow::witnessed("hook:work-1:tu-1".into(), &w.thread, Some("turn_a"), &from, Actor::Worker, WriteSource::Hook, 10);
        hook.worker_name = Some("worker-sonnet-f9".into());
        let mut land = WriteRow::witnessed(format!("land:work-1:{LANDED}:{landed}"), &w.thread, Some("turn_b"), &landed, Actor::Worker, WriteSource::Land, 20);
        land.worker_name = Some("worker-sonnet-f9".into());
        land.landed_from = canonical_of(&from);
        let forward = project(&w.thread, &[hook.clone(), land.clone()]);
        let backward = project(&w.thread, &[land, hook]);
        assert_eq!(forward, backward);
        assert_eq!(forward.len(), 1);
        let e = &forward[0];
        assert_eq!((e.writes, e.turn_id.as_deref(), e.first_turn_id.as_deref()), (2, Some("turn_b"), Some("turn_a")));
        assert_eq!((e.name.as_str(), e.source), ("notes.pdf", WriteSource::Land));
    }

    /// §12.2b: a `land` row whose `landedFrom` matches nothing (the worktree write was never
    /// witnessed) retires nothing and is listed on its own; an unrelated file stays listed.
    #[test]
    fn output_a_land_row_whose_landed_from_matches_nothing_is_listed_on_its_own() {
        let w = world("output-s2b-alone");
        let (from, landed) = worktree_and_landed(&w, "notes.md");
        let other = w.dir.file("other.md", "o");
        w.created("work-1", "agent-w", "worker-sonnet-f9");
        w.wrote("work-1", Some("agent-w"), "tu-1", &other);
        w.landed("work-1", &landed, &from, Some(LANDED), "worker-sonnet-f9", None);
        let (assignments, evidence) = w.sources();
        let store = OutputStore::for_data_dir(&w.dir.0);
        let sources = Sources { turns: w.ledger.turns(), assignments: &assignments, evidence_root: Some(&evidence), machinery: None };
        let list = store.list(&w.thread, &sources).unwrap();
        assert_eq!(list.count, 2, "{list:?}");
        let notes = by_name(&list, "notes.md");
        assert_eq!((notes.writes, notes.worker_name.as_deref(), notes.source), (1, Some("worker-sonnet-f9"), WriteSource::Land));
        assert_eq!(by_name(&list, "other.md").source, WriteSource::Hook);
    }

    /// §12.2b: a `land` row with no commit is not a land row and is skipped (as is one with no
    /// worker name, or a relative worktree path): nothing is attributed on a guess.
    #[test]
    fn output_a_land_row_without_a_commit_or_a_worker_name_is_skipped() {
        let w = world("output-s2b-bad");
        let (from, landed) = worktree_and_landed(&w, "notes.md");
        w.landed("work-1", &landed, &from, None, "worker-sonnet-f9", None);
        w.landed("work-1", &landed, &from, Some("not-a-commit"), "worker-sonnet-f9", None);
        w.landed("work-1", &landed, &from, Some(LANDED), "", None);
        w.landed("work-1", &landed, "notes.md", Some(LANDED), "worker-sonnet-f9", None);
        let rows = evidence_rows(&w.evidence, "work-1", &w.join(), &w.thread, &[]);
        assert!(rows.is_empty(), "{rows:?}");
    }
}

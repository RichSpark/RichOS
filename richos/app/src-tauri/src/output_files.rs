//! **REACHING A FILE A THREAD PRODUCED, SAFELY** — the file commands and the preview scheme of
//! the Output side panel (richos-hq `docs/prds/2026-10-05-output-side-panel.md` §5, slice S3,
//! §12.3).
//!
//! The record (`richos-core/src/output.rs`, slices S1 and S2) says which files a thread
//! produced. This file is the only way the page reaches one of them: list it, describe it,
//! preview it, open it, show it in Finder, and serve its bytes to an `<img>`, `<video>`,
//! `<audio>` or `<iframe>` through `richos-output://<output id>`.
//!
//! # The one rule: an output id, never a path (§5.1)
//!
//! Every command and the scheme take an OUTPUT ID and resolve it inside Rust, against the record
//! of the thread the shell says is active — never a thread or a path the page names
//! ([`OutputFiles::locate`]). An id the record does not hold is *"I don't have that file in this
//! thread's output."* This is `mac_attachments.rs`'s rule applied again: the shelf of paths is
//! the shell's, the page holds handles. `list_output` is the one command that names a thread,
//! because it is how the page learns the ids; it hands out ids and paths, and the paths are
//! already the panel's to show (§6.3).
//!
//! # The checks, in order, before any read or action (§5.2)
//!
//! 1. The id is in the active thread's record.
//! 2. `lstat` the recorded path: a symbolic link is refused outright, and so is anything that
//!    is not a regular file (a folder, a socket, a FIFO).
//! 3. `canonicalize(path)` equals the canonical path recorded when it was witnessed; a mismatch
//!    means something was swapped in under the recorded path (a parent folder replaced by a
//!    link) and is refused with the link sentence.
//! 4. The size against the kind's preview cap (§7): over it is a state, not an error.
//! 5. Every read is an `O_NOFOLLOW` open of the canonical path.
//!
//! *Show in Finder still works* on a link or a swapped file (§6.7): it only selects what is at
//! the recorded path, and opens nothing.
//!
//! # What is here, and what is not
//!
//! Here: `list_output`, `output_file` (with the Launch Services app list), `output_preview`
//! (capped text, a parsed CSV, image header dimensions, a QuickLook rendition with its seven-day
//! cache sweep, scheme URLs for media and PDF), `output_open`, `output_reveal`, the
//! `richos-output` scheme with `Range`, and (slice S6, §12.6) `output_save_copy`. Not here,
//! because their slices name them: *Add to chat* (S7, `mac_attachments.rs`) and the split
//! width (S9, `nav.rs`). `capabilities/default.json` is unchanged: app commands need no
//! capability.
//!
//! # The save sheet is Rust's, never the page's (§5.4)
//!
//! *Save a copy…* shows the system save sheet through `tauri-plugin-dialog`, Tauri's own
//! plugin, run from Rust inside `output_save_copy`. Its commands are **not** granted in
//! `capabilities/default.json` — the updater's posture — so the page can ask for a copy of a
//! RECORDED file and nothing else: it cannot open a picker, choose a path or name one. A test
//! in this file reads the capability files and fails if any of them grants `dialog:`.
//!
//! Said, not hidden: registering the plugin also injects its `init-iife.js` into the webview,
//! which replaces `window.alert` and `window.confirm` with calls to `plugin:dialog|message` and
//! `plugin:dialog|confirm`. Ungranted, those calls are refused, so an `alert()` or `confirm()`
//! in the page would silently do nothing. No shipped UI file calls either (the same test
//! checks), and `window.prompt` — which the rail's rename uses — is not replaced.
//!
//! # Said, not hidden
//!
//! - The checks and the act are two steps: a file swapped in the instant between them is
//!   handed to `/usr/bin/open` or `qlmanage` by its canonical path. Reads here close that gap
//!   with `O_NOFOLLOW`; the two tools do not offer one. One user, one Mac (§5.5).
//! - A file whose path did not exist when it was witnessed has no recorded canonical form, so
//!   step 3 has nothing to compare and is skipped for it; steps 2 and 5 still hold.

use richos_core::output::{entry_id, kind_of, Entry, OutputList, OutputStore, Sources};
use richos_core::read_view::{SpineReader, SpineView};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::http::{header, Request, Response, StatusCode};
use tauri::State;

/// The preview scheme (§5.3): `richos-output://<output id>?v=<modifiedAt>`. The `?v=` is a
/// cache-buster for the webview, not a check; the check is the handler's.
pub const SCHEME: &str = "richos-output";

// ---- the sentences (§5.1, §5.2, §6.7) ---------------------------------------------------

/// An id the active thread's record does not hold, or no active thread at all (§5.1).
pub const NOT_IN_RECORD: &str = "I don't have that file in this thread's output.";
/// A symbolic link, or a file swapped in under the recorded path (§5.2 steps 2–3).
pub const LINKED: &str =
    "This file is a link to somewhere else, so I won't open it from here. Show in Finder still works.";
/// A folder, a socket, a FIFO at the recorded path (§5.2 step 2). The PRD names the refusal
/// and not its words; these are the link sentence's shape.
pub const NOT_A_FILE: &str =
    "This is a folder or a special file, not a regular file, so I won't open it from here. Show in Finder still works.";
/// Nothing at the recorded path now (§4.6, §6.7).
pub const MISSING: &str = "This file is no longer where it was written. If it was moved, open it from its new place; if Rich writes it again, it will be listed here.";
/// The record itself could not be read (§6.7).
pub const RECORD_UNREADABLE: &str = "I can't read this thread's output record right now.";
/// `/usr/bin/open` refused or failed (§6.7, `opener.rs`'s wording).
pub const WOULD_NOT_OPEN: &str = "This Mac would not open it.";
/// *Open with…* asked for an app by its place in a list that is no longer the list this Mac
/// gives for the file (§5.4: the index is recomputed and compared, never trusted).
pub const APPS_CHANGED: &str = "The apps that open this file changed since the list was shown. Choose one again.";
/// *Open with…* asked for a place the shown list does not have.
pub const APP_NOT_OFFERED: &str = "That app is not one this Mac offers for this file. Choose one again.";
/// *Save a copy…*: the save sheet was dismissed with Cancel (S6). The action says what it did.
pub const NOTHING_SAVED: &str = "Nothing was saved.";
/// *Save a copy…*: the sheet pointed at the recorded file itself. Copying a file onto itself
/// truncates it first, so this is refused before a byte is written.
pub const SAME_FILE: &str = "That is the file itself, so nothing was copied.";
/// *Save a copy…*: the sheet answered something that is not a file's place (no folder, not
/// absolute). The system sheet never does; said rather than assumed.
pub const NOT_A_PLACE: &str = "I couldn't save the copy there: that is not a place a file can be saved.";
/// *Save a copy…*: the work behind the sheet stopped before it answered (a panic in the
/// blocking task).
pub const WOULD_NOT_SAVE: &str = "I couldn't save the copy.";

/// *Save a copy…* could not write the copy: *I couldn't save the copy: <OS sentence>.*
pub fn copy_failed(error: &std::io::Error) -> String {
    format!("I couldn't save the copy: {}.", os_sentence(error))
}

/// *Saved a copy of brief.md to alex/Desktop/.*, and *… as brief 2.md.* when the sheet named it
/// differently. The folder is named the way *Show in Finder*'s sentence names one.
pub fn saved_copy(name: &str, dest: &Path) -> String {
    let given = file_name(dest);
    if given == name {
        format!("Saved a copy of {name} to {}.", folder_label(dest))
    } else {
        format!("Saved a copy of {name} to {} as {given}.", folder_label(dest))
    }
}

/// *I couldn't read this file: <OS sentence>.* (§6.7)
pub fn read_failed(error: &std::io::Error) -> String {
    format!("I couldn't read this file: {}.", os_sentence(error))
}

/// `Permission denied (os error 13)` → `permission denied`: the OS's own words, without the
/// number nobody reading the panel can use.
fn os_sentence(error: &std::io::Error) -> String {
    let text = error.to_string();
    let words = match text.find(" (os error") {
        Some(at) => text[..at].to_string(),
        None => text,
    };
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => "the system gave no reason".to_string(),
    }
}

/// *I don't have a preview for this kind of file. Open in <app> has it.* (§6.7)
pub fn no_preview(apps: &Apps) -> String {
    format!("I don't have a preview for this kind of file. {} has it.", open_label(apps))
}

/// *Too large to preview here (1.4 GB). Open in <app> has the whole thing.* (§6.7)
pub fn too_large(bytes: u64, apps: &Apps) -> String {
    format!("Too large to preview here ({}). {} has the whole thing.", human_size(bytes), open_label(apps))
}

/// *Open in <app>*, or *Open* when Launch Services named no default (§5.4's degraded mode).
fn open_label(apps: &Apps) -> String {
    match &apps.default {
        Some(app) => format!("Open in {}", app.name),
        None => "Open".to_string(),
    }
}

/// Decimal units, as Finder shows them: `96 KB`, `14.2 MB`, `1.4 GB`.
pub fn human_size(bytes: u64) -> String {
    const K: f64 = 1000.0;
    let b = bytes as f64;
    if bytes < 1000 {
        format!("{bytes} bytes")
    } else if b < K * K {
        format!("{} KB", (b / K).round() as u64)
    } else if b < K * K * K {
        format!("{:.1} MB", b / (K * K))
    } else {
        format!("{:.1} GB", b / (K * K * K))
    }
}

// ---- the caps (§7) ------------------------------------------------------------------------

const MIB: u64 = 1024 * 1024;
/// `md` and `txt`: the first 2 MiB, then *Showing the first 2 MB*.
pub const TEXT_CAP: u64 = 2 * MIB;
/// `csv`: at most this much is read, and at most [`CSV_ROWS`] rows are shown.
pub const CSV_READ_CAP: u64 = 20 * MIB;
pub const CSV_ROWS: usize = 200;
/// Office and iWork documents: a QuickLook rendition of the first page, up to this size.
pub const RENDITION_CAP: u64 = 200 * MIB;
/// `pdf`, in WebKit's own viewer.
pub const PDF_CAP: u64 = 200 * MIB;
/// The `png` family, in an `<img>`.
pub const IMAGE_CAP: u64 = 80 * MIB;
/// The most one scheme response carries. Media is streamed in parts of this size, so a 2 GB
/// recording never crosses into memory whole.
pub const RANGE_CHUNK: u64 = 8 * MIB;
/// A rendition not asked for in this long is swept (§4.8).
pub const RENDITION_KEEP: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// `qlmanage` gets this long to make one rendition before it is stopped.
const RENDITION_TIMEOUT: Duration = Duration::from_secs(60);
/// The folder under the app's cache directory the renditions live in (§4.8).
pub const PREVIEWS_DIR: &str = "output-previews";

// ---- the apps that open a file (§5.4) ---------------------------------------------------

/// One application Launch Services offers for a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppChoice {
    pub name: String,
    pub bundle_id: String,
}

/// The default application and the others, as this Mac gives them for one file. `others`
/// never holds the default, holds each bundle once, and is ordered by name, so the same Mac
/// gives the same list twice.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Apps {
    pub default: Option<AppChoice>,
    pub others: Vec<AppChoice>,
}

/// Put Launch Services' answer in its stable shape.
pub fn settle(default: Option<AppChoice>, offered: Vec<AppChoice>) -> Apps {
    let mut others: Vec<AppChoice> = Vec::new();
    for app in offered {
        let is_default = default.as_ref().is_some_and(|d| d.bundle_id == app.bundle_id);
        if !is_default && !others.iter().any(|o| o.bundle_id == app.bundle_id) {
            others.push(app);
        }
    }
    others.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.bundle_id.cmp(&b.bundle_id)));
    Apps { default, others }
}

/// How the app list is read. Launch Services in the app; a fixed list in a test.
pub type AppLister = fn(&Path) -> Apps;
/// How `/usr/bin/open` is run: its arguments in, success or not out.
pub type Runner = Arc<dyn Fn(&[String]) -> bool + Send + Sync>;

/// **Launch Services through `core-foundation`, which Tauri already pulls on macOS** (§5.4).
/// `LSCopyDefaultApplicationURLForURL` and `LSCopyApplicationURLsForURL` are CoreServices C
/// functions; the crate supplies the CF types and their memory rules. The display name is the
/// bundle's `CFBundleDisplayName`, else `CFBundleName`, else the bundle's file name without
/// `.app`. An app with no bundle identifier is left out: `open -b` cannot name it.
#[cfg(target_os = "macos")]
mod launch_services {
    use super::{settle, AppChoice, Apps};
    use core_foundation::array::{CFArray, CFArrayRef};
    use core_foundation::base::{CFType, TCFType};
    use core_foundation::bundle::CFBundle;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::error::CFErrorRef;
    use core_foundation::string::CFString;
    use core_foundation::url::{CFURLRef, CFURL};
    use std::path::Path;

    /// `kLSRolesAll`: every application that claims the type, as Finder's *Open With* lists.
    const ROLES_ALL: u32 = 0xFFFF_FFFF;

    #[link(name = "CoreServices", kind = "framework")]
    extern "C" {
        fn LSCopyDefaultApplicationURLForURL(url: CFURLRef, roles: u32, error: *mut CFErrorRef) -> CFURLRef;
        fn LSCopyApplicationURLsForURL(url: CFURLRef, roles: u32) -> CFArrayRef;
    }

    pub fn apps_for(path: &Path) -> Apps {
        let Some(url) = CFURL::from_path(path, false) else { return Apps::default() };
        // SAFETY: both functions follow the Create rule (the caller owns the result) and
        // accept a null error pointer; a null result is "none", checked before wrapping.
        let default = unsafe {
            let raw = LSCopyDefaultApplicationURLForURL(url.as_concrete_TypeRef(), ROLES_ALL, std::ptr::null_mut());
            if raw.is_null() {
                None
            } else {
                describe(&CFURL::wrap_under_create_rule(raw))
            }
        };
        let mut offered = Vec::new();
        unsafe {
            let raw = LSCopyApplicationURLsForURL(url.as_concrete_TypeRef(), ROLES_ALL);
            if !raw.is_null() {
                let list: CFArray<CFURL> = CFArray::wrap_under_create_rule(raw);
                for app in list.iter() {
                    if let Some(choice) = describe(&app) {
                        offered.push(choice);
                    }
                }
            }
        }
        settle(default, offered)
    }

    fn describe(url: &CFURL) -> Option<AppChoice> {
        let path = url.to_path()?;
        let bundle = CFBundle::new(url.clone())?;
        let info: CFDictionary<CFString, CFType> = bundle.info_dictionary();
        let text = |key: &'static str| {
            info.find(CFString::from_static_string(key))
                .and_then(|value| value.downcast::<CFString>())
                .map(|s| s.to_string())
                .filter(|s| !s.trim().is_empty())
        };
        let bundle_id = text("CFBundleIdentifier")?;
        let name = text("CFBundleDisplayName")
            .or_else(|| text("CFBundleName"))
            .or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))?;
        Some(AppChoice { name, bundle_id })
    }
}

#[cfg(target_os = "macos")]
fn system_apps(path: &Path) -> Apps {
    launch_services::apps_for(path)
}

/// Not macOS: no Launch Services, so the degraded mode §5.4 declares — *Open*, no names.
#[cfg(not(target_os = "macos"))]
fn system_apps(_path: &Path) -> Apps {
    Apps::default()
}

/// The name of the app Launch Services opens this file with, or `None` when it names none.
/// Used by the file cards Rich puts in the conversation (`rich_files_host.rs`): *Open in Preview*.
pub fn default_app_name(path: &Path) -> Option<String> {
    system_apps(path).default.map(|app| app.name)
}

fn system_open(args: &[String]) -> bool {
    match Command::new("/usr/bin/open").args(args).status() {
        Ok(status) => status.success(),
        Err(e) => {
            eprintln!("[richos] output: /usr/bin/open could not be started: {e}");
            false
        }
    }
}

// ---- resolving an id (§5.1) and the checks (§5.2) ---------------------------------------

/// An id, found in a thread's record: the path its latest write was witnessed at, and the
/// canonical form recorded then.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Located {
    pub id: String,
    pub thread_id: String,
    pub path: PathBuf,
    pub canonical: Option<PathBuf>,
}

/// Why a recorded file is not read or acted on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// Nothing at the recorded path now.
    Missing,
    /// The recorded path is a symbolic link.
    Link,
    /// A folder, a socket, a FIFO.
    NotAFile,
    /// The recorded path resolves to a different file than the one witnessed.
    Swapped,
    /// The OS would not let it be looked at; its own words.
    Unreadable(String),
}

impl Problem {
    pub fn sentence(&self) -> String {
        match self {
            Problem::Missing => MISSING.to_string(),
            Problem::Link | Problem::Swapped => LINKED.to_string(),
            Problem::NotAFile => NOT_A_FILE.to_string(),
            Problem::Unreadable(s) => s.clone(),
        }
    }

    /// The preview's `why`, so the panel can tell a refusal from a missing file without
    /// parsing a sentence.
    fn why(&self) -> &'static str {
        match self {
            Problem::Missing => "missing",
            Problem::Link | Problem::Swapped | Problem::NotAFile => "refused",
            Problem::Unreadable(_) => "readFailed",
        }
    }
}

/// A file that passed §5.2 steps 2–3: the canonical path every read and act uses, and what
/// `stat` said about it just now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    pub path: PathBuf,
    pub bytes: u64,
    pub modified_ms: u64,
}

/// §5.2 steps 2 and 3.
pub fn check(located: &Located) -> Result<Checked, Problem> {
    let meta = match std::fs::symlink_metadata(&located.path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(Problem::Missing),
        Err(e) => return Err(Problem::Unreadable(read_failed(&e))),
    };
    if meta.file_type().is_symlink() {
        return Err(Problem::Link);
    }
    if !meta.is_file() {
        return Err(Problem::NotAFile);
    }
    let now = match std::fs::canonicalize(&located.path) {
        Ok(path) => path,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(Problem::Missing),
        Err(e) => return Err(Problem::Unreadable(read_failed(&e))),
    };
    if let Some(recorded) = &located.canonical {
        if &now != recorded {
            return Err(Problem::Swapped);
        }
    }
    let modified_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    Ok(Checked { path: now, bytes: meta.len(), modified_ms })
}

/// §5.2 step 5: never through a link at the last component, whatever happened since step 2.
fn open_nofollow(path: &Path) -> Result<std::fs::File, Problem> {
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|e| match e.raw_os_error() {
            Some(libc::ELOOP) => Problem::Link,
            _ if e.kind() == std::io::ErrorKind::NotFound => Problem::Missing,
            _ => Problem::Unreadable(read_failed(&e)),
        })
}

/// At most `limit` bytes from `start`, through [`open_nofollow`].
fn read_part(path: &Path, start: u64, limit: u64) -> Result<Vec<u8>, Problem> {
    let mut file = open_nofollow(path)?;
    file.seek(SeekFrom::Start(start)).map_err(|e| Problem::Unreadable(read_failed(&e)))?;
    let mut out = Vec::new();
    file.take(limit).read_to_end(&mut out).map_err(|e| Problem::Unreadable(read_failed(&e)))?;
    Ok(out)
}

fn usable_output_id(id: &str) -> bool {
    id.len() == 20 && id.starts_with("out_") && id[4..].bytes().all(|b| b.is_ascii_hexdigit())
}

// ---- what the commands answer -----------------------------------------------------------

/// `output_file`: the entry, the apps that open it, and what the panel can show of it (§5.4).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryDetail {
    #[serde(flatten)]
    pub entry: Entry,
    pub default_app: Option<AppChoice>,
    pub other_apps: Vec<AppChoice>,
    /// `text` | `table` | `image` | `video` | `audio` | `pdf` | `rendition` | `none`.
    pub previewable: &'static str,
    /// Why, when `previewable` is `none`: the §6.7 sentence the viewer shows.
    pub reason: Option<String>,
    /// Why nothing may be done with the file right now, when that is so (S6, §6.7): `missing`,
    /// `refused` (a link, a swapped file, not a regular file) or `readFailed`. `None` when the
    /// file passed §5.2 — over the preview cap or without a viewer still opens. The panel
    /// disables its actions from this, never by matching a sentence.
    pub problem: Option<&'static str>,
}

/// One row of a CSV, as text cells.
pub type Row = Vec<String>;

/// `output_preview` (§5.4, §7). Media, PDF and renditions come back as a scheme URL and
/// cross as bytes once (§5.3); text and tables come back here, capped.
///
/// `app` (S5) is the name of the application Launch Services opens the file with, or `None`
/// when it names none (§5.4's degraded mode). The panel's facts line says *The whole sheet
/// opens in Numbers* (§7) and a picture the webview could not draw says *Open in Preview has
/// it*; the preview carries the name so the viewer never asks a second command for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "view", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Preview {
    /// `md` and `txt`: the first [`TEXT_CAP`] bytes, cut at a character boundary.
    Text { text: String, truncated: bool, bytes: u64 },
    /// `csv`: the first [`CSV_ROWS`] rows; `total_rows` counts every row read, which is all of
    /// them when `counted_all`.
    Table { rows: Vec<Row>, total_rows: usize, counted_all: bool, bytes: u64, app: Option<String> },
    /// The `png` family; dimensions from the header, never by decoding (§7).
    Image { url: String, width: Option<u32>, height: Option<u32>, bytes: u64, app: Option<String> },
    /// The `mp4` family. Length and frame size from the container's `moov` header when it is
    /// cheap to reach ([`media_facts`]); otherwise `None`, never guessed (§7).
    Video { url: String, bytes: u64, duration_ms: Option<u64>, width: Option<u32>, height: Option<u32>, app: Option<String> },
    /// `audio`. The length is read the same way, from an MPEG-4 container (`.m4a`) only.
    Audio { url: String, bytes: u64, duration_ms: Option<u64>, app: Option<String> },
    Pdf { url: String, bytes: u64, app: Option<String> },
    /// Office and iWork: the QuickLook PNG of the first page.
    Rendition { url: String, bytes: u64, app: Option<String> },
    /// Nothing to show, and why: `missing`, `refused`, `readFailed`, `tooLarge` or `noViewer`.
    None { why: &'static str, reason: String },
}

/// What a kind is previewed as, and its cap.
fn plan(kind: &str) -> (&'static str, Option<u64>) {
    match kind {
        "md" | "txt" => ("text", None),
        "csv" => ("table", None),
        "png" => ("image", Some(IMAGE_CAP)),
        "pdf" => ("pdf", Some(PDF_CAP)),
        "mp4" => ("video", None),
        "audio" => ("audio", None),
        "xlsx" | "docx" | "pptx" => ("rendition", Some(RENDITION_CAP)),
        _ => ("none", None),
    }
}

// ---- the state the commands share --------------------------------------------------------

/// Where `list_output` converges from (§4.7): the published spine and the engine state.
struct Converge {
    reader: SpineReader,
    engine_state: PathBuf,
}

/// The file commands' state, managed by the app.
pub struct OutputFiles {
    store: OutputStore,
    /// The thread the SHELL says is active (§5.2 step 1) — never one the page names.
    active: Arc<dyn Fn() -> Option<String> + Send + Sync>,
    converge: Option<Converge>,
    /// `<cache>/output-previews`.
    previews: PathBuf,
    qlmanage: PathBuf,
    lister: AppLister,
    runner: Runner,
    /// The app list last shown for each id: what *Open with…*'s index is compared against.
    shown: Mutex<HashMap<String, Apps>>,
}

impl OutputFiles {
    /// The app's: the active thread from the published spine, convergence over its ledger,
    /// its machinery journal, the thread's assignments and the evidence folder.
    pub fn for_app(store: OutputStore, reader: SpineReader, data_dir: &Path, cache_dir: &Path) -> Self {
        let active_reader = reader.clone();
        let mut files = Self::with_active(
            store,
            Arc::new(move || active_reader.snapshot().active_thread().map(str::to_string)),
            cache_dir,
        );
        files.converge = Some(Converge { reader, engine_state: data_dir.join("engine-state") });
        files
    }

    /// One fixed thread and no convergence: what the record holds is what is listed. The
    /// probe (`examples/output_files_probe.rs`) and the tests; the app itself never calls it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn for_thread(store: OutputStore, thread_id: &str, cache_dir: &Path) -> Self {
        let thread = thread_id.to_string();
        Self::with_active(store, Arc::new(move || Some(thread.clone())), cache_dir)
    }

    fn with_active(store: OutputStore, active: Arc<dyn Fn() -> Option<String> + Send + Sync>, cache_dir: &Path) -> Self {
        OutputFiles {
            store,
            active,
            converge: None,
            previews: cache_dir.join(PREVIEWS_DIR),
            qlmanage: PathBuf::from("/usr/bin/qlmanage"),
            lister: system_apps,
            runner: Arc::new(system_open),
            shown: Mutex::new(HashMap::new()),
        }
    }

    #[cfg(test)]
    fn with_lister(mut self, lister: AppLister) -> Self {
        self.lister = lister;
        self
    }

    #[cfg(test)]
    fn with_runner(mut self, runner: Runner) -> Self {
        self.runner = runner;
        self
    }

    #[cfg(test)]
    fn with_qlmanage(mut self, path: &Path) -> Self {
        self.qlmanage = path.to_path_buf();
        self
    }

    /// §5.2 step 1: the id is in the ACTIVE thread's record. The latest write of the entry
    /// names the path and its recorded canonical form.
    pub fn locate(&self, output_id: &str) -> Result<Located, String> {
        if !usable_output_id(output_id) {
            return Err(NOT_IN_RECORD.into());
        }
        let Some(thread) = (self.active)() else { return Err(NOT_IN_RECORD.into()) };
        let read = self.store.read(&thread).map_err(|e| {
            eprintln!("[richos] output: the record of {thread} could not be read: {e}");
            RECORD_UNREADABLE.to_string()
        })?;
        let latest = read
            .rows
            .iter()
            .rev()
            .find(|row| entry_id(&thread, row.canonical.as_deref().unwrap_or(&row.path)) == output_id)
            .ok_or_else(|| NOT_IN_RECORD.to_string())?;
        let path = PathBuf::from(&latest.path);
        if !path.is_absolute() {
            return Err(NOT_IN_RECORD.into());
        }
        Ok(Located {
            id: output_id.to_string(),
            thread_id: thread,
            path,
            canonical: latest.canonical.as_ref().map(PathBuf::from),
        })
    }

    /// `list_output(thread_id)`: converge with the sources, then project and re-stat (§4.6,
    /// §4.7, §5.4).
    pub fn list(&self, thread_id: &str) -> Result<OutputList, String> {
        let listed = match &self.converge {
            None => self.store.project(thread_id),
            Some(c) => {
                let view = c.reader.snapshot();
                let assignments = match view.ledger().thread_binding(thread_id) {
                    Ok(binding) => {
                        richos_core::assignment::read_all(&c.engine_state, &binding.entity_id().to_string(), thread_id)
                            .unwrap_or_else(|e| {
                                eprintln!("[richos] output: the assignments of {thread_id} could not be read: {e}");
                                Vec::new()
                            })
                    }
                    Err(_) => Vec::new(),
                };
                let evidence = c.engine_state.join("evidence");
                let sources = Sources {
                    machinery: view.machinery_journal(),
                    turns: view.ledger().turns(),
                    assignments: &assignments,
                    evidence_root: Some(&evidence),
                };
                self.store.list(thread_id, &sources)
            }
        };
        listed.map_err(|e| {
            eprintln!("[richos] output: the record of {thread_id} could not be listed: {e}");
            RECORD_UNREADABLE.to_string()
        })
    }

    /// `output_file(output_id)` (§5.4).
    pub fn file(&self, output_id: &str) -> Result<EntryDetail, String> {
        let located = self.locate(output_id)?;
        let listed = self.store.project_unfiltered(&located.thread_id).map_err(|_| RECORD_UNREADABLE.to_string())?;
        let entry = listed.files.into_iter().find(|e| e.id == output_id).ok_or_else(|| NOT_IN_RECORD.to_string())?;
        let checked = check(&located);
        let problem = checked.as_ref().err().map(|p| p.why());
        let (apps, previewable, reason) = match checked {
            Err(problem) => (Apps::default(), "none", Some(problem.sentence())),
            Ok(checked) => {
                let apps = (self.lister)(&checked.path);
                let (view, cap) = plan(&entry.kind);
                let (previewable, reason) = if view == "none" || (view == "rendition" && !self.qlmanage.is_file()) {
                    ("none", Some(no_preview(&apps)))
                } else if cap.is_some_and(|cap| checked.bytes > cap) {
                    ("none", Some(too_large(checked.bytes, &apps)))
                } else {
                    (view, None)
                };
                (apps, previewable, reason)
            }
        };
        self.shown.lock().unwrap().insert(output_id.to_string(), apps.clone());
        Ok(EntryDetail { entry, default_app: apps.default, other_apps: apps.others, previewable, reason, problem })
    }

    /// `output_preview(output_id)` (§5.4, §7).
    pub fn preview(&self, output_id: &str) -> Result<Preview, String> {
        let located = self.locate(output_id)?;
        let checked = match check(&located) {
            Ok(checked) => checked,
            Err(problem) => return Ok(Preview::None { why: problem.why(), reason: problem.sentence() }),
        };
        let kind = kind_of(&located.path.to_string_lossy());
        let url = format!("{SCHEME}://{output_id}?v={}", checked.modified_ms);
        let bytes = checked.bytes;
        let (view, cap) = plan(kind);
        if cap.is_some_and(|cap| bytes > cap) {
            return Ok(Preview::None { why: "tooLarge", reason: too_large(bytes, &(self.lister)(&checked.path)) });
        }
        let failed = |problem: Problem| Preview::None { why: problem.why(), reason: problem.sentence() };
        // The default app's name, for the facts line and a picture the webview cannot draw.
        // Asked of Launch Services only by the kinds that say it; text never does.
        let app = || (self.lister)(&checked.path).default.map(|a| a.name);
        Ok(match view {
            "text" => match read_part(&checked.path, 0, TEXT_CAP) {
                Ok(raw) => Preview::Text { text: text_of(&raw, bytes > TEXT_CAP), truncated: bytes > TEXT_CAP, bytes },
                Err(problem) => failed(problem),
            },
            "table" => match read_part(&checked.path, 0, CSV_READ_CAP) {
                Ok(raw) => {
                    let counted_all = bytes <= CSV_READ_CAP;
                    let (rows, total_rows) = parse_csv(&text_of(&raw, !counted_all), CSV_ROWS);
                    Preview::Table { rows, total_rows, counted_all, bytes, app: app() }
                }
                Err(problem) => failed(problem),
            },
            "image" => match read_part(&checked.path, 0, HEADER_BYTES) {
                Ok(head) => {
                    let size = image_size(&head);
                    Preview::Image { url, width: size.map(|s| s.0), height: size.map(|s| s.1), bytes, app: app() }
                }
                Err(problem) => failed(problem),
            },
            "pdf" => Preview::Pdf { url, bytes, app: app() },
            "video" => {
                let facts = media_facts(&checked.path);
                Preview::Video { url, bytes, duration_ms: facts.duration_ms, width: facts.width, height: facts.height, app: app() }
            }
            "audio" => Preview::Audio { url, bytes, duration_ms: media_facts(&checked.path).duration_ms, app: app() },
            "rendition" => match self.rendition(output_id, &checked) {
                Some(_) => Preview::Rendition { url, bytes, app: app() },
                None => Preview::None { why: "noViewer", reason: no_preview(&(self.lister)(&checked.path)) },
            },
            _ => Preview::None { why: "noViewer", reason: no_preview(&(self.lister)(&checked.path)) },
        })
    }

    /// `output_open(output_id, app_index)` (§5.4): the default app, or the one at `app_index`
    /// in the list `output_file` last showed for this id — recomputed now and compared, so a
    /// list that changed underneath the page opens nothing rather than the wrong app.
    pub fn open(&self, output_id: &str, app_index: Option<usize>) -> Result<String, String> {
        let located = self.locate(output_id)?;
        let checked = check(&located).map_err(|p| p.sentence())?;
        let name = file_name(&located.path);
        let path = checked.path.to_string_lossy().into_owned();
        let (args, app) = match app_index {
            None => (vec![path], (self.lister)(&checked.path).default.map(|a| a.name)),
            Some(index) => {
                let shown = self.shown.lock().unwrap().get(output_id).cloned().ok_or_else(|| APPS_CHANGED.to_string())?;
                let pick = shown.others.get(index).cloned().ok_or_else(|| APP_NOT_OFFERED.to_string())?;
                let now = (self.lister)(&checked.path);
                if now.others.get(index) != Some(&pick) {
                    return Err(APPS_CHANGED.into());
                }
                (vec!["-b".to_string(), pick.bundle_id.clone(), path], Some(pick.name))
            }
        };
        if !(self.runner)(&args) {
            return Err(WOULD_NOT_OPEN.into());
        }
        Ok(match app {
            Some(app) => format!("Opening {name} in {app}."),
            None => format!("Opening {name}."),
        })
    }

    /// `output_reveal(output_id)` (§5.4): `open -R` on the recorded path. Works on a link or a
    /// swapped file (§6.7: *Show in Finder still works*); refused when nothing is there.
    pub fn reveal(&self, output_id: &str) -> Result<String, String> {
        let located = self.locate(output_id)?;
        match std::fs::symlink_metadata(&located.path) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(MISSING.into()),
            Err(e) => return Err(read_failed(&e)),
        }
        let args = vec!["-R".to_string(), located.path.to_string_lossy().into_owned()];
        if !(self.runner)(&args) {
            return Err(WOULD_NOT_OPEN.into());
        }
        Ok(format!("Finder opens {} with {} selected.", folder_label(&located.path), file_name(&located.path)))
    }

    /// `output_save_copy(output_id)` (§5.4, slice S6): §5.2 first — a missing file, a link or
    /// a swapped file is refused with its sentence and no sheet is shown — then `choose` shows
    /// the save sheet with the file's sanitized name and answers where it pointed, then the
    /// checks run AGAIN (the sheet can stay open for minutes) and the bytes are copied there.
    ///
    /// `choose` is the system save sheet in the app ([`output_save_copy`]) and a closure in the
    /// tests. `None` is Cancel: nothing is written and the action says so.
    pub fn save_copy(&self, output_id: &str, choose: impl FnOnce(&str) -> Option<PathBuf>) -> Result<String, String> {
        let located = self.locate(output_id)?;
        check(&located).map_err(|p| p.sentence())?;
        let name = file_name(&located.path);
        let Some(dest) = choose(&suggested_name(&name)) else { return Ok(NOTHING_SAVED.into()) };
        let located = self.locate(output_id)?;
        let checked = check(&located).map_err(|p| p.sentence())?;
        copy_to(&checked.path, &dest)?;
        Ok(saved_copy(&name, &dest))
    }

    // ---- the QuickLook rendition (§7) -----------------------------------------------------

    fn rendition_path(&self, output_id: &str, modified_ms: u64) -> PathBuf {
        self.previews.join(format!("{output_id}-{modified_ms}.png"))
    }

    /// Make or reuse the first-page PNG for an Office or iWork file: `qlmanage -t -s 1600`,
    /// cached as `output-previews/<id>-<mtime>.png` so an edited file gets a new one. `None`
    /// when `qlmanage` is absent, has no generator for the type, or does not finish.
    fn rendition(&self, output_id: &str, checked: &Checked) -> Option<PathBuf> {
        if !self.qlmanage.is_file() {
            return None;
        }
        let target = self.rendition_path(output_id, checked.modified_ms);
        if target.is_file() {
            return Some(target);
        }
        if let Err(e) = std::fs::create_dir_all(&self.previews) {
            eprintln!("[richos] output: the preview cache could not be made: {e}");
            return None;
        }
        sweep(&self.previews, RENDITION_KEEP);
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let work = self.previews.join(format!(".{output_id}-{}-{nanos}", std::process::id()));
        if std::fs::create_dir(&work).is_err() {
            return None;
        }
        let made = self.run_qlmanage(&checked.path, &work);
        let produced = work.join(format!("{}.png", file_name(&checked.path)));
        let result = (made && produced.is_file() && std::fs::rename(&produced, &target).is_ok()).then(|| target.clone());
        if let Err(e) = std::fs::remove_dir_all(&work) {
            eprintln!("[richos] output: a rendition's working folder was not removed: {e}");
        }
        result
    }

    fn run_qlmanage(&self, source: &Path, out: &Path) -> bool {
        let child = Command::new(&self.qlmanage)
            .args(["-t", "-s", "1600", "-o"])
            .arg(out)
            .arg(source)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        let mut child = match child {
            Ok(child) => child,
            Err(e) => {
                eprintln!("[richos] output: qlmanage could not be started: {e}");
                return false;
            }
        };
        let started = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(status)) => return status.success(),
                Ok(None) if started.elapsed() < RENDITION_TIMEOUT => std::thread::sleep(Duration::from_millis(50)),
                _ => {
                    // The child this call spawned, by its handle — never a process found by name.
                    if let Err(e) = child.kill() {
                        eprintln!("[richos] output: qlmanage did not finish and could not be stopped: {e}");
                    }
                    if let Err(e) = child.wait() {
                        eprintln!("[richos] output: a stopped qlmanage could not be reaped: {e}");
                    }
                    return false;
                }
            }
        }
    }

    // ---- the scheme (§5.3) ------------------------------------------------------------------

    /// `richos-output://<output id>?v=<modifiedAt>`: §5.2, then the bytes with the kind's MIME,
    /// `Content-Length` and `Range`. An id the active thread's record does not hold is a 404
    /// with no bytes; so is a missing file and a kind the scheme does not serve (text comes
    /// back through `output_preview`). A link, a swapped file or a non-file is a 403.
    pub fn serve(&self, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
        let method = request.method();
        let head = if method == tauri::http::Method::GET {
            false
        } else if method == tauri::http::Method::HEAD {
            true
        } else {
            return empty(StatusCode::METHOD_NOT_ALLOWED);
        };
        let Some(id) = request.uri().host() else { return empty(StatusCode::NOT_FOUND) };
        let Ok(located) = self.locate(id) else { return empty(StatusCode::NOT_FOUND) };
        let checked = match check(&located) {
            Ok(checked) => checked,
            Err(Problem::Link | Problem::Swapped | Problem::NotAFile) => return empty(StatusCode::FORBIDDEN),
            Err(_) => return empty(StatusCode::NOT_FOUND),
        };
        let kind = kind_of(&located.path.to_string_lossy());
        let (source, mime, cap): (PathBuf, &str, Option<u64>) = match plan(kind) {
            ("image" | "pdf" | "video" | "audio", cap) => (checked.path.clone(), mime_of(&checked.path), cap),
            ("rendition", _) => {
                let path = self.rendition_path(id, checked.modified_ms);
                if !path.is_file() {
                    return empty(StatusCode::NOT_FOUND);
                }
                (path, "image/png", None)
            }
            _ => return empty(StatusCode::NOT_FOUND),
        };
        let len = if source == checked.path {
            checked.bytes
        } else {
            match std::fs::symlink_metadata(&source) {
                Ok(meta) if meta.is_file() => meta.len(),
                _ => return empty(StatusCode::NOT_FOUND),
            }
        };
        if cap.is_some_and(|cap| len > cap) {
            return empty(StatusCode::PAYLOAD_TOO_LARGE);
        }
        let asked = request.headers().get(header::RANGE).and_then(|v| v.to_str().ok());
        let (status, start, end) = match parse_range(asked, len) {
            RangeAsk::Unsatisfiable => {
                let mut response = empty(StatusCode::RANGE_NOT_SATISFIABLE);
                response.headers_mut().insert(header::CONTENT_RANGE, header_value(&format!("bytes */{len}")));
                return response;
            }
            RangeAsk::Part(start, end) => {
                let (start, end) = chunk(start, end);
                (StatusCode::PARTIAL_CONTENT, start, end)
            }
            // A capped kind is served whole; an uncapped one (media) is never read whole into
            // memory: its first part answers, as a 206 the media loader continues from.
            RangeAsk::Whole if cap.is_some() || len <= RANGE_CHUNK => (StatusCode::OK, 0, len.saturating_sub(1)),
            RangeAsk::Whole => {
                let (start, end) = chunk(0, len - 1);
                (StatusCode::PARTIAL_CONTENT, start, end)
            }
        };
        let length = if len == 0 { 0 } else { end - start + 1 };
        let body = if head || length == 0 {
            Vec::new()
        } else {
            match read_part(&source, start, length) {
                Ok(bytes) if bytes.len() as u64 == length => bytes,
                Ok(_) => return empty(StatusCode::NOT_FOUND),
                Err(Problem::Link) => return empty(StatusCode::FORBIDDEN),
                Err(_) => return empty(StatusCode::NOT_FOUND),
            }
        };
        let mut response = Response::new(body);
        *response.status_mut() = status;
        let headers = response.headers_mut();
        headers.insert(header::CONTENT_TYPE, header_value(mime));
        headers.insert(header::CONTENT_LENGTH, header_value(&length.to_string()));
        headers.insert(header::ACCEPT_RANGES, header_value("bytes"));
        headers.insert(header::X_CONTENT_TYPE_OPTIONS, header_value("nosniff"));
        if status == StatusCode::PARTIAL_CONTENT {
            headers.insert(header::CONTENT_RANGE, header_value(&format!("bytes {start}-{end}/{len}")));
        }
        if mime == "image/svg+xml" {
            // An SVG opened as a document could run script in this scheme's origin.
            headers.insert(header::CONTENT_SECURITY_POLICY, header_value("default-src 'none'; style-src 'unsafe-inline'; sandbox"));
        }
        response
    }
}

fn header_value(text: &str) -> tauri::http::HeaderValue {
    tauri::http::HeaderValue::from_str(text).unwrap_or_else(|_| tauri::http::HeaderValue::from_static(""))
}

/// A response with no bytes.
fn empty(status: StatusCode) -> Response<Vec<u8>> {
    let mut response = Response::new(Vec::new());
    *response.status_mut() = status;
    response.headers_mut().insert(header::CONTENT_LENGTH, header_value("0"));
    response
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// The name the save sheet is opened with (§5.5: "the save sheet gets a sanitized name"). The
/// PRD cites "the desk's `sanitize_title` rule"; the attachments desk's rule is
/// `sanitize_name` (`phone/attachments.rs`) — `sanitize_title` is the assignment title's, in
/// `richos-core` — and this is that rule without its kind check, because a copy keeps its own
/// extension: only the last path component, no control or direction-override characters, no
/// leading dots or surrounding space (so the name is neither hidden nor `..`), the stem at most
/// 100 bytes, and `copy` when nothing is left. It is written out rather than called because
/// this file is also compiled by path into `examples/output_files_probe.rs`.
pub fn suggested_name(raw: &str) -> String {
    let last = raw.rsplit(['/', '\\', ':']).next().unwrap_or("");
    let bidi = |c: char| matches!(c, '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}');
    let cleaned: String = last.chars().filter(|c| !c.is_control() && !bidi(*c)).collect();
    let cleaned = cleaned.trim().trim_start_matches('.').trim();
    let (stem, ext) = match cleaned.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => (stem, Some(ext)),
        _ => (cleaned, None),
    };
    let mut end = stem.len().min(100);
    while !stem.is_char_boundary(end) {
        end -= 1;
    }
    let stem = stem[..end].trim();
    match (stem.is_empty(), ext) {
        (true, Some(ext)) => format!("copy.{ext}"),
        (true, None) => "copy".to_string(),
        (false, Some(ext)) => format!("{stem}.{ext}"),
        (false, None) => stem.to_string(),
    }
}

/// Copy the checked file to where the sheet pointed. The source is read through
/// [`open_nofollow`] (§5.2 step 5), never `std::fs::copy`, which follows a link. The bytes go
/// to a new hidden file in the destination folder first and are renamed over the name the
/// sheet gave, so a copy that fails part-way leaves nothing half-written under that name. The
/// sheet already asked him before replacing an existing file, so the rename replaces it.
fn copy_to(source: &Path, dest: &Path) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    let (Some(folder), Some(given)) = (dest.parent(), dest.file_name()) else { return Err(NOT_A_PLACE.into()) };
    if !dest.is_absolute() {
        return Err(NOT_A_PLACE.into());
    }
    let folder = std::fs::canonicalize(folder).map_err(|e| copy_failed(&e))?;
    let target = folder.join(given);
    // The file itself — by the same path, a link to it, or a hard link — is never the target:
    // writing over it would destroy the original before a byte of the copy existed.
    let original = std::fs::metadata(source).map_err(|e| read_failed(&e))?;
    if let Ok(there) = std::fs::metadata(&target) {
        if there.dev() == original.dev() && there.ino() == original.ino() {
            return Err(SAME_FILE.into());
        }
    }
    let mut from = open_nofollow(source).map_err(|p| p.sentence())?;
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let partial = folder.join(format!(".{}.richos-copy-{}-{nanos}", given.to_string_lossy(), std::process::id()));
    let written = (|| -> std::io::Result<()> {
        let mut to = std::fs::OpenOptions::new().write(true).create_new(true).open(&partial)?;
        std::io::copy(&mut from, &mut to)?;
        to.set_permissions(original.permissions())?;
        to.sync_all()?;
        std::fs::rename(&partial, &target)
    })();
    if let Err(e) = written {
        if let Err(gone) = std::fs::remove_file(&partial) {
            if gone.kind() != std::io::ErrorKind::NotFound {
                eprintln!("[richos] output: a part-written copy was not removed: {gone}");
            }
        }
        return Err(copy_failed(&e));
    }
    Ok(())
}

/// The last two folders above a file, as the notice names them: `acme/counter/` (§6.7).
fn folder_label(path: &Path) -> String {
    let parts: Vec<String> = path
        .parent()
        .map(|p| p.components().filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        }).collect())
        .unwrap_or_default();
    if parts.is_empty() {
        return "/".to_string();
    }
    let from = parts.len().saturating_sub(2);
    format!("{}/", parts[from..].join("/"))
}

/// Delete every rendition (and stale working folder) not touched in `keep` (§4.8).
pub fn sweep(dir: &Path, keep: Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        let old = meta.modified().ok().and_then(|t| now.duration_since(t).ok()).is_some_and(|age| age > keep);
        if !old {
            continue;
        }
        let path = entry.path();
        let gone = if meta.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
        if let Err(e) = gone {
            eprintln!("[richos] output: an old preview could not be swept: {e}");
        }
    }
}

// ---- Range (§5.3) ---------------------------------------------------------------------------

/// What a `Range` header asks of a file of `len` bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeAsk {
    /// No header, or one this handler ignores (another unit, several ranges, a malformed
    /// spec): RFC 9110 §14.2 lets a server answer with the whole representation.
    Whole,
    /// First and last byte, inclusive, already clamped to the file.
    Part(u64, u64),
    /// Starts at or past the end, or asks for the last zero bytes: a 416.
    Unsatisfiable,
}

pub fn parse_range(header: Option<&str>, len: u64) -> RangeAsk {
    let Some(spec) = header.and_then(|h| h.trim().strip_prefix("bytes=")) else { return RangeAsk::Whole };
    if spec.contains(',') {
        return RangeAsk::Whole;
    }
    let Some((first, last)) = spec.trim().split_once('-') else { return RangeAsk::Whole };
    let (first, last) = (first.trim(), last.trim());
    if first.is_empty() {
        let Ok(suffix) = last.parse::<u64>() else { return RangeAsk::Whole };
        if suffix == 0 || len == 0 {
            return RangeAsk::Unsatisfiable;
        }
        return RangeAsk::Part(len.saturating_sub(suffix), len - 1);
    }
    let Ok(start) = first.parse::<u64>() else { return RangeAsk::Whole };
    let end = if last.is_empty() {
        None
    } else {
        match last.parse::<u64>() {
            Ok(end) if end >= start => Some(end),
            _ => return RangeAsk::Whole,
        }
    };
    if start >= len {
        return RangeAsk::Unsatisfiable;
    }
    RangeAsk::Part(start, end.map_or(len - 1, |e| e.min(len - 1)))
}

/// One response carries at most [`RANGE_CHUNK`] bytes; the loader asks again from where it
/// ends.
pub fn chunk(start: u64, end: u64) -> (u64, u64) {
    (start, end.min(start + RANGE_CHUNK - 1))
}

/// The MIME the scheme serves, from the extension (§5.3).
pub fn mime_of(path: &Path) -> &'static str {
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "heic" => "image/heic",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "m4v" => "video/x-m4v",
        "webm" => "video/webm",
        "m4a" => "audio/mp4",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
}

// ---- text, CSV and image headers (§7) -----------------------------------------------------

/// Bytes as text. When the read was cut, a character split by the cut is dropped rather than
/// shown as a replacement mark; anything else that is not UTF-8 is shown as one.
pub fn text_of(raw: &[u8], cut: bool) -> String {
    let raw = raw.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(raw);
    let usable = match std::str::from_utf8(raw) {
        Err(e) if cut && e.error_len().is_none() => &raw[..e.valid_up_to()],
        _ => raw,
    };
    String::from_utf8_lossy(usable).into_owned()
}

/// RFC 4180: comma-separated, `"` quotes a field, `""` is a quote inside one, a quoted field
/// may hold commas and line breaks, CRLF or LF ends a record. Returns the first `keep` rows and
/// how many rows there were.
pub fn parse_csv(text: &str, keep: usize) -> (Vec<Row>, usize) {
    let mut rows: Vec<Row> = Vec::new();
    let mut total = 0usize;
    let mut row: Row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut started = false;
    let mut chars = text.chars().peekable();
    let end_row = |row: &mut Row, rows: &mut Vec<Row>, total: &mut usize| {
        *total += 1;
        let done = std::mem::take(row);
        if rows.len() < keep {
            rows.push(done);
        }
    };
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => {
                quoted = true;
                started = true;
            }
            ',' => {
                row.push(std::mem::take(&mut field));
                started = true;
            }
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => {
                row.push(std::mem::take(&mut field));
                end_row(&mut row, &mut rows, &mut total);
                started = false;
            }
            _ => {
                field.push(c);
                started = true;
            }
        }
    }
    if started || !field.is_empty() || !row.is_empty() {
        row.push(field);
        end_row(&mut row, &mut rows, &mut total);
    }
    (rows, total)
}

/// How much of an image is read to find its size.
const HEADER_BYTES: u64 = 256 * 1024;

/// Width and height from the header (§7: read, not decoded): PNG, GIF, JPEG and WebP. HEIC and
/// SVG are `None`, and the facts line leaves them out rather than guessing.
pub fn image_size(head: &[u8]) -> Option<(u32, u32)> {
    let be16 = |b: &[u8], i: usize| -> Option<u32> { Some(u32::from(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]))) };
    let le16 = |b: &[u8], i: usize| -> Option<u32> { Some(u32::from(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?]))) };
    let be32 = |b: &[u8], i: usize| -> Option<u32> { Some(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?)) };
    let le24 = |b: &[u8], i: usize| -> Option<u32> {
        Some(u32::from(*b.get(i)?) | u32::from(*b.get(i + 1)?) << 8 | u32::from(*b.get(i + 2)?) << 16)
    };
    if head.starts_with(b"\x89PNG\r\n\x1a\n") && head.get(12..16) == Some(b"IHDR") {
        return Some((be32(head, 16)?, be32(head, 20)?));
    }
    if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        return Some((le16(head, 6)?, le16(head, 8)?));
    }
    if head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WEBP") {
        return match head.get(12..16)? {
            b"VP8 " => Some((le16(head, 26)? & 0x3FFF, le16(head, 28)? & 0x3FFF)),
            b"VP8L" => {
                let bits = u32::from_le_bytes(head.get(21..25)?.try_into().ok()?);
                Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
            }
            b"VP8X" => Some((le24(head, 24)? + 1, le24(head, 27)? + 1)),
            _ => None,
        };
    }
    if head.starts_with(&[0xFF, 0xD8]) {
        let mut i = 2;
        while i + 3 < head.len() {
            if head[i] != 0xFF {
                return None;
            }
            let marker = head[i + 1];
            if marker == 0xFF {
                i += 1;
                continue;
            }
            if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) {
                i += 2;
                continue;
            }
            let length = be16(head, i + 2)? as usize;
            // SOF0..SOF15, except DHT (C4), JPG (C8) and DAC (CC), carry the frame's size.
            if (0xC0..=0xCF).contains(&marker) && ![0xC4, 0xC8, 0xCC].contains(&marker) {
                return Some((be16(head, i + 7)?, be16(head, i + 5)?));
            }
            i += 2 + length;
        }
    }
    None
}

/// What the facts line says about a video or a recording (§7: *0:31 · 1920 × 1080 · 14.2 MB*).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MediaFacts {
    pub duration_ms: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// The most of a `moov` box that is read. A `moov` is the index of the file (a few KB to a few
/// hundred KB for the clips Rich makes); one larger than this is not "cheap" and is left alone.
const MOOV_CAP: u64 = 4 * MIB;
/// How many top-level boxes are stepped over looking for `moov` before giving up.
const TOP_BOXES: usize = 64;

/// **The length and frame size from an MPEG-4 container's own header** (`.mp4`, `.mov`,
/// `.m4v`, `.m4a`), §7's "from the container's header where cheap; otherwise omitted, never
/// guessed". Nothing is decoded: the top-level boxes are stepped over by their sizes (a seek
/// each, so a `moov` written after a 2 GB `mdat` costs the same as one before it), and inside
/// `moov` the movie header gives the length (`mvhd`: duration ÷ timescale) and the first track
/// header with a picture gives the size (`tkhd`: width and height in 16.16 fixed point). Any
/// other container (WebM, MP3, WAV) and any header that does not parse is all `None`.
pub fn media_facts(path: &Path) -> MediaFacts {
    let Ok(mut file) = open_nofollow(path) else { return MediaFacts::default() };
    let Ok(len) = file.seek(SeekFrom::End(0)) else { return MediaFacts::default() };
    let mut at = 0u64;
    for _ in 0..TOP_BOXES {
        if at + 8 > len || file.seek(SeekFrom::Start(at)).is_err() {
            break;
        }
        let mut head = [0u8; 16];
        if file.read_exact(&mut head[..8]).is_err() {
            break;
        }
        let small = u64::from(u32::from_be_bytes(head[0..4].try_into().unwrap()));
        let (size, header) = match small {
            0 => (len - at, 8),
            1 => {
                if file.read_exact(&mut head[8..16]).is_err() {
                    break;
                }
                (u64::from_be_bytes(head[8..16].try_into().unwrap()), 16)
            }
            n => (n, 8),
        };
        if size < header || at + size > len {
            break;
        }
        if &head[4..8] == b"moov" {
            let body = size - header;
            if body > MOOV_CAP {
                break;
            }
            let mut moov = vec![0u8; body as usize];
            if file.read_exact(&mut moov).is_err() {
                break;
            }
            return moov_facts(&moov);
        }
        at += size;
    }
    MediaFacts::default()
}

/// The children of one box's body: `(type, body)` in order. Stops at the first box whose size
/// does not fit, rather than reading past what the parent holds.
fn boxes(body: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at + 8 <= body.len() {
        let small = u32::from_be_bytes(body[at..at + 4].try_into().unwrap()) as u64;
        let kind = &body[at + 4..at + 8];
        let (size, header) = match small {
            0 => ((body.len() - at) as u64, 8usize),
            1 if at + 16 <= body.len() => (u64::from_be_bytes(body[at + 8..at + 16].try_into().unwrap()), 16),
            1 => break,
            n => (n, 8),
        };
        if size < header as u64 || size > (body.len() - at) as u64 {
            break;
        }
        let size = size as usize;
        out.push((kind, &body[at + header..at + size]));
        at += size;
    }
    out
}

fn moov_facts(moov: &[u8]) -> MediaFacts {
    let be32 = |b: &[u8], i: usize| -> Option<u64> { Some(u64::from(u32::from_be_bytes(b.get(i..i + 4)?.try_into().ok()?))) };
    let be64 = |b: &[u8], i: usize| -> Option<u64> { Some(u64::from_be_bytes(b.get(i..i + 8)?.try_into().ok()?)) };
    let mut facts = MediaFacts::default();
    for (kind, body) in boxes(moov) {
        match kind {
            b"mvhd" => {
                // version 0: creation(4) modification(4) timescale(4) duration(4) after the
                // version and flags word; version 1 widens the times and the duration to 8.
                let (scale, duration) = match body.first() {
                    Some(0) => (be32(body, 12), be32(body, 16).filter(|d| *d != u64::from(u32::MAX))),
                    Some(1) => (be32(body, 20), be64(body, 24).filter(|d| *d != u64::MAX)),
                    _ => (None, None),
                };
                if let (Some(scale), Some(duration)) = (scale, duration) {
                    if scale > 0 {
                        facts.duration_ms = duration.checked_mul(1000).map(|d| d / scale);
                    }
                }
            }
            b"trak" if facts.width.is_none() => {
                for (inner, tkhd) in boxes(body) {
                    if inner != b"tkhd" {
                        continue;
                    }
                    // width and height close the box: at 76/80 in version 0, 88/92 in version 1.
                    let at = match tkhd.first() {
                        Some(0) => 76,
                        Some(1) => 88,
                        _ => continue,
                    };
                    let (Some(w), Some(h)) = (be32(tkhd, at), be32(tkhd, at + 4)) else { continue };
                    let (w, h) = ((w >> 16) as u32, (h >> 16) as u32);
                    // A sound track's header says 0 × 0; only a picture has a size.
                    if w > 0 && h > 0 {
                        facts.width = Some(w);
                        facts.height = Some(h);
                    }
                }
            }
            _ => {}
        }
    }
    facts
}

// ---- the commands (§5.4) ------------------------------------------------------------------

#[richos_command::command]
pub fn list_output(files: State<'_, OutputFiles>, thread_id: String) -> Result<OutputList, String> {
    files.list(&thread_id)
}

#[richos_command::command]
pub fn output_file(files: State<'_, OutputFiles>, output_id: String) -> Result<EntryDetail, String> {
    files.file(&output_id)
}

#[richos_command::command]
pub fn output_preview(files: State<'_, OutputFiles>, output_id: String) -> Result<Preview, String> {
    files.preview(&output_id)
}

#[richos_command::command]
pub fn output_open(files: State<'_, OutputFiles>, output_id: String, app_index: Option<usize>) -> Result<String, String> {
    files.open(&output_id, app_index)
}

#[richos_command::command]
pub fn output_reveal(files: State<'_, OutputFiles>, output_id: String) -> Result<String, String> {
    files.reveal(&output_id)
}

/// `output_save_copy(output_id)` (§5.4, slice S6): the system save sheet, attached to the
/// window that asked, then the copy. The sheet is `tauri-plugin-dialog` run HERE, from Rust;
/// none of its commands is granted to the page (see the module header).
///
/// The sheet blocks until he answers it, so it is waited for on the blocking pool, never on
/// the main thread (the plugin's own rule for `blocking_save_file`) and never on an async
/// worker.
///
/// Every setting of the sheet, chosen rather than inherited (CEO, 2026-09-10: no third-party
/// default stands unproven):
/// - parent: the asking window, so it is a SHEET on the app, as §5.4 says;
/// - title: *Save a copy of <name>*, which says what Save will do;
/// - file name: [`suggested_name`] of the recorded name;
/// - can create folders: yes, as every Mac save sheet lets you;
/// - starting folder: NOT set. macOS then opens the sheet where this app last saved, which is
///   the system's own per-app memory of where he keeps things; a fixed folder would override
///   his last choice every time;
/// - filters: none — a copy keeps whatever extension the file has.
#[richos_command::command]
pub async fn output_save_copy(app: tauri::AppHandle, window: tauri::WebviewWindow, output_id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        use tauri_plugin_dialog::DialogExt;
        let Some(files) = app.try_state::<OutputFiles>() else { return Err(NOT_IN_RECORD.to_string()) };
        files.save_copy(&output_id, |name| {
            app.dialog()
                .file()
                .set_parent(&window)
                .set_title(format!("Save a copy of {name}"))
                .set_file_name(name)
                .set_can_create_directories(true)
                .blocking_save_file()
                .and_then(|chosen| chosen.into_path().ok())
        })
    })
    .await
    .map_err(|e| {
        eprintln!("[richos] output: the save sheet's task did not finish: {e}");
        WOULD_NOT_SAVE.to_string()
    })?
}

/// The scheme's handler, registered on the builder for every webview. The file is read off
/// the main thread; a request that reaches the app before the state is managed is a 404.
pub fn scheme_handler<R: tauri::Runtime>(
    ctx: tauri::UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: tauri::UriSchemeResponder,
) {
    use tauri::Manager;
    let app = ctx.app_handle().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let response = match app.try_state::<OutputFiles>() {
            Some(files) => files.serve(&request),
            None => empty(StatusCode::NOT_FOUND),
        };
        responder.respond(response);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use richos_core::output::{Actor, WriteRow, WriteSource};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Dir(PathBuf);
    impl Dir {
        fn new(tag: &str) -> Dir {
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            static N: AtomicUsize = AtomicUsize::new(0);
            let p = std::env::temp_dir().join(format!(
                "output-files-{tag}-{}-{nanos}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::SeqCst)
            ));
            std::fs::create_dir_all(&p).unwrap();
            // Canonical from the start: `/var` is `/private/var` on macOS.
            Dir(std::fs::canonicalize(&p).unwrap())
        }
        fn file(&self, name: &str, body: &[u8]) -> PathBuf {
            let p = self.0.join(name);
            std::fs::write(&p, body).unwrap();
            p
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            if let Err(e) = std::fs::remove_dir_all(&self.0) {
                eprintln!("test scratch {} was not removed: {e}", self.0.display());
            }
        }
    }

    const THREAD: &str = "thr_s3";
    const PNG_1280_800: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\x05\x00\0\0\x03\x20\x08\x06\0\0\0";

    fn fixed_apps(_: &Path) -> Apps {
        settle(
            Some(AppChoice { name: "TextEdit".into(), bundle_id: "com.apple.TextEdit".into() }),
            vec![
                AppChoice { name: "Xcode".into(), bundle_id: "com.apple.dt.Xcode".into() },
                AppChoice { name: "TextEdit".into(), bundle_id: "com.apple.TextEdit".into() },
                AppChoice { name: "BBEdit".into(), bundle_id: "com.barebones.bbedit".into() },
            ],
        )
    }

    fn other_apps(_: &Path) -> Apps {
        settle(None, vec![AppChoice { name: "Xcode".into(), bundle_id: "com.apple.dt.Xcode".into() }])
    }

    fn no_apps(_: &Path) -> Apps {
        Apps::default()
    }

    /// A store under `data`, a runner that records and never opens anything, and a fixed
    /// app list.
    fn files(data: &Dir) -> (OutputFiles, Arc<Mutex<Vec<Vec<String>>>>) {
        let ran = Arc::new(Mutex::new(Vec::new()));
        let seen = ran.clone();
        let files = OutputFiles::for_thread(OutputStore::for_data_dir(&data.0), THREAD, &data.0.join("cache"))
            .with_lister(fixed_apps)
            .with_runner(Arc::new(move |args: &[String]| {
                seen.lock().unwrap().push(args.to_vec());
                true
            }))
            .with_qlmanage(&data.0.join("no-qlmanage"));
        (files, ran)
    }

    /// Record one witnessed write and return its output id.
    fn record(files: &OutputFiles, path: &Path) -> String {
        let row = WriteRow::witnessed(
            format!("mach:{}", path.display()),
            THREAD,
            Some("turn_1"),
            &path.to_string_lossy(),
            Actor::Rich,
            WriteSource::Tool,
            1,
        );
        files.store.append(THREAD, &[row]).unwrap();
        let listed = files.list(THREAD).unwrap();
        listed.files.iter().find(|e| e.path == path.to_string_lossy()).unwrap().id.clone()
    }

    fn get(files: &OutputFiles, id: &str, range: Option<&str>) -> Response<Vec<u8>> {
        let mut builder = Request::builder().uri(format!("{SCHEME}://{id}?v=1"));
        if let Some(range) = range {
            builder = builder.header(header::RANGE, range);
        }
        files.serve(&builder.body(Vec::new()).unwrap())
    }

    #[test]
    fn output_files_every_refusal_has_its_sentence() {
        let data = Dir::new("data");
        let work = Dir::new("work");
        let (files, ran) = files(&data);

        // 1. An id the record does not hold, a malformed id, and no active thread at all.
        assert_eq!(files.open("out_0000000000000000", None).unwrap_err(), NOT_IN_RECORD);
        assert_eq!(files.open("../../etc/passwd", None).unwrap_err(), NOT_IN_RECORD);
        assert_eq!(files.preview("out_0000000000000000").unwrap_err(), NOT_IN_RECORD);
        assert_eq!(files.reveal("out_0000000000000000").unwrap_err(), NOT_IN_RECORD);
        let brief = work.file("brief.md", b"# Brief\n");
        let id = record(&files, &brief);
        let nobody = OutputFiles::with_active(OutputStore::for_data_dir(&data.0), Arc::new(|| None), &data.0);
        assert_eq!(nobody.open(&id, None).unwrap_err(), NOT_IN_RECORD);
        let elsewhere = OutputFiles::for_thread(OutputStore::for_data_dir(&data.0), "thr_other", &data.0);
        assert_eq!(elsewhere.file(&id).unwrap_err(), NOT_IN_RECORD, "an id resolves only in its own thread");

        // 2. A symbolic link at the recorded path: refused, Show in Finder still works.
        let target = work.file("target.md", b"elsewhere");
        let link = work.0.join("link.md");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let link_id = record(&files, &link);
        assert_eq!(files.open(&link_id, None).unwrap_err(), LINKED);
        assert_eq!(files.preview(&link_id).unwrap(), Preview::None { why: "refused", reason: LINKED.into() });
        let detail = files.file(&link_id).unwrap();
        assert_eq!((detail.previewable, detail.reason.as_deref()), ("none", Some(LINKED)));
        assert!(files.reveal(&link_id).is_ok(), "Show in Finder still works on a link");

        // 2. Not a regular file.
        let folder = work.0.join("folder.md");
        std::fs::create_dir(&folder).unwrap();
        let folder_id = record(&files, &folder);
        assert_eq!(files.open(&folder_id, None).unwrap_err(), NOT_A_FILE);
        assert_eq!(files.preview(&folder_id).unwrap(), Preview::None { why: "refused", reason: NOT_A_FILE.into() });

        // 3. Swapped: the file's folder replaced by a link to another folder holding a file of
        // the same name. `lstat` sees a regular file; the canonical path gives it away.
        let real = Dir::new("real");
        let decoy = Dir::new("decoy");
        let inside = real.file("plan.md", b"the plan");
        decoy.file("plan.md", b"not the plan");
        let swapped_id = record(&files, &inside);
        let moved = real.0.with_extension("moved");
        std::fs::rename(&real.0, &moved).unwrap();
        std::os::unix::fs::symlink(&decoy.0, &real.0).unwrap();
        assert_eq!(files.open(&swapped_id, None).unwrap_err(), LINKED);
        assert_eq!(get(&files, &swapped_id, None).status(), StatusCode::FORBIDDEN);
        std::fs::remove_file(&real.0).unwrap();
        std::fs::rename(&moved, &real.0).unwrap();

        // Missing: everything refused with the missing sentence, Show in Finder too.
        std::fs::remove_file(&brief).unwrap();
        assert_eq!(files.open(&id, None).unwrap_err(), MISSING);
        assert_eq!(files.reveal(&id).unwrap_err(), MISSING);
        assert_eq!(files.preview(&id).unwrap(), Preview::None { why: "missing", reason: MISSING.into() });
        assert_eq!(files.file(&id).unwrap().reason.as_deref(), Some(MISSING));
        assert!(!files.file(&id).unwrap().entry.exists);

        // `open` was asked exactly once: the reveal of the link. Nothing refused reached it.
        assert_eq!(*ran.lock().unwrap(), vec![vec!["-R".to_string(), link.to_string_lossy().into_owned()]]);

        // The runner failing is the opener's sentence.
        let fine = work.file("fine.md", b"ok");
        let fine_id = record(&files, &fine);
        let failing = files.with_runner(Arc::new(|_: &[String]| false));
        assert_eq!(failing.open(&fine_id, None).unwrap_err(), WOULD_NOT_OPEN);
        assert_eq!(failing.reveal(&fine_id).unwrap_err(), WOULD_NOT_OPEN);
    }

    #[test]
    fn output_files_open_and_reveal_say_what_they_did() {
        let data = Dir::new("data");
        let work = Dir::new("work");
        let (files, ran) = files(&data);
        let brief = work.file("brief.md", b"# Brief\n");
        let id = record(&files, &brief);
        assert_eq!(files.open(&id, None).unwrap(), "Opening brief.md in TextEdit.");
        let parent = brief.parent().unwrap();
        let two = format!(
            "{}/{}/",
            parent.parent().unwrap().file_name().unwrap().to_string_lossy(),
            parent.file_name().unwrap().to_string_lossy()
        );
        assert_eq!(files.reveal(&id).unwrap(), format!("Finder opens {two} with brief.md selected."));
        let path = brief.to_string_lossy().into_owned();
        assert_eq!(*ran.lock().unwrap(), vec![vec![path.clone()], vec!["-R".to_string(), path]]);
        let unnamed = files.with_lister(no_apps);
        assert_eq!(unnamed.open(&id, None).unwrap(), "Opening brief.md.", "no default app named: Open, no name (§5.4)");
    }

    #[test]
    fn output_files_the_app_index_is_recomputed_and_an_out_of_range_index_is_refused() {
        let data = Dir::new("data");
        let work = Dir::new("work");
        let (files, ran) = files(&data);
        let brief = work.file("brief.md", b"# Brief\n");
        let id = record(&files, &brief);

        // No list shown for this id yet: nothing to have chosen from.
        assert_eq!(files.open(&id, Some(0)).unwrap_err(), APPS_CHANGED);

        let detail = files.file(&id).unwrap();
        assert_eq!(detail.default_app.as_ref().unwrap().name, "TextEdit");
        // The default is not repeated, and the others are in name order.
        let names: Vec<&str> = detail.other_apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["BBEdit", "Xcode"]);

        assert_eq!(files.open(&id, Some(2)).unwrap_err(), APP_NOT_OFFERED);
        assert_eq!(files.open(&id, Some(1)).unwrap(), "Opening brief.md in Xcode.");
        assert_eq!(ran.lock().unwrap().last().unwrap(), &vec!["-b".to_string(), "com.apple.dt.Xcode".to_string(), brief.to_string_lossy().into_owned()]);

        // The Mac's list changed under the page: index 0 is now Xcode, not BBEdit. Nothing opens.
        let before = ran.lock().unwrap().len();
        let changed = files.with_lister(other_apps);
        assert_eq!(changed.open(&id, Some(0)).unwrap_err(), APPS_CHANGED);
        assert_eq!(ran.lock().unwrap().len(), before);
    }

    #[test]
    fn output_files_range_arithmetic() {
        assert_eq!(parse_range(None, 100), RangeAsk::Whole);
        assert_eq!(parse_range(Some("bytes=0-1"), 100), RangeAsk::Part(0, 1));
        assert_eq!(parse_range(Some("bytes=0-"), 100), RangeAsk::Part(0, 99));
        assert_eq!(parse_range(Some("bytes=90-"), 100), RangeAsk::Part(90, 99));
        assert_eq!(parse_range(Some("bytes=50-500"), 100), RangeAsk::Part(50, 99), "clamped to the file");
        assert_eq!(parse_range(Some("bytes=-10"), 100), RangeAsk::Part(90, 99));
        assert_eq!(parse_range(Some("bytes=-500"), 100), RangeAsk::Part(0, 99));
        assert_eq!(parse_range(Some("bytes=99-99"), 100), RangeAsk::Part(99, 99));
        assert_eq!(parse_range(Some("bytes=100-"), 100), RangeAsk::Unsatisfiable);
        assert_eq!(parse_range(Some("bytes=-0"), 100), RangeAsk::Unsatisfiable);
        assert_eq!(parse_range(Some("bytes=0-"), 0), RangeAsk::Unsatisfiable);
        assert_eq!(parse_range(Some("bytes=5-2"), 100), RangeAsk::Whole, "an invalid spec is ignored");
        assert_eq!(parse_range(Some("bytes=0-1,5-6"), 100), RangeAsk::Whole, "several ranges are ignored");
        assert_eq!(parse_range(Some("items=0-1"), 100), RangeAsk::Whole);
        assert_eq!(parse_range(Some("bytes=x-1"), 100), RangeAsk::Whole);
        // One response is at most one chunk: 8 MiB = 8,388,608 bytes, so 0..=8,388,607.
        assert_eq!(chunk(0, 10 * MIB), (0, 8_388_607));
        assert_eq!(chunk(100, 200), (100, 200));
    }

    #[test]
    fn output_files_the_scheme_serves_a_recorded_file_with_range_and_refuses_a_bad_id_with_no_bytes() {
        let data = Dir::new("data");
        let work = Dir::new("work");
        let (files, _) = files(&data);
        let mut png = PNG_1280_800.to_vec();
        png.extend((0..100u8).collect::<Vec<_>>());
        let shot = work.file("chart.png", &png);
        let id = record(&files, &shot);

        let bad = get(&files, "out_0000000000000000", None);
        assert_eq!(bad.status(), StatusCode::NOT_FOUND);
        assert!(bad.body().is_empty(), "a bad id gets no bytes");
        let malformed = get(&files, "out_not-an-id", None);
        assert_eq!(malformed.status(), StatusCode::NOT_FOUND);
        assert!(malformed.body().is_empty());

        let whole = get(&files, &id, None);
        assert_eq!(whole.status(), StatusCode::OK);
        assert_eq!(whole.body(), &png);
        assert_eq!(whole.headers()[header::CONTENT_TYPE], "image/png");
        assert_eq!(whole.headers()[header::CONTENT_LENGTH], png.len().to_string().as_str());

        let part = get(&files, &id, Some("bytes=8-15"));
        assert_eq!(part.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(part.body(), &png[8..16]);
        assert_eq!(part.headers()[header::CONTENT_RANGE], format!("bytes 8-15/{}", png.len()).as_str());

        let past = get(&files, &id, Some("bytes=9999-"));
        assert_eq!(past.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert!(past.body().is_empty());
        assert_eq!(past.headers()[header::CONTENT_RANGE], format!("bytes */{}", png.len()).as_str());

        // Text comes back through output_preview, never the scheme.
        let notes = work.file("notes.md", b"text");
        let notes_id = record(&files, &notes);
        assert_eq!(get(&files, &notes_id, None).status(), StatusCode::NOT_FOUND);

        // Media is never read whole: an unranged ask for a large video gets its first chunk.
        let video = work.0.join("clip.mp4");
        std::fs::File::create(&video).unwrap().set_len(RANGE_CHUNK + 10).unwrap();
        let video_id = record(&files, &video);
        let first = get(&files, &video_id, None);
        assert_eq!(first.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(first.body().len() as u64, RANGE_CHUNK);
        assert_eq!(first.headers()[header::CONTENT_TYPE], "video/mp4");
    }

    #[test]
    fn output_files_an_oversize_file_is_a_state_with_open_still_lit() {
        let data = Dir::new("data");
        let work = Dir::new("work");
        let (files, ran) = files(&data);
        let big = work.0.join("huge.png");
        let handle = std::fs::File::create(&big).unwrap();
        handle.set_len(IMAGE_CAP + 1).unwrap();
        let id = record(&files, &big);
        let reason = "Too large to preview here (83.9 MB). Open in TextEdit has the whole thing.";
        // 80 MiB + 1 = 83,886,081 bytes = 83.9 MB in Finder's decimal units.
        assert_eq!(files.preview(&id).unwrap(), Preview::None { why: "tooLarge", reason: reason.into() });
        let detail = files.file(&id).unwrap();
        assert_eq!((detail.previewable, detail.reason.as_deref()), ("none", Some(reason)));
        assert_eq!(get(&files, &id, None).status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert!(files.open(&id, None).is_ok(), "over the cap is not an error: Open is lit");
        assert_eq!(ran.lock().unwrap().len(), 1);
    }

    #[test]
    fn output_files_qlmanage_absence_degrades_to_no_preview() {
        let data = Dir::new("data");
        let work = Dir::new("work");
        let (files, _) = files(&data);
        let deck = work.file("deck.pptx", b"PK\x03\x04 not really a deck");
        let id = record(&files, &deck);
        let sentence = "I don't have a preview for this kind of file. Open in TextEdit has it.";
        assert_eq!(files.preview(&id).unwrap(), Preview::None { why: "noViewer", reason: sentence.into() });
        let detail = files.file(&id).unwrap();
        assert_eq!((detail.previewable, detail.reason.as_deref()), ("none", Some(sentence)));
        assert_eq!(get(&files, &id, None).status(), StatusCode::NOT_FOUND, "no rendition, no bytes");
        // `other` has no viewer either, and says so the same way.
        let blob = work.file("data.bin", b"\0\x01");
        let blob_id = record(&files, &blob);
        assert_eq!(files.preview(&blob_id).unwrap(), Preview::None { why: "noViewer", reason: sentence.into() });
    }

    #[test]
    fn output_files_a_rendition_is_cached_by_mtime_and_served_by_the_scheme() {
        let data = Dir::new("data");
        let work = Dir::new("work");
        // A stand-in qlmanage that writes `<name>.png` into `-o`, as the real one does.
        let fake = work.file("qlmanage", b"#!/bin/sh\nout=\"$5\"; src=\"$6\"; printf 'PNGDATA' > \"$out/$(basename \"$src\").png\"\n");
        std::fs::set_permissions(&fake, <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o755)).unwrap();
        let (files, _) = files(&data);
        let files = files.with_qlmanage(&fake);
        let sheet = work.file("q3.xlsx", b"PK\x03\x04");
        let id = record(&files, &sheet);
        let Preview::Rendition { url, .. } = files.preview(&id).unwrap() else { panic!("no rendition") };
        assert!(url.starts_with(&format!("{SCHEME}://{id}?v=")));
        let served = get(&files, &id, None);
        assert_eq!(served.status(), StatusCode::OK);
        assert_eq!(served.body(), b"PNGDATA");
        assert_eq!(served.headers()[header::CONTENT_TYPE], "image/png");
        let cached: Vec<_> = std::fs::read_dir(data.0.join("cache").join(PREVIEWS_DIR)).unwrap().flatten().collect();
        assert_eq!(cached.len(), 1, "one rendition, and the working folder is gone");
        // A second preview reuses it rather than running qlmanage again.
        std::fs::write(&fake, b"#!/bin/sh\nexit 1\n").unwrap();
        assert!(matches!(files.preview(&id).unwrap(), Preview::Rendition { .. }));
    }

    #[test]
    fn output_files_text_is_capped_at_a_character_boundary() {
        let data = Dir::new("data");
        let work = Dir::new("work");
        let (files, _) = files(&data);
        // 2 MiB - 1 bytes of `a`, then a two-byte `é`: the cap cuts the `é` in half.
        let mut body = vec![b'a'; (TEXT_CAP - 1) as usize];
        body.extend("é and more".as_bytes());
        let log = work.file("run.log", &body);
        let id = record(&files, &log);
        let Preview::Text { text, truncated, bytes } = files.preview(&id).unwrap() else { panic!("not text") };
        assert!(truncated);
        assert_eq!(bytes, body.len() as u64);
        assert_eq!(text.len() as u64, TEXT_CAP - 1, "the split character is dropped, not shown as a mark");
        let short = work.file("notes.md", "# Notes\n\nÉcrit.\n".as_bytes());
        let short_id = record(&files, &short);
        assert_eq!(
            files.preview(&short_id).unwrap(),
            Preview::Text { text: "# Notes\n\nÉcrit.\n".into(), truncated: false, bytes: 17 }
        );
    }

    #[test]
    fn output_files_a_csv_with_quoted_commas_parses_into_rows() {
        let text = "name,note,amount\r\n\"Acme, Inc.\",\"said \"\"hi\"\"\",12\n\"multi\nline\",,3\nlast,row,4";
        let (rows, total) = parse_csv(text, 200);
        assert_eq!(total, 4);
        assert_eq!(rows[0], ["name", "note", "amount"]);
        assert_eq!(rows[1], ["Acme, Inc.", "said \"hi\"", "12"]);
        assert_eq!(rows[2], ["multi\nline", "", "3"]);
        assert_eq!(rows[3], ["last", "row", "4"]);
        // Only the first rows are kept; every row is counted.
        let (kept, counted) = parse_csv("a\nb\nc\n", 2);
        assert_eq!((kept.len(), counted), (2, 3));

        let data = Dir::new("data");
        let work = Dir::new("work");
        let (files, _) = files(&data);
        let sheet = work.file("q3.csv", text.as_bytes());
        let id = record(&files, &sheet);
        let Preview::Table { rows, total_rows, counted_all, .. } = files.preview(&id).unwrap() else { panic!("not a table") };
        assert_eq!((rows.len(), total_rows, counted_all), (4, 4, true));
        assert_eq!(files.file(&id).unwrap().previewable, "table");
    }

    #[test]
    fn output_files_image_dimensions_come_from_the_header() {
        assert_eq!(image_size(PNG_1280_800), Some((1280, 800)));
        assert_eq!(image_size(b"GIF89a\x40\x01\xF0\x00"), Some((320, 240)));
        // JPEG: SOI, an APP0 segment, then SOF0 with height 600 and width 800.
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x4A, 0x46, 0xFF, 0xC0, 0x00, 0x11, 0x08, 0x02, 0x58, 0x03, 0x20, 0x03,
        ];
        assert_eq!(image_size(&jpeg), Some((800, 600)));
        // WebP VP8X: canvas width-1 and height-1 as 24-bit little-endian at 24 and 27.
        let mut webp = b"RIFF\0\0\0\0WEBPVP8X\x0a\0\0\0\0\0\0\0".to_vec();
        webp.extend([0x3F, 0x01, 0x00, 0xC7, 0x00, 0x00]);
        assert_eq!(image_size(&webp), Some((320, 200)));
        assert_eq!(image_size(b"<svg xmlns='http://www.w3.org/2000/svg'/>"), None);

        let data = Dir::new("data");
        let work = Dir::new("work");
        let (files, _) = files(&data);
        let shot = work.file("chart.png", PNG_1280_800);
        let id = record(&files, &shot);
        let Preview::Image { url, width, height, .. } = files.preview(&id).unwrap() else { panic!("not an image") };
        assert_eq!((width, height), (Some(1280), Some(800)));
        assert!(url.starts_with(&format!("{SCHEME}://{id}?v=")));
    }

    #[test]
    fn output_files_sizes_read_as_finder_shows_them() {
        assert_eq!(human_size(999), "999 bytes");
        assert_eq!(human_size(18_400), "18 KB");
        assert_eq!(human_size(14_200_000), "14.2 MB");
        assert_eq!(human_size(1_400_000_000), "1.4 GB");
    }

    #[test]
    fn output_files_a_rendition_older_than_seven_days_is_swept() {
        let dir = Dir::new("sweep");
        let old = dir.file("out_old-1.png", b"x");
        let fresh = dir.file("out_new-2.png", b"y");
        let eight_days = SystemTime::now() - Duration::from_secs(8 * 24 * 60 * 60);
        std::fs::File::options().write(true).open(&old).unwrap().set_modified(eight_days).unwrap();
        sweep(&dir.0, RENDITION_KEEP);
        assert!(!old.exists());
        assert!(fresh.exists());
    }

    // ---- S5: the previews (§7, §12.5) ------------------------------------------------------

    /// One MPEG-4 box: its 32-bit size, its type, its body.
    fn mp4_box(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out
    }

    /// `mvhd` version 0: timescale and duration after creation and modification times.
    fn mvhd_v0(scale: u32, duration: u32) -> Vec<u8> {
        let mut body = vec![0u8; 4 + 4 + 4];
        body.extend(scale.to_be_bytes());
        body.extend(duration.to_be_bytes());
        body.extend(vec![0u8; 80]);
        mp4_box(b"mvhd", &body)
    }

    /// `mvhd` version 1: 64-bit times and duration.
    fn mvhd_v1(scale: u32, duration: u64) -> Vec<u8> {
        let mut body = vec![1u8, 0, 0, 0];
        body.extend(vec![0u8; 16]);
        body.extend(scale.to_be_bytes());
        body.extend(duration.to_be_bytes());
        body.extend(vec![0u8; 80]);
        mp4_box(b"mvhd", &body)
    }

    /// A `trak` holding only a version-0 `tkhd` with this size (16.16 fixed point at 76/80).
    fn trak(width: u32, height: u32) -> Vec<u8> {
        let mut tkhd = vec![0u8; 76];
        tkhd.extend((width << 16).to_be_bytes());
        tkhd.extend((height << 16).to_be_bytes());
        mp4_box(b"trak", &mp4_box(b"tkhd", &tkhd))
    }

    #[test]
    fn output_files_media_facts_come_from_the_moov_header_wherever_it_sits() {
        let work = Dir::new("media");
        let ftyp = mp4_box(b"ftyp", b"isom\0\0\x02\0isomiso2mp41");
        let mdat = mp4_box(b"mdat", &vec![0xAB; 50_000]);
        // A sound track first (0 x 0, as a sound track's header says), then the picture.
        let mut moov_body = mvhd_v0(600, 18_600);
        moov_body.extend(trak(0, 0));
        moov_body.extend(trak(1920, 1080));
        let moov = mp4_box(b"moov", &moov_body);

        // moov AFTER the media data, as a recorder writes it: stepped over by a seek.
        let tail: Vec<u8> = [ftyp.clone(), mdat.clone(), moov.clone()].concat();
        let late = work.file("walkthrough.mp4", &tail);
        // 18,600 ticks / 600 per second = 31.000 s = 31,000 ms; 1920 x 1080 from the second track.
        assert_eq!(media_facts(&late), MediaFacts { duration_ms: Some(31_000), width: Some(1920), height: Some(1080) });

        // moov first ("fast start"): the same answer.
        let early = work.file("fast.mov", &[ftyp.clone(), moov, mdat.clone()].concat());
        assert_eq!(media_facts(&early).duration_ms, Some(31_000));

        // Version 1 header, audio only (.m4a): a length, no size.
        let mut audio = mvhd_v1(44_100, 44_100 * 130);
        audio.extend(trak(0, 0));
        let m4a = work.file("note.m4a", &[ftyp.clone(), mp4_box(b"moov", &audio)].concat());
        assert_eq!(media_facts(&m4a), MediaFacts { duration_ms: Some(130_000), width: None, height: None });

        // No moov, a box that claims more than the file holds, and not MPEG-4 at all: all None,
        // never a guess.
        let none = MediaFacts::default();
        assert_eq!(media_facts(&work.file("cut.mp4", &[ftyp.clone(), mdat].concat())), none);
        let mut lying = ftyp.clone();
        lying.extend(0x00FF_FFFFu32.to_be_bytes());
        lying.extend(b"moov\0\0\0\0");
        assert_eq!(media_facts(&work.file("lying.mp4", &lying)), none, "a box larger than the file is not followed");
        assert_eq!(media_facts(&work.file("junk.mp4", &[ftyp, mp4_box(b"moov", &[0xFF; 4])].concat())), none);
        assert_eq!(media_facts(&work.file("song.mp3", b"ID3\x04\0\0\0\0\0\0")), none);

        // Through the preview: the facts and the default app ride on the Video answer.
        let data = Dir::new("data");
        let (files, _) = files(&data);
        let id = record(&files, &late);
        let Preview::Video { url, bytes, duration_ms, width, height, app } = files.preview(&id).unwrap() else { panic!("not a video") };
        assert!(url.starts_with(&format!("{SCHEME}://{id}?v=")));
        assert_eq!((bytes, duration_ms, width, height), (tail.len() as u64, Some(31_000), Some(1920), Some(1080)));
        assert_eq!(app.as_deref(), Some("TextEdit"));
        let audio_id = record(&files, &m4a);
        let Preview::Audio { duration_ms, .. } = files.preview(&audio_id).unwrap() else { panic!("not audio") };
        assert_eq!(duration_ms, Some(130_000));
    }

    #[test]
    fn output_files_every_kind_in_section_7_has_its_viewer_and_its_cap() {
        // §7's table, row by row: the view each kind is previewed as and the cap it is held to.
        let table: [(&str, &str, Option<u64>); 10] = [
            ("md", "text", None),
            ("txt", "text", None),
            ("csv", "table", None),
            ("xlsx", "rendition", Some(200 * MIB)),
            ("docx", "rendition", Some(200 * MIB)),
            ("pptx", "rendition", Some(200 * MIB)),
            ("pdf", "pdf", Some(200 * MIB)),
            ("png", "image", Some(80 * MIB)),
            ("mp4", "video", None),
            ("audio", "audio", None),
        ];
        for (kind, view, cap) in table {
            assert_eq!(plan(kind), (view, cap), "{kind}");
        }
        assert_eq!(plan("other"), ("none", None), "other: no viewer, the §6.7 sentence");
        // The text kinds have their own caps, applied by the read rather than refused.
        assert_eq!((TEXT_CAP, CSV_READ_CAP, CSV_ROWS), (2 * MIB, 20 * MIB, 200));
    }

    #[test]
    fn output_files_a_pdf_and_a_document_over_their_caps_are_too_large_with_open_lit() {
        let data = Dir::new("data");
        let work = Dir::new("work");
        let (files, _) = files(&data);
        for name in ["huge.pdf", "huge.docx"] {
            let big = work.0.join(name);
            std::fs::File::create(&big).unwrap().set_len(200 * MIB + 1).unwrap();
            let id = record(&files, &big);
            // 200 MiB + 1 = 209,715,201 bytes = 209.7 MB in Finder's decimal units.
            let reason = "Too large to preview here (209.7 MB). Open in TextEdit has the whole thing.";
            assert_eq!(files.preview(&id).unwrap(), Preview::None { why: "tooLarge", reason: reason.into() }, "{name}");
            let served = get(&files, &id, None);
            assert!(served.status().is_client_error() && served.body().is_empty(), "{name}: the scheme serves no bytes over the cap");
            assert!(files.open(&id, None).is_ok(), "{name}: over the cap, Open still works");
        }
        // At the cap exactly, a PDF is served.
        let edge = work.0.join("edge.pdf");
        std::fs::File::create(&edge).unwrap().set_len(200 * MIB).unwrap();
        let edge_id = record(&files, &edge);
        assert!(matches!(files.preview(&edge_id).unwrap(), Preview::Pdf { .. }));
    }

    #[test]
    fn output_files_a_csv_shows_its_first_200_rows_and_counts_the_rest() {
        let data = Dir::new("data");
        let work = Dir::new("work");
        let (files, _) = files(&data);
        let mut text = String::from("month,revenue\n");
        for i in 0..249 {
            text.push_str(&format!("m{i},{i}\n"));
        }
        let sheet = work.file("long.csv", text.as_bytes());
        let id = record(&files, &sheet);
        let Preview::Table { rows, total_rows, counted_all, app, .. } = files.preview(&id).unwrap() else { panic!("not a table") };
        // 1 header + 249 rows = 250 read, 200 kept.
        assert_eq!((rows.len(), total_rows, counted_all), (200, 250, true));
        assert_eq!(rows[0], ["month", "revenue"]);
        assert_eq!(rows[199], ["m198", "198"]);
        assert_eq!(app.as_deref(), Some("TextEdit"), "the facts line names the app the whole sheet opens in");

        // Over the 20 MiB read: what was read is counted, and the count says it is not all.
        // 21 MiB of 8-byte rows = 2,752,512 rows on disk; at most 20 MiB / 8 = 2,621,440 read.
        let row = "abc,123\n";
        let big = work.file("big.csv", row.repeat((21 * MIB as usize) / row.len()).as_bytes());
        let big_id = record(&files, &big);
        let Preview::Table { rows, total_rows, counted_all, .. } = files.preview(&big_id).unwrap() else { panic!("not a table") };
        assert_eq!(rows.len(), 200);
        assert!(!counted_all);
        assert_eq!(total_rows as u64, CSV_READ_CAP / row.len() as u64);
    }

    #[test]
    fn output_files_a_rendition_is_made_again_when_the_file_changes() {
        let data = Dir::new("data");
        let work = Dir::new("work");
        // A stand-in qlmanage that stamps each rendition with a counter it keeps beside itself.
        let count = work.0.join("runs");
        let script = format!(
            "#!/bin/sh\nn=$(cat '{c}' 2>/dev/null || echo 0); n=$((n+1)); echo $n > '{c}'\nprintf \"PNG$n\" > \"$5/$(basename \"$6\").png\"\n",
            c = count.display()
        );
        let fake = work.file("qlmanage", script.as_bytes());
        std::fs::set_permissions(&fake, <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o755)).unwrap();
        let (files, _) = files(&data);
        let files = files.with_qlmanage(&fake);
        let deck = work.file("deck.pptx", b"PK\x03\x04 v1");
        let id = record(&files, &deck);
        let Preview::Rendition { url: first, app, .. } = files.preview(&id).unwrap() else { panic!("no rendition") };
        assert_eq!(app.as_deref(), Some("TextEdit"));
        assert_eq!(get(&files, &id, None).body(), b"PNG1");
        // Asked again with the file unchanged: the cached one, qlmanage not run.
        assert!(matches!(files.preview(&id).unwrap(), Preview::Rendition { .. }));
        assert_eq!(std::fs::read_to_string(&count).unwrap().trim(), "1");

        // The file is written again (a new modification time): a new rendition, a new URL.
        let later = SystemTime::now() + Duration::from_secs(120);
        std::fs::File::options().write(true).open(&deck).unwrap().set_modified(later).unwrap();
        let Preview::Rendition { url: second, .. } = files.preview(&id).unwrap() else { panic!("no rendition") };
        assert_ne!(first, second, "the ?v= cache-buster follows the modification time");
        assert_eq!(get(&files, &id, None).body(), b"PNG2", "the scheme serves the new one");
        assert_eq!(std::fs::read_to_string(&count).unwrap().trim(), "2");
    }

    // ---- slice S6: Save a copy…, and the dialog plugin's posture -------------------------------

    fn listing(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn output_files_a_saved_copy_lands_where_the_sheet_pointed_with_the_name_it_gave() {
        use std::os::unix::fs::PermissionsExt;
        let data = Dir::new("data");
        let work = Dir::new("work");
        let elsewhere = Dir::new("copies");
        let (files, ran) = files(&data);
        let body = b"# Brief\n\nHold at list minus 3%.\n";
        let brief = work.file("brief.md", body);
        std::fs::set_permissions(&brief, std::fs::Permissions::from_mode(0o640)).unwrap();
        let id = record(&files, &brief);
        let two = |p: &Path| {
            let parent = p.parent().unwrap();
            format!("{}/{}/", parent.parent().unwrap().file_name().unwrap().to_string_lossy(), parent.file_name().unwrap().to_string_lossy())
        };

        // The sheet opens with the file's own name and answers a different one: the copy has it.
        let offered = std::cell::RefCell::new(String::new());
        let renamed = elsewhere.0.join("brief copy.md");
        let said = files
            .save_copy(&id, |name| {
                *offered.borrow_mut() = name.to_string();
                Some(renamed.clone())
            })
            .unwrap();
        assert_eq!(*offered.borrow(), "brief.md", "the sheet is offered the file's own name");
        assert_eq!(said, format!("Saved a copy of brief.md to {} as brief copy.md.", two(&renamed)));
        assert_eq!(std::fs::read(&renamed).unwrap(), body);
        assert_eq!(std::fs::metadata(&renamed).unwrap().permissions().mode() & 0o777, 0o640, "the copy keeps the file's permissions");
        assert_eq!(listing(&elsewhere.0), ["brief copy.md"], "nothing part-written is left beside it");
        assert_eq!(std::fs::read(&brief).unwrap(), body, "the original is untouched");

        // The same name somewhere else, over a file that is already there (the sheet asked him).
        let same = elsewhere.0.join("brief.md");
        std::fs::write(&same, b"an older brief").unwrap();
        assert_eq!(files.save_copy(&id, |_| Some(same.clone())).unwrap(), format!("Saved a copy of brief.md to {}.", two(&same)));
        assert_eq!(std::fs::read(&same).unwrap(), body, "the copy replaces what the sheet agreed to replace");

        // Cancel: nothing written, and the action says so.
        assert_eq!(files.save_copy(&id, |_| None).unwrap(), NOTHING_SAVED);
        assert_eq!(listing(&elsewhere.0), ["brief copy.md", "brief.md"]);

        // The sheet pointed at the file itself — by its path, or by a link to it: refused before
        // a byte is written, because a copy onto itself truncates the original first.
        assert_eq!(files.save_copy(&id, |_| Some(brief.clone())).unwrap_err(), SAME_FILE);
        let alias = elsewhere.0.join("alias.md");
        std::os::unix::fs::symlink(&brief, &alias).unwrap();
        assert_eq!(files.save_copy(&id, |_| Some(alias.clone())).unwrap_err(), SAME_FILE);
        assert_eq!(std::fs::read(&brief).unwrap(), body, "the original survives being chosen as its own copy");

        // A folder that is not there: the OS's own words, and nothing left behind.
        let nowhere = elsewhere.0.join("no-such-folder").join("brief.md");
        let refused = files.save_copy(&id, |_| Some(nowhere.clone())).unwrap_err();
        assert!(refused.starts_with("I couldn't save the copy: "), "{refused}");
        assert_eq!(files.save_copy(&id, |_| Some(PathBuf::from("brief.md"))).unwrap_err(), NOT_A_PLACE);

        assert!(ran.lock().unwrap().is_empty(), "saving a copy runs /usr/bin/open never");
    }

    #[test]
    fn output_files_save_a_copy_refuses_before_the_sheet_and_again_after_it() {
        let data = Dir::new("data");
        let work = Dir::new("work");
        let copies = Dir::new("copies");
        let (files, _) = files(&data);
        let asked = std::cell::Cell::new(0);
        let dest = copies.0.join("copy.md");
        let sheet = |_: &str| {
            asked.set(asked.get() + 1);
            Some(dest.clone())
        };

        // An id the record does not hold: no sheet.
        assert_eq!(files.save_copy("out_0000000000000000", sheet).unwrap_err(), NOT_IN_RECORD);
        // A missing file: its sentence, no sheet.
        let gone = work.file("gone.md", b"x");
        let gone_id = record(&files, &gone);
        std::fs::remove_file(&gone).unwrap();
        assert_eq!(files.save_copy(&gone_id, sheet).unwrap_err(), MISSING);
        // A link: its sentence, no sheet.
        let target = work.file("target.md", b"elsewhere");
        let link = work.0.join("link.md");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let link_id = record(&files, &link);
        assert_eq!(files.save_copy(&link_id, sheet).unwrap_err(), LINKED);
        assert_eq!(asked.get(), 0, "the sheet was shown for a file that cannot be copied");

        // The file goes while the sheet is open: checked again, refused, nothing written.
        let brief = work.file("brief.md", b"# Brief\n");
        let id = record(&files, &brief);
        let dest = copies.0.join("brief.md");
        let vanished = files.save_copy(&id, |_| {
            std::fs::remove_file(&brief).unwrap();
            Some(dest.clone())
        });
        assert_eq!(vanished.unwrap_err(), MISSING);
        assert!(listing(&copies.0).is_empty(), "a copy was written of a file that was gone");
    }

    #[test]
    fn output_files_the_save_sheet_is_offered_a_sanitized_name() {
        assert_eq!(suggested_name("brief.md"), "brief.md");
        assert_eq!(suggested_name("counter-draft v1.docx"), "counter-draft v1.docx");
        assert_eq!(suggested_name(".env"), "env", "a leading dot would hide the copy");
        assert_eq!(suggested_name("..."), "copy");
        assert_eq!(suggested_name("a/b\\c:report.pdf"), "report.pdf", "only the last component");
        assert_eq!(suggested_name("\u{7}bell\u{1b}.txt"), "bell.txt", "control characters go");
        assert_eq!(suggested_name("invoice\u{202E}fdp.exe"), "invoicefdp.exe", "a direction override cannot disguise the extension");
        let long = format!("{}.md", "é".repeat(80));
        let kept = suggested_name(&long);
        assert!(kept.ends_with(".md") && kept.len() <= 103, "the stem is capped at 100 bytes: {} bytes", kept.len());
        assert!(kept.trim_end_matches(".md").chars().all(|c| c == 'é'), "cut at a character boundary");
    }

    /// THE DIALOG PLUGIN IS RUST'S, NEVER THE PAGE'S (§5.4, §12.6 "Done when"). The plugin is
    /// registered in the builder, so the only thing standing between the webview and a file
    /// picker is that no capability grants its commands. This reads every capability file the
    /// app ships and the config's own list, and fails on any `dialog:` permission.
    #[test]
    fn output_files_the_dialog_plugin_is_never_granted_to_the_webview() {
        fn identifiers(value: &serde_json::Value, out: &mut Vec<String>) {
            match value {
                serde_json::Value::String(s) => out.push(s.clone()),
                serde_json::Value::Object(map) => {
                    if let Some(id) = map.get("identifier").and_then(|v| v.as_str()) {
                        out.push(id.to_string());
                    }
                }
                _ => {}
            }
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut granted = Vec::new();
        let mut read = 0;
        for entry in std::fs::read_dir(root.join("capabilities")).unwrap().flatten() {
            let text = std::fs::read_to_string(entry.path()).unwrap();
            let json: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", entry.path().display()));
            for p in json["permissions"].as_array().cloned().unwrap_or_default() {
                identifiers(&p, &mut granted);
            }
            read += 1;
        }
        assert_eq!(read, 1, "a capability file was added; this test must read it on purpose");
        assert_eq!(granted, ["core:default"], "capabilities/default.json is not what it was: {granted:?}");
        let conf: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(root.join("tauri.conf.json")).unwrap()).unwrap();
        let inline = conf["app"]["security"]["capabilities"].as_array().cloned().unwrap_or_default();
        let inline_text = serde_json::to_string(&inline).unwrap();
        assert!(!inline_text.contains("dialog"), "tauri.conf.json grants a dialog permission inline: {inline_text}");

        // The plugin replaces `window.alert` and `window.confirm` with its refused commands, so
        // a page that called either would silently get nothing (module header).
        let ui = root.join("../ui");
        for entry in std::fs::read_dir(&ui).unwrap().flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("js") {
                continue;
            }
            for (n, line) in std::fs::read_to_string(&path).unwrap().lines().enumerate() {
                let code = line.trim_start();
                if code.starts_with("//") || code.starts_with('*') {
                    continue;
                }
                for call in ["alert(", "confirm("] {
                    let hit = code.match_indices(call).any(|(at, _)| {
                        at == 0 || !code[..at].ends_with(|c: char| c.is_alphanumeric() || c == '_' || c == '-')
                    });
                    assert!(!hit, "{}:{} calls {call}…) — refused under the dialog plugin: {line}", path.display(), n + 1);
                }
            }
        }
    }

    #[test]
    fn output_files_a_missing_or_linked_file_names_its_problem_for_the_panel() {
        let data = Dir::new("data");
        let work = Dir::new("work");
        let (files, _) = files(&data);
        let fine = work.file("fine.md", b"ok");
        let fine_id = record(&files, &fine);
        assert_eq!(files.file(&fine_id).unwrap().problem, None);
        let big = work.0.join("huge.png");
        std::fs::File::create(&big).unwrap().set_len(IMAGE_CAP + 1).unwrap();
        let big_id = record(&files, &big);
        assert_eq!(files.file(&big_id).unwrap().problem, None, "too large to preview is not a problem: Open is lit");
        let target = work.file("target.md", b"x");
        let link = work.0.join("link.md");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let link_id = record(&files, &link);
        assert_eq!(files.file(&link_id).unwrap().problem, Some("refused"));
        std::fs::remove_file(&fine).unwrap();
        assert_eq!(files.file(&fine_id).unwrap().problem, Some("missing"));
        let json = serde_json::to_value(files.file(&fine_id).unwrap()).unwrap();
        assert_eq!(json["problem"], "missing", "serialized for the page as `problem`");
    }

    #[test]
    fn output_files_the_preview_url_parses_back_to_its_id() {
        let uri: tauri::http::Uri = format!("{SCHEME}://out_0123456789abcdef?v=1759660800123").parse().unwrap();
        assert_eq!(uri.host(), Some("out_0123456789abcdef"));
        assert!(usable_output_id("out_0123456789abcdef"));
        assert!(!usable_output_id("out_0123456789abcdeg"));
        assert!(!usable_output_id("out_0123"));
    }
}

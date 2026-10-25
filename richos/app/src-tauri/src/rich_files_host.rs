//! **A FILE RICH PUT IN FRONT OF THE USER, on the Mac** (§138, build plan item 12; round 36
//! screen 3). The record and the tool are `richos-core`'s `rich_files.rs`; this file is the
//! window's half: the cards' items in the conversation, the two clicks (open, Show in Finder),
//! and the watch that shows a file the moment it is attached.
//!
//! Opening goes through `opener.rs`, which resolves the attachment id against the conversation's
//! record and opens the file with the Mac's default app. The page holds ids, never paths.

use crate::AppState;
use richos_core::read_view::SpineView;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};

/// The engine state the tool writes under: the question store's own root.
pub fn root(state: &AppState) -> PathBuf {
    state.data_dir.join("engine-state")
}

/// The conversation's file cards as timeline items, each naming the app the Mac would open it
/// with (`Open in Preview`), read from Launch Services at the moment the card is drawn.
pub fn items(root: &Path, entity_id: &str, thread_id: &str) -> Vec<Value> {
    richos_core::rich_files::list(root, entity_id, thread_id)
        .iter()
        .map(item)
        .collect()
}

/// The conversation's file cards added to Techy Mode's technical view, `get_machinery`'s
/// `{"timeline": {"items": [...]}}`: the same rows `question_host::payload` adds to the calm view,
/// so a reload in either mode keeps the cards (review rv-20261021T083350Z-8ca9bacf-e592: with
/// Techy Mode on they vanished at every turn's end and on reopening). Appended, not re-sorted: the
/// technical rows keep the projection's own order, and the window places each card by its turn.
/// An unbound conversation has no cards.
pub fn machinery_with_files(view: &dyn SpineView, files_root: &Path, thread_id: &str) -> Result<Value, String> {
    let mut payload = crate::machinery_view::machinery_payload(view, thread_id)?;
    if let Ok(binding) = view.ledger().thread_binding(thread_id) {
        if let Some(items) = payload["timeline"]["items"].as_array_mut() {
            items.extend(self::items(files_root, binding.entity_id().as_str(), thread_id));
            items.extend(richos_core::your_instructions::items(files_root, binding.entity_id().as_str(), thread_id));
        }
    }
    Ok(payload)
}

/// **The "Your instructions · Saved" cards** (round 36 screen 1, `your_instructions.rs`) added to
/// the calm view's snapshot for the WINDOW, `get_timeline`. Not in `question_host::payload_in`,
/// which the phone reads too: the phone has no instructions sheet for the card's button to open,
/// so it gets no card. Placed by `createdAt`, as the payload's other rows are.
pub fn with_instruction_cards(view: &dyn SpineView, files_root: &Path, thread_id: &str, mut payload: Value) -> Value {
    if let Ok(binding) = view.ledger().thread_binding(thread_id) {
        if let Some(items) = payload["items"].as_array_mut() {
            items.extend(richos_core::your_instructions::items(files_root, binding.entity_id().as_str(), thread_id));
            items.sort_by_key(|i| i["createdAt"].as_u64().unwrap_or(0));
        }
    }
    payload
}

/// One card's item, as the window and the phone get it (the phone lab, `mobile/dev/mac-server.rs`,
/// sends the same).
pub(crate) fn item(row: &richos_core::rich_files::Row) -> Value {
    let mut item = richos_core::rich_files::item(row);
    if item["file"]["exists"] == true {
        if let Some(app) = crate::output_files::default_app_name(Path::new(&row.path)) {
            item["file"]["app"] = json!(app);
        }
        if item["file"]["ext"] == "PDF" {
            if let Some(pages) = pdf_pages(Path::new(&row.path)) {
                richos_core::rich_files::add_pages(&mut item["file"], pages);
            }
        }
    }
    item
}

/// A PDF's page count for its card ("PDF · 184 KB · 2 pages", round 36 screen 3), as the Mac's
/// own PDF reader counts it (Core Graphics, the engine Preview draws with), or `None` for a file
/// it cannot read: locked, damaged, or not a PDF after all. Remembered by path, size and
/// modification time, because every reload of the conversation draws the card again.
pub fn pdf_pages(path: &Path) -> Option<usize> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    type Key = (PathBuf, u64, Option<std::time::SystemTime>);
    static SEEN: OnceLock<Mutex<HashMap<Key, Option<usize>>>> = OnceLock::new();
    let meta = std::fs::metadata(path).ok()?;
    let key: Key = (path.to_path_buf(), meta.len(), meta.modified().ok());
    let seen = SEEN.get_or_init(Default::default);
    if let Some(known) = seen.lock().ok().and_then(|s| s.get(&key).copied()) {
        return known;
    }
    let pages = pdf::pages(path);
    if let Ok(mut s) = seen.lock() {
        if s.len() >= 512 {
            s.clear();
        }
        s.insert(key, pages);
    }
    pages
}

#[cfg(target_os = "macos")]
mod pdf {
    use std::ffi::c_void;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFURLCreateFromFileSystemRepresentation(alloc: *const c_void, buffer: *const u8, len: isize, is_directory: u8) -> *const c_void;
        fn CFRelease(cf: *const c_void);
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPDFDocumentCreateWithURL(url: *const c_void) -> *mut c_void;
        fn CGPDFDocumentGetNumberOfPages(document: *mut c_void) -> usize;
        fn CGPDFDocumentRelease(document: *mut c_void);
    }

    pub fn pages(path: &Path) -> Option<usize> {
        let bytes = path.as_os_str().as_bytes();
        // SAFETY: documented C entry points of frameworks this process already links. The URL is
        // made from a byte slice that outlives the call and released here; the document is made
        // here, read once, and released here on every path. A NULL from either create is checked
        // before use.
        unsafe {
            let url = CFURLCreateFromFileSystemRepresentation(std::ptr::null(), bytes.as_ptr(), bytes.len() as isize, 0);
            if url.is_null() {
                return None;
            }
            let document = CGPDFDocumentCreateWithURL(url);
            CFRelease(url);
            if document.is_null() {
                return None;
            }
            let pages = CGPDFDocumentGetNumberOfPages(document);
            CGPDFDocumentRelease(document);
            (pages > 0).then_some(pages)
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod pdf {
    pub fn pages(_path: &std::path::Path) -> Option<usize> {
        None
    }
}

/// Open the file with the Mac's default app for it.
#[richos_command::command]
pub fn rich_file_open(state: tauri::State<AppState>, thread_id: String, file_id: String) -> Result<(), String> {
    crate::opener::open_attached(&root(&state), &thread_id, &file_id)
}

/// Show the file in a Finder window.
#[richos_command::command]
pub fn rich_file_reveal(state: tauri::State<AppState>, thread_id: String, file_id: String) -> Result<(), String> {
    crate::opener::reveal_attached(&root(&state), &thread_id, &file_id)
}

/// Watch the record: each newly attached file goes to the window (`rich://rich-file`) and to the
/// phone's stream at once, so a file Stu attaches between turns shows without a reload.
pub fn start(app: AppHandle) {
    let directory = root(&app.state::<AppState>()).join(richos_core::rich_files::DIR);
    if let Err(error) = std::fs::create_dir_all(&directory) {
        eprintln!("[richos] the file cards cannot be watched: {error}");
        return;
    }
    let (changed, events) = std::sync::mpsc::sync_channel::<()>(1);
    std::thread::Builder::new()
        .name("richos-rich-files".into())
        .spawn(move || {
            let mut watcher = match notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if event.is_ok_and(|e| !e.kind.is_access()) {
                    changed.try_send(()).unwrap_or(());
                }
            }) {
                Ok(watcher) => watcher,
                Err(error) => {
                    eprintln!("[richos] the file cards cannot be watched: {error}");
                    return;
                }
            };
            if let Err(error) = notify::Watcher::watch(&mut watcher, &directory, notify::RecursiveMode::NonRecursive) {
                eprintln!("[richos] the file cards cannot be watched: {error}");
                return;
            }
            let state_root = directory.parent().map(Path::to_path_buf).unwrap_or_default();
            let mut seen: std::collections::HashSet<String> =
                richos_core::rich_files::all(&state_root).into_iter().map(|row| row.id).collect();
            // A save of the user's instructions (`your_instructions.rs`) is written to this same
            // folder, so its card shows at once too: the window only, never the phone's stream.
            seen.extend(richos_core::your_instructions::all(&state_root).into_iter().map(|card| card.id));
            for () in events {
                for card in richos_core::your_instructions::all(&state_root) {
                    if seen.insert(card.id.clone()) {
                        drop(app.emit("rich://rich-file", &richos_core::your_instructions::item(&card)));
                    }
                }
                for row in richos_core::rich_files::all(&state_root) {
                    if seen.insert(row.id.clone()) {
                        let item = item(&row);
                        drop(app.emit("rich://rich-file", &item));
                        app.state::<std::sync::Arc<crate::phone::PhoneRuntime>>().question_changed(&item);
                    }
                }
            }
        })
        .map(|_| ())
        .unwrap_or_else(|error| eprintln!("[richos] the file cards cannot be watched: {error}"));
}

#[cfg(test)]
mod tests {
    use richos_core::entity::{Entity, EntityId, EntityRegistry};
    use richos_core::ledger::Ledger;
    use richos_core::spine::Spine;

    /// **BOTH SNAPSHOT PATHS CARRY THE CARDS** (review rv-20261021T083350Z-8ca9bacf-e592): the
    /// calm view's (`question_host::payload_in`, `get_timeline` and the phone) and Techy Mode's
    /// (`machinery_with_files`, `get_machinery`), each over a real spine's published view. Before
    /// the fix Techy Mode's had none, so every reload after a turn took the cards away.
    #[test]
    fn a_file_rich_attached_is_in_the_calm_and_the_techy_snapshot_alike() {
        let dir = std::env::temp_dir().join(format!("richos-rich-files-paths-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).unwrap();
        }
        std::fs::create_dir_all(&dir).unwrap();
        let mut spine = Spine::new(Ledger::open(dir.join("ledger.jsonl")).unwrap());
        spine.set_entity_registry(EntityRegistry::new(vec![Entity::new("alpha", "Alpha", &["/fixture/alpha"]).unwrap()]).unwrap());
        let thread = spine.create_thread("A conversation", &EntityId::parse("alpha").unwrap()).unwrap();
        let root = dir.join("engine-state");
        let file = dir.join("counter.pdf");
        std::fs::write(&file, b"%PDF-1.4").unwrap();
        let row = richos_core::rich_files::attach(&root, "alpha", &thread, "turn-1", "front_desk", &file.to_string_lossy()).unwrap();
        let view = spine.reader().snapshot();
        let has_card = |items: &serde_json::Value| {
            items.as_array().unwrap().iter().any(|i| i["kind"] == "rich_file" && i["file"]["id"] == row.id.as_str())
        };
        let calm = crate::question_host::payload_in(&*view, &richos_core::questions::Store::new(&root), &root, &thread).unwrap();
        assert!(has_card(&calm["items"]), "the calm snapshot has the card: {calm}");
        let techy = super::machinery_with_files(&*view, &root, &thread).unwrap();
        assert!(has_card(&techy["timeline"]["items"]), "Techy Mode's snapshot has the card: {techy}");
        // Another company's conversation never sees it.
        assert_eq!(richos_core::rich_files::items(&root, "beta", &thread).len(), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A PDF written the way every PDF writer lays one out: header, objects, cross-reference table
    /// with each object's byte offset, trailer. `pages` blank Letter pages.
    fn pdf(pages: usize) -> Vec<u8> {
        let mut objects = vec!["<< /Type /Catalog /Pages 2 0 R >>".to_string()];
        let kids: Vec<String> = (0..pages).map(|i| format!("{} 0 R", 3 + i)).collect();
        objects.push(format!("<< /Type /Pages /Kids [{}] /Count {pages} >>", kids.join(" ")));
        objects.extend((0..pages).map(|_| "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] >>".to_string()));
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, object) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for offset in offsets {
            out.extend(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
        out
    }

    /// **A PDF'S CARD SAYS ITS PAGES** (round 36 screen 3, "PDF · 184 KB · 2 pages"), counted by
    /// the Mac's own PDF reader; a file it cannot read as a PDF keeps the card without a count, and
    /// a changed file is counted again.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_pdf_card_counts_its_pages_with_the_macs_own_reader() {
        let dir = std::env::temp_dir().join(format!("richos-rich-files-pages-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).unwrap();
        }
        std::fs::create_dir_all(&dir).unwrap();
        let root = dir.join("engine-state");
        let file = dir.join("Acme-counteroffer.pdf");
        std::fs::write(&file, pdf(2)).unwrap();
        let row = richos_core::rich_files::attach(&root, "alpha", "t1", "turn-1", "front_desk", &file.to_string_lossy()).unwrap();
        let card = super::item(&row);
        assert_eq!(card["file"]["pages"], 2, "{card}");
        let meta = card["file"]["meta"].as_str().unwrap();
        assert!(meta.starts_with("PDF · ") && meta.ends_with(" · 2 pages"), "{meta}");
        // Rewritten with three pages: counted again, not remembered.
        std::fs::write(&file, pdf(3)).unwrap();
        assert!(super::item(&row)["file"]["meta"].as_str().unwrap().ends_with(" · 3 pages"));
        // Not a PDF after all: the card as before, with no count.
        std::fs::write(&file, b"%PDF-1.4 truncated, nothing after this").unwrap();
        let broken = super::item(&row);
        assert_eq!(broken["file"]["pages"], serde_json::Value::Null);
        assert_eq!(broken["file"]["meta"], "PDF · 38 bytes");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Both commands answer off Tauri's async workers (`crate::command_rule_tests`).
    #[test]
    fn its_two_commands_run_off_the_async_workers() {
        let source = include_str!("rich_files_host.rs");
        assert_eq!(crate::command_rule_tests::bare_tauri_attributes(source), Vec::<String>::new());
        let ours = source.lines().filter(|line| line.trim() == "#[richos_command::command]").count();
        assert_eq!(ours, 2, "rich_file_open, rich_file_reveal");
    }
}

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
        }
    }
    Ok(payload)
}

fn item(row: &richos_core::rich_files::Row) -> Value {
    let mut item = richos_core::rich_files::item(row);
    if item["file"]["exists"] == true {
        if let Some(app) = crate::output_files::default_app_name(Path::new(&row.path)) {
            item["file"]["app"] = json!(app);
        }
    }
    item
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
            for () in events {
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

    /// Both commands answer off Tauri's async workers (`crate::command_rule_tests`).
    #[test]
    fn its_two_commands_run_off_the_async_workers() {
        let source = include_str!("rich_files_host.rs");
        assert_eq!(crate::command_rule_tests::bare_tauri_attributes(source), Vec::<String>::new());
        let ours = source.lines().filter(|line| line.trim() == "#[richos_command::command]").count();
        assert_eq!(ours, 2, "rich_file_open, rich_file_reveal");
    }
}

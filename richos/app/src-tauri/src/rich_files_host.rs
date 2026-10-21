//! **A FILE RICH PUT IN FRONT OF THE USER, on the Mac** (§138, build plan item 12; round 36
//! screen 3). The record and the tool are `richos-core`'s `rich_files.rs`; this file is the
//! window's half: the cards' items in the conversation, the two clicks (open, Show in Finder),
//! and the watch that shows a file the moment it is attached.
//!
//! Opening goes through `opener.rs`, which resolves the attachment id against the conversation's
//! record and opens the file with the Mac's default app. The page holds ids, never paths.

use crate::AppState;
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
    /// Both commands answer off Tauri's async workers (`crate::command_rule_tests`).
    #[test]
    fn its_two_commands_run_off_the_async_workers() {
        let source = include_str!("rich_files_host.rs");
        assert_eq!(crate::command_rule_tests::bare_tauri_attributes(source), Vec::<String>::new());
        let ours = source.lines().filter(|line| line.trim() == "#[richos_command::command]").count();
        assert_eq!(ours, 2, "rich_file_open, rich_file_reveal");
    }
}

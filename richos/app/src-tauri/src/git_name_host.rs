//! **Your name in Git, on the Mac** (richos-hq design round 41, piece 2). The answer, the ask and
//! the save that waits are `richos-core`'s `git_name.rs`; this file is the window's half: the card
//! in the conversation it was asked in, the Settings row's state, saving the answer, and the watch
//! that shows the card the moment the team's first save asks.

use crate::AppState;
use richos_core::git_name;
use richos_core::read_view::SpineView;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};

/// The engine state the answer is kept under: the question store's own root.
pub fn root(state: &AppState) -> PathBuf {
    state.data_dir.join("engine-state")
}

/// The name the card's field starts with: Settings' "Your name" when it is set, else the Mac
/// account's full name.
fn suggested(state: &AppState) -> Option<String> {
    let set = state.config.lock().ok().and_then(|c| c.user_name().map(str::to_string));
    set.or_else(git_name::account_name)
}

/// The conversation's card as a timeline item, or `None` when it was asked elsewhere or never.
fn card(state: &AppState, thread_id: &str) -> Option<Value> {
    let view = state.reader.snapshot();
    let binding = view.ledger().thread_binding(thread_id).ok()?;
    git_name::item(&root(state), binding.entity_id().as_str(), thread_id, suggested(state).as_deref())
}

/// `get_timeline`'s snapshot with the conversation's card in its place in time.
pub fn with_card(state: &AppState, thread_id: &str, payload: Result<Value, String>) -> Result<Value, String> {
    let mut payload = payload?;
    if let (Some(item), Some(items)) = (card(state, thread_id), payload["items"].as_array_mut()) {
        let at = item["createdAt"].as_u64().unwrap_or(0);
        let place = items.iter().position(|i| i["createdAt"].as_u64().unwrap_or(0) > at).unwrap_or(items.len());
        items.insert(place, item);
    }
    Ok(payload)
}

/// Techy Mode's snapshot (`get_machinery`) with the card, so a reload in either mode keeps it.
pub fn with_card_in_machinery(state: &AppState, thread_id: &str, payload: Result<Value, String>) -> Result<Value, String> {
    let mut payload = payload?;
    if let (Some(item), Some(items)) = (card(state, thread_id), payload["timeline"]["items"].as_array_mut()) {
        items.push(item);
    }
    Ok(payload)
}

/// The open card counts as an open question in its conversation (the rail's count).
pub fn count_open(state: &AppState, counts: &mut serde_json::Map<String, Value>) {
    let root = root(state);
    if git_name::choice(&root).is_some() { return; }
    let Some(asked) = git_name::asked(&root) else { return };
    if let Some(n) = counts.get_mut(&asked.thread_id) {
        *n = json!(n.as_u64().unwrap_or(0) + 1);
    }
}

/// **What the Settings row and its sheet show.** The user's own Git setup first: when it names
/// them, that is the name, read only. Otherwise the app's answer, or nothing yet.
fn status(state: &AppState) -> Value {
    let login = richos_core::login_environment::current();
    let path = login.get("PATH").cloned().unwrap_or_else(|| "/usr/bin:/bin:/usr/sbin:/sbin".into());
    let home = login.get("HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(PathBuf::from));
    // The runtime's Git reads `~/.gitconfig` when the user has no Git of their own on PATH. Apple's
    // stand-in `/usr/bin/git` is never run here (it asks to install the developer tools).
    let runtime = state.engine_dir.lock().ok().map(|d| d.clone()).and_then(|d| richos_core::runtime::verify_engine(&d).ok());
    let own = runtime.as_ref().and_then(|r| richos_core::user_git::identity(&path, home.as_deref(), &r.root, &r.git));
    if let Some(own) = own {
        return json!({"source": "git", "name": own.name, "email": own.email, "file": shown(&own.file, home.as_deref())});
    }
    let (choice, name, email) = match git_name::choice(&root(state)) {
        Some(git_name::Choice::Own { name, email }) => ("own", Some(name), Some(email)),
        Some(git_name::Choice::Richos) => ("richos", None, None),
        None => ("none", None, None),
    };
    json!({"source": "app", "choice": choice, "name": name, "email": email, "suggested": suggested(state)})
}

/// A path the way the sheet writes it: the home folder as `~`.
fn shown(file: &Path, home: Option<&Path>) -> String {
    match home.and_then(|h| file.strip_prefix(h).ok()) {
        Some(rest) => format!("~/{}", rest.display()),
        None => file.display().to_string(),
    }
}

/// Tell the window the card changed: asked, answered.
fn announce(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Some(asked) = git_name::asked(&root(&state)) else { return };
    if let Some(item) = card(&state, &asked.thread_id) {
        drop(app.emit("rich://git-name", &item));
    }
}

#[richos_command::command]
pub fn git_name_status(state: tauri::State<AppState>) -> Result<Value, String> {
    Ok(status(&state))
}

/// Save the user's name and email. A name or email that cannot be saved is refused with its
/// reason (`no_name`, `no_email`, `email`), and nothing changes.
#[richos_command::command]
pub fn git_name_save(app: AppHandle, state: tauri::State<AppState>, name: String, email: String) -> Result<Value, String> {
    if let Err(problem) = git_name::check(&name, &email) {
        return Err(serde_json::to_value(problem).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default());
    }
    git_name::save(&root(&state), &name, &email)?;
    announce(&app);
    Ok(status(&state))
}

/// Keep the name RichOS: answered, and never asked again.
#[richos_command::command]
pub fn git_name_use_richos(app: AppHandle, state: tauri::State<AppState>) -> Result<Value, String> {
    git_name::use_richos(&root(&state))?;
    announce(&app);
    Ok(status(&state))
}

/// Open the user's own Git setup file with the Mac's default app for it, or show it in Finder.
/// The file is the one the user's Git names, read again here: the page never names a path.
#[richos_command::command]
pub fn git_name_open_file(state: tauri::State<AppState>, reveal: bool) -> Result<(), String> {
    let login = richos_core::login_environment::current();
    let path = login.get("PATH").cloned().unwrap_or_default();
    let home = login.get("HOME").map(PathBuf::from).or_else(|| std::env::var_os("HOME").map(PathBuf::from));
    let runtime = state.engine_dir.lock().ok().map(|d| d.clone()).and_then(|d| richos_core::runtime::verify_engine(&d).ok())
        .ok_or("RichOS could not find your Git setup file.")?;
    let own = richos_core::user_git::identity(&path, home.as_deref(), &runtime.root, &runtime.git)
        .ok_or("RichOS could not find your Git setup file.")?;
    let file = own.file.to_string_lossy().into_owned();
    let argv: Vec<&str> = if reveal { vec!["/usr/bin/open", "-R", &file] } else { vec!["/usr/bin/open", &file] };
    let status = std::process::Command::new(argv[0]).args(&argv[1..]).status().map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err("RichOS could not open your Git setup file.".into()) }
}

/// Watch the answer's folder: the team's first save asking, and the answer, reach the window at
/// once (`rich://git-name`).
pub fn start(app: AppHandle) {
    let directory = git_name::dir(&root(&app.state::<AppState>()));
    if let Err(error) = std::fs::create_dir_all(&directory) {
        eprintln!("[richos] the Git name question cannot be watched: {error}");
        return;
    }
    let (changed, events) = std::sync::mpsc::sync_channel::<()>(1);
    std::thread::Builder::new()
        .name("richos-git-name".into())
        .spawn(move || {
            let mut watcher = match notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                if event.is_ok_and(|e| !e.kind.is_access()) {
                    changed.try_send(()).unwrap_or(());
                }
            }) {
                Ok(watcher) => watcher,
                Err(error) => {
                    eprintln!("[richos] the Git name question cannot be watched: {error}");
                    return;
                }
            };
            if let Err(error) = notify::Watcher::watch(&mut watcher, &directory, notify::RecursiveMode::NonRecursive) {
                eprintln!("[richos] the Git name question cannot be watched: {error}");
                return;
            }
            let mut last = None;
            for () in events {
                let state = app.state::<AppState>();
                let now = git_name::asked(&root(&state)).map(|a| (a.thread_id, git_name::choice(&root(&state))));
                if now != last {
                    announce(&app);
                    last = now;
                }
            }
        })
        .map(|_| ())
        .unwrap_or_else(|error| eprintln!("[richos] the Git name question cannot be watched: {error}"));
}

#[cfg(test)]
mod tests {
    /// Its commands answer off Tauri's async workers (`crate::command_rule_tests`): each asks
    /// Git, which is a process.
    #[test]
    fn its_four_commands_run_off_the_async_workers() {
        let source = include_str!("git_name_host.rs");
        assert_eq!(crate::command_rule_tests::bare_tauri_attributes(source), Vec::<String>::new());
        let ours = source.lines().filter(|line| line.trim() == "#[richos_command::command]").count();
        assert_eq!(ours, 4, "git_name_status, git_name_save, git_name_use_richos, git_name_open_file");
    }

    #[test]
    fn a_file_in_the_home_folder_is_written_with_a_tilde() {
        use std::path::Path;
        assert_eq!(super::shown(Path::new("/Users/dana/.gitconfig"), Some(Path::new("/Users/dana"))), "~/.gitconfig");
        assert_eq!(super::shown(Path::new("/etc/gitconfig"), Some(Path::new("/Users/dana"))), "/etc/gitconfig");
    }
}

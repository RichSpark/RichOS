//! **Your name in Git** (richos-hq design round 41, piece 2; build plan 2026-10-21, item 8).
//!
//! Commits carry the user's own name and email. A user whose Git names nobody gets the app's
//! stand-in, `RichOS <richos@localhost>` (`user_git::system_layer`). So the first time the team is
//! about to save a change in a connected folder and nothing names the user, the app asks once, as
//! a card in the conversation, and only that save waits for the answer. The answer is kept in the
//! app's own data, never in `~/.gitconfig`:
//!
//! - `<engine-state>/git-name/choice.json`: the answer, the user's name and email or "use RichOS".
//!   Once it is there the app never asks again.
//! - `<engine-state>/git-name/identity.gitconfig`: the answer as Git reads it. The lease's lowest
//!   layer includes this file (`user_git::system_layer`), above the stand-in and below everything
//!   of the user's own, so it reaches every lease's next commit with nothing restarted.
//! - `<engine-state>/git-name/asked.json`: where and when it was asked, written once by the first
//!   save that waits. The window draws the card from it.
//!
//! THE WAIT is a `PreToolUse[Bash]` hook ([`wait_cli`], run as `<app> --git-name-wait`, registered
//! by `EngineProfile::install_git_name_wait`): a shell command that commits, in a connected folder,
//! where the name or email Git would use is the stand-in, is held until the answer is saved. A
//! held hook makes no model call (measured with a 700 s hold, `agent_hold.py`), the rest of the
//! team keeps working, and the commit then runs under the saved name.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const DIR: &str = "git-name";
const CHOICE: &str = "choice.json";
const IDENTITY: &str = "identity.gitconfig";
const ASKED: &str = "asked.json";
/// The card's own place in the conversation's order: one card, asked once.
pub const ITEM_ID: &str = "git-name";

pub fn dir(state: &Path) -> PathBuf {
    state.join(DIR)
}

/// The file the lease's lowest Git layer includes ([`crate::user_git::system_layer`]).
pub fn identity_file(state: &Path) -> PathBuf {
    dir(state).join(IDENTITY)
}

/// The user's answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "choice", rename_all = "snake_case")]
pub enum Choice {
    /// The user's own name and email go on the team's changes.
    Own { name: String, email: String },
    /// The stand-in stays: "Use "RichOS" instead".
    Richos,
}

/// Where and when the app asked.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asked {
    pub entity_id: String,
    pub thread_id: String,
    /// The connected folder the first save was in (`northwind-site`), when it could be told.
    pub folder: Option<String>,
    pub at_ms: u64,
}

fn read<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

pub fn choice(state: &Path) -> Option<Choice> {
    read(&dir(state).join(CHOICE))
}

pub fn asked(state: &Path) -> Option<Asked> {
    read(&dir(state).join(ASKED))
}

/// Write `body` to `path` through a temporary file in the same folder, so a reader sees the old
/// file or the new one, never half of one.
fn write_atomic(path: &Path, body: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let folder = path.parent().ok_or("no folder")?;
    std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
    let temp = folder.join(format!(".{}.new", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = std::fs::OpenOptions::new().create_new(true).write(true).open(&temp).map_err(|e| e.to_string())?;
        file.write_all(body).and_then(|_| file.sync_all()).map_err(|e| e.to_string())?;
        std::fs::rename(&temp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        drop(std::fs::remove_file(&temp));
    }
    result
}

/// Why an answer cannot be saved as typed. The window says each in words.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Problem {
    NoName,
    NoEmail,
    Email,
}

/// A name, and an email with an `@` and a domain: what Git needs and what GitHub matches.
pub fn check(name: &str, email: &str) -> Result<(), Problem> {
    if name.trim().is_empty() || name.contains(['\n', '\r', '<', '>']) {
        return Err(Problem::NoName);
    }
    let email = email.trim();
    if email.is_empty() {
        return Err(Problem::NoEmail);
    }
    let complete = email.split_once('@').is_some_and(|(local, domain)| {
        !local.is_empty() && !domain.contains('@') && domain.split('.').count() > 1 && domain.split('.').all(|part| !part.is_empty())
    });
    if !complete || email.contains(|c: char| c.is_whitespace() || c == '<' || c == '>') {
        return Err(Problem::Email);
    }
    Ok(())
}

/// A Git config value in quotes, its quotes and backslashes escaped.
fn quoted(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// **Save the user's name and email.** The Git file first, then the answer, so a save that
/// waits is let go only once its commit will carry the name.
pub fn save(state: &Path, name: &str, email: &str) -> Result<Choice, String> {
    check(name, email).map_err(|problem| format!("{problem:?}"))?;
    let (name, email) = (name.trim().to_string(), email.trim().to_string());
    write_atomic(&identity_file(state), format!("[user]\n\tname = {}\n\temail = {}\n", quoted(&name), quoted(&email)).as_bytes())?;
    let choice = Choice::Own { name, email };
    write_atomic(&dir(state).join(CHOICE), &serde_json::to_vec(&choice).map_err(|e| e.to_string())?)?;
    Ok(choice)
}

/// **Keep the stand-in** ("Use "RichOS" instead"): asked, answered, never asked again.
pub fn use_richos(state: &Path) -> Result<Choice, String> {
    match std::fs::remove_file(identity_file(state)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.to_string()),
    }
    write_atomic(&dir(state).join(CHOICE), &serde_json::to_vec(&Choice::Richos).map_err(|e| e.to_string())?)?;
    Ok(Choice::Richos)
}

/// **Ask, once**: record where the first waiting save was. A second save, in this lease or
/// another, finds the record there and waits on the same question.
pub fn ask(state: &Path, asked: &Asked) -> Result<bool, String> {
    use std::io::Write;
    std::fs::create_dir_all(dir(state)).map_err(|e| e.to_string())?;
    let body = serde_json::to_vec(asked).map_err(|e| e.to_string())?;
    // Written whole beside it, then linked into place: the name appears only with its contents,
    // and only once.
    let temp = dir(state).join(format!(".{}.new", uuid::Uuid::new_v4()));
    let mut file = std::fs::OpenOptions::new().create_new(true).write(true).open(&temp).map_err(|e| e.to_string())?;
    file.write_all(&body).and_then(|_| file.sync_all()).map_err(|e| e.to_string())?;
    let placed = std::fs::hard_link(&temp, dir(state).join(ASKED));
    drop(std::fs::remove_file(&temp));
    match placed {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}

/// **The card, as a timeline item** for the conversation it was asked in, or `None` in any other.
/// `suggested` is the name the field starts with: Settings' "Your name" or the Mac account's.
pub fn item(state: &Path, entity_id: &str, thread_id: &str, suggested: Option<&str>) -> Option<Value> {
    let asked = asked(state).filter(|a| a.entity_id == entity_id && a.thread_id == thread_id)?;
    let (answer, name, email) = match choice(state) {
        None => ("open", suggested.unwrap_or_default().to_string(), String::new()),
        Some(Choice::Own { name, email }) => ("saved", name, email),
        Some(Choice::Richos) => ("richos", suggested.unwrap_or_default().to_string(), String::new()),
    };
    Some(json!({
        "kind": "git_name", "id": ITEM_ID, "entityId": entity_id, "threadId": thread_id, "turnId": ITEM_ID,
        "visibility": "ceo", "createdAt": asked.at_ms, "sequence": asked.at_ms,
        "gitName": {"state": answer, "folder": asked.folder, "name": name, "email": email},
    }))
}

/// The Mac account's full name, or `None` when it has none.
pub fn account_name() -> Option<String> {
    // SAFETY: getpwuid returns a pointer into static storage or null; it is read at once.
    let gecos = unsafe {
        let entry = libc::getpwuid(libc::getuid());
        if entry.is_null() || (*entry).pw_gecos.is_null() { return None; }
        std::ffi::CStr::from_ptr((*entry).pw_gecos).to_string_lossy().into_owned()
    };
    // The full name is the first comma-separated field.
    gecos.split(',').next().map(str::trim).filter(|n| !n.is_empty()).map(str::to_string)
}

// ---------------------------------------------------------------------------------------------
// THE WAIT
// ---------------------------------------------------------------------------------------------

/// Split a shell command into its simple commands' words: quotes kept together, `;`, `&`, `|`,
/// newlines and parentheses ending a command. Lexical only, like every reader of a command line
/// the engine has: it finds `git commit` where a person would see it.
fn commands(line: &str) -> Vec<Vec<String>> {
    let mut out = vec![Vec::new()];
    let mut word: Option<String> = None;
    let mut chars = line.chars().peekable();
    let end_word = |word: &mut Option<String>, out: &mut Vec<Vec<String>>| {
        if let Some(w) = word.take() { out.last_mut().unwrap().push(w); }
    };
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                let w = word.get_or_insert_with(String::new);
                for c in chars.by_ref() { if c == '\'' { break; } w.push(c); }
            }
            '"' => {
                let w = word.get_or_insert_with(String::new);
                while let Some(c) = chars.next() {
                    match c {
                        '"' => break,
                        '\\' => { if let Some(n) = chars.next() { w.push(n); } }
                        c => w.push(c),
                    }
                }
            }
            '\\' => { if let Some(n) = chars.next() { if n != '\n' { word.get_or_insert_with(String::new).push(n); } } }
            ';' | '&' | '|' | '\n' | '(' | ')' => {
                end_word(&mut word, &mut out);
                if !out.last().unwrap().is_empty() { out.push(Vec::new()); }
            }
            c if c.is_whitespace() => end_word(&mut word, &mut out),
            c => word.get_or_insert_with(String::new).push(c),
        }
    }
    end_word(&mut word, &mut out);
    out.retain(|c| !c.is_empty());
    out
}

/// **The folders `line` commits in**, run from `cwd`: each `git … commit`, in the folder its
/// `-C` options and any `cd` before it name.
pub fn commit_folders(line: &str, cwd: &Path) -> Vec<PathBuf> {
    let mut here = cwd.to_path_buf();
    let mut out = Vec::new();
    for words in commands(line) {
        let mut words = words.iter().map(String::as_str).skip_while(|w| w.contains('=') && !w.starts_with('-')).peekable();
        let Some(first) = words.next() else { continue };
        if first == "cd" {
            if let Some(to) = words.next() { here = here.join(to); }
            continue;
        }
        if Path::new(first).file_name().and_then(|n| n.to_str()) != Some("git") { continue; }
        let mut folder = here.clone();
        while let Some(word) = words.next() {
            match word {
                "-C" => { if let Some(to) = words.next() { folder = folder.join(to); } }
                "-c" | "--git-dir" | "--work-tree" | "--namespace" | "--exec-path" | "--config-env" => { words.next(); }
                w if w.starts_with('-') => {}
                "commit" => { out.push(folder); break; }
                _ => break,
            }
        }
    }
    out
}

/// One answer to `git -C folder …`, run as the lease runs Git (its environment, with `env` added),
/// or `None`.
fn git(folder: &Path, env: &[(&str, &Path)], args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git").arg("-C").arg(folder).args(args).envs(env.iter().copied())
        .stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// **Should a commit in `folder` wait for the answer?** When it is a connected folder (not one of
/// the app's own, under `app_data`, other than a worker's workspace) and the name or email Git
/// would put on it comes from the stand-in, which is the `layer` file itself. Returns the
/// connected folder's name.
fn waits(folder: &Path, app_data: &Path, workspaces: &Path, layer: &Path, env: &[(&str, &Path)]) -> Option<Option<String>> {
    let common = PathBuf::from(git(folder, env, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?);
    let top = PathBuf::from(git(folder, env, &["rev-parse", "--path-format=absolute", "--show-toplevel"]).unwrap_or_default());
    // Git answers with real paths (`/private/var/...` for `/var/...`), so the two folders are compared as real paths too.
    let real = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let (app_data, workspaces) = (real(app_data), real(workspaces));
    let app_own = |p: &Path| p.starts_with(&app_data) && !p.starts_with(&workspaces);
    if app_own(&common) && app_own(&top) { return None; }
    let layer = std::fs::canonicalize(layer).unwrap_or_else(|_| layer.to_path_buf());
    let from_stand_in = |key: &str| {
        // A lookup Git cannot answer holds nothing: the stand-in always answers when it is there.
        git(folder, env, &["config", "--show-origin", "--get", key]).is_some_and(|line| {
            line.split_once('\t').and_then(|(origin, _)| origin.strip_prefix("file:"))
                .map(|file| std::fs::canonicalize(file).unwrap_or_else(|_| PathBuf::from(file)) == layer)
                .unwrap_or(false)
        })
    };
    if !from_stand_in("user.name") && !from_stand_in("user.email") { return None; }
    // The connected folder's own name: a workspace's shared `.git` is in the folder it was made from.
    let named = if common.file_name().is_some_and(|n| n == ".git") { common.parent().map(Path::to_path_buf) } else { Some(common.clone()) };
    Some(named.and_then(|p| p.file_name().map(|n| n.to_string_lossy().trim_end_matches(".git").to_string())))
}

/// **The hook**: reads Claude Code's `PreToolUse` payload on stdin and, for a commit that waits,
/// asks once and holds until the answer is saved. Always exits 0 with nothing on stdout: it
/// never refuses a command, it only makes the first save wait. It lets go on its own when the
/// session that called it is gone, so no wait outlives its lease.
pub fn wait_cli() -> i32 {
    use std::io::Read;
    let mut raw = Vec::new();
    if std::io::stdin().take(1 << 20).read_to_end(&mut raw).is_err() { return 0; }
    let Ok(payload) = serde_json::from_slice::<Value>(&raw) else { return 0 };
    if payload["tool_name"] != "Bash" { return 0; }
    let Some(line) = payload["tool_input"]["command"].as_str() else { return 0 };
    let cwd = payload["cwd"].as_str().map(PathBuf::from).or_else(|| std::env::current_dir().ok()).unwrap_or_default();
    let folders = commit_folders(line, &cwd);
    if folders.is_empty() { return 0; }
    let (Some(state), Some(layer)) = (std::env::var_os("RICHOS_APP_STATE").map(PathBuf::from), std::env::var_os("GIT_CONFIG_SYSTEM").map(PathBuf::from)) else { return 0 };
    if !state.is_absolute() || choice(&state).is_some() { return 0; }
    let app_data = state.parent().map(Path::to_path_buf).unwrap_or_default();
    let workspaces = state.join("target-worktrees");
    let Some(folder) = folders.iter().find_map(|f| waits(f, &app_data, &workspaces, &layer, &[])) else { return 0 };
    // Where to ask: the conversation this lease works for. Without one there is nowhere to
    // answer, so the save is not held.
    let scope = std::env::var_os("RICHOS_APP_SCOPE").and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok()).unwrap_or_default();
    let (Some(entity_id), Some(thread_id)) = (scope["binding"]["entity_id"].as_str(), scope["binding"]["thread_id"].as_str()) else { return 0 };
    let asked = Asked { entity_id: entity_id.into(), thread_id: thread_id.into(), folder, at_ms: crate::util::now_millis() };
    if let Err(why) = ask(&state, &asked) {
        eprintln!("[richos] the Git name could not be asked ({why}); this save goes ahead with the name RichOS");
        return 0;
    }
    hold(&state, std::time::Duration::from_millis(500), || std::os::unix::process::parent_id() == 1);
    0
}

/// Wait until the answer is saved, or until `orphaned` says the caller is gone.
fn hold(state: &Path, every: std::time::Duration, orphaned: impl Fn() -> bool) {
    while choice(state).is_none() && !orphaned() {
        std::thread::sleep(every);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("richos-git-name-{name}-{}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_commit_is_found_with_its_folder_however_the_command_is_written() {
        let cwd = Path::new("/work/site");
        assert_eq!(commit_folders("git commit -m 'Update prices'", cwd), [PathBuf::from("/work/site")]);
        assert_eq!(commit_folders("git add -A && git commit -qm \"a; b\"", cwd), [PathBuf::from("/work/site")]);
        assert_eq!(commit_folders("cd ../shop && GIT_EDITOR=true git -C sub -c core.x=1 --no-pager commit", cwd), [PathBuf::from("/work/site/../shop/sub")]);
        assert_eq!(commit_folders("/usr/bin/git -C /elsewhere commit --amend", cwd), [PathBuf::from("/elsewhere")]);
        assert_eq!(commit_folders("git log --grep commit; echo git commit; git status", cwd), Vec::<PathBuf>::new());
        assert_eq!(commit_folders("git commit-tree HEAD^{tree}", cwd), Vec::<PathBuf>::new());
    }

    #[test]
    fn an_email_needs_an_at_and_a_domain_and_a_name_is_needed() {
        assert_eq!(check("Dana Whitfield", "dana@northwind.com"), Ok(()));
        assert_eq!(check("Dana", "dana@"), Err(Problem::Email));
        assert_eq!(check("Dana", "dana@northwind"), Err(Problem::Email));
        assert_eq!(check("Dana", "dana northwind.com"), Err(Problem::Email));
        assert_eq!(check("Dana", "  "), Err(Problem::NoEmail));
        assert_eq!(check(" ", "dana@northwind.com"), Err(Problem::NoName));
    }

    /// Asked once: the first save writes the record, a second finds it. Saved: Git reads the
    /// name from the included file, a waiting save is let go, and the answer stays.
    #[test]
    fn it_is_asked_once_and_a_save_lets_the_waiting_commit_go_under_the_users_name() {
        let state = scratch("save");
        let asked = Asked { entity_id: "northwind".into(), thread_id: "t1".into(), folder: Some("northwind-site".into()), at_ms: 7 };
        assert_eq!(ask(&state, &asked), Ok(true));
        assert_eq!(ask(&state, &Asked { thread_id: "t2".into(), ..asked.clone() }), Ok(false), "never asked twice");
        assert_eq!(super::asked(&state), Some(asked.clone()));
        let card = item(&state, "northwind", "t1", Some("Dana Whitfield")).unwrap();
        assert_eq!(card["gitName"], json!({"state": "open", "folder": "northwind-site", "name": "Dana Whitfield", "email": ""}));
        assert!(super::item(&state, "northwind", "t2", None).is_none(), "only in the conversation it was asked in");

        // The hold asks `orphaned` once per look at the answer, so its count of looks is the fact
        // waited on: three looks with no answer, and it is still holding.
        let looks = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let waiting = {
            let (state, looks) = (state.clone(), looks.clone());
            std::thread::spawn(move || hold(&state, std::time::Duration::from_millis(5), || {
                looks.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                false
            }))
        };
        while looks.load(std::sync::atomic::Ordering::SeqCst) < 3 && !waiting.is_finished() { std::thread::yield_now(); }
        assert!(!waiting.is_finished(), "the save waits for the answer");
        assert_eq!(save(&state, " Dana \"D\" Whitfield ", "dana@northwind.com"),
                   Ok(Choice::Own { name: "Dana \"D\" Whitfield".into(), email: "dana@northwind.com".into() }));
        waiting.join().unwrap();
        // Git reads the file as the lease's layer includes it.
        let layer = state.join("layer");
        std::fs::write(&layer, crate::user_git::system_layer(&Default::default(), Some(&identity_file(&state)))).unwrap();
        let name = std::process::Command::new("git").args(["config", "--file"]).arg(&layer).args(["--includes", "--get", "user.name"]).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&name.stdout).trim(), "Dana \"D\" Whitfield");
        assert_eq!(item(&state, "northwind", "t1", None).unwrap()["gitName"]["state"], "saved");
        assert!(save(&state, "Dana", "dana@").is_err(), "a wrong email is refused and changes nothing");
        assert_eq!(choice(&state), Some(Choice::Own { name: "Dana \"D\" Whitfield".into(), email: "dana@northwind.com".into() }));
        std::fs::remove_dir_all(&state).unwrap();
    }

    #[test]
    fn use_richos_keeps_the_stand_in_and_is_never_asked_again() {
        let state = scratch("richos");
        save(&state, "Dana", "dana@northwind.com").unwrap();
        assert_eq!(use_richos(&state), Ok(Choice::Richos));
        assert!(!identity_file(&state).exists(), "the stand-in is what Git reads again");
        assert_eq!(choice(&state), Some(Choice::Richos));
        std::fs::remove_dir_all(&state).unwrap();
    }

    /// A commit waits only in a connected folder whose name and email Git would take from the
    /// stand-in; the app's own folders and a folder whose own Git names the user never wait.
    #[test]
    fn a_commit_waits_only_where_the_stand_in_would_sign_it() {
        let root = scratch("waits");
        let home = root.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let app_data = root.join("data");
        let state = app_data.join("engine-state");
        let layer = app_data.join("layer.gitconfig");
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(&layer, crate::user_git::system_layer(&Default::default(), Some(&identity_file(&state)))).unwrap();
        // Git runs as a lease's does: only the app's layer below an empty home.
        let global = home.join(".gitconfig");
        let env: &[(&str, &Path)] = &[("GIT_CONFIG_SYSTEM", &layer), ("GIT_CONFIG_GLOBAL", &global)];
        let repo = |path: &Path| {
            std::fs::create_dir_all(path).unwrap();
            assert!(std::process::Command::new("git").args(["init", "-q"]).current_dir(path).status().unwrap().success());
        };
        let site = root.join("northwind-site");
        repo(&site);
        assert_eq!(waits(&site, &app_data, &state.join("target-worktrees"), &layer, env), Some(Some("northwind-site".into())));
        let own = app_data.join("coordination");
        repo(&own);
        assert_eq!(waits(&own, &app_data, &state.join("target-worktrees"), &layer, env), None, "the app's own folder");
        std::fs::write(&global, "[user]\n\tname = Dana\n\temail = dana@northwind.com\n").unwrap();
        assert_eq!(waits(&site, &app_data, &state.join("target-worktrees"), &layer, env), None, "the user's own Git names them");
        std::fs::remove_dir_all(&root).unwrap();
    }
}

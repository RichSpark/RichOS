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
//! THE WAIT is Git's own hook ([`wait_cli`], `<app> --git-name-wait`, in the lease's Git layer as
//! [`hook_section`], added by `EngineProfile::install_git_name_wait`): Git runs it at every commit
//! the lease's Git makes, however the command was written, and it holds the commit when Git would
//! sign it with the stand-in (`git var`), in a connected folder, until the answer is saved. The
//! rest of the team keeps working. Git read the name when that commit started, so once it is made
//! the second hook, `<app> --git-name-redo` at `post-commit` / `post-merge` ([`redo_cli`]), makes
//! it again under the saved name; every later commit reads the saved name from the layer.
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
pub(crate) fn quoted(value: &str) -> String {
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

/// The Git hook event the wait runs at. Git runs `prepare-commit-msg` for every commit it makes
/// (`commit`, `merge`, `cherry-pick`, `revert`), however the command that started it was written,
/// and `--no-verify` does not skip it (it skips `pre-commit` and `commit-msg` only).
pub const HOOK_EVENT: &str = "prepare-commit-msg";

/// **The events the waited commit is redone at.** Git reads the name and email once, when the
/// commit starts, before any hook runs, so the commit that waited for the answer was already
/// signed by the stand-in when the answer came (review rv-20261025T021627Z-c3e80047-cfea#1). Once
/// it is made, it is made again under the saved name ([`redo`]): `post-commit` after `commit`,
/// `cherry-pick` and `revert`, `post-merge` after a merge.
pub const REDO_EVENTS: [&str; 2] = ["post-commit", "post-merge"];

/// The set by the redo on the amend it runs, so the hooks of that amend do nothing.
const REDOING: &str = "RICHOS_GIT_NAME_REDO";

/// **The lease's Git runs the wait at every commit**: a `[hook]` section for the lease's lowest
/// layer (`user_git::system_layer`), whose command is the app's own `--git-name-wait`. Git reads
/// config hooks from 2.54 on and runs them beside a repository's own hook, also when the
/// repository moves its hooks with `core.hooksPath` (measured 2026-10-25 with the runtime's Git
/// 2.55.0: both ran; with this Mac's Git 2.52.0 the config hook did not run, which is why the hook
/// is in the layer of the Git the lease runs first, the runtime's).
pub fn hook_section(executable: &Path) -> String {
    let exe = format!("'{}'", executable.to_string_lossy().replace('\'', "'\\''"));
    let mut section = format!("[hook \"richos-git-name\"]\n\tevent = {HOOK_EVENT}\n\tcommand = {}\n", quoted(&format!("{exe} --git-name-wait")));
    section.push_str("[hook \"richos-git-name-redo\"]\n");
    for event in REDO_EVENTS { section.push_str(&format!("\tevent = {event}\n")); }
    section.push_str(&format!("\tcommand = {}\n", quoted(&format!("{exe} --git-name-redo"))));
    section
}

/// The mark the wait leaves for the commit it held, by that commit's own Git folder
/// (`git rev-parse --absolute-git-dir`, one per worktree): both hooks of one commit run in it, and
/// Git may run each through its own shell, so no process id is shared between them.
fn waited(state: &Path, folder: &Path, env: &[(&str, &Path)]) -> Option<PathBuf> {
    use sha2::Digest;
    let git_dir = git(folder, env, &["rev-parse", "--absolute-git-dir"])?;
    Some(dir(state).join(format!("waited-{:x}", sha2::Sha256::digest(git_dir.as_bytes()))))
}

/// Is process `pid` still there?
fn alive(pid: u32) -> bool {
    // SAFETY: signal 0 sends nothing; it only asks whether the process exists.
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

/// The parent of process `pid`, as `ps` reports it.
fn parent_of(pid: u32) -> Option<u32> {
    let out = std::process::Command::new("/bin/ps").args(["-o", "ppid=", "-p", &pid.to_string()]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// One answer to `git -C folder …`, run as the lease runs Git (its environment, with `env` added),
/// or `None`.
fn git(folder: &Path, env: &[(&str, &Path)], args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git").arg("-C").arg(folder).args(args).envs(env.iter().copied())
        .stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// **Would the stand-in sign this commit?** Git's own answer for the folder: the author and the
/// committer it would write (`git var`), which take in everything that decides them, the user's
/// files, the repository's, `-c` and the `GIT_AUTHOR_*` / `GIT_COMMITTER_*` variables.
fn stand_in_signs(folder: &Path, env: &[(&str, &Path)]) -> bool {
    let stand_in = format!("{} <{}>", crate::user_git::STAND_IN_NAME, crate::user_git::STAND_IN_EMAIL);
    ["GIT_AUTHOR_IDENT", "GIT_COMMITTER_IDENT"].iter()
        .any(|v| git(folder, env, &["var", v]).is_some_and(|ident| ident.starts_with(&stand_in)))
}

/// **Should this commit wait for the answer?** When it is in a connected folder (not one of the
/// app's own, under `app_data`, other than a worker's workspace) and the stand-in would sign it.
/// Returns the connected folder's name.
fn waits(folder: &Path, app_data: &Path, workspaces: &Path, env: &[(&str, &Path)]) -> Option<Option<String>> {
    let common = PathBuf::from(git(folder, env, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?);
    let top = PathBuf::from(git(folder, env, &["rev-parse", "--path-format=absolute", "--show-toplevel"]).unwrap_or_default());
    // Git answers with real paths (`/private/var/...` for `/var/...`), so the two folders are compared as real paths too.
    let real = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let (app_data, workspaces) = (real(app_data), real(workspaces));
    let app_own = |p: &Path| p.starts_with(&app_data) && !p.starts_with(&workspaces);
    if app_own(&common) && app_own(&top) { return None; }
    if !stand_in_signs(folder, env) { return None; }
    // The connected folder's own name: a workspace's shared `.git` is in the folder it was made from.
    let named = if common.file_name().is_some_and(|n| n == ".git") { common.parent().map(Path::to_path_buf) } else { Some(common.clone()) };
    Some(named.and_then(|p| p.file_name().map(|n| n.to_string_lossy().trim_end_matches(".git").to_string())))
}

/// **The hook**, run by Git at [`HOOK_EVENT`] in the folder being committed in: when the answer is
/// not saved yet and the stand-in would sign this commit, ask once and hold the commit until the
/// answer is saved. Always exits 0 with nothing on stdout, so it never stops a commit; it only
/// makes it wait. It lets go on its own when the Git that called it is gone, so no wait outlives
/// its commit. Nothing outside a lease runs it: without the lease's state and scope it returns.
pub fn wait_cli() -> i32 {
    if std::env::var_os(REDOING).is_some() { return 0; }
    let Some(state) = std::env::var_os("RICHOS_APP_STATE").map(PathBuf::from) else { return 0 };
    if !state.is_absolute() || choice(&state).is_some() { return 0; }
    let Ok(folder) = std::env::current_dir() else { return 0 };
    let app_data = state.parent().map(Path::to_path_buf).unwrap_or_default();
    let Some(folder_name) = waits(&folder, &app_data, &state.join("target-worktrees"), &[]) else { return 0 };
    // Where to ask: the conversation this lease works for. Without one there is nowhere to
    // answer, so the commit is not held.
    let scope = std::env::var_os("RICHOS_APP_SCOPE").and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok()).unwrap_or_default();
    let (Some(entity_id), Some(thread_id)) = (scope["binding"]["entity_id"].as_str(), scope["binding"]["thread_id"].as_str()) else { return 0 };
    let asked = Asked { entity_id: entity_id.into(), thread_id: thread_id.into(), folder: folder_name, at_ms: crate::util::now_millis() };
    if let Err(why) = ask(&state, &asked) {
        eprintln!("[richos] the Git name could not be asked ({why}); this commit goes ahead with the name RichOS");
        return 0;
    }
    // The Git making the commit is the parent, or the grandparent when Git ran the hook through a
    // shell; when either is gone, the commit is gone and the hold lets go.
    let parent = std::os::unix::process::parent_id();
    let grandparent = parent_of(parent).filter(|&p| p > 1);
    let gone = || std::os::unix::process::parent_id() != parent || grandparent.is_some_and(|g| !alive(g));
    hold(&state, std::time::Duration::from_millis(500), gone);
    // This commit was signed before the answer came: mark it, so it is made again under the
    // saved name once it exists.
    if !gone() && matches!(choice(&state), Some(Choice::Own { .. })) {
        let marked = waited(&state, &folder, &[]).ok_or_else(|| "its Git folder could not be told".to_string())
            .and_then(|mark| std::fs::write(mark, b"").map_err(|e| e.to_string()));
        if let Err(why) = marked {
            eprintln!("[richos] the commit that waited could not be marked ({why}); it keeps the name RichOS");
        }
    }
    0
}

/// **The redo hook**, run by Git after a commit or merge: when the wait held this commit, make it
/// again under the saved name ([`redo`]). Always exits 0.
pub fn redo_cli() -> i32 {
    if std::env::var_os(REDOING).is_some() { return 0; }
    let Some(state) = std::env::var_os("RICHOS_APP_STATE").map(PathBuf::from) else { return 0 };
    let Ok(folder) = std::env::current_dir() else { return 0 };
    if let Err(why) = redo(&state, &folder, &[]) {
        eprintln!("[richos] the commit that waited keeps the name RichOS: {why}");
    }
    0
}

/// Redo the commit just made in `folder` when the wait marked it; `Ok(false)` when it did not.
///
/// The commit is rebuilt from ITSELF, never from the index (review
/// rv-20261025T031723Z-5154b8c7-2b36#1: `git commit -- a` with `b` also staged commits only `a`,
/// and an amend took `b` too): `git commit-tree` with HEAD's own tree, parents, message and author
/// date, signed with the saved name, then HEAD moved to it with `git update-ref`, only while it
/// still points at the commit that waited. The index and the working folder are not touched.
pub(crate) fn redo(state: &Path, folder: &Path, env: &[(&str, &Path)]) -> Result<bool, String> {
    let Some(mark) = waited(state, folder, env) else { return Ok(false) };
    if std::fs::remove_file(mark).is_err() { return Ok(false); }
    let Some(Choice::Own { name, email }) = choice(state) else { return Ok(false) };
    let base: Vec<(String, String)> = env.iter().map(|(k, v)| (k.to_string(), v.to_string_lossy().into_owned())).collect();
    let head = git_io(folder, &["rev-parse", "--verify", "HEAD"], None, &base)?;
    let raw = git_io(folder, &["cat-file", "commit", &head], None, &base)?;
    let (headers, message) = raw.split_once("\n\n").unwrap_or((raw.as_str(), ""));
    let tree = headers.lines().find_map(|l| l.strip_prefix("tree ")).ok_or("the commit has no tree")?;
    let mut args = vec!["commit-tree", tree];
    for parent in headers.lines().filter_map(|l| l.strip_prefix("parent ")) { args.extend(["-p", parent]); }
    let mut signed = base.clone();
    for key in ["GIT_AUTHOR_NAME", "GIT_COMMITTER_NAME"] { signed.push((key.into(), name.clone())); }
    for key in ["GIT_AUTHOR_EMAIL", "GIT_COMMITTER_EMAIL"] { signed.push((key.into(), email.clone())); }
    // The author's time stays the commit's own: `<seconds> <zone>` at the end of its author line.
    if let Some((_, date)) = headers.lines().find_map(|l| l.strip_prefix("author ")).and_then(|l| l.rsplit_once("> ")) {
        signed.push(("GIT_AUTHOR_DATE".into(), date.to_string()));
    }
    let mut body = message.to_string();
    if !body.ends_with('\n') { body.push('\n'); }
    let rebuilt = git_io(folder, &args, Some(body.as_bytes()), &signed)?;
    git_io(folder, &["update-ref", "-m", "RichOS: the saved Git name on the commit that waited", "HEAD", &rebuilt, &head], None, &base)?;
    Ok(true)
}

/// One `git -C folder ...` for the redo: `env` added after Git's own hook variables for the
/// identity are dropped, its own hooks quiet ([`REDOING`]), `stdin` written when given.
fn git_io(folder: &Path, args: &[&str], stdin: Option<&[u8]>, env: &[(String, String)]) -> Result<String, String> {
    use std::io::Write;
    let mut child = std::process::Command::new("git").arg("-C").arg(folder).args(args)
        .env_remove("GIT_AUTHOR_NAME").env_remove("GIT_AUTHOR_EMAIL").env_remove("GIT_AUTHOR_DATE")
        .env_remove("GIT_COMMITTER_NAME").env_remove("GIT_COMMITTER_EMAIL")
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str()))).env(REDOING, "1")
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped())
        .spawn().map_err(|e| e.to_string())?;
    let mut input = child.stdin.take().ok_or("no stdin")?;
    if let Some(bytes) = stdin { input.write_all(bytes).map_err(|e| e.to_string())?; }
    drop(input);
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(String::from_utf8_lossy(&out.stdout).trim().to_string()) }
    else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
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

    /// Git reads the hook back as the shell command it runs, whatever the app's path holds.
    #[test]
    fn the_wait_is_a_git_hook_on_every_commit_with_the_apps_own_executable() {
        let dir = scratch("hook");
        let layer = dir.join("layer");
        let exe = Path::new("/Applications/RichOS Test.app/Contents/MacOS/rich'os");
        std::fs::write(&layer, hook_section(exe)).unwrap();
        let get = |key: &str| {
            let out = std::process::Command::new("git").args(["config", "--file"]).arg(&layer).args(["--get", key]).output().unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        assert_eq!(get("hook.richos-git-name.event"), "prepare-commit-msg");
        assert_eq!(get("hook.richos-git-name.command"), "'/Applications/RichOS Test.app/Contents/MacOS/rich'\\''os' --git-name-wait");
        let events = std::process::Command::new("git").args(["config", "--file"]).arg(&layer)
            .args(["--get-all", "hook.richos-git-name-redo.event"]).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&events.stdout), "post-commit\npost-merge\n");
        assert_eq!(get("hook.richos-git-name-redo.command"), "'/Applications/RichOS Test.app/Contents/MacOS/rich'\\''os' --git-name-redo");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// **Review rv-20261025T011529Z-041708c7-5916, finding 1**: `repo="/folder"; git -C "$repo"
    /// commit` and `env GIT_EDITOR=true git -C /folder commit` went through unasked, because the
    /// command line was read. Now nothing reads it: Git runs its `prepare-commit-msg` hook for the
    /// commit itself, and what the hook sees there is Git's own answer, the stand-in. Run here as
    /// a real `git commit` through the shell in each form, with the hook in the repository's own
    /// hooks folder so any Git runs it; the lease's Git (2.55) runs the same hook from its layer
    /// (`hook_section`).
    #[test]
    fn however_a_commit_is_written_git_runs_the_hook_and_the_stand_in_is_what_it_would_sign() {
        let root = scratch("forms");
        let home = root.join("home");
        std::fs::create_dir_all(&home).unwrap();
        let layer = root.join("layer");
        std::fs::write(&layer, crate::user_git::system_layer(&Default::default(), None)).unwrap();
        let repo = root.join("northwind-site");
        std::fs::create_dir_all(&repo).unwrap();
        let sh = |line: &str, cwd: &Path| {
            std::process::Command::new("/bin/sh").arg("-c").arg(line).current_dir(cwd)
                .env("HOME", &home).env("GIT_CONFIG_SYSTEM", &layer).env("GIT_CONFIG_GLOBAL", home.join(".gitconfig"))
                .env_remove("GIT_AUTHOR_NAME").env_remove("GIT_COMMITTER_NAME").env_remove("GIT_AUTHOR_EMAIL").env_remove("GIT_COMMITTER_EMAIL")
                .status().unwrap()
        };
        assert!(sh("git init -q", &repo).success());
        let seen = root.join("seen");
        let hook = repo.join(".git/hooks/prepare-commit-msg");
        std::fs::write(&hook, format!("#!/bin/sh\ngit var GIT_COMMITTER_IDENT >> '{}'\n", seen.display())).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        let folder = repo.display().to_string();
        for line in [
            format!("repo=\"{folder}\"; git -C \"$repo\" commit -q --allow-empty -m one"),
            format!("env GIT_EDITOR=true git -C {folder} commit -q --allow-empty -m two"),
            "git commit -q --no-verify --allow-empty -m three".to_string(),
        ] {
            assert!(sh(&line, &root.join(if line.starts_with("git commit") { "northwind-site" } else { "" })).success(), "{line}");
        }
        let seen = std::fs::read_to_string(&seen).unwrap();
        let lines: Vec<_> = seen.lines().collect();
        assert_eq!(lines.len(), 3, "the hook ran at every commit: {seen}");
        assert!(lines.iter().all(|l| l.starts_with("RichOS <richos@localhost>")), "{seen}");
        // What the hook decides with: the same Git answer, from the folder itself.
        assert!(stand_in_signs(&repo, &[("GIT_CONFIG_SYSTEM", &layer), ("GIT_CONFIG_GLOBAL", &home.join(".gitconfig"))]));
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// A commit waits only in a connected folder the stand-in would sign: never in the app's own
    /// folders, and never where the user's Git, the repository or the commit itself names someone.
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
            assert!(std::process::Command::new("git").args(["init", "-q"]).current_dir(path).envs(env.iter().copied()).status().unwrap().success());
        };
        let workspaces = state.join("target-worktrees");
        let site = root.join("northwind-site");
        repo(&site);
        assert_eq!(waits(&site, &app_data, &workspaces, env), Some(Some("northwind-site".into())));
        let own = app_data.join("coordination");
        repo(&own);
        assert_eq!(waits(&own, &app_data, &workspaces, env), None, "the app's own folder");
        let named = std::path::PathBuf::from("Fixture Name");
        assert_eq!(waits(&site, &app_data, &workspaces, &[env[0], env[1], ("GIT_COMMITTER_NAME", &named), ("GIT_AUTHOR_NAME", &named)]), None,
                   "the commit itself names someone");
        let params = std::path::PathBuf::from("'user.name=Fixture' 'user.email=fixture@example.invalid'");
        assert_eq!(waits(&site, &app_data, &workspaces, &[env[0], env[1], ("GIT_CONFIG_PARAMETERS", &params)]), None, "git -c names someone");
        std::fs::write(&global, "[user]\n\tname = Dana\n\temail = dana@northwind.com\n").unwrap();
        assert_eq!(waits(&site, &app_data, &workspaces, env), None, "the user's own Git names them");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// **Review rv-20261025T021627Z-c3e80047-cfea#1**: Git signs a commit with the name it read
    /// when the commit started, so the commit that waited for the card was made by the stand-in
    /// even though the user had saved their name. The waited commit is made again: its own author
    /// and committer are the saved name, with the same tree, parent and message; a commit the wait
    /// did not hold, and an answer of "use RichOS", are left as they are.
    #[test]
    fn the_commit_that_waited_is_signed_with_the_saved_name_author_and_committer() {
        let root = scratch("redo");
        let (home, state) = (root.join("home"), root.join("data/engine-state"));
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&state).unwrap();
        let layer = root.join("layer");
        std::fs::write(&layer, crate::user_git::system_layer(&Default::default(), Some(&identity_file(&state)))).unwrap();
        let global = home.join(".gitconfig");
        let env: &[(&str, &Path)] = &[("GIT_CONFIG_SYSTEM", &layer), ("GIT_CONFIG_GLOBAL", &global), ("HOME", &home)];
        let repo = root.join("northwind-site");
        std::fs::create_dir_all(&repo).unwrap();
        let run = |args: &[&str]| {
            let out = std::process::Command::new("git").arg("-C").arg(&repo).args(args).envs(env.iter().copied())
                .env_remove("GIT_AUTHOR_NAME").env_remove("GIT_COMMITTER_NAME").env_remove("GIT_AUTHOR_EMAIL").env_remove("GIT_COMMITTER_EMAIL")
                .output().unwrap();
            assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        run(&["init", "-q"]);
        run(&["commit", "-q", "--allow-empty", "-m", "Before"]);
        std::fs::write(repo.join("prices.html"), "<p>New prices</p>\n").unwrap();
        run(&["add", "prices.html"]);
        // The commit that waited: started under the stand-in, as on the current tip.
        run(&["commit", "-q", "-m", "Update prices"]);
        let signed = || run(&["log", "-1", "--format=%an <%ae>|%cn <%ce>"]);
        let shape = || run(&["log", "-1", "--format=%T %P %s"]);
        assert_eq!(signed(), "RichOS <richos@localhost>|RichOS <richos@localhost>");
        let before = shape();
        // No mark, no redo.
        save(&state, "Dana Whitfield", "dana@northwind.com").unwrap();
        assert_eq!(redo(&state, &repo, env), Ok(false));
        assert_eq!(signed(), "RichOS <richos@localhost>|RichOS <richos@localhost>", "a commit the wait did not hold is left alone");
        // The wait held it and marked it; its post-commit hook redoes it.
        std::fs::write(waited(&state, &repo, env).unwrap(), b"").unwrap();
        // Git hands its hooks the commit's author in GIT_AUTHOR_NAME/EMAIL (measured with Git 2.55,
        // docs/verification/2026-10-25-terminal-and-git-walk/redo-measurement.sh); the redo drops them.
        let (stand_in, stand_in_email) = (Path::new("RichOS"), Path::new("richos@localhost"));
        let in_hook: &[(&str, &Path)] = &[env[0], env[1], env[2], ("GIT_AUTHOR_NAME", stand_in), ("GIT_AUTHOR_EMAIL", stand_in_email)];
        assert_eq!(redo(&state, &repo, in_hook), Ok(true));
        assert_eq!(signed(), "Dana Whitfield <dana@northwind.com>|Dana Whitfield <dana@northwind.com>", "the waiting commit's own author and committer");
        assert_eq!(shape(), before, "the same tree, parent and message");
        assert!(!waited(&state, &repo, env).unwrap().exists(), "redone once");
        assert_eq!(redo(&state, &repo, env), Ok(false));
        // "Use RichOS": the mark is cleared and the commit keeps the stand-in.
        use_richos(&state).unwrap();
        run(&["commit", "-q", "--allow-empty", "-m", "Later"]);
        std::fs::write(waited(&state, &repo, env).unwrap(), b"").unwrap();
        assert_eq!(redo(&state, &repo, env), Ok(false));
        assert_eq!(signed(), "RichOS <richos@localhost>|RichOS <richos@localhost>");
        std::fs::remove_dir_all(&root).unwrap();
    }

    /// **Review rv-20261025T031723Z-5154b8c7-2b36#1**: with changes to `a` and `b` staged, `git
    /// commit -- a` commits only `a`. Putting the saved name on that commit changes nothing it
    /// holds: the commit has only `a`, under the user's name, and `b` is still staged as it was.
    #[test]
    fn the_saved_name_changes_who_signed_the_commit_and_nothing_it_holds() {
        let root = scratch("only-a");
        let (home, state) = (root.join("home"), root.join("data/engine-state"));
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&state).unwrap();
        let layer = root.join("layer");
        std::fs::write(&layer, crate::user_git::system_layer(&Default::default(), Some(&identity_file(&state)))).unwrap();
        let global = home.join(".gitconfig");
        let env: &[(&str, &Path)] = &[("GIT_CONFIG_SYSTEM", &layer), ("GIT_CONFIG_GLOBAL", &global), ("HOME", &home)];
        let repo = root.join("northwind-site");
        std::fs::create_dir_all(&repo).unwrap();
        let run = |args: &[&str]| {
            let out = std::process::Command::new("git").arg("-C").arg(&repo).args(args).envs(env.iter().copied())
                .env_remove("GIT_AUTHOR_NAME").env_remove("GIT_COMMITTER_NAME").env_remove("GIT_AUTHOR_EMAIL").env_remove("GIT_COMMITTER_EMAIL")
                .output().unwrap();
            assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        run(&["init", "-q"]);
        std::fs::write(repo.join("a"), "original\n").unwrap();
        std::fs::write(repo.join("b"), "original\n").unwrap();
        run(&["add", "a", "b"]);
        run(&["commit", "-q", "-m", "Start"]);
        std::fs::write(repo.join("a"), "new prices\n").unwrap();
        std::fs::write(repo.join("b"), "unrelated staged change\n").unwrap();
        run(&["add", "a", "b"]);
        // The commit that waited: only `a`, signed by the stand-in when it started.
        run(&["commit", "-q", "-m", "Update prices", "--", "a"]);
        let before = run(&["log", "-1", "--format=%T %P %s %ad"]);
        save(&state, "Dana Whitfield", "dana@northwind.com").unwrap();
        std::fs::write(waited(&state, &repo, env).unwrap(), b"").unwrap();
        assert_eq!(redo(&state, &repo, env), Ok(true));
        assert_eq!(run(&["log", "-1", "--format=%an <%ae>|%cn <%ce>"]), "Dana Whitfield <dana@northwind.com>|Dana Whitfield <dana@northwind.com>");
        assert_eq!(run(&["log", "-1", "--format=%T %P %s %ad"]), before, "the same tree, parent, message and author date");
        assert_eq!(run(&["show", "--name-only", "--format=", "HEAD"]), "a", "the commit holds only a");
        assert_eq!(run(&["show", "HEAD:b"]), "original", "b in the commit is as it was");
        assert_eq!(run(&["diff", "--cached", "--name-only"]), "b", "b is still staged");
        assert_eq!(std::fs::read_to_string(repo.join("b")).unwrap(), "unrelated staged change\n");
        assert_eq!(run(&["rev-list", "--count", "HEAD"]), "2", "the commit was replaced, not added to");
        std::fs::remove_dir_all(&root).unwrap();
    }
}

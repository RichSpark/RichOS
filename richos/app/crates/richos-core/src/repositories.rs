//! User-selected repository connection. No terminal configuration is adopted.
use crate::entity::{EntityId, EntityRegistry};
use crate::runtime::EngineRuntime;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn git(runtime: &EngineRuntime, root: &Path, args: &[&str]) -> Result<Output, String> {
    Command::new(&runtime.git).args(["-c", "core.hooksPath=/dev/null", "-c", "core.fsmonitor=false",
        "-c", "commit.gpgSign=false", "-c", "user.name=RichOS", "-c", "user.email=richos@localhost"])
        .args(args).current_dir(root).env("PATH", runtime.path())
        .env("GIT_CONFIG_NOSYSTEM", "1").env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_COUNT", "0").env_remove("GIT_CONFIG_PARAMETERS").env_remove("GIT_TEMPLATE_DIR")
        .env_remove("GIT_DIR").env_remove("GIT_WORK_TREE").env_remove("GIT_INDEX_FILE")
        .output().map_err(|e| format!("The delivered Git runtime could not run: {e}"))
}
fn answer(output: Output) -> Result<String, String> {
    if !output.status.success() { return Err("Git could not verify this repository.".into()); }
    String::from_utf8(output.stdout).map(|s|s.trim().to_string()).map_err(|_| "Git returned an unreadable path.".into())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Repository {
    pub root: PathBuf,
    pub branch: String,
    pub initialized: bool,
}

/// Validate scope before touching the selected directory, then make sure it has Git.
///
/// EVERY CONNECTED FOLDER GETS GIT TRACKING (CEO, 2026-10-06). A repository's main checkout
/// connects as it is, its history and local changes untouched. A folder without Git gets
/// `git init` and its files become the first commit (an empty folder gets an empty one).
/// Every refusal comes before Git is touched.
///
/// `initialize_empty` NO LONGER CHANGES ANYTHING: the checkbox it carried is gone, because
/// a folder gets Git whatever it holds. It stays in the signature only so the four live
/// probes under `examples/` (which pass `true` for an empty folder, where the outcome is the
/// same) build unchanged; the app and the tests call [`connect_folder`].
pub fn connect(registry: &EntityRegistry, entity: &EntityId, selected: &Path,
    _initialize_empty: bool, runtime: &EngineRuntime, protected: &[&Path]) -> Result<(EntityRegistry, Repository), String> {
    connect_folder(registry, entity, selected, runtime, protected)
}

/// [`connect`] without the retired flag: connect `selected` to `entity`, giving it Git.
pub fn connect_folder(registry: &EntityRegistry, entity: &EntityId, selected: &Path,
    runtime: &EngineRuntime, protected: &[&Path]) -> Result<(EntityRegistry, Repository), String> {
    if !selected.is_absolute() { return Err("Choose an absolute folder path.".into()); }
    let root = std::fs::canonicalize(selected).map_err(|_| "Choose an existing folder.".to_string())?;
    if !root.is_dir() { return Err("Choose a folder, not a file.".into()); }
    for path in protected {
        let path = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
        if root.starts_with(&path) || path.starts_with(&root) { return Err("Choose a repository outside engine code and app storage.".into()); }
    }
    let mut next = registry.clone();
    next.connect_repository(entity, root.clone()).map_err(|e| e.to_string())?;
    // Canonicalize other registered roots as well: legacy files may contain aliases.
    for other in registry.entities().iter().filter(|e| &e.id != entity) {
        // A repository another company has connected is shared, not overlapped, whatever other
        // roots that company owns (its own company folder may be an ancestor of the repository).
        if other.connected_repositories.iter().any(|c| std::fs::canonicalize(c).unwrap_or_else(|_| c.clone()) == root) { continue; }
        for old in &other.roots {
            let old = std::fs::canonicalize(old).unwrap_or_else(|_| old.clone());
            if root.starts_with(&old) || old.starts_with(&root) { return Err("That folder overlaps another company's folder.".into()); }
        }
    }
    let inside = git(runtime, &root, &["rev-parse", "--show-toplevel"])?;
    let created = if inside.status.success() {
        let top = std::fs::canonicalize(answer(inside)?).map_err(|e|e.to_string())?;
        if top != root || !root.join(".git").is_dir() || root.join(".git").is_symlink() {
            return Err("Choose the repository's main checkout, not a subfolder or linked worktree.".into());
        }
        false
    } else {
        // A `.git` that Git cannot read is somebody's damaged repository, never ours to replace.
        if root.join(".git").symlink_metadata().is_ok() { return Err("Git could not verify this repository.".into()); }
        answer(git(runtime, &root, &["init", "--template=", "-q", "--initial-branch=main"] )?)?;
        true
    };
    let initialized = match first_commit(runtime, &root, created) {
        Ok(initialized) => initialized,
        Err(error) => {
            // This call made the `.git` a moment ago; a set-up that failed leaves none behind.
            if created {
                if let Err(cleanup) = std::fs::remove_dir_all(root.join(".git")) {
                    return Err(format!("{error} The Git folder it started could not be removed: {cleanup}"));
                }
            }
            return Err(error);
        }
    };
    let branch = answer(git(runtime, &root, &["symbolic-ref", "--short", "HEAD"] )?)?;
    Ok((next, Repository {root, branch, initialized}))
}

/// Make sure HEAD names a commit; `true` when this call made the first one.
///
/// `richos.initializing=true` sits in the repository's own config from `init` until the
/// first commit exists, so a set-up killed between the two is finished by the next connect
/// rather than refused forever. A repository with no commit that RichOS did NOT start is
/// finished only when it is empty, which is all an older version could leave behind; one
/// with files is his to commit, and is refused as before.
fn first_commit(runtime: &EngineRuntime, root: &Path, created: bool) -> Result<bool, String> {
    const MARK: &str = "richos.initializing";
    if created { answer(git(runtime, root, &["config", MARK, "true"])?)?; }
    let head = git(runtime, root, &["rev-parse", "--verify", "HEAD"] )?;
    if head.status.success() { return Ok(false); }
    let ours = created || answer(git(runtime, root, &["config", "--get", MARK])?).is_ok_and(|v| v == "true");
    if !ours {
        let no_files = std::fs::read_dir(root).map_err(|e|e.to_string())?
            .collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?.iter().all(|e|e.file_name() == ".git");
        let index = answer(git(runtime, root, &["ls-files"] )?)?;
        if !no_files || !index.is_empty() { return Err("This repository has files but no initial commit.".into()); }
    }
    // `-A` honors any `.gitignore` already in the folder; nothing is pushed and no remote is added.
    answer(git(runtime, root, &["add", "-A"] )?)?;
    answer(git(runtime, root, &["commit", "--allow-empty", "-q", "-m", "Initialize repository"] )?)?;
    if ours { answer(git(runtime, root, &["config", "--unset", MARK])?)?; }
    Ok(true)
}

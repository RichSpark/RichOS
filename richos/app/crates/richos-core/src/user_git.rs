//! **The user's own Git, for a lease's commits and pushes** (build plan 2026-10-21, item 8).
//!
//! A lease runs the runtime's Git first on its `PATH`, and that Git has no system settings of
//! its own. On a Mac the credential helper is most often set in the SYSTEM file of the Git the
//! user installed (Homebrew's and Apple's both set `credential.helper = osxkeychain` there),
//! not in `~/.gitconfig`: measured on the development Mac 2026-10-21,
//! `git config --show-scope --show-origin --get-all credential.helper` printed one line, scope
//! `system`. Apple's Git keeps its `osxkeychain` default in a vendor file below the system one,
//! which `git config --system` does not list: measured the same day with Apple Git 2.50.1
//! (Apple Git-155) and an empty home, `git config --show-scope --show-origin --list` printed it
//! with scope `unknown` (`<developer folder>/usr/share/git-core/gitconfig`), and `--system`
//! failed on a missing `/etc/gitconfig`. So the lease reads the user's own global file natively,
//! includes every file the user's Git reads below the global one (scopes `system` and
//! `unknown`), and finds that Git's helpers on `PATH` (`EngineProfile::configure`).
use std::path::{Path, PathBuf};

/// The Git the user's terminal runs, as a lease needs it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserGit {
    /// The settings files that Git reads below the user's global file, in its order: its
    /// vendor file (Apple's) and its system file, when it has them.
    pub system_configs: Vec<PathBuf>,
    /// That Git's helper folder (`git --exec-path`), where `git-credential-osxkeychain` lives.
    pub exec_path: Option<PathBuf>,
}

/// Apple's `/usr/bin/git` is a stand-in that asks to install the developer tools when they
/// are not there. It is run only when the developer tools it would hand over to are present.
fn runnable(git: &Path) -> bool {
    if git != Path::new("/usr/bin/git") { return true; }
    let mut developer_folders = vec![PathBuf::from("/Library/Developer/CommandLineTools")];
    developer_folders.extend(std::fs::read_link("/var/db/xcode_select_link").ok());
    developer_folders.iter().any(|developer| developer.join("usr/bin/git").is_file())
}

/// **Find the user's Git**: the first `git` on `path` (the user's login `PATH`) that is not
/// the runtime's own, asked where its system file and helpers are. Nothing found is the
/// default, which is the lease Git as before.
pub fn discover(path: &str, home: Option<&Path>, runtime_root: &Path) -> UserGit {
    let runtime_root = std::fs::canonicalize(runtime_root).unwrap_or_else(|_| runtime_root.to_path_buf());
    let Some(git) = path.split(':').filter(|dir| !dir.is_empty()).map(|dir| Path::new(dir).join("git"))
        .filter(|git| git.is_file())
        .find(|git| !std::fs::canonicalize(git).is_ok_and(|real| real.starts_with(&runtime_root)))
        .filter(|git| runnable(git)) else { return UserGit::default() };
    let ask = |args: &[&str]| -> Option<String> {
        let mut command = std::process::Command::new(&git);
        command.args(args).env_clear().env("PATH", path).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        if let Some(home) = home { command.env("HOME", home); }
        // Outside any repository, so a repository's own settings are never listed.
        let output = command.current_dir("/").output().ok().filter(|o| o.status.success())?;
        String::from_utf8(output.stdout).ok()
    };
    let exec_path = ask(&["--exec-path"]).map(|p| PathBuf::from(p.trim())).filter(|p| p.is_dir());
    let mut system_configs: Vec<PathBuf> = Vec::new();
    for line in ask(&["config", "--list", "--show-origin", "--show-scope"]).unwrap_or_default().lines() {
        let mut fields = line.splitn(3, '\t');
        let (Some(scope), Some(origin)) = (fields.next(), fields.next()) else { continue };
        let Some(file) = origin.strip_prefix("file:").map(PathBuf::from) else { continue };
        if matches!(scope, "system" | "unknown") && file.is_file() && !system_configs.contains(&file) {
            system_configs.push(file);
        }
    }
    UserGit { system_configs, exec_path }
}

/// **The lowest Git settings layer a lease reads** (its `GIT_CONFIG_SYSTEM`): the app's own
/// identity, used only when no other layer names one, then the user's Git's own lower files. The
/// user's `~/.gitconfig` and each repository's own settings are read as Git always reads them,
/// above this, so the user's name, email and helpers win.
pub fn system_layer(git: &UserGit) -> String {
    let mut layer = String::from("[user]\n\tname = RichOS\n\temail = richos@localhost\n");
    for file in git.system_configs.iter().map(|f| f.display().to_string()).filter(|f| !f.contains(['"', '\\', '\n'])) {
        layer.push_str(&format!("[include]\n\tpath = \"{file}\"\n"));
    }
    layer
}

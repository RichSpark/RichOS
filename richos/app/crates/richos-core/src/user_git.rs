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

/// The app's stand-in identity, used only when nothing names the user (`git_name.rs`).
pub const STAND_IN_NAME: &str = "RichOS";
pub const STAND_IN_EMAIL: &str = "richos@localhost";

/// **The lowest Git settings layer a lease reads** (its `GIT_CONFIG_SYSTEM`): the app's own
/// identity, used only when no other layer names one, then the name and email the user gave the
/// app (`identity`, the file `git_name::save` writes: Git reads an include that is not there as
/// nothing, so the layer is the same before and after the answer, and a save reaches every
/// running lease's next commit), then the user's Git's own lower files. The user's `~/.gitconfig`
/// and each repository's own settings are read as Git always reads them, above this, so the
/// user's name, email and helpers win.
pub fn system_layer(git: &UserGit, identity: Option<&Path>) -> String {
    let mut layer = format!("[user]\n\tname = {STAND_IN_NAME}\n\temail = {STAND_IN_EMAIL}\n");
    let files = identity.into_iter().map(Path::to_path_buf).chain(git.system_configs.iter().cloned());
    for file in files.map(|f| f.display().to_string()).filter(|f| !f.contains(['"', '\\', '\n'])) {
        layer.push_str(&format!("[include]\n\tpath = \"{file}\"\n"));
    }
    layer
}

/// A name and email the user's own Git setup gives, and the file the name is in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub name: String,
    pub email: String,
    pub file: PathBuf,
}

/// **Does the user's own Git setup name them?** Asked of the user's Git (`discover`'s, the first
/// on the login `path` that is not the runtime's) or, when there is none, of `fallback` (the
/// runtime's Git, which reads `~/.gitconfig` the same way), outside any repository and with no
/// app layer, so only the user's own files answer. Both a name and an email, or `None`.
pub fn identity(path: &str, home: Option<&Path>, runtime_root: &Path, fallback: &Path) -> Option<Identity> {
    let runtime_root = std::fs::canonicalize(runtime_root).unwrap_or_else(|_| runtime_root.to_path_buf());
    let git = path.split(':').filter(|dir| !dir.is_empty()).map(|dir| Path::new(dir).join("git"))
        .filter(|git| git.is_file())
        .find(|git| !std::fs::canonicalize(git).is_ok_and(|real| real.starts_with(&runtime_root)))
        .filter(|git| runnable(git))
        .unwrap_or_else(|| fallback.to_path_buf());
    let mut command = std::process::Command::new(&git);
    command.args(["config", "--list", "--show-origin"]).env_clear().env("PATH", path)
        .stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).current_dir("/");
    if let Some(home) = home { command.env("HOME", home); }
    let output = command.output().ok().filter(|o| o.status.success())?;
    identity_in(&String::from_utf8_lossy(&output.stdout))
}

/// The last `user.name` and `user.email` in `git config --list --show-origin` output, as Git
/// itself takes the last value.
fn identity_in(listing: &str) -> Option<Identity> {
    let (mut name, mut email) = (None, None);
    for line in listing.lines() {
        let Some((origin, setting)) = line.split_once('\t') else { continue };
        let Some((key, value)) = setting.split_once('=') else { continue };
        let file = origin.strip_prefix("file:").map(PathBuf::from);
        match key.to_ascii_lowercase().as_str() {
            "user.name" if !value.trim().is_empty() => name = Some((value.trim().to_string(), file)),
            "user.email" if !value.trim().is_empty() => email = Some(value.trim().to_string()),
            _ => {}
        }
    }
    let (name, file) = name?;
    Some(Identity { name, email: email?, file: file? })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_users_identity_is_the_last_name_and_email_their_git_lists_with_the_names_file() {
        let listing = "file:/etc/gitconfig\tcredential.helper=osxkeychain\n\
                       file:/Users/dana/.gitconfig\tuser.name=Old Name\n\
                       file:/Users/dana/.gitconfig\tuser.email=dana@northwind.com\n\
                       file:/Users/dana/.config/git/work\tuser.name=Dana Whitfield\n";
        assert_eq!(identity_in(listing), Some(Identity { name: "Dana Whitfield".into(), email: "dana@northwind.com".into(),
            file: PathBuf::from("/Users/dana/.config/git/work") }));
        assert_eq!(identity_in("file:/Users/dana/.gitconfig\tuser.name=Dana\n"), None, "a name without an email does not name them");
        assert_eq!(identity_in(""), None);
    }

    #[test]
    fn the_answer_is_included_after_the_stand_in_and_before_the_users_own_files() {
        let git = UserGit { system_configs: vec![PathBuf::from("/opt/homebrew/etc/gitconfig")], exec_path: None };
        let layer = system_layer(&git, Some(Path::new("/data/engine-state/git-name/identity.gitconfig")));
        assert_eq!(layer, "[user]\n\tname = RichOS\n\temail = richos@localhost\n\
            [include]\n\tpath = \"/data/engine-state/git-name/identity.gitconfig\"\n\
            [include]\n\tpath = \"/opt/homebrew/etc/gitconfig\"\n");
    }
}

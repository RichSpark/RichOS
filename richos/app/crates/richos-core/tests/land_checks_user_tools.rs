#![cfg(unix)]
//! An app land runs the repository's merge checks with the user's own tools (handover review
//! rv-20261021T090455Z-529d1d26-253b): a check that runs `cargo check` finds the `cargo` in the
//! user's `~/.cargo/bin`, as the same land from the user's terminal does.
//!
//! The land (`mega-lander/app.py` `land_merge`) runs inside a lease's hook and inherits the
//! lease's environment, so the tools a check finds are the ones on the lease's `PATH`. Build plan
//! item 4 (`login_environment`, `EngineProfile::configure`) puts the user's login-shell `PATH`
//! there; this test drives the real `land_merge` in exactly the environment `configure` gives a
//! lease, with a `cargo` stand-in in the fixture home's `.cargo/bin`.
use richos_core::engine_profile::EngineProfile;
use richos_core::runtime::EngineRuntime;
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("richos land tools fixture {}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        Self(std::fs::canonicalize(root).unwrap())
    }
    fn runtime(&self) -> EngineRuntime {
        EngineRuntime { root: self.0.join("runtime"), git: PathBuf::from("/usr/bin/git"),
            python: self.0.join("runtime/bin/python3"), node: self.0.join("runtime/bin/node"), versions: BTreeMap::new() }
    }
}
impl Drop for Scratch { fn drop(&mut self) { std::fs::remove_dir_all(&self.0).ok(); } }

fn engine() -> PathBuf { Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().join("engine") }

fn executable(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Fixture Git, reading only the fixture home's settings and running no hook.
fn git(home: &Path, repo: &Path, args: &[&str]) -> String {
    let out = Command::new("/usr/bin/git").args(["-c", "core.hooksPath=/dev/null", "-C"]).arg(repo).args(args)
        .env("HOME", home).env("XDG_CONFIG_HOME", home.join(".config")).env("GIT_CONFIG_NOSYSTEM", "1")
        .env_remove("GIT_CONFIG_GLOBAL").env_remove("GIT_DIR").env_remove("GIT_WORK_TREE").env_remove("GIT_INDEX_FILE")
        .output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A repository on `main` with one reviewed commit on `work`, whose own pre-merge-commit check
/// runs `cargo check`. Returns (repo, tip, reviewed commit).
fn repository(home: &Path, root: &Path) -> (PathBuf, String, String) {
    let repo = root.join("acme");
    std::fs::create_dir_all(&repo).unwrap();
    git(home, &repo, &["init", "-q", "--template=", "-b", "main"]);
    git(home, &repo, &["config", "user.name", "Fixture"]);
    git(home, &repo, &["config", "user.email", "fixture@example.invalid"]);
    git(home, &repo, &["commit", "-q", "--allow-empty", "-m", "base"]);
    let tip = git(home, &repo, &["rev-parse", "HEAD"]);
    git(home, &repo, &["checkout", "-q", "-b", "work"]);
    std::fs::write(repo.join("result.txt"), "reviewed work\n").unwrap();
    git(home, &repo, &["add", "result.txt"]);
    git(home, &repo, &["commit", "-q", "-m", "work"]);
    let commit = git(home, &repo, &["rev-parse", "HEAD"]);
    git(home, &repo, &["checkout", "-q", "main"]);
    executable(&repo.join(".git/hooks/pre-merge-commit"), "cargo check\n");
    (repo, tip, commit)
}

const LAND: &str = r#"import importlib.util, sys
spec = importlib.util.spec_from_file_location('richos_app_land', sys.argv[1])
app = importlib.util.module_from_spec(spec)
spec.loader.exec_module(app)
try:
    print('LANDED ' + app.land_merge(sys.argv[2], sys.argv[3], sys.argv[4], 'main', 'fixture-review'))
except ValueError as refused:
    print('REFUSED\n' + str(refused))
"#;

/// `land_merge` run by the runtime's Python in the environment `profile.configure` gives a lease
/// (and every hook in it), in the user's home.
fn land(profile: &EngineProfile, f: &Scratch, home: &Path, repo: &Path, tip: &str, commit: &str) -> String {
    let mut lease = Command::new("/fictional/provider");
    profile.configure(&mut lease, Path::new("/fictional/provider"), "session", &f.0.join("scope.json"));
    let mut hook = Command::new(f.runtime().python);
    for (name, value) in lease.get_envs() {
        match value { Some(value) => { hook.env(name, value); } None => { hook.env_remove(name); } }
    }
    hook.env("HOME", home).env("XDG_CONFIG_HOME", home.join(".config"));
    let out = hook.arg("-c").arg(LAND).arg(engine().join("mega-lander/app.py")).arg(repo).arg(tip).arg(commit)
        .output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn an_app_land_runs_the_repositorys_check_with_the_users_own_tools() {
    let f = Scratch::new();
    // The runtime's Python is a real one here (the fixture runtime ships none).
    let found = Command::new("python3").args(["-c", "import sys; print(sys.executable)"]).output().unwrap();
    let python = String::from_utf8_lossy(&found.stdout).trim().to_string();
    std::fs::create_dir_all(f.0.join("runtime/bin")).unwrap();
    std::os::unix::fs::symlink(&python, f.0.join("runtime/bin/python3")).unwrap();
    // The user's installed toolchain, which the runtime does not carry.
    let home = f.0.join("home");
    let cargo_bin = home.join(".cargo/bin");
    executable(&cargo_bin.join("cargo"), "echo \"cargo $* reached the user's toolchain\"\nexit 0\n");
    let (repo, tip, commit) = repository(&home, &f.0);

    // No login environment (the app before build plan item 4): the check cannot find cargo, the
    // land is refused with the check's words, and main and the checkout are as they were.
    let without = EngineProfile::prepare_for(&engine(), &f.0.join("data-without"), f.runtime(),
        richos_core::machine_settings::Settings::default(), None, None).unwrap();
    let said = land(&without, &f, &home, &repo, &tip, &commit);
    assert!(said.starts_with("REFUSED") && said.contains("cargo") && said.contains("not found"), "{said}");
    assert_eq!(git(&home, &repo, &["rev-parse", "main"]), tip);
    assert_eq!(git(&home, &repo, &["status", "--porcelain"]), "");

    // The user's login shell puts ~/.cargo/bin on PATH, as their terminal has it: the same land
    // goes in, its check having reached the user's cargo.
    let login: BTreeMap<String, String> = [("PATH".to_string(), format!("{}:/usr/bin:/bin", cargo_bin.display()))].into();
    let with = EngineProfile::prepare_for(&engine(), &f.0.join("data-with"), f.runtime(),
        richos_core::machine_settings::Settings::default(), None, Some(std::sync::Arc::new(login))).unwrap();
    let said = land(&with, &f, &home, &repo, &tip, &commit);
    let landed = said.strip_prefix("LANDED ").unwrap_or_else(|| panic!("{said}")).trim().to_string();
    assert_eq!(git(&home, &repo, &["rev-parse", "main"]), landed);
    assert_eq!(git(&home, &repo, &["rev-list", "--parents", "-n", "1", "main"]), format!("{landed} {tip} {commit}"));
    assert_eq!(git(&home, &repo, &["status", "--porcelain"]), "");
}

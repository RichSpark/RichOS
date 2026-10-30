//! yt-dlp install and refresh (media-tools plan slice 2), driven through fakes: the release tag
//! and every download are values, so "a newer nightly", "a tampered file" and "offline" need no
//! network. Each test names the plan's proof or the review finding (m1, m2, m3) it holds.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use richos_core::media_tools::*;
use richos_core::setup::{Fetcher, SetupError};

/// A scratch folder removed when the test ends, pass or fail.
struct Scratch(PathBuf);

impl Drop for Scratch {
    // Best effort: a scratch folder that cannot be removed is not a test failure.
    #[allow(clippy::let_underscore_must_use)]
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl std::ops::Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

fn scratch(name: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!(
        "richos-media-{name}-{}",
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    Scratch(dir)
}

fn sha(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// A nightly repository: a current tag, the files of each published release, and a log of every
/// request, so a test can say exactly what was fetched.
#[derive(Default)]
struct FakeNightly {
    tag: Mutex<Option<String>>,
    files: Mutex<HashMap<String, Vec<u8>>>,
    fetched: Mutex<Vec<String>>,
    tag_asks: Mutex<usize>,
    /// Published as the new `latest` right after the first file is fetched: a nightly that lands
    /// in the middle of a check.
    publish_mid_check: Mutex<Option<(String, Vec<u8>)>>,
}

impl FakeNightly {
    fn publish(&self, tag: &str, body: &[u8]) {
        self.publish_files(tag, body, &sha(body));
    }
    fn publish_files(&self, tag: &str, body: &[u8], listed: &str) {
        let release = format!("{NIGHTLY_RELEASES}/download/{tag}");
        let sums = format!("{}  yt-dlp.exe\n{listed}  yt-dlp\n{}  yt-dlp_macos\n", sha(b"exe"), sha(b"mac"));
        let mut files = self.files.lock().unwrap();
        files.insert(format!("{release}/SHA2-256SUMS"), sums.into_bytes());
        files.insert(format!("{release}/yt-dlp"), body.to_vec());
        *self.tag.lock().unwrap() = Some(tag.to_string());
    }
    fn offline(&self) {
        *self.tag.lock().unwrap() = None;
        self.files.lock().unwrap().clear();
    }
    fn fetched(&self) -> Vec<String> {
        self.fetched.lock().unwrap().clone()
    }
}

impl Fetcher for FakeNightly {
    fn fetch(&self, url: &str, dest: &Path) -> Result<u64, SetupError> {
        self.fetched.lock().unwrap().push(url.to_string());
        let body = self.files.lock().unwrap().get(url).cloned();
        if let Some((tag, newer)) = self.publish_mid_check.lock().unwrap().take() {
            self.publish(&tag, &newer);
        }
        let body = body.ok_or_else(|| SetupError::NoNetwork { url: url.into(), detail: "offline".into() })?;
        std::fs::write(dest, &body).unwrap();
        Ok(body.len() as u64)
    }
}

impl LatestRelease for FakeNightly {
    fn latest_tag(&self) -> Result<String, SetupError> {
        *self.tag_asks.lock().unwrap() += 1;
        self.tag.lock().unwrap().clone()
            .ok_or_else(|| SetupError::NoNetwork { url: format!("{NIGHTLY_RELEASES}/latest"), detail: "offline".into() })
    }
}

/// Every file in the tools folder with its bytes, to prove a failed check changed nothing.
fn snapshot(tools: &Path) -> Vec<(String, Vec<u8>)> {
    let mut all: Vec<_> = std::fs::read_dir(tools).unwrap().flatten()
        .map(|e| (e.file_name().to_string_lossy().to_string(), std::fs::read(e.path()).unwrap_or_default()))
        .collect();
    all.sort();
    all
}

fn names(tools: &Path) -> Vec<String> {
    snapshot(tools).into_iter().map(|(name, _)| name).collect()
}

const A: &[u8] = b"#!/usr/bin/env python3\nPK fake zipapp, nightly A\n";
const B: &[u8] = b"#!/usr/bin/env python3\nPK fake zipapp, nightly B\n";

#[test]
fn first_check_installs_the_newest_nightly_with_its_config_and_launcher() {
    let root = scratch("first");
    let tools = root.join("tools");
    let net = FakeNightly::default();
    net.publish("2026.09.27.232945", A);

    let done = refresh(&net, &net, &tools).unwrap();
    assert_eq!(done, Refreshed::Installed { tag: "2026.09.27.232945".into(), sha256: sha(A), previous: None });
    let have = installed(&tools).expect("an intact copy");
    assert_eq!(have.tag, "2026.09.27.232945");
    assert_eq!(std::fs::read(&have.file).unwrap(), A);
    assert_eq!(std::fs::read_to_string(tools.join("yt-dlp.conf")).unwrap(), "--js-runtimes node\n");
    use std::os::unix::fs::PermissionsExt;
    for runnable in [tools.join("yt-dlp"), have.file.clone()] {
        assert_eq!(std::fs::metadata(&runnable).unwrap().permissions().mode() & 0o777, 0o755, "{}", runnable.display());
    }
    // Nothing else is left in the folder: no staging folder, no checksum list, no partial file.
    assert_eq!(names(&tools), vec!["yt-dlp".to_string(), format!("yt-dlp-{}", &sha(A)[..12]), "yt-dlp.conf".into()]);
    assert_eq!(next_check(&tools), REFRESH_EVERY);
}

/// Plan proof "refresh happens", and m1: the swap leaves the superseded version's file in place
/// for a run that started on it, and the FOLLOWING check removes it.
#[test]
fn a_newer_nightly_replaces_the_copy_and_the_old_file_goes_one_check_later() {
    let root = scratch("refresh");
    let tools = root.join("tools");
    let net = FakeNightly::default();
    net.publish("2026.09.16.000001", A);
    refresh(&net, &net, &tools).unwrap();
    let old = installed(&tools).unwrap().file;

    net.publish("2026.09.27.232945", B);
    let done = refresh(&net, &net, &tools).unwrap();
    assert_eq!(done, Refreshed::Installed { tag: "2026.09.27.232945".into(), sha256: sha(B), previous: Some("2026.09.16.000001".into()) });
    let new = installed(&tools).unwrap();
    assert_eq!(new.tag, "2026.09.27.232945");
    assert_eq!(std::fs::read(&new.file).unwrap(), B);
    assert_eq!(std::fs::read(&old).unwrap(), A, "m1: the version a running yt-dlp may still import from is untouched at the swap");

    assert_eq!(refresh(&net, &net, &tools).unwrap(), Refreshed::Unchanged { tag: "2026.09.27.232945".into() });
    assert!(!old.exists(), "the superseded version is removed at the following check");
    assert_eq!(installed(&tools).unwrap(), new);
}

#[test]
fn an_unchanged_nightly_downloads_only_the_checksum_list() {
    let root = scratch("unchanged");
    let tools = root.join("tools");
    let net = FakeNightly::default();
    net.publish("2026.09.27.232945", A);
    refresh(&net, &net, &tools).unwrap();
    net.fetched.lock().unwrap().clear();

    assert_eq!(refresh(&net, &net, &tools).unwrap(), Refreshed::Unchanged { tag: "2026.09.27.232945".into() });
    assert_eq!(net.fetched(), vec![format!("{NIGHTLY_RELEASES}/download/2026.09.27.232945/SHA2-256SUMS")]);
}

/// Plan proof "a mismatch is deleted": nothing of it stays, it is not fetched again in the same
/// check, and the installed copy keeps working.
#[test]
fn a_download_that_does_not_match_its_checksum_is_deleted_and_the_old_copy_stays() {
    let root = scratch("mismatch");
    let tools = root.join("tools");
    let net = FakeNightly::default();
    net.publish("2026.09.16.000001", A);
    refresh(&net, &net, &tools).unwrap();
    let before = snapshot(&tools);
    net.fetched.lock().unwrap().clear();

    net.publish_files("2026.09.27.232945", b"tampered", &sha(B));
    let err = refresh(&net, &net, &tools).unwrap_err();
    assert_eq!(err.kind(), "digest-mismatch");
    assert_eq!(snapshot(&tools), before, "the tampered bytes are gone and the old copy is exactly as it was");
    assert_eq!(installed(&tools).unwrap().tag, "2026.09.16.000001");
    let file_fetches = net.fetched().iter().filter(|u| u.ends_with("/yt-dlp")).count();
    assert_eq!(file_fetches, 1, "a mismatch is not retried");
}

#[test]
fn a_mismatch_on_first_install_leaves_no_copy_and_retries_in_minutes() {
    let root = scratch("mismatch-first");
    let tools = root.join("tools");
    let net = FakeNightly::default();
    net.publish_files("2026.09.27.232945", b"tampered", &sha(B));
    assert_eq!(refresh(&net, &net, &tools).unwrap_err().kind(), "digest-mismatch");
    assert_eq!(installed(&tools), None);
    assert_eq!(names(&tools), vec!["yt-dlp.conf".to_string()]);
    assert_eq!(next_check(&tools), RETRY_WHILE_MISSING);
}

/// Plan proof "offline keeps the old copy".
#[test]
fn offline_keeps_the_installed_copy_exactly() {
    let root = scratch("offline");
    let tools = root.join("tools");
    let net = FakeNightly::default();
    net.publish("2026.09.27.232945", A);
    refresh(&net, &net, &tools).unwrap();
    let before = snapshot(&tools);

    net.offline();
    assert_eq!(refresh(&net, &net, &tools).unwrap_err().kind(), "no-network");
    assert_eq!(snapshot(&tools), before);
    assert_eq!(installed(&tools).unwrap().tag, "2026.09.27.232945");
    assert_eq!(next_check(&tools), REFRESH_EVERY);
}

/// m3: a first launch while offline does not wait 6 hours for its first copy.
#[test]
fn offline_with_no_copy_retries_in_minutes_not_hours() {
    let root = scratch("offline-first");
    let tools = root.join("tools");
    let net = FakeNightly::default();
    assert_eq!(refresh(&net, &net, &tools).unwrap_err().kind(), "no-network");
    assert_eq!(installed(&tools), None);
    assert_eq!(next_check(&tools), RETRY_WHILE_MISSING);
    assert!(RETRY_WHILE_MISSING.as_secs() <= 5 * 60 && REFRESH_EVERY.as_secs() == 6 * 60 * 60);

    net.publish("2026.09.27.232945", A);
    assert!(matches!(refresh(&net, &net, &tools).unwrap(), Refreshed::Installed { .. }));
    assert_eq!(next_check(&tools), REFRESH_EVERY);
}

/// m3: the tag is resolved once and both files come from that release, so a nightly published
/// between the two downloads cannot turn into a mismatch.
#[test]
fn a_nightly_published_mid_check_does_not_cause_a_mismatch() {
    let root = scratch("midcheck");
    let tools = root.join("tools");
    let net = FakeNightly::default();
    net.publish("2026.09.16.000001", A);
    *net.publish_mid_check.lock().unwrap() = Some(("2026.09.27.232945".into(), B.to_vec()));

    let done = refresh(&net, &net, &tools).unwrap();
    assert_eq!(done, Refreshed::Installed { tag: "2026.09.16.000001".into(), sha256: sha(A), previous: None });
    assert_eq!(*net.tag_asks.lock().unwrap(), 1);
    for url in net.fetched() {
        assert!(url.starts_with(&format!("{NIGHTLY_RELEASES}/download/2026.09.16.000001/")), "{url}");
        assert!(!url.contains("/latest/"), "{url}");
    }
    // The next check finds the one published meanwhile.
    assert!(matches!(refresh(&net, &net, &tools).unwrap(), Refreshed::Installed { ref tag, .. } if tag == "2026.09.27.232945"));
}

#[test]
fn a_launcher_or_version_file_that_was_changed_is_not_an_installed_copy() {
    let root = scratch("intact");
    let tools = root.join("tools");
    let net = FakeNightly::default();
    net.publish("2026.09.27.232945", A);
    refresh(&net, &net, &tools).unwrap();
    let have = installed(&tools).unwrap();

    std::fs::write(&have.file, B).unwrap();
    assert_eq!(installed(&tools), None, "a version file that no longer hashes to its record");
    assert!(matches!(refresh(&net, &net, &tools).unwrap(), Refreshed::Installed { .. }), "the next check repairs it");
    assert_eq!(std::fs::read(&installed(&tools).unwrap().file).unwrap(), A);

    let launcher = tools.join("yt-dlp");
    let text = std::fs::read_to_string(&launcher).unwrap();
    std::fs::write(&launcher, text.replace("RICHOS_RUNTIME_PYTHON", "ELSEWHERE")).unwrap();
    assert_eq!(installed(&tools), None, "a launcher that runs something else");
}

/// The launcher runs `python3` from PATH on the current version file and passes every argument.
/// `python3` here is a stand-in that prints what it was given.
#[test]
fn the_launcher_runs_python3_on_the_current_version_with_the_arguments() {
    let root = scratch("launcher");
    let tools = root.join("tools with a space");
    let net = FakeNightly::default();
    net.publish("2026.09.27.232945", A);
    refresh(&net, &net, &tools).unwrap();
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::write(bin.join("python3"), "#!/bin/sh\nfor a in \"$@\"; do echo \"[$a]\"; done\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(bin.join("python3"), std::fs::Permissions::from_mode(0o755)).unwrap();

    let out = std::process::Command::new(tools.join("yt-dlp"))
        .args(["--version", "two words"])
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .output()
        .unwrap();
    assert!(out.status.success());
    let file = installed(&tools).unwrap().file;
    assert_eq!(String::from_utf8_lossy(&out.stdout), format!("[{}]\n[--version]\n[two words]\n", file.display()));

    // In a lease: the runtime's Python, named in RICHOS_RUNTIME_PYTHON, even with the user's
    // `python3` first on PATH (Hank's parity audit 2026-10-30, item 2: an agent's own commands
    // find the user's tools first, and Apple's Python 3.9 cannot run yt-dlp).
    let runtime = root.join("runtime python3");
    std::fs::write(&runtime, "#!/bin/sh\necho runtime \"$1\"\n").unwrap();
    std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = std::process::Command::new(tools.join("yt-dlp"))
        .arg("--version")
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env(richos_core::media_tools::RUNTIME_PYTHON, &runtime)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), format!("runtime {}\n", file.display()));
}

/// A launcher written before the launcher ran the runtime's Python is not an installed copy, so
/// the next check writes today's.
#[test]
fn a_launcher_from_before_the_runtime_python_is_rewritten_at_the_next_check() {
    let root = scratch("old-launcher");
    let tools = root.join("tools");
    let net = FakeNightly::default();
    net.publish("2026.09.27.232945", A);
    refresh(&net, &net, &tools).unwrap();
    let launcher = tools.join("yt-dlp");
    let today = std::fs::read_to_string(&launcher).unwrap();
    std::fs::write(&launcher, today.replace("\"${RICHOS_RUNTIME_PYTHON:-python3}\"", "python3")).unwrap();
    assert_eq!(installed(&tools), None);
    assert!(matches!(refresh(&net, &net, &tools).unwrap(), Refreshed::Installed { .. }));
    assert_eq!(std::fs::read_to_string(&launcher).unwrap(), today);
}

/// m2: the tools folder is on Rich's PATH, and after every system folder.
#[test]
fn the_tools_folder_is_on_the_path_after_the_system_folders() {
    let path = richos_core::runtime::search_path(Path::new("/rt/bin"), Some(Path::new("/h/Library/Application Support/RichOS/tools")));
    assert_eq!(path, "/rt/bin:/usr/bin:/bin:/usr/sbin:/sbin:/h/Library/Application Support/RichOS/tools");
    assert_eq!(richos_core::runtime::search_path(Path::new("/rt/bin"), None), "/rt/bin:/usr/bin:/bin:/usr/sbin:/sbin");
    assert_eq!(richos_core::runtime::search_path(Path::new("/rt/bin"), Some(Path::new("/a:b/tools"))), "/rt/bin:/usr/bin:/bin:/usr/sbin:/sbin");

    let home = PathBuf::from(std::env::var_os("HOME").expect("HOME"));
    let runtime = richos_core::runtime::EngineRuntime {
        root: "/rt".into(), python: "/rt/bin/python3".into(), node: "/rt/bin/node".into(),
        git: "/rt/bin/git".into(), versions: Default::default(),
    };
    assert_eq!(runtime.path(), format!("/rt/bin:/usr/bin:/bin:/usr/sbin:/sbin:{}", tools_dir(&home).display()));
    assert!(tools_dir(Path::new("/h")).ends_with("Library/Application Support/RichOS/tools"));
}

#[test]
fn only_a_plain_nightly_tag_is_taken_from_the_redirect() {
    assert_eq!(tag_from_redirect(&format!("{NIGHTLY_RELEASES}/tag/2026.09.27.232945")).as_deref(), Some("2026.09.27.232945"));
    for bad in [
        format!("{NIGHTLY_RELEASES}/tag/"),
        format!("{NIGHTLY_RELEASES}/tag/2026.09.27/../../evil"),
        format!("{NIGHTLY_RELEASES}/tag/v2026?x=1"),
        "https://example.com/yt-dlp/yt-dlp-nightly-builds/releases/tag/2026.09.27.232945".to_string(),
        String::new(),
    ] {
        assert_eq!(tag_from_redirect(&bad), None, "{bad}");
    }
}

#[test]
fn the_checksum_list_yields_the_yt_dlp_line_only() {
    let sums = format!("{}  yt-dlp_macos\n{}  yt-dlp\n{}  yt-dlp.exe\n", sha(b"m"), sha(b"y"), sha(b"e"));
    assert_eq!(wanted_hash(&sums), Some(sha(b"y")));
    assert_eq!(wanted_hash(&format!("{}  yt-dlp_macos\n", sha(b"m"))), None);
    assert_eq!(wanted_hash("not-a-hash  yt-dlp\n"), None);
    let root = scratch("no-line");
    let tools = root.join("tools");
    let net = FakeNightly::default();
    net.publish("2026.09.27.232945", A);
    net.files.lock().unwrap().insert(
        format!("{NIGHTLY_RELEASES}/download/2026.09.27.232945/SHA2-256SUMS"), format!("{}  yt-dlp_macos\n", sha(b"m")).into_bytes());
    assert_eq!(refresh(&net, &net, &tools).unwrap_err().kind(), "download-failed");
    assert_eq!(installed(&tools), None);
}

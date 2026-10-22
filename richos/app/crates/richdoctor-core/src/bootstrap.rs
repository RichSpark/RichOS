//! **THE BUNDLED INSTALL** (PRD §3): RichOS carries the signed RichDoctor app as a sealed payload
//! and, early in its own start and in the background, installs or refreshes a SEPARATE copy
//! beside itself. This file is that installer; RichOS calls [`refresh`] on a thread before its
//! settings or engine exist and never waits for it.
//!
//! The rules, each from PRD §3:
//!
//!   - macOS 13 or newer, or nothing is installed.
//!   - Both `/Applications` and `~/Applications` are searched for the expected identity
//!     (`com.richos.richdoctor`) first. An existing copy is reused and refreshed IN PLACE only
//!     when it is older; a newer one is preserved; with two valid copies the newer is preferred
//!     and neither is deleted. A bundle at the path that is NOT RichDoctor is never overwritten.
//!   - With no copy: beside RichOS in a writable Applications folder, otherwise in
//!     `~/Applications`. An unwritable location for an existing copy defers the refresh rather
//!     than creating another copy.
//!   - The whole copy is staged in the same folder, its signature and identity verified, and only
//!     then exchanged atomically with the old one (`renameatx_np` with `RENAME_SWAP`, as
//!     richos-user-update does). A failure at any step leaves the installed copy as it was.
//!   - The archive gets the updater's single-root checks (richos-user-update `extract`): one
//!     `RichDoctor.app` root, no path that escapes it, no hard links, links created last and
//!     contained, bounded entries and size.
//!   - A running older copy with no help session open is asked to quit (it saves first) and is
//!     replaced and reopened quietly; busy, unsaved, refusing or silent for five seconds defers
//!     the refresh to a later RichOS launch. Nothing is ever swapped under a running process or
//!     force-closed.
//!   - A first install opens the new copy once, in the background, for its own explanation and
//!     login registration.
use flate2::read::GzDecoder;
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{self, Read};
use std::os::unix::fs::{symlink, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

pub const APP: &str = "RichDoctor.app";
pub const PAYLOAD_DIR: &str = "RichDoctor";
pub const PAYLOAD: &str = "RichDoctor.app.tar.gz";
const MAX_ENTRIES: usize = 20_000;
const MAX_BYTES: u64 = 512 << 20;

/// Where things are, so tests can stand in for the Mac's own folders.
#[derive(Debug, Clone)]
pub struct Places {
    pub system_apps: PathBuf,
    pub user_apps: PathBuf,
    /// RichDoctor's data folder (its instance socket lives there).
    pub doctor_data: PathBuf,
    /// The folder the running RichOS.app is in.
    pub richos_folder: Option<PathBuf>,
}

impl Places {
    pub fn for_home(home: &Path, richos_app: Option<&Path>) -> Places {
        Places {
            system_apps: PathBuf::from("/Applications"),
            user_apps: home.join("Applications"),
            doctor_data: home.join("Library/Application Support").join(crate::IDENTIFIER),
            richos_folder: richos_app.and_then(Path::parent).map(Path::to_path_buf),
        }
    }
}

/// What a refresh did, for RichOS's log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing to do: the installed copy is as new or newer.
    Current(PathBuf),
    Installed(PathBuf),
    Refreshed(PathBuf),
    /// Not now: the reason, and the copy that stays.
    Deferred(String),
    /// Not on this macOS, or no payload in this RichOS.
    NotApplicable(String),
    Failed(String),
}

fn refuse(m: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, m.to_string())
}

/// `CFBundleIdentifier` and `CFBundleShortVersionString` of an app bundle.
pub fn identity(app: &Path) -> Option<(String, String)> {
    let info = crate::install::read_info(app)?;
    let get = |k: &str| info.get(k).and_then(|v| v.as_string()).map(str::to_string);
    Some((get("CFBundleIdentifier")?, get("CFBundleShortVersionString")?))
}

/// Semantic-version order, with a pre-release lower than its release ("1.0.0-dev.x" < "1.0.0").
pub fn older(a: &str, b: &str) -> bool {
    fn parts(v: &str) -> (Vec<u64>, Option<&str>) {
        let (core, pre) = match v.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (v, None),
        };
        (core.split('.').map(|n| n.parse().unwrap_or(0)).collect(), pre)
    }
    let (ca, pa) = parts(a);
    let (cb, pb) = parts(b);
    let len = ca.len().max(cb.len());
    for i in 0..len {
        let (x, y) = (*ca.get(i).unwrap_or(&0), *cb.get(i).unwrap_or(&0));
        if x != y {
            return x < y;
        }
    }
    match (pa, pb) {
        (Some(_), None) => true,
        (Some(x), Some(y)) => x < y,
        _ => false,
    }
}

// ---------------------------------------------------------------------------------------
// the archive: the updater's single-root checks, for RichDoctor.app
// ---------------------------------------------------------------------------------------

fn relative(path: &Path) -> io::Result<PathBuf> {
    if path.components().count() > 128 {
        return Err(refuse("archive path is too deep"));
    }
    let mut parts = path.components();
    if parts.next() != Some(Component::Normal(APP.as_ref())) {
        return Err(refuse("archive must have exactly one RichDoctor.app root"));
    }
    let mut out = PathBuf::new();
    for part in parts {
        match part {
            Component::Normal(s) => out.push(s),
            _ => return Err(refuse("archive path escapes the application")),
        }
    }
    Ok(out)
}

fn safe_link(rel: &Path, target: &Path) -> io::Result<()> {
    let mut depth = rel.parent().map_or(0, |p| p.components().count());
    for c in target.components() {
        match c {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir if depth > 0 => depth -= 1,
            _ => return Err(refuse("archive link escapes the application")),
        }
    }
    Ok(())
}

/// Extract the payload into `app` (which must not exist).
pub fn extract(bytes: &[u8], app: &Path) -> io::Result<()> {
    fs::create_dir(app)?;
    let mut archive = tar::Archive::new(GzDecoder::new(bytes));
    let mut seen = BTreeSet::new();
    let mut links = Vec::new();
    let mut total = 0u64;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let rel = relative(&entry.path()?)?;
        if !seen.insert(rel.clone()) || seen.len() > MAX_ENTRIES {
            return Err(refuse("duplicate or excessive archive entries"));
        }
        let kind = entry.header().entry_type();
        if rel.as_os_str().is_empty() {
            if !kind.is_dir() {
                return Err(refuse("invalid app root"));
            }
            continue;
        }
        if let Some(parent) = rel.parent() {
            fs::create_dir_all(app.join(parent))?;
        }
        let dest = app.join(&rel);
        if kind.is_dir() {
            fs::create_dir_all(&dest)?;
        } else if kind.is_file() {
            total = total.checked_add(entry.size()).ok_or_else(|| refuse("archive size overflow"))?;
            if total > MAX_BYTES {
                return Err(refuse("archive is too large"));
            }
            let mode = if entry.header().mode()? & 0o111 != 0 { 0o755 } else { 0o644 };
            let mut out = OpenOptions::new().write(true).create_new(true).mode(mode).custom_flags(libc::O_NOFOLLOW).open(&dest)?;
            io::copy(&mut entry, &mut out)?;
            out.sync_all()?;
            fs::set_permissions(&dest, fs::Permissions::from_mode(mode))?;
        } else if kind.is_symlink() {
            let target = entry.link_name()?.ok_or_else(|| refuse("missing link target"))?.into_owned();
            safe_link(&rel, &target)?;
            links.push((rel, target));
        } else {
            return Err(refuse("unsupported archive entry (including hard links)"));
        }
    }
    for (rel, target) in &links {
        if let Some(parent) = rel.parent() {
            fs::create_dir_all(app.join(parent))?;
        }
        symlink(target, app.join(rel))?;
    }
    let canonical = app.canonicalize()?;
    for (rel, _) in &links {
        if !app.join(rel).canonicalize()?.starts_with(&canonical) {
            return Err(refuse("application link resolves outside the bundle"));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------
// signature and identity
// ---------------------------------------------------------------------------------------

fn run(cmd: &str, args: &[&str]) -> Option<(bool, String)> {
    let mut child = std::process::Command::new(cmd).args(args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn().ok()?;
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut out = String::new();
                child.stdout.take()?.take(1 << 20).read_to_string(&mut out).ok();
                let mut err = String::new();
                child.stderr.take()?.take(1 << 20).read_to_string(&mut err).ok();
                return Some((status.success(), out + &err));
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                child.kill().ok();
                child.wait().ok();
                return None;
            }
        }
    }
}

/// The signing team of a bundle, or `None` for an ad-hoc signature, or `Err` when it does not
/// verify.
pub fn signing_team(app: &Path) -> Result<Option<String>, String> {
    let p = app.to_string_lossy();
    match run("/usr/bin/codesign", &["--verify", "--deep", "--strict", &p]) {
        Some((true, _)) => {}
        Some((false, why)) => return Err(format!("signature does not verify: {}", why.trim())),
        None => return Err("codesign did not finish".into()),
    }
    let (_, details) = run("/usr/bin/codesign", &["-dv", "--verbose=2", &p]).ok_or("codesign did not finish")?;
    Ok(details.lines().find_map(|l| l.strip_prefix("TeamIdentifier=")).map(str::trim).filter(|t| *t != "not set").map(str::to_string))
}

/// **THE STAGED COPY IS THE EXPECTED APP**: its signature verifies, its identifier is
/// RichDoctor's, and it is signed by the same team as the RichOS carrying it (both ad-hoc on a
/// development build).
pub fn verify(staged: &Path, expected_team: &Option<String>) -> Result<String, String> {
    let (id, version) = identity(staged).ok_or("the payload has no readable Info.plist")?;
    if id != crate::IDENTIFIER {
        return Err(format!("the payload is {id}, not {}", crate::IDENTIFIER));
    }
    let team = signing_team(staged)?;
    if &team != expected_team {
        return Err(format!("the payload is signed by {team:?}, RichOS by {expected_team:?}"));
    }
    Ok(version)
}

#[cfg(target_os = "macos")]
fn exchange(a: &Path, b: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    extern "C" {
        fn renameatx_np(afd: i32, a: *const libc::c_char, bfd: i32, b: *const libc::c_char, flags: u32) -> i32;
    }
    let a = CString::new(a.as_os_str().as_bytes()).map_err(|_| refuse("invalid path"))?;
    let b = CString::new(b.as_os_str().as_bytes()).map_err(|_| refuse("invalid path"))?;
    // RENAME_SWAP = 0x2.
    // SAFETY: two NUL-terminated paths.
    if unsafe { renameatx_np(libc::AT_FDCWD, a.as_ptr(), libc::AT_FDCWD, b.as_ptr(), 2) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".richdoctor-write-{}", std::process::id()));
    match OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(_) => {
            fs::remove_file(&probe).ok();
            true
        }
        Err(_) => false,
    }
}

/// macOS's version as major number, from its own SystemVersion.plist.
pub fn macos_major() -> Option<u32> {
    crate::collect::macos_version()?.split('.').next()?.parse().ok()
}

/// The copies already installed, newest first: (path, version).
pub fn installed(places: &Places) -> Vec<(PathBuf, String)> {
    let mut out: Vec<(PathBuf, String)> = [&places.system_apps, &places.user_apps]
        .iter()
        .map(|d| d.join(APP))
        .filter_map(|p| identity(&p).filter(|(id, _)| id == crate::IDENTIFIER).map(|(_, v)| (p, v)))
        .collect();
    out.sort_by(|a, b| if older(&a.1, &b.1) { std::cmp::Ordering::Greater } else if older(&b.1, &a.1) { std::cmp::Ordering::Less } else { std::cmp::Ordering::Equal });
    out
}

/// Where a new copy goes: beside RichOS when that Applications folder is writable, otherwise
/// `~/Applications` (made if missing). A path holding something that is not RichDoctor is never
/// used.
fn new_home(places: &Places) -> Option<PathBuf> {
    let free = |d: &Path| !d.join(APP).exists();
    if let Some(f) = &places.richos_folder {
        if (f == &places.system_apps || f == &places.user_apps) && f.is_dir() && writable(f) && free(f) {
            return Some(f.clone());
        }
    }
    if places.system_apps.is_dir() && writable(&places.system_apps) && free(&places.system_apps) && places.richos_folder.as_deref() == Some(places.system_apps.as_path()) {
        return Some(places.system_apps.clone());
    }
    fs::create_dir_all(&places.user_apps).ok()?;
    (writable(&places.user_apps) && free(&places.user_apps)).then(|| places.user_apps.clone())
}

/// Ask a running RichDoctor to quit when it has no help session open. `Ok(true)`: it was
/// running and quit; `Ok(false)`: none was running; `Err`: busy, refusing or silent.
fn quit_running(places: &Places) -> Result<bool, String> {
    use std::io::{BufRead, BufReader, Write};
    let sock = places.doctor_data.join("instance.sock");
    let Ok(mut s) = std::os::unix::net::UnixStream::connect(&sock) else { return Ok(false) };
    s.set_read_timeout(Some(Duration::from_secs(5))).ok();
    writeln!(s, "quit-if-idle").map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).map_err(|_| "RichDoctor did not answer within five seconds".to_string())?;
    if line.trim() != "ok" {
        return Err("a RichDoctor help session is open".into());
    }
    // Wait (five seconds at most) for it to be gone: its socket stops answering.
    let until = Instant::now() + Duration::from_secs(5);
    while Instant::now() < until {
        if std::os::unix::net::UnixStream::connect(&sock).is_err() {
            std::thread::sleep(Duration::from_millis(300));
            return Ok(true);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("RichDoctor did not quit within five seconds".into())
}

fn open_quietly(app: &Path) {
    std::process::Command::new("/usr/bin/open").arg("-g").arg(app).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().ok();
}

/// **INSTALL OR REFRESH** from `payload` (the archive inside RichOS.app), whose signer must be
/// `team`. Never panics, never waits on RichOS, leaves the installed copy intact on any failure.
pub fn refresh(payload: &Path, payload_version: &str, team: &Option<String>, places: &Places) -> Outcome {
    refresh_with(payload, payload_version, team, places, &open_quietly)
}

/// [`refresh`] with the "open it quietly" step given, so a test never launches anything.
pub fn refresh_with(payload: &Path, payload_version: &str, team: &Option<String>, places: &Places, open: &dyn Fn(&Path)) -> Outcome {
    if macos_major().is_some_and(|m| m < 13) {
        return Outcome::NotApplicable("RichDoctor requires macOS 13 or newer".into());
    }
    let Ok(bytes) = fs::read(payload) else { return Outcome::NotApplicable("this RichOS carries no RichDoctor".into()) };
    let copies = installed(places);
    let (target_dir, existing) = match copies.first() {
        Some((path, version)) if !older(version, payload_version) => return Outcome::Current(path.clone()),
        Some((path, _)) => {
            let dir = path.parent().unwrap().to_path_buf();
            if !writable(&dir) {
                return Outcome::Deferred(format!("{} is not writable; the installed copy stays", dir.display()));
            }
            (dir, Some(path.clone()))
        }
        None => match new_home(places) {
            Some(d) => (d, None),
            None => return Outcome::Deferred("no Applications folder can take RichDoctor".into()),
        },
    };
    let staged = target_dir.join(format!(".RichDoctor.app.staging-{}", crate::util::hex(&crate::util::random_bytes(4).unwrap_or_default())));
    let cleanup = |p: &Path| {
        fs::remove_dir_all(p).ok();
    };
    if let Err(e) = extract(&bytes, &staged) {
        cleanup(&staged);
        return Outcome::Failed(format!("the payload did not extract: {e}"));
    }
    match verify(&staged, team) {
        Ok(v) if v == payload_version => {}
        Ok(v) => {
            cleanup(&staged);
            return Outcome::Failed(format!("the payload says {v}, RichOS expected {payload_version}"));
        }
        Err(e) => {
            cleanup(&staged);
            return Outcome::Failed(e);
        }
    }
    match existing {
        None => {
            let dest = target_dir.join(APP);
            if dest.exists() || fs::rename(&staged, &dest).is_err() {
                cleanup(&staged);
                return Outcome::Failed("the new copy could not be put in place".into());
            }
            open(&dest);
            Outcome::Installed(dest)
        }
        Some(path) => {
            let was_running = match quit_running(places) {
                Ok(r) => r,
                Err(why) => {
                    cleanup(&staged);
                    return Outcome::Deferred(why);
                }
            };
            if let Err(e) = exchange(&staged, &path) {
                cleanup(&staged);
                return Outcome::Failed(format!("the exchange failed: {e}"));
            }
            // `staged` now holds the old copy.
            cleanup(&staged);
            if was_running {
                open(&path);
            }
            Outcome::Refreshed(path)
        }
    }
}

/// The payload's place inside a RichOS bundle, from that bundle's executable.
pub fn payload_of(richos_exe: &Path) -> Option<(PathBuf, PathBuf)> {
    let contents = richos_exe.parent()?.parent()?;
    let app = contents.parent()?.to_path_buf();
    Some((contents.join("Resources").join(PAYLOAD_DIR).join(PAYLOAD), app))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rd-boot-{tag}-{}", crate::util::hex(&crate::util::random_bytes(4).unwrap())));
        fs::create_dir_all(&d).unwrap();
        d.canonicalize().unwrap()
    }

    /// A tiny RichDoctor-shaped bundle, ad-hoc signed like a development build.
    fn bundle(dir: &Path, id: &str, version: &str) -> PathBuf {
        let app = dir.join(APP);
        fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        fs::write(app.join("Contents/Info.plist"), format!(r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>{id}</string><key>CFBundleShortVersionString</key><string>{version}</string><key>CFBundleExecutable</key><string>RichDoctor</string><key>CFBundlePackageType</key><string>APPL</string></dict></plist>"#)).unwrap();
        fs::copy("/usr/bin/true", app.join("Contents/MacOS/RichDoctor")).unwrap();
        let ok = std::process::Command::new("/usr/bin/codesign").args(["--force", "--sign", "-", app.to_str().unwrap()]).output().unwrap();
        assert!(ok.status.success(), "{}", String::from_utf8_lossy(&ok.stderr));
        app
    }

    fn archive(app: &Path, out: &Path) {
        let f = fs::File::create(out).unwrap();
        let gz = flate2::write::GzEncoder::new(f, flate2::Compression::fast());
        let mut t = tar::Builder::new(gz);
        t.follow_symlinks(false);
        t.append_dir_all(APP, app).unwrap();
        t.into_inner().unwrap().finish().unwrap();
    }

    fn places(root: &Path) -> Places {
        let p = Places { system_apps: root.join("Applications"), user_apps: root.join("home/Applications"), doctor_data: root.join("data"), richos_folder: Some(root.join("Applications")) };
        fs::create_dir_all(&p.system_apps).unwrap();
        p
    }

    #[test]
    fn versions_order_with_prereleases_below_their_release() {
        assert!(older("1.0.0", "1.0.1"));
        assert!(older("1.0.0-dev.abc", "1.0.0"));
        assert!(!older("1.0.0", "1.0.0"));
        assert!(!older("1.10.0", "1.9.9"));
    }

    #[test]
    fn a_first_install_goes_beside_richos_and_a_newer_copy_is_kept() {
        let root = temp("first");
        let build = temp("build");
        let payload = root.join(PAYLOAD);
        archive(&bundle(&build, crate::IDENTIFIER, "1.0.0"), &payload);
        let p = places(&root);
        let out = refresh_with(&payload, "1.0.0", &None, &p, &|_| {});
        assert_eq!(out, Outcome::Installed(p.system_apps.join(APP)));
        assert_eq!(identity(&p.system_apps.join(APP)).unwrap().1, "1.0.0");
        // The same payload again: current, nothing touched.
        assert!(matches!(refresh_with(&payload, "1.0.0", &None, &p, &|_| {}), Outcome::Current(_)));
        // An older payload never replaces a newer copy.
        let older_build = temp("older");
        let older_payload = root.join("older.tar.gz");
        archive(&bundle(&older_build, crate::IDENTIFIER, "0.9.0"), &older_payload);
        assert!(matches!(refresh_with(&older_payload, "0.9.0", &None, &p, &|_| {}), Outcome::Current(_)));
        assert_eq!(identity(&p.system_apps.join(APP)).unwrap().1, "1.0.0");
        for d in [root, build, older_build] {
            fs::remove_dir_all(d).ok();
        }
    }

    #[test]
    fn an_older_copy_is_refreshed_in_place_and_a_foreign_app_is_never_overwritten() {
        let root = temp("refresh");
        let p = places(&root);
        let old = bundle(&p.user_apps, crate::IDENTIFIER, "0.9.0");
        let build = temp("build2");
        let payload = root.join(PAYLOAD);
        archive(&bundle(&build, crate::IDENTIFIER, "1.0.0"), &payload);
        assert_eq!(refresh_with(&payload, "1.0.0", &None, &p, &|_| {}), Outcome::Refreshed(old.clone()));
        assert_eq!(identity(&old).unwrap().1, "1.0.0");
        assert!(!p.system_apps.join(APP).exists(), "no second copy was made");
        assert_eq!(fs::read_dir(&p.user_apps).unwrap().count(), 1, "no staging folder left behind");
        // Something else named RichDoctor.app where a new copy would go: left alone.
        let other = temp("other");
        let po = places(&other);
        bundle(&po.system_apps, "com.example.other", "9.9.9");
        let out = refresh_with(&payload, "1.0.0", &None, &po, &|_| {});
        assert_eq!(identity(&po.system_apps.join(APP)).unwrap().0, "com.example.other");
        assert!(matches!(out, Outcome::Installed(ref path) if path == &po.user_apps.join(APP)), "{out:?}");
        for d in [root, build, other] {
            fs::remove_dir_all(d).ok();
        }
    }

    #[test]
    fn a_bad_payload_leaves_the_installed_copy_as_it_was() {
        let root = temp("bad");
        let p = places(&root);
        let old = bundle(&p.system_apps, crate::IDENTIFIER, "0.9.0");
        let before = fs::read(old.join("Contents/Info.plist")).unwrap();
        // The wrong identity inside the payload.
        let build = temp("build3");
        let payload = root.join(PAYLOAD);
        archive(&bundle(&build, "com.example.impostor", "1.0.0"), &payload);
        assert!(matches!(refresh_with(&payload, "1.0.0", &None, &p, &|_| {}), Outcome::Failed(_)));
        // A tampered payload: its signature no longer verifies.
        let build4 = temp("build4");
        let app = bundle(&build4, crate::IDENTIFIER, "1.0.0");
        fs::write(app.join("Contents/MacOS/RichDoctor"), b"#!/bin/sh\n").unwrap();
        archive(&app, &payload);
        assert!(matches!(refresh_with(&payload, "1.0.0", &None, &p, &|_| {}), Outcome::Failed(_)));
        // A team that does not match RichOS's.
        let build5 = temp("build5");
        archive(&bundle(&build5, crate::IDENTIFIER, "1.0.0"), &payload);
        assert!(matches!(refresh_with(&payload, "1.0.0", &Some("TZ33A4QCZJ".into()), &p, &|_| {}), Outcome::Failed(_)));
        assert_eq!(fs::read(old.join("Contents/Info.plist")).unwrap(), before);
        assert_eq!(fs::read_dir(&p.system_apps).unwrap().count(), 1, "no staging folder left behind");
        for d in [root, build, build4, build5] {
            fs::remove_dir_all(d).ok();
        }
    }

    #[test]
    fn an_archive_that_escapes_its_root_is_refused() {
        let root = temp("escape");
        let mut bytes = Vec::new();
        {
            let gz = flate2::write::GzEncoder::new(&mut bytes, flate2::Compression::fast());
            let mut t = tar::Builder::new(gz);
            let mut h = tar::Header::new_gnu();
            h.set_size(1);
            h.set_mode(0o644);
            h.set_entry_type(tar::EntryType::Regular);
            t.append_data(&mut h, "Other.app/x", &b"x"[..]).unwrap();
            t.into_inner().unwrap().finish().unwrap();
        }
        assert!(extract(&bytes, &root.join("out.app")).is_err());
        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn a_busy_running_copy_defers_the_refresh() {
        let root = temp("busy");
        let p = places(&root);
        let old = bundle(&p.system_apps, crate::IDENTIFIER, "0.9.0");
        let build = temp("build6");
        let payload = root.join(PAYLOAD);
        archive(&bundle(&build, crate::IDENTIFIER, "1.0.0"), &payload);
        // A stand-in for a running RichDoctor with a help session open.
        crate::util::private_dir(&p.doctor_data).unwrap();
        let listener = std::os::unix::net::UnixListener::bind(p.doctor_data.join("instance.sock")).unwrap();
        let t = std::thread::spawn(move || {
            use std::io::{BufRead, BufReader, Write};
            let (s, _) = listener.accept().unwrap();
            let mut line = String::new();
            BufReader::new(&s).read_line(&mut line).unwrap();
            assert_eq!(line.trim(), "quit-if-idle");
            let mut w = &s;
            writeln!(w, "busy").unwrap();
        });
        assert!(matches!(refresh_with(&payload, "1.0.0", &None, &p, &|_| {}), Outcome::Deferred(_)));
        t.join().unwrap();
        assert_eq!(identity(&old).unwrap().1, "0.9.0", "nothing was swapped under it");
        for d in [root, build] {
            fs::remove_dir_all(d).ok();
        }
    }
}

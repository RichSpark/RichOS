//! **yt-dlp on the user's Mac, kept at the newest `nightly` release** (media-tools plan, slice 2:
//! richos-hq `docs/plans/2026-10-07-media-tools.md` §1 and §3).
//!
//! The CEO, 2026-10-07: *"yt-dlp needs frequent updating in order to work with Youtube. So,
//! either the video skill or better our RichOS app needs to make sure the most up-to-date version
//! of that tool is always available on the user's machine."* The app stays lean: nothing here is
//! inside the `.app`. The app downloads yt-dlp into `~/Library/Application Support/RichOS/tools/`
//! and keeps it current.
//!
//! **Why not the engine runtime.** The runtime is a fixed, hashed inventory and `runtime.rs`
//! refuses any file it does not list; "newest" means these bytes change whenever yt-dlp
//! publishes. So the tools folder is its own home, and [`crate::runtime::EngineRuntime::path`]
//! puts it on Rich's PATH (after `/sbin`, see below).
//!
//! **What is installed.** The platform-independent `yt-dlp` file (a Python zipapp, about 3 MB;
//! it runs on the delivered Python), stored under its own hash, plus a one-line `yt-dlp.conf`
//! (`--js-runtimes node`, so YouTube's challenges run on the delivered Node). yt-dlp reads that
//! file as its portable config because it sits in the same folder as the zipapp (measured: `-v`
//! printed `Portable config ".../tools/yt-dlp.conf": ['--js-runtimes', 'node']` and
//! `JS runtimes: node-24.21.0` through the launcher below).
//!
//! **Fetch and integrity**, reusing `setup.rs`: [`Fetcher`] (`CurlFetcher` in the app, https
//! only), [`sha256_file`] and the staging folder that cannot outlive a failure. The hash comes
//! from the same release's `SHA2-256SUMS`. That is **single-witness**: it catches corruption and
//! a half-finished download, not a compromised host (the same honest weakness `provision.rs`
//! names for one model). A file that does not match is deleted and not retried in that check.
//!
//! **Frank's three findings on this code** (richos-hq `docs/plans/2026-10-07-media-tools-review.md`):
//!
//! - **m1, a refresh while yt-dlp runs.** yt-dlp imports extractors lazily from its own zip, so
//!   replacing the zip under a running copy can crash that run. Each version is stored as
//!   `tools/yt-dlp-<first 12 hex of its sha256>` and never rewritten; `tools/yt-dlp` is a small
//!   `/bin/sh` launcher that `exec`s `python3` on the current version, and only the launcher is
//!   swapped (one `rename`). A superseded version is deleted at the FOLLOWING refresh, not at the
//!   swap, so a run that started on it keeps its file. A symlink would not do: Python reopens the
//!   path it was given.
//! - **m2, an unverified folder ahead of `/usr/bin`.** The tools folder is user-writable and not
//!   hash-checked at load, so it goes on PATH after `/sbin`: nothing put there can shadow a
//!   system command. `yt-dlp` has no system namesake, so the order costs it nothing.
//! - **m3, no yt-dlp for 6 hours.** The release tag is resolved ONCE per check (from the
//!   `releases/latest` redirect) and both files come from `releases/download/<tag>/`, so a nightly
//!   published between the two requests cannot produce a mismatch. While no copy is installed,
//!   [`next_check`] retries every [`RETRY_WHILE_MISSING`] instead of every [`REFRESH_EVERY`].
//!
//! **Offline or any other failure changes nothing**: the installed copy and its launcher stay as
//! they were.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use crate::setup::{app_support_richos, sha256_file, CurlFetcher, Fetcher, SetupError, Staging};

/// The `nightly` channel's repository. yt-dlp's README calls nightly "the recommended channel
/// for regular users".
pub const NIGHTLY_RELEASES: &str = "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases";

/// What the portable config next to yt-dlp says: run YouTube's challenge scripts on Node, which
/// the engine runtime delivers (yt-dlp accepts Node 22.0.0 or later; the runtime has 24.21.0).
pub const CONFIG: &str = "--js-runtimes node\n";

/// How often a running app looks for a newer nightly once a copy is installed (plan §3).
pub const REFRESH_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// How often it tries again while NO copy is installed (Frank m3): a first launch while offline,
/// or a check that failed before anything was ever installed.
pub const RETRY_WHILE_MISSING: Duration = Duration::from_secs(5 * 60);

/// How long after launch the first check runs, beside the app's own update check.
pub const FIRST_CHECK_AFTER: Duration = Duration::from_secs(3);

/// The largest `SHA2-256SUMS` accepted. The measured one is 1,505 bytes.
const SUMS_LIMIT: u64 = 64 * 1024;

/// A staging folder left by a process that was killed mid-download is removed once it is this
/// old; a younger one may belong to a check that is still running.
const STALE_STAGING: Duration = Duration::from_secs(60 * 60);

/// One check at a time in this process, so a timer tick and a later caller (setup, slice 3)
/// cannot interleave their swaps.
static REFRESHING: Mutex<()> = Mutex::new(());

/// `~/Library/Application Support/RichOS/tools`.
pub fn tools_dir(home: &Path) -> PathBuf {
    app_support_richos(home).join("tools")
}

/// Which tag `releases/latest` points at right now. A trait so "offline" and "a nightly published
/// mid-check" are values in a test, as [`Fetcher`] is for downloads.
pub trait LatestRelease: Send + Sync {
    fn latest_tag(&self) -> Result<String, SetupError>;
}

/// One HEAD request with `/usr/bin/curl`, not following the redirect: GitHub answers `302` with
/// `Location: .../releases/tag/<tag>` (measured 2026-10-07: `.../tag/2026.09.27.232945`).
impl LatestRelease for CurlFetcher {
    fn latest_tag(&self) -> Result<String, SetupError> {
        let url = format!("{NIGHTLY_RELEASES}/latest");
        let out = std::process::Command::new("/usr/bin/curl")
            .args(["--fail", "--silent", "--show-error", "--head", "--proto", "=https", "--tlsv1.2",
                "--connect-timeout", "20", "--max-time", "60", "--output", "/dev/null",
                "--write-out", "%{redirect_url}"])
            .arg(&url)
            .output()
            .map_err(|e| SetupError::NoNetwork { url: url.clone(), detail: format!("curl could not be run: {e}") })?;
        if !out.status.success() {
            let code = out.status.code().unwrap_or(-1);
            let detail = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(match code {
                6 | 7 | 28 | 35 => SetupError::NoNetwork { url, detail },
                _ => SetupError::DownloadFailed { url, status: if detail.is_empty() { format!("curl exit {code}") } else { detail } },
            });
        }
        tag_from_redirect(String::from_utf8_lossy(&out.stdout).trim())
            .ok_or_else(|| SetupError::DownloadFailed { url, status: "no release tag in its redirect".into() })
    }
}

/// The tag in `https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/tag/<tag>`. Only digits
/// and dots are accepted (nightly tags are `YYYY.MM.DD.HHMMSS`), because the tag is put into the
/// next two URLs.
pub fn tag_from_redirect(location: &str) -> Option<String> {
    let tag = location.strip_prefix(NIGHTLY_RELEASES)?.strip_prefix("/tag/")?;
    let valid = !tag.is_empty() && tag.len() <= 32 && tag.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        && tag.bytes().any(|b| b.is_ascii_digit());
    valid.then(|| tag.to_string())
}

/// The installed copy, as its launcher records it and its file proves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// The nightly release tag, which is also what `yt-dlp --version` prints.
    pub tag: String,
    pub sha256: String,
    /// `tools/yt-dlp-<sha12>`, the file the launcher runs.
    pub file: PathBuf,
}

/// What one check did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refreshed {
    /// The installed copy is already the newest nightly.
    Unchanged { tag: String },
    /// A new copy is installed; `previous` is the tag it replaced, if there was one.
    Installed { tag: String, sha256: String, previous: Option<String> },
}

fn version_name(sha256: &str) -> String {
    format!("yt-dlp-{}", &sha256[..12])
}

/// The launcher, byte for byte. Line 2 is the record of what is installed; [`installed`] accepts
/// a launcher only if it is exactly this text for the tag and hash it names. It runs the delivered
/// runtime's Python, which every lease names in [`RUNTIME_PYTHON`]: an agent's own commands find
/// the user's `python3` first (`runtime::commands_path`), and yt-dlp no longer runs on the Python
/// 3.9 that Apple's developer tools put in `/usr/bin` (measured 2026-10-30: a traceback before
/// `--version` printed). Outside a lease, `python3`
/// from PATH. `-E -s`: an agent's commands carry the user's own `PYTHONPATH`, `PYTHONHOME` and
/// `pip --user` packages (`EngineProfile::commands_env`), which are for the user's Python and
/// never reach this one. A launcher written before this text is not an installed copy, so the
/// next check writes this one.
fn launcher_text(tag: &str, sha256: &str) -> String {
    format!("#!/bin/sh\n# yt-dlp nightly {tag} sha256 {sha256}\nexec \"${{{RUNTIME_PYTHON}:-python3}}\" -E -s \"$(dirname \"$0\")/{}\" \"$@\"\n",
        version_name(sha256))
}

/// The name every lease gives the delivered runtime's Python under (`EngineProfile::configure`).
pub const RUNTIME_PYTHON: &str = "RICHOS_RUNTIME_PYTHON";

fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The installed yt-dlp, or `None` when there is none or it is not intact: the launcher must be
/// exactly what this module writes, and the version file it runs must hash to the recorded value.
/// Setup's "present" check (slice 3) is this function.
pub fn installed(tools: &Path) -> Option<Installed> {
    let launcher = tools.join("yt-dlp");
    if std::fs::metadata(&launcher).ok()?.len() > 4096 { return None; }
    let text = std::fs::read_to_string(&launcher).ok()?;
    let record = text.lines().nth(1)?.strip_prefix("# yt-dlp nightly ")?;
    let (tag, sha256) = record.split_once(" sha256 ")?;
    if !is_sha256(sha256) || text != launcher_text(tag, sha256) { return None; }
    let file = tools.join(version_name(sha256));
    (sha256_file(&file).ok()? == sha256).then(|| Installed { tag: tag.to_string(), sha256: sha256.to_string(), file })
}

/// When the next check should run: every 6 hours while a copy is installed, every 5 minutes while
/// none is (Frank m3).
pub fn next_check(tools: &Path) -> Duration {
    if installed(tools).is_some() { REFRESH_EVERY } else { RETRY_WHILE_MISSING }
}

/// The `yt-dlp` line of a `SHA2-256SUMS` file (`<hex>  yt-dlp`). Other assets (`yt-dlp_macos`,
/// `yt-dlp.exe`, ...) are different names and never match.
pub fn wanted_hash(sums: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, name) = line.split_once(char::is_whitespace)?;
        (name.trim_start_matches([' ', '*']) == "yt-dlp" && is_sha256(&hash.to_ascii_lowercase()))
            .then(|| hash.to_ascii_lowercase())
    })
}

fn failed(what: &str, path: &Path, e: std::io::Error) -> SetupError {
    SetupError::InstallFailed { what: what.to_string(), detail: format!("{}: {e}", path.display()) }
}

fn set_mode(path: &Path, mode: u32) -> Result<(), SetupError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).map_err(|e| failed("yt-dlp", path, e))
}

/// Write `content` to `dest` by rename from inside `staging`, so a reader sees the old file or the
/// new one, never half of one.
fn replace_file(staging: &Path, dest: &Path, content: &str, mode: u32) -> Result<(), SetupError> {
    let incoming = staging.join(dest.file_name().unwrap_or_default());
    std::fs::write(&incoming, content).map_err(|e| failed("yt-dlp", &incoming, e))?;
    set_mode(&incoming, mode)?;
    std::fs::rename(&incoming, dest).map_err(|e| failed("yt-dlp", dest, e))
}

/// Remove what an earlier check left behind: version files the launcher no longer runs (m1: they
/// were superseded at a previous check, never at this one), and staging folders of a killed
/// process that are over an hour old. Only names this module writes are touched.
// A file that cannot be removed now is only disk space; the next check tries again, and nothing
// reads it in the meantime, so a failed removal is deliberately not an error.
#[allow(clippy::let_underscore_must_use)]
fn remove_superseded(tools: &Path, current: Option<&Installed>) {
    let Ok(entries) = std::fs::read_dir(tools) else { return };
    let keep = current.map(|c| version_name(&c.sha256));
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let path = entry.path();
        if let Some(short) = name.strip_prefix("yt-dlp-") {
            let ours = short.len() == 12 && short.bytes().all(|b| b.is_ascii_hexdigit());
            if ours && Some(&name) != keep.as_ref() { let _ = std::fs::remove_file(&path); }
        } else if name.starts_with("yt-dlp.download.") {
            let old = entry.metadata().ok().and_then(|m| m.modified().ok())
                .and_then(|t| t.elapsed().ok()).is_some_and(|age| age > STALE_STAGING);
            if old { let _ = std::fs::remove_dir_all(&path); }
        }
    }
}

/// **One check**: make sure the newest nightly yt-dlp is installed in `tools`.
///
/// 1. Write `yt-dlp.conf` if it is missing or different.
/// 2. Remove versions superseded at an EARLIER check (m1).
/// 3. Resolve the newest tag once (m3), fetch that release's `SHA2-256SUMS`.
/// 4. If the installed copy already has that hash, stop.
/// 5. Otherwise fetch that release's `yt-dlp` into a staging folder, check its hash (a mismatch is
///    deleted, not retried), `chmod 755`, rename it to `yt-dlp-<sha12>`, then swap the launcher.
///
/// Any error leaves the installed copy and its launcher exactly as they were.
pub fn refresh(fetcher: &dyn Fetcher, latest: &dyn LatestRelease, tools: &Path) -> Result<Refreshed, SetupError> {
    let _one_at_a_time = REFRESHING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    std::fs::create_dir_all(tools).map_err(|e| failed("the tools folder", tools, e))?;
    let current = installed(tools);
    remove_superseded(tools, current.as_ref());

    let staging = Staging::new(&tools.join("yt-dlp"), "download")?;
    let config = tools.join("yt-dlp.conf");
    if std::fs::read_to_string(&config).ok().as_deref() != Some(CONFIG) {
        replace_file(staging.path(), &config, CONFIG, 0o644)?;
    }

    let tag = latest.latest_tag()?;
    let release = format!("{NIGHTLY_RELEASES}/download/{tag}");
    let sums_url = format!("{release}/SHA2-256SUMS");
    let sums_path = staging.path().join("SHA2-256SUMS");
    let sums_size = fetcher.fetch(&sums_url, &sums_path)?;
    if sums_size > SUMS_LIMIT {
        return Err(SetupError::DownloadFailed { url: sums_url, status: format!("{sums_size} bytes, more than a checksum list") });
    }
    let sums = std::fs::read_to_string(&sums_path).map_err(|e| failed("yt-dlp's checksum list", &sums_path, e))?;
    let wanted = wanted_hash(&sums)
        .ok_or_else(|| SetupError::DownloadFailed { url: sums_url.clone(), status: "no yt-dlp line in the checksum list".into() })?;
    if current.as_ref().is_some_and(|c| c.sha256 == wanted) {
        return Ok(Refreshed::Unchanged { tag });
    }

    let file_url = format!("{release}/yt-dlp");
    let incoming = staging.path().join("yt-dlp");
    fetcher.fetch(&file_url, &incoming)?;
    let got = sha256_file(&incoming).map_err(|e| failed("yt-dlp", &incoming, e))?;
    if got != wanted {
        // Deleted, not retried: dropping the staging folder removes the mismatched file with it.
        drop(staging);
        return Err(SetupError::DigestMismatch { url: file_url, expected: wanted, got });
    }
    set_mode(&incoming, 0o755)?;
    let version = tools.join(version_name(&wanted));
    std::fs::rename(&incoming, &version).map_err(|e| failed("yt-dlp", &version, e))?;
    replace_file(staging.path(), &tools.join("yt-dlp"), &launcher_text(&tag, &wanted), 0o755)?;
    Ok(Refreshed::Installed { tag, sha256: wanted, previous: current.map(|c| c.tag) })
}

/// The app's check, with the real network: `CurlFetcher` for both the tag and the files.
pub fn refresh_with_curl(tools: &Path) -> Result<Refreshed, SetupError> {
    refresh(&CurlFetcher, &CurlFetcher, tools)
}

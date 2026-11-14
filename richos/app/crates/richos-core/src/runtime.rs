//! Verified runtime delivery, separate from private app state and target repositories.
use serde::Deserialize;
use sha2_fast::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, thiserror::Error)]
#[error("RichOS runtime setup is incomplete: {0}")]
pub struct RuntimeError(pub String);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Delivery {
    schema: u32,
    platform: String,
    versions: BTreeMap<String, String>,
    files: BTreeMap<String, String>,
    links: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct EngineRuntime {
    pub root: PathBuf,
    pub python: PathBuf,
    pub node: PathBuf,
    pub git: PathBuf,
    pub versions: BTreeMap<String, String>,
}

/// What the filesystem says about a delivered file without reading it: size, both times (ctime
/// cannot be set from userland, so a write that put `mtime` back is still seen), inode and device.
#[derive(Clone, PartialEq, Eq)]
struct Stamp(u64, i64, i64, i64, i64, u64, u64);

fn stamp_of(meta: &std::fs::Metadata) -> Stamp {
    use std::os::unix::fs::MetadataExt;
    Stamp(meta.len(), meta.mtime(), meta.mtime_nsec(), meta.ctime(), meta.ctime_nsec(), meta.ino(), meta.dev())
}

/// **A file whose bytes this process has already hashed against the digest the delivery names,
/// and which the filesystem says is the same file since** (hunt part 1 finding 30). Every
/// conversation or work connection verifies the runtime; reading and hashing every delivered file
/// again for each one is the cost this removes. Only the byte read is skipped: the inventory walk,
/// the link checks, the escape checks and the executable checks still run on every load, so an
/// unexpected, missing, re-linked or re-permissioned file is refused as before. A file that is
/// touched at all (its size, either time, its inode) is hashed again.
///
/// **AND IT OUTLIVES THE PROCESS** (launch-speed audit 2026-11-14, piece 1; Sage's review,
/// finding 5). Remembered only in memory, every launch read and hashed all 431 MB in 6,542 files
/// while the window waited. So the app names a file in ITS OWN data folder
/// ([`remember_verified_at`]) and a load reads what an earlier process verified from it and
/// writes back what it verified itself. Never inside the engine folder: anything new under
/// `runtime/` is an unexpected file and fails the engine, and an engine shipped inside the app
/// bundle would have its bundle checksum broken by a write there. A saved entry is trusted
/// exactly as an in-memory one is: only while the file's stamp is unchanged AND the digest it
/// was hashed to is the one the delivery names now.
#[derive(Default)]
struct Verified {
    files: std::collections::HashMap<PathBuf, (Stamp, String)>,
    hashed: std::collections::HashMap<PathBuf, usize>,
    /// The runtime roots whose saved file this process has already read.
    read_saved: BTreeSet<PathBuf>,
}
static VERIFIED: Mutex<Option<Verified>> = Mutex::new(None);
/// Where verified stamps are saved between processes; `None` keeps them in memory only, as
/// every caller but the app (a test, an example) has always had it.
static SAVED_AT: Mutex<Option<PathBuf>> = Mutex::new(None);

/// How many delivered files this process has read and hashed under `root` (the canonical
/// runtime directory). An observation for tests and the boot log.
pub fn files_hashed_under(root: &Path) -> usize {
    VERIFIED.lock().ok().and_then(|held| held.as_ref()?.hashed.get(root).copied()).unwrap_or(0)
}

/// Save what this process verifies to `file`, and start from what an earlier process saved
/// there. The app calls it once, before its first engine check, with a file in its own data
/// folder (under `RICHOS_TEST_DATA_DIR` that is the test's folder, never the live one). The app
/// and its dictation process both write it: a write is a temporary file of this process renamed
/// into place, and a file that does not read cleanly is ignored, which costs one full check.
pub fn remember_verified_at(file: &Path) {
    // (The app's file is `<its data folder>/`[`VERIFIED_FILE`], in the app and its dictation
    // process alike.)
    if let Ok(mut at) = SAVED_AT.lock() {
        *at = Some(file.to_path_buf());
    }
}

/// The saved file's name in the app's data folder.
pub const VERIFIED_FILE: &str = "runtime-verified.txt";
const SAVED_HEADER: &str = "richos-runtime-verified 1";

/// The saved file's entries for `root`, or nothing when it is missing, for another root, or not
/// exactly in the shape [`save_verified`] writes. One line per file:
/// `<digest> <size> <mtime> <mtime_ns> <ctime> <ctime_ns> <inode> <device>\t<relative path>`.
fn read_saved(file: &Path, root: &Path) -> Vec<(PathBuf, Stamp, String)> {
    let Ok(text) = std::fs::read_to_string(file) else { return Vec::new() };
    let mut lines = text.split('\n');
    let root_line = format!("root\t{}", root.display());
    if lines.next() != Some(SAVED_HEADER) || lines.next() != Some(root_line.as_str()) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for line in lines {
        if line.is_empty() { continue; }
        let Some((fields, name)) = line.split_once('\t') else { return Vec::new() };
        let parts: Vec<&str> = fields.split(' ').collect();
        let parsed = (|| -> Option<(Stamp, String)> {
            let [digest, size, mt, mtn, ct, ctn, ino, dev] = parts.as_slice() else { return None };
            if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) { return None; }
            Some((Stamp(size.parse().ok()?, mt.parse().ok()?, mtn.parse().ok()?, ct.parse().ok()?,
                ctn.parse().ok()?, ino.parse().ok()?, dev.parse().ok()?), digest.to_string()))
        })();
        match parsed {
            Some((stamp, digest)) if relative(name) => out.push((root.join(name), stamp, digest)),
            _ => return Vec::new(),
        }
    }
    out
}

/// Write every verified entry under `root` to the saved file. Best effort: a file that cannot be
/// written costs the next launch a full check, never a refusal.
fn save_verified(file: &Path, root: &Path, files: &std::collections::HashMap<PathBuf, (Stamp, String)>) {
    let mut text = format!("{SAVED_HEADER}\nroot\t{}\n", root.display());
    let mut rows: Vec<_> = files.iter().filter_map(|(path, entry)| Some((path.strip_prefix(root).ok()?, entry))).collect();
    rows.sort_by(|a, b| a.0.cmp(b.0));
    for (name, (Stamp(size, mt, mtn, ct, ctn, ino, dev), digest)) in rows {
        let Some(name) = name.to_str().filter(|n| relative(n)) else { continue };
        text.push_str(&format!("{digest} {size} {mt} {mtn} {ct} {ctn} {ino} {dev}\t{name}\n"));
    }
    let temporary = file.with_extension(format!("{}.tmp", std::process::id()));
    let written = file.parent().map(std::fs::create_dir_all).transpose().is_ok()
        && std::fs::write(&temporary, text).is_ok()
        && std::fs::rename(&temporary, file).is_ok();
    if !written { std::fs::remove_file(&temporary).ok(); }
}

/// Forget everything verified under `root`, in memory and in the saved file, so the next load
/// reads and hashes every delivered file. The ending of [`recheck_runtime_bytes`] on a mismatch.
fn forget_verified(root: &Path) {
    if let Ok(mut held) = VERIFIED.lock() {
        if let Some(held) = held.as_mut() {
            held.files.retain(|path, _| !path.starts_with(root));
        }
    }
    if let Some(file) = SAVED_AT.lock().ok().and_then(|at| at.clone()) {
        if !read_saved(&file, root).is_empty() { std::fs::remove_file(file).ok(); }
    }
}

/// What the late full check of the engine runtime found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeRecheck {
    /// Every delivered file was read and hashed to the digest the delivery names.
    Confirmed,
    /// This file's bytes are not what the delivery names. Everything verified under the runtime
    /// is forgotten, so the next engine check reads every file, refuses, and set-up offers the
    /// reinstall it offers today (`setup::find_engine`'s "there but cannot be used").
    Changed(String),
}

/// **THE FULL CHECK, LATE AND IN THE BACKGROUND** (Sage's review, finding 6). With the stamps
/// saved, a launch reads no bytes; what a stamp cannot see is damage to a file's contents that
/// changes nothing else about it. This reads every delivered file regardless of stamps, from the
/// caller's thread, which should be a late, low-priority one: it is 431 MB of reading. Its
/// ending is defined: on a mismatch it forgets what was verified, and work already running is
/// left alone (stopping it over a damaged file it may never touch is worse than the damage; the
/// review's stated default).
pub fn recheck_runtime_bytes(engine: &Path) -> Result<RuntimeRecheck, RuntimeError> {
    let root = std::fs::canonicalize(engine.join("runtime")).map_err(|_| RuntimeError("the selected engine has no delivered runtimes".into()))?;
    let data = std::fs::read(root.join("delivery.json")).map_err(|e| RuntimeError(e.to_string()))?;
    let delivery: Delivery = serde_json::from_slice(&data).map_err(|e| RuntimeError(e.to_string()))?;
    for (name, wanted) in &delivery.files {
        if !relative(name) || interpreter_bytecode_cache(name) { continue; }
        let same = std::fs::File::open(root.join(name)).ok()
            .and_then(|mut file| hash_file(&mut file).ok())
            .is_some_and(|digest| digest.eq_ignore_ascii_case(wanted));
        if !same {
            forget_verified(&root);
            return Ok(RuntimeRecheck::Changed(name.clone()));
        }
    }
    Ok(RuntimeRecheck::Confirmed)
}

/// Lower-case hex SHA-256 of a file's bytes, spelled as `format!("{:x}")` spelled 0.10's digest
/// (0.11's has no hex formatting).
fn hash_file(file: &mut std::fs::File) -> std::io::Result<String> {
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let length = file.read(&mut buffer)?;
        if length == 0 { break; }
        hash.update(&buffer[..length]);
    }
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// One entry of the runtime as the walk found it, without following a link.
enum Found {
    File(std::fs::Metadata),
    Link,
}

/// **THE TREE, LISTED ONCE** (Sage's review, finding 5). Every delivered file used to be resolved
/// path by path (one lookup per folder level), opened, and only then compared, and the tree was
/// listed again afterwards. Now it is listed once without following links, each entry `lstat`ed
/// in the same pass. A file reached only through real folders from the runtime's own resolved
/// root cannot be outside it, so the per-file resolve is not needed and no check is lost: a
/// delivered name that is a link, or that lies under a link, is not a regular file in this list.
fn walk(root: &Path, directory: &Path, found: &mut BTreeMap<String, Found>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let path = entry.path();
        if kind.is_dir() { walk(root, &path, found)?; continue; }
        let name = path.strip_prefix(root).unwrap().to_string_lossy().to_string();
        if kind.is_file() {
            // A regular file only: a symlinked "cache" is recorded, and refused as unexpected.
            if interpreter_bytecode_cache(&name) { continue; }
            found.insert(name, Found::File(entry.metadata()?));
        } else if kind.is_symlink() {
            found.insert(name, Found::Link);
        } else {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unsupported runtime file type"));
        }
    }
    Ok(())
}

fn relative(name: &str) -> bool {
    !name.is_empty() && !name.contains('\n') && Path::new(name).components()
        .all(|part| matches!(part, std::path::Component::Normal(_)))
}

/// **Is this a bytecode cache the delivered Python writes for itself?** `python/…/__pycache__/
/// <module>.<tag>.pyc`, or the `<that>.<digits>` temporary importlib writes it through.
///
/// # WHY THESE ARE NOT PART OF THE DELIVERY — the CEO's startup sheet, 2026-10-29
///
/// Until this rule, one Python run of the delivered interpreter without the launcher's `-B` and
/// `PYTHONDONTWRITEBYTECODE` (any `subprocess.run([sys.executable, …], env={…})` that rebuilds
/// its environment) made a READY engine unready for good: it wrote `__pycache__/*.pyc` beside
/// the standard library and recompiled the three delivered `encodings` caches that relocation
/// invalidates (`build-runtimes.py`, `prepare_runtime_launchers`). The next launch's
/// [`verify_engine`] then said *"runtime delivery has missing or unexpected files"*,
/// `setup::find_engine` recorded the engine as *"there but cannot be used"*, and the window
/// asked him to set up an engine that was installed, pinned and working. MEASURED on his Mac:
/// `engine.previous.8f630c73…` (installed 06:24:20 UTC from the pinned `9ddce07265cd`) held 36
/// unexpected caches written at 07:07:09/08:30:52 and 3 recompiled ones; `engine.previous.
/// 8d0dd53e…` (installed 11:43:50 from the same asset) held 47 written at 18:19:24 and the same
/// 3. Each was moved aside by a reinstall at the next start (11:43:50, 18:33:38) and replaced by
/// the same asset. With these caches forgiven, both directories verify clean.
///
/// What forgiving them gives up, said plainly: a hand-forged cache in `__pycache__` is no longer
/// caught here. This check is a delivery check, not a defense against a writer who already has
/// his files: anyone able to write `runtime/` can equally write the engine's own Python
/// (`scripts/`, `ecs/`, …), which [`verify_engine`] has never hashed. The `.py` sources a cache
/// is compiled from are still hashed. Everything else stays exactly as strict: a cache must be a
/// REGULAR file directly inside a `__pycache__` directory under `python/`, so a symlinked cache,
/// a `.py` dropped in `__pycache__`, a stray `.pyc` anywhere else, or a changed source is refused.
fn interpreter_bytecode_cache(name: &str) -> bool {
    let path = Path::new(name);
    let parent = path.parent().and_then(Path::file_name);
    let Some(file) = path.file_name().and_then(|f| f.to_str()) else { return false };
    if !name.starts_with("python/") || parent != Some(std::ffi::OsStr::new("__pycache__")) {
        return false;
    }
    match file.rsplit_once(".pyc") {
        Some((stem, "")) => !stem.is_empty(),
        Some((stem, temporary)) => !stem.is_empty() && temporary.strip_prefix('.')
            .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())),
        None => false,
    }
}

impl EngineRuntime {
    /// The override is an explicit development/testing input. Ordinary delivery
    /// uses the runtime in the selected engine, never Homebrew or a visited repo.
    pub fn load(engine: &Path, explicit: Option<&Path>) -> Result<Self, RuntimeError> {
        let chosen = explicit.map(Path::to_path_buf).unwrap_or_else(|| engine.join("runtime"));
        let root = std::fs::canonicalize(&chosen).map_err(|_| RuntimeError("the selected engine has no delivered runtimes".into()))?;
        let manifest = root.join("delivery.json");
        if std::fs::metadata(&manifest).map(|m| m.len()).unwrap_or(u64::MAX) > 2 * 1024 * 1024 {
            return Err(RuntimeError("runtime inventory is missing or too large".into()));
        }
        let data = std::fs::read(&manifest).map_err(|e| RuntimeError(e.to_string()))?;
        let delivery: Delivery = serde_json::from_slice(&data).map_err(|e| RuntimeError(e.to_string()))?;
        if delivery.schema != 1 || delivery.platform != "aarch64-apple-darwin" || !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            return Err(RuntimeError("unsupported runtime delivery".into()));
        }
        let mut found = BTreeMap::new();
        walk(&root, &root, &mut found).map_err(|e| RuntimeError(e.to_string()))?;
        // What an earlier process verified and saved, read once per runtime per process.
        let saved_at = SAVED_AT.lock().ok().and_then(|at| at.clone());
        if let Some(file) = &saved_at {
            if let Ok(mut held) = VERIFIED.lock() {
                let held = held.get_or_insert_with(Verified::default);
                if held.read_saved.insert(root.clone()) {
                    for (path, stamp, digest) in read_saved(file, &root) {
                        held.files.entry(path).or_insert((stamp, digest));
                    }
                }
            }
        }
        let mut hashed_now = 0usize;
        let mut expected = BTreeSet::from(["delivery.json".to_string()]);
        for (name, wanted) in &delivery.files {
            if !relative(name) || wanted.len() != 64 || !wanted.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(RuntimeError("invalid runtime inventory entry".into()));
            }
            // Delivered, and the interpreter's to rewrite or drop (see `interpreter_bytecode_cache`):
            // neither hashed nor required, and left out of the inventory on both sides.
            if interpreter_bytecode_cache(name) { continue; }
            let path = root.join(name);
            let listed = match found.get(name) {
                Some(Found::File(meta)) => stamp_of(meta),
                Some(Found::Link) => return Err(RuntimeError(format!("runtime file escapes its delivery: {name}"))),
                // Missing, or reached only through a link: either way not a file of this delivery.
                None => return Err(RuntimeError(format!("missing runtime file: {name}"))),
            };
            let known = VERIFIED.lock().is_ok_and(|held| {
                held.as_ref()
                    .and_then(|v| v.files.get(&path))
                    .is_some_and(|(seen, digest)| *seen == listed && digest.eq_ignore_ascii_case(wanted))
            });
            if !known {
                use std::os::unix::fs::OpenOptionsExt;
                let mut file = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(&path)
                    .map_err(|e| RuntimeError(e.to_string()))?;
                // Stamped from the open handle BEFORE the read, so a write that lands during the
                // hash leaves a stamp that no longer matches and is hashed again next time.
                let stamp = stamp_of(&file.metadata().map_err(|e| RuntimeError(e.to_string()))?);
                if !hash_file(&mut file).map_err(|e| RuntimeError(e.to_string()))?.eq_ignore_ascii_case(wanted) {
                    return Err(RuntimeError(format!("runtime file changed: {name}")));
                }
                hashed_now += 1;
                if let Ok(mut held) = VERIFIED.lock() {
                    let held = held.get_or_insert_with(Verified::default);
                    held.files.insert(path.clone(), (stamp, wanted.clone()));
                    *held.hashed.entry(root.clone()).or_default() += 1;
                }
            }
            expected.insert(name.clone());
        }
        for (name, target) in &delivery.links {
            if !relative(name) { return Err(RuntimeError("invalid runtime link name".into())); }
            let path = root.join(name);
            if !matches!(found.get(name), Some(Found::Link))
                || std::fs::read_link(&path).ok().as_deref() != Some(Path::new(target))
                || !std::fs::canonicalize(&path).map(|p| p.starts_with(&root)).unwrap_or(false) {
                return Err(RuntimeError(format!("invalid runtime link: {name}")));
            }
            if !expected.insert(name.clone()) { return Err(RuntimeError("duplicate runtime member".into())); }
        }
        if !found.keys().eq(expected.iter()) { return Err(RuntimeError("runtime delivery has missing or unexpected files".into())); }
        // Save only after the whole delivery passed, and only when something was hashed now: a
        // launch that hashed nothing has nothing new to tell the next one.
        if let (Some(file), true) = (&saved_at, hashed_now > 0) {
            if let Ok(held) = VERIFIED.lock() {
                if let Some(held) = held.as_ref() {
                    save_verified(file, &root, &held.files);
                }
            }
        }
        // ffmpeg and ffprobe since 2026-10-07: Rich watches and hears videos with them, and his
        // PATH reaches nothing outside this runtime and the system folders.
        for relative in ["bin/python3", "bin/node", "bin/git", "bin/jq", "bin/ffmpeg", "bin/ffprobe", "git/libexec/git-core/git-remote-https"] {
            let path = root.join(relative);
            if !path.is_file() { return Err(RuntimeError(format!("runtime is missing {relative}"))); }
            #[cfg(unix)] {
                use std::os::unix::fs::PermissionsExt;
                if std::fs::metadata(&path).map_err(|e| RuntimeError(e.to_string()))?.permissions().mode() & 0o111 == 0 {
                    return Err(RuntimeError(format!("runtime is not executable: {relative}")));
                }
            }
        }
        Ok(Self { python: root.join("bin/python3"), node: root.join("bin/node"), git: root.join("bin/git"), root, versions: delivery.versions })
    }

    /// Rich's PATH, and every teammate's and hook's (`engine_profile.rs` sets it): the verified
    /// runtime first, then the system folders, then the app's tools folder (yt-dlp,
    /// `media_tools.rs`). The tools folder is LAST on purpose (media-tools review m2): it is
    /// user-writable and not hash-checked at load, so nothing put there may shadow a system
    /// command.
    pub fn path(&self) -> String {
        self.path_with(None)
    }

    /// [`Self::path`] with the user's own `PATH` (their login shell's, build plan item 4) after
    /// the runtime and before the system folders: the runtime's interpreters stay first, and
    /// every tool the user's terminal finds is found.
    pub fn path_with(&self, user: Option<&str>) -> String {
        let home = std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from);
        search_path_with(&self.root.join("bin"), user, home.map(|h| crate::media_tools::tools_dir(&h)).as_deref())
    }

    /// [`commands_path`] for this runtime: the PATH an agent's own commands run with.
    pub fn commands_path(&self, user: &str, stand_ins: &crate::user_git::StandIns) -> String {
        let home = std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from);
        commands_path(&self.root.join("bin"), user, home.map(|h| crate::media_tools::tools_dir(&h)).as_deref(), stand_ins)
    }
}

/// The PATH for a runtime `bin` folder and, if there is one, the tools folder. A folder whose
/// name holds `:` cannot be a PATH entry and is left out rather than split in two.
pub fn search_path(runtime_bin: &Path, tools: Option<&Path>) -> String {
    search_path_with(runtime_bin, None, tools)
}

/// The runtime `bin` folder, then each folder of `user` in its order, then the system folders,
/// then the tools folder; each folder once, at its first place.
pub fn search_path_with(runtime_bin: &Path, user: Option<&str>, tools: Option<&Path>) -> String {
    let runtime = runtime_bin.display().to_string();
    let tools = tools.map(|t| t.display().to_string()).filter(|t| !t.contains(':'));
    let mut entries: Vec<&str> = vec![&runtime];
    let user = user.unwrap_or_default().split(':').filter(|entry| !entry.is_empty());
    for entry in user.chain(["/usr/bin", "/bin", "/usr/sbin", "/sbin"]).chain(tools.as_deref()) {
        if !entries.contains(&entry) { entries.push(entry); }
    }
    entries.join(":")
}

/// **The PATH an agent's own commands run with** (Hank's parity audit 2026-10-30, item 2): each
/// folder of the user's `PATH` in its order, so `python3`, `node` and `git` are the ones the
/// user's terminal runs, with their packages; the runtime `bin` folder just before the first
/// folder that holds an Apple stand-in (`user_git::StandIns`), so a Mac without the developer
/// tools gets the runtime's copy and never the dialog, otherwise after the user's folders, so a
/// tool the user does not have is still found; then the system folders and the tools folder, as
/// in [`search_path_with`]. Each folder once, at its first place.
pub fn commands_path(runtime_bin: &Path, user: &str, tools: Option<&Path>, stand_ins: &crate::user_git::StandIns) -> String {
    let runtime = runtime_bin.display().to_string();
    let tools = tools.map(|t| t.display().to_string()).filter(|t| !t.contains(':'));
    let mut entries: Vec<&str> = Vec::new();
    let mut placed = false;
    for entry in user.split(':').filter(|entry| !entry.is_empty()) {
        if !placed && stand_ins.holds_one(Path::new(entry)) {
            if !entries.contains(&runtime.as_str()) { entries.push(&runtime); }
            placed = true;
        }
        if !entries.contains(&entry) { entries.push(entry); }
    }
    if !entries.contains(&runtime.as_str()) { entries.push(&runtime); }
    for entry in ["/usr/bin", "/bin", "/usr/sbin", "/sbin"].into_iter().chain(tools.as_deref()) {
        if !entries.contains(&entry) { entries.push(entry); }
    }
    entries.join(":")
}

/// Delivered interpreters must not import a launching terminal's modules or
/// startup scripts. Credentials and ordinary OS settings are not copied here.
pub fn isolate_interpreter_environment(command: &mut std::process::Command) {
    for key in INTERPRETER_KEYS {
        command.env_remove(key);
    }
    command.env(NO_USER_SITE, "1").env("PYTHONDONTWRITEBYTECODE", "1");
}

/// The user's interpreter settings [`isolate_interpreter_environment`] removes. An agent's own
/// commands get the user's back (`EngineProfile::commands_env`).
pub const INTERPRETER_KEYS: [&str; 8] = ["PYTHONPATH", "PYTHONHOME", "PYTHONSTARTUP", "PYTHONUSERBASE",
    "NODE_PATH", "NODE_OPTIONS", "BASH_ENV", "ENV"];

/// Set to `1` by [`isolate_interpreter_environment`]: Python skips the user's `pip --user` packages.
pub const NO_USER_SITE: &str = "PYTHONNOUSERSITE";
pub fn interpreter_command(path: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut command = std::process::Command::new(path);
    isolate_interpreter_environment(&mut command);
    command
}

/// Validate the delivered component set before activation. This establishes
/// presence and compatible protocol identities, not behavioral acceptance.
pub fn verify_engine(engine: &Path) -> Result<EngineRuntime, RuntimeError> {
    let read_json = |name: &str| -> Result<serde_json::Value, RuntimeError> {
        let path = engine.join(name);
        if std::fs::metadata(&path).map(|m| m.len()).unwrap_or(u64::MAX) > 64 * 1024 {
            return Err(RuntimeError(format!("{name} is missing or too large")));
        }
        serde_json::from_slice(&std::fs::read(path).map_err(|e| RuntimeError(e.to_string()))?)
            .map_err(|_| RuntimeError(format!("{name} is invalid")))
    };
    let version = std::fs::read_to_string(engine.join("VERSION")).unwrap_or_default();
    let compatibility = read_json("compatibility.json")?;
    let plugin = read_json(".claude-plugin/plugin.json")?;
    if version.trim() != "1.2.0" || plugin["version"] != "1.2.0" || compatibility["schema"] != 1 ||
        compatibility["app_version"] != "1.2.0" || compatibility["engine_version"] != "1.2.0" ||
        compatibility["archive_root"] != "engine" || compatibility["components"]["loro"]["context_schema"] != 1 ||
        compatibility["components"]["ecs"]["app_protocol"] != 1 {
        return Err(RuntimeError("the app and engine component contracts are incompatible".into()));
    }
    for (component, path) in [("loro", "loro"), ("ecs", "ecs"), ("mega_lander", "mega-lander"), ("ass_kicker", "ass-kicker")] {
        if compatibility["components"][component]["path"] != path {
            return Err(RuntimeError(format!("{component} has an unsupported component location")));
        }
    }
    for name in ["loro/bin/loro-context.mjs", "loro/bin/loro-write.mjs", "loro/lib/layout.js",
        "ecs/bin/ecs", "ecs/adapters/app.py", "ecs/adapters/mcp.py", "ecs/core/ecs_core.py",
        "ecs/migrations/007_correction_reobservations.sql", "mega-lander/workspaces.py",
        "mega-lander/create-teammate-worktree.sh", "mega-lander/app.py", "mega-lander/DESKTOP.md", "ass-kicker/brief-provenance.py",
        "ass-kicker/brief-scope.py", "ass-kicker/ass-kicker.py",
        "scripts/lib/app-evidence.py", "scripts/spawn.sh", "scripts/lib/spawn.py",
        "scripts/codex-channel.sh", "scripts/lib/codex_channels.py",
        // The classification of whose work each spawn guard's reason protects, and its
        // reader. Without them the app cannot tell which guards may judge a dispatch it
        // makes on a user's own Mac, and `EngineProfile::prepare` refuses rather than
        // guessing — so their absence is a delivery fault, named here where every other
        // missing component entry point is named.
        crate::engine_profile::GUARD_AUDIENCE_DECLARATION, "scripts/lib/spawn-guard-audience.py",
        "scripts/hooks/guard-worktree-isolation.sh", "scripts/hooks/guard-brief-scope.sh",
        "scripts/app-engine-hook.py", "scripts/provider-supervisor.py", "mega-lander/duties/worker.md", "mega-lander/duties/reviewer.md",
        "mega-lander/duties/consult.md"] {
        if !engine.join(name).is_file() { return Err(RuntimeError(format!("missing component entry point: {name}"))); }
    }
    EngineRuntime::load(engine, None)
}

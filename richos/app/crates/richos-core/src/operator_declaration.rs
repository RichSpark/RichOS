//! THE OPERATOR DECLARATION — the one file that switches his own team on behind the app,
//! on his Mac and nowhere else (operator back-end spec r2, richos-hq
//! `docs/plans/2026-09-24-operator-back-end-spec-r2.md` (f); CEO ruling §86).
//!
//! **Three states, and only the first is the product.**
//!
//! | `<app data>/operator.json` | What runs |
//! |---|---|
//! | absent | the product back end, exactly as today ([`Gate::Product`]) |
//! | present and valid | his team, from his engine ([`Gate::Operator`]) |
//! | present and invalid | **no background work at all** ([`Gate::Refused`]) |
//!
//! The third row is Frank's B8 and it is the reason this file exists as a gate rather than a
//! config reader with a default. On his Mac the product back end would land into his real
//! repositories as `RichOS <richos@localhost>` with the generic worker, so a broken file is
//! never read as "no file". A broken file refuses work until it is fixed or removed, and the
//! refusal names what is wrong in one sentence.
//!
//! **Only absence is the product path**, and absence means the path does not exist at all
//! (`ENOENT`). A file that exists and cannot be read, a directory, a link, an empty file:
//! each is present, so each refuses.
//!
//! **The land fences are part of "valid"** (spec r3 §11 item 2, e8). A declaration is valid
//! only while `OPERATOR_FENCES="on"` stands in his `orchestration.config` AND
//! `<declared engine>/scripts/operator-fences.sh status` exits 0. The engine does not ship that
//! script yet (zach, r3 §6), so today every declaration refuses at this step, which is the
//! intended order: no operator lead opens without the fences.
//!
//! ## What the file holds (schema 1)
//!
//! Written only by the private `enable.sh` (spec §6, zach), read only here. Every path is
//! absolute and is used as written, whatever this app's own `HOME` is: a nightly runs with a
//! scratch `HOME` (`richos/app/NIGHTLY.md`), and his team must run with his.
//!
//! ```json
//! {
//!   "schema": 1,
//!   "entity_root": "/Users/<him>/ab/femcboost",
//!   "engine_root": "/Users/<him>/ab/richos/richos/engine",
//!   "home": "/Users/<him>",
//!   "claim": {"file": "/Users/<him>/.claude/state/operator-lead.json",
//!             "lock": "/Users/<him>/.claude/state/operator-lead.lock"},
//!   "permission_mode": "bypassPermissions",
//!   "environment": {"PATH": "…", "LANG": "…", "USER": "…"},
//!   "origins": ["desk-typed", "desk-voice", "desk-file"],
//!   "file_roots": ["/Users/<him>/ab"],
//!   "contract": {"path": "/…/operator-contract.md", "sha256": "<64 hex>"}
//! }
//! ```
//!
//! Unknown fields are refused. A gate that skipped a field it did not know would let a typo
//! in a safety field read as that field being absent.
//!
//! ## The environment rule, (i)
//!
//! `environment` holds the names `enable.sh` captured from his terminal, with their values.
//! It never holds a credential: any `ANTHROPIC_*` or `CLAUDE*` name, or any name containing
//! `TOKEN`, `KEY`, `SECRET` or `PASSWORD`, refuses the whole file ([`is_denied_name`]). That
//! covers `CLAUDE_CODE_MESSAGING_TOKEN`, which his terminal carries (spec §2). It also never
//! holds `HOME`, `SSH_AUTH_SOCK` or `TMPDIR`, which are re-derived at every lead start
//! ([`REDERIVED_NAMES`]), nor a `RICHOS_*` name, which only the app sets. This is the license
//! condition in `native.rs`'s module doc: RichOS never collects, stores or passes a Claude
//! credential. `enable.sh` refuses to write such a file; this refuses to read one, so a
//! hand-edited file cannot slip one through.
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

/// The file name, inside the install's own data folder (`app_data_dir()`).
pub const DECLARATION_FILE: &str = "operator.json";
/// The only schema this build reads.
pub const SCHEMA: u32 = 1;
/// A declaration is a few hundred bytes. A megabyte of it is not a declaration.
const MAX_DECLARATION_BYTES: u64 = 64 * 1024;
/// The operator contract is a page of text; this bounds the digest read.
const MAX_CONTRACT_BYTES: u64 = 256 * 1024;

/// Names the app derives itself at every lead start, never stored (spec (i)).
///
/// `HOME` comes from [`Declaration::home`]; `SSH_AUTH_SOCK` from `launchctl getenv`; `TMPDIR`
/// from the per-user temporary directory. A stored copy of any of them would be a second
/// source for one value, and the stored one would be the stale one.
pub const REDERIVED_NAMES: [&str; 3] = ["HOME", "SSH_AUTH_SOCK", "TMPDIR"];

/// The intake channels an assignment can come from (spec (s)). `phone` is valid so that
/// opening the phone to his team is one line in this file, and his call; `enable.sh` does not
/// write it.
pub const KNOWN_ORIGINS: [&str; 4] = ["desk-typed", "desk-voice", "desk-file", "phone"];

/// `claude --permission-mode` choices, read from `claude --help` on 2.1.282.
pub const KNOWN_PERMISSION_MODES: [&str; 6] =
    ["acceptEdits", "auto", "bypassPermissions", "manual", "dontAsk", "plan"];

/// The engine files the app runs for the operator, from HIS engine (spec (b), B9). A root
/// without them is not an engine the app can use, and saying so at the gate beats failing
/// at the first stop.
pub const REQUIRED_ENGINE_FILES: [&str; 5] = [
    ".claude-plugin/plugin.json",
    "hooks/hooks.json",
    "scripts/provider-supervisor.py",
    "scripts/agent-liveness.sh",
    "scripts/stop.sh",
];

/// The claim's two file names (spec (e), "The claim", items 2 and 5).
pub const CLAIM_FILE_NAME: &str = "operator-lead.json";
pub const CLAIM_LOCK_NAME: &str = "operator-lead.lock";

/// The switch in his `orchestration.config` that turns the land fences on (spec r3 e8). Off
/// by default, and off means his team does not run in the app (r3 §11 item 2).
pub const FENCE_SWITCH: &str = "OPERATOR_FENCES";
/// The engine's own answer to "are the fences installed and active?" (r3 e8), run from the
/// DECLARED root with `status` as its only argument.
pub const FENCE_STATUS_SCRIPT: &str = "scripts/operator-fences.sh";
/// How long the fence status command may take before it counts as failed. It reads three
/// hook files and a config line; twenty seconds is a hung command, not a slow one.
pub const FENCE_STATUS_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// Asks the engine whether the fences are on and installed. A trait so the gate's own tests
/// can say "the fence answered yes" without a fence, and the real runner is tested apart.
pub trait FenceStatus {
    /// `Ok(())` only when the status command exited 0. `Err` carries a short reason.
    fn status(&self, declaration: &Declaration) -> Result<(), String>;
}

/// The real runner: `<declared engine>/scripts/operator-fences.sh status`, from his entity
/// folder, with his stored environment and his `HOME` and nothing of the app's (B9, (i)).
///
/// **A missing script is a failure** (r3 §11 item 4). Until the engine ships it, no declaration
/// can switch his team on, which is the intended order: no operator lead opens without the
/// fences.
pub struct EngineFenceStatus;

impl FenceStatus for EngineFenceStatus {
    fn status(&self, declaration: &Declaration) -> Result<(), String> {
        let script = declaration.engine_root.join(FENCE_STATUS_SCRIPT);
        if !script.is_file() {
            return Err(format!("{FENCE_STATUS_SCRIPT} is missing from the engine"));
        }
        let mut command = std::process::Command::new("/bin/bash");
        command.arg(&script).arg("status")
            .current_dir(&declaration.entity_root)
            .env_clear()
            .envs(script_environment(declaration))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        crate::owned_process::OwnedChild::configure(&mut command);
        let child = command.spawn().map_err(|e| format!("{FENCE_STATUS_SCRIPT} could not be started ({e})"))?;
        // Owned: this pid is the one just spawned, in its own group, and the group is
        // fenced on every exit path by `OwnedChild`'s drop.
        let mut child = crate::owned_process::OwnedChild::new(child);
        let deadline = std::time::Instant::now() + FENCE_STATUS_TIMEOUT;
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(status)) => return Err(format!("{FENCE_STATUS_SCRIPT} status answered {status}")),
                Ok(None) if std::time::Instant::now() >= deadline => {
                    return Err(format!("{FENCE_STATUS_SCRIPT} status did not answer within {} s",
                        FENCE_STATUS_TIMEOUT.as_secs()));
                }
                Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
                Err(e) => return Err(format!("{FENCE_STATUS_SCRIPT} status could not be waited for ({e})")),
            }
        }
    }
}

/// The environment an engine script runs with on his behalf, outside a lead: his stored
/// names and his `HOME`, built from empty. The lead itself gets more (`operator_profile.rs`).
pub fn script_environment(declaration: &Declaration) -> BTreeMap<String, String> {
    let mut environment = declaration.environment.clone();
    environment.insert("HOME".into(), declaration.home.display().to_string());
    environment
}

/// Is `OPERATOR_FENCES` on in his `orchestration.config`? Shell-style `NAME="value"`; the last
/// assignment wins, as it would for the shell that sources the file. Absent is off.
fn fence_switch_on(entity_root: &Path) -> Result<bool, Refusal> {
    let text = std::fs::read_to_string(entity_root.join("orchestration.config"))
        .map_err(|_| Refusal::new("its orchestration.config could not be read"))?;
    let mut on = false;
    for line in text.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix(FENCE_SWITCH).and_then(|rest| rest.strip_prefix('=')) {
            let value = value.split('#').next().unwrap_or("").trim().trim_matches('"').trim_matches('\'');
            on = value == "on";
        }
    }
    Ok(on)
}

/// What the gate decided for this install.
#[derive(Debug)]
pub enum Gate {
    /// No declaration: the product back end, exactly as before this file existed.
    Product,
    /// A declaration is present and is wrong. No background work runs on this install.
    Refused(Refusal),
    /// A valid declaration: his team, from his engine.
    Operator(Box<Declaration>),
}

/// Why a present declaration refuses work. `what` is a plain phrase that completes the
/// sentence [`Refusal::sentence`] gives him.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub what: String,
}

impl Refusal {
    fn new(what: impl Into<String>) -> Self {
        Refusal { what: what.into() }
    }

    /// The one sentence he is told (spec (f), B8): what is wrong, and that Rich can fix it.
    /// It says his team is off, because the front desk still talks and must not imply
    /// otherwise.
    pub fn sentence(&self) -> String {
        format!("Your team is switched off on this Mac because {}. Rich can fix it.", self.what)
    }
}

/// The sentence for a valid declaration on a build that does not carry the operator back end
/// yet. **Not the product path**: B8's reason holds for a valid file exactly as for a broken
/// one, so the customer worker never runs on an install that has the file.
/// **A declaration that appeared after this launch.** Operator mode is decided once, when the
/// app starts (the shell builds his team's desk only then), so a file written later is refused
/// by the work lease's gate rather than half-honored, and never becomes the product path.
pub const SWITCHED_ON_AFTER_LAUNCH: &str =
    "Your team was switched on after RichOS started, so no background work will start. Quit RichOS and open it again.";

pub const NOT_IN_THIS_BUILD: &str =
    "Your team is switched on for this Mac, but this build of RichOS can't run it yet, so no background work will start.";

/// Where the claim lives. Both files sit side by side under his `~/.claude`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaimPaths {
    pub file: PathBuf,
    pub lock: PathBuf,
}

/// A declaration that passed every check. Paths are canonical where they must exist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    pub entity_root: PathBuf,
    pub engine_root: PathBuf,
    pub home: PathBuf,
    pub claim: ClaimPaths,
    pub permission_mode: String,
    pub environment: BTreeMap<String, String>,
    pub origins: Vec<String>,
    pub file_roots: Vec<PathBuf>,
    pub contract_path: PathBuf,
    pub contract_sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawClaim {
    file: PathBuf,
    lock: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawContract {
    path: PathBuf,
    sha256: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDeclaration {
    schema: u32,
    entity_root: PathBuf,
    engine_root: PathBuf,
    home: PathBuf,
    claim: RawClaim,
    permission_mode: String,
    environment: BTreeMap<String, String>,
    origins: Vec<String>,
    file_roots: Vec<PathBuf>,
    contract: RawContract,
}

/// Is this variable name one that may never reach his lead from a stored file?
///
/// A credential rule, stated as names because values are never read for it: any
/// `ANTHROPIC_*` name, any name starting `CLAUDE` (which covers `CLAUDECODE` and every
/// `CLAUDE_CODE_*` per-session name `claude` sets for itself), and any name that contains
/// `TOKEN`, `KEY`, `SECRET` or `PASSWORD` in any case.
pub fn is_denied_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    upper.starts_with("ANTHROPIC_")
        || upper.starts_with("CLAUDE")
        || ["TOKEN", "KEY", "SECRET", "PASSWORD"].iter().any(|word| upper.contains(word))
}

fn is_variable_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Absolute, and free of `.`/`..`, so the path means what it says before anything resolves it.
fn plain_absolute(path: &Path) -> bool {
    path.is_absolute() && path.components().all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
}

/// Read the gate for one install's data folder. Never writes anything.
///
/// Called where a back end is about to open, not once at boot, so removing the file (which
/// is what `disable.sh` does) takes effect at the next back end without a relaunch.
///
/// With no file, nothing is run and nothing is read beyond one `lstat`. With a file, the
/// fences are asked last, after every check that needs no process.
pub fn gate(data_dir: &Path) -> Gate {
    gate_with(data_dir, &EngineFenceStatus)
}

/// [`gate`], with the fence status answered by `fences`.
pub fn gate_with(data_dir: &Path, fences: &dyn FenceStatus) -> Gate {
    match read_declaration(data_dir) {
        Gate::Operator(declaration) => match fence_switch_on(&declaration.entity_root) {
            Err(refusal) => Gate::Refused(refusal),
            Ok(false) => refuse(format!(
                "the land fences are switched off ({FENCE_SWITCH} is not \"on\" in its orchestration.config)")),
            Ok(true) => match fences.status(&declaration) {
                Ok(()) => Gate::Operator(declaration),
                Err(why) => refuse(format!("the land fences are not in place ({why})")),
            },
        },
        other => other,
    }
}

fn read_declaration(data_dir: &Path) -> Gate {
    let path = data_dir.join(DECLARATION_FILE);
    // `symlink_metadata`, never `metadata`: a link must be judged as a link, not as whatever
    // it points at, or a link to a valid file elsewhere would switch his team on from a file
    // `enable.sh` never wrote.
    let metadata = match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Gate::Product,
        Err(error) => return refuse(format!("the file that switches it on could not be read ({error})")),
        Ok(metadata) => metadata,
    };
    if !metadata.file_type().is_file() {
        return refuse("the file that switches it on is not a plain file");
    }
    if metadata.len() > MAX_DECLARATION_BYTES {
        return refuse("the file that switches it on is too large to be a declaration");
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) => return refuse(format!("the file that switches it on could not be read ({error})")),
    };
    // The version first, so a newer file gets the true sentence rather than a field error.
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
        if let Some(schema) = value.get("schema").and_then(serde_json::Value::as_u64) {
            if schema != u64::from(SCHEMA) {
                return refuse(format!(
                    "the file that switches it on is version {schema}, a version this build cannot read"));
            }
        }
    }
    let raw: RawDeclaration = match serde_json::from_str(&text) {
        Ok(raw) => raw,
        Err(error) => return refuse(format!(
            "the file that switches it on could not be read as a declaration ({error})")),
    };
    match validate(raw) {
        Ok(declaration) => Gate::Operator(Box::new(declaration)),
        Err(refusal) => Gate::Refused(refusal),
    }
}

fn refuse(what: impl Into<String>) -> Gate {
    Gate::Refused(Refusal::new(what))
}

fn existing_dir(label: &str, path: &Path) -> Result<PathBuf, Refusal> {
    if !plain_absolute(path) {
        return Err(Refusal::new(format!("the {label} path is not a plain absolute path ({})", path.display())));
    }
    let canonical = std::fs::canonicalize(path)
        .map_err(|_| Refusal::new(format!("the {label} folder does not exist ({})", path.display())))?;
    if !canonical.is_dir() {
        return Err(Refusal::new(format!("the {label} path is not a folder ({})", path.display())));
    }
    Ok(canonical)
}

fn validate(raw: RawDeclaration) -> Result<Declaration, Refusal> {
    if raw.schema != SCHEMA {
        return Err(Refusal::new(format!(
            "the file that switches it on is version {}, a version this build cannot read", raw.schema)));
    }
    let home = existing_dir("home", &raw.home)?;

    // ---- his entity: femcboost, with his rules and his record ---------------------------
    let entity_root = existing_dir("team", &raw.entity_root)?;
    for required in ["orchestration.config", "CLAUDE.md"] {
        if !entity_root.join(required).is_file() {
            return Err(Refusal::new(format!(
                "the team folder has no {required} ({})", entity_root.display())));
        }
    }

    // ---- his engine: the live checkout, never the plugin cache (B9, P1) ------------------
    if !plain_absolute(&raw.engine_root) {
        return Err(Refusal::new(format!(
            "the engine path is not a plain absolute path ({})", raw.engine_root.display())));
    }
    let engine_root = std::fs::canonicalize(&raw.engine_root).map_err(|_| Refusal::new(format!(
        "the engine folder does not exist ({})", raw.engine_root.display())))?;
    // Judged on the CANONICAL path, so a link into the cache is still the cache. The cache is
    // a copy made at install time; running scripts from it is the version skew (b) forbids.
    let cache_marker: PathBuf = [".claude", "plugins", "cache"].iter().collect();
    let in_cache = engine_root.starts_with(home.join(&cache_marker))
        || engine_root.components().collect::<Vec<_>>().windows(3).any(|w| {
            w.iter().map(|c| c.as_os_str()).eq(cache_marker.components().map(|c| c.as_os_str()))
        });
    if in_cache {
        return Err(Refusal::new(format!(
            "the engine it names is the plugin cache, not the live engine ({})", engine_root.display())));
    }
    for required in REQUIRED_ENGINE_FILES {
        if !engine_root.join(required).is_file() {
            return Err(Refusal::new(format!(
                "the engine it names has no {required} ({})", engine_root.display())));
        }
    }

    // ---- the claim's two files, side by side under his ~/.claude (claim item 5) ---------
    let claude_dir = home.join(".claude");
    let (file, lock) = (raw.claim.file, raw.claim.lock);
    let claim_ok = plain_absolute(&file) && plain_absolute(&lock)
        && file.file_name().is_some_and(|n| n == CLAIM_FILE_NAME)
        && lock.file_name().is_some_and(|n| n == CLAIM_LOCK_NAME)
        && file.parent().is_some() && file.parent() == lock.parent()
        && (file.starts_with(&claude_dir) || file.starts_with(raw.home.join(".claude")));
    if !claim_ok {
        return Err(Refusal::new(format!(
            "the claim files it names disagree: they must be {CLAIM_FILE_NAME} and {CLAIM_LOCK_NAME}, side by side under {}",
            claude_dir.display())));
    }

    // ---- the permission mode his terminal runs --------------------------------------------
    if !KNOWN_PERMISSION_MODES.contains(&raw.permission_mode.as_str()) {
        return Err(Refusal::new(format!(
            "it names a permission mode Claude Code does not have ({})", raw.permission_mode)));
    }

    // ---- the stored environment (i) --------------------------------------------------------
    for (name, value) in &raw.environment {
        if !is_variable_name(name) {
            return Err(Refusal::new(format!("its stored environment has a malformed name ({name})")));
        }
        if is_denied_name(name) {
            return Err(Refusal::new(format!(
                "its stored environment holds {name}, which may never be passed to your team")));
        }
        if REDERIVED_NAMES.contains(&name.as_str()) || (name.starts_with("RICHOS_") && !crate::machine_settings::KEYS.contains(&name.as_str())) {
            return Err(Refusal::new(format!(
                "its stored environment holds {name}, which the app sets itself at every start")));
        }
        if value.contains('\0') {
            return Err(Refusal::new(format!("its stored environment value for {name} is malformed")));
        }
    }
    if raw.environment.get("PATH").is_none_or(|path| path.is_empty()) {
        return Err(Refusal::new("its stored environment has no PATH"));
    }

    // ---- where his words may come from (s) ------------------------------------------------
    if raw.origins.is_empty() {
        return Err(Refusal::new("it lists no origin your team may take work from"));
    }
    for (index, origin) in raw.origins.iter().enumerate() {
        if !KNOWN_ORIGINS.contains(&origin.as_str()) {
            return Err(Refusal::new(format!("it lists an origin this build does not know ({origin})")));
        }
        if raw.origins[..index].contains(origin) {
            return Err(Refusal::new(format!("it lists the origin {origin} twice")));
        }
    }

    // ---- file roots (c, note 4) -----------------------------------------------------------
    if raw.file_roots.is_empty() {
        return Err(Refusal::new("it names no file root"));
    }
    let mut file_roots = Vec::new();
    for root in &raw.file_roots {
        file_roots.push(existing_dir("file root", root)?);
    }

    // ---- the operator contract, byte for byte what enable.sh saw (a) ---------------------
    let contract = &raw.contract;
    if !plain_absolute(&contract.path) {
        return Err(Refusal::new(format!(
            "the operator contract path is not a plain absolute path ({})", contract.path.display())));
    }
    let contract_ok = std::fs::symlink_metadata(&contract.path)
        .is_ok_and(|m| m.file_type().is_file() && m.len() <= MAX_CONTRACT_BYTES);
    let actual = contract_ok.then(|| std::fs::read(&contract.path).ok()).flatten().map(|bytes| {
        use sha2::Digest;
        format!("{:x}", sha2::Sha256::digest(bytes))
    });
    if actual.as_deref() != Some(contract.sha256.to_ascii_lowercase().as_str()) || contract.sha256.len() != 64 {
        return Err(Refusal::new(format!(
            "the operator contract is missing or has changed since it was switched on ({})",
            contract.path.display())));
    }

    Ok(Declaration {
        entity_root,
        engine_root,
        home,
        claim: ClaimPaths { file, lock },
        permission_mode: raw.permission_mode,
        environment: raw.environment,
        origins: raw.origins,
        file_roots,
        contract_path: contract.path.clone(),
        contract_sha256: contract.sha256.to_ascii_lowercase(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use sha2::Digest;

    /// A fixture Mac: a home, an entity, an engine, a contract, a data folder. Canonical
    /// paths, because `/var` is a link to `/private/var` on macOS and the gate canonicalizes.
    struct Fixture {
        root: PathBuf,
        data: PathBuf,
        home: PathBuf,
        entity: PathBuf,
        engine: PathBuf,
        contract: PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_dir_all(&self.root) { eprintln!("fixture cleanup: {error}"); }
        }
    }

    fn write(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn fixture() -> Fixture {
        let root = std::env::temp_dir().join(format!("operator-declaration-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let root = std::fs::canonicalize(&root).unwrap();
        let home = root.join("home");
        let entity = home.join("ab/femcboost");
        let engine = home.join("ab/richos/richos/engine");
        let data = root.join("data");
        std::fs::create_dir_all(&data).unwrap();
        write(&entity.join("orchestration.config"), "MODEL_CEILING=\"opus\"\nOPERATOR_FENCES=\"on\"\n");
        write(&entity.join("CLAUDE.md"), "# fixture\n");
        for file in REQUIRED_ENGINE_FILES {
            write(&engine.join(file), "fixture\n");
        }
        let contract = home.join("ab/operator-contract.md");
        write(&contract, "The operator contract.\n");
        Fixture { root, data, home, entity, engine, contract }
    }

    fn digest(path: &Path) -> String {
        format!("{:x}", sha2::Sha256::digest(std::fs::read(path).unwrap()))
    }

    impl Fixture {
        fn valid(&self) -> Value {
            json!({
                "schema": 1,
                "entity_root": self.entity,
                "engine_root": self.engine,
                "home": self.home,
                "claim": {"file": self.home.join(".claude/state/operator-lead.json"),
                          "lock": self.home.join(".claude/state/operator-lead.lock")},
                "permission_mode": "bypassPermissions",
                "environment": {"PATH": "/usr/bin:/bin", "LANG": "en_US.UTF-8", "USER": "fixture",
                                "GIT_EDITOR": "vi", "JAVA_HOME": "/fixture/java"},
                "origins": ["desk-typed", "desk-voice", "desk-file"],
                "file_roots": [self.home.join("ab")],
                "contract": {"path": self.contract, "sha256": digest(&self.contract)},
            })
        }

        fn declare(&self, value: &Value) -> Gate {
            write(&self.data.join(DECLARATION_FILE), &value.to_string());
            gate_with(&self.data, &FencesAnswer(Ok(())))
        }

        fn listing(&self) -> Vec<PathBuf> {
            let mut all = Vec::new();
            let mut stack = vec![self.root.clone()];
            while let Some(dir) = stack.pop() {
                for entry in std::fs::read_dir(&dir).unwrap() {
                    let path = entry.unwrap().path();
                    if path.is_dir() {
                        stack.push(path.clone());
                    }
                    all.push(path);
                }
            }
            all.sort();
            all
        }
    }

    fn refused(gate: Gate) -> Refusal {
        match gate {
            Gate::Refused(refusal) => refusal,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    fn refused_with(fixture: &Fixture, value: Value, needle: &str) {
        let refusal = refused(fixture.declare(&value));
        assert!(refusal.what.contains(needle), "refusal {:?} does not name {needle:?}", refusal.what);
        assert!(refusal.sentence().starts_with("Your team is switched off on this Mac because "));
        assert!(refusal.sentence().ends_with(" Rich can fix it."));
    }

    // ---- N1: absence is the product, and the gate writes nothing ------------------------

    #[test]
    fn without_the_file_the_install_gets_the_product_path_and_nothing_is_written() {
        let f = fixture();
        let before = f.listing();
        assert!(matches!(gate(&f.data), Gate::Product));
        assert_eq!(f.listing(), before, "reading the gate must never create or change a file");
    }

    #[test]
    fn a_valid_declaration_turns_the_operator_on_with_canonical_paths() {
        let f = fixture();
        let Gate::Operator(declaration) = f.declare(&f.valid()) else { panic!("expected operator mode") };
        assert_eq!(declaration.entity_root, f.entity);
        assert_eq!(declaration.engine_root, f.engine);
        assert_eq!(declaration.home, f.home);
        assert_eq!(declaration.permission_mode, "bypassPermissions");
        assert_eq!(declaration.environment["PATH"], "/usr/bin:/bin");
        assert_eq!(declaration.origins, ["desk-typed", "desk-voice", "desk-file"]);
        assert_eq!(declaration.claim.file, f.home.join(".claude/state/operator-lead.json"));
    }

    // ---- B8: present means present --------------------------------------------------------

    #[test]
    fn a_present_file_that_is_not_json_refuses_work_and_never_falls_back() {
        let f = fixture();
        write(&f.data.join(DECLARATION_FILE), "{ this is not json");
        assert!(refused(gate(&f.data)).what.contains("could not be read as a declaration"));
        refused_with(&f, Value::Null, "could not be read as a declaration");
    }

    #[test]
    fn an_empty_file_is_present_and_refuses() {
        let f = fixture();
        write(&f.data.join(DECLARATION_FILE), "");
        refused(gate(&f.data));
    }

    #[test]
    fn a_directory_where_the_file_should_be_is_present_and_refuses() {
        let f = fixture();
        std::fs::create_dir_all(f.data.join(DECLARATION_FILE)).unwrap();
        assert!(refused(gate(&f.data)).what.contains("is not a plain file"));
    }

    #[cfg(unix)]
    #[test]
    fn a_link_where_the_file_should_be_refuses_even_when_its_target_is_valid() {
        let f = fixture();
        let elsewhere = f.root.join("elsewhere.json");
        write(&elsewhere, &f.valid().to_string());
        std::os::unix::fs::symlink(&elsewhere, f.data.join(DECLARATION_FILE)).unwrap();
        assert!(refused(gate(&f.data)).what.contains("is not a plain file"));
    }

    #[test]
    fn an_oversized_file_refuses() {
        let f = fixture();
        write(&f.data.join(DECLARATION_FILE), &" ".repeat(70 * 1024));
        assert!(refused(gate(&f.data)).what.contains("too large"));
    }

    #[test]
    fn an_unknown_schema_or_an_unknown_field_refuses() {
        let f = fixture();
        let mut v = f.valid();
        v["schema"] = json!(2);
        refused_with(&f, v, "version this build cannot read");
        let mut v = f.valid();
        v["premission_mode"] = json!("default");
        refused_with(&f, v, "could not be read as a declaration");
    }

    // ---- (f)'s list of what makes a declaration invalid ------------------------------------

    #[test]
    fn an_entity_without_orchestration_config_or_claude_md_refuses() {
        let f = fixture();
        std::fs::remove_file(f.entity.join("orchestration.config")).unwrap();
        refused_with(&f, f.valid(), "orchestration.config");
        write(&f.entity.join("orchestration.config"), "x\n");
        std::fs::remove_file(f.entity.join("CLAUDE.md")).unwrap();
        refused_with(&f, f.valid(), "CLAUDE.md");
    }

    #[test]
    fn an_engine_root_that_does_not_resolve_refuses() {
        let f = fixture();
        let mut v = f.valid();
        v["engine_root"] = json!(f.home.join("nowhere/engine"));
        refused_with(&f, v, "engine");
    }

    #[test]
    fn an_engine_missing_a_script_the_app_runs_refuses_and_names_it() {
        let f = fixture();
        std::fs::remove_file(f.engine.join("scripts/stop.sh")).unwrap();
        refused_with(&f, f.valid(), "scripts/stop.sh");
    }

    #[test]
    fn the_plugin_cache_is_never_his_engine() {
        let f = fixture();
        let cache = f.home.join(".claude/plugins/cache/richos-local/richos-engine/1.0.0");
        for file in REQUIRED_ENGINE_FILES {
            write(&cache.join(file), "fixture\n");
        }
        let mut v = f.valid();
        v["engine_root"] = json!(cache);
        refused_with(&f, v, "plugin cache");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_into_the_plugin_cache_is_still_the_plugin_cache() {
        let f = fixture();
        let cache = f.home.join(".claude/plugins/cache/richos-local/richos-engine/1.0.0");
        for file in REQUIRED_ENGINE_FILES {
            write(&cache.join(file), "fixture\n");
        }
        let link = f.home.join("ab/engine-link");
        std::os::unix::fs::symlink(&cache, &link).unwrap();
        let mut v = f.valid();
        v["engine_root"] = json!(link);
        refused_with(&f, v, "plugin cache");
    }

    #[test]
    fn a_contract_whose_digest_does_not_match_refuses() {
        let f = fixture();
        let mut v = f.valid();
        v["contract"]["sha256"] = json!("0".repeat(64));
        refused_with(&f, v, "operator contract");
        let f = fixture();
        let v = f.valid();
        write(&f.contract, "The operator contract, edited after enable.sh ran.\n");
        refused_with(&f, v, "operator contract");
    }

    #[test]
    fn claim_paths_that_disagree_refuse() {
        let f = fixture();
        let mut v = f.valid();
        v["claim"]["lock"] = json!(f.home.join(".claude/other/operator-lead.lock"));
        refused_with(&f, v, "claim");
        let mut v = f.valid();
        v["claim"]["file"] = json!(f.root.join("elsewhere/operator-lead.json"));
        v["claim"]["lock"] = json!(f.root.join("elsewhere/operator-lead.lock"));
        refused_with(&f, v, "claim");
        let mut v = f.valid();
        v["claim"]["file"] = json!(f.home.join(".claude/state/lead.json"));
        refused_with(&f, v, "claim");
    }

    #[test]
    fn a_relative_or_dotted_path_refuses() {
        let f = fixture();
        let mut v = f.valid();
        v["home"] = json!("home");
        refused_with(&f, v, "absolute");
        let mut v = f.valid();
        v["entity_root"] = json!(format!("{}/../femcboost", f.entity.display()));
        refused_with(&f, v, "absolute");
    }

    #[test]
    fn no_credential_name_is_ever_accepted_into_the_stored_environment() {
        let f = fixture();
        for name in ["ANTHROPIC_API_KEY", "ANTHROPIC_BASE_URL", "CLAUDE_CODE_MESSAGING_TOKEN",
                     "CLAUDECODE", "CLAUDE_CONFIG_DIR", "GITHUB_TOKEN", "AWS_SECRET_ACCESS_KEY",
                     "npm_config_password", "SSH_KEY_PATH"] {
            let mut v = f.valid();
            v["environment"][name] = json!("x");
            refused_with(&f, v, name);
        }
    }

    #[test]
    fn a_re_derived_or_app_owned_name_is_never_stored() {
        let f = fixture();
        for name in ["HOME", "SSH_AUTH_SOCK", "TMPDIR", "RICHOS_OPERATOR_LEAD", "RICHOS_SESSION_PID"] {
            let mut v = f.valid();
            v["environment"][name] = json!("/x");
            refused_with(&f, v, name);
        }
    }

    #[test]
    fn an_environment_without_path_or_with_a_malformed_name_refuses() {
        let f = fixture();
        let mut v = f.valid();
        v["environment"].as_object_mut().unwrap().remove("PATH");
        refused_with(&f, v, "PATH");
        let mut v = f.valid();
        v["environment"]["A=B"] = json!("x");
        refused_with(&f, v, "A=B");
    }

    #[test]
    fn origins_and_permission_modes_are_from_the_known_sets() {
        let f = fixture();
        let mut v = f.valid();
        v["origins"] = json!([]);
        refused_with(&f, v, "origin");
        let mut v = f.valid();
        v["origins"] = json!(["desk-typed", "telegram"]);
        refused_with(&f, v, "telegram");
        let mut v = f.valid();
        v["permission_mode"] = json!("yolo");
        refused_with(&f, v, "yolo");
        let mut v = f.valid();
        v["origins"] = json!(["desk-typed", "phone"]);
        assert!(matches!(f.declare(&v), Gate::Operator(_)), "phone is his call, and valid");
    }

    #[test]
    fn file_roots_must_exist() {
        let f = fixture();
        let mut v = f.valid();
        v["file_roots"] = json!([]);
        refused_with(&f, v, "file root");
        let mut v = f.valid();
        v["file_roots"] = json!([f.home.join("missing")]);
        refused_with(&f, v, "file root");
    }

    // ---- r3 §11 item 2: the fences are part of a valid declaration ------------------------

    /// A fence status that answers whatever it was built with.
    struct FencesAnswer(Result<(), String>);
    impl FenceStatus for FencesAnswer {
        fn status(&self, _: &Declaration) -> Result<(), String> {
            self.0.clone()
        }
    }

    #[test]
    fn the_fence_switch_off_or_absent_refuses_and_names_it() {
        let f = fixture();
        write(&f.entity.join("orchestration.config"), "MODEL_CEILING=\"opus\"\nOPERATOR_FENCES=\"off\"\n");
        refused_with(&f, f.valid(), "OPERATOR_FENCES");
        write(&f.entity.join("orchestration.config"), "MODEL_CEILING=\"opus\"\n");
        refused_with(&f, f.valid(), "OPERATOR_FENCES");
        // The last assignment wins, as it does for the shell that sources the file.
        write(&f.entity.join("orchestration.config"), "OPERATOR_FENCES=\"off\"\nOPERATOR_FENCES=on # later\n");
        assert!(matches!(f.declare(&f.valid()), Gate::Operator(_)));
    }

    #[test]
    fn a_fence_status_that_fails_refuses_and_says_why() {
        let f = fixture();
        write(&f.data.join(DECLARATION_FILE), &f.valid().to_string());
        let refusal = refused(gate_with(&f.data, &FencesAnswer(Err("richos is not fenced".into()))));
        assert!(refusal.what.contains("land fences are not in place"), "{}", refusal.what);
        assert!(refusal.what.contains("richos is not fenced"), "{}", refusal.what);
    }

    #[test]
    fn with_no_file_the_fences_are_never_asked() {
        let f = fixture();
        struct Panics;
        impl FenceStatus for Panics {
            fn status(&self, _: &Declaration) -> Result<(), String> {
                panic!("the product path must not run an engine script")
            }
        }
        assert!(matches!(gate_with(&f.data, &Panics), Gate::Product));
    }

    #[test]
    fn the_real_gate_refuses_while_the_engine_has_no_fence_status_command() {
        let f = fixture();
        write(&f.data.join(DECLARATION_FILE), &f.valid().to_string());
        let refusal = refused(gate(&f.data));
        assert!(refusal.what.contains("operator-fences.sh is missing"), "{}", refusal.what);
    }

    fn declaration(f: &Fixture) -> Declaration {
        match f.declare(&f.valid()) {
            Gate::Operator(declaration) => *declaration,
            other => panic!("expected operator mode, got {other:?}"),
        }
    }

    #[test]
    fn the_real_fence_runner_reads_the_exit_code_of_status() {
        let f = fixture();
        let d = declaration(&f);
        let script = f.engine.join(FENCE_STATUS_SCRIPT);
        write(&script, "[ \"$1\" = status ] || exit 9\nexit 0\n");
        assert_eq!(EngineFenceStatus.status(&d), Ok(()));
        write(&script, "exit 3\n");
        let why = EngineFenceStatus.status(&d).unwrap_err();
        assert!(why.contains("status answered"), "{why}");
        std::fs::remove_file(&script).unwrap();
        assert!(EngineFenceStatus.status(&d).unwrap_err().contains("missing"));
    }

    #[test]
    fn the_fence_runner_runs_with_his_environment_built_from_empty() {
        let f = fixture();
        let d = declaration(&f);
        let out = f.root.join("fence-env.txt");
        write(&f.engine.join(FENCE_STATUS_SCRIPT),
              &format!("cd / && /usr/bin/env > '{}'\n", out.display()));
        EngineFenceStatus.status(&d).unwrap();
        let text = std::fs::read_to_string(&out).unwrap();
        let mut names: Vec<&str> = text.lines().filter_map(|l| l.split_once('=').map(|(n, _)| n))
            // What bash adds for itself, whatever it was started with.
            .filter(|n| !["PWD", "OLDPWD", "SHLVL", "_"].contains(n)).collect();
        names.sort();
        assert_eq!(names, ["GIT_EDITOR", "HOME", "JAVA_HOME", "LANG", "PATH", "USER"],
            "exactly his stored names plus HOME, and nothing of this test process: {text}");
        assert!(text.contains(&format!("HOME={}\n", f.home.display())));
    }

    #[test]
    fn the_denied_name_rule_is_exactly_the_spec_s_rule() {
        for denied in ["ANTHROPIC_API_KEY", "anthropic_x", "CLAUDE_X", "CLAUDECODE", "MY_TOKEN",
                       "apikey", "A_SECRET", "PASSWORD", "db_password"] {
            assert!(is_denied_name(denied), "{denied} must be denied");
        }
        for allowed in ["PATH", "LANG", "USER", "LOGNAME", "SHELL", "JAVA_HOME", "ANDROID_HOME",
                        "ANDROID_SDK_ROOT", "HOMEBREW_PREFIX", "HOMEBREW_CELLAR", "HOMEBREW_REPOSITORY",
                        "INFOPATH", "GIT_EDITOR", "COREPACK_ENABLE_AUTO_PIN"] {
            assert!(!is_denied_name(allowed), "{allowed} is in his terminal's list (spec §2) and must pass");
        }
    }
}

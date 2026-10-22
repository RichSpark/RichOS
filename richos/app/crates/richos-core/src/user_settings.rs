//! **The user's own settings files** (CEO ruling §138; richos-hq build plan
//! `docs/plans/2026-10-21-everything-for-every-user.md`, item 0 and section 1, as changed by
//! the CEO on 2026-10-21: a technical user tweaks the app's equivalent of his `CLAUDE.md`,
//! Rich's and Stu's own files included, by hand).
//!
//! Plain files in the user's central folder (`~/myrichos` unless moved, `company.rs`), read the
//! same way for every user, so the only thing that differs between one user's app and
//! another's is what those files say:
//!
//! - [`RICH`] and [`STU`]: Rich's and Stu's instructions. On first run the app copies its
//!   shipped `rich.md` and `stu.md` here ([`provision`]); from then on every session of Rich
//!   and Stu starts from these copies ([`standing_plugin`]), never from the shipped ones. The
//!   user's files ARE what runs. When a new version ships improved ones, a copy the user never
//!   changed is updated, and a changed one only when the user says OK
//!   (`instructions_update.rs`).
//! - [`INSTRUCTIONS`]: the user's own standing instructions, created empty on first run.
//! - [`ORCHESTRATION_CONFIG`]: the user's values for the engine's checks. The coordination
//!   folder's generated config is that file plus the app's own keys ([`orchestration_config`]).
//! - [`SETTINGS`]: the user's own environment additions and hooks, in Claude Code's settings
//!   format ([`LeaseSettings`]).
//!
//! The user's teammates, the rest of what a terminal keeps in `.claude/agents`, already load
//! from `<app data>/team/` as written (`engine_profile::active_team`).
use crate::runtime::RuntimeError;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const RICH: &str = "rich.md";
pub const STU: &str = "stu.md";
pub const INSTRUCTIONS: &str = "instructions.md";
pub const ORCHESTRATION_CONFIG: &str = "orchestration.config";
pub const SETTINGS: &str = "settings.json";

/// Every file of the user's that a session reads, in [`digest`] order.
pub const FILES: [&str; 5] = [RICH, STU, INSTRUCTIONS, ORCHESTRATION_CONFIG, SETTINGS];

/// The keys the app writes into the generated config itself. A user's line for one is left
/// out, so every reader (first match or last match) sees one value: the app's.
pub const APP_KEYS: [&str; 1] = ["SESSION_TEAMS_DIR"];

/// The keys whose value, when the user's file does not set one, is the engine's own: its line
/// in the shipped engine's `orchestration.config`, as written there (build plan item 7). So a
/// new user gets the engine's full model list and order and its cost ceiling (`opus`), and a
/// model added there reaches every user's app with the engine, with nothing to change here.
pub const ENGINE_DEFAULTS: [&str; 3] = ["ALLOWED_MODELS", "MODEL_TIERS", "MODEL_CEILING"];

/// Create `path` with `body` only when nothing is at that name; returns whether it did. An
/// existing file, or a link of any kind, is the user's and is never written.
fn create_once(path: &Path, body: &[u8]) -> Result<bool, RuntimeError> {
    use std::io::Write;
    if std::fs::symlink_metadata(path).is_ok() { return Ok(false); }
    let mut file = match std::fs::OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return Ok(false),
        Err(e) => return Err(RuntimeError(format!("{} could not be created: {e}", path.display()))),
    };
    file.write_all(body).and_then(|_| file.sync_all())
        .map_err(|e| RuntimeError(format!("{} could not be written: {e}", path.display())))?;
    Ok(true)
}

/// **First run: copy the shipped `rich.md` and `stu.md`, and an empty `instructions.md`, into
/// the user's folder**, each only when the user has none, and record what each copy was made
/// from. `shipped` is the shipped plugin root (`<engine>/app`). Returns the names it created.
/// Then a copy the user never changed is brought to a newer shipped version
/// (`instructions_update::check`); a changed one is never written here.
pub fn provision(folder: &Path, shipped: &Path) -> Result<Vec<&'static str>, RuntimeError> {
    std::fs::create_dir_all(folder).map_err(|e| RuntimeError(format!("{} could not be created: {e}", folder.display())))?;
    let mut created = Vec::new();
    for name in [RICH, STU] {
        if std::fs::symlink_metadata(folder.join(name)).is_ok() { continue; }
        let body = std::fs::read(shipped.join("agents").join(name))
            .map_err(|e| RuntimeError(format!("the shipped {name} could not be read: {e}")))?;
        if create_once(&folder.join(name), &body)? {
            crate::instructions_update::record_copy(folder, name, &String::from_utf8_lossy(&body))?;
            created.push(name);
        }
    }
    if create_once(&folder.join(INSTRUCTIONS), b"")? { created.push(INSTRUCTIONS); }
    // A failed check never stops a session: the user's files are used as they are.
    if let Err(e) = crate::instructions_update::check(folder, shipped) {
        eprintln!("[richos] the newer shipped instructions could not be checked: {}", e.0);
    }
    Ok(created)
}

/// **The plugin Rich and Stu start from**: the shipped plugin's manifest and skills, with
/// `agents/rich.md` and `agents/stu.md` taken from the user's own copies, written to `root` for
/// one session. Read at every session start, so an edit is in the next session. A copy the
/// user removed falls back to the shipped file for that session.
pub fn standing_plugin(folder: &Path, shipped: &Path, root: &Path) -> Result<PathBuf, RuntimeError> {
    let copy = |from: &Path, to: &str| -> Result<(), RuntimeError> {
        let body = std::fs::read(from).map_err(|e| RuntimeError(format!("{} could not be read: {e}", from.display())))?;
        let target = root.join(to);
        if let Some(parent) = target.parent() { std::fs::create_dir_all(parent).map_err(|e| RuntimeError(e.to_string()))?; }
        std::fs::write(&target, body).map_err(|e| RuntimeError(format!("{} could not be written: {e}", target.display())))
    };
    copy(&crate::skills::manifest_path(shipped), ".claude-plugin/plugin.json")?;
    for name in crate::skills::SKILLS {
        copy(&crate::skills::skill_path(shipped, name), &format!("skills/{name}/SKILL.md"))?;
    }
    // The user's own standing instructions, in full and as written, at the end of both
    // definitions: the definition's body is that agent's standing instruction, so a line the
    // user writes in instructions.md is followed by Rich and by Stu from their next session.
    let own = instructions(folder)?.filter(|text| !text.trim().is_empty())
        .map(|text| format!("\n\n## The user's own instructions\n\nThe user wrote these in {}. They are the user's standing instructions to you; follow them.\n\n{text}",
            folder.join(INSTRUCTIONS).display()));
    for name in [RICH, STU] {
        let mine = folder.join(name);
        let from = if mine.is_file() { mine } else { shipped.join("agents").join(name) };
        copy(&from, &format!("agents/{name}"))?;
        with_memory(&root.join("agents").join(name))?;
        if let Some(own) = &own {
            use std::io::Write;
            std::fs::OpenOptions::new().append(true).open(root.join("agents").join(name))
                .and_then(|mut file| file.write_all(own.as_bytes()))
                .map_err(|e| RuntimeError(format!("the user's instructions could not be added to {name}: {e}")))?;
        }
    }
    Ok(root.to_path_buf())
}

/// **Rich's and Stu's auto-memory** (build plan item 10). Each runs as a plugin agent
/// (`--agent`), and for an agent Claude Code keeps memory by its definition's `memory:` scope,
/// not by `autoMemoryDirectory` (measured 2026-10-21 on 2.1.305 with a lease-shaped session:
/// with `autoMemoryDirectory` alone the folder stayed empty; with `memory: project` a memory was
/// written to `<working folder>/.claude/agent-memory/<agent>/` and a new session recalled it).
/// A lease's working folder is the coordination folder in the app's data, so `project` is one
/// folder whichever Claude account the lease runs on. A definition that names its own scope
/// keeps it; one with none gets `project`.
pub const MEMORY_SCOPE: &str = "memory: project";

fn with_memory(definition: &Path) -> Result<(), RuntimeError> {
    let text = std::fs::read_to_string(definition).map_err(|e| RuntimeError(e.to_string()))?;
    let Some(rest) = text.strip_prefix("---\n") else { return Ok(()) };
    let Some(end) = rest.find("\n---") else { return Ok(()) };
    if rest[..end].lines().any(|line| line.trim_start().starts_with("memory:")) { return Ok(()) }
    std::fs::write(definition, format!("---\n{MEMORY_SCOPE}\n{rest}")).map_err(|e| RuntimeError(e.to_string()))
}

/// A file's text, or `None` when it is not there.
fn read_optional(path: &Path) -> Result<Option<String>, RuntimeError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(RuntimeError(format!("{} could not be read: {e}", path.display()))),
    }
}

/// **Names the app sets for itself on every lease**, which a user's environment never
/// replaces: its own (`RICHOS_`, `LORO_`, `ECS_`), the Git names it sets, and which Claude
/// account and settings a lease runs on (`CLAUDE`, `ANTHROPIC_`: the account in use is the
/// app's choice, fill-first). `PATH` is composed, never replaced.
pub fn app_owned(name: &str) -> bool {
    name == "PATH" || ["RICHOS_", "LORO_", "ECS_", "GIT_", "CLAUDE", "ANTHROPIC_"].iter().any(|prefix| name.starts_with(prefix))
}

/// The user's standing instructions as written, or `None` when there are none.
pub fn instructions(folder: &Path) -> Result<Option<String>, RuntimeError> {
    read_optional(&folder.join(INSTRUCTIONS))
}

/// The key a config line assigns (`KEY=…` or `export KEY=…`), or `None` for anything else.
fn assigned_key(line: &str) -> Option<&str> {
    let line = line.trim_start();
    let line = line.strip_prefix("export ").map(str::trim_start).unwrap_or(line);
    let (key, _) = line.split_once('=')?;
    let mut bytes = key.bytes();
    (bytes.next().is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')).then_some(key)
}

/// **The coordination folder's `orchestration.config`**: the engine's line for each
/// [`ENGINE_DEFAULTS`] key the user's file does not set (`engine` is the shipped engine's
/// `orchestration.config`), the user's file as written (minus any [`APP_KEYS`] line), then the
/// app's own keys.
pub fn orchestration_config(folder: Option<&Path>, engine: &str, teams: &str) -> Result<String, RuntimeError> {
    let user = match folder { Some(folder) => read_optional(&folder.join(ORCHESTRATION_CONFIG))?, None => None };
    let user_lines: Vec<&str> = user.as_deref().map(|text| text.lines()
        .filter(|line| assigned_key(line).is_none_or(|key| !APP_KEYS.contains(&key))).collect()).unwrap_or_default();
    let set: Vec<&str> = user_lines.iter().filter_map(|line| assigned_key(line)).collect();
    let mut out = String::from("# Generated desktop coordination, not target-repository configuration.\n");
    for key in ENGINE_DEFAULTS {
        if set.contains(&key) { continue; }
        let line = engine.lines().find(|line| !line.starts_with(char::is_whitespace) && assigned_key(line) == Some(key))
            .ok_or_else(|| RuntimeError(format!("the engine's orchestration.config sets no {key}")))?;
        out.push_str(line);
        out.push('\n');
    }
    if let (Some(folder), true) = (folder, user.is_some()) {
        out.push_str(&format!("# The user's own values, from {}\n", folder.join(ORCHESTRATION_CONFIG).display()));
        for line in user_lines {
            // `export KEY=…` is written `KEY=…`: the engine's readers match an assignment at the
            // start of a line (`app.py` `definition_model`, `teammate-name.sh`), and a sourced
            // file means the same either way.
            let trimmed = line.trim_start();
            let line = match trimmed.strip_prefix("export ") { Some(rest) if assigned_key(line).is_some() => rest.trim_start(), _ => line };
            out.push_str(line);
            out.push('\n');
        }
    }
    out.push_str(&format!("SESSION_TEAMS_DIR={teams}\n"));
    Ok(out)
}

/// **What the user's `settings.json` adds to a lease**: its `env` (strings only) and its
/// `hooks`, as Claude Code's own settings file would. Every other key is Claude Code's or the
/// app's to set and is not read. A file that is not a JSON object is reported and adds
/// nothing, as Claude Code itself skips a settings file it cannot parse.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LeaseSettings {
    pub env: BTreeMap<String, String>,
    pub hooks: Option<serde_json::Value>,
}

pub fn lease_settings(folder: &Path) -> Result<LeaseSettings, RuntimeError> {
    let path = folder.join(SETTINGS);
    let Some(text) = read_optional(&path)? else { return Ok(LeaseSettings::default()) };
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value @ serde_json::Value::Object(_)) => value,
        Ok(_) => { eprintln!("[richos] {} is not a JSON object; nothing in it was applied", path.display()); return Ok(LeaseSettings::default()) }
        Err(e) => { eprintln!("[richos] {} is not valid JSON ({e}); nothing in it was applied", path.display()); return Ok(LeaseSettings::default()) }
    };
    let env = value.get("env").and_then(|env| env.as_object()).map(|env| env.iter()
        .filter_map(|(key, value)| value.as_str().map(|value| (key.clone(), value.to_string()))).collect()).unwrap_or_default();
    let hooks = value.get("hooks").filter(|hooks| hooks.is_object()).cloned();
    Ok(LeaseSettings { env, hooks })
}

/// **The digest of the user's files** ([`FILES`]): it moves exactly when one of them is
/// created, changed or removed, so a session started under older ones is renewed at its next
/// boundary, never mid-turn.
pub fn digest(folder: &Path) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    for name in FILES {
        let body = std::fs::read(folder.join(name)).ok();
        hasher.update((name.len() as u64).to_le_bytes());
        hasher.update(name.as_bytes());
        match body {
            Some(body) => { hasher.update([1]); hasher.update((body.len() as u64).to_le_bytes()); hasher.update(&body); }
            None => hasher.update([0]),
        }
    }
    format!("{:x}", hasher.finalize())
}

/// **What a lease takes from the user's own files**, read once when its profile is prepared.
/// `Default` is a lease with no user folder: tests, and an install with no home directory.
#[derive(Clone, Debug, Default)]
pub struct UserSetup {
    /// The user's central folder. `None` reads no user file at all.
    pub folder: Option<PathBuf>,
    /// [`digest`] of the files as this lease read them.
    pub digest: Option<String>,
    /// The user's `settings.json`, as this lease read it.
    pub settings: LeaseSettings,
    /// This lease's [`standing_plugin`], or `None` to start from the shipped plugin.
    pub standing: Option<PathBuf>,
    /// The user's login shell environment (`login_environment.rs`), or `None` for none.
    pub login: Option<std::sync::Arc<BTreeMap<String, String>>>,
    /// The Git the user's terminal runs (`user_git.rs`), found on the login `PATH`.
    pub git: crate::user_git::UserGit,
}

impl UserSetup {
    /// Provision the user's folder, then read it into one lease: `shipped` is the shipped
    /// plugin root and `standing` where this lease's [`standing_plugin`] is written.
    pub fn prepare(folder: Option<PathBuf>, shipped: &Path, standing: &Path) -> Result<Self, RuntimeError> {
        let Some(folder) = folder else { return Ok(Self::default()) };
        provision(&folder, shipped)?;
        let standing = standing_plugin(&folder, shipped, standing)?;
        Ok(Self { digest: Some(digest(&folder)), settings: lease_settings(&folder)?, standing: Some(standing), folder: Some(folder), login: None, git: Default::default() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_config_line_names_its_key_and_nothing_else_does() {
        assert_eq!(assigned_key("MODEL_CEILING=\"sonnet\""), Some("MODEL_CEILING"));
        assert_eq!(assigned_key("  export DIALECT_TARGET=en-US"), Some("DIALECT_TARGET"));
        assert_eq!(assigned_key("# ALLOWED_MODELS=\"x\""), None);
        assert_eq!(assigned_key("1BAD=x"), None);
        assert_eq!(assigned_key("plain text"), None);
    }
}

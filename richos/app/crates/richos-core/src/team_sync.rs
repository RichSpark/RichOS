//! **The governed repositories' teammates reach the app's team** (CEO 2026-11-02, "where and why
//! only there?": a change to a teammate's instructions reaches every place that teammate runs).
//!
//! In the terminal a teammate is `<governed repository>/.claude/agents/<name>.md`, read live. In
//! the app it is `<app data>/team/<name>.md` (`engine_profile::USER_TEAM`), and until this module
//! nothing filled that folder: his 38 files were copied once by hand on 2026-10-28
//! (richos-hq `docs/plans/2026-10-21-switching-to-the-app.md` §3, "(a) once copied"), so a rule
//! the terminal's copy lost on 2026-11-02 (`proceed at SHA`, `ray.md` and `kai.md`) still ran in
//! the app.
//!
//! **What this does, at every lease start** (`engine_profile::render_team`): for each repository
//! the user's companies own or have connected that is *governed* (the engine's own test:
//! `orchestration.config` at its root, engine `README.md` "Adopt, per repository") and carries
//! `.claude/agents/`, each definition is written to `team/<name>.md` with the two path rewrites
//! the plan's §3 made by hand (the terminal's install path to `$RICHOS_ENGINE_ROOT`, which the
//! app sets on every lease).
//!
//! **Rules, so nothing a user did is lost:**
//! - A name in no governed repository is the user's own (hired in the app) and is never touched.
//! - `dean` and `pierce` stay the app's stock ones (plan §3: the repository's Dean writes where
//!   the app never looks).
//! - A lease for a company takes that company's own governed repositories' definitions first
//!   (`sync(data, Some(company))`), so a change there always reaches it; a name that company does
//!   not define comes from the first other repository (company order in `entities.json`) holding
//!   it. With no company (the lease not yet scoped) it is company order alone.
//! - `team-sync.json` beside the team folder records the digest of what this module last wrote.
//!   An app copy still carrying that digest is replaced by a newer repository definition. An app
//!   copy that differs from it was edited in the app and is KEPT (reported, not overwritten).
//! - An app copy with no record (the hand copies of 2026-10-28, or a same-named teammate hired in
//!   the app) cannot be told from a stale one, so the repository wins and the previous text is
//!   kept in `team-replaced/<name>.<digest>.md`.
//! - A definition removed from the repository is left in the app: nothing is ever deleted.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Names the app keeps stock whatever a repository says (plan §3).
pub const KEEP_STOCK: [&str; 2] = ["dean", "pierce"];
/// A definition larger than this is not copied (`mega-lander/app.py` `DEFINITION_LIMIT`).
const LIMIT: usize = 64 * 1024;
const MANIFEST: &str = "team-sync.json";
const REPLACED: &str = "team-replaced";

/// The terminal's engine path becomes the app's, exactly the two forms the definitions use.
pub fn for_the_app(text: &str) -> String {
    text.replace("\"$HOME/.claude/richos-engine/", "\"$RICHOS_ENGINE_ROOT/")
        .replace("~/.claude/richos-engine/scripts/escalate.sh", "\"$RICHOS_ENGINE_ROOT/scripts/escalate.sh\"")
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TeamSyncReport {
    pub added: Vec<String>,
    pub updated: Vec<String>,
    /// Replaced although unrecorded; the previous text is in `team-replaced/`.
    pub replaced_unrecorded: Vec<String>,
    /// Edited in the app since the last sync: kept as the user wrote it.
    pub kept_edited: Vec<String>,
    pub skipped: Vec<String>,
}

impl TeamSyncReport {
    pub fn changed(&self) -> bool { !(self.added.is_empty() && self.updated.is_empty() && self.replaced_unrecorded.is_empty()) }
    pub fn summary(&self) -> String {
        format!("team sync: added {:?}, updated {:?}, replaced without a record {:?}, kept (edited in the app) {:?}, skipped {:?}",
            self.added, self.updated, self.replaced_unrecorded, self.kept_edited, self.skipped)
    }
}

fn digest(text: &str) -> String { format!("{:x}", Sha256::digest(text.as_bytes())) }

fn valid_name(name: &str) -> bool {
    let b = name.as_bytes();
    (2..=16).contains(&b.len()) && b[0].is_ascii_lowercase() && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && !crate::engine_profile::DUTIES.contains(&name) && !KEEP_STOCK.contains(&name)
}

/// The same bar `richos_work.team save` holds a definition to: front matter naming the file.
fn is_definition(name: &str, text: &str) -> bool {
    let Some(rest) = text.strip_prefix("---\n") else { return false };
    let Some(end) = rest.find("\n---") else { return false };
    let front = &rest[..end];
    front.lines().any(|l| l.trim() == format!("name: {name}"))
        && front.lines().any(|l| l.strip_prefix("description:").is_some_and(|v| !v.trim().is_empty()))
}

/// Governed repositories, each once: the named company's first, then the rest in company order.
fn governed_repositories(data: &Path, company: Option<&str>) -> Vec<PathBuf> {
    let registry = crate::entity::EntityRegistry::load(&data.join("entities.json")).registry;
    let mut seen = Vec::new();
    let (own, others): (Vec<_>, Vec<_>) = registry.entities().iter().partition(|e| company == Some(e.id.as_str()));
    for entity in own.into_iter().chain(others) {
        for root in entity.connected_repositories.iter().chain(entity.roots.iter()) {
            let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.clone());
            if !seen.contains(&root) && root.join("orchestration.config").is_file()
                && root.join(".claude/agents").is_dir() && !root.join(".claude/agents").is_symlink() {
                seen.push(root);
            }
        }
    }
    seen
}

fn write_atomic(path: &Path, body: &str) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let temporary = path.with_file_name(format!(".{}.incoming", uuid::Uuid::new_v4().simple()));
    let result = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).custom_flags(libc::O_NOFOLLOW)
        .open(&temporary).and_then(|mut f| { f.write_all(body.as_bytes())?; f.sync_all() })
        .and_then(|_| std::fs::rename(&temporary, path));
    if result.is_err() { std::fs::remove_file(&temporary).ok(); }
    result.map_err(|e| format!("{} could not be written: {e}", path.display()))
}

/// Bring `<data>/team/` up to the governed repositories' current definitions (module doc).
pub fn sync(data: &Path, company: Option<&str>) -> Result<TeamSyncReport, String> {
    let repos = governed_repositories(data, company);
    let mut report = TeamSyncReport::default();
    if repos.is_empty() { return Ok(report); }
    let team = data.join(crate::engine_profile::USER_TEAM);
    if team.is_symlink() { return Err("the team folder cannot be redirected".into()); }
    let manifest_path = data.join(MANIFEST);
    let mut recorded: BTreeMap<String, String> = std::fs::read_to_string(&manifest_path).ok()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|v| v["synced"].as_object().map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect()))
        .unwrap_or_default();
    // The first repository in that order wins a name.
    let mut sources: BTreeMap<String, String> = BTreeMap::new();
    for repo in &repos {
        let Ok(entries) = std::fs::read_dir(repo.join(".claude/agents")) else { continue };
        let mut files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        files.sort();
        for path in files {
            let Some(name) = path.file_stem().and_then(|s| s.to_str()).map(str::to_string) else { continue };
            if path.extension().is_none_or(|e| e != "md") || sources.contains_key(&name) { continue; }
            let meta = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            let text = if meta.is_file() && meta.len() as usize <= LIMIT { std::fs::read_to_string(&path).ok() } else { None };
            match text.map(|t| for_the_app(&t)) {
                Some(text) if valid_name(&name) && is_definition(&name, &text) => { sources.insert(name, text); }
                _ => if !KEEP_STOCK.contains(&name.as_str()) { report.skipped.push(name) },
            }
        }
    }
    if sources.is_empty() { return Ok(report); }
    std::fs::create_dir_all(&team).map_err(|e| e.to_string())?;
    for (name, text) in sources {
        let path = team.join(format!("{name}.md"));
        let new = digest(&text);
        let existing = match std::fs::symlink_metadata(&path) {
            Ok(m) if m.is_file() => Some(std::fs::read_to_string(&path).map_err(|e| format!("team/{name}.md could not be read: {e}"))?),
            Ok(_) => { report.skipped.push(name); continue; }
            Err(_) => None,
        };
        match existing {
            None => { write_atomic(&path, &text)?; report.added.push(name.clone()); }
            Some(old) if old == text => {}
            Some(old) => match recorded.get(&name) {
                Some(last) if *last == digest(&old) => { write_atomic(&path, &text)?; report.updated.push(name.clone()); }
                Some(_) => { report.kept_edited.push(name); continue; }
                None => {
                    let keep = data.join(REPLACED);
                    std::fs::create_dir_all(&keep).map_err(|e| e.to_string())?;
                    write_atomic(&keep.join(format!("{name}.{}.md", &digest(&old)[..12])), &old)?;
                    write_atomic(&path, &text)?;
                    report.replaced_unrecorded.push(name.clone());
                }
            },
        }
        recorded.insert(name, new);
    }
    write_atomic(&manifest_path, &json!({"version": 1, "synced": recorded}).to_string())?;
    Ok(report)
}

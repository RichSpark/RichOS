//! The ordinary engine plugin carrying Rich, Stu and on-demand skills.
use std::path::{Path, PathBuf};
use crate::doctrine::DoctrineError;
pub const PLUGIN_DIRNAME: &str = "rich-skills";
pub const PLUGIN_NAME: &str = "rich-skills";
pub const SKILLS: &[&str] = &["american-english", "bootstrap-interview", "watch-video", "audio-transcription"];
pub fn plugin_dir(config_dir: &Path) -> PathBuf { config_dir.join(PLUGIN_DIRNAME) }
pub fn manifest_path(root: &Path) -> PathBuf { root.join(".claude-plugin/plugin.json") }
pub fn skill_path(root: &Path, name: &str) -> PathBuf { root.join("skills").join(name).join("SKILL.md") }
pub fn qualified_names() -> Vec<String> { SKILLS.iter().map(|name| format!("{PLUGIN_NAME}:{name}")).collect() }
/// Copy an ordinary plugin for an isolated example or test fixture. Production loads the engine files directly.
pub fn ensure_rendered(config_dir: &Path, source: &Path) -> Result<PathBuf, DoctrineError> {
    let root = plugin_dir(config_dir);
    let mut paths = vec![".claude-plugin/plugin.json".to_string(), "agents/rich.md".into(), "agents/stu.md".into()];
    paths.extend(SKILLS.iter().map(|name|format!("skills/{name}/SKILL.md")));
    for rel in paths {
        let text = std::fs::read_to_string(source.join(&rel)).map_err(|error| DoctrineError::Unwritable { path: source.join(&rel).display().to_string(), why: error.to_string() })?;
        crate::doctrine::write_verified(&root.join(rel), &text)?;
    }
    Ok(root)
}
pub fn ensure_for_install() -> Result<PathBuf, DoctrineError> { Ok(crate::doctrine::install_config_dir()?.join("engine/app")) }
pub fn verify_present(root: &Path) -> Result<(), (PathBuf, String)> {
    let mut required = vec![manifest_path(root), root.join("agents/rich.md"), root.join("agents/stu.md")];
    required.extend(SKILLS.iter().map(|name|skill_path(root,name)));
    for path in required {
        let text=std::fs::read_to_string(&path).map_err(|error|(path.clone(),error.to_string()))?;
        if text.trim().is_empty() {return Err((path,"instruction file is empty".into()));}
    }
    Ok(())
}
/// What the first `system/init` frame said about our plugin.
///
/// Three states rather than a `bool`, because "we have not been told yet" and "we were told it
/// is not there" call for opposite responses, and collapsing them is how an absence gets
/// reported as a fact. Same reasoning as `entity.rs::RegistrySource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillsVerdict {
    /// No `system/init` frame has arrived yet. It lands with the first TURN, not the handshake,
    /// so this is the state for the whole of a lease that has not been used.
    NotYetReported,
    /// The binary listed our plugin. The skills are live.
    Loaded,
    /// The binary answered, and our plugin was not in its list. The files are on disk (preflight
    /// proved that) and the binary declined them: a schema it does not accept, a name collision,
    /// a change in a version that self-updated underneath us.
    Rejected,
}

/// Read the verdict off a `system/init` frame.
///
/// `plugins` is an array of objects carrying `name`, `path`, `source` and `version` — measured,
/// cell K2. A frame with no `plugins` key at all is `Rejected` rather than `NotYetReported`: an
/// init frame that does not mention plugins is an init frame that loaded none of ours.
pub fn verdict_from_init(init: &serde_json::Value) -> SkillsVerdict {
    let listed = init
        .get("plugins")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .any(|p| p.get("name").and_then(|n| n.as_str()) == Some(PLUGIN_NAME))
        })
        .unwrap_or(false);
    if listed {
        SkillsVerdict::Loaded
    } else {
        SkillsVerdict::Rejected
    }
}

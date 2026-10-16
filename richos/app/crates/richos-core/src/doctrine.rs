//! Plain instruction files are delivered by the engine plugin. This module only
//! supplies shared atomic file I/O and install identity data.
use crate::entity::app_config_dir;
use std::path::{Path, PathBuf};
#[derive(Debug, thiserror::Error)]
pub enum DoctrineError {
    #[error("no home directory: RichOS could not work out where to keep its own files")]
    NoHome,
    #[error("the standing instruction could not be written to {path} ({why}) — RichOS will not start Claude without it, because a Claude started without it is not Rich")]
    Unwritable { path: String, why: String },
}

/// The install-invariant identity rendered into the doctrine.
///
/// One field today, and a struct rather than an `Option<String>` parameter because the digest
/// that guards the rendered file has to cover EVERY input; a second input added as a bare
/// argument is a second input somebody forgets to hash.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DoctrineIdentity {
    ceo_name: Option<String>,
}

impl DoctrineIdentity {
    /// Blank, whitespace and absent all mean the same thing: nobody has told the app who this
    /// is. `config.rs` makes the same call for the same reason — *"there is no honest default
    /// for a person's name"* — so an unnamed install renders no name clause rather than an
    /// invented one.
    pub fn new(ceo_name: Option<&str>) -> DoctrineIdentity {
        DoctrineIdentity {
            ceo_name: ceo_name.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string),
        }
    }

    pub fn ceo_name(&self) -> Option<&str> {
        self.ceo_name.as_deref()
    }


}

/// Lowercase hex SHA-256.
pub fn doctrine_path(config_dir: &Path) -> PathBuf { config_dir.join("rich-skills/agents/rich.md") }
pub fn ensure_rendered(config_dir: &Path, _identity: &DoctrineIdentity) -> Result<PathBuf, DoctrineError> { Ok(doctrine_path(config_dir)) }
pub fn install_config_dir() -> Result<PathBuf, DoctrineError> {
    let home = std::env::var_os("HOME").map(PathBuf::from).ok_or(DoctrineError::NoHome)?;
    Ok(app_config_dir(&home))
}
pub fn ensure_for_install() -> Result<PathBuf, DoctrineError> { Ok(install_config_dir()?.join("engine/app/agents/rich.md")) }
pub fn identity_from_config(config_dir: &Path) -> DoctrineIdentity {
    match crate::config::ConfigStore::open(config_dir.join("config.json")) { Ok(c) => DoctrineIdentity::new(c.user_name()), Err(_) => DoctrineIdentity::default() }
}
pub(crate) fn write_verified(path: &Path, want: &str) -> Result<(), DoctrineError> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| DoctrineError::Unwritable {
        path: dir.display().to_string(),
        why: e.to_string(),
    })?;

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "richos".to_string());
    let tmp = dir.join(format!(
        "{name}.incoming.{}.{}",
        std::process::id(),
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::write(&tmp, want.as_bytes()).map_err(|e| DoctrineError::Unwritable {
        path: tmp.display().to_string(),
        why: e.to_string(),
    })?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(DoctrineError::Unwritable { path: path.display().to_string(), why: e.to_string() });
    }

    match std::fs::read_to_string(path) {
        Ok(back) if back == want => Ok(()),
        Ok(_) => Err(DoctrineError::Unwritable {
            path: path.display().to_string(),
            why: "the file on disk does not match what was just written to it".to_string(),
        }),
        Err(e) => Err(DoctrineError::Unwritable {
            path: path.display().to_string(),
            why: format!("it could not be read back after writing: {e}"),
        }),
    }
}

//! One optional literal settings file, snapshotted for each new engine job.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const KEYS: &[&str] = &[
    "RICHOS_BULK_ROOT", "CARGO_TARGET_DIR", "RICHOS_CARGO_CACHE_ROOT",
    "SCCACHE_DIR", "RICHOS_SCCACHE_TMPDIR", "RICHOS_BUILD_CACHE",
    "RICHOS_RECOVERY_EVIDENCE_ROOT", "RICHOS_RECOVERY_WIP_ROOT",
    "RICHOS_CPU_GUARD_STATE", "RICHOS_ENGINE_PASS_DIR",
    "RICHOS_PROOF_RUN_DIR", "RICHOS_AUTOCHECK_PROOF_ROOT",
    "RICHOS_TEST_RESULTS_ROOT", "RICHOS_ANDROID_CACHES_ROOT",
    "RICHOS_NATIVE_IOS_CACHE", "RICHOS_NATIVE_ANDROID_CACHE", "RANDROID_CACHE",
    "RICHOS_GPL_SOURCE_CACHE",
    "RICHOS_MOBILE_CACHE",
    "RICHOS_PHONE_LOCK_DIR",
    "RICHOS_PHONE_WATCH_HOME",
    "RICHOS_PHYSICAL_STORE",
    "RICHOS_PHYSICAL_PRODUCTS",
    "GRADLE_USER_HOME",
    "RICHOS_ENGINE_CHECK_PROOF_DIR", "RICHOS_CAMPAIGN_PARENT",
    "RICHOS_SEARCH_DOCUMENTS_ROOT",
];
pub const RESOLVED: &str = "RICHOS_MACHINE_SETTINGS_RESOLVED";
/// **The document folder the CEO opted into search, and no child process inherits it** (search
/// PRD v3.1, "Goal and first version": *"Setting it explicitly opts that folder into the CEO's
/// private archive search and CEO-bound Rich tool. Other company or worker bindings receive no
/// archive access."*). It is read by the app for its own search service only; the engine, its
/// workers and the nightly's gates never receive it through [`Settings::environment`], so the
/// path to a private archive does not travel with every job.
pub const NOT_FORWARDED: &[&str] = &["RICHOS_SEARCH_DOCUMENTS_ROOT"];
const EXAMPLE: &str = include_str!("../../../../engine/.env.example");

#[derive(Clone, Debug, Default)]
pub struct Settings(pub BTreeMap<String, String>);

fn valid_path(key: &str, value: &str) -> Result<(), String> {
    if value.is_empty() || value.contains('\0') || !Path::new(value).is_absolute() {
        return Err(format!("{key} requires a nonempty literal absolute path"));
    }
    Ok(())
}

pub fn parse(text: &str) -> Result<Settings, String> {
    let mut values = BTreeMap::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') { continue; }
        let (key, value) = line.split_once('=').ok_or_else(|| format!("line {}: expected KEY=value", index + 1))?;
        let (key, value) = (key.trim(), value.trim());
        if !KEYS.contains(&key) || values.contains_key(key) {
            return Err(format!("line {}: duplicate or unknown key {key}", index + 1));
        }
        valid_path(key, value).map_err(|error| format!("line {}: {error}", index + 1))?;
        values.insert(key.into(), value.into());
    }
    Ok(Settings(values))
}

pub fn file(home: &Path) -> PathBuf {
    crate::setup::app_support_richos(home).join(".env")
}

impl Settings {
    pub fn load(home: &Path, environment: &BTreeMap<String, String>) -> Result<Self, String> {
        let mut settings = Self::default();
        if environment.get(RESOLVED).is_none_or(|value| value != "1") {
            let explicit = environment.get("RICHOS_ENV_FILE");
            let source = explicit.map(PathBuf::from).unwrap_or_else(|| file(home));
            if !source.is_absolute() { return Err("RICHOS_ENV_FILE must be an absolute path".into()); }
            match std::fs::read_to_string(&source) {
                Ok(text) => settings = parse(&text).map_err(|error| format!("{}: {error}", source.display()))?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound && explicit.is_none() => {},
                Err(error) => return Err(format!("cannot read settings file {}: {error}", source.display())),
            }
        }
        for key in KEYS {
            if let Some(value) = environment.get(*key) {
                valid_path(key, value)?;
                settings.0.insert((*key).into(), value.clone());
            }
        }
        // Preserve old private files; the current consumer name wins if both exist.
        if let Some(legacy) = settings.0.get("RICHOS_NATIVE_ANDROID_CACHE").cloned() {
            settings.0.entry("RANDROID_CACHE".into()).or_insert(legacy);
        }
        Ok(settings)
    }

    pub fn current() -> Result<Self, String> {
        let mut env = BTreeMap::new();
        for key in KEYS.iter().copied().chain(["HOME", "RICHOS_ENV_FILE", RESOLVED]) {
            if let Some(value) = std::env::var_os(key) {
                env.insert(key.into(), value.into_string().map_err(|_| format!("{key} is not valid UTF-8"))?);
            }
        }
        let home = env.get("HOME").ok_or("HOME is unavailable for machine settings")?;
        Self::load(Path::new(home), &env)
    }

    pub fn environment(&self) -> BTreeMap<String, String> {
        let mut values = self.0.clone();
        for key in NOT_FORWARDED {
            values.remove(*key);
        }
        values.insert(RESOLVED.into(), "1".into());
        values
    }

    /// The folder the CEO included in document search, or `None` when he has not set one.
    /// Never defaulted: unset means no folder is searched (search PRD v3.1).
    pub fn search_documents_root(&self) -> Option<PathBuf> {
        self.0.get("RICHOS_SEARCH_DOCUMENTS_ROOT").map(PathBuf::from)
    }

    /// **The shared cache location for one named cache**, the same resolution as the engine's
    /// `machine_settings.py cache <name>`: `${RICHOS_BULK_ROOT}/caches/<name>` when the bulk
    /// root is set, otherwise `~/Library/Caches/RichOS/<name>` on macOS (`~/.cache/richos/<name>`
    /// elsewhere). A configured bulk root that is missing, unmounted or not a folder is an error
    /// naming it, never a quiet fall back to the default: a silently relocated cache hides a
    /// storage failure.
    pub fn cache(&self, home: &Path, name: &str) -> Result<PathBuf, String> {
        if let Some(bulk) = self.0.get("RICHOS_BULK_ROOT") {
            let root = validate_existing_storage(Path::new(bulk))
                .map_err(|error| format!("RICHOS_BULK_ROOT {bulk} is not usable: {error}"))?;
            if !root.is_dir() {
                return Err(format!("RICHOS_BULK_ROOT {bulk} is not a folder"));
            }
            return Ok(root.join("caches").join(name));
        }
        let base = if cfg!(target_os = "macos") { home.join("Library/Caches/RichOS") } else { home.join(".cache/richos") };
        Ok(base.join(name))
    }

    pub fn agent_root(&self) -> PathBuf {
        self.0.get("RICHOS_BULK_ROOT").map(|root| {
            let base = Path::new(root);
            base.canonicalize().unwrap_or_else(|_| base.to_path_buf()).join("tmp/claude")
        })
            .unwrap_or_else(|| std::env::temp_dir().join("richos-agents"))
    }
}

/// Validate an existing storage input before an isolated tool writes beneath it.
pub fn validate_existing_storage(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() { return Err("storage path must be absolute".into()); }
    let resolved = path.canonicalize().map_err(|error| error.to_string())?;
    #[cfg(unix)]
    if let Ok(tail) = resolved.strip_prefix("/Volumes") {
        if let Some(name) = tail.components().next() {
            use std::os::unix::fs::MetadataExt;
            let volume = Path::new("/Volumes").join(name.as_os_str());
            let mounted = std::fs::metadata(&volume).map_err(|error| error.to_string())?;
            let parent = std::fs::metadata("/Volumes").map_err(|error| error.to_string())?;
            if mounted.dev() == parent.dev() { return Err(format!("selected storage volume is not mounted: {}", volume.display())); }
        }
    }
    Ok(resolved)
}

/// Exclusive creation preserves all existing private settings, including a symlink.
pub fn create_starter(home: &Path) -> Result<(), String> {
    use std::io::Write;
    let path = file(home);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|error| error.to_string())?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)] {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut output) => output.write_all(EXAMPLE.as_bytes()).map_err(|error| error.to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_reader_and_complete_key_contract() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!("../../../../engine/scripts/lib/machine_settings.fixtures.json")).unwrap();
        let keys: Vec<_> = fixture["keys"].as_array().unwrap().iter().map(|key| key.as_str().unwrap()).collect();
        assert_eq!(KEYS, keys);
        for case in fixture["cases"].as_array().unwrap() {
            let result = parse(case["text"].as_str().unwrap());
            if case["error"].as_bool().unwrap_or(false) {
                assert!(result.is_err(), "{}", case["name"]);
            } else {
                assert_eq!(serde_json::to_value(result.unwrap().0).unwrap(), case["values"], "{}", case["name"]);
            }
        }
        let complete = KEYS.iter().map(|key| format!("{key}=/work data/clé")).collect::<Vec<_>>().join("\n");
        let settings = parse(&complete).unwrap();
        let forwarded = settings.environment();
        for key in KEYS {
            if NOT_FORWARDED.contains(key) {
                assert!(!forwarded.contains_key(*key), "{key} must never reach a child process");
            } else {
                assert_eq!(forwarded.get(*key).unwrap(), "/work data/clé");
            }
        }
        assert_eq!(forwarded.len(), KEYS.len() - NOT_FORWARDED.len() + 1);
    }
    #[test]
    fn search_documents_root_is_read_but_never_forwarded() {
        let settings = parse("RICHOS_SEARCH_DOCUMENTS_ROOT=/private archive/chats\n").unwrap();
        assert_eq!(settings.search_documents_root(), Some(PathBuf::from("/private archive/chats")));
        assert!(!settings.environment().contains_key("RICHOS_SEARCH_DOCUMENTS_ROOT"));
        assert_eq!(parse("").unwrap().search_documents_root(), None);
        assert!(parse("RICHOS_SEARCH_DOCUMENTS_ROOT=relative/chats\n").is_err());
    }
    #[test]
    fn named_cache_follows_the_bulk_root_or_the_platform_default_and_never_relocates() {
        let home = Path::new("/home of someone");
        let unset = Settings::default().cache(home, "search").unwrap();
        let expected = if cfg!(target_os = "macos") { "/home of someone/Library/Caches/RichOS/search" } else { "/home of someone/.cache/richos/search" };
        assert_eq!(unset, PathBuf::from(expected));
        let bulk = std::env::temp_dir().join(format!("bulk-cache-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&bulk).unwrap();
        let settings = parse(&format!("RICHOS_BULK_ROOT={}\n", bulk.display())).unwrap();
        assert_eq!(settings.cache(home, "search").unwrap(), bulk.canonicalize().unwrap().join("caches/search"));
        std::fs::remove_dir_all(&bulk).unwrap();
        let missing = settings.cache(home, "search").unwrap_err();
        assert!(missing.contains("RICHOS_BULK_ROOT"), "{missing}");
    }
    #[test]
    fn legacy_android_key_is_forwarded_with_explicit_current_name_priority() {
        let home = std::env::temp_dir().join(format!("legacy-android-settings-{}", uuid::Uuid::new_v4()));
        create_starter(&home).unwrap();
        std::fs::write(file(&home), "RICHOS_NATIVE_ANDROID_CACHE=/old cache\n").unwrap();
        let loaded = Settings::load(&home, &BTreeMap::new()).unwrap();
        assert_eq!(loaded.environment()["RANDROID_CACHE"], "/old cache");
        let override_env = BTreeMap::from([("RICHOS_NATIVE_ANDROID_CACHE".into(), "/legacy environment".into())]);
        assert_eq!(Settings::load(&home, &override_env).unwrap().0["RANDROID_CACHE"], "/legacy environment");
        std::fs::write(file(&home), "RICHOS_NATIVE_ANDROID_CACHE=/old cache\nRANDROID_CACHE=/current cache\n").unwrap();
        assert_eq!(Settings::load(&home, &override_env).unwrap().0["RANDROID_CACHE"], "/current cache");
        assert_eq!(Settings::load(&home, &loaded.environment()).unwrap().0, loaded.0);
        std::fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn file_selection_precedence_snapshot_and_starter_preservation() {
        let home = std::env::temp_dir().join(format!("machine-settings-{}-{}", std::process::id(), uuid::Uuid::new_v4()));
        create_starter(&home).unwrap();
        let source = file(&home);
        std::fs::write(&source, "CARGO_TARGET_DIR=/file cache\n").unwrap();
        create_starter(&home).unwrap();
        assert_eq!(std::fs::read_to_string(&source).unwrap(), "CARGO_TARGET_DIR=/file cache\n");
        let env = BTreeMap::from([("CARGO_TARGET_DIR".into(), "/environment cache".into())]);
        let settings = Settings::load(&home, &env).unwrap();
        assert_eq!(settings.0["CARGO_TARGET_DIR"], "/environment cache");
        std::fs::write(&source, "UNKNOWN=/new\n").unwrap();
        assert!(Settings::load(&home, &env).is_err());
        assert_eq!(Settings::load(&home, &settings.environment()).unwrap().0, settings.0);
        let missing = BTreeMap::from([("RICHOS_ENV_FILE".into(), home.join("absent").display().to_string())]);
        assert!(Settings::load(&home, &missing).is_err());
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&source).unwrap().permissions().mode() & 0o777, 0o600);
        }
        std::fs::remove_dir_all(home).unwrap();
    }
}

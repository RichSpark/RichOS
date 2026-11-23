//! RichDoctor's per-user physical footprint history. RichOS reads it through its status server.
use serde::Serialize;
use std::path::Path;
#[cfg(target_os = "macos")]
use std::path::PathBuf;

pub const TOOL: &str = "suite_memory";

#[cfg(target_os = "macos")]
mod history;
#[cfg(target_os = "macos")]
mod mac;

#[cfg(target_os = "macos")]
pub use history::Recorder;

#[cfg(target_os = "macos")]
fn path(state_root: &Path) -> PathBuf {
    state_root.join("suite-memory-v1.sqlite")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
enum Kind {
    RichOs,
    RichFiles,
    RichGlance,
    Capture,
    RichScribe,
    Gestures,
    RichDoctor,
    ClaudeCode,
    Codex,
}

const KINDS: [Kind; 9] = [
    Kind::RichOs,
    Kind::RichFiles,
    Kind::RichGlance,
    Kind::Capture,
    Kind::RichScribe,
    Kind::Gestures,
    Kind::RichDoctor,
    Kind::ClaudeCode,
    Kind::Codex,
];

impl Kind {
    fn label(self) -> &'static str {
        match self {
            Self::RichOs => "RichOS",
            Self::RichFiles => "RichFiles",
            Self::RichGlance => "RichGlance",
            Self::Capture => "RichGlance Capture",
            Self::RichScribe => "RichScribe",
            Self::Gestures => "Window Gestures",
            Self::RichDoctor => "RichDoctor",
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Snapshot {
    status: &'static str,
    metric: &'static str,
    sampled_at_ms: Option<i64>,
    age_ms: Option<i64>,
    apps: Vec<AppMemory>,
}

#[derive(Debug, Serialize)]
struct AppMemory {
    app: &'static str,
    current: Option<Current>,
    last_48_hours: Option<Trend>,
    last_90_days: Option<Trend>,
    sustained_growth: Option<Growth>,
}

#[derive(Debug, Serialize)]
struct Current {
    status: &'static str,
    bytes: Option<i64>,
    process_count: i64,
}

#[derive(Debug, Serialize)]
struct Trend {
    first_hour_start_ms: i64,
    last_hour_start_ms: i64,
    observed_hours: i64,
    samples: i64,
    min_bytes: i64,
    mean_bytes: i64,
    max_bytes: i64,
    first_hour_mean_bytes: i64,
    last_hour_mean_bytes: i64,
}

#[derive(Debug, Serialize)]
struct Growth {
    window_minutes: i64,
    first_hour_mean_bytes: i64,
    last_hour_mean_bytes: i64,
    increase_bytes: i64,
}

impl Snapshot {
    fn new(status: &'static str) -> Self {
        Self {
            status,
            metric: "phys_footprint_bytes",
            sampled_at_ms: None,
            age_ms: None,
            apps: KINDS
                .iter()
                .map(|kind| AppMemory {
                    app: kind.label(),
                    current: None,
                    last_48_hours: None,
                    last_90_days: None,
                    sustained_growth: None,
                })
                .collect(),
        }
    }
}

/// The last published measurements, age and historical summaries. An absent or stale
/// sampler cannot supply a current reading. This reader never creates the database.
pub fn read(state_root: &Path) -> Snapshot {
    #[cfg(target_os = "macos")]
    return history::read(state_root, crate::util::now_millis() as i64);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = state_root;
        Snapshot::new("unsupported")
    }
}

#[derive(Debug, Serialize)]
pub struct Health {
    pub status: &'static str,
    pub sampled_at_ms: Option<i64>,
    pub same_user_processes: i64,
    pub matched_processes: i64,
    pub unreadable_processes: i64,
    pub capture_us: i64,
}

impl Health {
    fn unavailable(status: &'static str) -> Self {
        Self {
            status,
            sampled_at_ms: None,
            same_user_processes: 0,
            matched_processes: 0,
            unreadable_processes: 0,
            capture_us: 0,
        }
    }
}

/// Read only the latest capture outcome for the bounded RichDoctor report.
pub fn health(state_root: &Path) -> Health {
    #[cfg(target_os = "macos")]
    return history::health(state_root);
    #[cfg(not(target_os = "macos"))]
    {
        let _ = state_root;
        Health::unavailable("unsupported")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::UnknownScreen;
    use crate::status_tools::{self, StatusToolScope};
    use serde_json::json;

    #[test]
    fn the_tool_requires_the_host_scope_and_never_creates_history() {
        let root = std::env::temp_dir().join(format!("suite-memory-tool-{}", uuid::Uuid::new_v4()));
        let scope = root.join("status.json");
        let state = root.join("engine-state");
        assert!(status_tools::call(&scope, &UnknownScreen, TOOL, json!({})).is_err());
        status_tools::write_scope(
            &scope,
            &StatusToolScope {
                version: 1,
                state_root: state.clone(),
                entity_id: "entity".into(),
                thread_id: "thread".into(),
            },
        )
        .unwrap();
        assert!(status_tools::call(
            &scope,
            &UnknownScreen,
            TOOL,
            json!({"state_root":"elsewhere"})
        )
        .is_err());
        let value = status_tools::call(&scope, &UnknownScreen, TOOL, json!({})).unwrap();
        assert_eq!(value["metric"], "phys_footprint_bytes");
        let status = if cfg!(target_os = "macos") {
            "unavailable"
        } else {
            "unsupported"
        };
        assert_eq!(value["status"], status);
        let apps = value["apps"].as_array().unwrap();
        assert_eq!(apps.len(), 9);
        assert!(apps
            .iter()
            .all(|app| app["current"].is_null() && app["last_48_hours"].is_null()));
        assert!(!state.exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}

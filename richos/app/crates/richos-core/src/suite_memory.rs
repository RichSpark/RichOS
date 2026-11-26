//! RichDoctor's per-user physical footprint history. RichOS reads it through its status server.
//! The same per-app reading, taken live and kept nowhere, is [`Live`] (My RichOS, round 109).
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

    /// A stable name for the app, for a reader that keys its own history by app.
    fn id(self) -> &'static str {
        match self {
            Self::RichOs => "richos",
            Self::RichFiles => "richfiles",
            Self::RichGlance => "richglance",
            Self::Capture => "richglance-capture",
            Self::RichScribe => "richscribe",
            Self::Gestures => "window-gestures",
            Self::RichDoctor => "richdoctor",
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
        }
    }
}

/// The suite apps that run inside RichOS (round 109's memory view lists them beside the others):
/// RichBrief (the meeting panel and its capture companion) and RichCleaner (its panel) have
/// processes of their own that RichOS started, given by the host as [`Hosted`] (RichCleaner's panel
/// and its own window); RichFinder is a page in RichOS's own window, so its memory is RichOS's; and
/// RichPass is a document that opens in the user's browser, whose memory is the browser's, which
/// RichOS does not measure ([`LiveApp::within`]).
const HOSTED: [(&str, &str); 4] = [("richbrief", "RichBrief"), ("richfinder", "RichFinder"), ("richpass", "RichPass"), ("richcleaner", "RichCleaner")];
const WITHIN: [(&str, &str); 2] = [("richfinder", "richos"), ("richpass", "browser")];

/// What holds `id`'s memory when it has no process of its own: `richos` for a page in RichOS's
/// window, `browser` for a document in the user's browser ([`LiveApp::within`]).
pub fn within(id: &str) -> Option<&'static str> {
    WITHIN.iter().find(|(i, _)| *i == id).map(|(_, w)| *w)
}

/// How many apps a live reading names.
pub const LIVE_APPS: usize = KINDS.len() + HOSTED.len();

/// Every app a live reading names, in round 109's order (its memory view's apps first, then the
/// rest this reader measures): `(id, label)`.
pub fn apps() -> [(&'static str, &'static str); LIVE_APPS] {
    let kind = |k: Kind| (k.id(), k.label());
    [
        kind(Kind::RichOs), kind(Kind::RichScribe), HOSTED[0], kind(Kind::RichFiles), kind(Kind::RichGlance), HOSTED[1],
        HOSTED[2], HOSTED[3], kind(Kind::Capture), kind(Kind::Gestures), kind(Kind::RichDoctor), kind(Kind::ClaudeCode),
        kind(Kind::Codex),
    ]
}

/// A suite app running inside RichOS's processes, as the host that started it knows it: its web
/// views' own WebKit processes (`web`, which the capture counts as RichOS's and which are moved
/// from RichOS to this app) and the helper processes it started (`helpers`, which the capture
/// counts nowhere). By pid; a pid that has exited or is no longer this user's is skipped.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hosted {
    pub id: &'static str,
    pub web: Vec<i32>,
    pub helpers: Vec<i32>,
}

/// One app's memory now, from one live reading.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LiveApp {
    pub id: &'static str,
    pub app: &'static str,
    /// The processes' summed physical footprint. `None` when one of them could not be read.
    pub bytes: Option<i64>,
    /// How many processes the app has now. Zero: it is not open.
    pub processes: i64,
    /// What holds this one's memory when it has no process of its own: `richos` (a page in
    /// RichOS's window) or `browser` (a document in the user's browser, not measured).
    pub within: Option<&'static str>,
}

/// **A live reading of each suite app's memory, as often as its caller asks** (round 109, My
/// RichOS, which asks every two seconds while it is on). The very capture RichDoctor's minute
/// recorder makes (`mac::capture`: one libproc pass, no subprocess), called more often and
/// written nowhere. RichScribe's WebKit services are put on RichScribe from what it last
/// published, read again at most every [`Live::SCRIBE_EVERY_MS`] (its publisher's own pace).
pub struct Live {
    #[cfg(target_os = "macos")]
    state_root: PathBuf,
    #[cfg(target_os = "macos")]
    scribe: Option<(i64, Option<mac::ScribeWebKit>)>,
}

impl Live {
    /// RichScribe publishes its WebKit services every 20 seconds (`dictation/bar.rs`).
    pub const SCRIBE_EVERY_MS: i64 = 20_000;

    pub fn new(state_root: &Path) -> Self {
        #[cfg(target_os = "macos")]
        return Self { state_root: state_root.to_path_buf(), scribe: None };
        #[cfg(not(target_os = "macos"))]
        {
            let _ = state_root;
            Self {}
        }
    }

    /// Each app's memory now, in [`apps`]'s order, with the hosted apps' processes as the host
    /// last gave them.
    pub fn sample(&mut self, now_ms: i64, hosted: &[Hosted]) -> Result<Vec<LiveApp>, String> {
        #[cfg(target_os = "macos")]
        {
            let fresh = self.scribe.as_ref().is_some_and(|(at, _)| (0..Self::SCRIBE_EVERY_MS).contains(&(now_ms - at)));
            if !fresh {
                self.scribe = Some((now_ms, history::richscribe_webkit(&self.state_root)));
            }
            let scribe = self.scribe.as_ref().and_then(|(_, scribe)| scribe.as_ref());
            let capture = mac::capture(scribe).map_err(|error| error.to_string())?;
            let measured: Vec<LiveApp> = KINDS
                .iter()
                .zip(capture.readings.iter())
                .map(|(kind, reading)| LiveApp {
                    id: kind.id(),
                    app: kind.label(),
                    bytes: if reading.count == 0 { Some(0) } else { reading.bytes },
                    processes: reading.count,
                    within: None,
                })
                .collect();
            Ok(arrange(measured, hosted, mac::footprint_of))
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (now_ms, hosted);
            Err("Live app memory is read on macOS.".into())
        }
    }
}

/// The measured apps and the hosted ones in [`apps`]'s order: each hosted app's web processes
/// moved from RichOS's figure to its own (they are in RichOS's by the capture's parentage), its
/// helpers added (they are in nobody's), RichFinder and RichPass named as within RichOS.
fn arrange(measured: Vec<LiveApp>, hosted: &[Hosted], footprint: impl Fn(i32) -> Option<i64>) -> Vec<LiveApp> {
    let mut by_id: std::collections::HashMap<&'static str, LiveApp> = measured.into_iter().map(|a| (a.id, a)).collect();
    for (id, app) in HOSTED {
        let mut row = LiveApp { id, app, bytes: Some(0), processes: 0, within: None };
        if let Some(holder) = within(id) {
            row.within = Some(holder);
            row.bytes = None;
        } else if let Some(given) = hosted.iter().find(|h| h.id == id) {
            let mut web = 0;
            for (pid, moved) in given.web.iter().map(|p| (p, true)).chain(given.helpers.iter().map(|p| (p, false))) {
                let Some(bytes) = footprint(*pid) else { continue };
                row.bytes = row.bytes.map(|b| b + bytes);
                row.processes += 1;
                if moved {
                    web += bytes;
                }
            }
            if let Some(richos) = by_id.get_mut("richos") {
                richos.bytes = richos.bytes.map(|b| (b - web).max(0));
            }
        }
        by_id.insert(id, row);
    }
    apps().iter().filter_map(|(id, _)| by_id.remove(id)).collect()
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
    fn a_live_reading_names_every_app_in_order_and_writes_nothing() {
        let state = std::env::temp_dir().join(format!("suite-memory-live-{}", uuid::Uuid::new_v4())).join("engine-state");
        let mut live = Live::new(&state);
        let reading = live.sample(crate::util::now_millis() as i64, &[]);
        assert_eq!(reading.is_ok(), cfg!(target_os = "macos"), "{reading:?}");
        if let Ok(apps) = reading {
            let ids: Vec<_> = apps.iter().map(|a| a.id).collect();
            assert_eq!(ids, apps_ids());
            // An app with no process reads as not open (0 bytes), never as unreadable; a page in
            // RichOS's window is named as within RichOS.
            assert!(apps.iter().all(|a| a.processes > 0 || a.bytes == Some(0) || a.within.is_some()), "{apps:?}");
        }
        assert!(!state.exists(), "a live reading created the history");
    }

    /// Round 109's memory view lists RichBrief, RichFinder, RichPass and RichCleaner beside the
    /// processes RichDoctor measures: every one is in a reading, in the view's order; a hosted
    /// app's web process moves from RichOS's figure to its own, its helper adds to it.
    #[test]
    fn every_app_of_round_109_is_in_a_reading_and_hosted_processes_move_off_richos() {
        let measured: Vec<LiveApp> = KINDS
            .iter()
            .map(|k| LiveApp { id: k.id(), app: k.label(), bytes: Some(if *k == Kind::RichOs { 1_000 } else { 0 }), processes: (*k == Kind::RichOs) as i64, within: None })
            .collect();
        let hosted = [Hosted { id: "richcleaner", web: vec![41], helpers: vec![] }, Hosted { id: "richbrief", web: vec![42], helpers: vec![43, 99] }];
        let footprint = |pid: i32| match pid { 41 => Some(100), 42 => Some(200), 43 => Some(50), _ => None };
        let apps = arrange(measured, &hosted, footprint);
        let names: Vec<_> = apps.iter().map(|a| a.app).collect();
        assert_eq!(&names[..8], ["RichOS", "RichScribe", "RichBrief", "RichFiles", "RichGlance", "RichFinder", "RichPass", "RichCleaner"]);
        assert_eq!(apps.len(), LIVE_APPS);
        let of = |id: &str| apps.iter().find(|a| a.id == id).unwrap().clone();
        assert_eq!(of("richos").bytes, Some(700), "the panels' web processes stay counted in RichOS");
        assert_eq!((of("richcleaner").bytes, of("richcleaner").processes), (Some(100), 1));
        assert_eq!((of("richbrief").bytes, of("richbrief").processes), (Some(250), 2), "an exited helper (99) is skipped");
        assert_eq!((of("richfinder").within, of("richpass").within), (Some("richos"), Some("browser")));
        assert_eq!(of("richfinder").bytes, None, "a page in RichOS's window is given no figure of its own");
    }

    fn apps_ids() -> Vec<&'static str> {
        apps().iter().map(|(id, _)| *id).collect()
    }

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

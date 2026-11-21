//! Local numeric state beside the turn mutex. Visibility never gates accounting.
use richos_core::{read_view::{SpineReader, SpineView}, steering::TurnControl,
    swearmeter::{Snapshot, DETECTOR_VERSION, EVENT_SWEARMETER}, swearmeter_periods::{Dates, Periods, local_month},
    swearmeter_provider_history::{History, Reader}, swearmeter_questions::{self, Projection}};
use sha2::{Digest, Sha256};
use std::{
    fs, io::{self, Write}, path::{Path, PathBuf},
    sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, State};

pub struct Service {
    app: AppHandle,
    reader: SpineReader,
    control: TurnControl,
    cards: Arc<Mutex<Projection>>,
    last: Mutex<Snapshot>,
    orphaned: AtomicBool,
    source_seen: AtomicBool,
    sources: ProviderSources,
    cache_path: PathBuf,
    history: Mutex<Option<SavedHistory>>,
}
impl Service {
    pub fn new(
        app: AppHandle, reader: SpineReader, control: TurnControl, cards: Projection,
        source_seen: bool, data_dir: &Path,
    ) -> Self {
        let sources = ProviderSources::new(&app);
        let cache_path = data_dir.join("swearmeter-provider-cache");
        let history = SavedHistory::load(&cache_path, sources.scope());
        let orphaned = reader.snapshot().ledger().swearmeter().composites().iter().any(|id| !cards.known_composites.contains(id));
        Self { app, reader, control, cards: Arc::new(Mutex::new(cards)), last: Mutex::new(Snapshot::default()),
            orphaned: AtomicBool::new(orphaned), source_seen: AtomicBool::new(source_seen), sources, cache_path, history: Mutex::new(history) }
    }
    pub fn cards(&self) -> Arc<Mutex<Projection>> { self.cards.clone() }
    pub fn snapshot(&self) -> Snapshot {
        // Serialize reads before observing either source, including a concurrent event
        // and initial bridge read. An older observation can never gain a newer revision.
        let mut last = self.last.lock().unwrap_or_else(|poison| poison.into_inner());
        let (view, pending, pending_unknown, intake_complete, pending_dates) = self.control.swearmeter_inputs(&self.reader);
        let messages = view.ledger().swearmeter();
        let cards = swearmeter_questions::lock(&self.cards);
        let history = self.history.lock().unwrap_or_else(|poison| poison.into_inner());
        let now = richos_core::util::now_millis();
        let mut periods = Dates::report(&[messages.dates(), &pending_dates, &cards.dates], now, local_month);
        if let Some(saved) = history.as_ref() { periods.include_saved(&saved.periods); }
        let mut next = Snapshot {
            detector_version: DETECTOR_VERSION,
            count: messages.count() + pending + cards.count + history.as_ref().map_or(0, |saved| saved.count),
            periods,
            history_loading: history.is_none(),
            provider_history_incomplete: history.as_ref().is_some_and(|saved| saved.incomplete),
            provider_history_stale: history.as_ref().is_some_and(|saved| saved.refresh_due(now)),
            ambiguous_matches: history.as_ref().map_or(0, |saved| saved.ambiguous_matches),
            unknown_card_inputs: cards.unknown_inputs,
            unknown_message_inputs: messages.unknown_inputs() + pending_unknown,
            incomplete_history: !view.ledger().history_health().is_clean() || !intake_complete
                || self.orphaned.load(Ordering::Relaxed),
            available: cards.available && !self.last.is_poisoned(),
            revision: last.revision,
        };
        if *last != next { next.revision += 1; *last = next; }
        last.clone()
    }
    pub fn unavailable(&self) {
        swearmeter_questions::lock(&self.cards).available = false;
        self.publish_current();
    }
    pub fn observe_card_source(&self, exists: bool) {
        if exists { self.source_seen.store(true, Ordering::Relaxed); }
        else if self.source_seen.load(Ordering::Relaxed) { self.unavailable(); }
    }
    fn refresh_coverage(&self) {
        let view = self.reader.snapshot();
        let cards = swearmeter_questions::lock(&self.cards);
        self.orphaned.store(view.ledger().swearmeter().composites().iter().any(|id| !cards.known_composites.contains(id)), Ordering::Relaxed);
    }
    pub fn publish_current(&self) { self.publish(&self.app); }
    pub fn publish(&self, app: &AppHandle) {
        if app.emit(EVENT_SWEARMETER, self.snapshot()).is_err() {
            // Closing the window can leave the maintained result without a listener.
        }
    }
}

fn configured_root(variable: &str, home: &Path, directory: &str) -> PathBuf {
    std::env::var_os(variable).map(PathBuf::from).filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(directory))
}

const REFRESH_MS: u64 = 60 * 60 * 1000;
const CACHE_SCHEMA: u32 = 1;
type ProviderScope = [Option<[u8; 32]>; 2];

struct ProviderSources {
    claude: Option<PathBuf>,
    codex: Option<PathBuf>,
}

impl ProviderSources {
    fn new(app: &AppHandle) -> Self {
        let home = app.path().home_dir().ok();
        Self {
            claude: home.as_ref().map(|home| configured_root("CLAUDE_CONFIG_DIR", home, ".claude")),
            codex: home.as_ref().map(|home| configured_root("CODEX_HOME", home, ".codex")),
        }
    }

    fn scope(&self) -> ProviderScope {
        [&self.claude, &self.codex].map(|root| root.as_ref().map(|path| {
            let mut digest = Sha256::new();
            let bytes = path.as_os_str().as_encoded_bytes();
            digest.update(bytes.len().to_le_bytes());
            digest.update(bytes);
            // Directory identity survives normal history writes and rejects a replaced root.
            #[cfg(unix)]
            if let Ok(metadata) = fs::metadata(path) {
                use std::os::unix::fs::MetadataExt;
                digest.update(metadata.dev().to_le_bytes());
                digest.update(metadata.ino().to_le_bytes());
            }
            digest.finalize().into()
        }))
    }
}

/// Provider text and per-submission identities stay in the reader. This snapshot
/// contains only numeric accounting, its version and its provider scope.
#[derive(serde::Deserialize, serde::Serialize)]
struct SavedHistory {
    schema: u32,
    detector_version: u32,
    scope: ProviderScope,
    through: u64,
    count: u64,
    periods: Periods,
    incomplete: bool,
    ambiguous_matches: u64,
}

impl SavedHistory {
    fn new(scope: ProviderScope, through: u64, history: History) -> Self {
        Self {
            schema: CACHE_SCHEMA,
            detector_version: DETECTOR_VERSION,
            scope,
            through,
            count: history.count,
            periods: Dates::report(&[&history.dates], through, local_month),
            incomplete: history.incomplete,
            ambiguous_matches: history.ambiguous_matches,
        }
    }

    fn load(path: &Path, scope: ProviderScope) -> Option<Self> {
        let bytes = fs::read(path).ok()?;
        let (checksum, payload) = bytes.split_at_checked(32)?;
        if Sha256::digest(payload).as_slice() != checksum { return None; }
        let saved: Self = serde_json::from_slice(payload).ok()?;
        (saved.schema == CACHE_SCHEMA && saved.detector_version == DETECTOR_VERSION && saved.scope == scope)
            .then_some(saved)
    }

    fn save(&self, path: &Path) -> io::Result<()> {
        let payload = serde_json::to_vec(self)?;
        let mut temporary = tempfile::NamedTempFile::new_in(path.parent().unwrap_or_else(|| Path::new(".")))?;
        temporary.write_all(&Sha256::digest(&payload))?;
        temporary.write_all(&payload)?;
        temporary.persist(path)?;
        Ok(())
    }

    fn refresh_after(&self, now: u64) -> u64 {
        now.checked_sub(self.through).map_or(0, |age| REFRESH_MS.saturating_sub(age))
    }

    fn refresh_due(&self, now: u64) -> bool {
        self.refresh_after(now) == 0
    }
}

pub fn start(app: AppHandle) {
    let handle = app.clone();
    let worker = std::thread::Builder::new().name("richos-swearmeter-history".into()).spawn(move || {
        let state = handle.state::<crate::AppState>();
        let service = &state.swearmeter;
        let mut reader = Reader::default();
        loop {
            let now = richos_core::util::now_millis();
            let delay = service.history.lock().unwrap_or_else(|poison| poison.into_inner())
                .as_ref().map_or(0, |saved| saved.refresh_after(now));
            if delay > 0 {
                // Recheck the wall clock after sleep or a clock change. Only the
                // deadline check wakes here. Provider files are read once per hour.
                std::thread::sleep(Duration::from_millis(delay.min(60_000)));
                continue;
            }

            let started = Instant::now();
            let scope = service.sources.scope();
            let mut history = reader.read_through(
                service.sources.claude.as_deref(), service.sources.codex.as_deref(), now,
            );
            history.incomplete |= service.sources.claude.is_none();
            let current_scope = service.sources.scope();
            if scope != current_scope {
                // An unrelated provider directory cannot inherit this batch's counts.
                reader = Reader::default();
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
            let saved = SavedHistory::new(current_scope, now, history);
            if let Err(error) = saved.save(&service.cache_path) {
                eprintln!("[richos] SwearMeter provider cache could not be saved: {error}");
            }
            {
                let mut previous = service.history.lock().unwrap_or_else(|poison| poison.into_inner());
                if previous.as_ref().is_none_or(|prior| prior.count != saved.count
                    || prior.incomplete != saved.incomplete
                    || prior.ambiguous_matches != saved.ambiguous_matches)
                {
                    eprintln!("[richos] SwearMeter historical sources: {} ({} ms)",
                        if saved.incomplete { "partial" } else { "read" }, started.elapsed().as_millis());
                }
                *previous = Some(saved);
            }
            service.publish_current();
        }
    });
    if let Err(error) = worker {
        eprintln!("[richos] SwearMeter historical sources unavailable: {error}");
        let state = app.state::<crate::AppState>();
        let service = &state.swearmeter;
        service.history.lock().unwrap_or_else(|poison| poison.into_inner()).get_or_insert_with(|| {
            SavedHistory::new(
                service.sources.scope(), richos_core::util::now_millis(),
                History { incomplete: true, ..History::default() },
            )
        });
        service.publish_current();
    }
}

#[derive(serde::Serialize)]
pub struct View { show: bool, result: Snapshot }

#[richos_command::command]
pub fn get_swearmeter(state: State<crate::AppState>) -> View {
    View { show: state.config.lock().unwrap().show_swearmeter(), result: state.swearmeter.snapshot() }
}

#[richos_command::command]
pub fn set_swearmeter_visible(state: State<crate::AppState>, show: bool) -> Result<(), String> {
    state.config.lock().unwrap().set_show_swearmeter(show).map_err(|e| e.to_string())
}

/// Explicit recovery from an unreadable source. A visibility click never calls this.
#[richos_command::command]
pub fn retry_swearmeter(app: AppHandle, state: State<crate::AppState>) -> Result<Snapshot, String> {
    swearmeter_questions::rebuild(&state.swearmeter.cards, || {
        if state.swearmeter.source_seen.load(Ordering::Relaxed)
            && !state.data_dir.join("engine-state/questions/store.json").is_file() {
            return Err("Saved card inputs are still unavailable".into());
        }
        richos_core::questions::Store::new(state.data_dir.join("engine-state")).swearmeter_projection()
    })?;
    state.swearmeter.last.clear_poison();
    state.swearmeter.refresh_coverage();
    let result = state.swearmeter.snapshot();
    if app.emit(EVENT_SWEARMETER, &result).is_err() {
        // A caller can still use the returned snapshot.
    }
    Ok(result)
}


/// No per-message checkpoint writes. An interrupted shutdown leaves a safe cache
/// miss; ordinary startup rebuilds from the two existing histories in that case.
pub fn save_on_exit(state: &crate::AppState) {
    let Ok(spine) = state.spine.try_lock() else { return; };
    let store = richos_core::questions::Store::new(state.data_dir.join("engine-state"));
    let Ok((_, cards, source)) = store.swearmeter_startup_cached(None) else { return; };
    if !cards.available { return; }
    let cards = richos_core::swearmeter_checkpoint::CachedCards { source, summary: cards.summary() };
    if let Err(error) = richos_core::swearmeter_checkpoint::Checkpoint::save(
        &state.data_dir.join("swearmeter-cache"), &state.data_dir.join("conversation-ledger.jsonl"),
        &state.data_dir.join("engine-state/questions/store.json"), spine.ledger(), cards) {
        eprintln!("[richos] numeric checkpoint was not saved: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_provider_counts_survive_restart_and_an_overdue_refresh() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("counts");
        let scope = [Some([11; 32]), Some([22; 32])];
        let through = 1_794_597_600_000;
        let mut history = History { count: 4, incomplete: true, ambiguous_matches: 1, ..History::default() };
        history.dates.add(Some(through - 20_000), 4);
        SavedHistory::new(scope, through, history).save(&path).unwrap();

        let saved = SavedHistory::load(&path, scope).unwrap();
        assert_eq!(saved.refresh_after(through + 10_000), REFRESH_MS - 10_000);
        assert!(!saved.refresh_due(through + REFRESH_MS - 1));
        assert!(saved.refresh_due(through + REFRESH_MS));
        assert!(saved.refresh_due(through - 1));
        let now = through + 2 * REFRESH_MS;
        let mut live = Dates::default();
        live.add(Some(now), 2);
        let mut report = Dates::report(&[&live], now, local_month);
        report.include_saved(&saved.periods);
        assert_eq!(saved.count, 4);
        assert_eq!(saved.periods.last_30_days, 4);
        assert_eq!(report.last_30_days, 6);
        assert_eq!(report.months[0].count, 6);
        assert!(saved.incomplete);
        assert_eq!(saved.ambiguous_matches, 1);

        let mut refreshed = SavedHistory::new(scope, now, History::default());
        refreshed.save(&path).unwrap();
        assert_eq!(SavedHistory::load(&path, scope).unwrap().count, 0);
        refreshed.detector_version += 1;
        refreshed.save(&path).unwrap();
        assert!(SavedHistory::load(&path, scope).is_none());
    }

    #[test]
    fn missing_corrupt_or_other_profile_cache_never_becomes_a_verified_count() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("counts");
        let scope = [Some([11; 32]), None];
        assert!(SavedHistory::load(&path, scope).is_none());
        SavedHistory::new(scope, 1_794_597_600_000, History::default()).save(&path).unwrap();
        assert!(SavedHistory::load(&path, [None, Some([11; 32])]).is_none());
        let mut bytes = fs::read(&path).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        fs::write(&path, bytes).unwrap();
        assert!(SavedHistory::load(&path, scope).is_none());
    }
}

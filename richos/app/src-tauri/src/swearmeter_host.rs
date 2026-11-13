//! Local numeric state beside the turn mutex. Visibility never gates accounting.
use richos_core::{read_view::{SpineReader, SpineView}, steering::TurnControl,
    swearmeter::{Snapshot, DETECTOR_VERSION, EVENT_SWEARMETER}, swearmeter_periods::{Dates, local_month},
    swearmeter_provider_history::{self, History}, swearmeter_questions::{self, Projection}};
use std::{path::{Path, PathBuf}, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}};
use tauri::{AppHandle, Emitter, Manager, State};

pub struct Service {
    app: AppHandle,
    reader: SpineReader,
    control: TurnControl,
    cards: Arc<Mutex<Projection>>,
    last: Mutex<Snapshot>,
    orphaned: AtomicBool,
    source_seen: AtomicBool,
    history: Mutex<Option<History>>,
}
impl Service {
    pub fn new(app: AppHandle, reader: SpineReader, control: TurnControl, cards: Projection, source_seen: bool) -> Self {
        let orphaned = reader.snapshot().ledger().swearmeter().composites().iter().any(|id| !cards.known_composites.contains(id));
        Self { app, reader, control, cards: Arc::new(Mutex::new(cards)), last: Mutex::new(Snapshot::default()),
            orphaned: AtomicBool::new(orphaned), source_seen: AtomicBool::new(source_seen), history: Mutex::new(None) }
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
        let mut dates = vec![messages.dates(), &pending_dates, &cards.dates];
        if let Some(history) = history.as_ref() { dates.push(&history.dates); }
        let mut next = Snapshot {
            detector_version: DETECTOR_VERSION,
            count: messages.count() + pending + cards.count + history.as_ref().map_or(0, |history| history.count),
            periods: Dates::report(&dates, richos_core::util::now_millis(), local_month),
            history_loading: history.is_none(),
            provider_history_incomplete: history.as_ref().is_some_and(|history| history.incomplete),
            ambiguous_matches: history.as_ref().map_or(0, |history| history.ambiguous_matches),
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

fn history_generation(claude: Option<&Path>, codex: Option<&Path>) -> Vec<std::io::Result<Option<richos_core::swearmeter_checkpoint::SourceStamp>>> {
    let files = [(claude, "history.jsonl"), (claude, "paste-cache"), (codex, "thread_history_1.sqlite"),
        (codex, "thread_history_1.sqlite-wal"), (codex, "sessions"), (codex, "archived_sessions")];
    files.into_iter().filter_map(|(root, name)| root.map(|root| richos_core::swearmeter_checkpoint::SourceStamp::at(&root.join(name)))).collect()
}

pub fn start(app: AppHandle) {
    let handle = app.clone();
    let worker = std::thread::Builder::new().name("richos-swearmeter-history".into()).spawn(move || {
        use notify::{Event, RecursiveMode, Watcher};
        use std::{sync::mpsc, time::{Duration, Instant}};

        let home = handle.path().home_dir().ok();
        let claude = home.as_ref().map(|home| configured_root("CLAUDE_CONFIG_DIR", home, ".claude"));
        let codex = home.as_ref().map(|home| configured_root("CODEX_HOME", home, ".codex"));
        let (changed, changes) = mpsc::sync_channel(1);
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            let relevant = event.is_err() || event.is_ok_and(|event| {
                if event.kind.is_access() { return false; }
                event.need_rescan() || event.paths.iter().filter_map(|path| path.file_name()?.to_str()).any(|name| {
                    matches!(name, "history.jsonl" | ".claude" | ".codex") || name.starts_with("thread_history_1.sqlite")
                })
            });
            if !relevant { return; }
            if changed.try_send(()).is_err() {
                // The pending notification already covers this change.
            }
        }).ok();
        let mut watched = std::collections::HashSet::new();
        let mut generation = None;
        loop {
            let mut watch_incomplete = watcher.is_none();
            if let Some(watcher) = watcher.as_mut() {
                for root in [claude.as_deref(), codex.as_deref()].into_iter().flatten() {
                    if !root.is_dir() || watched.contains(root) { continue; }
                    if watcher.watch(root, RecursiveMode::NonRecursive).is_ok() { watched.insert(root.to_owned()); }
                    else { watch_incomplete = true; }
                }
            }
            let service = &handle.state::<crate::AppState>().swearmeter;
            let current = history_generation(claude.as_deref(), codex.as_deref()).into_iter().collect::<std::io::Result<Vec<_>>>().ok();
            if current.is_none() || generation.as_ref() != Some(&current) {
                let started = Instant::now();
                let mut history = swearmeter_provider_history::read(claude.as_deref(), codex.as_deref());
                history.incomplete |= watch_incomplete || home.is_none();
                let mut previous = service.history.lock().unwrap_or_else(|poison| poison.into_inner());
                if previous.as_ref().is_none_or(|prior| prior.count != history.count || prior.incomplete != history.incomplete
                    || prior.ambiguous_matches != history.ambiguous_matches) {
                    eprintln!("[richos] SwearMeter historical sources: {} ({} ms)",
                        if history.incomplete { "partial" } else { "read" }, started.elapsed().as_millis());
                }
                *previous = Some(history);
                generation = Some(current);
            }
            service.publish_current();
            // Source changes are coalesced and never hold up a submission or a window.
            // The interval also notices a provider installed after RichOS started and
            // advances the time window when the user is idle.
            match changes.recv_timeout(Duration::from_secs(60)) {
                Ok(()) => {
                    std::thread::sleep(Duration::from_millis(500));
                    while changes.try_recv().is_ok() {}
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {},
                Err(mpsc::RecvTimeoutError::Disconnected) => std::thread::sleep(Duration::from_secs(60)),
            }
        }
    });
    if let Err(error) = worker {
        eprintln!("[richos] SwearMeter historical sources unavailable: {error}");
        let state = app.state::<crate::AppState>();
        *state.swearmeter.history.lock().unwrap() = Some(History { incomplete: true, ..History::default() });
        state.swearmeter.publish_current();
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

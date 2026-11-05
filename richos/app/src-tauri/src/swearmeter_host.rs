//! Local numeric state beside the turn mutex. Visibility never gates accounting.
use richos_core::{read_view::{SpineReader, SpineView}, steering::TurnControl,
    swearmeter::{Snapshot, DETECTOR_VERSION, EVENT_SWEARMETER}, swearmeter_questions::{self, Projection}};
use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
use tauri::{AppHandle, Emitter, State};

pub struct Service {
    app: AppHandle,
    reader: SpineReader,
    control: TurnControl,
    cards: Arc<Mutex<Projection>>,
    last: Mutex<Snapshot>,
    orphaned: AtomicBool,
    source_seen: AtomicBool,
}
impl Service {
    pub fn new(app: AppHandle, reader: SpineReader, control: TurnControl, cards: Projection, source_seen: bool) -> Self {
        let orphaned = reader.snapshot().ledger().swearmeter().composites().iter().any(|id| !cards.known_composites.contains(id));
        Self { app, reader, control, cards: Arc::new(Mutex::new(cards)), last: Mutex::new(Snapshot::default()),
            orphaned: AtomicBool::new(orphaned), source_seen: AtomicBool::new(source_seen) }
    }
    pub fn cards(&self) -> Arc<Mutex<Projection>> { self.cards.clone() }
    pub fn snapshot(&self) -> Snapshot {
        // Serialize reads before observing either source, including a concurrent event
        // and initial bridge read. An older observation can never gain a newer revision.
        let mut last = self.last.lock().unwrap_or_else(|poison| poison.into_inner());
        let (view, pending, pending_unknown, intake_complete) = self.control.swearmeter_inputs(&self.reader);
        let messages = view.ledger().swearmeter();
        let cards = swearmeter_questions::lock(&self.cards);
        let mut next = Snapshot {
            detector_version: DETECTOR_VERSION,
            count: messages.count() + pending + cards.count,
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
        drop(app.emit(EVENT_SWEARMETER, self.snapshot()));
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
    drop(app.emit(EVENT_SWEARMETER, &result));
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

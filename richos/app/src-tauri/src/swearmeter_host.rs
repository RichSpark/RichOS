//! Local numeric state beside the turn mutex. Visibility never gates accounting.
use richos_core::{read_view::{SpineReader, SpineView}, steering::TurnControl,
    swearmeter::{Snapshot, DETECTOR_VERSION, EVENT_SWEARMETER}, swearmeter_questions::Projection};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

pub struct Service {
    app: AppHandle,
    reader: SpineReader,
    control: TurnControl,
    cards: Arc<Mutex<Projection>>,
    last: Mutex<Snapshot>,
}
impl Service {
    pub fn new(app: AppHandle, reader: SpineReader, control: TurnControl, cards: Projection) -> Self {
        Self { app, reader, control, cards: Arc::new(Mutex::new(cards)), last: Mutex::new(Snapshot::default()) }
    }
    pub fn cards(&self) -> Arc<Mutex<Projection>> { self.cards.clone() }
    pub fn snapshot(&self) -> Snapshot {
        // Serialize reads before observing either source, including a concurrent event
        // and initial bridge read. An older observation can never gain a newer revision.
        let mut last = self.last.lock().unwrap();
        let (view, pending, pending_unknown, intake_complete) = self.control.swearmeter_inputs(&self.reader);
        let messages = view.ledger().swearmeter();
        let cards = self.cards.lock().unwrap();
        let mut next = Snapshot {
            detector_version: DETECTOR_VERSION,
            count: messages.count() + pending + cards.count,
            unknown_card_inputs: cards.unknown_inputs,
            unknown_message_inputs: messages.unknown_inputs() + pending_unknown,
            incomplete_history: !view.ledger().history_health().is_clean() || !intake_complete
                || messages.composites().iter().any(|id| !cards.known_composites.contains(id)),
            available: cards.available,
            revision: last.revision,
        };
        if *last != next { next.revision += 1; *last = next; }
        last.clone()
    }
    pub fn unavailable(&self) {
        self.cards.lock().unwrap().available = false;
        self.publish_current();
    }
    pub fn publish_current(&self) { self.publish(&self.app); }
    pub fn publish(&self, app: &AppHandle) {
        drop(app.emit(EVENT_SWEARMETER, self.snapshot()));
    }
}

#[derive(serde::Serialize)]
pub struct View { show: bool, result: Snapshot }

#[tauri::command]
pub fn get_swearmeter(state: State<crate::AppState>) -> View {
    View { show: state.config.lock().unwrap().show_swearmeter(), result: state.swearmeter.snapshot() }
}

#[tauri::command]
pub fn set_swearmeter_visible(state: State<crate::AppState>, show: bool) -> Result<(), String> {
    state.config.lock().unwrap().set_show_swearmeter(show).map_err(|e| e.to_string())
}

/// Explicit recovery from an unreadable source. A visibility click never calls this.
#[richos_command::command]
pub fn retry_swearmeter(app: AppHandle, state: State<crate::AppState>) -> Result<Snapshot, String> {
    {
        // Serialize recovery with committed-operation callbacks. A write concurrent
        // with this read is either included here or applied after this lock releases.
        let mut cards = state.swearmeter.cards.lock().unwrap();
        *cards = richos_core::questions::Store::new(state.data_dir.join("engine-state"))
            .swearmeter_projection()?;
    }
    let result = state.swearmeter.snapshot();
    drop(app.emit(EVENT_SWEARMETER, &result));
    Ok(result)
}

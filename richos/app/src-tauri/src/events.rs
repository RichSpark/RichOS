//! The ADDITIVE live-work event relay (UX brief §13, slice 3 of §24).
//!
//! A separate module rather than another block at the bottom of `main.rs`, for two
//! reasons: `main.rs` should not grow without bound, and slice 4's Tauri query commands
//! are being appended to that same file on a parallel branch — a new module turns a
//! certain conflict into a one-line one.
//!
//! ## What this layer is allowed to do
//!
//! **Relay a name and a payload.** It builds no payload, resolves no scope and
//! makes no visibility decision. Optional timing logs observe event identities only.
//! Payload construction and visibility policy live in
//! `richos_core::live`, where it is unit-tested without a webview; if this file could
//! construct an event, the ECS fence and the visibility gate would both be one careless
//! edit away from being bypassed. `LiveEvent`'s fence is not even constructible from here
//! — `EventFence::for_turn` is crate-private to richos-core and takes a `ThreadBinding`
//! only the ledger can issue.
//!
//! ## Why a THIRD emitter beside `TauriEmitter` and `TauriMachineryEmitter`
//!
//! Three observers means three subscription lists, and a UI's subscription list is then
//! the PROOF of what it can render rather than a promise about what it chooses to:
//!
//!   - `rich://turn-started` / `chunk` / `turn-completed` / `turn-error` — the shipping
//!     calm view. **Completely unchanged by this slice** (`app/STREAMING.md`).
//!   - `rich://machinery` — the technical family, which the calm view does not subscribe
//!     to and must not.
//!   - the seven §13 events below — the typed calm family: turn status, message phase,
//!     semantic activity, DELEGATED WORKERS, thread summary. Only `Visibility::Ceo` items are handed to
//!     it, and the spine's `forward_live` chokepoint is what enforces that, before this
//!     file ever sees an event.
//!
//! A webview subscribing to nothing new keeps working exactly as it does today.

use richos_core::live::{LiveEvent, LiveObserver};
use tauri::{AppHandle, Emitter, Manager};

/// Forwards each additive §13 event to the webview under its own event name.
///
/// Same shape and same posture as `TauriEmitter`: best-effort, infallible from the
/// spine's view. §13's ordering rules say *"missed UI events never block the spine"* and
/// *"the ledger and typed task graph remain authoritative"* — so a dropped or absent
/// webview can never stall or fail a turn, and the `let _ =` is deliberate.
pub struct TauriLiveEmitter {
    pub app: AppHandle,
}

impl LiveObserver for TauriLiveEmitter {
    fn on_live_event(&self, event: &LiveEvent) {
        let payload = event.payload();
        // Opt-in timing evidence carries identities and a clock, never message content.
        // An emitted text event and text rendered by the window are separate milestones.
        if crate::hop_trace_is_on() && matches!(event.event_name(),
            "rich://message-delta" | "rich://ceo-message" | "rich://turn-status") {
            let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis()).unwrap_or(0);
            eprintln!("[richos] event-timing at={at} event={} thread={} turn={}",
                event.event_name(), payload["threadId"], payload["turnId"]);
        }
        let _ = self.app.emit(event.event_name(), payload);
        on_human_acceptance(event.event_name(), || {
            if let Some(state) = self.app.try_state::<crate::AppState>() { state.swearmeter.publish_current(); }
        });
    }
}

// This callback runs after the published read view has folded durable acceptance.
fn on_human_acceptance(name: &str, publish: impl FnOnce()) {
    if name == "rich://ceo-message" { publish(); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use richos_core::{cognition::MockLeaseFactory, entity::{Entity, EntityId, EntityRegistry},
        ledger::{Ledger, Source}, read_view::{SpineReader, SpineView}, spine::Spine};
    use std::sync::{Arc, Mutex};
    struct Witness { reader: SpineReader, counts: Arc<Mutex<Vec<u64>>> }
    impl LiveObserver for Witness {
        fn on_live_event(&self, event: &LiveEvent) {
            on_human_acceptance(event.event_name(), || {
                let view = self.reader.snapshot();
                assert!(view.ledger().turns().last().unwrap().assistant_text.is_empty(), "count is visible before the reply");
                self.counts.lock().unwrap().push(view.ledger().swearmeter().count());
            });
        }
    }
    #[test]
    fn a_real_accepted_message_publishes_its_count_before_the_reply_without_reply_increments() {
        let path = std::env::temp_dir().join(format!("swearmeter-event-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let mut spine = Spine::new(Ledger::open(&path).unwrap());
        spine.set_entity_registry(EntityRegistry::new(vec![Entity::new("fixture", "Fixture", &["/fixture"]).unwrap()]).unwrap());
        spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec!["Ready", "shit reply"])));
        let thread = spine.create_thread("Fixture", &EntityId::parse("fixture").unwrap()).unwrap();
        spine.switch_thread(&thread).unwrap();
        let counts = Arc::new(Mutex::new(Vec::new()));
        let reader = spine.reader();
        spine.set_live_observer(Box::new(Witness { reader, counts: counts.clone() }));
        spine.submit_prompt("damn", Source::Text).unwrap();
        assert_eq!(*counts.lock().unwrap(), vec![1]);
        drop(spine);
        std::fs::remove_file(path).unwrap();
    }
}

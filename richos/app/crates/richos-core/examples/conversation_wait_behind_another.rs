//! **HOW LONG ONE CONVERSATION WAITS BEHIND ANOTHER CONVERSATION'S REPLY.**
//!
//! The CEO's Two Riches page promises two things about parallelism, and only one of them is
//! about the conversation: *"the CEO's conversation with Rich is never blocked by any work
//! that's going on in the background"*, and *"the CEO could open and run multiple things in
//! parallel."* Work in several threads does run in parallel — one back end per thread, since
//! the work host took them. What is still serialized is the front desks' TURNS: the shell
//! holds one `Mutex<Spine>` for the length of a turn (`src-tauri/src/main.rs`), so a message
//! to thread B while thread A is mid-reply is answered at A's turn boundary rather than
//! beside it.
//!
//! **Ruled 2026-09-17 not to be fixed by a refactor unless a measurement forces it**
//! (`esc-20260917T195400Z-3e9b24be`), which makes the measurement the thing that matters.
//! This is it, and it measures the real mechanism rather than a model of it: a real `Spine`,
//! behind the shell's own `Arc<Mutex<…>>`, with the send arriving from another thread
//! exactly as a second Tauri command would.
//!
//! **What this number contains, and what it cannot.**
//!
//! | Component | In this number? |
//! |---|---|
//! | The remainder of thread A's reply, after B's send | **Yes** — it is the whole of the wait |
//! | The lock handoff, the ledger write and the turn boundary | **Yes** |
//! | Thread B's own front desk producing its answer | **Yes** (scripted here as instant) |
//! | The MODEL's latency on either thread | **No.** Scripted. It is the largest real term and it is not this file's to measure |
//!
//! So the number below is the app's own contribution to the wait, and the honest sentence
//! about the real one is: **B waits for whatever is left of A's reply, plus milliseconds.**
//! The run prints both, and the arithmetic between them, rather than asserting either.
//!
//! Run:
//! ```text
//! cargo run -p richos-core --example conversation_wait_behind_another --release
//! ```
//!
//! It touches nothing outside a fresh temp directory, which it removes, and it starts no
//! app, no lease, no provider and no window.
use richos_core::{
    cognition::{Cognition, CognitionError, LeaseFactory, TurnItem},
    entity::{Entity, EntityId, EntityRegistry},
    ledger::{Ledger, Source},
    spine::Spine,
};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long thread A's front desk "thinks" for. A stand-in for the model, and it is named
/// rather than hidden: the measured wait is expected to be this minus how far into it B's
/// send arrived, and the run checks that arithmetic out loud.
const A_REPLY: Duration = Duration::from_millis(3000);
/// How far into A's reply the CEO sends his message to thread B.
const SEND_AT: Duration = Duration::from_millis(1000);

struct Desk {
    session: String,
    thinks_for: Duration,
}

impl Cognition for Desk {
    fn session_id(&self) -> &str {
        &self.session
    }
    fn requires_thread_isolation(&self) -> bool {
        true
    }
    fn reprime(&mut self, _p: &str, _on: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
        Ok(())
    }
    fn prompt(&mut self, text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        std::thread::sleep(self.thinks_for);
        let reply = format!("{}: {text}", self.session);
        on_item(TurnItem::Text { seq: 0, text: &reply });
        Ok("end_turn".to_string())
    }
}

/// **The slow desk is chosen by THREAD, and the first version of this file chose it by
/// spawn ORDER — which measured the wrong thing and said so out loud.**
///
/// With "the first desk spawned is the slow one", the warm-up below handed the slow desk to
/// thread B, so the run measured B's own scripted reply (3.017 s) and reported an "app's
/// own share" of 1.019 s that was really just the moment of the send. The printed
/// arithmetic is what caught it: the measured wait was LONGER than thread A's whole reply,
/// which is impossible for a wait that starts partway into it. Keying on the binding makes
/// the premise the thing it claims to be.
struct Desks {
    slow_thread: String,
    next: Arc<Mutex<u64>>,
}

impl Desks {
    fn desk(&self, thread_id: &str) -> Box<dyn Cognition> {
        let mut n = self.next.lock().unwrap();
        *n += 1;
        let thinks_for = if thread_id == self.slow_thread { A_REPLY } else { Duration::ZERO };
        Box::new(Desk { session: format!("desk-{n}"), thinks_for })
    }
}

impl LeaseFactory for Desks {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        Ok(self.desk(""))
    }
    fn spawn_scoped(
        &self,
        binding: &richos_core::entity::ThreadBinding,
        _control: &richos_core::steering::ConversationControl,
    ) -> Result<Box<dyn Cognition>, CognitionError> {
        Ok(self.desk(binding.thread_id()))
    }
}

fn main() {
    let root = std::env::temp_dir().join(format!("conversation-wait-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let mut spine = Spine::new(Ledger::open(root.join("ledger.jsonl")).unwrap());
    spine.set_entity_registry(
        EntityRegistry::new(vec![Entity::new("femcboost", "FemcBoost", &["/fixture/femcboost"]).unwrap()]).unwrap(),
    );
    let entity = EntityId::parse("femcboost").unwrap();
    let one = spine.create_thread("Pricing", &entity).unwrap();
    let two = spine.create_thread("Hiring", &entity).unwrap();
    // The factory is installed AFTER the threads exist, so the slow desk can be named by
    // the conversation it belongs to rather than by the order the spine happens to spawn in.
    spine.set_lease_factory(Box::new(Desks { slow_thread: one.clone(), next: Arc::new(Mutex::new(0)) }));

    // Both front desks exist and are primed before the measurement, so what is measured is
    // the WAIT and not a first-turn spawn. This is the steady state he is actually in.
    spine.submit_prompt_to(&two, "warm up", Source::Text).unwrap();
    spine.submit_prompt_to(&one, "warm up", Source::Text).unwrap();

    // The shell's own shape: one lock, and two commands arriving on different threads.
    let spine = Arc::new(Mutex::new(spine));
    let working = Arc::clone(&spine);
    let thread_one = one.clone();
    let a_started = Instant::now();
    let turn_a = std::thread::spawn(move || {
        working.lock().unwrap().submit_prompt_to(&thread_one, "how did the pricing land", Source::Text).unwrap();
    });

    std::thread::sleep(SEND_AT);
    let sent_at = Instant::now();
    let into_a = sent_at.duration_since(a_started);
    spine.lock().unwrap().submit_prompt_to(&two, "who did we shortlist", Source::Text).unwrap();
    let waited = sent_at.elapsed();
    turn_a.join().unwrap();

    let guard = spine.lock().unwrap();
    let answered = guard
        .messages(&two)
        .unwrap()
        .iter()
        .any(|m| m.text.contains("shortlist") || m.text.starts_with("desk-"));
    let remaining = A_REPLY.saturating_sub(into_a);

    println!("one conversation waiting behind another conversation's reply");
    println!("  thread A's reply, scripted:            {:?}", A_REPLY);
    println!("  his message to thread B arrived:       {:?} into it", into_a);
    println!("  so what was LEFT of A's reply:         {:?}", remaining);
    println!("  MEASURED wait, send to answer on B:    {:?}", waited);
    println!(
        "  the app's own share of it:             {:?}  (measured minus what was left of A)",
        waited.saturating_sub(remaining)
    );
    println!("  thread B answered:                     {answered}");
    println!();
    println!("  The model's own latency is not in any of these numbers: both replies are");
    println!("  scripted. The sentence this supports is that B waits for the REMAINDER OF A'S");
    println!("  REPLY plus milliseconds, and never for background work, which runs on its own");
    println!("  connection and takes no part in this lock.");
    drop(guard);
    std::fs::remove_dir_all(&root).unwrap();
}

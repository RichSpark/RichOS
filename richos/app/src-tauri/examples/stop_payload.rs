//! What a CEO STOP actually puts on the wire — from a real running turn, a real stop
//! pressed from another thread, the real ledger, and the real `get_timeline` body.
//!
//! ## Why this exists
//!
//! `screencapture` on this machine has returned an all-black PNG for four slices running
//! (the display is locked), so a screenshot of the app proves nothing. The precedent slice
//! 5 set instead: print the payload from the ACTUAL chain, then render THOSE EXACT BYTES
//! with the real renderer under WebKit (`app/ui/tests/steering.js`). Between the two, the
//! claim "the CEO sees `You stopped after 1s`" is backed end to end without a display.
//!
//! The chain here is: `Spine` behind an `Arc<Mutex<..>>` exactly as the Tauri shell holds
//! it -> a turn running on one thread and holding that lock -> `ConversationControl::request_stop`
//! from ANOTHER thread (the same call `stop_turn` makes) -> `Ledger` -> `Spine::timeline`
//! -> `Timeline::view(ViewMode::Ceo)` -> `timeline_view::timeline_payload`, which is the
//! same file `get_timeline` calls. Nothing here re-implements a command.
//!
//! ## What it does NOT prove
//!
//! There is no live lease — no `claude` child is spawned — so the lease is
//! `CancellableMockCognition`, whose cancel seam sets a flag instead of writing a
//! `control_request{interrupt}` to a child. The wire half is proven separately, against a
//! real child process over real stdio, in
//! `crates/richos-core/tests/native_cancel_tests.rs`.
//!
//! What is REAL here: the spine lock genuinely held for the whole turn, the stop taken
//! without it, every durable write, the terminal state, the measured duration, the
//! visibility gate, the item ids and the JSON encoding.
//!
//! Run:
//!   cargo run --example stop_payload            # from app/src-tauri
//!   cargo run --example stop_payload -- --json  # payload only, for piping into the browser test

#[path = "../src/timeline_view.rs"]
mod timeline_view;

use richos_core::cognition::CancellableMockCognition;
use richos_core::entity::{Entity, EntityId, EntityRegistry};
use richos_core::ledger::{Ledger, Source, TurnState};
use richos_core::spine::Spine;
use richos_core::steering::{StopOutcome, TurnControl};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use timeline_view::timeline_payload;

fn main() {
    let json_only = std::env::args().any(|a| a == "--json");
    let dir = std::env::temp_dir().join(format!("richos-stop-payload-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");

    let ledger = Ledger::open(dir.join("ledger.jsonl")).expect("open ledger");
    let mut spine = Spine::new(ledger);
    spine.set_entity_registry(EntityRegistry::new(vec![
        Entity::new("richos", "RichOS", &["/fixture/richos"]).expect("entity"),
    ]).expect("entity registry"));
    let entity = EntityId::parse("richos").expect("entity");
    let thread = spine.create_thread("Stop payload proof", &entity).expect("thread");
    spine.switch_thread(&thread).expect("switch");

    // 120 chunks x 25ms = 3.000s of scripted turn. The stop lands about 1s in, so roughly
    // 40 of the 120 chunks should have arrived — enough that "partial output survives" is
    // visible in the payload rather than merely asserted.
    let script: Vec<String> = (0..120).map(|i| format!("part {i}. ")).collect();
    spine.attach_lease(Box::new(CancellableMockCognition::new(
        "sess_stop_1",
        script.iter().map(|s| s.as_str()).collect(),
        Duration::from_millis(25),
    )));
    let control = TurnControl::open(dir.join("intake.jsonl")).expect("intake").for_thread(&thread);
    spine.set_turn_control(control.clone());

    let spine = Arc::new(Mutex::new(spine));
    let runner = {
        let spine = Arc::clone(&spine);
        std::thread::spawn(move || {
            spine
                .lock()
                .unwrap()
                .submit_prompt("draft the whole memory strategy", Source::Text)
                .expect("turn")
        })
    };

    // Wait for the turn to be genuinely running — read from the control's mirror of
    // `turn_in_progress`, never from a sleep-and-hope.
    let began = std::time::Instant::now();
    while control.active_turn().is_none() {
        assert!(began.elapsed() < Duration::from_secs(5), "the turn never started");
        std::thread::sleep(Duration::from_millis(2));
    }
    std::thread::sleep(Duration::from_millis(1_000));

    // THE THING BEING PROVEN. The turn owns the spine lock right now.
    let lock_held = spine.try_lock().is_err();
    let outcome = control.request_stop().expect("stop");
    let turn_id = runner.join().expect("join");

    let guard = spine.lock().unwrap();
    let turn = guard.ledger().turn(&turn_id).expect("turn").clone();
    let payload = timeline_payload(&*guard, &thread).expect("payload");
    drop(guard);

    if json_only {
        println!("{}", serde_json::to_string_pretty(&payload).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }

    eprintln!("[stop] ledger  = {}", dir.join("ledger.jsonl").display());
    eprintln!("[stop] intake  = {}", dir.join("intake.jsonl").display());
    eprintln!("[stop] thread  = {thread}");
    eprintln!("[stop] turn    = {turn_id}");
    eprintln!("[stop] outcome = {outcome:?}");
    eprintln!();

    let items = payload["items"].as_array().cloned().unwrap_or_default();
    eprintln!("[stop] mode = {}  items = {}", payload["mode"], items.len());
    for it in &items {
        let kind = it["kind"].as_str().unwrap_or("?");
        let extra = match kind {
            "user_message" => format!("text={:?}", it["text"].as_str().unwrap_or("")),
            "rich_message" => {
                let t = it["text"].as_str().unwrap_or("");
                let shown: String = t.chars().take(46).collect();
                format!("chars={} text={:?}…", t.chars().count(), shown)
            }
            "work_duration" => format!(
                "state={} startedAt={} activeMs={} detail={}",
                it["state"], it["startedAt"], it["activeMs"], it["detail"]
            ),
            _ => String::new(),
        };
        eprintln!("  {:<14} slot={:<9} id={:<28} {}", kind, it["slot"].as_str().unwrap_or("?"), it["id"].as_str().unwrap_or("?"), extra);
    }

    eprintln!("\n[stop] --- the assertions the renderer depends on ---");
    let mut ok = true;
    let mut say = |label: &str, cond: bool, detail: String| {
        if !cond {
            ok = false;
        }
        eprintln!("  {} {:<56} {}", if cond { "PASS" } else { "FAIL" }, label, detail);
    };

    say(
        "the spine lock was held for the whole turn",
        lock_held,
        "try_lock failed while the turn ran — a stop routed through it could not have fired".into(),
    );
    say(
        "the stop reached the lease",
        matches!(outcome, StopOutcome::Requested { reached_lease: true, .. }),
        format!("{outcome:?}"),
    );
    say("the ledger records a CEO stop", turn.state == TurnState::Stopped, format!("state={:?}", turn.state));
    say(
        "it is NOT recorded as interrupted",
        turn.state != TurnState::Interrupted,
        "a crash and a CEO stop are different terminal states".into(),
    );
    say(
        "the stop request time is recorded",
        turn.stop_requested_at.is_some(),
        format!("stop_requested_at={:?}", turn.stop_requested_at),
    );
    say(
        "the duration is MEASURED, not estimated",
        turn.active_ms().is_some(),
        format!("active_ms={:?} (ended_at - started_at)", turn.active_ms()),
    );
    let delivered = turn.assistant_text.matches("part ").count();
    say(
        "partial assistant output survived the stop (§9.3 step 4)",
        delivered > 0 && delivered < 120,
        format!("{delivered} of 120 scripted chunks are durable"),
    );

    let duration_state = items
        .iter()
        .find(|i| i["kind"] == "work_duration")
        .map(|i| i["state"].as_str().unwrap_or("?").to_string())
        .unwrap_or_default();
    say(
        "the CEO view carries work_duration state=\"stopped\"",
        duration_state == "stopped",
        format!("state={duration_state:?} — the only value app/ui/timeline.js may render \"You stopped after\" from"),
    );
    let has_error_row = items.iter().any(|i| i["kind"] == "system_error");
    say(
        "no system_error row: a stop is not a failure",
        !has_error_row,
        "§5.5's failure treatment belongs to failures".into(),
    );
    let active_ms = items.iter().find(|i| i["kind"] == "work_duration").map(|i| i["activeMs"].clone());
    say(
        "activeMs is a number on the wire, not null",
        active_ms.as_ref().map(|v| v.is_number()).unwrap_or(false),
        format!("activeMs={:?}", active_ms.unwrap_or(serde_json::Value::Null)),
    );

    let _ = std::fs::remove_dir_all(&dir);
    eprintln!();
    if ok {
        eprintln!("[stop] all assertions PASSED");
    } else {
        eprintln!("[stop] FAILURES above");
        std::process::exit(1);
    }
}

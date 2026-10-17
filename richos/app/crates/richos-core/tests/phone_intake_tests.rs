//! **THE PHONE'S ROAD IN** — the intake record a channel writes, and the idle drain that
//! is the whole difference between a message landing and a message appearing to vanish.
//!
//! Phone-client plan (`richos-hq/docs/plans/richos-phone-client-2026-09-18.md`) §4.2,
//! findings (i), (ii) and (vii):
//!
//! > `drain_intake()` is called from exactly two places: `after_turn_boundary()` and
//! > `reconcile_intake()` at boot. **Nothing drains the intake log while the app sits
//! > idle.** … He is standing in the kitchen watching his phone. **`poll_intake()` is not a
//! > half-day nicety any more; it is the feature.**
//!
//! # Every test here has a positive control
//!
//! A negative test that passes for the wrong reason is worse than no test. So each "it does
//! not happen" below is paired with the same setup in which it DOES happen, and the pair is
//! asserted in one test rather than in two that can drift apart. The one that matters most
//! is [`the_idle_drain_is_the_only_thing_that_makes_an_idle_message_land`]: without
//! `poll_intake` the record sits in the log and the ledger is empty, and with it the turn
//! runs — same fixture, same record, one call apart.
//!
//! Headless throughout: no live Claude, no network, no Tauri, no socket.

use richos_core::cognition::MockLeaseFactory;
use richos_core::entity::EntityId;
use richos_core::ledger::{Ledger, Source};
use richos_core::live::{LiveEvent, LiveObserver};
use richos_core::spine::Spine;
use richos_core::steering::{IntakeLog, IntakeRecord, SteeringError, TurnControl, ConversationControl};
use serde_json::Value;
use std::sync::{Arc, Mutex};

mod support;

fn femcboost() -> EntityId {
    EntityId::parse("femcboost").unwrap()
}

fn tmp_path(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "richos-phone-intake-{tag}-{}-{}.jsonl",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    let _ = std::fs::remove_file(&p);
    p
}

/// A spine with a durable intake log and a lease factory, on one thread, ready to be
/// spoken to from somewhere that is not the desktop window.
fn phone_ready(tag: &str, replies: Vec<&'static str>) -> (Spine, ConversationControl, String, std::path::PathBuf) {
    let ledger_path = tmp_path(&format!("{tag}-ledger"));
    let intake_path = tmp_path(&format!("{tag}-intake"));
    let mut spine = support::spine(Ledger::open(&ledger_path).unwrap());
    let thread = spine.create_thread("the proposal", &femcboost()).unwrap();
    spine.switch_thread(&thread).unwrap();
    spine.set_lease_factory(Box::new(MockLeaseFactory::new(replies)));
    let control = TurnControl::open(&intake_path).unwrap();
    spine.set_turn_control(control.clone());
    let control=control.for_thread(&thread);
    (spine, control, thread, ledger_path)
}

/// The turns the CEO can actually see — the internal priming turn has no render path
/// anywhere, so a test about the conversation filters it out the same way the UI does.
fn ceo_texts(spine: &Spine, thread: &str) -> Vec<String> {
    let binding = spine.ledger().thread_binding(thread).unwrap();
    spine
        .ledger()
        .thread_turns_scoped(&binding)
        .unwrap()
        .into_iter()
        .filter(|t| t.source == Source::Text)
        .map(|t| t.user_text.clone())
        .collect()
}

// -------------------------------------------------------------------------------------
// THE RECORD
// -------------------------------------------------------------------------------------

#[test]
fn a_channel_message_is_durable_before_the_caller_is_answered() {
    // §4.2 (i): "It is on disk and `fsync`ed before anything acts on it." The observable
    // form of that claim is that the bytes are readable by a SECOND reader opened after
    // the write returned — not that a flag was set.
    let path = tmp_path("durable");
    let control = TurnControl::open(&path).unwrap();
    let record = control.submit_from_channel("thr_1", Some(femcboost()), "where are we?", "phone").unwrap();
    let id = record.id();

    let reopened = IntakeLog::open(&path).unwrap();
    assert!(reopened.health().is_clean(), "{:?}", reopened.skipped_records());
    let pending = reopened.pending();
    assert_eq!(pending.len(), 1, "the record was not on disk when the write returned");
    match &pending[0] {
        IntakeRecord::Channel { id: got, thread_id, text, channel, .. } => {
            assert_eq!(*got, id);
            assert_eq!(thread_id, "thr_1");
            assert_eq!(text, "where are we?");
            assert_eq!(channel, "phone");
        }
        other => panic!("wrong record type: {other:?}"),
    }
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_channel_message_does_not_need_a_running_turn_and_steering_still_does() {
    // THE PAIR. `steer` refuses when nothing is running, and that refusal is correct —
    // "add this to what Rich is doing" is meaningless when Rich is doing nothing. The
    // phone has no such fork, which is why it is a second entry point rather than a
    // relaxed flag on the first.
    let path = tmp_path("no-turn");
    let control = TurnControl::open(&path).unwrap();

    assert!(
        matches!(control.for_thread("thr_1").steer("added while working"), Err(SteeringError::NoActiveTurn)),
        "steering with nothing running must still refuse"
    );
    assert!(
        control.submit_from_channel("thr_1", None, "sent from the couch", "phone").is_ok(),
        "a phone message with nothing running must be accepted"
    );
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_channel_record_carries_no_steering_turn_id_because_there_is_no_turn_to_attribute_it_to() {
    // The reason this is a separate variant rather than a `Steer` with a made-up field.
    // §6.1's "You stopped after {duration}" is an ATTRIBUTION and this file's whole
    // posture is that an attribution needs evidence; a fabricated turn id is evidence of
    // nothing. The serialized form is asserted because it is what an older build reads.
    let record = IntakeRecord::Channel {
        id: 7,
        thread_id: "thr_1".into(),
        entity_id: None,
        text: "hello".into(),
        at: 1,
        channel: "phone".into(),
        client_id: None,
        voice_note: false,
        source: None,
    };
    let json = serde_json::to_value(&record).unwrap();
    assert_eq!(json.get("record").unwrap().as_str().unwrap(), "channel");
    assert!(json.get("steering_turn_id").is_none(), "a channel record must not carry a turn id");
    assert_eq!(json.get("channel").unwrap().as_str().unwrap(), "phone");
    // A record without the phone's id is byte-for-byte the shape every older build reads.
    assert!(json.get("client_id").is_none(), "no id, no key");
    // And a typed one carries no voice-note key, for the same reason.
    assert!(json.get("voice_note").is_none(), "typed, no key");
}

#[test]
fn an_older_build_counts_a_channel_record_rather_than_dropping_it() {
    // The cost of a new tag, paid where it can be seen. A build that predates `channel`
    // cannot fold the record — but `IntakeLog::open` classifies it, COUNTS it, salvages
    // its id so the counter cannot hand the same id out twice, and reports it. The
    // simulation is a record with a tag no build knows, which is exactly what `channel`
    // looks like to a build that does not have it.
    let path = tmp_path("from-future");
    std::fs::write(
        &path,
        "{\"record\":\"channel\",\"id\":1,\"thread_id\":\"t\",\"text\":\"x\",\"at\":1,\"channel\":\"phone\"}\n\
         {\"record\":\"a_tag_no_build_has\",\"id\":2,\"text\":\"his words\"}\n",
    )
    .unwrap();
    let log = IntakeLog::open(&path).unwrap();
    let health = log.health();
    assert_eq!(health.records_read, 2);
    // THIS build reads the channel record; the unknown one is counted, not dropped.
    assert_eq!(health.records_applied, 1);
    assert_eq!(health.skipped, 1, "the unreadable record was not counted");
    assert!(!health.is_clean());
    // The id counter cleared the salvaged id, so the next write cannot collide with it.
    let mut log = log;
    let next = log.channel_message("t", None, "the next one", "phone").unwrap();
    assert!(next.id() > 2, "the id counter collided with a record it could not read: {}", next.id());
    let _ = std::fs::remove_file(path);
}

// -------------------------------------------------------------------------------------
// THE IDLE DRAIN
// -------------------------------------------------------------------------------------

#[test]
fn the_idle_drain_is_the_only_thing_that_makes_an_idle_message_land() {
    // §4.2 (ii) and (vii), with its own positive control in the same test. FIRST HALF: the
    // record is written to an idle app and nothing else is called. It stays in the log and
    // the ledger stays empty — this is the gap the plan measured, reproduced.
    let (mut spine, control, thread, ledger_path) = phone_ready("idle", vec!["On it! Here is where we are."]);
    control.submit_from_channel(&thread, Some(femcboost()), "where are we on the proposal?", "phone").unwrap();

    assert!(!spine.is_turn_in_progress(), "nothing should be running");
    assert_eq!(ceo_texts(&spine, &thread).len(), 0, "a message reached the ledger with no drain");
    assert_eq!(control.pending_intake().len(), 1, "the record left the log with no drain");

    // SECOND HALF: the one new entry point, and the same message lands.
    spine.poll_intake().unwrap();

    assert_eq!(
        ceo_texts(&spine, &thread),
        vec!["where are we on the proposal?".to_string()],
        "poll_intake did not turn the record into a turn"
    );
    assert!(control.pending_intake().is_empty(), "the record was not marked drained");
    assert!(!spine.is_turn_in_progress(), "the turn should have completed inside the call");
    let _ = std::fs::remove_file(ledger_path);
}

#[test]
fn the_idle_drain_is_idempotent_so_a_retried_post_cannot_ask_twice() {
    // The de-duplication key is the intake id and the proof is `turn_for_intake`. Calling
    // the drain again must not file the CEO's one message as a second turn — the record's
    // own doc calls that "the CEO's one message, asked twice".
    let (mut spine, control, thread, ledger_path) = phone_ready("twice", vec!["first", "second"]);
    control.submit_from_channel(&thread, Some(femcboost()), "one message", "phone").unwrap();
    spine.poll_intake().unwrap();
    spine.poll_intake().unwrap();
    spine.poll_intake().unwrap();
    assert_eq!(ceo_texts(&spine, &thread), vec!["one message".to_string()]);
    let _ = std::fs::remove_file(ledger_path);
}

#[test]
fn the_idle_drain_on_an_empty_log_does_nothing_at_all() {
    // Called after every write, so it is called often and must be free when there is
    // nothing to do. No turn, no ledger entry, no error.
    let (mut spine, _control, thread, ledger_path) = phone_ready("empty", vec!["unused"]);
    spine.poll_intake().unwrap();
    assert_eq!(ceo_texts(&spine, &thread).len(), 0);
    assert!(!spine.is_turn_in_progress());
    let _ = std::fs::remove_file(ledger_path);
}

#[test]
fn the_idle_drain_refuses_to_deliver_while_a_turn_is_running() {
    // Continuity §3.1 is queue-not-interrupt BY CONSTRUCTION, and this is the second lock
    // on that door. In the shipping app a caller cannot even reach the spine mid-turn —
    // `submit_prompt` holds the one mutex for the whole turn — so this drives the flag
    // directly, which is the only way to reach the branch at all.
    let (mut spine, control, thread, ledger_path) = phone_ready("mid-turn", vec!["not yet"]);
    control.submit_from_channel(&thread, Some(femcboost()), "sent mid-turn", "phone").unwrap();

    spine.debug_set_turn_in_progress(true);
    spine.poll_intake().unwrap();
    assert_eq!(ceo_texts(&spine, &thread).len(), 0, "poll_intake delivered into a running turn");
    assert_eq!(control.pending_intake().len(), 1, "the record was consumed mid-turn");

    // POSITIVE CONTROL: the identical call, at the boundary, lands it.
    spine.debug_set_turn_in_progress(false);
    spine.poll_intake().unwrap();
    assert_eq!(ceo_texts(&spine, &thread), vec!["sent mid-turn".to_string()]);
    let _ = std::fs::remove_file(ledger_path);
}

#[test]
fn a_phone_message_and_a_desktop_message_are_the_same_kind_of_turn() {
    // §4.2 (iv): "The back end never learns which mouth the CEO used." So the two turns
    // must be indistinguishable in the ledger except for their text — same `Source`, same
    // thread, same shape. A difference here would be a channel leaking into the record.
    let (mut spine, control, thread, ledger_path) = phone_ready("same", vec!["a", "b"]);
    spine.submit_prompt("typed at the desk", Source::Text).unwrap();
    control.submit_from_channel(&thread, Some(femcboost()), "sent from the phone", "phone").unwrap();
    spine.poll_intake().unwrap();

    let binding = spine.ledger().thread_binding(&thread).unwrap();
    let turns: Vec<_> = spine
        .ledger()
        .thread_turns_scoped(&binding)
        .unwrap()
        .into_iter()
        .filter(|t| t.source == Source::Text)
        .collect();
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0].source, turns[1].source, "the channel changed the turn's source");
    assert_eq!(turns[0].thread_id, turns[1].thread_id);
    // Only the phone turn carries an intake id, because only it came through the log.
    assert_eq!(turns[0].intake_id, None);
    assert!(turns[1].intake_id.is_some());
    let _ = std::fs::remove_file(ledger_path);
}

#[test]
fn a_message_for_a_thread_that_does_not_exist_stops_the_drain_rather_than_vanishing() {
    // `drain_intake`'s deliberate trade, inherited rather than re-decided: a record that
    // cannot become a turn is NOT marked drained and NOT discarded. It blocks everything
    // behind it and comes back at the next call. For the phone that means an unknown
    // thread id is an error the CEO's words survive, not a message that disappears.
    let (mut spine, control, _thread, ledger_path) = phone_ready("bad-thread", vec!["unused"]);
    control.submit_from_channel("thr_does_not_exist", Some(femcboost()), "his words", "phone").unwrap();
    assert!(spine.poll_intake().is_err(), "an unknown thread must be an error, not a silent drop");
    assert_eq!(control.pending_intake().len(), 1, "his words were discarded");
    let _ = std::fs::remove_file(ledger_path);
}

// -------------------------------------------------------------------------------------
// WHAT THE MAC LEARNS, AND WHEN — Ray's candidate .13 defect R3
// -------------------------------------------------------------------------------------

/// The wire, recorded as NAME + PAYLOAD: exactly what the Tauri shell forwards to a webview
/// and to the phone, never the Rust value. A test that asserted the enum could pass on a
/// field no renderer would ever see.
#[derive(Clone, Default)]
struct RecordingLive {
    events: Arc<Mutex<Vec<(String, Value)>>>,
}

impl RecordingLive {
    fn names(&self) -> Vec<String> {
        self.events.lock().unwrap().iter().map(|(n, _)| n.clone()).collect()
    }

    fn of(&self, name: &str) -> Vec<Value> {
        self.events.lock().unwrap().iter().filter(|(n, _)| n == name).map(|(_, p)| p.clone()).collect()
    }
}

impl LiveObserver for RecordingLive {
    fn on_live_event(&self, event: &LiveEvent) {
        self.events.lock().unwrap().push((event.event_name().to_string(), event.payload()));
    }
}

#[test]
fn his_phone_message_reaches_the_mac_with_his_words_in_it_and_not_only_a_status() {
    // **RAY'S R3, AS AN EVENT LIST.** He typed on his phone and the Mac showed nothing for
    // 5.68 s — measured twice, 5.77 s and 5.68 s — and then his words and Rich's finished
    // answer arrived in the same frame. At 2.99 s the only thing that had changed was the
    // sidebar's working dot.
    //
    // That dot is `rich://thread-summary-updated`, and it is the whole diagnosis: the
    // channel's drain was emitting the turn's STATUS and the sidebar's ROW on time, and
    // not the one event in the family that carries what he SAID. The Mac's window had
    // nothing to draw with, because until a second surface existed it had always drawn his
    // sentence itself the moment he pressed Send.
    //
    // So the assertion is not "an event was emitted" but WHICH ONE, and that his text is
    // in it, and that it is on the wire before Rich starts answering.
    let (mut spine, control, thread, ledger_path) = phone_ready("r3", vec!["On it! Here is where we are."]);
    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));

    control.submit_from_channel(&thread, Some(femcboost()), "where are we on the proposal?", "phone").unwrap();
    assert!(live.names().is_empty(), "a durable write alone told the Mac something");

    spine.poll_intake().unwrap();

    let his = live.of("rich://ceo-message");
    assert_eq!(his.len(), 1, "the Mac was never told what he said: {:?}", live.names());
    assert_eq!(
        his[0]["text"].as_str(),
        Some("where are we on the proposal?"),
        "the event carries something other than his sentence"
    );

    // **IT COMES BEFORE THE ANSWER, NOT WITH IT.** The defect a person saw was ORDERING, so
    // the test is about ordering: his words must be on the wire before the first byte of
    // Rich's reply, or the Mac is still showing him nothing while Rich writes.
    let names = live.names();
    let his_at = names.iter().position(|n| n == "rich://ceo-message").unwrap();
    let reply_at = names
        .iter()
        .position(|n| n == "rich://message-started" || n == "rich://message-delta")
        .expect("the mock lease produced no reply at all, so this proves nothing");
    assert!(
        his_at < reply_at,
        "his own words reached the Mac after Rich started answering them: {names:?}"
    );

    // And the identity is the PROJECTION'S, which is what stops the live row and the row a
    // reload projects being two rows for one sentence.
    let id = his[0]["messageId"].as_str().unwrap().to_string();
    let binding = spine.ledger().thread_binding(&thread).unwrap();
    let turn = spine
        .ledger()
        .thread_turns_scoped(&binding)
        .unwrap()
        .into_iter()
        .find(|t| t.user_text == "where are we on the proposal?")
        .expect("no ledger turn for his message");
    assert_eq!(id, format!("{}:user", turn.id), "the event's id is not this turn's `{{turn}}:user`");
    assert_eq!(
        his[0]["createdAt"].as_u64(),
        Some(turn.created_at),
        "the live row would sort differently from the projected one"
    );

    let _ = std::fs::remove_file(ledger_path);
}

#[test]
fn a_message_deferred_at_the_desk_carries_his_words_too() {
    // The `Steer` arm had the same hole and no surface had noticed yet: the desktop window
    // draws its own optimistic bubble, so a missing event there is invisible until a second
    // surface asks. Fixed in the same place and held here, because "invisible today" is how
    // this one got in.
    let (mut spine, control, thread, ledger_path) = phone_ready("steer", vec!["queued answer"]);
    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));

    // `steer()` refuses when nothing is running, correctly — so the turn it would be added
    // to is put in front of it, exactly as the running app does.
    control.begin_turn(richos_core::steering::ActiveTurn {
        turn_id: "turn-already-running".to_string(),
        thread_id: thread.clone(),
        entity_id: Some(femcboost()),
        started_at: Some(1),
    });
    control.steer("a correction, typed while he waited").unwrap();
    control.end_turn("turn-already-running");
    spine.poll_intake().unwrap();

    let his = live.of("rich://ceo-message");
    assert_eq!(his.len(), 1, "the steering arm still emits a status with no words: {:?}", live.names());
    assert_eq!(his[0]["text"].as_str(), Some("a correction, typed while he waited"));
    let _ = std::fs::remove_file(ledger_path);
}

#[test]
fn the_fence_on_his_phone_message_is_the_one_the_open_window_is_holding() {
    // **RAY'S R3 AGAIN, ON CANDIDATE .16, AND THIS IS THE HALF AN EVENT LIST CANNOT SEE.**
    // `his_phone_message_reaches_the_mac_with_his_words_in_it_and_not_only_a_status` proves
    // the event is emitted, carries his sentence, and precedes the reply. It was GREEN when
    // Ray measured **7,288 ms** on the real app, with the bubble and the whole reply landing
    // in ONE paint (`docs/verification/2026-09-19-nightly-1.2.0-nightly.20260919.5-stale-engine-and-phone-audit.md`
    // §"R3, phone → Mac").
    //
    // The event was emitted and THE WINDOW THREW IT AWAY. `timeline.js accepts()` applies
    // §13's staleness fence: `payload.bindingRevision < model.bindingRevision` → reject. The
    // window binds its model to the ACTIVATION revision (`main.js` → `active_binding()` →
    // `Spine::activate` → `Ledger::rebind_at_new_revision`, which mints a fresh revision from
    // `take_revision()` and deliberately persists nothing). `drain_intake` built its fence
    // from `Ledger::thread_binding()` instead — the revision written ONCE when the thread was
    // bound and never rewritten. So an intake fence is always strictly lower than the open
    // window's, and every intake-borne event in this family is dropped as stale.
    //
    // The sidebar's dot still moved at +471 ms because `rich://thread-summary-updated` is
    // handled outside the timeline model and is not fenced at all — the same "the path was
    // live and one thing was missing from it" shape R3 had the first time.
    //
    // A DESK message is unaffected: `submit_prompt_inner` uses `ensure_active_thread()`, the
    // activation binding. So the invariant is that both mouths fence the same.
    let (mut spine, control, thread, ledger_path) = phone_ready("fence", vec!["On it!"]);
    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));

    // POSITIVE CONTROL, in the same fixture: what the desk emits for the same thread is what
    // the window accepts, by construction. If this ever stops being strictly-greater-or-equal
    // the fence itself has changed and the assertion below is measuring nothing.
    let open_window_revision = spine.active_binding().expect("no active binding").binding_revision();
    spine.submit_prompt("typed at the desk", Source::Text).unwrap();
    for payload in live.of("rich://ceo-message") {
        assert_eq!(
            payload["bindingRevision"].as_u64(),
            Some(open_window_revision),
            "the desk's own fence no longer matches the window's, so this test proves nothing"
        );
    }

    let before = live.names().len();
    control.submit_from_channel(&thread, Some(femcboost()), "phone to Mac", "phone").unwrap();
    spine.poll_intake().unwrap();
    assert!(live.names().len() > before, "the drain emitted nothing at all");

    for name in ["rich://ceo-message", "rich://turn-status", "rich://thread-summary-updated"] {
        for payload in live.of(name).into_iter().skip_while(|p| p["text"] == "typed at the desk") {
            let on_the_wire =
                payload["bindingRevision"].as_u64().expect("no fence revision on the wire");
            assert!(
                on_the_wire >= open_window_revision,
                "{name} carries bindingRevision {on_the_wire}; the open window holds \
                 {open_window_revision}, so `accepts()` drops it and the thread pane does not \
                 repaint until `turn-completed` reloads it"
            );
        }
    }

    let _ = std::fs::remove_file(ledger_path);
}

// -------------------------------------------------------------------------------------
// THE MOUTH, KEPT ON AN OPERATOR INSTALL (operator back-end spec r3 (s); the operator-client
// record's §7 item 3)
// -------------------------------------------------------------------------------------

/// Every sentence of his in the ledger, as `(words, channel)`, in order.
fn mouths(spine: &Spine, thread: &str) -> Vec<(String, Option<String>)> {
    let binding = spine.ledger().thread_binding(thread).unwrap();
    spine
        .ledger()
        .thread_turns_scoped(&binding)
        .unwrap()
        .into_iter()
        .filter(|t| matches!(t.source, Source::Text | Source::Jam))
        .map(|t| (t.user_text.clone(), t.channel.clone()))
        .collect()
}

/// **The phone's words keep their mouth on an operator install, and nothing changes on a
/// product one.** r3 (s): *"the spine erases their origin"* (`drain_intake`'s `Channel` arm
/// files phone words as `Source::Text`), and the host must know it to refuse a phone
/// assignment. So a spine told to keep the channel records `phone` for the phone, the
/// record's own value for a mouth this build has never heard of, and `desk` for every road
/// from the Mac (typed, spoken, deferred through the log); a spine never told records none,
/// the product's lines exactly as before. Same fixture, same five sentences, both halves in
/// one test so they cannot drift apart.
#[test]
fn a_keeping_spine_records_every_prompt_s_mouth_and_a_product_spine_records_none() {
    for keep in [true, false] {
        let (mut spine, control, thread, ledger_path) =
            phone_ready(&format!("mouth-{keep}"), vec!["one", "two", "three", "four", "five"]);
        if keep {
            spine.keep_intake_channel(true);
        }
        control.submit_from_channel(&thread, Some(femcboost()), "from the couch", "phone").unwrap();
        spine.poll_intake().unwrap();
        spine.submit_prompt("typed at the desk", Source::Text).unwrap();
        spine.submit_prompt_spoken("said at the desk", Source::Jam, false).unwrap();
        control.submit_from_desk(&thread, Some(femcboost()), "deferred at the desk").unwrap();
        spine.poll_intake().unwrap();
        control.submit_from_channel(&thread, Some(femcboost()), "a new mouth", "watch").unwrap();
        spine.poll_intake().unwrap();
        let said = |mouth: &str| if keep { Some(mouth.to_string()) } else { None };
        assert_eq!(
            mouths(&spine, &thread),
            vec![
                ("from the couch".to_string(), said("phone")),
                ("typed at the desk".to_string(), said("desk")),
                ("said at the desk".to_string(), said("desk")),
                ("deferred at the desk".to_string(), said("desk")),
                ("a new mouth".to_string(), said("watch")),
            ],
            "keep = {keep}"
        );
        if let Err(error) = std::fs::remove_file(&ledger_path) { eprintln!("fixture cleanup: {error}"); }
    }
}

// -------------------------------------------------------------------------------------
// THE PHONE'S OWN ID FOR ITS WORDS (adoption ledger §2.8 row C5: the client id the phone
// assigns is the identity end to end)
// -------------------------------------------------------------------------------------

/// **A phone message comes back to the phone under the id the phone gave it.** Before this,
/// the CEO row the Mac sent for a phone message carried `client_id: null`
/// (`phone/rows.rs`), so a row that beat the phone's receipt could only be matched by its
/// words, and a voice note not at all until the receipt brought its transcript hash: the
/// phone showed the recording AND the transcript as two lines (iPhone
/// `EchoBeforeAcceptanceTests`, known issue since `bf08f0a64`).
///
/// The id is checked at all three places a phone can learn of the row: the turn itself, the
/// live `ceo-message` (the echo that can beat the receipt), and the projection read back
/// from disk (a reconnect's snapshot). Positive controls in the same fixture: a phone record
/// written WITHOUT an id (an older phone, or a record from before this field) and a message
/// typed at the desk both carry none, so nothing is invented for a turn the phone did not
/// name.
#[test]
fn a_phone_message_keeps_the_phones_own_id_on_its_turn_its_live_row_and_its_projection() {
    use richos_core::timeline::{Timeline, TimelineItem, ViewMode};
    let (mut spine, control, thread, ledger_path) = phone_ready("client-id", vec!["On it!", "Noted.", "Done."]);
    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));

    control.submit_from_channel_as(&thread, Some(femcboost()), "call the bank", "phone", Some("v1")).unwrap();
    control.submit_from_channel(&thread, Some(femcboost()), "sent by an older phone", "phone").unwrap();
    spine.poll_intake().unwrap();
    spine.submit_prompt("typed at the desk", Source::Text).unwrap();

    // The turn.
    let binding = spine.ledger().thread_binding(&thread).unwrap();
    let ids: Vec<(String, Option<String>)> = spine
        .ledger()
        .thread_turns_scoped(&binding)
        .unwrap()
        .into_iter()
        .filter(|t| t.source == Source::Text)
        .map(|t| (t.user_text.clone(), t.client_id.clone()))
        .collect();
    assert_eq!(
        ids,
        vec![
            ("call the bank".to_string(), Some("v1".to_string())),
            ("sent by an older phone".to_string(), None),
            ("typed at the desk".to_string(), None),
        ]
    );

    // The live row, which is the one that can reach the phone before the receipt does.
    let rows = live.of("rich://ceo-message");
    let live_id = |text: &str| -> Option<Value> {
        let row = rows.iter().find(|p| p["text"] == text).unwrap_or_else(|| panic!("no ceo-message for {text:?}"));
        row.get("clientId").cloned()
    };
    assert_eq!(live_id("call the bank"), Some(serde_json::json!("v1")));
    assert_eq!(live_id("sent by an older phone"), None);
    assert_eq!(live_id("typed at the desk"), None);

    // The projection, from the ledger re-read off disk as a restart reads it.
    let reopened = Ledger::open(&ledger_path).unwrap();
    let binding = reopened.thread_binding(&thread).unwrap();
    let timeline = Timeline::project(&reopened, &binding, &[]).unwrap();
    let projected: Vec<(String, Option<Value>)> = timeline
        .view(ViewMode::Ceo)
        .items()
        .iter()
        .filter(|i| matches!(i, TimelineItem::UserMessage { .. }))
        .map(|i| {
            let item = serde_json::to_value(i).unwrap();
            (item["text"].as_str().unwrap().to_string(), item.get("clientId").cloned())
        })
        .collect();
    assert_eq!(
        projected,
        vec![
            ("call the bank".to_string(), Some(serde_json::json!("v1"))),
            ("sent by an older phone".to_string(), None),
            ("typed at the desk".to_string(), None),
        ]
    );
    if let Err(error) = std::fs::remove_file(&ledger_path) { eprintln!("fixture cleanup: {error}"); }
}

// -------------------------------------------------------------------------------------
// A PHONE VOICE NOTE IS A VOICE MESSAGE FROM ITS FIRST APPEARANCE
// -------------------------------------------------------------------------------------

/// **The early echo of a phone voice note says it is a voice note.** The phone's voice upload
/// writes the transcript into the intake log like typed words, and the drain turned it into an
/// ordinary typed turn: the live `ceo-message`, which is the row that can reach the phone
/// before the upload's receipt, then carried nothing a phone could tell a voice note by, and
/// `phone/rows.rs` projected it as `kind: "text"`. The phone drew the transcript as a text
/// bubble until the receipt arrived.
///
/// The fact is checked where a phone learns of the row: the turn, the live `ceo-message`
/// (`voiceNote: true`), and the projection re-read from disk (hello, backfill, history after a
/// reconnect). `source` stays `text`: a voice note is not the desk's voice mode. Positive
/// controls in the same fixture: a typed phone message, a record from an older phone and a
/// message typed at the desk carry no voice-note key at all.
#[test]
fn a_phone_voice_note_says_so_on_its_turn_its_live_row_and_its_projection() {
    use richos_core::timeline::{Timeline, TimelineItem, ViewMode};
    let (mut spine, control, thread, ledger_path) =
        phone_ready("voice-note", vec!["On it!", "Noted.", "Done.", "Sure."]);
    let live = RecordingLive::default();
    spine.set_live_observer(Box::new(live.clone()));

    control.submit_voice_note_from_channel(&thread, Some(femcboost()), "call the bank", "phone_voice", Some("v1")).unwrap();
    control.submit_from_channel_as(&thread, Some(femcboost()), "and the landlord", "phone_typed", Some("t1")).unwrap();
    control.submit_from_channel(&thread, Some(femcboost()), "sent by an older phone", "phone").unwrap();
    spine.poll_intake().unwrap();
    spine.submit_prompt("typed at the desk", Source::Text).unwrap();
    let expected = |voice: Value| {
        vec![
            ("call the bank".to_string(), voice),
            ("and the landlord".to_string(), Value::Null),
            ("sent by an older phone".to_string(), Value::Null),
            ("typed at the desk".to_string(), Value::Null),
        ]
    };

    // The turn: a voice note, and still `Source::Text`.
    let binding = spine.ledger().thread_binding(&thread).unwrap();
    let turns: Vec<(String, Value)> = spine
        .ledger()
        .thread_turns_scoped(&binding)
        .unwrap()
        .into_iter()
        .filter(|t| t.source == Source::Text)
        .map(|t| (t.user_text.clone(), if t.voice_note { Value::Bool(true) } else { Value::Null }))
        .collect();
    assert_eq!(turns, expected(Value::Bool(true)));

    // The live row: the one that can beat the receipt.
    let rows = live.of("rich://ceo-message");
    let live_rows: Vec<(String, Value)> = ["call the bank", "and the landlord", "sent by an older phone", "typed at the desk"]
        .iter()
        .map(|text| {
            let row = rows.iter().find(|p| p["text"] == *text).unwrap_or_else(|| panic!("no ceo-message for {text:?}"));
            assert_eq!(row["source"], "text", "{text}: a voice note is not the desk's voice mode");
            (text.to_string(), row.get("voiceNote").cloned().unwrap_or(Value::Null))
        })
        .collect();
    assert_eq!(live_rows, expected(Value::Bool(true)), "the early echo");

    // The projection, from the ledger re-read off disk as a restart reads it.
    let reopened = Ledger::open(&ledger_path).unwrap();
    let binding = reopened.thread_binding(&thread).unwrap();
    let timeline = Timeline::project(&reopened, &binding, &[]).unwrap();
    let projected: Vec<(String, Value)> = timeline
        .view(ViewMode::Ceo)
        .items()
        .iter()
        .filter(|i| matches!(i, TimelineItem::UserMessage { .. }))
        .map(|i| {
            let item = serde_json::to_value(i).unwrap();
            (item["text"].as_str().unwrap().to_string(), item.get("voiceNote").cloned().unwrap_or(Value::Null))
        })
        .collect();
    assert_eq!(projected, expected(Value::Bool(true)), "the projection after a restart");
    if let Err(error) = std::fs::remove_file(&ledger_path) { eprintln!("fixture cleanup: {error}"); }
}

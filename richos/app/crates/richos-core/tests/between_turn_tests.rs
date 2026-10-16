//! BETWEEN-TURN TRAFFIC AGAINST A REAL CHILD PROCESS — techy-mode design §1.5, gap #1.
//!
//! *"the client delivers a frame only if `current_prompt` is `Some`. Anything the agent
//! emits at session start or between turns hits no sink at all."*
//!
//! **Ported from the ACP wire to the native `claude` wire (`wiki/ceo-decisions.md` §16).**
//! Same three tests, same three invariants, plus one the old wire could not express (see
//! the watermark test below). They do not mock `NativeClient`: they spawn a real child over
//! real stdio and drive the real handshake, the real reader thread, real `dispatch`, real
//! between-turn buffer, following `native_cancel_tests.rs`'s POSIX-`sh` fake agent exactly —
//! so nothing here needs Claude, a network, Node, or the `claude` binary.
//!
//! The agent emits its `system/init` at TWO moments the real one was measured emitting it
//! (`docs/verification/native-claude-stream-json-2026-08-31/raw/run9-rust-driven.jsonl`, four
//! `system/init` frames across four turns): once at session start, and once after the turn's
//! `result` has already been returned. Both are moments when `current_prompt` is `None`,
//! which is the whole point.
//!
//! **And the repeat is NOT byte-identical, which is the change from the ACP version of this
//! file.** The measured `system/init` frames differ by a per-frame `uuid`, so the fake
//! differs by one too — and the suppression assertion below therefore proves
//! `machinery::meta_identity` is doing real work rather than getting lucky on a fixture that
//! was easier to write than the wire is.
//!
//! ## Why these poll instead of sleeping a fixed time
//! The trailing frames are written by the child AFTER the `result` that unblocks `prompt`,
//! so the reader thread may not have consumed them when `prompt` returns. A fixed sleep
//! would be either flaky or slow. `wait_for` polls to a deadline and FAILS with what it
//! actually saw, so a real regression reads as a regression rather than as a timeout.

use richos_core::cognition::TurnItem;
use richos_core::machinery::MachineryRecord;
use richos_core::native::NativeClient;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// A fake native agent in POSIX sh that emits SessionMeta traffic where no turn is running.
///
/// The `case` patterns match the exact bytes `NativeClient` writes: `serde_json` serializes a
/// `json!` object through a `BTreeMap`, so keys come out alphabetically and
/// `"type":"user"` is a stable substring rather than a hopeful one.
fn fake_agent(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "richos-between-native-{tag}-{}-{}.sh",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    let script = r#"#!/bin/sh
TURN=0
while IFS= read -r line; do
  case "$line" in
    *'"subtype":"initialize"'*)
      printf '{"type":"control_response","response":{"subtype":"success","request_id":"req_init","response":{}}}\n'
      # SESSION-START TRAFFIC. No turn has been sent, so `current_prompt` is None.
      # It also carries the session MODEL, which is what lets the watermark key its
      # denominator by name instead of taking an arbitrary map entry.
      printf '{"type":"system","subtype":"init","model":"claude-sonnet-5","tools":["Bash"],"session_id":"s","uuid":"u-start"}\n'
      ;;
    *'"type":"user"'*)
      TURN=`expr $TURN + 1`
      printf '{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1"}}}\n'
      printf '{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"answer"}}}\n'
      printf '{"type":"stream_event","event":{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"input_tokens":2,"cache_creation_input_tokens":3603,"cache_read_input_tokens":25737,"output_tokens":95}}}\n'
      printf '{"type":"result","subtype":"success","stop_reason":"end_turn","terminal_reason":"completed","modelUsage":{"claude-haiku-4-5":{"contextWindow":200000},"claude-sonnet-5":{"contextWindow":1000000}}}\n'
      # BETWEEN-TURN TRAFFIC: after the result, so `current_prompt` is None again.
      # `system/init` repeats with a DIFFERENT uuid, exactly as the measured agent repeats it.
      printf '{"type":"system","subtype":"init","model":"claude-sonnet-5","tools":["Bash"],"session_id":"s","uuid":"u-turn-%s"}\n' "$TURN"
      printf '{"type":"system","subtype":"status","status":"requesting","session_id":"s","uuid":"u-st"}\n'
      ;;
  esac
done
"#;
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(script.as_bytes()).unwrap();
    drop(f);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    path
}

fn connect(script: &PathBuf) -> NativeClient {
    // The chat lease carries RichOS's standing instruction, so these fakes need one on disk.
    // Rendered by the real `doctrine` module rather than stubbed, so a doctrine that stopped
    // satisfying `preflight` would break these too.
    let dir = std::env::temp_dir().join(format!("richos-doctrine-fake-{}", std::process::id()));
    let doctrine = richos_core::doctrine::ensure_rendered(
        &dir,
        &richos_core::doctrine::DoctrineIdentity::default(),
    )
    .expect("the doctrine fixture must render");
    let skills = richos_core::skills::ensure_rendered(&dir, &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app")).expect("the skills fixture must render");
    match NativeClient::spawn(script, std::path::Path::new("/tmp"), &doctrine, &skills) {
        Ok(c) => c,
        Err(e) => panic!("the fake agent should have completed the handshake: {e}"),
    }
}

/// Poll `f` until it returns `Some`, or fail with `what` after 5 seconds.
fn wait_for<T>(what: &str, mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(v) = f() {
            return v;
        }
        if Instant::now() > deadline {
            panic!("timed out waiting for {what}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn drain_some(client: &NativeClient, session: &str, what: &str) -> Vec<MachineryRecord> {
    wait_for(what, || {
        let r = client.drain_between_turn(session);
        (!r.is_empty()).then_some(r)
    })
}

#[test]
fn session_start_traffic_reaches_the_lane_instead_of_no_sink_at_all() {
    let script = fake_agent("start");
    let client = connect(&script);
    let session = client.session_id().to_string();

    // No turn has ever been sent on this client. Before §1.5's fix there was, by
    // construction, nowhere for this frame to be — `current_prompt` is None and the only
    // sink lived inside it.
    let records = drain_some(&client, &session, "the session-start frame");

    assert_eq!(records.len(), 1, "one frame was emitted at session start");
    assert_eq!(records[0].title, "system:init");
    // §1.4 G4 — the record attaches to the THREAD, not to a turn, because there is no turn
    // it belongs to. Inventing one would be a false attribution.
    assert_eq!(records[0].turn_id, None);
    assert_eq!(records[0].session_id, session, "which lease said it stays reconstructible");
    // Retained verbatim (§1.4 G5): the payload is the wire object, not a summary of it.
    assert_eq!(records[0].payload.as_ref().unwrap()["tools"], serde_json::json!(["Bash"]));
    let _ = std::fs::remove_file(&script);
}

#[test]
fn traffic_after_the_turn_result_lands_in_the_lane_and_not_in_the_finished_turn() {
    let script = fake_agent("after");
    let client = connect(&script);
    let session = client.session_id().to_string();
    // Take the session-start record out of the way so this test is about the OTHER moment.
    drain_some(&client, &session, "the session-start frame");

    // Run a real turn. THE IN-TURN PATH IS UNCHANGED IN SHAPE: text still arrives as `Text`,
    // everything else as in-turn machinery on the shared counter.
    let mut text = String::new();
    let mut in_turn: Vec<(u64, String)> = Vec::new();
    let stop = client
        .prompt("hello", &mut |item| match item {
            TurnItem::Text { text: t, .. } => text.push_str(t),
            TurnItem::Machinery(m) => in_turn.push((m.seq, m.title.clone())),
            TurnItem::AnswerComplete => {}
        })
        .unwrap();
    assert_eq!(stop, "end_turn");
    assert_eq!(text, "answer");
    // SPIKE CAVEAT C3, PINNED. The numerator is on this turn's `message_delta`, but the
    // denominator only arrives with the `result` that ENDS the turn — so the first turn of a
    // fresh lease carries NO measurement, and the frame is retained verbatim instead of a
    // denominator being guessed. `watermark_test` below proves the second turn has one.
    assert_eq!(
        in_turn,
        vec![(1, "stream_event:message_delta".to_string())],
        "no derived measurement on turn 1 — C3"
    );

    let records = drain_some(&client, &session, "the post-result frames");

    // The repeated `system/init` differs only by `uuid`, so §1.5's `last_session_meta` slot
    // suppresses it and only `system:status` survives — §1.2's "last value wins", doing the
    // work that bounds a long session. A slot comparing frames VERBATIM would have kept it.
    let titles: Vec<&str> = records.iter().map(|r| r.title.as_str()).collect();
    assert_eq!(titles, vec!["system:status"], "got {titles:?}");
    assert_eq!(client.suppressed_between_turn_repeats(), 1);
    assert!(records.iter().all(|r| r.turn_id.is_none()));
    // Its own counter, continuing from the session-start record rather than restarting.
    assert_eq!(records[0].seq, 1);
    let _ = std::fs::remove_file(&script);
}

#[test]
fn the_second_turn_carries_the_derived_context_measurement() {
    // NEW, and it exists because the wire change created the gap it closes. ACP handed over
    // `usage_update {used, size}` on every turn; this wire splits the pair across a mid-turn
    // frame and a turn-ending one, so the measurement can only exist from turn 2 onward.
    let script = fake_agent("watermark");
    let client = connect(&script);
    let session = client.session_id().to_string();
    drain_some(&client, &session, "the session-start frame");

    client.prompt("first", &mut |_| {}).unwrap();
    drain_some(&client, &session, "the post-result frames");

    let mut usage: Vec<(u64, u64)> = Vec::new();
    let mut titles: Vec<String> = Vec::new();
    client
        .prompt("second", &mut |item| {
            if let TurnItem::Machinery(m) = item {
                titles.push(m.title.clone());
                if let Some(u) = m.context_usage() {
                    usage.push((u.used, u.size));
                }
            }
        })
        .unwrap();

    // 2 + 25_737 + 3_603 = 29_342, re-derived rather than trusted. `input_tokens` alone
    // would have said 2.
    assert_eq!(usage, vec![(29_342, 1_000_000)]);
    // And the DENOMINATOR is the SESSION model's window, not an arbitrary entry from the
    // `modelUsage` map — the fake offers `claude-haiku-4-5`'s 200,000 first, which is the
    // exact mistake findings §10 caught the spike's own probe making.
    assert_ne!(usage[0].1, 200_000, "the wrong model's window would be a 5x error");
    // The derived record does NOT replace the frame it came from: both are retained.
    assert!(titles.contains(&"context_usage".to_string()));
    assert!(titles.contains(&"stream_event:message_delta".to_string()));
    let _ = std::fs::remove_file(&script);
}

#[test]
fn a_drained_lane_is_empty_and_stays_empty() {
    // The honest empty state at its source: a lane that has been drained reports nothing,
    // and "nothing" here means the agent said nothing since — never that the channel is
    // broken. The renderer's version of this sentence is checked in `app/ui/tests`.
    let script = fake_agent("empty");
    let client = connect(&script);
    let session = client.session_id().to_string();
    drain_some(&client, &session, "the session-start frame");
    assert!(client.drain_between_turn(&session).is_empty());
    assert!(client.drain_between_turn(&session).is_empty());
    let _ = std::fs::remove_file(&script);
}

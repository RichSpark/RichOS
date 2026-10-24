//! **THE MESSAGE ROW — the one translation between the Mac's gated bytes and the phone's shape.**
//!
//! The landed phone merges rows shaped like this (`web/web-app/CONTRACT-STUB.md` §2(b)):
//!
//! ```text
//! {"id","thread_id","cursor","role":"ceo"|"rich","kind":"text"|"voice","text","created_at",
//!  "client_id","has_audio","from_microphone","state","complete"}
//! ```
//!
//! and the Mac holds a `TimelineView` and a stream of `LiveEvent`s. This file is the whole of the
//! distance between the two.
//!
//! # WHAT MAKES THIS SAFE, AND IT IS NOT CARE
//!
//! Plan §4.2 (iii): *"the phone emitter must never read raw events or the ledger directly, only
//! this stream and the same gated timeline payload the webview gets."*
//!
//! Every function here takes **already-gated JSON** and nothing else: the value of
//! `Timeline::view(ViewMode::Ceo).payload()`, or the `payload()` of a `LiveEvent` the spine's
//! `forward_live` chokepoint already allowed through. There is no `Ledger`, no `Timeline` and no
//! `Spine` in this file's imports — so it cannot reach an ungated byte, rather than choosing not
//! to.
//!
//! # And what it throws away
//!
//! Machinery. A gated CEO timeline still contains `work_duration`, `activity` and
//! `worker_activity` rows, and plan §6 is explicit: *"**Machinery on the phone.** The drill-down
//! view is optional on the desktop and absent on the phone. The small screen is the one place
//! 'clean output' is most easily broken and hardest to recover."* [`rows_from_payload`] keeps
//! `user_message` and `rich_message` and drops everything else.
//!
//! # The cursor
//!
//! **A row's cursor is its 1-based position among the kept rows of the gated projection.** That
//! is a deterministic function of the append-only ledger, so a reload computes the same numbers
//! a stream sent — which is what makes `before=<cursor>` backfill and `since=<cursor>` agree
//! with each other across a restart. Live rows continue the count from the projection's length
//! at the moment the stream opened, and every `hello` re-seeds it from the projection, so a
//! drift cannot survive one reconnection.
//!
//! **Its honest limit, stated because the phone merges on it:** the cursor is a position, not an
//! identity. `id` is the identity — the timeline item's own derived id, stable across
//! re-projection by construction (`timeline.rs`: *"Re-projecting the same durable inputs after a
//! restart produces the same id"*).

use serde_json::{json, Value};

/// The rows the phone understands, in order, cursors assigned.
///
/// **A file Rich attached goes under his words** (round 36 screen 3): the tool is called while
/// he works, so its item can sort before the reply it belongs to. Each `rich_file` item is
/// moved to just after the last `rich_message` of its own turn; a file with no reply in its
/// turn (one Stu attached for a job) keeps its own place.
pub fn rows_from_payload(payload: &Value) -> Vec<Value> {
    let empty: Vec<Value> = Vec::new();
    let items = payload.get("items").and_then(|v| v.as_array()).unwrap_or(&empty);
    let last_reply: std::collections::HashMap<&str, usize> = items
        .iter()
        .enumerate()
        .filter(|(_, i)| i["kind"] == "rich_message")
        .filter_map(|(at, i)| Some((i["turnId"].as_str()?, at)))
        .collect();
    let mut ordered: Vec<&Value> = Vec::with_capacity(items.len());
    let mut held: Vec<(usize, &Value)> = Vec::new();
    for (at, item) in items.iter().enumerate() {
        let after = (item["kind"] == richos_core::rich_files::ITEM_KIND)
            .then(|| item["turnId"].as_str().and_then(|t| last_reply.get(t)).copied())
            .flatten()
            .filter(|reply| *reply > at);
        match after {
            Some(reply) => held.push((reply, item)),
            None => ordered.push(item),
        }
        held.retain(|(reply, file)| {
            if *reply == at {
                ordered.push(file);
                false
            } else {
                true
            }
        });
    }
    let mut out: Vec<Value> = Vec::new();
    for item in ordered {
        if let Some(mut row) = row_from_item(item) {
            row["cursor"] = json!(out.len() as u64 + 1);
            out.push(row);
        }
    }
    out
}

/// Select the completed turn's reply without renumbering its conversation cursor.
pub fn reply_for_turn(payload: &Value, turn: &str) -> Option<Value> {
    let ids: std::collections::HashSet<&str> = payload.get("items")?.as_array()?.iter()
        .filter(|item| item["turnId"] == turn)
        .filter_map(|item| item["id"].as_str()).collect();
    rows_from_payload(payload).into_iter().rev().find(|row|
        row["role"] == "rich" && row["kind"] != "question" && row["kind"] != "file"
            && row["id"].as_str().is_some_and(|id| ids.contains(id)))
}

/// One gated timeline item as a phone row, or `None` when it is not a message.
pub fn row_from_item(item: &Value) -> Option<Value> {
    let kind = item.get("kind").and_then(|v| v.as_str())?;
    let id = item.get("id").and_then(|v| v.as_str())?.to_string();
    let thread_id = item.get("threadId").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let created_at = item.get("createdAt").and_then(|v| v.as_u64()).unwrap_or(0);
    let text = item.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
    match kind {
        "question"=>Some(json!({"id":id,"thread_id":thread_id,"cursor":0,"role":"rich","kind":"question","text":text,
            "question":item["question"],"created_at":iso8601(created_at),"complete":true,"state":"complete","has_audio":false})),
        "user_message" => {
            // `source` is the ledger's own word for how the input arrived. `jam` is a spoken
            // turn, which the phone shows with its microphone marker; anything else is typed.
            let spoken = item.get("source").and_then(|v| v.as_str()) == Some("jam");
            Some(json!({
                "id": id,
                "thread_id": thread_id,
                "cursor": 0,
                "role": "ceo",
                "kind": message_kind(spoken, item),
                "text": text,
                "created_at": iso8601(created_at),
                // The phone's own id for this message when a phone sent it, else `null`.
                "client_id": client_id_of(item),
                "has_audio": false,
                "from_microphone": spoken,
                "state": "sent",
                "complete": true,
            }))
        }
        "rich_message" => Some(json!({
            "id": id,
            "thread_id": thread_id,
            "cursor": 0,
            "role": "rich",
            "kind": "text",
            "text": text,
            "created_at": iso8601(created_at),
            "client_id": Value::Null,
            // Nothing is synthesized until he asks for it (plan §3.4 verb 4), so a projected
            // row never claims audio is waiting. Slice B is what makes this ever true.
            "has_audio": false,
            "from_microphone": false,
            "state": "complete",
            "complete": true,
        })),
        // A FILE RICH PUT IN FRONT OF THE USER (round 36 screen 3): a Rich row of kind `file`,
        // carrying the card's own description (`rich_files::file_value`): an id to ask the Mac
        // for, never a path. The phone fetches the bytes only when the user taps it
        // (`GET /api/messages?kind=file`, `routes.rs`). A client that does not know the kind
        // skips it.
        k if k == richos_core::rich_files::ITEM_KIND => Some(json!({
            "id": id,
            "thread_id": thread_id,
            "cursor": 0,
            "role": "rich",
            "kind": "file",
            "text": text,
            "file": item["file"],
            "created_at": iso8601(created_at),
            "client_id": Value::Null,
            "has_audio": false,
            "from_microphone": false,
            "state": "complete",
            "complete": true,
        })),
        // Machinery. Plan §6: absent on the phone, by construction rather than by a renderer
        // remembering to skip it.
        _ => None,
    }
}

/// **The `client_id` a CEO row carries: the one the phone sent with the message, or `null`.**
///
/// A phone assigns each message an id at Send and keeps it as that message's line from the tap
/// to the Mac's row (adoption ledger §2.8 row C5: the client id is the identity end to end).
/// The Mac keeps that id on the turn (`IntakeRecord::Channel` -> `PromptReceived` ->
/// `Turn::client_id`), and both the projection and the live `ceo-message` carry it as
/// `clientId`. Until they did, every row here said `null`, so a row that reached the phone
/// before the phone's receipt could only be matched by its words, and a voice note not at all:
/// the phone drew the recording and its transcript as two lines until the receipt came.
///
/// `null` for everything a phone did not send (typed or spoken at the Mac, a turn from before
/// this field), and for anything that is not a string, so a phone never matches on a value the
/// Mac did not mean as an id.
fn client_id_of(source: &Value) -> Value {
    match source.get("clientId") {
        Some(Value::String(id)) if !id.is_empty() => Value::String(id.clone()),
        _ => Value::Null,
    }
}

/// **A CEO row's `kind`: `"voice"` for anything he said rather than typed.** Two facts make it
/// so, and either is enough:
///
/// - `source: "jam"`, a turn spoken at the Mac in voice mode (which also sets
///   `from_microphone`);
/// - `voiceNote: true`, the transcript of a voice message he recorded on a phone
///   (`Turn::voice_note`, kept from the intake record the voice upload wrote).
///
/// The second is read off the same gated JSON as everything else here, on the projection and on
/// the live `ceo-message` alike, so the row that beats the phone's receipt is already a voice
/// row. Until it was, that row said `"text"` and the phone drew the transcript as a text bubble
/// until the receipt arrived. `from_microphone` is deliberately left to `jam` alone: it is the
/// web phone's "Spoken" label for the desk's voice mode, and an older phone reading it must see
/// exactly what it saw before. Anything that is not the boolean `true` is not a voice note.
fn message_kind(spoken: bool, source: &Value) -> &'static str {
    if spoken || source.get("voiceNote") == Some(&Value::Bool(true)) {
        "voice"
    } else {
        "text"
    }
}

/// One live event as the phone's `(event name, data)`, or `None` when the phone has no use for
/// it.
///
/// `cursor` is the position the caller has assigned to this message — see the module doc.
pub fn event_from_live(name: &str, payload: &Value, cursor: u64) -> Option<(&'static str, Value)> {
    let message_id = payload.get("messageId").and_then(|v| v.as_str())?.to_string();
    let thread_id = payload.get("threadId").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let at = payload.get("at").and_then(|v| v.as_u64()).unwrap_or(0);
    match name {
        // **WHAT THE CEO HIMSELF SAID, LIVE** — `esc-20260919T003541Z-6885f74b`, and the half of
        // defect 4.1 that could not be fixed on the page.
        //
        // He types on the Mac; the phone is looking at the same thread. Until
        // `LiveEvent::CeoMessage` existed there was no event in this family about his own
        // message, so the phone learned about it only by asking the Mac for the rows again —
        // which it does after a reply settles, and which never happens at all on a turn that
        // produces no reply. His own sentence then stayed off the phone until the next reload.
        //
        // **THE ID IS THE PROJECTION'S**, `{turn}:user`, and `created_at` is the turn's own. So
        // the row this builds and the row a backfill builds for the same message are ONE row to
        // a phone that merges on `id`, which is the property that stops this becoming a
        // duplicate-message defect instead of a missing-message one.
        "rich://ceo-message" => {
            // The same rule `row_from_item` applies to a projected user message, in the same
            // words: `jam` is a spoken turn, anything else is typed.
            let spoken = payload.get("source").and_then(|v| v.as_str()) == Some("jam");
            Some((
                "message",
                json!({
                    "id": message_id,
                    "thread_id": thread_id,
                    "cursor": cursor,
                    "role": "ceo",
                    // **THE ROW THAT CAN BEAT THE RECEIPT**, so a voice note is a voice row here
                    // already — see [`message_kind`].
                    "kind": message_kind(spoken, payload),
                    "text": payload.get("text").and_then(|v| v.as_str()).unwrap_or(""),
                    // The TURN's instant, not this event's: the projected row carries the same
                    // one, and two timestamps for one message is two orderings for one thread.
                    "created_at": iso8601(payload.get("createdAt").and_then(|v| v.as_u64()).unwrap_or(at)),
                    // **THE PHONE'S OWN ID, so this row can beat the receipt and still be
                    // recognized** — see [`client_id_of`].
                    "client_id": client_id_of(payload),
                    "has_audio": false,
                    "from_microphone": spoken,
                    "state": "sent",
                    "complete": true,
                }),
            ))
        }
        // A run of Rich's prose opens. The row goes out empty and incomplete; the deltas fill
        // it. This is what lets the phone show the reply arriving rather than appearing.
        "rich://message-started" => Some((
            "message",
            json!({
                "id": message_id,
                "thread_id": thread_id,
                "cursor": cursor,
                "role": "rich",
                "kind": "text",
                "text": "",
                "created_at": iso8601(at),
                "client_id": Value::Null,
                "has_audio": false,
                "from_microphone": false,
                "state": "streaming",
                "complete": false,
            }),
        )),
        "rich://message-delta" => Some((
            "delta",
            // **`thread_id` IS ON THE DELTA TOO** (phone protocol contract 2026-09-22 §11 item 4).
            // The hub is one broadcast for every conversation, and a delta without its thread is
            // a reply that a phone can only place in whatever conversation is on screen. Every
            // `message` row already carries it; an older client ignores the extra key.
            json!({
                "message_id": message_id,
                "thread_id": thread_id,
                "cursor": cursor,
                "text": payload.get("textDelta").and_then(|v| v.as_str()).unwrap_or(""),
            }),
        )),
        // **The full text again, deliberately.** `MessageCompleted` carries the whole run read
        // back from the ledger *"so a consumer that missed every delta is still correct"* — so
        // the phone gets a complete row here, not just a state flag. A phone that came back
        // mid-reply is then right without a reload.
        "rich://message-completed" => Some((
            "message",
            json!({
                "id": message_id,
                "thread_id": thread_id,
                "cursor": cursor,
                "role": "rich",
                "kind": "text",
                "text": payload.get("text").and_then(|v| v.as_str()).unwrap_or(""),
                "created_at": iso8601(at),
                "client_id": Value::Null,
                "has_audio": false,
                "from_microphone": false,
                "state": "complete",
                "complete": true,
            }),
        )),
        // Everything else in the §13 family is either machinery or about a turn rather than a
        // message, and the phone has no row for it. Dropped here rather than sent and ignored,
        // so his cellular connection does not carry bytes nothing renders.
        _ => None,
    }
}

/// Does this live event open a new message row — that is, does it consume a cursor?
///
/// **`rich://ceo-message` is one, and forgetting it would have been silent.** A cursor is a
/// position among the kept rows of the projection, and the projection counts his message: a
/// live family that emitted his row without consuming a cursor would put every Rich row after
/// it one position ahead of where a reload puts it, and `before=`/`since=` would stop agreeing
/// across a restart — which is the drift the module doc says a `hello` exists to repair.
pub fn opens_a_row(name: &str) -> bool {
    name == "rich://message-started" || name == "rich://ceo-message"
}

/// Milliseconds since the epoch as an ISO-8601 instant in UTC, e.g. `2026-09-18T12:34:56.789Z`.
///
/// Written out rather than adding a date crate: it is one function, the phone only ever parses
/// it with `new Date(...)`, and the algorithm is the standard civil-from-days one with its own
/// known-answer tests below.
pub fn iso8601(millis: u64) -> String {
    let secs = (millis / 1000) as i64;
    let ms = millis % 1000;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (hour, minute, second) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{ms:03}Z")
}

/// Days since 1970-01-01 to a calendar date. Howard Hinnant's `civil_from_days`, which is the
/// reference algorithm for this and is correct for every date the Gregorian calendar has.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_completed_turns_push_keeps_its_reply_when_a_later_turn_is_already_streaming() {
        let payload=json!({"items":[
            {"kind":"user_message","id":"a:user","threadId":"thread","turnId":"a","text":"First"},
            {"kind":"rich_message","id":"a:reply","threadId":"thread","turnId":"a","text":"First done"},
            {"kind":"user_message","id":"b:user","threadId":"thread","turnId":"b","text":"Second"},
            {"kind":"rich_message","id":"b:reply","threadId":"thread","turnId":"b","text":"Second still writing"}
        ]});
        let first=reply_for_turn(&payload,"a").unwrap();
        assert_eq!(first["id"],"a:reply");
        assert_eq!(first["text"],"First done");
        assert_eq!(first["cursor"],2,"a push retains its position in the whole conversation");
        assert!(reply_for_turn(&payload,"unanswered").is_none());
        assert_eq!(reply_for_turn(&payload,"b").unwrap()["id"],"b:reply");
    }

    /// A gated CEO timeline payload with both message kinds and three machinery rows, in the
    /// shape `Timeline::view(ViewMode::Ceo).payload()` actually produces (`kind` is the serde
    /// tag, snake_case; the base is flattened; fields are camelCase).
    fn payload() -> Value {
        json!({
            "entityId": "femcboost",
            "threadId": "thr_5c1e",
            "mode": "ceo",
            "betweenTurns": [],
            "items": [
                { "kind": "user_message", "id": "i1", "threadId": "thr_5c1e", "turnId": "t1",
                  "createdAt": 1_758_200_000_000u64, "text": "where are we on the proposal?",
                  "source": "text", "slot": "opening", "visibility": "ceo", "sequence": null,
                  "entityId": "femcboost", "bindingRevision": 1 },
                { "kind": "work_duration", "id": "i2", "threadId": "thr_5c1e", "turnId": "t1",
                  "createdAt": 1_758_200_000_100u64, "state": "working", "slot": "terminal",
                  "visibility": "ceo", "sequence": null, "entityId": "femcboost",
                  "bindingRevision": 1 },
                { "kind": "activity", "id": "i3", "threadId": "thr_5c1e", "turnId": "t1",
                  "createdAt": 1_758_200_000_200u64, "slot": "stream", "visibility": "ceo",
                  "sequence": 1, "entityId": "femcboost", "bindingRevision": 1 },
                { "kind": "rich_message", "id": "i4", "threadId": "thr_5c1e", "turnId": "t1",
                  "createdAt": 1_758_200_000_300u64, "text": "On it! Here is where we are.",
                  "phase": "unknown", "slot": "stream", "visibility": "ceo", "sequence": 2,
                  "entityId": "femcboost", "bindingRevision": 1 },
                { "kind": "worker_activity", "id": "i5", "threadId": "thr_5c1e", "turnId": "t1",
                  "createdAt": 1_758_200_000_400u64, "slot": "stream", "visibility": "ceo",
                  "sequence": 3, "entityId": "femcboost", "bindingRevision": 1 },
                { "kind": "user_message", "id": "i6", "threadId": "thr_5c1e", "turnId": "t2",
                  "createdAt": 1_758_200_001_000u64, "text": "and the numbers?",
                  "source": "jam", "slot": "opening", "visibility": "ceo", "sequence": null,
                  "entityId": "femcboost", "bindingRevision": 1 }
            ]
        })
    }

    #[test]
    fn only_the_conversation_reaches_the_phone_and_no_machinery_does() {
        // Plan §6, as a property of the translation rather than of a renderer. The fixture
        // carries a work-duration row, an activity row and a worker row on purpose.
        let rows = rows_from_payload(&payload());
        assert_eq!(rows.len(), 3, "machinery reached the phone: {rows:?}");
        let ids: Vec<&str> = rows.iter().map(|r| r["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["i1", "i4", "i6"]);
        for row in &rows {
            assert!(matches!(row["role"].as_str().unwrap(), "ceo" | "rich"));
        }
    }

    #[test]
    fn the_cursor_is_the_position_among_the_kept_rows_and_counts_from_one() {
        // Not the position among ALL items — if it were, the cursors would have gaps wherever
        // machinery had been dropped, and `before=<cursor>` would skip real messages.
        let rows = rows_from_payload(&payload());
        assert_eq!(rows[0]["cursor"], 1);
        assert_eq!(rows[1]["cursor"], 2);
        assert_eq!(rows[2]["cursor"], 3);
    }

    #[test]
    fn the_same_payload_projects_the_same_cursors_every_time() {
        // What makes `since=` and `before=` survive a restart: the projection is a pure
        // function of the ledger, so the numbers are reproducible rather than remembered.
        assert_eq!(rows_from_payload(&payload()), rows_from_payload(&payload()));
    }

    #[test]
    fn a_spoken_turn_carries_the_microphone_marker_and_a_typed_one_does_not() {
        // The 🎤 marker the bridge established (plan §4.3), as data rather than as a character
        // in the text — so the phone decides how to show it.
        let rows = rows_from_payload(&payload());
        assert_eq!(rows[0]["from_microphone"], false);
        assert_eq!(rows[0]["kind"], "text");
        assert_eq!(rows[2]["from_microphone"], true);
        assert_eq!(rows[2]["kind"], "voice");
    }

    #[test]
    fn a_row_carries_every_field_the_phone_reads_and_nothing_else() {
        // The phone merges on these names. A missing field is a row it silently renders wrong.
        let rows = rows_from_payload(&payload());
        let mut keys: Vec<&String> = rows[0].as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "client_id",
                "complete",
                "created_at",
                "cursor",
                "from_microphone",
                "has_audio",
                "id",
                "kind",
                "role",
                "state",
                "text",
                "thread_id",
            ]
        );
    }

    #[test]
    fn an_empty_or_malformed_payload_is_no_rows_rather_than_a_panic() {
        // A thread with nothing in it is ordinary. A payload this build cannot read is not, and
        // it still must not take the stream down.
        assert!(rows_from_payload(&json!({ "items": [] })).is_empty());
        assert!(rows_from_payload(&json!({})).is_empty());
        assert!(rows_from_payload(&Value::Null).is_empty());
        assert!(rows_from_payload(&json!({ "items": [ { "kind": "user_message" } ] })).is_empty());
    }

    // --- the live events ------------------------------------------------------------------------

    fn live(name: &str, extra: Value) -> (&'static str, Value) {
        let mut payload = json!({
            "entityId": "femcboost",
            "threadId": "thr_5c1e",
            "turnId": "t1",
            "bindingRevision": 1,
            "visibility": "ceo",
            "messageId": "msg_1",
            "at": 1_758_200_000_300u64,
        });
        for (k, v) in extra.as_object().unwrap() {
            payload[k] = v.clone();
        }
        event_from_live(name, &payload, 7).expect("the event was dropped")
    }

    /// **The phone's own id for its message comes back on the Mac's row for it** (adoption
    /// ledger §2.8 row C5). Both roads: the projected row (a hello or backfill) and the live
    /// `ceo-message` (the echo that can beat the phone's receipt). Before this every CEO row
    /// said `client_id: null`, so the iPhone could not tie an early echo of a voice note to
    /// the recording and drew two lines (`EchoBeforeAcceptanceTests`, known issue since
    /// `bf08f0a64`). Positive controls: a row the phone did not send, and a value that is
    /// not a string, are `null`, the shape every older Mac sends.
    #[test]
    fn a_ceo_row_carries_the_phones_own_id_when_a_phone_sent_it_and_null_otherwise() {
        let mut projected = payload();
        projected["items"][0]["clientId"] = json!("01JPHONE");
        projected["items"][5]["clientId"] = json!(7);
        let rows = rows_from_payload(&projected);
        assert_eq!(rows[0]["client_id"], "01JPHONE", "the projected row for a phone message");
        assert_eq!(rows[2]["client_id"], Value::Null, "a value that is not a string is never an id");
        assert_eq!(rows_from_payload(&payload())[0]["client_id"], Value::Null, "a message the phone did not send");

        let at = json!({ "text": "call the bank", "source": "text", "createdAt": 1_758_200_000_000u64 });
        let mut from_phone = at.clone();
        from_phone["clientId"] = json!("v1");
        let (name, row) = live("rich://ceo-message", from_phone);
        assert_eq!(name, "message");
        assert_eq!((row["id"].as_str(), row["role"].as_str()), (Some("msg_1"), Some("ceo")));
        assert_eq!(row["client_id"], "v1", "the live row for a phone message, which can beat its receipt");
        let (_, row) = live("rich://ceo-message", at);
        assert_eq!(row["client_id"], Value::Null, "typed at the Mac: no id to give");
    }

    /// **A phone voice note is a voice row from its first appearance on the phone stream.** The
    /// live `ceo-message` is the row that can beat the phone's receipt; it used to say
    /// `kind: "text"` for a voice note (only `jam` made a voice row), so the phone drew the
    /// transcript as a text bubble until the receipt arrived. The fixture is the shape
    /// `LiveEvent::CeoMessage::payload` writes for a voice note (`source: "text"`, `clientId`,
    /// `voiceNote: true`), and the projected item (hello, backfill, history after a reconnect)
    /// is checked by the same rule. Positive controls: a typed phone message and a malformed
    /// flag stay text, and `from_microphone` stays the desk's voice mode only.
    #[test]
    fn a_phone_voice_note_is_a_voice_row_before_and_after_its_receipt_and_a_typed_one_is_text() {
        let note = json!({ "text": "call the bank", "source": "text", "clientId": "v1", "voiceNote": true,
                           "createdAt": 1_758_200_000_000u64 });
        let (name, row) = live("rich://ceo-message", note);
        assert_eq!(name, "message");
        assert_eq!(row["kind"], "voice", "the early echo of a voice note: {row}");
        assert_eq!(row["client_id"], "v1");
        assert_eq!(row["from_microphone"], false, "not the desk's voice mode; older phones see what they saw");

        let typed = json!({ "text": "call the bank", "source": "text", "clientId": "t1", "createdAt": 1u64 });
        assert_eq!(live("rich://ceo-message", typed).1["kind"], "text", "a typed phone message stays text");
        for not_a_flag in [json!(false), json!("true"), json!(1), Value::Null] {
            let odd = json!({ "text": "x", "source": "text", "voiceNote": not_a_flag.clone(), "createdAt": 1u64 });
            assert_eq!(live("rich://ceo-message", odd).1["kind"], "text", "voiceNote {not_a_flag} is not a voice note");
        }

        let mut projected = payload();
        projected["items"][0]["voiceNote"] = json!(true);
        let rows = rows_from_payload(&projected);
        assert_eq!(rows[0]["kind"], "voice", "the projected row of a voice note");
        assert_eq!(rows[0]["from_microphone"], false);
        assert_eq!(rows_from_payload(&payload())[0]["kind"], "text", "the same message typed");
    }

    #[test]
    fn a_message_opening_streaming_and_completing_becomes_the_three_things_the_phone_handles() {
        let (name, started) = live("rich://message-started", json!({ "phase": "unknown", "seq": 2 }));
        assert_eq!(name, "message");
        assert_eq!(started["complete"], false);
        assert_eq!(started["state"], "streaming");
        assert_eq!(started["text"], "");
        assert_eq!(started["cursor"], 7);
        assert_eq!(started["role"], "rich");

        let (name, delta) = live("rich://message-delta", json!({ "seq": 3, "textDelta": "On it" }));
        assert_eq!(name, "delta");
        assert_eq!(delta["message_id"], "msg_1");
        assert_eq!(delta["text"], "On it");
        assert_eq!(delta["cursor"], 7);
        // The conversation a delta belongs to, so a phone can drop one for another conversation
        // instead of growing it in the one on screen (contract 2026-09-22 §11 item 4).
        assert_eq!(delta["thread_id"], "thr_5c1e");
        assert_eq!(started["thread_id"], delta["thread_id"], "a delta and its row disagree about the conversation");
        let mut keys: Vec<&str> = delta.as_object().unwrap().keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["cursor", "message_id", "text", "thread_id"], "the delta's shape moved");

        // THE COMPLETION CARRIES THE WHOLE TEXT AGAIN, deliberately: a phone that came back
        // mid-reply and missed every delta is correct without a reload.
        let (name, done) =
            live("rich://message-completed", json!({ "phase": "unknown", "text": "On it! Here it is." }));
        assert_eq!(name, "message");
        assert_eq!(done["complete"], true);
        assert_eq!(done["text"], "On it! Here it is.");
        assert_eq!(done["thread_id"], "thr_5c1e");
    }

    #[test]
    fn the_events_the_phone_has_no_row_for_are_dropped_rather_than_sent_and_ignored() {
        // His cellular connection should not carry bytes nothing renders.
        let payload = json!({
            "threadId": "thr_5c1e", "messageId": "msg_1", "at": 1u64, "visibility": "ceo"
        });
        for name in [
            "rich://turn-status",
            "rich://activity-upserted",
            "rich://worker-upserted",
            "rich://thread-summary-updated",
            "rich://something-added-later",
        ] {
            assert!(event_from_live(name, &payload, 1).is_none(), "{name} was forwarded");
        }
        // POSITIVE CONTROL: the three that ARE handled come through on the same payload.
        assert!(event_from_live("rich://message-started", &payload, 1).is_some());
        assert!(event_from_live("rich://message-delta", &payload, 1).is_some());
        assert!(event_from_live("rich://message-completed", &payload, 1).is_some());
        assert!(event_from_live("rich://ceo-message", &payload, 1).is_some());
    }

    #[test]
    fn his_own_message_crosses_as_a_ceo_row_carrying_the_turn_s_own_instant() {
        // `esc-20260919T003541Z-6885f74b`. The two fields the phone merges on are `id` and
        // `created_at`, and both are the PROJECTION'S — `{turn}:user` and the turn's own
        // instant — so a backfill of the same message is the same row rather than a second one.
        let payload = json!({
            "threadId": "thr_5c1e", "messageId": "turn_9:user", "text": "land the pricing branch",
            "source": "text", "createdAt": 1_758_200_400_000u64, "at": 1_758_200_400_500u64,
            "visibility": "ceo"
        });
        let (kind, row) = event_from_live("rich://ceo-message", &payload, 7).expect("his row");
        assert_eq!(kind, "message");
        assert_eq!(row["role"], "ceo");
        assert_eq!(row["kind"], "text");
        assert_eq!(row["from_microphone"], false);
        assert_eq!(row["text"], "land the pricing branch");
        assert_eq!(row["id"], "turn_9:user");
        assert_eq!(row["cursor"], 7);
        assert_eq!(row["state"], "sent");
        assert_eq!(row["complete"], true);
        // The TURN's instant, not the event's: two timestamps for one message is two orderings.
        assert_eq!(row["created_at"], "2025-09-18T13:00:00.000Z");

        // A SPOKEN turn takes the microphone marker, by the same rule `row_from_item` applies
        // to a projected one — `jam` and nothing else.
        let spoken = json!({
            "threadId": "thr_5c1e", "messageId": "turn_9:user", "text": "land it",
            "source": "jam", "createdAt": 1u64, "at": 1u64, "visibility": "ceo"
        });
        let (_, voice) = event_from_live("rich://ceo-message", &spoken, 1).expect("his spoken row");
        assert_eq!(voice["kind"], "voice");
        assert_eq!(voice["from_microphone"], true);
    }

    #[test]
    fn only_an_opening_message_consumes_a_cursor() {
        assert!(opens_a_row("rich://message-started"));
        // His OWN message opens one too — a row in the projection is a row here, or the live
        // and reloaded orderings drift by one from the first thing he ever says.
        assert!(opens_a_row("rich://ceo-message"));
        assert!(!opens_a_row("rich://message-delta"));
        assert!(!opens_a_row("rich://message-completed"));
        assert!(!opens_a_row("rich://turn-status"));
    }

    #[test]
    fn an_event_with_no_message_id_is_dropped_rather_than_given_an_empty_one() {
        let payload = json!({ "threadId": "t", "at": 1u64 });
        assert!(event_from_live("rich://message-started", &payload, 1).is_none());
    }

    // --- the timestamp ---------------------------------------------------------------------------

    #[test]
    fn the_timestamp_is_iso_8601_and_the_arithmetic_is_checked_against_known_dates() {
        // Known answers rather than a round trip against itself: a date function that is wrong
        // is wrong in both directions and a round trip cannot tell.
        assert_eq!(iso8601(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(iso8601(1), "1970-01-01T00:00:00.001Z");
        assert_eq!(iso8601(86_399_999), "1970-01-01T23:59:59.999Z");
        assert_eq!(iso8601(86_400_000), "1970-01-02T00:00:00.000Z");
        // A leap day, which is the one the algorithm exists for.
        assert_eq!(iso8601(951_782_400_000), "2000-02-29T00:00:00.000Z");
        // 2100 is NOT a leap year — the century rule, which a naive "every four years" gets
        // wrong and which no test before 2100 would otherwise catch.
        //
        // EVERY VALUE IN THIS TEST WAS READ OFF `date -u -r <seconds>` RATHER THAN TYPED, and
        // that is not ceremony: the first draft of this line asserted `2100-03-01` from memory
        // and `date -u -r 4107456000` said `2100-02-28`. A date function checked against a
        // number somebody remembered is a date function checked against nothing.
        assert_eq!(iso8601(4_107_456_000_000), "2100-02-28T00:00:00.000Z");
        assert_eq!(iso8601(1_758_200_400_000), "2025-09-18T13:00:00.000Z");
    }
}

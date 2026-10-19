//! **THE ROAD OUT — one stream, and the gate it inherits.** Plan §4.2 (iii), and the plan calls
//! this property *"the single most important … and the cheapest to get right"*:
//!
//! > *"`LiveObserver` is a single-method, non-blocking, infallible-from-the-spine trait, and the
//! > spine's `forward_live` chokepoint hands it **only `Visibility::Ceo` items**. So a phone
//! > emitter placed beside `TauriLiveEmitter` cannot leak a tool call, a file path or an internal
//! > turn **even by mistake** — the bytes never reach it."*
//!
//! [`PhoneLiveEmitter`] is that emitter. It holds two handles — a [`PhoneHub`] and nothing else —
//! and it has **no path to the ledger, to the raw event stream or to a `Timeline`.** That is not a
//! rule this file promises to follow; it is the only thing it has.
//!
//! # The wire shape is the landed phone's
//!
//! **RECONCILED 2026-09-18.** `web/web-app/lib/api.js` shipped first and subscribes to exactly five
//! event names — `hello`, `message`, `delta`, `state`, `heartbeat` — and merges on an integer
//! cursor. So the §13 live family is translated into those ([`super::rows`]) rather than forwarded
//! verbatim, and the cursor is an integer rather than the `<boot>-<seq>` string an earlier draft
//! of the contract specified. The translation reads only already-gated JSON, so §4.2 (iii) is
//! unaffected by it.

use super::rows::{event_from_live, opens_a_row};
use richos_core::live::{LiveEvent, LiveObserver, EVENT_CEO_MESSAGE};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;

/// How many recent frames the hub remembers, so a reconnection with `since=` can be answered
/// from memory. Beyond it the phone is sent a fresh `hello` and backfills — never a partial tail,
/// because a gap it cannot see is the one failure this design refuses.
const REPLAY_MEMORY: usize = 512;

/// How many frames a slow consumer may fall behind before it is told to start again.
const BROADCAST_DEPTH: usize = 256;

/// One thing to send down an open event stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// The SSE `id:`. The phone orders rows by the `cursor` inside the data, and presents the last
    /// `id:` it heard as `since=` when it reconnects (`web-app/app.js` `makeLink`): a row's cursor
    /// can be a projection position (`mod.rs` `question_changed`), never a place in this stream.
    pub cursor: u64,
    /// The SSE `event:` name: one of the five the phone subscribes to.
    pub kind: &'static str,
    /// The SSE `data:` — one line of JSON.
    pub data: String,
}

impl Frame {
    /// The wire form, exactly as SSE specifies it: field lines, then a blank line.
    ///
    /// `data` is one line because every payload here is compact JSON with no literal newline in
    /// it — serde never emits one — and a multi-line `data` would silently split the event in
    /// two. There is a test that proves the invariant rather than assuming it.
    pub fn to_wire(&self) -> String {
        format!("id: {}\nevent: {}\ndata: {}\n\n", self.cursor, self.kind, self.data)
    }
}

/// What a reconnecting client should be sent.
#[derive(Debug, PartialEq, Eq)]
pub enum Replay {
    /// Exactly the frames after the cursor it presented.
    Tail(Vec<Frame>),
    /// A fresh `hello`: no cursor, a cursor we no longer hold, or a cursor this run never issued.
    Hello,
}

/// The fan-out. One per running channel.
///
/// **It is inert until a listener runs** — an emitter attached at boot with no channel simply
/// drops what it is given, which is what makes it safe to install the emitter once, at start-up,
/// rather than reaching into the spine when a phone pairs.
pub struct PhoneHub {
    state: Mutex<HubState>,
    tx: broadcast::Sender<Frame>,
}

struct HubState {
    /// The last cursor issued. Seeded from the gated projection's row count whenever a `hello`
    /// is built, so live cursors continue the projection rather than starting a second sequence.
    cursor: u64,
    recent: VecDeque<Frame>,
    live: bool,
    /// How many frames this hub has ever published. `recent` is always the newest
    /// `recent.len()` of them, so this number names a position in the stream that stays true after
    /// the buffer rolls: the first retained frame is number `published - recent.len()`.
    published: u64,
}

impl PhoneHub {
    pub fn new() -> Arc<Self> {
        let (tx, _rx) = broadcast::channel(BROADCAST_DEPTH);
        Arc::new(PhoneHub {
            state: Mutex::new(HubState { cursor: 0, recent: VecDeque::new(), live: false, published: 0 }),
            tx,
        })
    }

    /// Start or stop accepting events. Called when the listener starts and stops.
    pub fn set_live(&self, live: bool) {
        let mut state = self.state.lock().unwrap();
        state.live = live;
        if !live {
            // Nothing that happened while a phone was paired is kept once it is not. The ledger
            // is the record; this buffer is only ever a reconnection convenience, and his words
            // must not sit in memory after "Forget this phone".
            state.recent.clear();
        }
    }

    /// Whether a listener is running. Read by the tests that prove the hub is inert before one is,
    /// which is the property plan §2.5 item 1 turns into "off means no socket".
    #[allow(dead_code)]
    pub fn is_live(&self) -> bool {
        self.state.lock().unwrap().live
    }

    /// Set the cursor from the gated projection's row count. Called whenever a `hello` is built,
    /// so a drift between the live sequence and what a reload computes cannot survive one
    /// reconnection.
    pub fn seed_cursor(&self, rows: u64) {
        let mut state = self.state.lock().unwrap();
        if rows > state.cursor {
            state.cursor = rows;
        }
    }

    pub fn cursor_now(&self) -> u64 {
        self.state.lock().unwrap().cursor
    }

    /// Subscribe without a position. Tests only: a real stream subscribes with
    /// [`subscribe_from`](Self::subscribe_from), which cannot lose the frames published before it.
    #[cfg(test)]
    pub fn subscribe(&self) -> broadcast::Receiver<Frame> {
        self.tx.subscribe()
    }

    /// Publish one frame under a cursor the caller has decided. Returns the frame, or `None`
    /// when no listener is running.
    pub fn publish(&self, kind: &'static str, cursor: u64, data: String) -> Option<Frame> {
        let mut state = self.state.lock().unwrap();
        if !state.live {
            return None;
        }
        // The high-water mark follows what was actually published, so `replay_after` can tell a
        // cursor this run issued from one it never did. A test caught the version that did not do
        // this: every reconnection was answered with a whole `hello`, because the hub believed it
        // had issued nothing.
        if cursor > state.cursor {
            state.cursor = cursor;
        }
        let frame = Frame { cursor, kind, data };
        state.recent.push_back(frame.clone());
        state.published += 1;
        while state.recent.len() > REPLAY_MEMORY {
            state.recent.pop_front();
        }
        // **SENT WHILE THE LOCK IS STILL HELD**, so that "recorded in `recent`" and "delivered to
        // every subscriber that exists" happen as one step. [`PhoneHub::subscribe_from`] takes
        // the same lock to read `recent` and to subscribe, so a frame is either in what it read
        // or in what it will receive, never in neither and never in both (hunt 2026-09-29, part 1,
        // finding 13). `send` does not block: the channel is bounded and drops its oldest.
        //
        // A send with no subscribers is not an error: the phone is not always looking, and
        // §13's rule that "missed UI events never block the spine" applies to this observer
        // exactly as it applies to the webview's.
        let _ = self.tx.send(frame.clone());
        drop(state);
        Some(frame)
    }

    /// Take the next cursor, for a frame that opens a new row.
    pub fn next_cursor(&self) -> u64 {
        let mut state = self.state.lock().unwrap();
        state.cursor += 1;
        state.cursor
    }

    /// What to send a client that presented `since` (from the query, or from `Last-Event-ID`).
    ///
    /// Tests only: a real stream asks [`replay_after_marked`](Self::replay_after_marked), because
    /// it needs the position as well as the answer.
    #[cfg(test)]
    pub fn replay_after(&self, since: Option<u64>) -> Replay {
        self.replay_after_marked(since).0
    }

    /// [`replay_after`](Self::replay_after), and the position in the stream the answer is exact
    /// up to. Hand that position to [`subscribe_from`](Self::subscribe_from) when the stream
    /// actually opens: everything published in between is then delivered, once.
    ///
    /// The position is read under the same lock as the answer, and BEFORE the caller builds a
    /// `hello` from the ledger, so a `hello` can only be older than the position, never newer.
    pub fn replay_after_marked(&self, since: Option<u64>) -> (Replay, u64) {
        let state = self.state.lock().unwrap();
        let mark = state.published;
        let Some(since) = since else { return (Replay::Hello, mark) };
        if !state.live {
            // No channel is running, so there is nothing to be the tail OF. Without this, a
            // buffer just cleared by an unpair would answer "you are up to date" about a stream
            // that no longer exists — a test caught exactly that.
            return (Replay::Hello, mark);
        }
        if since > state.cursor {
            // A cursor from the future: this run never issued it.
            return (Replay::Hello, mark);
        }
        let oldest_held = state.recent.front().map(|f| f.cursor);
        match oldest_held {
            Some(oldest) if oldest <= since + 1 => {
                // **A CURSOR NAMES A ROW, NOT A FRAME** (hunt 2026-09-29, part 1, finding 12):
                // a reply's opening, its deltas and its completion all carry the one cursor. A
                // phone that saw the opening and dropped presents that cursor, and "everything
                // after it" would be nothing, leaving the reply stuck at its first word. So the
                // row AT the cursor is caught up first, unless it is plainly finished.
                let at_cursor: Vec<Frame> = state.recent.iter().filter(|f| f.cursor == since).cloned().collect();
                let mut frames = match catch_up_row(at_cursor, &state.recent) {
                    Some(frames) => frames,
                    // Part of that row is gone from the buffer; the truth is better than a gap.
                    None => return (Replay::Hello, mark),
                };
                frames.extend(state.recent.iter().filter(|f| f.cursor > since).cloned());
                (Replay::Tail(frames), mark)
            }
            // The buffer has rolled past what it asked for. A partial tail would be a silent
            // gap, so it gets the truth instead.
            Some(_) => (Replay::Hello, mark),
            None if since == state.cursor => (Replay::Tail(Vec::new()), mark),
            None => (Replay::Hello, mark),
        }
    }

    /// **Subscribe at a position, losing nothing and repeating nothing** (hunt 2026-09-29, part 1,
    /// finding 13). `mark` is what [`replay_after_marked`](Self::replay_after_marked) returned.
    /// The frames published since then are returned, and the receiver that follows them is
    /// subscribed under the same lock that publishing takes, so the two halves meet exactly.
    ///
    /// The returned frames are made safe to repeat (see [`coalesce_rows`]), because the opening a
    /// `hello` was built from may already include some of them.
    ///
    /// `Err` means the position can no longer be answered: the buffer rolled past it, or a delta
    /// has no row to fold into. The caller tells the phone to start again.
    pub fn subscribe_from(&self, mark: u64) -> Result<(Vec<Frame>, broadcast::Receiver<Frame>), ()> {
        let state = self.state.lock().unwrap();
        let first_held = state.published - state.recent.len() as u64;
        if mark < first_held || mark > state.published {
            return Err(());
        }
        let missed: Vec<Frame> = state.recent.iter().skip((mark - first_held) as usize).cloned().collect();
        let missed = coalesce_rows(missed, &state.recent).ok_or(())?;
        Ok((missed, self.tx.subscribe()))
    }
}

/// The frames that bring a phone whose last cursor is `frames`' row up to date, or `None` when
/// the row cannot be reconstructed from what is held.
///
/// For a row that is finished and was sent whole (his own message, a stand-in, a question card),
/// only the conversation lists at that cursor; otherwise the row's current state as
/// [`coalesce_rows`] builds it, which passes those lists through as well.
///
/// **A LIST TAKES NO CURSOR, SO IT SHARES THE NEWEST ROW'S** (`mod.rs` `threads_changed`; review
/// `rv-20261019T115102Z-1f88727c-a59d`): a conversation made on the Mac while the phone was away
/// goes out under the number the phone comes back with. A list replaces the phone's list whole,
/// so sending one the phone already had is harmless; leaving it out loses the conversation.
fn catch_up_row(frames: Vec<Frame>, history: &VecDeque<Frame>) -> Option<Vec<Frame>> {
    let still_arriving = frames.iter().any(|f| f.kind == "delta" || is_incomplete_row(f));
    if !still_arriving {
        return Some(frames.into_iter().filter(|f| f.kind == "threads").collect());
    }
    coalesce_rows(frames, history)
}

fn data_of(frame: &Frame) -> serde_json::Value {
    serde_json::from_str(&frame.data).unwrap_or_default()
}

/// A `message` frame that says its row is still being written.
fn is_incomplete_row(frame: &Frame) -> bool {
    frame.kind == "message" && data_of(frame)["complete"] == false
}

/// **Make a run of frames safe to apply to a phone that may already have some of them.**
///
/// The phone APPENDS a delta to its row and REPLACES a row by id. So a delta sent twice prints
/// twice, while a whole row sent twice is harmless. For each reply that appears in `frames`:
///
/// * if its completion is there, the completion (the whole text, by design) stands alone;
/// * otherwise its opening and deltas are folded into ONE row holding everything held for it so
///   far, which `history` (the hub's buffer) supplies, placed where the reply first appeared.
///
/// `None` when a delta belongs to a reply whose opening is no longer held.
fn coalesce_rows(frames: Vec<Frame>, history: &VecDeque<Frame>) -> Option<Vec<Frame>> {
    use std::collections::{HashMap, HashSet};
    let id_of = |f: &Frame| -> Option<String> {
        let data = data_of(f);
        match f.kind {
            "delta" => data["message_id"].as_str().map(str::to_string),
            "message" => data["id"].as_str().map(str::to_string),
            _ => None,
        }
    };
    // The replies this run of frames touches while they are still being written.
    let mut streaming: Vec<String> = Vec::new();
    let mut finished: HashSet<String> = HashSet::new();
    for f in &frames {
        let Some(id) = id_of(f) else { continue };
        if f.kind == "message" && data_of(f)["complete"] == true {
            finished.insert(id);
        } else if (f.kind == "delta" || is_incomplete_row(f)) && !streaming.contains(&id) {
            streaming.push(id);
        }
    }
    let mut folded: HashMap<String, Frame> = HashMap::new();
    for id in streaming.iter().filter(|id| !finished.contains(*id)) {
        // The row as it was opened, and every delta held for it since.
        let opening = history.iter().find(|f| is_incomplete_row(f) && id_of(f).as_deref() == Some(id.as_str()))?;
        let text: String = history
            .iter()
            .filter(|f| f.kind == "delta" && id_of(f).as_deref() == Some(id.as_str()))
            .map(|f| data_of(f)["text"].as_str().unwrap_or("").to_string())
            .collect();
        let mut row = data_of(opening);
        row["text"] = serde_json::Value::String(text);
        folded.insert(id.clone(), Frame { cursor: opening.cursor, kind: "message", data: row.to_string() });
    }
    let mut out: Vec<Frame> = Vec::new();
    for f in frames {
        let Some(id) = id_of(&f) else { out.push(f); continue };
        let streaming_part = f.kind == "delta" || is_incomplete_row(&f);
        if !streaming_part {
            out.push(f);
        } else if finished.contains(&id) {
            // The completion carries the whole text; the pieces before it add nothing.
        } else if let Some(row) = folded.remove(&id) {
            out.push(row);
        }
        // else: this reply's folded row was already placed at its first appearance.
    }
    Some(out)
}

/// **The phone's emitter, beside `TauriLiveEmitter`.**
///
/// It receives only what `Spine::forward_live` allows through — `Visibility::Ceo` items — and it
/// holds nothing else. Same posture as the webview's emitter: best-effort, infallible from the
/// spine's view, never able to stall or fail a turn.
pub struct PhoneLiveEmitter {
    hub: Arc<PhoneHub>,
    /// The cursor of the message currently streaming, so its deltas and its completion carry the
    /// same one. A `Mutex<Option<u64>>` rather than a field on the hub, because it is per-emitter
    /// bookkeeping and not part of what any stream reads.
    open_row: Mutex<Option<u64>>,
}

impl PhoneLiveEmitter {
    pub fn new(hub: Arc<PhoneHub>) -> Self {
        PhoneLiveEmitter { hub, open_row: Mutex::new(None) }
    }
}

impl LiveObserver for PhoneLiveEmitter {
    fn on_live_event(&self, event: &LiveEvent) {
        let name = event.event_name();
        let cursor = if opens_a_row(name) {
            let next = self.hub.next_cursor();
            *self.open_row.lock().unwrap() = Some(next);
            next
        } else {
            // A delta or a completion belongs to the row that is open. If none is — the stream
            // opened mid-reply — the current cursor is the honest answer: it names the newest
            // row, which is the one being written.
            self.open_row.lock().unwrap().unwrap_or_else(|| self.hub.cursor_now())
        };
        // The translation reads the event's already-gated payload and nothing else. This layer
        // resolves no scope, builds no payload from the ledger and makes no visibility decision.
        if let Some((kind, data)) = event_from_live(name, &event.payload(), cursor) {
            let _ = self.hub.publish(kind, cursor, data.to_string());
        }
    }
}

/// **HIS OWN SENTENCE, ON HIS PHONE, BEFORE THE SPINE HAS BEEN ASKED FOR ANYTHING** — Ray's
/// `.20260920.1` defect 1.
///
/// # The measurement this exists for
///
/// His typed message reached the phone 1.3-6.1 s after he pressed Send on four of five turns,
/// while the REPLY to the same message crossed in under a second. Both legs were always on
/// this one path and the path is not slow: `phone::listen`'s two timing tests measure the leg
/// at **5 ms** over real TLS, and then measure it again with the spine held for 1,500 ms and
/// get **1,507 ms**. The seconds were never on the wire; they were spent waiting for
/// `take_the_spine` in `send_message`, because until the window has that mutex his words
/// cannot reach the ledger (`Ledger::record_prompt_received` is `&mut self`) and until they
/// reach the ledger there is nothing for [`PhoneLiveEmitter`] to publish.
///
/// # What is announced, and why it is not a claim the Mac has not made
///
/// The caller writes his words to the DURABLE intake log first, without any lock, and hands
/// the record's own id here. So this is never "the phone showing something the Mac has not
/// accepted" — plan §6's refusal — it is the phone showing something the Mac has written to
/// disk and will turn into a turn, in order, the moment the spine is free. That is exactly
/// the guarantee the phone's OWN messages have had since `bridge.rs` was written.
///
/// # And it is the same row the projection will send
///
/// Built by handing [`super::rows::event_from_live`] the payload `rich://ceo-message` carries,
/// so there is ONE definition of a CEO row on this wire and not a second one that can drift.
/// The id is the intake id — `intake_<n>`, the same id `POST /api/messages` answers a phone
/// with — and the projected row that follows carries `{turn}:user`. The page retires the
/// stand-in when the projected row arrives (`web/web-app/lib/thread.js`, `remember`), which is
/// the same retire-on-the-Mac's-own-row rule it has used for a phone's pending bubble since
/// the send queue was written.
pub fn announce_his_words(
    hub: &PhoneHub,
    thread_id: &str,
    message_id: &str,
    text: &str,
    at: u64,
) -> Option<Frame> {
    let cursor = hub.next_cursor();
    let payload = serde_json::json!({
        "messageId": message_id,
        "threadId": thread_id,
        "text": text,
        // Typed, always: this road is the desktop composer and nothing else writes to it.
        "source": "text",
        "createdAt": at,
        "at": at,
    });
    let (kind, data) = event_from_live(EVENT_CEO_MESSAGE, &payload, cursor)?;
    hub.publish(kind, cursor, data.to_string())
}

/// **Fan one live stream out to several observers.**
///
/// `Spine::set_live_observer` takes ONE observer, and there are now two that want it: the
/// webview's and the phone's. Rather than change the spine's shape, this forwards to both — and
/// it forwards the SAME `&LiveEvent`, after the same gate, so neither observer can see anything
/// the other cannot.
pub struct FanOutLiveEmitter {
    observers: Vec<Box<dyn LiveObserver>>,
}

impl FanOutLiveEmitter {
    pub fn new(observers: Vec<Box<dyn LiveObserver>>) -> Self {
        FanOutLiveEmitter { observers }
    }
}

impl LiveObserver for FanOutLiveEmitter {
    fn on_live_event(&self, event: &LiveEvent) {
        for observer in &self.observers {
            observer.on_live_event(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live_hub() -> Arc<PhoneHub> {
        let hub = PhoneHub::new();
        hub.set_live(true);
        hub
    }

    #[test]
    fn nothing_is_published_until_a_listener_is_running() {
        // Plan §2.5 item 1 seen from the inside: the emitter can be installed at boot and does
        // nothing at all until a phone is paired. That is why there is no second code path for
        // "attach the emitter later".
        let hub = PhoneHub::new();
        assert!(!hub.is_live());
        assert_eq!(hub.publish("message", 1, "{}".into()), None);
        assert_eq!(hub.cursor_now(), 0);
        hub.set_live(true);
        assert!(hub.publish("message", 1, "{}".into()).is_some());
    }

    #[test]
    fn the_cursor_is_minted_by_the_mac_and_continues_the_projection() {
        // Plan §4.2 (vi): "server-issued, never client-generated ordering". And it CONTINUES the
        // gated projection's row count rather than starting a second sequence, which is what
        // makes a live cursor and a reloaded one mean the same thing.
        let hub = live_hub();
        hub.seed_cursor(12);
        assert_eq!(hub.cursor_now(), 12);
        assert_eq!(hub.next_cursor(), 13);
        assert_eq!(hub.next_cursor(), 14);
        // Seeding never moves it backwards: a stale projection must not rewind a live stream.
        hub.seed_cursor(3);
        assert_eq!(hub.cursor_now(), 14);
    }

    #[test]
    fn a_reconnection_gets_exactly_its_tail() {
        let hub = live_hub();
        hub.publish("message", 1, "{\"n\":1}".into()).unwrap();
        hub.publish("message", 2, "{\"n\":2}".into()).unwrap();
        hub.publish("message", 3, "{\"n\":3}".into()).unwrap();
        match hub.replay_after(Some(1)) {
            Replay::Tail(frames) => {
                assert_eq!(frames.len(), 2);
                assert_eq!(frames[0].data, "{\"n\":2}");
                assert_eq!(frames[1].data, "{\"n\":3}");
            }
            other => panic!("expected a tail, got {other:?}"),
        }
    }

    #[test]
    fn a_client_that_is_already_up_to_date_gets_an_empty_tail_and_not_a_whole_hello() {
        // The commonest case on a flaky Wi-Fi: the stream dropped after the last event. A
        // `hello` there would redraw his thread for no reason.
        let hub = live_hub();
        hub.seed_cursor(4);
        hub.publish("message", 5, "{}".into()).unwrap();
        assert_eq!(hub.replay_after(Some(5)), Replay::Tail(Vec::new()));
    }

    #[test]
    fn a_conversation_list_sharing_the_resume_cursor_still_reaches_a_reconnecting_phone() {
        // Review `rv-20261019T115102Z-1f88727c-a59d` finding 1: a question card takes a fresh
        // cursor and is sent whole; the phone drops; a conversation is created on the Mac and
        // its list goes out under `cursor_now()`, the question's own number (`mod.rs`
        // `threads_changed`). The phone comes back presenting that number. The list must be in
        // the tail, or the new conversation is missing from its picker.
        let hub = live_hub();
        hub.seed_cursor(9);
        let cursor = hub.next_cursor();
        let question = serde_json::json!({ "id": "q", "cursor": cursor, "role": "rich", "kind": "question", "text": "Formal or casual?", "complete": true });
        hub.publish("message", cursor, question.to_string()).unwrap();
        let list = "{\"threads\":[{\"id\":\"A\"},{\"id\":\"C\"}]}".to_string();
        hub.publish("threads", hub.cursor_now(), list.clone()).unwrap();
        assert_eq!(hub.cursor_now(), cursor, "the list took no cursor of its own");
        match hub.replay_after(Some(cursor)) {
            // The question itself is not sent again: the phone already has it whole.
            Replay::Tail(frames) => assert_eq!(frames, vec![Frame { cursor, kind: "threads", data: list }]),
            other => panic!("expected a tail, got {other:?}"),
        }
    }

    /// Publishes one reply the way [`PhoneLiveEmitter`] does: the opening, two deltas and the
    /// completion, all under ONE cursor (a cursor names a row, not a frame).
    fn publish_one_reply(hub: &PhoneHub, cursor: u64, id: &str) {
        let opening = serde_json::json!({ "id": id, "cursor": cursor, "role": "rich", "text": "", "complete": false });
        hub.publish("message", cursor, opening.to_string()).unwrap();
        for piece in ["Here is ", "the answer."] {
            let delta = serde_json::json!({ "message_id": id, "cursor": cursor, "text": piece });
            hub.publish("delta", cursor, delta.to_string()).unwrap();
        }
        let done = serde_json::json!({ "id": id, "cursor": cursor, "role": "rich", "text": "Here is the answer.", "complete": true });
        hub.publish("message", cursor, done.to_string()).unwrap();
    }

    fn texts(replay: Replay) -> Vec<(String, String, bool)> {
        match replay {
            Replay::Tail(frames) => frames
                .iter()
                .map(|f| {
                    let v: serde_json::Value = serde_json::from_str(&f.data).unwrap();
                    (f.kind.to_string(), v["text"].as_str().unwrap_or("").to_string(), v["complete"] == true)
                })
                .collect(),
            other => panic!("expected a tail, got {other:?}"),
        }
    }

    /// **A RECONNECTION IN THE MIDDLE OF A REPLY GETS THE REST OF IT** (hunt 2026-09-29, part 1,
    /// finding 12). A cursor names a ROW, so the opening, every delta and the completion of one
    /// reply share it. A phone that saw the opening (cursor 7) and dropped used to present
    /// `since=7` and be told it was up to date, with the reply stuck at its first word.
    #[test]
    fn a_reconnection_partway_through_a_reply_gets_the_rest_of_that_reply() {
        let hub = live_hub();
        hub.seed_cursor(6);
        publish_one_reply(&hub, 7, "reply-7");

        let got = texts(hub.replay_after(Some(7)));
        assert!(!got.is_empty(), "since=7 was answered as up to date, but the reply at cursor 7 was still arriving");
        // Whatever shape the catch-up takes, the phone must end with the whole reply, complete.
        let (_, text, complete) = got.last().unwrap();
        assert_eq!((text.as_str(), *complete), ("Here is the answer.", true), "{got:?}");

        // And the phone that had everything is still told nothing NEW happened after it.
        assert_eq!(texts(hub.replay_after(Some(6))).last().unwrap().1, "Here is the answer.");
    }

    /// A reply still being written: the catch-up is ONE row holding everything so far, not the raw
    /// deltas, because the phone APPENDS a delta to its row and a delta it already applied would
    /// print twice.
    #[test]
    fn a_reconnection_during_a_live_reply_is_caught_up_without_repeating_a_delta() {
        let hub = live_hub();
        hub.seed_cursor(6);
        let opening = serde_json::json!({ "id": "r7", "cursor": 7, "role": "rich", "text": "", "complete": false });
        hub.publish("message", 7, opening.to_string()).unwrap();
        for piece in ["Here is ", "the "] {
            let delta = serde_json::json!({ "message_id": "r7", "cursor": 7, "text": piece });
            hub.publish("delta", 7, delta.to_string()).unwrap();
        }
        let got = texts(hub.replay_after(Some(7)));
        assert_eq!(got, vec![("message".to_string(), "Here is the ".to_string(), false)], "{got:?}");
    }

    /// The other half of finding 12's reason: a phone whose last cursor is a finished ROW OF HIS
    /// (one frame, nothing more to come) is still told it is up to date.
    #[test]
    fn a_finished_single_frame_row_leaves_nothing_to_replay() {
        let hub = live_hub();
        let his = serde_json::json!({ "id": "t:user", "cursor": 3, "role": "ceo", "text": "hi", "complete": true });
        hub.publish("message", 3, his.to_string()).unwrap();
        assert_eq!(hub.replay_after(Some(3)), Replay::Tail(Vec::new()));
    }

    /// Finding 13, at the hub: what is published between answering the route and subscribing is
    /// handed over once, and what follows comes through the receiver.
    #[test]
    fn subscribing_from_a_position_loses_nothing_and_repeats_nothing() {
        let hub = live_hub();
        hub.publish("message", 1, "{\"n\":1}".into()).unwrap();
        let (_, mark) = hub.replay_after_marked(Some(1));
        hub.publish("message", 2, "{\"n\":2}".into()).unwrap();
        let (missed, mut rx) = hub.subscribe_from(mark).unwrap();
        assert_eq!(missed.iter().map(|f| f.data.as_str()).collect::<Vec<_>>(), vec!["{\"n\":2}"]);
        assert!(rx.try_recv().is_err(), "the frame handed over as missed was also delivered live");
        hub.publish("message", 3, "{\"n\":3}".into()).unwrap();
        assert_eq!(rx.try_recv().unwrap().data, "{\"n\":3}");
    }

    /// A `hello` built from the ledger after the position may already hold part of a reply that
    /// is still arriving; handing over its raw deltas would append them twice. They are handed
    /// over folded into one row, which replaces instead of appending.
    #[test]
    fn frames_handed_over_after_a_hello_are_safe_to_apply_twice() {
        let hub = live_hub();
        let (_, mark) = hub.replay_after_marked(None);
        let opening = serde_json::json!({ "id": "r1", "cursor": 1, "role": "rich", "text": "", "complete": false });
        hub.publish("message", 1, opening.to_string()).unwrap();
        for piece in ["One ", "two"] {
            let delta = serde_json::json!({ "message_id": "r1", "cursor": 1, "text": piece });
            hub.publish("delta", 1, delta.to_string()).unwrap();
        }
        let (missed, _rx) = hub.subscribe_from(mark).unwrap();
        assert!(missed.iter().all(|f| f.kind != "delta"), "a raw delta would be appended twice: {missed:?}");
        let row: serde_json::Value = serde_json::from_str(&missed.last().unwrap().data).unwrap();
        assert_eq!((row["id"].as_str(), row["text"].as_str(), row["complete"].as_bool()), (Some("r1"), Some("One two"), Some(false)));
    }

    #[test]
    fn a_position_the_buffer_has_rolled_past_cannot_be_subscribed_from() {
        let hub = live_hub();
        let (_, mark) = hub.replay_after_marked(None);
        for n in 1..=REPLAY_MEMORY as u64 + 10 {
            hub.publish("message", n, "{}".into()).unwrap();
        }
        assert!(hub.subscribe_from(mark).is_err(), "a gap the phone cannot see was papered over");
        // POSITIVE CONTROL: a current position is fine.
        let (_, now) = hub.replay_after_marked(None);
        assert!(hub.subscribe_from(now).is_ok());
    }

    #[test]
    fn no_cursor_at_all_is_a_hello() {
        let hub = live_hub();
        hub.publish("message", 1, "{}".into()).unwrap();
        assert_eq!(hub.replay_after(None), Replay::Hello);
    }

    #[test]
    fn a_cursor_the_buffer_has_rolled_past_is_a_hello_and_never_a_partial_tail() {
        // A partial tail is a silent gap, which is the one thing this design refuses.
        let hub = live_hub();
        for n in 1..=REPLAY_MEMORY as u64 + 100 {
            hub.publish("message", n, format!("{{\"n\":{n}}}")).unwrap();
        }
        assert_eq!(hub.replay_after(Some(1)), Replay::Hello);
        // POSITIVE CONTROL: a cursor still inside the window gets a tail.
        let inside = REPLAY_MEMORY as u64 + 99;
        assert!(matches!(hub.replay_after(Some(inside)), Replay::Tail(f) if f.len() == 1));
    }

    #[test]
    fn a_cursor_this_run_never_issued_is_a_hello() {
        let hub = live_hub();
        hub.publish("message", 1, "{}".into()).unwrap();
        assert_eq!(hub.replay_after(Some(99_999)), Replay::Hello);
    }

    #[test]
    fn unpairing_leaves_nothing_of_the_conversation_in_the_hub() {
        let hub = live_hub();
        hub.publish("message", 1, "{\"words\":\"where are we on the proposal\"}".into()).unwrap();
        hub.set_live(false);
        assert_eq!(hub.replay_after(Some(1)), Replay::Hello);
        assert_eq!(hub.publish("message", 2, "{}".into()), None);
    }

    #[test]
    fn the_wire_form_is_server_sent_events_and_the_payload_is_one_line() {
        // A literal newline inside `data` would silently split the event in two on the wire.
        let frame = Frame { cursor: 4, kind: "message", data: "{\"a\":\"b\"}".into() };
        assert_eq!(frame.to_wire(), "id: 4\nevent: message\ndata: {\"a\":\"b\"}\n\n");

        let awkward = serde_json::json!({ "text": "a line\nand another\r\nand a tab\t" }).to_string();
        assert!(!awkward.contains('\n'), "serde emitted a literal newline: {awkward}");
        assert!(!awkward.contains('\r'), "serde emitted a literal carriage return: {awkward}");
        let framed = Frame { cursor: 1, kind: "delta", data: awkward }.to_wire();
        assert_eq!(framed.matches("\n\n").count(), 1, "the frame was split: {framed}");
    }

    #[test]
    fn an_open_stream_receives_what_is_published_after_it_subscribed() {
        let hub = live_hub();
        let mut rx = hub.subscribe();
        let published = hub.publish("message", 1, "{\"n\":1}".into()).unwrap();
        assert_eq!(rx.try_recv().unwrap(), published);
        assert!(rx.try_recv().is_err(), "a second frame arrived from nowhere");
    }

    #[test]
    fn publishing_with_nobody_listening_is_not_an_error() {
        // §13's "missed UI events never block the spine", inherited: a phone that is not looking
        // must never be able to fail a turn.
        let hub = live_hub();
        assert!(hub.publish("message", 1, "{}".into()).is_some());
        let rx = hub.subscribe();
        drop(rx);
        assert!(hub.publish("message", 2, "{}".into()).is_some());
    }

    // -----------------------------------------------------------------------------------
    // THE GATE, THROUGH A REAL SPINE
    // -----------------------------------------------------------------------------------
    //
    // `LiveEvent` CANNOT BE CONSTRUCTED FROM THIS CRATE, and that is the fence working:
    // `EventFence::for_turn` is `pub(crate)` to richos-core and takes a `ThreadBinding` only the
    // ledger can issue. So these tests drive a real `Spine` with a mock lease instead of
    // hand-building events — a stronger test anyway, because it exercises the actual
    // `forward_live` chokepoint rather than a stand-in for it.

    use richos_core::cognition::MockLeaseFactory;
    use richos_core::entity::{Entity, EntityId, EntityRegistry};
    use richos_core::ledger::{Ledger, Source};
    use richos_core::spine::Spine;
    use richos_core::timeline::Visibility;

    struct VisibilityRecorder(Arc<Mutex<Vec<Visibility>>>);
    impl LiveObserver for VisibilityRecorder {
        fn on_live_event(&self, event: &LiveEvent) {
            self.0.lock().unwrap().push(event.visibility());
        }
    }

    struct NameRecorder(Arc<Mutex<Vec<String>>>);
    impl LiveObserver for NameRecorder {
        fn on_live_event(&self, event: &LiveEvent) {
            self.0.lock().unwrap().push(event.event_name().to_string());
        }
    }

    fn spine_for(tag: &str, reply: &'static str) -> (std::path::PathBuf, Spine, String) {
        let path = std::env::temp_dir().join(format!(
            "richos-phone-stream-{tag}-{}-{}.jsonl",
            std::process::id(),
            super::super::now_millis()
        ));
        let _ = std::fs::remove_file(&path);
        let mut spine = Spine::new(Ledger::open(&path).unwrap());
        spine.set_entity_registry(
            EntityRegistry::new(vec![
                Entity::new("femcboost", "FemcBoost", &["/fixture/femcboost"]).unwrap()
            ])
            .unwrap(),
        );
        let entity = EntityId::parse("femcboost").unwrap();
        let thread = spine.create_thread("the proposal", &entity).unwrap();
        spine.switch_thread(&thread).unwrap();
        spine.set_lease_factory(Box::new(MockLeaseFactory::new(vec![reply])));
        (path, spine, thread)
    }

    #[test]
    fn the_phone_emitter_only_ever_receives_ceo_items_because_the_spine_gate_is_upstream_of_it() {
        // Plan §4.2 (iii), asserted rather than described.
        let seen = Arc::new(Mutex::new(Vec::new()));
        let hub = live_hub();
        let (path, mut spine, _thread) = spine_for("gate", "On it! Here is where we are.");
        spine.set_live_observer(Box::new(FanOutLiveEmitter::new(vec![
            Box::new(PhoneLiveEmitter::new(Arc::clone(&hub))),
            Box::new(VisibilityRecorder(Arc::clone(&seen))),
        ])));

        spine.submit_prompt("where are we on the proposal?", Source::Text).unwrap();

        let recorded = seen.lock().unwrap().clone();
        assert!(!recorded.is_empty(), "the emitter received nothing at all — the test proves nothing");
        assert!(
            recorded.iter().all(|v| *v == Visibility::Ceo),
            "a non-CEO item reached the phone emitter: {recorded:?}"
        );
        assert!(hub.cursor_now() > 0, "no frame reached the hub");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn the_fan_out_gives_the_webview_and_the_phone_the_same_events() {
        let a = Arc::new(Mutex::new(Vec::new()));
        let b = Arc::new(Mutex::new(Vec::new()));
        let (path, mut spine, _thread) = spine_for("fanout", "Two observers, one stream.");
        spine.set_live_observer(Box::new(FanOutLiveEmitter::new(vec![
            Box::new(NameRecorder(Arc::clone(&a))),
            Box::new(NameRecorder(Arc::clone(&b))),
        ])));
        spine.submit_prompt("say something", Source::Text).unwrap();
        assert!(!a.lock().unwrap().is_empty(), "the fan-out forwarded nothing");
        assert_eq!(*a.lock().unwrap(), *b.lock().unwrap(), "the two observers saw different streams");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_real_turn_becomes_the_events_the_landed_phone_subscribes_to() {
        // END TO END THROUGH THE SPINE: a real reply, translated, and nothing on the wire that
        // `web/web-app/lib/api.js` does not handle. Its listener list is
        // ['hello','message','delta','state','heartbeat'] — so anything else here would be bytes
        // his cellular connection carries for nothing.
        let hub = live_hub();
        let mut rx = hub.subscribe();
        let (path, mut spine, _thread) = spine_for("translated", "On it! Here is where we are.");
        spine.set_live_observer(Box::new(PhoneLiveEmitter::new(Arc::clone(&hub))));
        spine.submit_prompt("where are we?", Source::Text).unwrap();

        let mut kinds: Vec<&'static str> = Vec::new();
        let mut delta_text = String::new();
        let mut completed = String::new();
        while let Ok(frame) = rx.try_recv() {
            assert!(
                ["hello", "message", "delta", "state", "heartbeat"].contains(&frame.kind),
                "the phone does not subscribe to {}",
                frame.kind
            );
            let value: serde_json::Value = serde_json::from_str(&frame.data).unwrap();
            if frame.kind == "delta" {
                delta_text.push_str(value["text"].as_str().unwrap());
            }
            if frame.kind == "message" && value["complete"] == true {
                completed = value["text"].as_str().unwrap().to_string();
            }
            kinds.push(frame.kind);
        }
        assert!(kinds.contains(&"message"), "no message row was sent: {kinds:?}");
        assert_eq!(completed, "On it! Here is where we are.", "the completed row lost the reply");
        assert!(
            delta_text.is_empty() || completed.contains(delta_text.trim_end()),
            "the deltas and the completion disagree: {delta_text:?} vs {completed:?}"
        );
        let _ = std::fs::remove_file(path);
    }

    /// **A MESSAGE TYPED ON THE MAC REACHES THE PHONE'S STREAM, LIVE** —
    /// `esc-20260919T003541Z-6885f74b`, raised by `echo-opus-phonepage1` as the half of Ray's
    /// candidate-.11 defect 4.1 that could not be fixed on the page.
    ///
    /// The page could merge the rows it is GIVEN, and it now does; what it could not do is learn
    /// about his own sentence without asking for the rows again, because no event in the §13
    /// family was about the CEO. A turn that produced no reply left his own message off the
    /// phone until the next reload.
    ///
    /// Three things are asserted and each is a different way for this to be wrong:
    ///
    ///   1. a `message` frame with `role: "ceo"` and his words, BEFORE Rich's row;
    ///   2. its `id` is `{turn}:user` — the projection's own id for that message — so a phone
    ///      that backfills the same message merges it rather than drawing it twice;
    ///   3. it CONSUMED a cursor, so Rich's row sits at the next position rather than at his.
    #[test]
    fn a_message_typed_on_the_mac_reaches_the_phone_as_a_live_event() {
        let hub = live_hub();
        let mut rx = hub.subscribe();
        let (path, mut spine, _thread) = spine_for("ceo-typed", "On it!");
        spine.set_live_observer(Box::new(PhoneLiveEmitter::new(Arc::clone(&hub))));
        let turn = spine.submit_prompt("add a line to notes.txt and land it", Source::Text).unwrap();

        let mut rows: Vec<(u64, String, String, String)> = Vec::new(); // cursor, role, text, id
        while let Ok(frame) = rx.try_recv() {
            if frame.kind != "message" {
                continue;
            }
            let v: serde_json::Value = serde_json::from_str(&frame.data).unwrap();
            rows.push((
                v["cursor"].as_u64().unwrap(),
                v["role"].as_str().unwrap_or("").to_string(),
                v["text"].as_str().unwrap_or("").to_string(),
                v["id"].as_str().unwrap_or("").to_string(),
            ));
        }

        let his = rows
            .iter()
            .find(|(_, role, _, _)| role == "ceo")
            .unwrap_or_else(|| panic!("his own message never reached the phone: {rows:?}"));
        assert_eq!(his.2, "add a line to notes.txt and land it", "his row lost his words: {rows:?}");
        assert_eq!(his.3, format!("{turn}:user"), "the live row's id is not the projection's: {rows:?}");
        assert_eq!(his.0, 1, "his message is the thread's first row: {rows:?}");

        // AND RICH'S REPLY IS AFTER IT, at its own cursor. A CEO row that did not consume one
        // would put every reply a position ahead of where a reload puts it.
        let reply = rows
            .iter()
            .find(|(_, role, _, _)| role == "rich")
            .unwrap_or_else(|| panic!("the reply never reached the phone: {rows:?}"));
        assert!(reply.0 > his.0, "the reply took his row's cursor: {rows:?}");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn every_frame_of_one_reply_carries_one_cursor() {
        // The phone merges a reply by cursor, so its opening, its deltas and its completion must
        // all name the same row. A delta under a different cursor is a second bubble.
        let hub = live_hub();
        let mut rx = hub.subscribe();
        let (path, mut spine, _thread) = spine_for("one-cursor", "One row, several frames.");
        spine.set_live_observer(Box::new(PhoneLiveEmitter::new(Arc::clone(&hub))));
        spine.submit_prompt("say something", Source::Text).unwrap();

        // **THE REPLY'S FRAMES, WHICH IS NOT THE SAME SET AS "EVERY FRAME" ANY MORE.** This
        // assertion read `cursors.iter().all(|c| *c == cursors[0])` over every frame of the
        // turn, and that was correct while the only rows a turn produced were Rich's. Since
        // `esc-20260919T003541Z-6885f74b` his OWN message is a row too, at its own cursor — a
        // stale assertion, not a defect, and the invariant this test is named for is unchanged:
        // one reply, one cursor.
        let mut his: Vec<u64> = Vec::new();
        let mut reply: Vec<u64> = Vec::new();
        while let Ok(frame) = rx.try_recv() {
            let v: serde_json::Value = serde_json::from_str(&frame.data).unwrap_or_default();
            if frame.kind == "message" && v["role"] == "ceo" {
                his.push(frame.cursor);
            } else {
                reply.push(frame.cursor);
            }
        }
        assert_eq!(his.len(), 1, "his message is one row: {his:?}");
        assert!(!reply.is_empty(), "nothing of the reply was published");
        assert!(
            reply.iter().all(|c| *c == reply[0]),
            "one reply was spread over several cursors: {reply:?}"
        );
        assert!(reply[0] > his[0], "the reply did not take the row after his: {his:?} then {reply:?}");
        let _ = std::fs::remove_file(path);
    }
}

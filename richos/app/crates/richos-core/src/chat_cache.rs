//! **Each chat's context size and its cache clock, read off its two sessions' own streams**
//! (CEO, 2026-11-16, round 83): the context meter and "Keep chat alive".
//!
//! A chat holds two Claude Code sessions: Rich talking with the user, and Rich directing the
//! team (the back end the record calls Stu). Each session's reader thread tells this store three
//! things as they happen ([`LeaseMeter`]):
//!
//! - **the lead made a model request** (its reply's `message_start`; a helper's frames are not
//!   the lead's and are not counted), stamped with the wall clock. The provider's cache lifetime
//!   runs on the server's clock from the last request, so a clock that stops while the Mac
//!   sleeps would be wrong here;
//! - **the context size of that request**: input + cache read + cache creation of the lead's own
//!   `message_delta`. Measured on Claude Code 2.1.325 with `scripts/qa/context-figure-probe.py`
//!   (2026-11-16): on a one-request turn, a two-request turn and a follow-up it was 30,251,
//!   32,381 and 32,402, exactly what Claude Code's own `get_context_usage` answered each time,
//!   while the turn's `result.usage` said 64,674 for the two-request turn (it adds the
//!   requests up);
//! - **the turn's end**: the model's window (`result.modelUsage[model].contextWindow`,
//!   1,000,000 in that run) and which cache lifetime the turn wrote under
//!   (`usage.cache_creation.ephemeral_1h_input_tokens` or `ephemeral_5m_input_tokens`).
//!
//! The readings are kept on disk so a chat shows its last size after a relaunch and a keep-alive
//! can tell a cache that is about to lapse from one that already has.
//!
//! **The keep-alive rule is [`heartbeat_due`], and it is the whole of "smart"** (his word): one
//! technical message, only when a session's last request was 55 minutes ago or more and its
//! one-hour cache has not lapsed yet. Research and prices:
//! richos-hq `docs/research/2026-11-16-cache-heartbeat-what-can-be-done.md`.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// A session that has made no request for this long gets one keep-alive message (his words:
/// "when Rich or Stu really have been idle for 55 minutes").
pub const HEARTBEAT_AFTER_MS: u64 = 55 * 60 * 1000;
/// The provider's cache lifetime in the app's mode (measured: every turn writes
/// `ephemeral_1h_input_tokens`). Past it there is nothing left to keep: a message then would
/// write the whole chat again, which is what the user's own next message costs anyway.
pub const CACHE_LIFETIME_MS: u64 = 60 * 60 * 1000;

/// What opens the app's keep-alive message. The engine's hook reads this prefix to give that
/// turn no notice meant for a real one (`engine/scripts/app-engine-hook.py`).
pub const KEEP_ALIVE_TAG: &str = "<richos-keep-alive>";
/// The message itself: host machinery, never shown and never a turn of the user's.
pub const KEEP_ALIVE_TEXT: &str = "<richos-keep-alive>The RichOS app sent this by itself to keep this chat ready. \
It is not from the user and asks for nothing. Reply with the single word ok and do nothing else.</richos-keep-alive>";

/// Which of a chat's two sessions a reading is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seat {
    /// Rich, talking with the user.
    Rich,
    /// Rich directing the team (Stu's session).
    Team,
}

/// What one session last said about itself. Every field is `None` until it has been read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reading {
    /// Tokens in the context at the session's last model request.
    pub used: Option<u64>,
    /// The model's context window.
    pub size: Option<u64>,
    /// Wall-clock milliseconds of the session's last model request.
    pub last_request_ms: Option<u64>,
    /// Whether the last turn that wrote to the cache wrote under the one-hour lifetime.
    pub one_hour: Option<bool>,
}

/// One chat's two readings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Chat {
    pub rich: Reading,
    pub team: Reading,
}

impl Chat {
    pub fn seat(&self, seat: Seat) -> &Reading {
        match seat {
            Seat::Rich => &self.rich,
            Seat::Team => &self.team,
        }
    }
    fn seat_mut(&mut self, seat: Seat) -> &mut Reading {
        match seat {
            Seat::Rich => &mut self.rich,
            Seat::Team => &mut self.team,
        }
    }
}

/// **Is a keep-alive message due on this session now?** Only under the one-hour lifetime (a
/// five-minute cache is gone long before 55 minutes), only from 55 minutes after its last
/// request, and only while that cache is still there. A Mac that slept through the hour comes
/// back past the lifetime, and nothing is sent.
pub fn heartbeat_due(reading: &Reading, now_ms: u64) -> bool {
    if reading.one_hour != Some(true) {
        return false;
    }
    reading.last_request_ms.is_some_and(|at| {
        let idle = now_ms.saturating_sub(at);
        (HEARTBEAT_AFTER_MS..CACHE_LIFETIME_MS).contains(&idle)
    })
}

/// A keep-alive that was tried is not tried again for this long, whatever came of it: one that
/// could not be sent (a turn began, the provider refused) is tried at most three times before
/// the hour is up, never every time the clock is looked at.
pub const HEARTBEAT_RETRY_AFTER_MS: u64 = 2 * 60 * 1000;

/// Told when a chat's context size changed: `(thread id, the chat's readings)`.
pub type Observer = Arc<dyn Fn(&str, Chat) + Send + Sync>;

/// Which chats the user has checked "Keep chat alive" in, asked by thread id.
pub type Kept = Arc<dyn Fn(&str) -> bool + Send + Sync>;

/// Every chat's readings, shared by the leases that write them and the shell that reads them.
#[derive(Default)]
pub struct Readings {
    path: Option<PathBuf>,
    chats: Mutex<BTreeMap<String, Chat>>,
    saving: Mutex<()>,
    observer: Mutex<Option<Observer>>,
    /// When a keep-alive was last tried on each session (thread, whether it is the team's).
    tried: Mutex<BTreeMap<(String, bool), u64>>,
}

impl std::fmt::Debug for Readings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Readings({:?})", self.path)
    }
}

impl Readings {
    /// The readings kept in `path`. A file that is missing or cannot be read starts empty: the
    /// next turn of each chat writes its reading again.
    pub fn open(path: &Path) -> Arc<Self> {
        let chats = std::fs::read(path).ok().and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
        Arc::new(Self { path: Some(path.to_path_buf()), chats: Mutex::new(chats), ..Self::default() })
    }

    /// **Take the keep-alive that is due on this session now, if one is.** `true` at most once
    /// per [`HEARTBEAT_RETRY_AFTER_MS`] for a session, and only while [`heartbeat_due`] says so;
    /// whoever is told `true` sends it.
    pub fn claim_heartbeat(&self, thread: &str, seat: Seat, now_ms: u64) -> bool {
        if !heartbeat_due(self.chat(thread).seat(seat), now_ms) {
            return false;
        }
        let mut tried = self.tried.lock().unwrap();
        let key = (thread.to_string(), seat == Seat::Team);
        if tried.get(&key).is_some_and(|at| now_ms.saturating_sub(*at) < HEARTBEAT_RETRY_AFTER_MS) {
            return false;
        }
        tried.insert(key, now_ms);
        true
    }

    /// Readings kept in memory only (tests, headless runs).
    pub fn in_memory() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn set_observer(&self, observer: Observer) {
        *self.observer.lock().unwrap() = Some(observer);
    }

    pub fn chat(&self, thread: &str) -> Chat {
        self.chats.lock().unwrap().get(thread).copied().unwrap_or_default()
    }

    /// The handle one lease writes through.
    pub fn lease(self: &Arc<Self>, thread: &str, seat: Seat) -> LeaseMeter {
        LeaseMeter { readings: Arc::clone(self), thread: thread.to_string(), seat }
    }

    fn update(&self, thread: &str, seat: Seat, change: impl FnOnce(&mut Reading)) -> Option<Chat> {
        let mut chats = self.chats.lock().unwrap();
        let chat = chats.entry(thread.to_string()).or_default();
        let before = *chat;
        change(chat.seat_mut(seat));
        (*chat != before).then_some(*chat)
    }

    fn tell(&self, thread: &str, chat: Chat) {
        let observer = self.observer.lock().unwrap().clone();
        if let Some(observer) = observer {
            observer(thread, chat);
        }
    }

    fn save(&self, path: &Path) -> std::io::Result<()> {
        // Serialize saves from the snapshot through rename. Locking only the write could
        // let a delayed writer replace the newest snapshot with one it took earlier.
        let _saving = self.saving.lock().unwrap();
        let body = serde_json::to_vec(&*self.chats.lock().unwrap()).map_err(std::io::Error::other)?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, body).and_then(|()| std::fs::rename(&temporary, path))
    }
}

/// One lease's way to write its own readings. Cheap to clone; called on the lease's reader thread.
#[derive(Clone)]
pub struct LeaseMeter {
    readings: Arc<Readings>,
    thread: String,
    seat: Seat,
}

impl LeaseMeter {
    /// The lead made a model request now.
    pub fn requested(&self, now_ms: u64) {
        self.readings.update(&self.thread, self.seat, |reading| reading.last_request_ms = Some(now_ms));
    }

    /// The context size of the lead's last request ([`crate::native`]'s `tokens_in_context`).
    pub fn context(&self, used: u64) {
        if let Some(chat) = self.readings.update(&self.thread, self.seat, |reading| reading.used = Some(used)) {
            self.readings.tell(&self.thread, chat);
        }
    }

    /// A turn of the lead's ended: `size` is the model's window when the result named it, and
    /// `one_hour` the cache lifetime it wrote under when it wrote anything.
    pub fn turn_ended(&self, size: Option<u64>, one_hour: Option<bool>) {
        let changed = self.readings.update(&self.thread, self.seat, |reading| {
            if size.is_some() { reading.size = size; }
            if one_hour.is_some() { reading.one_hour = one_hour; }
        });
        if let Some(chat) = changed {
            self.readings.tell(&self.thread, chat);
        }
        // Once per turn, with the request time and the size that turn left behind.
        let Some(path) = &self.readings.path else { return };
        if let Err(error) = self.readings.save(path) {
            eprintln!("[richos] chat readings were not saved to {}: {error}", path.display());
        }
    }
}

impl std::fmt::Debug for LeaseMeter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LeaseMeter({}, {:?})", self.thread, self.seat)
    }
}

/// The cache lifetime a turn's `result` wrote under: `Some(true)` for one hour, `Some(false)`
/// for five minutes, `None` when the turn wrote nothing to the cache (a pure read says nothing
/// new, so the last answer stands).
pub fn lifetime_of(result: &serde_json::Value) -> Option<bool> {
    let written = result.pointer("/usage/cache_creation")?;
    let tokens = |key: &str| written.get(key).and_then(serde_json::Value::as_u64).unwrap_or(0);
    if tokens("ephemeral_1h_input_tokens") > 0 {
        Some(true)
    } else if tokens("ephemeral_5m_input_tokens") > 0 {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: u64 = 60 * 1000;

    fn idle_for(minutes: u64, one_hour: Option<bool>) -> (Reading, u64) {
        let at = 1_700_000_000_000;
        (Reading { last_request_ms: Some(at), one_hour, ..Reading::default() }, at + minutes * MINUTE)
    }

    /// KEEP CHAT ALIVE, the rule (CEO 2026-11-16: "the cache heartbeat only needs to happen when
    /// Rich or Stu really have been idle for 55 minutes i.e. the cache heartbeat needs to be
    /// smart, not dumb").
    #[test]
    fn a_keep_alive_is_due_only_from_55_minutes_idle_and_only_while_the_cache_is_still_there() {
        for (minutes, due) in [(0, false), (30, false), (54, false), (55, true), (59, true), (60, false), (61, false), (600, false)] {
            let (reading, now) = idle_for(minutes, Some(true));
            assert_eq!(heartbeat_due(&reading, now), due, "{minutes} minutes after the last request");
        }
    }

    /// A five-minute cache (an API key, a subscription over its limits) is gone long before 55
    /// minutes, and a session that has not said which lifetime it writes under is not guessed at.
    #[test]
    fn no_keep_alive_without_the_one_hour_cache_or_a_request_to_count_from() {
        let (five_minutes, now) = idle_for(56, Some(false));
        assert!(!heartbeat_due(&five_minutes, now));
        let (unknown, now) = idle_for(56, None);
        assert!(!heartbeat_due(&unknown, now));
        assert!(!heartbeat_due(&Reading { one_hour: Some(true), ..Reading::default() }, now), "no request was ever made");
    }

    /// The Mac slept through the hour: the wall clock says so and nothing is sent ("on a
    /// laptop, if they close the laptop, they wouldn't expect to have there anything running").
    #[test]
    fn a_mac_that_slept_past_the_hour_gets_no_keep_alive_on_waking() {
        let (reading, now) = idle_for(8 * 60, Some(true));
        assert!(!heartbeat_due(&reading, now));
    }

    /// One keep-alive per session when it is due, and a try that came to nothing is not
    /// repeated at every look of the clock: at most three before the hour is up.
    #[test]
    fn a_due_keep_alive_is_taken_once_and_tried_again_only_two_minutes_later() {
        let readings = Readings::in_memory();
        let at = 1_700_000_000_000;
        let rich = readings.lease("thr_a", Seat::Rich);
        rich.requested(at);
        rich.turn_ended(Some(1_000_000), Some(true));
        assert!(!readings.claim_heartbeat("thr_a", Seat::Rich, at + 54 * MINUTE), "taken before it was due");
        assert!(readings.claim_heartbeat("thr_a", Seat::Rich, at + 55 * MINUTE));
        assert!(!readings.claim_heartbeat("thr_a", Seat::Rich, at + 55 * MINUTE + 30_000), "taken twice within thirty seconds");
        assert!(!readings.claim_heartbeat("thr_a", Seat::Team, at + 55 * MINUTE), "the team's session has made no request");
        let tries = (0..20).filter(|tick| readings.claim_heartbeat("thr_a", Seat::Rich, at + 55 * MINUTE + 30_000 + tick * 30_000)).count();
        assert_eq!(tries, 2, "a keep-alive that never got through was tried {} times in all", tries + 1);
        // It got through: the session's own request starts the clock again.
        rich.requested(at + 59 * MINUTE);
        assert!(!readings.claim_heartbeat("thr_a", Seat::Rich, at + 60 * MINUTE));
        assert!(readings.claim_heartbeat("thr_a", Seat::Rich, at + (59 + 55) * MINUTE));
    }

    /// The engine's hook knows the app's keep-alive message by the tag that opens it, and gives
    /// that turn no notice meant for a real one (`engine/scripts/app-engine-hook.py`,
    /// `keep_alive`). The two files must name the same tag, and the message must open with it.
    #[test]
    fn the_engines_hook_knows_the_keep_alive_message_by_the_tag_that_opens_it() {
        let hook = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).unwrap().join("engine/scripts/app-engine-hook.py");
        let source = std::fs::read_to_string(&hook).unwrap_or_else(|e| panic!("{}: {e}", hook.display()));
        assert!(source.contains(&format!("KEEP_ALIVE_TAG = \"{KEEP_ALIVE_TAG}\"")), "the hook names another tag than {KEEP_ALIVE_TAG}");
        assert!(KEEP_ALIVE_TEXT.starts_with(KEEP_ALIVE_TAG), "the keep-alive message does not open with its tag");
    }

    #[test]
    fn a_result_says_which_cache_lifetime_its_turn_wrote_under() {
        let hour = serde_json::json!({"usage": {"cache_creation": {"ephemeral_1h_input_tokens": 17, "ephemeral_5m_input_tokens": 0}}});
        let five = serde_json::json!({"usage": {"cache_creation": {"ephemeral_1h_input_tokens": 0, "ephemeral_5m_input_tokens": 13497}}});
        let read_only = serde_json::json!({"usage": {"cache_creation": {"ephemeral_1h_input_tokens": 0, "ephemeral_5m_input_tokens": 0}}});
        assert_eq!(lifetime_of(&hour), Some(true));
        assert_eq!(lifetime_of(&five), Some(false));
        assert_eq!(lifetime_of(&read_only), None);
        assert_eq!(lifetime_of(&serde_json::json!({"usage": {}})), None);
    }

    /// THE CONTEXT METER's store: each chat keeps its own two figures, a change is told once,
    /// and what a turn left behind is read back after a relaunch.
    #[test]
    fn each_chat_keeps_its_own_two_readings_and_they_survive_a_relaunch() {
        let dir = std::env::temp_dir().join(format!("richos-chat-cache-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("chat-readings.json");
        let readings = Readings::open(&path);
        let told = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&told);
        readings.set_observer(Arc::new(move |thread, chat| sink.lock().unwrap().push((thread.to_string(), chat))));

        let rich = readings.lease("thr_a", Seat::Rich);
        rich.requested(1_000);
        rich.context(500_000);
        rich.context(500_000); // the same figure again is not a change
        rich.turn_ended(Some(1_000_000), Some(true));
        let team = readings.lease("thr_a", Seat::Team);
        team.context(212_000);
        team.turn_ended(None, None);

        let chat = readings.chat("thr_a");
        assert_eq!((chat.rich.used, chat.rich.size), (Some(500_000), Some(1_000_000)));
        assert_eq!((chat.rich.last_request_ms, chat.rich.one_hour), (Some(1_000), Some(true)));
        assert_eq!((chat.team.used, chat.team.size), (Some(212_000), None));
        assert_eq!(readings.chat("thr_b"), Chat::default(), "another chat has its own, unread");
        assert_eq!(told.lock().unwrap().len(), 3, "the size, the window and the team's size, each once");

        let reopened = Readings::open(&path);
        assert_eq!(reopened.chat("thr_a"), chat);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn concurrent_saves_publish_complete_snapshots_without_losing_readings() {
        let dir = std::env::temp_dir().join(format!("richos-chat-cache-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("chat-readings.json");
        let readings = Readings::open(&path);
        let writers = 12;
        let ready = std::sync::Barrier::new(writers);
        std::thread::scope(|scope| {
            let mut saves = Vec::new();
            for writer in 0..writers {
                let meter = readings.lease(&format!("thr_{writer}"), Seat::Rich);
                let (readings, path, ready) = (&readings, &path, &ready);
                saves.push(scope.spawn(move || {
                    let mut snapshots = Vec::new();
                    for round in 1..=8 {
                        meter.context(round);
                        ready.wait();
                        snapshots.push((round, readings.save(path), std::fs::read(path)));
                        ready.wait();
                    }
                    snapshots
                }));
            }
            for save in saves {
                for (round, saved, snapshot) in save.join().unwrap() {
                    saved.expect("every writer must be able to publish");
                    let snapshot: BTreeMap<String, Chat> = serde_json::from_slice(&snapshot.unwrap()).unwrap();
                    assert_eq!(snapshot.len(), writers);
                    assert!(snapshot.values().all(|chat| chat.rich.used == Some(round)));
                }
            }
        });
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

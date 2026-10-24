//! **THE SEAM INTO THE REST OF THE APP** — [`routes::Bridge`] implemented over the Tauri handles,
//! and the one honest limit it runs into.
//!
//! # The wall this file works around, measured
//!
//! `Spine::submit_prompt` takes `&mut self` and does not return until the turn is over, and the
//! shell holds the one `Spine` behind a `Mutex` (`main.rs`, `state.spine.lock().unwrap()`). So for
//! the whole length of a turn — the spec's own example is *"Worked for 2h 17m 50s"* — **every path
//! into the spine is blocked.**
//!
//! Two consequences, and they are handled differently because they are different problems:
//!
//! **Writing** is already solved and not by this file: the CEO's words go to the durable intake
//! log, which is reachable without the spine lock, and `Spine::poll_intake` drains it. So
//! [`PhoneBridge::submit_text`] returns in the time one `fsync` takes, and the turn itself runs on
//! a thread that is allowed to wait for the lock for as long as it needs.
//!
//! **Reading** is the one that needed a decision. A phone opening its stream while Rich is
//! mid-turn cannot be made to wait for the lock: that is an HTTP request held open for
//! potentially hours, which presents as a phone that will not start. So this file keeps **the last
//! gated payload it was able to read** and serves that when the lock is busy.
//!
//! **What that costs, stated rather than buried:** a phone whose very first connection happens
//! during a long turn, on an install that has never had one, gets a readable *"Rich is working"*
//! instead of the conversation. Every other case is covered, because the cache is primed when the
//! channel starts (he has just opened the pairing screen, so nothing is running) and refreshed
//! after every turn the channel itself starts. It is a cache of something already gated — never a
//! second source of truth, and never something the phone can disagree with the Mac about for
//! longer than one turn.

use super::routes::{Accepted, Bridge};
use crate::timeline_view::timeline_payload;
use crate::AppState;
use richos_core::read_view::SpineView;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// What the phone is allowed to reach, and how.
pub struct PhoneBridge {
    app: AppHandle,
    /// The last gated payload per thread, and the thread list. Refreshed whenever the spine lock
    /// happens to be free.
    cache: Mutex<Cache>,
    voice: super::voice::VoiceDesk,
}

#[derive(Default)]
struct Cache {
    payloads: HashMap<String, Value>,
    threads: Vec<(String, String)>,
    active: Option<String>,
}

impl PhoneBridge {
    pub fn new(app: AppHandle) -> Self {
        let directory = app.state::<AppState>().data_dir.join("phone/speech");
        let bridge = PhoneBridge { app, cache: Mutex::new(Cache::default()), voice: super::voice::VoiceDesk::new(directory) };
        // Primed at construction, which is when the CEO opens the pairing screen — so nothing is
        // running and the lock is free. This is what makes the degraded path above rare rather
        // than ordinary.
        bridge.refresh();
        bridge
    }

    /// Read everything the phone might ask for, if the spine is free. Called at construction and
    /// after every turn the channel starts.
    pub fn refresh(&self) {
        let state = self.app.state::<AppState>();
        let Ok(spine) = state.spine.try_lock() else { return };
        let threads = titled(spine.threads(), state.nav.lock().unwrap().state());
        let active = spine.active_thread().map(|s| s.to_string()).or_else(|| threads.first().map(|t| t.0.clone()));
        let mut payloads = HashMap::new();
        if let Some(id) = active.as_deref() {
            if let Ok(payload) = timeline_payload(&*spine, id) {
                payloads.insert(id.to_string(), payload);
            }
        }
        let mut cache = self.cache.lock().unwrap();
        cache.threads = threads;
        cache.active = active;
        for (k, v) in payloads {
            cache.payloads.insert(k, v);
        }
    }

    /// **The conversation list, read now, for a conversation just created or renamed** (Ray's
    /// nightly 66 and 67 walks: a conversation made on the Mac was `Conversation` in the phone's
    /// picker until opened, because this list was read only at construction and after a turn).
    /// Read from the reader's snapshot, which needs no spine lock, so a running turn cannot
    /// leave it stale; returned for [`super::PhoneRuntime::threads_changed`] to send.
    pub fn threads_changed(&self) -> Vec<(String, String)> {
        let state = self.app.state::<AppState>();
        let threads = titled(state.reader.snapshot().threads(), state.nav.lock().unwrap().state());
        self.cache.lock().unwrap().threads = threads.clone();
        threads
    }
}

/// `(id, title)` per conversation, with the title the Mac's rail shows
/// (`nav::NavState::display_title`): the CEO's rename where he made one, then the title Haiku
/// wrote from the first message, the ledger's title otherwise.
pub(crate) fn titled(
    threads: Vec<richos_core::thread::ThreadSummary>,
    nav: &crate::nav::NavState,
) -> Vec<(String, String)> {
    threads
        .into_iter()
        .map(|t| {
            let title = nav.display_title(&t.id, &t.title);
            (t.id, title)
        })
        .collect()
}

impl Bridge for PhoneBridge {
    fn questions_available(&self)->bool {true}
    fn answer_question(&self,thread:&str,answer:richos_core::questions::AnswerRequest)->Result<Value,String> {
        let method=if answer.text.trim().is_empty(){"phone_tap"}else{"phone_typed"};
        let state=self.app.state::<AppState>();
        Ok(crate::question_host::answer(&state,thread,answer,method,"phone")?.public_value())
    }
    fn question_shown(&self,thread:&str,question:&str)->Result<(),String> {
        crate::question_host::shown(&self.app.state::<AppState>(),thread,question,"phone")
    }

    fn native_notifications_available(&self)->bool {true}
    fn register_native_notifications(&self,device:&str,registration:Option<super::notifications::Registration>)->Result<Value,String> {
        self.app.state::<std::sync::Arc<super::PhoneRuntime>>().register_native_notifications(device,registration)
    }
    fn voice_available(&self) -> bool { self.voice.available() }
    fn oxford_comma(&self) -> bool { self.app.state::<AppState>().config.lock().unwrap().oxford_comma() }
    /// `intake_<n>` -> the turn drained from intake record `n`, read with `try_lock` so a phone
    /// request never waits on a running turn: busy means "not yet", and the next request asks again.
    fn turn_for_intake(&self, message_id: &str) -> Option<String> {
        let n = message_id.strip_prefix("intake_")?.parse::<u64>().ok()?;
        let state = self.app.state::<AppState>();
        let spine = state.spine.try_lock().ok()?;
        spine.ledger().turn_for_intake(n).map(|turn| turn.id.clone())
    }
    fn transcribe(&self, bytes: &[u8]) -> Result<String, String> { self.voice.transcribe(bytes) }
    fn reply_audio(&self, thread: Option<&str>, id: &str) -> Result<Vec<u8>, String> {
        let payload = self.snapshot(thread)?;
        let rows = super::rows::rows_from_payload(&payload);
        let row = rows.iter().find(|row| row["id"] == id && row["role"] == "rich" && row["complete"] != false)
            .ok_or("This reply is not available for playback.")?;
        self.voice.synthesize(row["text"].as_str().unwrap_or(""))
    }

    fn files_available(&self) -> bool { true }
    fn rich_file(&self, thread: &str, id: &str) -> Result<(String, Vec<u8>), richos_core::rich_files::Fetch> {
        let root = crate::rich_files_host::root(&self.app.state::<AppState>());
        richos_core::rich_files::read_for_phone(&root, thread, id).map(|(row, body)| (row.name, body))
    }

    fn submit_text(&self, thread_id: Option<&str>, text: &str, client_id: &str) -> Result<Accepted, String> {self.submit_words(thread_id,text,"phone_typed",client_id,false)}
    /// The transcript of a voice note: the same road as typed words, written as a voice note, so
    /// every row of the turn (the live one that can beat this POST's answer included) is a voice
    /// row from its first appearance (`rows::message_kind`).
    fn submit_voice(&self, thread_id: Option<&str>, text: &str, client_id: &str) -> Result<Accepted, String> {self.submit_words(thread_id,text,"phone_voice",client_id,true)}

    fn snapshot(&self, thread_id: Option<&str>) -> Result<Value, String> {
        let state = self.app.state::<AppState>();
        let thread = thread_id
            .map(|s| s.to_string())
            .or_else(|| self.cache.lock().unwrap().active.clone())
            .ok_or_else(|| "this Mac has no conversation yet".to_string())?;
        crate::question_host::payload(&state,&thread)
    }

    fn current_thread(&self) -> Option<(String, String)> {
        let (id, threads) = {
            let cache = self.cache.lock().unwrap();
            (cache.active.clone()?, cache.threads.clone())
        };
        let title = self
            .shown(threads)
            .into_iter()
            .find(|(known, _)| *known == id)
            .map(|(_, t)| t)
            .unwrap_or_else(|| "Rich".to_string());
        Some((id, title))
    }

    fn threads(&self) -> Vec<(String, String)> {
        let threads = self.cache.lock().unwrap().threads.clone();
        self.shown(threads)
    }
}

impl PhoneBridge {
    /// **The titles the Mac's sidebar shows**, read when the phone asks rather than when the
    /// cache was filled: his own rename, then the title Haiku wrote from the first message
    /// (`nav::NavState::display_title`). So a conversation is called the same thing on both,
    /// and a title Haiku wrote after the last turn is on the phone's next list.
    fn shown(&self, threads: Vec<(String, String)>) -> Vec<(String, String)> {
        let state = self.app.state::<AppState>();
        let Ok(nav) = state.nav.lock() else { return threads };
        threads.into_iter().map(|(id, title)| {
            let shown = nav.state().display_title(&id, &title);
            (id, shown)
        }).collect()
    }
}


impl PhoneBridge {
    fn submit_words(&self, thread_id: Option<&str>, text: &str, channel:&str, client_id: &str, voice_note: bool) -> Result<Accepted, String> {
        let state = self.app.state::<AppState>();
        // The thread the words belong to, resolved HERE and never at drain time: the desktop's
        // active thread can move while a record waits, and re-scoping the CEO's words to wherever
        // the Mac happens to be looking would launder them across an entity boundary (ECS §3.4).
        let thread = thread_id
            .map(|s| s.to_string())
            .or_else(|| self.cache.lock().unwrap().active.clone())
            .ok_or_else(|| "this Mac has no conversation to add to yet".to_string())?;
        let entity = Some(state.reader.snapshot().ledger().thread_binding(&thread)
            .map_err(|e| e.to_string())?.entity_id().clone());

        // Durable before the phone is answered. This is the whole reason the phone writes here
        // rather than calling the spine. The phone's own id for the message goes into the SAME
        // record, so no drain can make a turn of these words without it, and the row the Mac
        // sends back names the line the phone is already showing. Whether the words are a voice
        // note's transcript goes into the same record for the same reason.
        let record = write_phone_words(&state.control, &thread, entity, text, channel, client_id, voice_note)?;
        let intake_id = record.id();

        crate::dispatch_incoming_messages(&self.app, &thread)?;

        Ok(Accepted {
            // The intake id, not a turn id: there is no turn yet, and inventing one here would be
            // a claim that the message had been accepted as work rather than as words.
            message_id: format!("intake_{intake_id}"),
            thread_id: thread,
            at: super::now_millis(),
        })
    }

}

/// **The durable write of the phone's words, with the phone's own id for them in the same
/// record** (adoption ledger §2.8 row C5). One `fsync`, so no drain can make a turn of these
/// words without the id, and the CEO row the Mac sends back names the line the phone is already
/// showing (`rows::client_id_of`). `voice_note` rides in the same record: the words are a voice
/// note's transcript, so every row of the turn is a voice row (`rows::message_kind`). Apart from
/// [`PhoneBridge::submit_words`] only so it can be proved without a running app.
fn write_phone_words(
    control: &richos_core::steering::TurnControl,
    thread: &str,
    entity: Option<richos_core::entity::EntityId>,
    text: &str,
    channel: &str,
    client_id: &str,
    voice_note: bool,
) -> Result<richos_core::steering::IntakeRecord, String> {
    let written = if voice_note {
        control.submit_voice_note_from_channel(thread, entity, text, channel, Some(client_id))
    } else {
        control.submit_from_channel_as(thread, entity, text, channel, Some(client_id))
    };
    written.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{titled, write_phone_words};

    /// The phone's list names a conversation as the Mac's rail does: the CEO's rename when he
    /// made one, the ledger's title otherwise. Until this, a rename never reached the phone at all.
    #[test]
    fn the_phones_list_names_a_renamed_conversation_by_its_new_name() {
        let summary = |id: &str, title: &str| richos_core::thread::ThreadSummary {
            id: id.into(), title: title.into(), created_at: 0, message_count: 0, last_activity: 0, entity_id: None, binding_revision: 0,
        };
        let mut nav = crate::nav::NavState::default();
        nav.renamed_threads.insert("thr_b".to_string(), "Welcome email".to_string());
        assert_eq!(
            titled(vec![summary("thr_a", "Onboarding"), summary("thr_b", "New thread")], &nav),
            vec![("thr_a".to_string(), "Onboarding".to_string()), ("thr_b".to_string(), "Welcome email".to_string())]
        );
    }
    use richos_core::steering::{IntakeRecord, TurnControl};

    /// The record a phone message becomes carries the phone's own id, and a second reader of
    /// the log (a restart) reads it back: the drain that turns it into a turn cannot miss it.
    #[test]
    fn the_phones_words_are_written_with_the_phones_own_id_for_them() {
        let path = std::env::temp_dir().join(format!(
            "richos-bridge-intake-{}-{}.jsonl",
            std::process::id(),
            super::super::now_millis()
        ));
        let control = TurnControl::open(&path).unwrap();
        let record = write_phone_words(&control, "thr_1", None, "call the bank", "phone_voice", "v1", true).unwrap();
        let typed = write_phone_words(&control, "thr_1", None, "and the bank", "phone_typed", "t1", false).unwrap();
        let carried = |r: &IntakeRecord| match r {
            IntakeRecord::Channel { client_id, text, channel, voice_note, .. } => {
                (client_id.clone(), text.clone(), channel.clone(), *voice_note)
            }
            other => panic!("a phone message became {other:?}"),
        };
        let expected = (Some("v1".to_string()), "call the bank".to_string(), "phone_voice".to_string(), true);
        let expected_typed = (Some("t1".to_string()), "and the bank".to_string(), "phone_typed".to_string(), false);
        assert_eq!(carried(&record), expected);
        assert_eq!(carried(&typed), expected_typed);
        let reread = TurnControl::open(&path).unwrap().pending_intake();
        assert_eq!(
            reread.iter().map(carried).collect::<Vec<_>>(),
            vec![expected, expected_typed],
            "not on disk as written"
        );
        if let Err(error) = std::fs::remove_file(&path) { eprintln!("fixture cleanup: {error}"); }
    }
}

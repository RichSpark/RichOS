//! **The context meter and "Keep chat alive", for the window** (CEO, 2026-11-16, round 83).
//!
//! The readings are `richos_core::chat_cache`'s: each chat's two sessions write their context
//! size and the time of their last model request as their streams say them. This file is the
//! window's side of that: the switch in Settings, a chat's two figures (and each change as it
//! happens), a chat's own checkbox, and the one clock that looks, every thirty seconds, at the
//! chats whose box is checked.
//!
//! **Nothing here keeps the Mac awake** (his words: "on a laptop, if they close the laptop,
//! they wouldn't expect to have there anything running"). The clock is a sleeping thread; a Mac
//! that sleeps stops it, and the readings' wall-clock times say on waking how long it was.
use richos_core::chat_cache::{Chat, Kept, Reading, Seat};
use richos_core::read_view::SpineView;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};

/// A chat's context figures changed: `{threadId, rich: {used, size}, team: {used, size}}`.
pub const EVENT_CONTEXT_METER: &str = "rich://context-meter";

/// How often the kept chats are looked at. A keep-alive therefore goes out between 55 minutes
/// and 55 and a half after a session's last request, four and a half before its cache lapses.
const LOOK_EVERY: std::time::Duration = std::time::Duration::from_secs(30);

/// One session's figures as the window draws them: `null` until that session has said them.
#[derive(Clone, serde::Serialize)]
pub struct Side {
    used: Option<u64>,
    size: Option<u64>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Figures {
    thread_id: String,
    rich: Side,
    team: Side,
}

fn figures(thread: &str, chat: Chat) -> Figures {
    let side = |reading: Reading| Side { used: reading.used, size: reading.size };
    Figures { thread_id: thread.to_string(), rich: side(chat.rich), team: side(chat.team) }
}

/// A chat's figures changed on one of its sessions' streams: told to the window, which draws
/// them only while the switch is on.
pub fn publish(app: &AppHandle, thread: &str, chat: Chat) {
    if app.emit(EVENT_CONTEXT_METER, figures(thread, chat)).is_err() {
        // A closing window can leave the figures without a listener.
    }
}

/// Is `thread`'s "Keep chat alive" box checked? Archived chats are not kept: nobody is coming
/// back to one without opening it again.
fn is_kept(state: &crate::AppState, thread: &str) -> bool {
    let nav = state.nav.lock().unwrap();
    let nav = nav.state();
    nav.keep_alive_threads.iter().any(|kept| kept == thread) && !nav.archived_threads.iter().any(|archived| archived == thread)
}

/// The same answer for the work host, which is built before the app's state is.
pub fn kept(app: AppHandle) -> Kept {
    Arc::new(move |thread| app.try_state::<crate::AppState>().is_some_and(|state| is_kept(&state, thread)))
}

/// Whether a parked chat's session is held from the idle retirement: a kept chat's is, since
/// ending it only to resume it for its keep-alive would write part of its cache again.
pub fn holds_session(state: &crate::AppState, thread: &str) -> bool {
    is_kept(state, thread)
}

#[richos_command::command]
pub fn get_context_meter(state: State<crate::AppState>) -> bool {
    state.config.lock().unwrap().show_context_meter()
}

#[richos_command::command]
pub fn set_context_meter(state: State<crate::AppState>, show: bool) -> Result<(), String> {
    state.config.lock().unwrap().set_show_context_meter(show).map_err(|e| e.to_string())
}

/// One chat's two figures, as last read (kept across relaunches).
#[richos_command::command]
pub fn chat_context(state: State<crate::AppState>, thread_id: String) -> Figures {
    figures(&thread_id, state.chat_readings.chat(&thread_id))
}

#[richos_command::command]
pub fn get_keep_chat_alive(state: State<crate::AppState>, thread_id: String) -> bool {
    state.nav.lock().unwrap().state().keep_alive_threads.iter().any(|kept| kept == &thread_id)
}

#[richos_command::command]
pub fn set_keep_chat_alive(state: State<crate::AppState>, thread_id: String, kept: bool) -> Result<(), String> {
    state.nav.lock().unwrap().set_thread_kept_alive(&thread_id, kept).map_err(|e| e.to_string())
}

/// The clock, from launch to quit.
pub fn start(app: AppHandle) {
    let spawned = std::thread::Builder::new().name("keep-chat-alive".into()).spawn(move || loop {
        std::thread::sleep(LOOK_EVERY);
        if let Some(state) = app.try_state::<crate::AppState>() {
            look(&app, &state);
        }
    });
    if let Err(e) = spawned { eprintln!("[richos] keep chat alive: could not start its clock, no chat is kept alive: {e}"); }
}

/// One look at the kept chats. Rich's session of each is sent its keep-alive when it is due
/// (`chat_cache::heartbeat_due`: idle for 55 minutes, its one-hour cache not yet lapsed); the
/// team's session is the work host's to send, which is only made sure to be listening.
fn look(app: &AppHandle, state: &crate::AppState) {
    let kept: Vec<String> = {
        let nav = state.nav.lock().unwrap();
        nav.state().keep_alive_threads.clone()
    };
    for thread in kept.into_iter().filter(|thread| is_kept(state, thread)) {
        let Ok(binding) = state.reader.snapshot().ledger().thread_binding(&thread) else { continue };
        state.work.keep_team_alive(&binding);
        if !state.chat_readings.claim_heartbeat(&thread, Seat::Rich, richos_core::util::now_millis()) { continue; }
        let app = app.clone();
        let sent = std::thread::Builder::new().name("keep-chat-alive-send".into()).spawn(move || keep_rich_alive(&app, &thread));
        if let Err(e) = sent { eprintln!("[richos] keep chat alive: the keep-alive could not be started: {e}"); }
    }
}

/// Rich's session of `thread` is sent its keep-alive message, off the clock's thread: the reply
/// takes seconds, and a message the user sends meanwhile is delivered the moment it ends.
fn keep_rich_alive(app: &AppHandle, thread: &str) {
    let Some(state) = app.try_state::<crate::AppState>() else { return };
    let verdict = richos_core::spine::keep_front_desk_alive_without_the_spine(&state.spine, thread);
    // As after a preparation: whatever he sent meanwhile was saved, and runs now.
    if let Err(error) = richos_core::spine::drive_shared_turns(&state.spine) { eprintln!("[richos] queued reply: {error}"); }
    let spine = crate::take_the_spine(&state.spine);
    crate::adopt_at_the_turn_boundary(spine, &state.work);
    match verdict {
        richos_core::spine::KeptAlive::Sent { millis, spawned } => eprintln!(
            "[richos] keep chat alive: Rich's session of {thread} answered in {millis} ms{}",
            if spawned { ", resumed for it" } else { "" }),
        // Whatever holds the chat makes the request that renews its cache.
        richos_core::spine::KeptAlive::Busy => eprintln!("[richos] keep chat alive: {thread} is not idle; nothing was sent"),
        richos_core::spine::KeptAlive::NotSent(why) => eprintln!("[richos] keep chat alive: Rich's session of {thread} was not reached: {why}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the window is given for a chat (`chat_context`, `rich://context-meter`): the chat's
    /// id and, for each of its two sessions, the tokens in its context and its window, each
    /// `null` until that session has said it. Nothing else of a reading goes out.
    #[test]
    fn a_chats_figures_reach_the_window_as_its_id_and_two_sides_null_until_read() {
        let chat = Chat {
            rich: Reading { used: Some(500_000), size: Some(1_000_000), last_request_ms: Some(1_700_000_000_000), one_hour: Some(true) },
            team: Reading::default(),
        };
        let wire = serde_json::to_value(figures("thr_a", chat)).unwrap();
        assert_eq!(wire, serde_json::json!({
            "threadId": "thr_a",
            "rich": { "used": 500_000, "size": 1_000_000 },
            "team": { "used": null, "size": null },
        }));
    }
}

//! Voice-mode events for the UI — the same shape as the spine's streaming contract.
//!
//! `app/crates/richos-core/src/stream.rs` established the pattern (`rich://` names as
//! constants, camelCase JSON payloads, a `TurnObserver` sink so the crate stays UI-agnostic).
//! Voice follows it exactly rather than inventing a second convention, so the Tauri shell
//! relays both the same way and the webview learns one idiom.
//!
//! | event | when | payload |
//! |---|---|---|
//! | `rich://voice-state` | the mic state changed, a new input level is available, or the input went silent/came back | `{ state, level, bargeInArmed, noAudio, at }` |
//! | `rich://voice-transcript` | an utterance was recognized and submitted as a turn | `{ text, durationMs, latencyMs, at }` |
//! | `rich://voice-error` | voice mode could not start or had to stop | `{ message, at }` |
//! | `rich://voice-notice` | voice mode is working, but not the way it would on a better machine | `{ message, at }` |
//!
//! `message` on `voice-error` AND on `voice-notice` is ALWAYS a calm, Rich-voiced line. Device
//! names, exit codes and file paths go to stderr, never to the CEO (clean output).
//!
//! **`voice-notice` is NOT a quieter `voice-error`, and the split is the point.** An error says
//! voice stopped; a notice says voice is running and has quietly made a choice on the CEO's
//! behalf — today, that his machine could not carry the more accurate recognizer. Sending that
//! down `voice-error` would put a failure face on a working feature; suppressing it would be the
//! defect `hardware.rs` exists to remove, which is a product that silently gives a good machine
//! the weak model and never says so. It is the product explaining itself.

use crate::state::VoiceState;
use serde_json::{json, Value};

pub const EVENT_VOICE_STATE: &str = "rich://voice-state";
pub const EVENT_VOICE_TRANSCRIPT: &str = "rich://voice-transcript";
pub const EVENT_VOICE_ERROR: &str = "rich://voice-error";
pub const EVENT_VOICE_NOTICE: &str = "rich://voice-notice";

#[derive(Debug, Clone, PartialEq)]
pub enum VoiceEvent {
    /// Mic state and/or live input level. Emitted on every state change, at the level
    /// refresh rate while the mic is open so the meter moves with the CEO's voice, and on
    /// every `no_audio` transition.
    ///
    /// `no_audio` is the post-open silent-input verdict (`noaudio.rs`): the stream is open
    /// and healthy but has delivered nothing above -80.00 dBFS for 3.008 s. It is a FIELD of
    /// the state event rather than a `voice-error` on purpose — it is not fatal, it must
    /// clear by itself the moment audio returns, and it belongs in context next to the mic
    /// state the CEO is already looking at.
    State { state: VoiceState, level: f32, barge_in_armed: bool, no_audio: bool, at: u64 },
    /// What the CEO said, as recognized, already submitted to the spine as a turn.
    /// `duration_ms` is the captured audio length; `latency_ms` is end-of-speech to
    /// transcript — both measured, never estimated.
    Transcript { text: String, duration_ms: u64, latency_ms: u64, at: u64 },
    /// Something stopped voice mode. Rich-voiced; never a stack trace.
    Error { message: String, at: u64 },
    /// Voice mode is RUNNING, and something about how it is running is worth the CEO knowing.
    ///
    /// Today there is exactly one source: the model this machine could carry is not the best one
    /// on the ladder (`hardware.rs`). Emitted ONCE, at voice-mode start, never per utterance —
    /// resolution happens once and a sentence repeated after every thing you say is noise.
    ///
    /// A SEPARATE VARIANT FROM `Error` ON PURPOSE. They carry the same shape and mean opposite
    /// things: `Error` means voice stopped, `Notice` means voice works and has made a choice for
    /// him. Collapsing them would put a failure face on a working feature, and the UI proves the
    /// difference — `voice-error` also flips the panel out of voice mode.
    Notice { message: String, at: u64 },
}

impl VoiceEvent {
    pub fn event_name(&self) -> &'static str {
        match self {
            VoiceEvent::State { .. } => EVENT_VOICE_STATE,
            VoiceEvent::Transcript { .. } => EVENT_VOICE_TRANSCRIPT,
            VoiceEvent::Error { .. } => EVENT_VOICE_ERROR,
            VoiceEvent::Notice { .. } => EVENT_VOICE_NOTICE,
        }
    }

    pub fn payload(&self) -> Value {
        match self {
            VoiceEvent::State { state, level, barge_in_armed, no_audio, at } => json!({
                "state": state.as_str(),
                "level": level,
                "bargeInArmed": barge_in_armed,
                "noAudio": no_audio,
                "at": at,
            }),
            VoiceEvent::Transcript { text, duration_ms, latency_ms, at } => json!({
                "text": text,
                "durationMs": duration_ms,
                "latencyMs": latency_ms,
                "at": at,
            }),
            VoiceEvent::Error { message, at } | VoiceEvent::Notice { message, at } => {
                json!({ "message": message, "at": at })
            }
        }
    }
}

/// A sink for voice events. `Send + Sync` because the audio threads emit from wherever they
/// are; the Tauri shell implements it with `app_handle.emit`, tests with a recording Vec.
pub trait VoiceObserver: Send + Sync {
    /// MUST be non-blocking. The audio callback thread is on the other end of this: a sink
    /// that blocks here drops frames and clicks the CEO's audio.
    fn on_voice_event(&self, event: &VoiceEvent);

    /// On a controller thread. Run the cut while excluding native request
    /// preparation, then record its results. The audio callback never calls this.
    fn record_reply_cut(&self, cut: &mut dyn FnMut() -> Vec<crate::playback::ReplyCutoff>) {
        cut();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// INVARIANT: the payload keys are camelCase and match what app/ui/main.js reads.
    #[test]
    fn state_payload_keys_are_the_ones_the_webview_reads() {
        let e = VoiceEvent::State {
            state: VoiceState::Speaking,
            level: 0.42,
            barge_in_armed: true,
            no_audio: false,
            at: 1_700_000_000_000,
        };
        assert_eq!(e.event_name(), "rich://voice-state");
        let p = e.payload();
        assert_eq!(p["state"], "speaking");
        assert_eq!(p["bargeInArmed"], true);
        assert!((p["level"].as_f64().unwrap() - 0.42).abs() < 1e-6);
        assert_eq!(p["at"], 1_700_000_000_000u64);
    }

    /// INVARIANT: the silent-input verdict reaches the webview as `noAudio`, on the SAME
    /// event that carries the mic state — so the UI can never render "listening" and the
    /// no-audio line from two events that arrived out of order.
    #[test]
    fn the_silent_input_verdict_rides_the_state_event_as_no_audio() {
        let e = VoiceEvent::State {
            state: VoiceState::Listening,
            level: 0.0,
            barge_in_armed: false,
            no_audio: true,
            at: 7,
        };
        let p = e.payload();
        assert_eq!(p["noAudio"], true);
        assert_eq!(p["state"], "listening", "the mic IS open — that is the whole point");
        assert_eq!(e.event_name(), EVENT_VOICE_STATE, "not an error event: it self-clears");
    }

    /// INVARIANT: a transcript event carries MEASURED numbers, and they survive the round
    /// trip into JSON as numbers (not strings the UI would have to parse).
    #[test]
    fn transcript_payload_carries_measured_numbers_as_numbers() {
        let e = VoiceEvent::Transcript {
            text: "renegotiate Acme".into(),
            duration_ms: 3096,
            latency_ms: 470,
            at: 1,
        };
        let p = e.payload();
        assert_eq!(p["text"], "renegotiate Acme");
        assert_eq!(p["durationMs"].as_u64(), Some(3096));
        assert_eq!(p["latencyMs"].as_u64(), Some(470));
    }

    /// INVARIANT: the event names are stable — they are a published contract.
    #[test]
    fn the_voice_event_names_are_stable() {
        assert_eq!(EVENT_VOICE_STATE, "rich://voice-state");
        assert_eq!(EVENT_VOICE_TRANSCRIPT, "rich://voice-transcript");
        assert_eq!(EVENT_VOICE_ERROR, "rich://voice-error");
        assert_eq!(EVENT_VOICE_NOTICE, "rich://voice-notice");
    }

    /// INVARIANT: a notice is NOT an error, on the wire as well as in meaning.
    ///
    /// They carry an identical payload shape and mean opposite things — one says voice stopped,
    /// the other says voice is running and made a choice for him. The UI proves the difference
    /// costs something: `rich://voice-error` also flips the panel out of voice mode, so a
    /// degradation sent down that channel would end the conversation it was reporting on.
    #[test]
    fn a_notice_is_delivered_on_its_own_channel_and_never_as_an_error() {
        let n = VoiceEvent::Notice { message: "I'm using my faster hearing.".into(), at: 9 };
        let e = VoiceEvent::Error { message: "I'm using my faster hearing.".into(), at: 9 };
        assert_eq!(n.event_name(), EVENT_VOICE_NOTICE);
        assert_ne!(n.event_name(), e.event_name(), "same words, different channel");
        assert_eq!(n.payload(), e.payload(), "and an identical payload shape for the UI");
        assert_eq!(n.payload()["message"], "I'm using my faster hearing.");
        assert_eq!(n.payload()["at"].as_u64(), Some(9));
    }
}

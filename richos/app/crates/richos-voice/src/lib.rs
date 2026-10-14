//! **richos-voice** — voice as a MODE of the one persistent conversation with Rich.
//!
//! Not a room, not a call, not a huddle. The CEO taps once, talks, and everything he says
//! and everything Rich says lands in the SAME durable thread the typed conversation uses
//! (the RichOS front-end notes §Voice; the v1 front-end UX direction §4).
//! Nothing here is ephemeral.
//!
//! # Audio testing on this Mac — read before touching barge-in, the canceller or the gates
//!
//! **AUDIO TESTING ON THIS MAC — a standing limitation (CEO, 2026-09-18).** The Mac's own 
//! speakers are the only sound source an agent has. Anything played with `say`, `afplay` or the 
//! app's own playout goes through the SAME loudspeaker as Rich's voice, takes the same acoustic 
//! path into the microphone at the same level, and is therefore indistinguishable from Rich's 
//! echo to the canceller and to every gate downstream of it. It cannot stand in for a person at 
//! the microphone. So an interruption, barge-in, talk-over or "the CEO speaks while Rich 
//! speaks" test is NEVER done with `say` on this Mac. It is done either (a) on the committed 
//! echo-path fixture with a near-end voice recording superimposed on the microphone track at a 
//! stated near-field level, (b) by a human at the desk, or (c) by a separate sound source (a 
//! second device) near the microphone. A defect "found" with the Mac's own speakers standing in 
//! for the human is a harness artifact until reproduced by (a), (b) or (c) — that is exactly 
//! how the candidate-.6 "onset defect" of 2026-09-17 came about. Barge-in itself has existed 
//! since 2026-08-29 (`bargein.rs`). And at most ONE live playback per measurement, never a loop 
//! (CEO, 2026-09-17).
//! See `TESTING.md` beside `Cargo.toml`.
//!
//! ```text
//!   mic ──► capture ──► resample to 16 kHz ──► echo canceller (aec.rs)
//!                                                  │
//!                                                  ▼
//!                        VAD (16.000 ms frames) ──┬──► endpointer ──► utterance
//!                                                 │                      │
//!                                                 └──► barge-in monitor  │  whisper.cpp (local)
//!                                                          │             ▼
//!                                                          │        transcript
//!                                                          │             │
//!                                                          │             ▼
//!                                                          │   THE SAME SPINE TURN as typed
//!                                                          │      text (Source::Jam)
//!                                                          │             │
//!                                                          │      rich://chunk deltas
//!                                                          │             ▼
//!                                                          │      sentence chunker
//!                                                          │             ▼
//!                                                          └──stop── TTS ──► playout ──► speakers
//!                                                                              │
//!                                                                     reference signal
//!                                                              back over a lock-free ring
//! ```
//!
//! Layering, deliberately: `vad`, `bargein`, `endpoint`, `chunk`, `noaudio`, `state` and `wav` are
//! **pure and unit-tested** — no devices, no subprocesses, no clock. `capture`, `playout`,
//! `stt` and `tts` are the thin native edges. `controller` is the glue that owns the threads.
//!
//! **The honest v1 gaps, stated up front** (see the voice-pipeline brief, 2026-08-24):
//! - **Acoustic echo cancellation now exists** ([`aec`]) and is measured: 28.0 dB ERLE on a
//!   linear path, bit-transparent while Rich is silent, and it shortens the barge-in debounce
//!   from 5.008 s to 0.400 s once it has measured its own residual echo 6 dB below the VAD's
//!   speech threshold. **On the CEO's own hardware it cannot reach that bar**, because the
//!   loudspeaker-to-microphone path there is only ~5.5 dB linearly predictable (coherence-
//!   bounded, so no canceller of any kind can do better) — so the 5.008 s rule and "headphones
//!   recommended" remain in force there. `examples/aec_probe.rs` is the evidence and
//!   `aec.rs`'s module docs state the whole finding.
//! - STT is **utterance-endpointed**, not token-streaming: whisper.cpp runs once per finished
//!   utterance. There is no live partial transcript.
//! - TTS is Kyutai Pocket TTS speaking as Peter Yearsley (`pocket.rs`, CEO 2026-10-14), behind
//!   the [`tts::SpeechSynth`] trait; macOS `say` speaks until its pinned files are on the machine.

pub mod aec;
pub mod bargein;
pub mod chunk;
// Dictation in any app: every rule the dictation tool decides by (the key, the session, the
// evidence gate, the decode bound, the model, the paste), pure and unit-tested; the tool's macOS
// edges live in the Tauri shell (`src-tauri/src/dictation/`).
pub mod dictation;
// The dictation bar, the words' flight and the menu bar item (dictation plan slice 3): what each
// shows, how long, where it sits, the meter's rate and the item's picture, pure and unit-tested.
pub mod dictation_bar;
pub mod fft;
pub mod endpoint;
pub mod event;
// Which model this machine can carry, read off the machine rather than compiled in. Pure over a
// `Machine` struct and a measurement closure, so every resolution rule is decided in a unit test
// without a decoder, a WAV or a subprocess; the only impure parts are three sysctl reads and the
// speed cache. It exists because a repo-wide grep for `sysctl`/`hw.memsize`/`sysinfo` across
// `app/crates` used to return nothing at all.
pub mod hardware;
pub mod noaudio;
// Getting the pinned speech model ONTO this machine. Pure judgment plus local filesystem I/O —
// no sockets, no async runtime, no new dependency — so every failure path (a captive portal, a
// corrupted body, a truncated transfer, a full disk) is decided in a unit test without a network.
// The transport lives in the Tauri shell and makes no judgments of its own, which is the same
// split `engine/voice/provisioning/{model-integrity,model-fetch}.js` already uses.
pub mod provision;
// Finding the one echo-path fixture that is NOT committed here, because the CEO's own voice is in
// it and `richos` is public. `build.rs` `include!`s this same file so the compile-time
// `private_fixtures` cfg and the run-time file open can never disagree about where it is; see the
// module docs for the two locations and `build.rs` for why a missing fixture must report
// `ignored` and never `ok`.
pub mod private_fixtures;
pub mod state;
// Which whisper binary, which ggml backends and which weights are about to hear the CEO. Pure
// except for four I/O functions, and native-dependency-free — it hashes with `sha2` rather than
// shelling out, for the reason richos-core's own manifest gives: an integrity check whose answer
// depends on parsing a subprocess's stdout is the exact shape of "exit 0 while doing the wrong
// thing", which is also what whisper-cpp 1.8.3 does to `--version`.
pub mod toolchain;
// When the CEO has finished his TURN, as opposed to when one stretch of his speech ended: the
// endpointer's 0.800 s closes a fragment, and fragments are joined into one turn until he has
// been silent for 2.000 s. Pure; the supervisor feeds it.
pub mod turn;
pub mod voiced;
pub mod wav;

pub mod capture;
pub mod controller;
pub mod playout;
// Rich's voice: Pocket TTS (ONNX Runtime, statically linked) with the Peter Yearsley preset.
pub mod pocket;
pub mod stt;
pub mod tts;
pub mod vad;

/// The opt-in gate for the tests that open real audio hardware.
///
/// **The defect this replaces.** These tests used to begin `if env::var(…) != Ok("1") {
/// return; }`, and a test that returns is reported `ok`. Four of them were green lines
/// asserting nothing on every machine that had not opted in, including `app-voice-ci.yml`'s
/// runner. They now carry `#[cfg_attr(not(live_audio), ignore = …)]`, so libtest prints
/// `ignored, <reason>` and counts them in its own `N ignored` column — see `build.rs` for
/// why the cfg exists and how `RICHOS_VOICE_LIVE_AUDIO=1` still runs them.
///
/// This function closes the one hole the attribute leaves: `#[ignore]` suppresses the RUN,
/// not the BODY, so `cargo test -- --include-ignored` without an opt-in would still open a
/// device and make noise.
#[cfg(test)]
pub(crate) mod live_audio {
    /// Refuse, loudly, to run a live-audio test body that nobody asked for.
    ///
    /// Reachable only via `--include-ignored` / `--ignored` without `RICHOS_VOICE_LIVE_AUDIO=1`
    /// (under a normal run the test is `ignored` and never enters). It is the same condition
    /// the old silent `return` tested, with the opposite failure mode: a red line that says
    /// what to do, never a green one that says nothing.
    pub fn require_opt_in() {
        assert!(
            std::env::var("RICHOS_VOICE_LIVE_AUDIO").as_deref() == Ok("1"),
            "this test opens real audio hardware and was run without opting in — it is \
             `#[ignore]`d by default and you reached it with `--include-ignored` or \
             `--ignored`. Re-run as `RICHOS_VOICE_LIVE_AUDIO=1 cargo test -p richos-voice`, \
             which un-ignores it via build.rs. Refusing to open a device rather than \
             reporting a pass over one that was never opened."
        );
    }
}

pub use chunk::{speakable, SentenceChunker};
pub use controller::{VoiceController, VoiceOptions};
pub use event::{VoiceEvent, VoiceObserver};
pub use state::{VoiceState, VoiceStateMachine};
pub use bargein::{barge_in_debounce_secs, BargeInMonitor, EchoGate, NoEchoCancellation, BARGE_IN_DEBOUNCE_FRAMES};
pub use noaudio::{no_audio_window_secs, NoAudioDetector, LIVE_RMS, NO_AUDIO_FRAMES, SILENCE_RMS};
pub use vad::{frames_for_secs, frames_to_secs, Vad, SAMPLE_RATE, VAD_FRAME_SAMPLES};
pub use voiced::{voiced_run_secs, VoiceEvidence, PITCH_MOVEMENT_PERCENT, VOICED_PEAK, VOICED_RUN_WINDOWS};

//! The controller — the only stateful thing in the crate, and the only place threads live.
//!
//! Four threads, each with one job:
//!
//! | thread | job | must not |
//! |---|---|---|
//! | audio capture callback | VAD, barge-in monitor, utterance buffering | allocate much, block, or emit UI events |
//! | supervisor (40 Hz tick) | own the state machine, emit every UI event, dispatch work | do audio work |
//! | recognizer | whisper.cpp per finished utterance | block the mic |
//! | speaker | synthesize sentences and queue them for playout | block the turn |
//!
//! The capture callback owns its VAD/recorder/monitor outright — no locks — and talks to the
//! supervisor over a channel plus two atomics. That is deliberate: a `Mutex` on the audio
//! thread is how a voice pipeline starts clicking.
//!
//! ## The half-duplex TAINT rule — now conditional, and why
//!
//! While Rich is audible the microphone stays open (barge-in needs it), but any utterance that
//! BEGINS during his playout used to be marked **tainted** and discarded unless barge-in
//! actually fired during it. Without that rule, on speakers Rich hears himself, transcribes his
//! own sentence and answers it — the pilot's echo failure, one step worse.
//!
//! The rule's cost was stated plainly here: on speakers, the CEO could not start a new thought
//! while Rich was talking. He had to talk over him for the full 5.008 s debounce or tap "stop".
//! These docs said **"Real AEC deletes this rule. It is interim, and it is named as interim."**
//!
//! It is now deleted, conditionally and honestly. When [`crate::aec::EchoCanceller::confident`]
//! is true — the canceller has MEASURED its residual echo 6 dB below the VAD's speech floor and
//! held it there for 2.000 s — Rich's voice is no longer meaningfully present in the frames the
//! recorder buffers, so an utterance beginning while he speaks is not echo, it is the CEO
//! starting a sentence. It is kept.
//!
//! Whenever the canceller is NOT confident, the taint rule is exactly what it was. That is not
//! a hedge; it is the same fallback the barge-in debounce uses, driven by the same measurement.

use crate::aec::EchoCanceller;
use crate::bargein::{BargeInMonitor, BargeInMode, BARGE_IN_DEBOUNCE_FRAMES};
use crate::capture::{self, AudioSource, Capture};
use crate::chunk::SentenceChunker;
use crate::endpoint::{UtteranceRecorder, Utterance};
use crate::event::{VoiceEvent, VoiceObserver};
use crate::noaudio::{no_audio_window_secs, NoAudioDetector};
use crate::playout::Playout;
use crate::state::{VoiceState, VoiceStateMachine};
use crate::stt::{self, Recognizer};
use crate::tts::{RichVoice, SpeechSynth, TtsError};
use crate::turn::{may_sound, rich_audible, AnswerGate, TurnAssembler, Unheard};
use crate::vad::{frames_to_secs, Vad};
use crate::voiced::VoiceEvidence;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Supervisor tick. 25 ms is faster than the eye notices and far slower than the 16.000 ms
/// audio frame, so the UI never lags the mic and the tick never competes with audio.
const TICK: Duration = Duration::from_millis(25);

/// **HOW FAR BACK AN UTTERANCE'S RECORDING ACTUALLY REACHES, in VAD frames.**
///
/// `CapMsg::Started` does not mean "the first recorded sample is now". The recorder keeps
/// [`crate::endpoint::PRE_ROLL_FRAMES`] of audio from BEFORE onset (so whisper hears the first
/// consonant), and the onset itself takes [`crate::endpoint::SPEECH_ONSET_FRAMES`] to confirm.
/// So the WAV that reaches the recognizer begins:
///
/// ```text
///   PRE_ROLL_FRAMES + SPEECH_ONSET_FRAMES = 19 + 7 = 26 frames
///   26 x 256 / 16000 = 0.416 s before `Started` fires
/// ```
///
/// A taint test that only looks at `speaking` ON the `Started` frame therefore ignores 0.416 s
/// of audio that is already in the buffer — and whisper transcribes the whole buffer. If Rich
/// was audible anywhere in that window, his voice is in the file.
const ECHO_LOOKBACK_FRAMES: u32 =
    crate::endpoint::PRE_ROLL_FRAMES as u32 + crate::endpoint::SPEECH_ONSET_FRAMES;

/// How often a synthesized sentence held for the floor asks again. Well under one 16.000 ms
/// frame, so it plays within a frame of his giving the floor back; it is a poll interval, not a
/// verdict.
const FLOOR_POLL: Duration = Duration::from_millis(10);

/// Emit a level update at most this often — a 62.5 Hz meter is wasted work in a webview.
const LEVEL_EMIT_EVERY: Duration = Duration::from_millis(100);

pub fn now_millis() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// `HH:MM:SS.mmmZ` in UTC, with no dependency and no allocation beyond the string.
///
/// **It exists because a log line that cannot be lined up against the ledger is not evidence.**
/// Ray's candidate-.5 walk could establish that Rich's own counting was submitted as the CEO's
/// prompt at `20:06:43.693Z`, and could NOT establish whether the utterance began in a gap
/// between spoken sentences or after playout had been marked ended — because `app.log` carries
/// no timestamps at all. The ledger stamps in UTC milliseconds; so does this.
pub fn wall_clock_utc() -> String {
    let ms = now_millis();
    let secs = ms / 1000;
    format!("{:02}:{:02}:{:02}.{:03}Z", (secs / 3600) % 24, (secs / 60) % 60, secs % 60, ms % 1000)
}

/// One edge of a boolean that is being watched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Rose,
    Fell,
}

/// **THE WINDOW IN WHICH RICH IS AUDIBLE — the whole of an answer, not the queue's state.**
///
/// The flag the capture path calls `speaking` used to be `playout.is_playing()`, which is
/// `queued_samples() > 0`. Ray's candidate-.5 walk proved that insufficient on the CEO's rig:
/// Rich answered *"Please count slowly out loud from 1 to 20"*, and his own *"One... two...
/// three... four... five..."* came back through the Wave:3, was recognized, and was submitted
/// as the CEO's message at `20:06:43.693Z` — a model turn spent on words he never said.
///
/// A queue-depth test is false in three places inside one answer:
///
/// | hole | why the queue is empty | covered by |
/// |---|---|---|
/// | before the first sample | `say` has not finished spawning | `owed` |
/// | between spoken sentences | synthesis of N+1 is slower than playback of N | `owed` |
/// | after the last sample | the device has it, the room has not heard it yet | `hold` |
///
/// The second is the one that matters most here. Deltas chunk on runs of `.`/`!`/`?`
/// ([`crate::chunk`]), so `One... two... three...` becomes roughly twenty one-word sentences
/// and therefore twenty separate `say` spawns, each with its own gap.
///
/// `hold` is MEASURED, never assumed — see [`audible_hold_secs`].
///
/// Pure: driven by a millisecond count the caller supplies, so the whole rule is unit-testable
/// without a device, a clock or a sound.
pub struct AudibleWindow {
    open: bool,
    queued: bool,
    /// The last observation at which Rich was queued or owed.
    last_live_ms: u64,
}

impl Default for AudibleWindow {
    fn default() -> Self {
        AudibleWindow::new()
    }
}

impl AudibleWindow {
    pub fn new() -> Self {
        AudibleWindow { open: false, queued: false, last_live_ms: 0 }
    }

    /// Is Rich audible right now, in the sense the taint rule needs?
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Advance the window.
    ///
    /// - `queued` — the playout queue is non-empty.
    /// - `owed` — at least one sentence is between the speaker channel and the queue.
    /// - `hold_secs` — how long the window stays open after the last of both, from
    ///   [`audible_hold_secs`].
    /// - `now_ms` — a MONOTONIC millisecond count. Never the wall clock: the wall clock can
    ///   step backwards and a half-duplex window that closes early is the whole defect.
    ///
    /// Returns `(answer edge, chunk edge)` — the first is the extended window, the second is
    /// the raw queue. Both are logged, because the difference between them IS the diagnosis
    /// Ray could not make from the record.
    pub fn observe(
        &mut self,
        queued: bool,
        owed: bool,
        hold_secs: f32,
        now_ms: u64,
    ) -> (Option<Edge>, Option<Edge>) {
        let live = queued || owed;
        if live {
            self.last_live_ms = now_ms;
        }
        let hold_ms = (hold_secs.max(0.0) * 1000.0).ceil() as u64;
        // The tail only holds a window that was already open. Nothing has played, nothing to
        // hold.
        let within_hold =
            !live && self.open && now_ms.saturating_sub(self.last_live_ms) < hold_ms;
        let open = live || within_hold;

        let answer = match (self.open, open) {
            (false, true) => Some(Edge::Rose),
            (true, false) => Some(Edge::Fell),
            _ => None,
        };
        let chunk = match (self.queued, queued) {
            (false, true) => Some(Edge::Rose),
            (true, false) => Some(Edge::Fell),
            _ => None,
        };
        self.open = open;
        self.queued = queued;
        (answer, chunk)
    }
}

/// **HOW LONG RICH STAYS AUDIBLE AFTER THE LAST SAMPLE LEAVES THE QUEUE.** Every term is a
/// measurement taken from the running stream; nothing here is a constant someone chose.
///
/// ```text
///   output device tail   Playout::audible_tail_secs()  the DAC has it, the room does not yet
/// + input device latency Capture::input_latency_secs() this frame's audio is already that old
/// + one VAD frame        0.016 s                       the slicer's maximum hold, 256/16000
/// ```
///
/// The input term is there because the capture callback reads `speaking` at PROCESSING time
/// while judging audio that was in the room earlier; the slicer term because `FrameSlicer`
/// holds up to 255 samples waiting to complete a 256-sample frame.
///
/// Floored at one [`TICK`], because this hold is evaluated once per tick: a window shorter
/// than the interval at which it is sampled cannot be enforced, and claiming otherwise would
/// be a timing assertion the code cannot keep.
///
/// Measured on the CEO's rig on 2026-09-17 (Mac mini Speakers out, Elgato Wave:3 in):
/// output tail 10.7 ms, so the sum is well under one 25 ms tick and the floor is what applies.
pub fn audible_hold_secs(out_tail_secs: f32, in_latency_secs: f32) -> f32 {
    let slicer = frames_to_secs(1);
    (out_tail_secs + in_latency_secs + slicer).max(TICK.as_secs_f32())
}

#[derive(Debug)]
pub enum VoiceStartError {
    Capture(capture::CaptureError),
    Playout(crate::playout::PlayoutError),
    Stt(stt::SttError),
}

impl VoiceStartError {
    /// The calm line the CEO sees when the `◉` toggle cannot come up.
    pub fn ceo_message(&self) -> String {
        match self {
            VoiceStartError::Capture(e) => e.ceo_message(),
            VoiceStartError::Playout(e) => e.ceo_message(),
            VoiceStartError::Stt(e) => e.ceo_message(),
        }
    }
}

impl std::fmt::Display for VoiceStartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VoiceStartError::Capture(e) => write!(f, "{e}"),
            VoiceStartError::Playout(e) => write!(f, "{e}"),
            VoiceStartError::Stt(e) => write!(f, "{e}"),
        }
    }
}

/// How voice mode was configured — every field is a fact worth reporting, never a guess.
#[derive(Debug, Clone)]
pub struct Diagnostics {
    pub input_source: String,
    pub input_rate: u32,
    pub input_channels: u16,
    pub output_device: String,
    pub output_rate: u32,
    pub output_channels: u16,
    pub stt_model: String,
    pub stt_binary: String,
    pub echo_gate: String,
    pub echo_cancellation: bool,
    /// The CONSECUTIVE fallback: 313 frames = 5.008 s. In force only when there is no canceller
    /// at all, which since 2026-09-18 is the only case [`crate::bargein::BargeInMode::Consecutive`]
    /// covers.
    pub barge_in_frames: u32,
    pub barge_in_secs: f32,
    /// The near-end-gated window, which is in force **from the first frame** whenever a canceller
    /// is present: [`crate::bargein::AEC_BARGE_IN_REQUIRED_FRAMES`] of
    /// [`crate::bargein::AEC_BARGE_IN_WINDOW_FRAMES`].
    ///
    /// **It used to be reported as what the canceller could EARN, and that was true and is now
    /// wrong.** Earning it meant [`crate::aec::EchoCanceller::confident`], which on the CEO's
    /// hardware is unreachable by measurement — so the line promised something his rig could
    /// never reach and then printed the 5.008 s fallback as though it were temporary. It is no
    /// longer conditional on confidence, so it is no longer reported as a prospect.
    pub barge_in_window_frames: u32,
    pub barge_in_required_frames: u32,
    pub barge_in_required_secs: f32,
    pub barge_in_window_secs: f32,
    /// Is the monitor being fed the conjunction (VAD speech AND the canceller's near-end
    /// verdict)? `true` whenever a canceller is present. This is the one bit that decides which
    /// of the two rules above is running, so the line states it rather than implying it.
    pub barge_in_near_end_gated: bool,
}

impl Diagnostics {
    /// One line for stderr at voice-mode start. Developer-facing only — never the CEO's view.
    ///
    /// **IT NAMES THE DEBOUNCE AS THE FALLBACK IT IS.** Until 2026-09-17 this line read
    /// `aec=PBFDAF 2048 taps (128 ms tail) · barge-in=313 frames (5.008 s)` and stopped
    /// there, which reads as a settled capability — two exact figures, no qualifier. It is
    /// not one. `313` is the debounce in force AT START and only while the canceller has not
    /// proven itself; the code that fills this struct has always known that and said so in a
    /// comment, while the line it printed did not.
    ///
    /// That gap was read off the running app by audit-3 (§4 #1): the boot line advertised
    /// barge-in, the runtime log said the opposite, and both were taken at face value. A
    /// developer line that has to be read alongside a source comment to mean what it says is
    /// a line that will be misread again, so it now carries both numbers and which one is
    /// running.
    ///
    /// **AMENDED 2026-09-18, and the amendment is the whole point of the paragraph above.** The
    /// line used to read `barge-in=313 frames (5.008 s) until the canceller proves itself, then
    /// 25 frames (0.400 s)`. That named the fallback honestly and still misled, because on the
    /// CEO's hardware the canceller can NEVER prove itself — `confident()` requires the residual
    /// 6 dB under the VAD's speech floor and the coherence of that echo path caps any linear
    /// canceller at 4.3-5.5 dB. So *"until"* described a wait that does not end, and the 5.008 s
    /// figure was not a start-up condition, it was the permanent behavior. Barge-in no longer
    /// depends on confidence, so the line now states the rule that IS running and the one
    /// condition under which the other applies.
    pub fn summary(&self) -> String {
        let barge_in = if self.barge_in_near_end_gated {
            format!(
                "barge-in={} of {} frames ({:.3} s of near-end speech inside {:.3} s), gated on the canceller's per-frame near-end verdict from the first frame",
                self.barge_in_required_frames,
                self.barge_in_window_frames,
                self.barge_in_required_secs,
                self.barge_in_window_secs,
            )
        } else {
            format!(
                "barge-in={} consecutive frames ({:.3} s) — NO canceller, so a bare VAD verdict is all there is",
                self.barge_in_frames, self.barge_in_secs,
            )
        };
        format!(
            "in={} {} Hz/{} ch · out={} {} Hz/{} ch · stt={} · aec={} · {barge_in}",
            self.input_source,
            self.input_rate,
            self.input_channels,
            self.output_device,
            self.output_rate,
            self.output_channels,
            self.stt_model,
            self.echo_gate,
        )
    }
}

#[derive(Debug, Clone)]
pub struct VoiceOptions {
    pub source: AudioSource,
    /// Where the utterance WAVs and synthesis WAVs live. Under the app data dir in the shell.
    pub scratch_dir: PathBuf,
}

impl Default for VoiceOptions {
    fn default() -> Self {
        VoiceOptions {
            source: AudioSource::from_env(),
            scratch_dir: std::env::temp_dir().join("richos-voice"),
        }
    }
}

/// Messages from the audio callback to the supervisor. Small and allocation-light except for
/// `Utterance`, which is the whole point and happens once per sentence the CEO speaks.
#[derive(Debug)]
pub enum CapMsg {
    /// The CEO started talking. `tainted` = it began while Rich was audible; `rich_audible` =
    /// the same observation WITHOUT the two escapes — see [`AdmittedUtterance::rich_audible`].
    Started { tainted: bool, rich_audible: bool },
    /// An utterance completed and is worth recognizing, with its provenance.
    Utterance(Box<AdmittedUtterance>),
    /// An utterance completed but was discarded (too short, or tainted echo).
    Discarded { tainted: bool },
    /// The post-open silent-input verdict CHANGED (`noaudio.rs`). `silent` = the stream is
    /// open and healthy but has delivered nothing above -80.00 dBFS for 3.008 s. Sent once
    /// per transition, never once per frame.
    NoAudio { silent: bool },
    /// Rich was cut off — either by the full debounce, or by "tap to stop".
    ///
    /// `mid_utterance` says whether the CEO is actually talking at that instant. A debounce
    /// barge-in always is (that is what fired it); a "tap to stop" often is NOT — he can hit
    /// the button in silence. Without this the state machine would be told "he is talking"
    /// and, with no utterance to end, would sit in `Hearing` forever.
    BargeIn { mid_utterance: bool },
    /// **His turn is over**: [`crate::turn::END_OF_TURN_FRAMES`] consecutive frames the VAD did
    /// not call speech (125 x 256 / 16000 = 2.000 s), after an utterance has ended. Sent once per
    /// silence, never once per frame. Counted here, on the frames themselves, so the window is
    /// exact rather than a supervisor-tick estimate, and an utterance cut at the 30.000 s window
    /// while he is still talking does not start it early.
    TurnQuiet,
    /// **His turn reached the hard bound** ([`crate::turn::MAX_TURN_FRAMES`], 60.000 s from its
    /// first start) without that quiet. The open recording has just been closed (its `Utterance`
    /// or `Discarded` comes first); what he said up to here is his turn, even if more is already
    /// being said ([`crate::turn::TurnAssembler::time_limit`]).
    TurnLimit,
}

/// **AN ADMITTED UTTERANCE AND WHAT THE CAPTURE PATH KNEW ABOUT RICH'S OWN VOICE.**
///
/// The gap this closes, from the voice pipeline's own handoff (richos `c712ccd5`): a
/// `PromptReceived` with `source: Source::Jam` carried **no provenance at all**, so an
/// echo-born spoken turn and a genuine one were indistinguishable after the fact. Candidate
/// .5 left `"1, 2, 3, 4, 5."` — Rich's own counting — in the CEO's thread as the CEO's
/// message, and it is still sitting there because it was a real message; nothing recorded
/// which of the two it was.
///
/// A struct rather than a second field on [`Utterance`] because [`UtteranceRecorder`] knows
/// nothing about Rich and must not start to: it buffers audio, and whose audio it is is a fact
/// the [`CaptureBrain`] owns.
#[derive(Debug, Clone)]
pub struct AdmittedUtterance {
    pub utterance: Box<Utterance>,
    /// **Was Rich's audible window open during ANY part of this recording?**
    ///
    /// Observed at exactly the two points taint is — at `Started`, over the
    /// [`ECHO_LOOKBACK_FRAMES`] the recorder actually reaches back through, and re-evaluated
    /// on every frame for as long as the utterance is alive — **but without taint's two
    /// escapes.** `tainted` is `echo_is_in_the_recording && !barged && !confident`; this is
    /// `echo_is_in_the_recording` alone.
    ///
    /// That difference is the entire point. An utterance reaches this struct only by being
    /// admitted, and the two ways an admitted utterance can nevertheless have been recorded
    /// while Rich was audible are precisely those escapes:
    ///
    /// - **a barge-in cleared the taint** — the CEO deliberately talked over him, and that is
    ///   a positive signal that the near-end talker is the CEO;
    /// - **the canceller was confident** — it measured its residual 6 dB under the VAD's
    ///   speech floor for 2.000 s and vouched for it.
    ///
    /// Both produce genuine turns. So `true` is **not** a claim that this is echo; it is the
    /// narrow, measured fact that the window overlapped the audio, which is the one thing
    /// nothing downstream can reconstruct and the one thing a later investigation needs.
    pub rich_audible: bool,
}

/// **A STRICT WRAPPER, so an [`AdmittedUtterance`] still reads as the utterance it carries.**
///
/// `Deref` because this type adds an annotation and takes nothing away: everything that was
/// true of the `Utterance` is still true, and every existing reader — the endpointer's own
/// integration tests, `self_voice_replay`, `barge_in_composition` — asks it the same questions
/// it always did. Without this, adding one provenance field would rewrite a dozen assertions
/// that have nothing to do with provenance, and a diff that large around an audio path is how
/// a real regression gets reviewed past.
///
/// Source code that reads the provenance uses the explicit `u.utterance` / `u.rich_audible`
/// pair, so the one place the two facts are handled together says which is which.
impl std::ops::Deref for AdmittedUtterance {
    type Target = Utterance;
    fn deref(&self) -> &Utterance {
        &self.utterance
    }
}

/// Everything the audio callback decides, in one place, with no I/O and no locks.
///
/// The callback is a THIN adapter over this: it hands over a frame plus two booleans and
/// forwards whatever comes back. That split exists so the barge-in/taint/endpointing
/// COMPOSITION is unit-testable — testing the monitor and the recorder separately would
/// leave the wiring between them (which is where the bugs live) untested.
pub struct CaptureBrain {
    /// Rich's audible window overlapped the utterance currently being recorded. The same
    /// observation `tainted` is built on, MINUS the barge-in and confidence escapes — see
    /// [`AdmittedUtterance::rich_audible`]. Cleared with `tainted` when an utterance ends.
    rich_audible: bool,
    vad: Vad,
    recorder: UtteranceRecorder,
    monitor: BargeInMonitor,
    /// Post-open silent-input watch. Fed the RMS of the SAME frame the recorder buffers —
    /// collector-path parity, so what the CEO is warned about cannot drift from what STT
    /// would actually have received.
    noaudio: NoAudioDetector,
    /// The echo canceller, if one could be started. `None` is the honest fallback and puts
    /// the 5.008 s debounce and the taint rule permanently in force.
    aec: Option<EchoCanceller>,
    /// Scratch for the residual. Preallocated: this runs on the audio callback thread.
    residual: Vec<f32>,
    was_recording: bool,
    /// This utterance overlaps Rich being audible: echo until proven otherwise.
    tainted: bool,
    /// A barge-in fired during the current utterance, which proves it is NOT echo.
    barged: bool,
    /// Consecutive frames on which Rich was NOT audible, saturating. Compared against
    /// [`ECHO_LOOKBACK_FRAMES`] so the pre-roll already sitting in the recorder is judged too,
    /// not just the frame `Started` happens to land on.
    quiet_frames: u32,
    /// Consecutive frames the VAD has NOT called speech, saturating: the silence since his last
    /// speech frame, which is what [`crate::turn::END_OF_TURN_FRAMES`] is measured against.
    since_speech: u32,
    /// An utterance has ended and [`CapMsg::TurnQuiet`] has not yet been sent for the silence
    /// after it, so it is sent exactly once per silence.
    turn_quiet_armed: bool,
    /// A fragment of his current turn has been admitted and Rich has not been audible since: a
    /// short fragment now continues his turn and is not thrown away as a cough (review
    /// rv-20261013T204600Z). It stays open past [`CapMsg::TurnQuiet`], because words he says
    /// after the 2.000 s end of turn and before Rich answers aloud still join that turn
    /// (`TurnDesk::withdraw_unanswered`; review rv-20261013T210558Z, finding 1). Mirrored into
    /// the recorder, which applies the minimum.
    turn_open: bool,
    /// Frames since the first utterance of the turn under way started, 0 when none is: compared
    /// with [`crate::turn::MAX_TURN_FRAMES`], after which the turn ends whatever is heard.
    turn_frames: u32,
}

impl Default for CaptureBrain {
    fn default() -> Self {
        CaptureBrain::new()
    }
}

impl CaptureBrain {
    /// A brain with NO echo cancellation: the 5.008 s consecutive debounce and the taint rule,
    /// exactly as they were before `aec.rs` existed.
    pub fn new() -> Self {
        CaptureBrain {
            rich_audible: false,
            vad: Vad::default(),
            recorder: UtteranceRecorder::new(),
            monitor: BargeInMonitor::default(),
            noaudio: NoAudioDetector::default(),
            aec: None,
            residual: vec![0.0; crate::vad::VAD_FRAME_SAMPLES],
            was_recording: false,
            tainted: false,
            barged: false,
            quiet_frames: u32::MAX,
            since_speech: 0,
            turn_quiet_armed: false,
            turn_open: false,
            turn_frames: 0,
        }
    }

    /// A brain with a real echo canceller. The canceller lives HERE, owned outright by the
    /// capture path, so there is no lock on the audio thread — the only thing shared with the
    /// playout thread is the lock-free reference ring the canceller was built with.
    pub fn with_aec(aec: EchoCanceller) -> Self {
        CaptureBrain { aec: Some(aec), ..CaptureBrain::new() }
    }

    /// The canceller's live figures, for the diagnostics line and the UI.
    pub fn aec_metrics(&self) -> Option<crate::aec::AecMetrics> {
        self.aec.as_ref().map(|a| a.metrics())
    }

    /// Has the canceller measured itself into a position to be believed? This is what
    /// shortens the barge-in debounce and what relaxes the taint rule — nothing else does.
    pub fn aec_confident(&self) -> bool {
        self.aec.as_ref().is_some_and(|a| a.confident())
    }

    /// Which barge-in rule is in force right now.
    pub fn barge_in_mode(&self) -> BargeInMode {
        self.monitor.mode()
    }

    /// Live input level 0..1 for the UI meter.
    pub fn level(&self) -> f32 {
        self.vad.level()
    }

    /// RMS of the frame just pushed — POST-cancellation, i.e. the audio the endpointer and
    /// whisper actually see. The "why was nothing heard" diagnostic reads this against
    /// [`CaptureBrain::speech_threshold`]; the two together answer the question that took a
    /// whole on-screen walk to ask.
    pub fn last_rms(&self) -> f32 {
        self.vad.last_rms()
    }

    /// The RMS the next frame must exceed to be called speech at all — the VAD's adaptive
    /// floor times its ratio, never under its absolute floor. Exposed for the same reason
    /// [`CaptureBrain::barge_run_frames`] is: a threshold nobody can read is a threshold
    /// everybody guesses at.
    pub fn speech_threshold(&self) -> f32 {
        self.vad.speech_threshold()
    }

    /// The VAD's learned room level right now.
    pub fn noise_floor(&self) -> f32 {
        self.vad.noise_floor()
    }

    /// Consecutive speech frames the ENDPOINTER has counted toward the 7-frame (0.112 s)
    /// onset. Distinct from [`CaptureBrain::barge_run_frames`], which counts toward the
    /// 5.008 s interruption debounce — conflating the two is exactly the mistake the
    /// endpointer's module table exists to prevent.
    pub fn onset_run_frames(&self) -> u32 {
        // Zero once an utterance is open: the run is the thing still trying to START one.
        if self.recorder.is_recording() {
            0
        } else {
            self.recorder.endpointer().speech_run_frames()
        }
    }

    /// **He has started to make a sound that may be his next word**: speech frames are building
    /// toward the 7-frame onset while Rich is not audible. The floor ([`crate::turn::TurnAssembler::has_floor`])
    /// only becomes his at the onset, 0.112 s in; a sentence that finishes synthesis inside that
    /// 0.112 s would otherwise play over the start of his word, make it Rich's, and lose it. The
    /// speaker waits on this too ([`crate::turn::may_sound`]); a sound that never becomes an onset
    /// releases it a few frames later.
    ///
    /// **And it holds through the handoff to the floor** (review rv-20261013T192726Z, earlier
    /// finding 2 still open): at the onset frame the run above resets to zero and `Started` goes
    /// to the supervisor, which publishes the floor on its next 25 ms tick. Until then neither
    /// said "his", and a sentence ready in that gap played over his word (the reviewer's 16/25/10
    /// ms schedule: sounded at 120 ms, his word discarded as tainted). So an open recording that
    /// is his (`Started { tainted: false }` was sent) keeps this true; once it closes the
    /// supervisor's floor has long since been published (the close comes 0.800 s of silence
    /// later) and covers what follows.
    pub fn his_onset_building(&self) -> bool {
        let building = self.quiet_frames > 0 && self.onset_run_frames() > 0;
        let his_recording_open = self.recorder.is_recording() && !self.tainted;
        building || his_recording_open
    }

    /// **A reply that starts now would be heard, not discarded.** Rich has been silent for the
    /// whole [`ECHO_LOOKBACK_FRAMES`] (an utterance born sooner carries his echo in its pre-roll
    /// and is tainted), and no tainted recording is still open (one that is would absorb the
    /// reply and be discarded whole when its hangover ends). `HalfDuplexDelivery` waits for this
    /// before asking him to say it again (review rv-20261021T205407Z-bcae8bd4-dcd6).
    pub fn takes_a_reply(&self) -> bool {
        self.quiet_frames >= ECHO_LOOKBACK_FRAMES && !(self.recorder.is_recording() && self.tainted)
    }

    /// **He has just been asked to say it again: what he says next is kept however short.** The
    /// 0.304 s minimum ([`crate::endpoint::MIN_SPEECH_FRAMES`]) is there so a cough is not a
    /// sentence, but a reply to "please say it again" is often a word or two ("yes", "send it"),
    /// and dropping it unheard lost his words a second time with no notice at all (review
    /// rv-20261021T215433Z-1979e6d9-40f2). So the reply is treated like a fragment of an open
    /// turn: admitted from the 7-frame (0.112 s) onset and passed on, and the recognizer still
    /// asks it for a voice of its own, so a cough instead gets that desk's own line with its
    /// next step. Ends, like any open turn, the moment Rich is audible again.
    pub fn invite_reply(&mut self) {
        self.set_turn_open(true);
    }

    /// **Voice is going off: close the open recording and say what became of it** (review
    /// rv-20261013T183507Z). The capture stream is about to be destroyed, and with it whatever he
    /// said in the last 0.800 s, which the endpointer had not yet closed. It is closed here by the
    /// same rules as any other end: admitted if it holds a sentence's worth of speech and is not
    /// Rich's echo, discarded otherwise. Nothing is sent when nothing was being recorded.
    pub fn finish(&mut self) -> Vec<CapMsg> {
        let mut out = Vec::new();
        self.close_now(&mut out);
        self.turn_quiet_armed = false;
        self.turn_frames = 0;
        self.set_turn_open(false);
        out
    }

    /// Close the open recording now, by the rules of any other end: admitted if it holds a
    /// sentence's worth of speech and is not Rich's echo, discarded otherwise. `true` if admitted.
    fn close_now(&mut self, out: &mut Vec<CapMsg>) -> bool {
        let was_recording = self.recorder.is_recording();
        let admitted = match self.recorder.finish_now() {
            Some(utterance) if !self.tainted || self.barged => {
                out.push(CapMsg::Utterance(Box::new(AdmittedUtterance {
                    utterance: Box::new(utterance),
                    rich_audible: self.rich_audible,
                })));
                true
            }
            Some(_) => {
                out.push(CapMsg::Discarded { tainted: true });
                false
            }
            None if was_recording => {
                out.push(CapMsg::Discarded { tainted: self.tainted });
                false
            }
            None => false,
        };
        self.was_recording = false;
        self.tainted = false;
        self.barged = false;
        self.rich_audible = false;
        admitted
    }

    fn set_turn_open(&mut self, open: bool) {
        self.turn_open = open;
        self.recorder.set_turn_open(open);
    }

    /// Consecutive speech frames counted toward an interruption — the diagnostic that makes
    /// "why didn't it barge in" answerable.
    pub fn barge_run_frames(&self) -> u32 {
        self.monitor.run_frames()
    }

    /// Is the open stream currently delivering nothing at all?
    pub fn no_audio(&self) -> bool {
        self.noaudio.no_audio()
    }

    /// Consecutive dead frames counted so far — the "why did/didn't it warn" diagnostic.
    pub fn dead_run_frames(&self) -> u32 {
        self.noaudio.dead_run_frames()
    }

    /// One exact VAD frame. `speaking` = Rich currently has audio playing. `forced` = the UI's
    /// "tap to stop" was pressed since the last frame.
    pub fn push_frame(&mut self, frame: &[f32], speaking: bool, forced: bool) -> Vec<CapMsg> {
        let mut out = Vec::new();

        // ---- ECHO CANCELLATION, first and once ------------------------------------------
        // Everything downstream — the VAD, the endpointer, the recorder, the silent-input
        // watch, and therefore whisper — sees the RESIDUAL, not the raw microphone. That is
        // what "collector-path parity" has to mean once a canceller exists: there is exactly
        // one version of the audio and it is the one that becomes the transcript.
        //
        // While Rich is silent the residual is BIT-IDENTICAL to the raw frame (see
        // `aec::tests::silence_from_rich_leaves_the_microphone_bit_identical`), so dictation
        // and call transcription are provably unaffected by this line.
        let mut buf = std::mem::take(&mut self.residual);
        buf.clear();
        buf.extend_from_slice(frame);
        let near_end = match self.aec.as_mut() {
            Some(aec) if buf.len() == crate::aec::AEC_BLOCK => Some(aec.process_block(&mut buf)),
            _ => None,
        };
        let confident = self.aec.as_ref().is_some_and(|a| a.confident());
        let msgs = self.push_residual(&buf, speaking, forced, near_end, confident, &mut out);
        self.residual = buf;
        let _ = msgs;
        out
    }

    /// The decision half, over the post-cancellation frame. Split out so the borrow of the
    /// scratch buffer is obvious and so tests can drive it directly.
    #[allow(clippy::too_many_arguments)]
    fn push_residual(
        &mut self,
        frame: &[f32],
        speaking: bool,
        forced: bool,
        near_end: Option<bool>,
        confident: bool,
        out: &mut Vec<CapMsg>,
    ) {
        // While Rich is silent the VAD's floor may not climb past the room inside the CEO's speech,
        // or the end of his sentence stops being speech (nightly 65, `vad::ROOM_WINDOW_FRAMES`).
        // While Rich is audible the floor follows his echo exactly as before.
        self.vad.set_cap_to_room(!speaking);
        let is_speech = self.vad.push_frame(frame);

        // How long since Rich was last audible, in frames. Saturating, and updated BEFORE any
        // decision below reads it, so `quiet_frames == 0` means "he is audible on this frame".
        self.quiet_frames = if speaking { 0 } else { self.quiet_frames.saturating_add(1) };
        // Rich is answering aloud: what he says next is a new turn, so the 0.304 s minimum is
        // back. The same moment the supervisor forgets the unanswered turn.
        if speaking && self.turn_open {
            self.set_turn_open(false);
        }

        // COLLECTOR-PATH PARITY: `self.vad.last_rms()` is the RMS of THIS frame — the very
        // buffer handed to `self.recorder.push_frame` below and, from there, to whisper. The
        // silent-input verdict is therefore computed on the recorded audio itself and cannot
        // drift from what a real capture receives. (The echo gate has already run on it in
        // the callback, so it also reflects what STT actually gets, not what the device
        // handed over.) One transition, one message — never one per 16.000 ms frame.
        if self.noaudio.observe(self.vad.last_rms(), speaking) {
            out.push(CapMsg::NoAudio { silent: self.noaudio.no_audio() });
        }

        // The monitor only counts while Rich is audible: speech during his silence is an
        // ordinary utterance, not an interruption.
        if speaking != self.monitor.is_armed() {
            if speaking {
                self.monitor.arm();
            } else {
                self.monitor.disarm();
            }
        }

        // "Tap to stop" is AUTHORITATIVE and bypasses the debounce entirely. Routing it
        // through here (rather than only through the playout queue) is what clears the taint,
        // so the words the CEO is saying right now become the next turn instead of being
        // thrown away as echo.
        if forced {
            self.monitor.disarm();
        }

        // WHICH RULE IS IN FORCE. Driven by WHICH SIGNAL is about to be pushed, and by nothing
        // else — never a setting, never a guess about headphones, and since 2026-09-18 no longer
        // by `EchoCanceller::confident()`.
        //
        // **WHAT COUNTS AS AN INTERRUPTION, and why this line changed.** With a canceller
        // present, require BOTH: the VAD (which knows the room's adaptive noise floor) and the
        // canceller's near-end verdict (which knows how much residual echo to expect at this
        // reference level). Requiring both is what lets the debounce be 0.400 s instead of
        // 5.008 s without Rich cutting himself off.
        //
        // This used to read `Some(n) if confident`, and on the CEO's own hardware `confident()`
        // is false by measurement and always will be — the coherence of that echo path caps any
        // linear canceller at 4.3-5.5 dB, so the residual cannot get 6 dB under the VAD's speech
        // floor (`docs/verification/2026-09-17-aec-erle-on-the-ceo-rig.md`). The `if confident`
        // therefore routed his rig to `is_speech` and the 5.008 s continuity race, and on
        // 2026-09-18 he said *"no matter what I said, he couldn't hear me"*. He was heard, at
        // -18.1 dBFS against a -46.7 dBFS echo on the same microphone, five times; his longest
        // UNBROKEN run was 66 frames = 1.056 s, and the race wanted 313.
        //
        // The two questions are not the same question. `confident()` asks *"is leftover echo too
        // quiet to reach the VAD's threshold?"* — which is what you must know before believing a
        // BARE VAD verdict. The conjunction asks *"is there more here than the canceller predicts
        // for the reference level it can see?"*, which needs no converged filter at all: because
        // `leak_gain` is a minimum statistic, `predicted_echo` is deliberately a LOW estimate,
        // and it starts at its pessimistic 1.0. Measured over 14 echo-only conditions across two
        // recordings of his rig and six volumes, the conjunction's worst 25-frame density is
        // 6 of 25 against the required 15, while his own voice saturates it at 25 of 25. A bare
        // VAD verdict reaches 23 of 25 on echo alone, which is why `None` keeps the old rule.
        self.monitor.set_near_end_gated(near_end.is_some());
        let interrupting = match near_end {
            // A canceller is present: the conjunction is available on every frame, and it is
            // the only signal that separates the CEO from Rich on this hardware.
            Some(n) => is_speech && n,
            // NO canceller at all (`NoEchoCancellation`, or a platform where the real one cannot
            // run). Nothing can tell the two voices apart, so the continuity race is all there
            // is and it stays exactly as it was: 313 consecutive frames = 5.008 s.
            None => is_speech,
        };

        if forced || self.monitor.push(interrupting) {
            self.barged = true;
            self.tainted = false;
            out.push(CapMsg::BargeIn { mid_utterance: self.recorder.is_recording() });
        }

        let finished = self.recorder.push_frame(frame, is_speech);
        let recording = self.recorder.is_recording();
        let started = recording && !self.was_recording;
        let stopped = !recording && self.was_recording;
        self.was_recording = recording;

        if started {
            // **THE TAINT RULE, conditional on the canceller and now on the WHOLE utterance.**
            //
            // With a confident canceller Rich's voice is no longer meaningfully present in
            // `frame` — it has been subtracted, and the residual has been MEASURED 6 dB below
            // the VAD's speech floor for 2.000 s. An utterance beginning while he speaks is
            // therefore the CEO starting a sentence, not an echo of Rich's, and throwing it
            // away is the bug rather than the fix. That half is untouched.
            //
            // What changed on 2026-09-17 is the other half. The rule used to be
            // `speaking && !barged && !confident`, evaluated ONCE, on this frame. Two things
            // were outside it, and Rich's own counting came back through the CEO's speakers
            // and was submitted as his message because of them:
            //
            //   1. **The 0.416 s already in the buffer.** See `ECHO_LOOKBACK_FRAMES`.
            //   2. **Everything after this frame.** An utterance born in a synthesis gap was
            //      born untainted and stayed untainted while Rich spoke over the rest of it —
            //      `SILENCE_HANGOVER_FRAMES` is 0.800 s, so it survives the gaps easily and is
            //      admitted whole. That is the `else if` below.
            //
            // `speaking` itself now covers the whole answer rather than the queue's depth
            // (`AudibleWindow`), so "Rich was audible" finally means what it says.
            let echo_is_in_the_recording = speaking || self.quiet_frames < ECHO_LOOKBACK_FRAMES;
            self.tainted = echo_is_in_the_recording && !self.barged && !confident;
            // **THE PROVENANCE, WHICH IS THE SAME OBSERVATION WITHOUT TAINT'S TWO ESCAPES.**
            // A barge-in and a confident canceller both ADMIT an utterance recorded while Rich
            // was audible, and both are right to — but until 2026-09-17 nothing then recorded
            // that his voice had been in the room while those words were captured. Written
            // here rather than derived later because `quiet_frames` and the lookback are only
            // true at this instant. See `AdmittedUtterance::rich_audible`.
            self.rich_audible = echo_is_in_the_recording;
            out.push(CapMsg::Started { tainted: self.tainted, rich_audible: self.rich_audible });
        } else if recording && !self.tainted && speaking && !self.barged && !confident {
            // **TAINT IS RE-EVALUATED FOR AS LONG AS THE UTTERANCE IS ALIVE.** Rich became
            // audible during an utterance that had already started, and the canceller cannot
            // separate the two voices, so the recording now contains him. Once set it stays
            // set until the utterance ends — only a barge-in clears it, and a barge-in is a
            // positive signal that the CEO is the one talking.
            self.tainted = true;
        }

        // **THE PROVENANCE IS RE-EVALUATED TOO, AND WITH NO CONDITIONS BUT ONE.** Deliberately
        // NOT folded into the branch above: that one honors `!barged && !confident`, and those
        // are exactly the two cases where an utterance carrying Rich's voice is admitted anyway.
        // Folding them together would record `false` for the only turns worth investigating.
        if recording && speaking {
            self.rich_audible = true;
        }

        if let Some(utterance) = finished {
            if self.tainted && !self.barged {
                out.push(CapMsg::Discarded { tainted: true });
            } else {
                out.push(CapMsg::Utterance(Box::new(AdmittedUtterance {
                    utterance: Box::new(utterance),
                    rich_audible: self.rich_audible,
                })));
                self.set_turn_open(true);
            }
            self.tainted = false;
            self.barged = false;
            self.rich_audible = false;
        } else if stopped {
            // Recording ended without an utterance: too short to be a sentence (a cough, a
            // chair). Never reaches whisper, never reaches the CEO.
            out.push(CapMsg::Discarded { tainted: self.tainted });
            self.tainted = false;
            self.barged = false;
            self.rich_audible = false;
        } else if !speaking && !recording {
            // Rich has fallen silent and nothing is in flight: forget the interruption.
            self.barged = false;
        }

        // **THE END OF HIS TURN, counted on the frames, as SILENCE SINCE HIS LAST SPEECH FRAME.**
        // An utterance ends at 0.800 s of silence; his TURN ends at 2.000 s (`turn.rs`). After an
        // ordinary end the hangover's 50 silent frames are already counted, so the quiet comes 75
        // frames (1.200 s) after the END. It is NOT counted from the END itself: an utterance cut
        // at whisper's 30.000 s window ends while he is still talking, and timing from that cut
        // handed his turn over 1.200 s into a 1.408 s pause (review rv-20261013T163358Z, finding
        // 2). Any frame the VAD calls speech restarts the count, so a sound that is about to
        // become his next word is not quiet either.
        self.since_speech = if is_speech { 0 } else { self.since_speech.saturating_add(1) };
        if started {
            self.turn_quiet_armed = false;
        } else if stopped {
            // Admitted, too short, or cut at the window: the utterance is over on this frame.
            self.turn_quiet_armed = true;
        }
        if self.turn_quiet_armed
            && !recording
            && self.since_speech >= crate::turn::END_OF_TURN_FRAMES
        {
            self.turn_quiet_armed = false;
            self.turn_frames = 0;
            out.push(CapMsg::TurnQuiet);
        }

        // **THE HARD BOUND** (`turn::MAX_TURN_FRAMES`, 60.000 s from the turn's first start). The
        // quiet above needs 2.000 s the VAD did not call speech, and nothing guarantees an input
        // ever gives it that; this does. The recording is closed by the ordinary rules, so what
        // he said is admitted, and the turn is handed over. `turn_frames` is 1 on the start's own
        // frame, so the limit falls exactly 3750 frames = 60.000 s after it.
        if self.turn_frames > 0 {
            self.turn_frames += 1;
        } else if started {
            self.turn_frames = 1;
        }
        if self.turn_frames > crate::turn::MAX_TURN_FRAMES {
            if self.close_now(out) {
                self.set_turn_open(true);
            }
            self.turn_quiet_armed = false;
            self.turn_frames = 0;
            out.push(CapMsg::TurnLimit);
        }
    }
}

/// A sentence handed to the speaker thread, tagged with the generation it belongs to so a
/// barge-in can invalidate work already in flight.
#[derive(Clone)]
struct SpeakMsg {
    generation: u64,
    text: String,
    /// `Some` for an announcement (a job's result, [`VoiceController::speak_result`]), which
    /// belongs to no answer: its id in [`Shared::unheard`], given at [`dispatch`]. An
    /// interruption that ends its generation does not end IT; it is said again until heard
    /// (review rv-20261015T132600Z-fdc3dafb, finding 1).
    notice: Option<u64>,
    /// Which job result an announcement's sentence belongs to: every sentence of one
    /// [`VoiceController::speak_result`] carries the same number, so a Stop during one of them
    /// ends the rest of that result and no other ([`Unheard::reclaim`]). `0` for an answer.
    result: u64,
}

/// A fresh number for [`SpeakMsg::result`]; never `0`.
fn next_result() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// **THE ONLY PLACE A SENTENCE ACTUALLY GOES TO THE SPEAKER THREAD**, for the controller and the
/// supervisor alike. The increment of [`Shared::pending_speech`] happens here, at the send, for
/// the reason [`VoiceController::send_speak`] gives; a failed send takes it back. An
/// announcement is entered in [`Shared::unheard`] here, under a fresh id.
fn dispatch(shared: &Shared, tx: &Sender<SpeakMsg>, mut msg: SpeakMsg) {
    if msg.notice.is_some() {
        if let Ok(mut u) = shared.unheard.lock() {
            msg.notice = Some(u.sent(msg.clone()));
        }
    } else {
        shared.pending_answer.fetch_add(1, Ordering::Relaxed);
    }
    shared.pending_speech.fetch_add(1, Ordering::Relaxed);
    let notice = msg.notice;
    let answer = notice.is_none();
    if tx.send(msg).is_err() {
        let _undo = shared.pending_speech.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
            Some(n.saturating_sub(1))
        });
        if answer {
            let _undo = shared.pending_answer.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                Some(n.saturating_sub(1))
            });
        }
        if let (Some(id), Ok(mut u)) = (notice, shared.unheard.lock()) {
            u.forget(id);
        }
    }
}

/// **What the speaker path needs of the device queue**: [`Playout`] in the app, a counter in the
/// tests, so the path from the gate to the device runs in a test without a sound card.
trait SpeechQueue {
    /// Queue samples; returns where the last of them sits in [`SpeechQueue::played`]'s count.
    fn queue(&self, mono: &[f32]) -> u64;
    /// Drop everything not yet played; returns how many samples were dropped.
    fn stop_now(&self) -> usize;
    /// Samples handed to the device so far.
    fn played(&self) -> u64;
}

impl SpeechQueue for Playout {
    fn queue(&self, mono: &[f32]) -> u64 {
        Playout::queue(self, mono)
    }
    fn stop_now(&self) -> usize {
        Playout::stop_now(self)
    }
    fn played(&self) -> u64 {
        self.played_samples()
    }
}

/// [`SpeechSynth::synthesize_stream`] for one sentence, its target rate and scratch bound: the
/// text, and the block sink that returns false to cancel.
type Synthesize<'a> = &'a mut dyn FnMut(&str, &mut dyn FnMut(&[f32]) -> bool) -> Result<(), TtsError>;

/// **One sentence through the speaker thread**: synthesize it and queue it at the device, block
/// by block. Each block waits while he has the floor, and a sentence whose generation is over
/// (a barge-in, a Stop, his words starting a new turn) is dropped, before synthesis or at its
/// next block. An announcement's blocks are reported to [`Shared::unheard`], so the interruption
/// that cut it can tell whether he heard it and hand it back if not ([`reclaim_unheard`]).
fn speak_one(shared: &Arc<Shared>, msg: &SpeakMsg, out: &dyn SpeechQueue, synthesize: Synthesize) -> Result<(), TtsError> {
    // This sentence is no longer "owed" from the moment this body ends, whichever way it ends.
    // See `PendingSpeech`.
    let _pending = PendingSpeech { shared: shared.clone(), answer: msg.notice.is_none() };
    if let Ok(mut u) = shared.unheard.lock() {
        u.heard(out.played());
    }
    // Dropped before we start: a barge-in already invalidated this sentence. An announcement
    // among them was handed back to the gate by that barge-in.
    if msg.generation != shared.generation.load(Ordering::Relaxed) {
        return Ok(());
    }
    let mut held = false;
    let mut cut = false;
    let result = synthesize(&msg.text, &mut |samples| {
        // Every PCM block must still belong to this answer and wait for his floor.
        while !may_sound(shared.his_floor.load(Ordering::Relaxed), shared.his_onset.load(Ordering::Relaxed))
            && msg.generation == shared.generation.load(Ordering::Relaxed)
            && shared.running.load(Ordering::Relaxed)
        {
            if !held {
                held = true;
                eprintln!("[richos-voice] {} answer HELD after synthesis — he started talking while it was being prepared", wall_clock_utc());
            }
            std::thread::sleep(FLOOR_POLL);
        }
        if !shared.running.load(Ordering::Relaxed) || msg.generation != shared.generation.load(Ordering::Relaxed) {
            cut = true;
            return false;
        }
        shared.answer_sounded.store(true, Ordering::Relaxed);
        let end = out.queue(samples);
        if let (Some(id), Ok(mut u)) = (msg.notice, shared.unheard.lock()) {
            u.queued(id, end.saturating_sub(samples.len() as u64), end);
        }
        true
    });
    if let (Some(id), Ok(mut u)) = (msg.notice, shared.unheard.lock()) {
        match &result {
            Ok(()) if !cut => u.complete(id),
            // Cut: the interruption that cut it decides (`reclaim_unheard`).
            Ok(()) => {}
            // He is told synthesis failed; saying it again would fail again.
            Err(_) => u.forget(id),
        }
    }
    result
}

/// **THE DECREMENT OF [`Shared::pending_speech`], MADE STRUCTURAL.**
///
/// A counter that says "Rich still owes the speakers a sentence" is only safe if it can never
/// be left high: a stuck non-zero value would hold `speaking` true forever, and a microphone
/// that is permanently tainted is a microphone that has stopped working. The speaker loop has
/// four exits from one iteration — the stale-generation skip before synthesis, the stale-
/// generation skip after it, a synthesis error, and the success path — plus unwind. A `Drop`
/// guard covers all five by construction, which an `else` branch does not.
/// `answer`: the sentence is part of an answer, so [`Shared::pending_answer`] counted it too.
struct PendingSpeech {
    shared: Arc<Shared>,
    answer: bool,
}

impl Drop for PendingSpeech {
    fn drop(&mut self) {
        // Saturating, so a decrement that somehow outnumbers its increment reads zero rather
        // than `usize::MAX` — the failure mode here must be "Rich is heard", never "the CEO
        // is never heard again".
        let _ = self.shared.pending_speech.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
            Some(n.saturating_sub(1))
        });
        if self.answer {
            let _undo = self.shared.pending_answer.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                Some(n.saturating_sub(1))
            });
        }
    }
}

struct Shared {
    /// Live input level 0..1, f32 bits. Written per frame by the audio thread.
    level: AtomicU32,
    /// **TRUE FOR THE WHOLE OF RICH'S ANSWER**, not merely while the playout queue is
    /// non-empty — read by the audio thread to arm barge-in and to taint what it records.
    ///
    /// It used to be exactly `playout.is_playing()`, and that left two holes an utterance
    /// could be born in, both of them inside an answer the CEO can hear:
    ///
    /// 1. **The gaps between spoken sentences.** Sentences are synthesized one at a time on
    ///    the speaker thread (`MacSay` spawns `say` per sentence). Whenever synthesis of
    ///    sentence N+1 takes longer than the playback of sentence N, the queue empties and
    ///    this flag went false mid-answer. Ray's candidate-.5 walk on 2026-09-17 answered
    ///    *"Please count slowly out loud from 1 to 20"*, whose deltas chunk into roughly
    ///    twenty one-word sentences and therefore twenty separate `say` spawns.
    /// 2. **The device tail.** See [`crate::playout::Playout::audible_tail_secs`].
    ///
    /// So it is now `queued || synthesis still owed || within the measured tail of either`.
    /// [`Shared::pending_speech`] is the "still owed" term.
    speaking: AtomicBool,
    /// Sentences handed to the speaker thread that have NOT yet reached the playout queue —
    /// synthesis in flight. Non-zero means Rich's answer is not over however empty the queue
    /// is, which is the fact that closes hole 1 above. Incremented at the send, decremented
    /// by a guard that runs on every exit from the speaker loop's body including unwind.
    pending_speech: AtomicUsize,
    /// The part of [`Shared::pending_speech`] that is an ANSWER's: announcements not counted. What
    /// his new words would leave unsaid ([`Floor::his_new_turn`]).
    pending_answer: AtomicUsize,
    /// Announcements sent to the speaker and not yet heard to their last sample. Every
    /// interruption that cuts the device queue hands them back to the gate ([`reclaim_unheard`]).
    unheard: Mutex<Unheard<SpeakMsg>>,
    /// Bumped on every barge-in/stop. Synthesis for an older generation is discarded.
    generation: AtomicU64,
    /// The open stream is delivering nothing (see `noaudio.rs`). Owned by the supervisor,
    /// readable by the shell/tests without touching the audio thread.
    no_audio: AtomicBool,
    /// **The CEO has the floor** ([`crate::turn::TurnAssembler::has_floor`]), published by the
    /// supervisor the moment it changes. [`VoiceController::send_speak`] reads it to hold Rich's
    /// sentences instead of saying them over him.
    his_floor: AtomicBool,
    /// A sample of the current answer has reached the device. Set by the speaker thread at the
    /// queue, cleared by the supervisor when the answer's window closes. Until it is set nothing of
    /// Rich is in the room, so [`crate::turn::rich_audible`] is false however much is owed.
    answer_sounded: AtomicBool,
    /// His next word is building toward its onset, or his recording is open and the supervisor
    /// may not have published the floor yet (`CaptureBrain::his_onset_building`). Written per
    /// frame by the capture callback; the speaker waits on it as on the floor.
    his_onset: AtomicBool,
    /// A reply starting now would be heard (`CaptureBrain::takes_a_reply`). Written per frame by
    /// the capture callback; the half-duplex line waits on it.
    takes_a_reply: AtomicBool,
    /// The half-duplex line has just asked him to say it again: the capture callback calls
    /// `CaptureBrain::invite_reply` on its next frame and clears this.
    reply_invited: AtomicBool,
    /// Voice is going off: the capture callback closes the open recording on its next frame.
    finish_capture: AtomicBool,
    /// The capture callback has closed it and sent what became of it.
    capture_finished: AtomicBool,
    /// The supervisor has drained everything the capture callback sent before it finished, so
    /// his last utterance is with the recognizer and counted in the turn.
    drained_after_finish: AtomicBool,
    running: AtomicBool,
}

impl Shared {
    fn set_level(&self, v: f32) {
        self.level.store(v.to_bits(), Ordering::Relaxed);
    }
    fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }
}

/// **THE CANCELLER'S LIVE STATE, PUBLISHED LOSSLESSLY.**
///
/// This replaces a single `AtomicU32` that packed confidence into bit 0 and whole-dB ERLE into
/// bits 1.., via `(erle_db.max(0.0) as u32) << 1`. That packing destroyed the two facts an
/// operator needs most:
///
/// - **it clamped negatives to zero.** Measured on this Mac on 2026-09-17 the live figure is
///   **−0.1 dB** — the canceller removing marginally less than nothing — and the log printed
///   `erle=0 dB`. "Zero" reads as "not started yet"; the truth is "running and achieving
///   nothing on this path", which is a different problem with a different answer.
/// - **it truncated toward zero**, so anything under 1.0 dB also printed `0`.
///
/// Ray's candidate-.4 walk read `erle=0 dB` off the running app and took it as evidence the
/// canceller was not learning at all. It was learning; it was learning a path whose coherence
/// caps any linear canceller at 4.3 dB (`docs/verification/2026-09-17-aec-erle-on-the-ceo-rig.md`).
/// A number that cannot be negative cannot report that, so it is stored in signed millidecibels.
///
/// One relaxed store per audio frame, no locks, readable from anywhere.
#[derive(Debug, Default)]
pub struct AecShared {
    /// The canceller has measured its residual low enough, for long enough, to be trusted.
    confident: AtomicBool,
    /// Echo Return Loss Enhancement in **millidecibels, signed**. Negative is a real reading.
    erle_mdb: AtomicI32,
    /// The tracked typical residual while Rich is audible, in **millidecibels full scale**.
    /// This is the number [`crate::aec::CONFIDENT_LEAK_RMS`] is compared against, so publishing
    /// it is what makes "how far short is it" answerable from a log line instead of a guess.
    leak_mdbfs: AtomicI32,
    /// **IS `erle_mdb` A MEASUREMENT?** Published beside the number for the reason the number
    /// is published signed: a value whose "no reading yet" state is indistinguishable from a
    /// real reading is not a diagnostic. See [`crate::aec::AecMetrics::erle_measured`].
    erle_is_measured: AtomicBool,
    /// **IS `leak_mdbfs` A MEASUREMENT?** Same rule, and here the sentinel is the worse of the
    /// two: an unseeded residual tracker publishes `0.0 dBFS`, which is full scale.
    /// See [`crate::aec::AecMetrics::leak_measured`].
    leak_is_measured: AtomicBool,
}

impl AecShared {
    fn store(
        &self,
        confident: bool,
        erle_db: f32,
        erle_measured: bool,
        leak_rms: f32,
        leak_measured: bool,
    ) {
        self.confident.store(confident, Ordering::Relaxed);
        self.erle_mdb.store(millis(erle_db), Ordering::Relaxed);
        self.leak_mdbfs.store(millis(dbfs(leak_rms)), Ordering::Relaxed);
        self.erle_is_measured.store(erle_measured, Ordering::Relaxed);
        self.leak_is_measured.store(leak_measured, Ordering::Relaxed);
    }
    fn confident(&self) -> bool {
        self.confident.load(Ordering::Relaxed)
    }
    /// The live ERLE, or `None` when nothing has been measured yet.
    ///
    /// **`Option`, not a float with a sentinel, and that is the whole of defect 3.** Ray's
    /// candidate-.6 walk read `erle=0.0 dB, residual 0.0 dBFS` off four discards
    /// (`docs/verification/2026-09-17-nightly-1.2.0-20260917.6-onscreen-audit.md` defect 3) and
    /// could not tell a canceller achieving nothing from a canceller that had measured nothing.
    /// Both are real states with different answers. The type now forces the caller to say which.
    fn erle_db(&self) -> Option<f32> {
        self.erle_is_measured
            .load(Ordering::Relaxed)
            .then(|| self.erle_mdb.load(Ordering::Relaxed) as f32 / 1000.0)
    }
    /// The tracked residual in dBFS, or `None` when the tracker has never been seeded.
    fn leak_dbfs(&self) -> Option<f32> {
        self.leak_is_measured
            .load(Ordering::Relaxed)
            .then(|| self.leak_mdbfs.load(Ordering::Relaxed) as f32 / 1000.0)
    }
}

/// A measured decibel figure, or the words `not measured` — the one formatting rule every
/// diagnostic in this file obeys.
///
/// It takes the unit as a parameter rather than baking one in, because the two numbers this
/// serves are in different units (dB of enhancement, dBFS of level) and a helper that guesses
/// would be the next place they drift.
fn measured_or_not(v: Option<f32>, unit: &str) -> String {
    match v {
        Some(x) => format!("{x:.1} {unit}"),
        None => "not measured".to_string(),
    }
}

/// dB to signed millidecibels, saturating rather than wrapping. A non-finite reading stores 0
/// — the one value that cannot be mistaken for a measurement of anything.
fn millis(db: f32) -> i32 {
    if !db.is_finite() {
        return 0;
    }
    (db * 1000.0).clamp(i32::MIN as f32, i32::MAX as f32) as i32
}

/// RMS to dBFS, with the same floor `aec.rs` and the examples use.
fn dbfs(rms: f32) -> f32 {
    20.0 * rms.max(1e-12).log10()
}

/// Voice mode, running. Dropping it closes the microphone and silences Rich.
pub struct VoiceController {
    shared: Arc<Shared>,
    machine: Arc<Mutex<VoiceStateMachine>>,
    chunker: Mutex<SentenceChunker>,
    /// Rich's sentences, held while the CEO has the floor, and the mute that follows a superseded
    /// or interrupted answer (`turn.rs`). Shared with the supervisor, which knows the floor.
    gate: Arc<Mutex<AnswerGate<SpeakMsg>>>,
    /// `rich://turn-started` events so far; the supervisor compares it with the count when a turn
    /// was handed over to tell whether that turn is the one running.
    turns_started: Arc<AtomicU64>,
    /// How the shell stops the running turn when his continuation withdraws it
    /// ([`VoiceController::on_withdraw`]).
    withdraw: WithdrawHook,
    /// `Option` so `Drop` can close the channel BEFORE joining the speaker thread —
    /// joining first would deadlock on a `recv()` that can never fail.
    speak_tx: Option<Sender<SpeakMsg>>,
    playout: Arc<Playout>,
    force_barge: Arc<AtomicBool>,
    /// The canceller's live state — confidence, signed ERLE, tracked leak floor. Written once
    /// per audio frame by the capture thread. See [`AecShared`] for why it is not one packed
    /// word any more.
    aec_state: Arc<AecShared>,
    diagnostics: Diagnostics,
    /// `Option` so `Drop` can close the microphone BEFORE joining the worker threads: the
    /// joins can wait on a recognition or an AI turn, and the CEO's Off must not.
    capture: Option<Capture>,
    threads: Vec<std::thread::JoinHandle<()>>,
}

/// Consecutive utterances that come back as whisper noise before the CEO is told.
///
/// **THE FLOOR THIS PUTS UNDER THE NOTICE, derived rather than felt.** An utterance only
/// reaches whisper at all after [`crate::endpoint::MIN_SPEECH_FRAMES`] of speech and
/// [`crate::endpoint::SILENCE_HANGOVER_FRAMES`] of silence to close it:
///
/// ```text
///   minimum speech   19 x 256 / 16000 = 0.304 s
///   silence hangover 50 x 256 / 16000 = 0.800 s
///   one utterance                    >= 1.104 s
///   three in a row                   >= 3.312 s   (+ whisper, measured 0.47-0.74 s each)
/// ```
///
/// So this cannot fire on a cough, a chair or a single stray word, and it CAN fire well
/// inside the 25+ seconds of silent "listening…" measured on published v1.0.0 on
/// 2026-09-04 — roughly seven times over. Three is the smallest number with both
/// properties; `endpoint.rs`'s own test asserts the two frame counts it rests on.
pub const SILENT_DISCARD_RUN: u32 = 3;

/// **WHAT VOICE MODE REPORTS THAT IS NOT AN ERROR.**
///
/// `SttError`, `CaptureError` and `PlayoutError` all carry a `ceo_message`, and every one of
/// them is a thing that FAILED. This is the other kind: nothing failed, the pipeline is
/// working exactly as specified, and the CEO still needs to be told something — which is
/// precisely the condition that shipped silent.
///
/// **The method is called `ceo_message` for a second reason, and it is not decoration.**
/// `app/ui/tests/lib/state-strings.js` scrapes the product's CEO-facing sentences out of
/// source, and under `app/crates` the only shape it can see is a literal inside a function
/// with that name (`state-strings.js:469`). A sentence the state registry cannot see is a
/// sentence nobody has said whether the CEO can act on — the exact defect that left
/// `LORO_DESK_ABSENT_MESSAGE` invisible and two suites red for a day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceNotice {
    /// Three utterances in a row came back as whisper's documented silence noise. The
    /// microphone is open, the level meter is moving and `noaudio.rs` is satisfied that
    /// signal is arriving, so nothing else in this pipeline has anything to report.
    ///
    /// **Its meaning NARROWED on 2026-09-05 and is better for it.** Since
    /// [`crate::voiced::VoiceEvidence`] now refuses voiceless audio before whisper is ever
    /// called, an utterance that reaches this notice has already been measured to contain a
    /// human voice. So it no longer means "the room is noisy"; it means the CEO really did
    /// speak and the recognizer still got no words out of it.
    SoundButNoWords,
    /// **The recording carried no voice, so nothing was sent.** Raised by the audio-grounded
    /// gate in [`crate::voiced`], before the recognizer runs — see [`RecognizerDesk`].
    ///
    /// This is the refusal that the CEO has to be told about, because a silent drop is its
    /// own kind of lying: he tapped the talk button, something happened, and if the app says
    /// nothing he cannot tell "it ignored me" from "it is still listening".
    ///
    /// **It is latched per RUN of refusals, and that is a judgment worth stating.** An open
    /// mic in a quiet room can produce one voiceless utterance every ~1.104 s, and a line
    /// per refusal would be a drip that trains him to ignore the one that matters. So the
    /// first refusal says it, consecutive refusals are stderr only, and the latch clears the
    /// moment an utterance is admitted — because that proves the input recovered.
    HeardNoVoice,
    /// **RICH WAS CUT OFF MID-ANSWER AND STOPPED, and until 2026-09-05 nothing said so.**
    ///
    /// `open-items.md` row 3.30: *"A voice turn cut mid-sentence needs to say so audibly,
    /// not fail silent. The CEO is speaking to a system that has stopped listening and does
    /// not know it."*
    ///
    /// **The failure is precise, and it is worse than silence.** On `rich://turn-error` the
    /// shell called `voice_speak_end`, which FLUSHES the chunker's tail — so a turn that
    /// died mid-sentence spoke its half-sentence aloud, trailed off, and then said nothing
    /// at all. The CEO hears Rich stop in the middle of a word and has no way to tell that
    /// from thinking.
    ///
    /// **It is the only notice in this enum that is SPOKEN as well as shown**, and that is
    /// the whole requirement rather than a flourish: in voice mode his eyes are not on the
    /// panel. `SoundButNoWords` and `HeardNoVoice` both describe the microphone, which he
    /// discovers by the app not answering; this one describes the ANSWER, which he is in
    /// the middle of listening to.
    ///
    /// **It names no control and offers no retry**, for the reason the other two give: the
    /// affordance for asking again is the open microphone, there is no button to point at,
    /// and an imperative with no control is a request wearing a status's clothes.
    ReplyCutOff,
    /// **A VOICE WAS MEASURED, WHISPER RAN, AND THE WORDS IT RETURNED WERE NOT WORDS** —
    /// whisper's documented non-speech noise (`(clears throat)`, `[BLANK_AUDIO]`), refused by
    /// [`stt::is_meaningful`]. Dropping it is right. Saying nothing about it was not.
    ///
    /// **The defect this closes is an ASYMMETRY, not a missing feature** (audit-3 §4 #4,
    /// frame `a3-15`). Two refusal paths sit side by side in [`RecognizerDesk::handle`] and
    /// they treated the CEO completely differently:
    ///
    /// | path | when | what he was told |
    /// |---|---|---|
    /// | [`crate::voiced`] gate, pre-whisper | the audio carried no voice | [`VoiceNotice::HeardNoVoice`], on the FIRST refusal |
    /// | [`stt::is_meaningful`], post-whisper | a voice, but no words | **nothing** until the THIRD in a row |
    ///
    /// So the case where he definitely DID speak — a voice was measured, which is a stronger
    /// signal than the pre-whisper path ever has — was the quieter of the two. A 2.3 s
    /// utterance came back `(clears throat)` and the window did not change at all: he could
    /// not tell "not heard" from "heard and discarded" from "broken".
    ///
    /// **It is latched per RUN, exactly like [`VoiceNotice::HeardNoVoice`]**, and for that
    /// notice's stated reason: an open mic can produce these back to back, and a line per
    /// discard is a drip that trains him to ignore the one that matters. First discard says
    /// it; the rest are stderr only; the latch clears the moment an utterance is admitted,
    /// because that proves the input recovered. At [`SILENT_DISCARD_RUN`] the stronger
    /// [`VoiceNotice::SoundButNoWords`] takes over and this one stands aside, so a run of
    /// discards produces two sentences in total and never two at once.
    ///
    /// **It goes down `rich://voice-notice`, not `rich://voice-error`.** Nothing failed and
    /// voice did not stop, which is the split `event.rs` draws. It also means the line is not
    /// suppressed when the voice panel is closed — `main.js`'s notice listener is the only one
    /// of the four that does not bail on `!voiceMode`.
    DidNotCatchThat,
    /// **RICH COULD NOT LISTEN WHILE HE WAS SPEAKING, AND WHATEVER THE MICROPHONE PICKED UP
    /// THEN WAS THROWN AWAY** — audit-3 §4 #1, corrected after candidate .4's walk proved the
    /// first wording false on the CEO's own rig.
    ///
    /// ## What it replaced, and why the replacement is not a softening
    ///
    /// This variant used to be `TalkedOverRich` and it said *"You started talking while I was
    /// still speaking … so that didn't reach me and I haven't sent anything."* Ray walked it on
    /// the CEO's screen on 2026-09-17 and it fired while he was provably silent
    /// (`docs/verification/2026-09-17-nightly-1.2.0-20260917.4-onscreen-audit.md` §3): at output
    /// volume 85 a spoken turn produced the notice with nobody talking, and the identical turn
    /// with the output volume set to 0 produced no discard and no notice at all. The single
    /// variable was whether Rich's own voice was audible in the room.
    ///
    /// **The code says the same thing, and it says it structurally.** `push_residual` sets
    /// `self.tainted = speaking && !self.barged && !confident`, so a tainted discard requires
    /// `confident == false` — the canceller declining to vouch for its own residual. When it IS
    /// confident the utterance is ADMITTED rather than discarded and there is nothing to
    /// announce. **So the old sentence was emitted only, and exactly, on the path where the app
    /// cannot know whose voice it heard.** There was no reachable path on which it was known to
    /// be true. That is why the accusation is deleted rather than gated behind a condition.
    ///
    /// ## It is not a transient state on the CEO's hardware — measured
    ///
    /// Measured on this Mac on 2026-09-17 — five live runs of `examples/aec_live`, two of
    /// `examples/aec_probe`, and one recorded pair replayed offline, all pasted in
    /// `docs/verification/2026-09-17-aec-erle-on-the-ceo-rig.md`. Mac mini Speakers out, Elgato
    /// Wave:3 in, at the CEO's own volume setting:
    ///
    /// ```text
    ///   steady-state ERLE, five live runs      -0.5 .. +0.7 dB   (nothing removed)
    ///   ERLE over the recorded sentence         6.3 dB
    ///   microphone while Rich is audible      -42 .. -46 dBFS
    ///   aec::CONFIDENT_LEAK_RMS               -52.04 dBFS
    ///   far-active blocks under that threshold  77.6 %, longest run 1.376 s of the 2.000 s
    ///   coherence ceiling for ANY linear filter single digits (4.3 dB full band on a click
    ///                                           train, 7.6 dB over 300-3400 Hz)
    /// ```
    ///
    /// The canceller is marginal there by measurement, not by opinion, and it does not reach
    /// confidence. So the taint rule stays in force and every spoken answer through the speakers
    /// produces one of these discards.
    ///
    /// That is what makes the wording load-bearing rather than cosmetic: this line is not a rare
    /// edge case he might see once, it is the line he meets after every spoken answer.
    ///
    /// ## What the sentence is allowed to assert
    ///
    /// Only what this file can prove: Rich was speaking, the canceller could not separate the
    /// two voices, and the audio captured in that window was not used. **Everything about the
    /// CEO is conditional** — `if you said something just then` — because the app genuinely does
    /// not know, and `the_notice_makes_no_unconditional_claim_about_the_ceo` pins that.
    ///
    /// It still states the CONSEQUENCE, for the reason [`VoiceNotice::HeardNoVoice`] does: the
    /// failure this explains is the app sending the TAIL of his sentence as though it were the
    /// whole of it, and "I haven't sent anything" is the fact that makes Rich answering a
    /// fragment make sense. It still ends with a STATUS and not an instruction — "wait until I
    /// finish" would be an instruction to use the product more carefully to work around a
    /// limitation, which is not a thing to ask of him.
    ///
    /// **Latched once per voice session**, not per run — see [`RefusalNotices`]. What it
    /// describes is a standing property of the room and the hardware, not an event.
    ///
    /// **And since 2026-09-17 it spends the SAME budget as the three recognizer lines**, so
    /// it cannot be joined on screen by a second and third apology for the same silence.
    /// It is the strongest sentence in that family and therefore the one that stands.
    ///
    /// ## It KEEPS this trigger after the half-duplex window was widened — decided, with the number
    ///
    /// On 2026-09-17 the window stopped being the playout queue's depth and became the whole of
    /// Rich's answer, gaps and device tail included (`AudibleWindow`), and taint stopped being
    /// decided once at `Started`. That makes this line fire on strictly more discards, so the
    /// question was asked directly: is there anything left for it to say that is TRUE, or is
    /// every discard on this path now indistinguishable from echo, leaving the sentence saying
    /// nothing?
    ///
    /// **It is still true on every firing, and there is no honest narrower trigger.** Two
    /// different things produce a tainted discard here — Rich's own echo, and the CEO genuinely
    /// talking over Rich without meeting the 5.008 s debounce — and the line already refuses to
    /// choose between them: every clause about him is conditional. That is not a hedge, it is
    /// the measurement.
    ///
    /// The candidate narrower trigger was "fire only on a voiced residual standing above the
    /// echo the canceller expects". It does not survive contact with the numbers:
    ///
    /// - **Voicing cannot separate them.** Rich's echo IS voiced speech, so `voiced.rs`'s pitch
    ///   and harmonicity evidence answers yes to both. Only LEVEL is left.
    /// - **Level cannot either, on this path.** Measured over the CEO's own recording with no
    ///   near-end talker present, so every decibel of it is Rich
    ///   (`tests/self_voice_replay.rs::separating_the_ceo_from_richs_echo_by_level_needs_a_margin_this_large`,
    ///   447 far-active blocks): median residual **-48.6 dBFS**, p90 **-40.2**, p99 **-36.2**,
    ///   peak **-34.3** — a spread of **14.3 dB** above the median. A level test would have to
    ///   sit at -34.3 dBFS to stop firing on Rich alone, and the VAD's own absolute speech
    ///   floor is 0.005 RMS = **-46.02 dBFS** (`vad.rs:86-91`). So the discriminator would sit
    ///   **11.7 dB above the level at which the app is willing to call something speech at
    ///   all**: it would stay silent for a CEO speaking normally and announce itself only when
    ///   he raised his voice, which is the wrong way round.
    ///
    /// So the sentence stays, unchanged, latched once per session. It is what the app knows.
    CouldNotListenWhileSpeaking,
}

impl VoiceNotice {
    /// The CEO-facing line. No path, no device name, no decibel figure — the operator's
    /// `eprintln` beside the discard carries the transcript that was thrown away, and this
    /// carries the sentence.
    ///
    /// It states what is true, names the thing that is usually wrong, and INVENTS NO
    /// CONTROL: the ◉ that ends voice is already on screen with its own footnote two lines
    /// below this notice, so a sentence pointing at it would be a request wearing a status's
    /// clothes. It does not offer to switch voice off either — doing that to the CEO
    /// mid-sentence is not a decision this file gets to make.
    pub fn ceo_message(&self) -> &'static str {
        match self {
            VoiceNotice::SoundButNoWords => {
                "I can hear sound, but I'm not getting words out of it. The microphone may \
                 be picking up the room rather than you. Please say it again closer to the \
                 microphone. I'm still listening."
            }
            // STATES THE CONSEQUENCE, not just the observation. "I didn't catch that"
            // leaves open whether something was sent anyway, and the whole defect being
            // fixed here is the app sending a sentence he never said — so the line has to
            // close that question, in his own words, before anything else.
            //
            // THEN IT SAYS WHAT TO DO, since 2026-10-21. A line that tells him his words may
            // not have gone anywhere and stops there is a dead end (the failure class in
            // `docs/development/failure-classes/lost-input-without-a-next-step.md`, guarded
            // by `affordances.js`). The step is conditional, because a cough reaches this
            // path as well as a word, and the open microphone is its control.
            VoiceNotice::HeardNoVoice => {
                "That didn't come through as speech, so I haven't sent anything. If you said \
                 something, please say it again. I'm still listening."
            }
            // IT SAYS THE ANSWER STOPPED, not that "something went wrong". The CEO is
            // listening, not reading, and the fact he needs first is that what he heard is
            // all there is — otherwise he waits for a sentence that is never coming.
            //
            // IT ENDS WITH A STATUS, like the two above: the microphone is still open, and
            // saying so is what tells him he can simply ask again. The REASON is a separate
            // sentence supplied by the caller, because voice does not know a 529 from a
            // broken pipe and inventing a cause here would be worse than naming none.
            VoiceNotice::ReplyCutOff => {
                "I was cut off partway through that answer, so what you heard is all I got \
                 out. I'm still listening."
            }
            // OPENS WITH THE AUDIT'S OWN WORDS, and they are the right words: "I didn't catch
            // that" is what a person says when they heard you and the words did not land. It
            // is the honest description of this path — a voice WAS measured, so "I heard
            // nothing" would be false, and naming whisper's `(clears throat)` would put an
            // implementation detail in his ear.
            //
            // IT STATES THE CONSEQUENCE SECOND, for `HeardNoVoice`'s reason: the question a
            // silent drop leaves open is whether something was sent anyway, and a fragment
            // sent as if it were a whole sentence is the failure mode this crate has already
            // shipped once. So the line closes that question before anything else.
            //
            // THEN IT SAYS WHAT TO DO, for `HeardNoVoice`'s reason, and here unconditionally:
            // a voice WAS measured, so he did say something and it is lost unless he says it
            // again. Same last three words as `HeardNoVoice`, deliberately: the two are the
            // same event to him, and only the cause differs.
            VoiceNotice::DidNotCatchThat => {
                "I didn't catch that, so I haven't sent anything. Please say it again. I'm \
                 still listening."
            }
            // EVERY CLAUSE ABOUT THE CEO IS CONDITIONAL, and that is the whole correction.
            // The sentence this replaced opened "You started talking while I was still
            // speaking", which is an assertion about something HE did, made on the one code
            // path where the canceller has explicitly declined to vouch for its residual.
            // On the CEO's own rig it fired at him while he was silent, after every spoken
            // answer. A status may describe the app with certainty; it may not describe the
            // reader with certainty it does not have.
            //
            // IT DESCRIBES WHAT RICH DID, which IS knowable here: Rich was speaking, the two
            // voices could not be told apart, and the audio from that window was not used.
            // It makes no capability claim — "I can't hear you while I'm speaking" would be
            // false twice, because talking over Rich for long enough DOES cut him off today
            // and a confident canceller admits the utterance outright.
            //
            // IT STATES THE CONSEQUENCE, for the reason `HeardNoVoice` does, and here the
            // reason is sharper than anywhere else in this enum: the failure it explains is
            // the app sending the TAIL of his sentence as though it were the whole of it.
            // He watched Rich answer a fragment. "I haven't sent anything" is the fact that
            // makes that make sense.
            //
            // IT ENDS BY SAYING WHAT TO DO, since 2026-10-21. It used to end with a status
            // ("I'm listening now.") on the argument that the open microphone is the
            // affordance and an imperative with no button is a request. A new user on 1.2.1
            // got this line right after a failed answer and was left with a sentence that
            // explained a loss and gave no next step (the failure class in
            // `docs/development/failure-classes/lost-input-without-a-next-step.md`). The step
            // is real and has a control: the microphone is open, so whatever he said is
            // recovered by saying it again now. It is still CONDITIONAL ("If you did"),
            // because on this path the app cannot know whether he spoke, and it is still not
            // "wait until I finish", which would ask him to work around a limitation.
            VoiceNotice::CouldNotListenWhileSpeaking => {
                "While I was speaking, I couldn't tell your voice from my own, so if you \
                 said something just then, it didn't reach me and I haven't sent anything. \
                 If you did, please say it again now. I'm listening."
            }
        }
    }
}

/// **WHAT VOICE MODE DOES WHEN THE ANSWER IT IS SPEAKING DIES** — row 3.30's first answer.
///
/// A struct, not three lines inside [`VoiceController::turn_cut_off`], for exactly the
/// reason [`RecognizerDesk`] is one: the thing that has to be provable here is not *what*
/// is said but *where it goes*. The defect is that the notice reaches the panel and not the
/// speaker, and a test that only inspected a `VoiceEvent` would pass over it forever. So
/// `handle` takes the SPEAK sink as a parameter and `controller::tests` asserts the sentence
/// arrived in both.
///
/// ## THE ORDER IS THE DESIGN, AND IT IS NOT INTERCHANGEABLE
///
/// 1. **Drop the dangling fragment.** The chunker holds whatever arrived after the last
///    sentence boundary — half a sentence that will never be completed, because the lease
///    that was writing it is gone. Until 2026-09-05 the shell called `voice_speak_end` here,
///    which FLUSHES that fragment: Rich spoke half a sentence aloud, trailed off, and said
///    nothing else. That is worse than silence, because trailing off is what a person does
///    while thinking.
/// 2. **Then say the notice**, appended AFTER whatever is already queued rather than
///    replacing it. The sentences Rich already completed are real answer and the CEO is
///    entitled to hear them finish; the generation counter is deliberately NOT bumped and
///    playout is deliberately NOT stopped. Silencing them in order to announce a cut-off
///    would destroy the thing being announced.
/// 3. **Then the reason, if the caller has one.** Voice cannot tell a `529` from a broken
///    pipe and does not try: `richos-core`'s `upstream.rs` classifies, and whatever sentence
///    it authored is spoken after this one. A `None` reason costs nothing — the notice
///    already carries the fact that matters.
///
/// It is latched per turn by the caller holding it, so a turn that produces two terminal
/// events cannot say this twice.
pub struct CutOffDesk;

impl CutOffDesk {
    /// Announce one cut-off. `speak` queues a sentence for the speaker thread; `observer`
    /// gets the same words for the panel.
    ///
    /// **Both, always, and neither is optional.** The panel alone is the defect this exists
    /// to end — in voice mode his eyes are not on it. The speaker alone would leave nothing
    /// on screen for him to read back afterwards, and the screen is where the REASON is
    /// legible (a request id is unspeakable).
    pub fn handle<S>(reason: Option<&str>, observer: &dyn VoiceObserver, mut speak: S)
    where
        S: FnMut(&str),
    {
        let notice = VoiceNotice::ReplyCutOff.ceo_message();
        speak(notice);
        // The reason is SPOKEN too, after the notice, when there is one. It is one sentence
        // of plain English authored by `upstream.rs` for exactly this purpose; the operator
        // detail (request ids, HTTP statuses) never travels on this path, because a spoken
        // `req_011Cegb417YK6i1BEVDFmzU1` is noise with a syllable count.
        if let Some(r) = reason.map(str::trim).filter(|r| !r.is_empty()) {
            speak(r);
        }
        // The panel gets the whole statement in one line, because a screen reads at once.
        let shown = match reason.map(str::trim).filter(|r| !r.is_empty()) {
            Some(r) => format!("{notice} {r}"),
            None => notice.to_string(),
        };
        observer.on_voice_event(&VoiceEvent::Error { message: shown, at: now_millis() });
    }
}

/// **THE ONE PLACE A RECOGNIZED UTTERANCE CAN BECOME A TURN — and the order it happens in.**
///
/// The recognizer thread used to be a `while let` with the whole decision inlined, and the
/// decision was: run whisper, then ask [`stt::is_meaningful`] whether the TEXT looked like
/// something. That is a text heuristic over a ten-phrase list, and on 2026-09-04 it let
/// *"1, 2, 3, testing."* through from a silent room on the CEO's own Mac, into his own
/// ledger, under his own name (`ray-opus-a2`, published v1.0.1). No list of phrases can fix
/// that, because the sentence is indistinguishable AS TEXT from one a person would say.
///
/// So the decision is now made on the AUDIO, and it is made FIRST:
///
/// 1. [`VoiceEvidence::measure`] asks whether the recording carried a voice.
/// 2. If it did not, this returns. **Whisper is never called**, so no transcript exists to
///    be submitted, mis-filtered or logged. The failure direction is silence by construction
///    rather than by care.
/// 3. Only then does the recognizer run, and only then can `submit` be reached.
///
/// It is a struct rather than a closure so the ORDER is something a test can hold: `handle`
/// takes the transcriber as a parameter, and `controller::tests` passes one that PANICS if
/// it is ever called. "Refused audio never reaches whisper" is therefore proven, not
/// asserted — the same reason [`CaptureBrain`] was split out of the audio callback.
pub struct RecognizerDesk {
    /// Consecutive transcripts rejected by [`stt::is_meaningful`] — see [`SILENT_DISCARD_RUN`].
    /// A COUNTER, not a latch: it stays here because it describes this desk's run of work.
    discards: u32,
    /// **The three latches that used to live here are now ONE, and it is SHARED with
    /// [`supervise`].** That is the whole of defect 1: they were three independent "at most
    /// once" flags in two threads, and a person saw three cards. See [`RefusalNotices`].
    ///
    /// `Arc<Mutex<_>>` because the two emitters genuinely are on different threads — this desk
    /// runs on the recognizer thread and the half-duplex discard is decided on the supervisor
    /// thread. The lock is taken for the length of one comparison and never across a
    /// `transcribe` call.
    notices: Arc<Mutex<RefusalNotices>>,
    /// The last utterance was cut at the 30.000 s window and carried a voice, so the next one,
    /// its continuation, does not have to prove a voice on its own (review rv-20261013T202747Z,
    /// finding 2).
    cut_voiced: bool,
}

impl Default for RecognizerDesk {
    fn default() -> Self {
        RecognizerDesk::new()
    }
}

impl RecognizerDesk {
    /// A desk with a budget of its own. For tests and for any caller that is the only emitter;
    /// the shipped wiring uses [`Self::sharing`].
    pub fn new() -> RecognizerDesk {
        RecognizerDesk::sharing(Arc::new(Mutex::new(RefusalNotices::new())))
    }

    /// A desk that spends the SAME budget as [`supervise`]. This is the shipped construction.
    pub fn sharing(notices: Arc<Mutex<RefusalNotices>>) -> RecognizerDesk {
        RecognizerDesk { discards: 0, notices, cut_voiced: false }
    }

    /// The budget this desk is spending, so a test can hand it to the other emitter.
    pub fn notices(&self) -> Arc<Mutex<RefusalNotices>> {
        self.notices.clone()
    }

    /// Ask the shared budget for one sentence. A poisoned lock says nothing rather than
    /// panicking in the recognizer thread — losing a status line is recoverable, losing the
    /// thread that turns his speech into turns is not.
    fn claim(&self, pick: impl FnOnce(&mut RefusalNotices) -> Option<VoiceNotice>) -> Option<VoiceNotice> {
        self.notices.lock().ok().and_then(|mut n| pick(&mut n))
    }

    /// Decide one finished utterance.
    ///
    /// `transcribe` is only ever called for audio that has already been measured to carry a
    /// voice. `submit` is only ever called for a transcript that survived both that gate and
    /// the narrow noise-phrase filter. Since 2026-10-13 what it receives is one FRAGMENT of his
    /// turn, which the supervisor's [`crate::turn::TurnAssembler`] joins with the rest before
    /// the turn takes the SAME path typed text takes.
    pub fn handle<T, S>(
        &mut self,
        admitted: &AdmittedUtterance,
        observer: &dyn VoiceObserver,
        transcribe: T,
        submit: S,
    ) where
        T: FnOnce(&[f32]) -> Result<(String, u64), stt::SttError>,
        S: FnOnce(String, bool),
    {
        let utterance = &admitted.utterance;
        let duration_ms = (utterance.duration_secs() * 1000.0) as u64;

        // ---- 1. THE AUDIO, BEFORE ANY TRANSCRIPT EXISTS ---------------------------------
        let evidence = VoiceEvidence::measure(&utterance.samples);
        // **THE REST OF A CUT SENTENCE IS ALREADY PROVEN.** The recorder admits the continuation
        // of an utterance cut at the 30.000 s window with as little as one frame of speech,
        // because it is the end of his sentence; measured alone, a last word 0.032 s past the cut
        // is 2 voiced windows against the 6 the gate needs (review rv-20261013T202747Z, finding 2).
        // The cut fragment it continues carried a voice, so it does too.
        let carried = evidence.carried_speech() || (utterance.continues_cut && self.cut_voiced);
        self.cut_voiced = carried && utterance.reason == crate::endpoint::EndReason::MaxLength;
        if carried && !evidence.carried_speech() {
            eprintln!(
                "[richos-voice] the rest of a cut utterance, admitted on the voice its first part carried: {}",
                evidence.summary()
            );
        }
        if !carried {
            // The operator gets the measurement; the CEO gets the sentence. Neither gets a
            // transcript, because none was produced.
            eprintln!(
                "[richos-voice] refused before recognition — the audio carried no voice: {}",
                evidence.summary()
            );
            if let Some(n) = self.claim(|b| b.heard_no_voice()) {
                observer.on_voice_event(&VoiceEvent::Error {
                    message: n.ceo_message().to_string(),
                    at: now_millis(),
                });
            }
            return;
        }
        // A VOICE WAS MEASURED — and that is deliberately NOT a recovery any more.
        //
        // It used to clear the `HeardNoVoice` latch right here, on the theory that measuring a
        // voice proves the input recovered. It does not prove it: whisper may still return
        // `[BLANK_AUDIO]` two lines below, and when it did, the cleared latch plus
        // `DidNotCatchThat`'s own latch produced TWO cards for ONE noise — audit-5 #8, and
        // cards 2 and 3 of candidate .6's defect 1. Recovery is now the stronger signal only:
        // an utterance ADMITTED AND UNDERSTOOD. See `RefusalNotices::recovered`.

        // ---- 2. ONLY NOW DOES WHISPER EXIST ----------------------------------------------
        match transcribe(&utterance.samples) {
            Ok((text, latency_ms)) => {
                if !stt::is_meaningful(&text) {
                    // THE OPERATOR'S LINE STAYS EXACTLY AS IT WAS. It carries the transcript
                    // that was thrown away, which is the one thing the CEO's sentence must
                    // never carry, and audit-3 read the defect off it.
                    eprintln!("[richos-voice] discarded non-speech transcript: {text:?}");
                    self.discards += 1;
                    if self.discards >= SILENT_DISCARD_RUN {
                        // THE STRONGER LINE TAKES OVER AND THE SHORT ONE STANDS ASIDE — now by
                        // AUTHORITY rather than by two flags set together. `SoundButNoWords`
                        // outranks `DidNotCatchThat`, so it replaces it exactly once, and the
                        // short line cannot come back afterwards because equal-or-lower rank
                        // never replaces what is standing.
                        if let Some(n) = self.claim(|b| b.sound_but_no_words()) {
                            observer.on_voice_event(&VoiceEvent::Error {
                                message: n.ceo_message().to_string(),
                                at: now_millis(),
                            });
                        }
                    } else if let Some(n) = self.claim(|b| b.did_not_catch_that()) {
                        // THE FIX FOR audit-3 §4 #4. He spoke, a voice was measured, and
                        // until now the window did not change at all. A NOTICE, not an
                        // error: voice is running and has made a call on his behalf.
                        observer.on_voice_event(&VoiceEvent::Notice {
                            message: n.ceo_message().to_string(),
                            at: now_millis(),
                        });
                    }
                    return;
                }
                self.discards = 0;
                // ADMITTED AND UNDERSTOOD — the one positive proof that the input works. The
                // run is over and the next genuine refusal speaks again. The session-scoped
                // half-duplex latch is untouched: what it describes is the room, not this run.
                let _ = self.claim(|b| {
                    b.recovered();
                    None
                });
                // **NO `VoiceEvent::Transcript` HERE ANY MORE, and that is half of the 2026-10-13
                // fix.** These words are one FRAGMENT of his turn (`turn.rs`). The window draws a
                // user message for every transcript event it receives, so announcing fragments
                // would draw his one thought as several messages; the turn announces itself once,
                // whole, when it is handed to the spine (`supervise`). The operator still gets the
                // fragment and its recognition time.
                eprintln!(
                    "[richos-voice] {} fragment recognized — {duration_ms} ms of audio in {latency_ms} ms; held until his turn is over",
                    wall_clock_utc()
                );
                // **THE PROVENANCE TRAVELS WITH THE WORDS, and this is the only place it
                // can.** `submit` is what becomes a ledger record; by the time anything
                // downstream sees the text, every fact about the audio it came from is gone.
                submit(text, admitted.rich_audible);
            }
            Err(e) => {
                eprintln!("[richos-voice] stt failed: {e}");
                observer.on_voice_event(&VoiceEvent::Error {
                    message: e.ceo_message(),
                    at: now_millis(),
                });
            }
        }
    }
}

impl VoiceController {
    /// Bring voice mode up. Every external dependency is resolved HERE, so a missing model or
    /// a missing microphone is one calm message at the toggle rather than a failure in the
    /// middle of a sentence.
    ///
    /// `submit` receives each recognized utterance and is expected to run a spine turn with
    /// it — the SAME path typed text takes. It is called on a dedicated thread and may block
    /// for the whole turn.
    pub fn start(
        opts: VoiceOptions,
        observer: Arc<dyn VoiceObserver>,
        submit: Arc<dyn Fn(String, bool) + Send + Sync>,
    ) -> Result<VoiceController, VoiceStartError> {
        let recognizer = Recognizer::resolve().map_err(VoiceStartError::Stt)?;

        // Automatic model selection is recorded in recognizer provenance. It does not
        // create a conversation notice when the user starts talking.

        // The echo canceller and the lock-free ring that feeds it. The ring goes to the
        // OUTPUT callback; the canceller itself is owned outright by the capture path (inside
        // `CaptureBrain`), so neither audio thread ever takes a lock to move the reference.
        let (aec, reference_ring) = EchoCanceller::new();
        let playout =
            Arc::new(Playout::start(Some(reference_ring)).map_err(VoiceStartError::Playout)?);

        let shared = Arc::new(Shared {
            level: AtomicU32::new(0),
            speaking: AtomicBool::new(false),
            his_floor: AtomicBool::new(false),
            answer_sounded: AtomicBool::new(false),
            his_onset: AtomicBool::new(false),
            takes_a_reply: AtomicBool::new(false),
            reply_invited: AtomicBool::new(false),
            finish_capture: AtomicBool::new(false),
            capture_finished: AtomicBool::new(false),
            drained_after_finish: AtomicBool::new(false),
            pending_speech: AtomicUsize::new(0),
            pending_answer: AtomicUsize::new(0),
            unheard: Mutex::new(Unheard::default()),
            generation: AtomicU64::new(0),
            no_audio: AtomicBool::new(false),
            running: AtomicBool::new(true),
        });
        // Rich's voice and no other (tts.rs): a sentence waits while it is installed or loaded,
        // and stops waiting the moment voice goes off.
        let wanted = shared.clone();
        let synth: Arc<dyn SpeechSynth> = Arc::new(RichVoice::waiting_while(move || wanted.running.load(Ordering::Relaxed)));
        let machine = Arc::new(Mutex::new(VoiceStateMachine::new()));
        machine.lock().unwrap().start();
        let gate: Arc<Mutex<AnswerGate<SpeakMsg>>> = Arc::new(Mutex::new(AnswerGate::new()));

        let (cap_tx, cap_rx) = channel::<CapMsg>();
        let (speak_tx, speak_rx) = channel::<SpeakMsg>();
        let (utt_tx, utt_rx) = channel::<Box<AdmittedUtterance>>();
        // `(text, rich_audible)` rather than `String`: the provenance has to reach the ledger
        // writer, and this serialized hop is the only thing between the recognizer and it.
        let (submit_tx, submit_rx) = channel::<(String, bool)>();
        // One message per admitted utterance, in admission order: its words, or `None` when the
        // recognizer refused it. The supervisor's `TurnAssembler` needs the refusals as much as
        // the words — a turn may not be released while a fragment is still at the recognizer,
        // and a refusal is how it learns the fragment has come back empty.
        let (words_tx, words_rx) = channel::<Option<(String, bool)>>();
        let force_barge = Arc::new(AtomicBool::new(false));
        // Finding 4: turns withdrawn before the submit thread reached them, the count of turns
        // started, and the shell's way to stop the one that is running.
        let withdrawn: Arc<Mutex<HashSet<u64>>> = Arc::new(Mutex::new(HashSet::new()));
        let his_floor_hold: FloorHold = Default::default();
        let submits_done = Arc::new(AtomicU64::new(0));
        let turns_started = Arc::new(AtomicU64::new(0));
        let withdraw: WithdrawHook = Arc::new(Mutex::new(None));
        // The canceller's live state, lossless and signed — see `AecShared`. One relaxed store
        // per frame from the audio thread, readable by anyone without a lock.
        let aec_state_for_diagnostics = Arc::new(AecShared::default());
        let aec_state_read = aec_state_for_diagnostics.clone();

        // ---- the audio capture callback ------------------------------------------------
        // A THIN adapter over CaptureBrain: no locks, no allocation beyond the frame copy the
        // echo gate needs, and no UI events. All the decisions live in CaptureBrain, which is
        // unit-tested as a whole (tests/barge_in_composition.rs).
        let cb_shared = shared.clone();
        let cb_force = force_barge.clone();
        // The canceller moves INTO the brain, which moves into the callback closure. No locks
        // on the audio thread, and the echo cancellation, the VAD, the barge-in monitor and
        // the endpointer are one composed unit that `tests/barge_in_composition.rs` can drive.
        let mut brain = CaptureBrain::with_aec(aec);
        let aec_state = aec_state_for_diagnostics.clone();

        let capture = capture::start(&opts.source, move |frame| {
            // VOICE IS GOING OFF: close what he is still saying, once, and hear nothing after.
            if cb_shared.finish_capture.load(Ordering::SeqCst) {
                if !cb_shared.capture_finished.load(Ordering::SeqCst) {
                    for msg in brain.finish() {
                        // Fails only when the supervisor has already gone; then nobody is left
                        // to hand his words to, and there is nothing more to send.
                        if cap_tx.send(msg).is_err() {
                            break;
                        }
                    }
                    cb_shared.capture_finished.store(true, Ordering::SeqCst);
                }
                return;
            }
            let speaking = cb_shared.speaking.load(Ordering::Relaxed);
            let forced = cb_force.swap(false, Ordering::Relaxed);
            if cb_shared.reply_invited.swap(false, Ordering::Relaxed) {
                brain.invite_reply();
            }
            for msg in brain.push_frame(frame, speaking, forced) {
                let _ = cap_tx.send(msg);
            }
            cb_shared.his_onset.store(brain.his_onset_building(), Ordering::Relaxed);
            cb_shared.takes_a_reply.store(brain.takes_a_reply(), Ordering::Relaxed);
            cb_shared.set_level(brain.level());
            // Publish the canceller's live state for the supervisor and the UI without ever
            // touching the audio thread from outside it. Signed and unclamped: a negative ERLE
            // is a real reading and the old packing could not express it.
            // NO CANCELLER AT ALL AND A COLD CANCELLER BOTH PUBLISH "not measured", which is
            // the truth in both cases and was `0.0` in both cases until 2026-09-17.
            let m = brain.aec_metrics();
            aec_state.store(
                brain.aec_confident(),
                m.map(|m| m.erle_db).unwrap_or(0.0),
                m.is_some_and(|m| m.erle_measured),
                m.map(|m| m.leak_floor_rms).unwrap_or(0.0),
                m.is_some_and(|m| m.leak_measured),
            );
        })
        .map_err(VoiceStartError::Capture)?;

        let diagnostics = Diagnostics {
            input_source: capture.source_label.clone(),
            input_rate: capture.input_rate,
            input_channels: capture.input_channels,
            output_device: playout.device_label.clone(),
            output_rate: playout.device_rate,
            output_channels: playout.channels,
            stt_model: recognizer.model_id().to_string(),
            stt_binary: recognizer.binary_path().display().to_string(),
            echo_gate: format!(
                "PBFDAF {} taps ({:.0} ms tail)",
                crate::aec::AEC_TAPS,
                1000.0 * crate::aec::filter_tail_secs()
            ),
            echo_cancellation: true,
            // The consecutive fallback, reported because it is what applies with no canceller.
            barge_in_frames: BARGE_IN_DEBOUNCE_FRAMES,
            barge_in_secs: frames_to_secs(BARGE_IN_DEBOUNCE_FRAMES),
            // The rule ACTUALLY IN FORCE here, from the one frame-math helper. Never typed.
            barge_in_window_frames: crate::bargein::AEC_BARGE_IN_WINDOW_FRAMES,
            barge_in_required_frames: crate::bargein::AEC_BARGE_IN_REQUIRED_FRAMES,
            barge_in_window_secs: frames_to_secs(crate::bargein::AEC_BARGE_IN_WINDOW_FRAMES),
            barge_in_required_secs: frames_to_secs(crate::bargein::AEC_BARGE_IN_REQUIRED_FRAMES),
            // This constructor builds the session that HAS a canceller (`echo_cancellation: true`
            // three lines up), so the conjunction is available on every frame from the first one.
            barge_in_near_end_gated: true,
        };
        // **STAMPED.** Defect 4 of the candidate-.6 walk: this line, the only thing printed
        // when voice starts, carried no time at all, so it could not be lined up against the
        // ledger or against the answer-window lines that DO carry one. Same clock as
        // `wall_clock_utc` everywhere else in this file, for the reason that function exists.
        eprintln!("[richos-voice] {} voice session OPEN — {}", wall_clock_utc(), diagnostics.summary());

        let mut threads = Vec::new();

        // **ONE NOTICE BUDGET FOR THE WHOLE VOICE SESSION**, created here because here is the
        // only place both emitters can be handed the same one. Until 2026-09-17 the recognizer
        // thread and the supervisor thread each kept their own latches and a person saw three
        // apologies for one silence — see `RefusalNotices`.
        let notices = Arc::new(Mutex::new(RefusalNotices::new()));

        // ---- recognizer thread ----------------------------------------------------------
        {
            let observer = observer.clone();
            let notices = notices.clone();
            let scratch = opts.scratch_dir.clone();
            let submit_tx = submit_tx.clone();
            threads.push(std::thread::spawn(move || {
                // THE WHOLE DECISION LIVES IN `RecognizerDesk`, deliberately.
                //
                // This thread used to hold it inline: transcribe, then judge the TEXT. That
                // shape is what let *"1, 2, 3, testing."* be submitted as the CEO's own
                // message from a silent room on 2026-09-04. The desk asks the AUDIO first
                // and only reaches whisper if the recording earned it, and it is a separate
                // type so that ordering is testable rather than merely visible.
                let mut desk = RecognizerDesk::sharing(notices);
                while let Ok(admitted) = utt_rx.recv() {
                    let mut words = None;
                    desk.handle(
                        &admitted,
                        observer.as_ref(),
                        |samples| recognizer.transcribe(samples, &scratch),
                        |text, rich_audible| words = Some((text, rich_audible)),
                    );
                    // To the supervisor, which holds it until his turn is over. If the supervisor
                    // is already gone, voice was switched off while this fragment was at the
                    // recognizer, and words he said before the toggle have always still reached
                    // the conversation: they go straight to the submit thread, as they did before
                    // turns were assembled.
                    if let Err(unsent) = words_tx.send(words) {
                        if let Some(fragment) = unsent.0 {
                            if submit_tx.send(fragment).is_err() {
                                eprintln!("[richos-voice] a fragment recognized after voice went off could not be handed over: the submit thread has ended");
                            }
                        }
                    }
                }
            }));
        }

        // ---- submit thread (one turn at a time, in order) --------------------------------
        {
            let withdrawn = withdrawn.clone();
            let hold = his_floor_hold.clone();
            let done = submits_done.clone();
            threads.push(std::thread::spawn(move || submit_turns(submit_rx, withdrawn, hold, done, submit)));
        }

        // ---- speaker thread --------------------------------------------------------------
        {
            let shared = shared.clone();
            let playout = playout.clone();
            let scratch = opts.scratch_dir.clone();
            let observer = observer.clone();
            threads.push(std::thread::spawn(move || {
                let mut told: Option<u64> = None;
                while let Ok(msg) = speak_rx.recv() {
                    let result = speak_one(&shared, &msg, &*playout, &mut |text, emit| {
                        synth.synthesize_stream(text, playout.device_rate, &scratch, emit)
                    });
                    if let Err(e) = result {
                        eprintln!("[richos-voice] tts failed: {e}");
                        // Told once, not once per sentence (again only after an interruption starts
                        // a new speech generation), and never after voice went off.
                        if shared.running.load(Ordering::Relaxed) && told != Some(msg.generation) {
                            told = Some(msg.generation);
                            observer.on_voice_event(&VoiceEvent::Error { message: e.ceo_message(), at: now_millis() });
                        }
                    }
                }
            }));
        }

        // ---- supervisor: the ONLY place UI events are emitted -----------------------------
        {
            let shared = shared.clone();
            let machine = machine.clone();
            let playout = playout.clone();
            let observer = observer.clone();
            // The canceller's live state, published per frame by the audio thread. The
            // supervisor reads it to report WHY a discard happened instead of guessing.
            let aec_state = aec_state_for_diagnostics.clone();
            let input_latency = capture.latency();
            let notices = notices.clone();
            let floor = Floor { gate: gate.clone(), speak_tx: speak_tx.clone(), held_logged: 0 };
            let withdrawal = Withdrawal {
                withdrawn: withdrawn.clone(),
                turns_started: turns_started.clone(),
                submits_done: submits_done.clone(),
                hook: withdraw.clone(),
                floor: his_floor_hold.clone(),
            };
            threads.push(std::thread::spawn(move || {
                supervise(
                    shared,
                    machine,
                    playout,
                    observer,
                    cap_rx,
                    utt_tx,
                    words_rx,
                    submit_tx,
                    floor,
                    withdrawal,
                    aec_state,
                    input_latency,
                    notices,
                );
            }));
        }

        Ok(VoiceController {
            shared,
            machine,
            chunker: Mutex::new(SentenceChunker::new()),
            gate,
            turns_started,
            withdraw,
            speak_tx: Some(speak_tx),
            playout,
            force_barge,
            aec_state: aec_state_read,
            diagnostics,
            capture: Some(capture),
            threads,
        })
    }

    pub fn state(&self) -> VoiceState {
        self.machine.lock().map(|m| m.state()).unwrap_or(VoiceState::Off)
    }

    pub fn diagnostics(&self) -> &Diagnostics {
        &self.diagnostics
    }

    /// **Has the echo canceller earned the short barge-in window yet?**
    ///
    /// False at start-up and for the first few seconds of Rich actually speaking, because
    /// `EchoCanceller::confident` requires 2.000 s of far-end audio to learn the path plus a
    /// 2.000 s hold. While it is false the CEO needs the 5.008 s talk-over or the "tap to
    /// stop" control, exactly as before; once it is true a 0.400 s interruption registers.
    ///
    /// The UI's "headphones recommended" note should follow THIS, not `Diagnostics`.
    pub fn echo_cancellation_confident(&self) -> bool {
        self.aec_state.confident()
    }

    /// Live Echo Return Loss Enhancement in dB — how much of Rich's own voice the canceller is
    /// currently removing from the microphone. **`None` until a single block has been measured**,
    /// and negative when the filter is adding energy rather than removing it, which is a real
    /// state this returned `0` for until 2026-09-17. Measured, never estimated, and never
    /// zero-as-a-placeholder.
    pub fn echo_return_loss_enhancement_db(&self) -> Option<f32> {
        self.aec_state.erle_db()
    }

    /// The tracked typical residual echo while Rich is audible, in dBFS, or `None` while the
    /// tracker is unseeded. Compare with [`crate::aec::CONFIDENT_LEAK_RMS`] to see how far the
    /// canceller is from earning the short barge-in window on this hardware — the gap, not
    /// the verdict.
    pub fn echo_leak_floor_dbfs(&self) -> Option<f32> {
        self.aec_state.leak_dbfs()
    }

    /// The barge-in debounce actually in force right now, in seconds: 5.008 while the
    /// canceller is unconfident, 0.400 once it is.
    pub fn barge_in_secs_now(&self) -> f32 {
        if self.echo_cancellation_confident() {
            crate::bargein::aec_barge_in_window_secs()
        } else {
            crate::bargein::barge_in_debounce_secs()
        }
    }

    /// The mic is open and healthy but has delivered nothing above -80.00 dBFS for 3.008 s.
    /// The UI reads this off `rich://voice-state`; this accessor exists for the live
    /// hardware check and for the shell's diagnostics.
    pub fn no_audio(&self) -> bool {
        self.shared.no_audio.load(Ordering::Relaxed)
    }

    /// **THE ONLY PLACE A SENTENCE IS HANDED TO THE SPEAKER THREAD.**
    ///
    /// One function because the increment of [`Shared::pending_speech`] has to happen at the
    /// SEND and not at the synthesis: between the two sits `say`'s process spawn, and that
    /// interval is precisely the gap an utterance used to be born untainted in. Counting from
    /// the send means Rich's answer is "in progress" from the instant its first sentence is
    /// handed over, before any sound exists — which also closes the 25 ms `TICK` the
    /// supervisor would otherwise take to notice the queue filling.
    ///
    /// A failed send decrements immediately: the speaker thread is gone, so nothing is owed.
    ///
    /// **And it asks the floor first** (`turn.rs`, [`AnswerGate`]). While the CEO has the floor the
    /// sentence is held rather than sent, so nothing of Rich is owed and nothing of him is audible
    /// while the CEO is still talking; the supervisor sends it when he gives the floor back. A
    /// superseded or interrupted answer's sentences are dropped here.
    fn send_speak(&self, generation: u64, text: String) {
        self.send_to_speaker(generation, text, false);
    }

    /// [`VoiceController::send_speak`] for a sentence that belongs to no answer
    /// ([`AnswerGate::offer_announcement`]).
    fn send_to_speaker(&self, generation: u64, text: String, announcement: bool) {
        let Some(tx) = &self.speak_tx else { return };
        send_to_speaker(&self.shared, &self.gate, tx, generation, text, announcement);
    }

    /// A `rich://chunk` delta arrived. Accumulate, and hand every completed sentence to the
    /// speaker thread immediately — this is the pipelining.
    pub fn speak_delta(&self, delta: &str) {
        let generation = self.shared.generation.load(Ordering::Relaxed);
        let sentences = match self.chunker.lock() {
            Ok(mut c) => c.push(delta),
            Err(_) => return,
        };
        for text in sentences {
            self.send_speak(generation, text);
        }
    }

    /// The turn's terminal event arrived: speak whatever is left.
    pub fn speak_end(&self) {
        let generation = self.shared.generation.load(Ordering::Relaxed);
        let tail = match self.chunker.lock() {
            Ok(mut c) => c.flush(),
            Err(_) => None,
        };
        if let Some(text) = tail {
            self.send_speak(generation, text);
        }
    }

    /// **A job's result, said on its own** ("Will is on your team now"; `rich://work-result`).
    /// It is not part of any answer, so it does not use the answer's chunker, and a Stop he gave
    /// an earlier reply does not silence it (review rv-20261015T125238Z-37f970a3, finding 1:
    /// after tap-to-stop no turn starts, so the mute stayed and the result was dropped). It still
    /// waits while he has the floor, queues behind Rich's own sentences, and a Stop while any
    /// sentence of it is being said stops the whole result, not just that sentence
    /// ([`send_result`]).
    pub fn speak_result(&self, text: &str) {
        let Some(tx) = &self.speak_tx else { return };
        let generation = self.shared.generation.load(Ordering::Relaxed);
        send_result(&self.shared, &self.gate, tx, generation, text);
    }

    /// **The turn DIED. Say so out loud** (row 3.30, answer 1).
    ///
    /// The counterpart to [`VoiceController::speak_end`], and it must never be that
    /// function: `speak_end` FLUSHES the chunker's tail, so calling it on a turn that died
    /// mid-sentence speaks the half-sentence and then falls silent. This drops the fragment
    /// and speaks [`VoiceNotice::ReplyCutOff`] instead.
    ///
    /// `reason` is one plain sentence from whoever knows why — `richos-core`'s
    /// `upstream::UpstreamFault::ceo_message` for a `529` or a `429`. `None` is fine and
    /// common; voice never invents one.
    ///
    /// **Nothing already queued is stopped.** The generation counter is not bumped and
    /// `playout.stop_now()` is not called, so completed sentences finish and the notice
    /// follows them. See [`CutOffDesk`] for why that order is the design.
    pub fn turn_cut_off(&self, reason: Option<&str>, observer: &dyn VoiceObserver) {
        // 1. The fragment that will never be finished.
        if let Ok(mut c) = self.chunker.lock() {
            c.reset();
        }
        // A turn he SUPERSEDED (he went on before Rich answered, and it was stopped so his whole
        // thought is one request) did not fail: saying "I was cut off" about it would be false.
        if self.gate.lock().map(|g| g.muted()).unwrap_or(false) {
            if let Ok(mut m) = self.machine.lock() {
                m.turn_ended();
            }
            return;
        }
        // 2. and 3. — the decision itself, in the unit-tested desk.
        let generation = self.shared.generation.load(Ordering::Relaxed);
        CutOffDesk::handle(reason, observer, |text| {
            self.send_speak(generation, text.to_string());
        });
        // 4. The state machine last: the turn is over whatever the speaker does with the
        //    sentences, and `turn_ended` is what puts the panel back to listening.
        if let Ok(mut m) = self.machine.lock() {
            m.turn_ended();
        }
    }

    /// `rich://turn-started` — Rich has the turn.
    pub fn turn_started(&self) {
        self.turns_started.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut m) = self.machine.lock() {
            m.turn_started();
        }
        // A new turn's answer is his to hear, whatever the last one was.
        if let Ok(mut gate) = self.gate.lock() {
            gate.next_turn();
        }
        if let Ok(mut c) = self.chunker.lock() {
            c.reset();
        }
    }

    /// A terminal turn event. Rich may still be speaking the tail.
    pub fn turn_ended(&self) {
        if let Ok(mut m) = self.machine.lock() {
            m.turn_ended();
        }
    }

    /// The UI's "tap to stop": interrupt Rich right now, no debounce. Returns the number of
    /// mono samples dropped — real units, not an adjective.
    pub fn force_barge_in(&self) -> usize {
        self.force_barge.store(true, Ordering::Relaxed);
        let dropped = stop_pressed(&self.shared, &self.gate, &*self.playout);
        if let Ok(mut c) = self.chunker.lock() {
            c.reset();
        }
        if let Ok(mut m) = self.machine.lock() {
            m.barge_in();
        }
        dropped
    }

    /// **How the shell stops the turn that is running**, used when his continuation withdraws a
    /// turn he had not finished (review rv-20261013T184256Z, finding 4): the same stop as the
    /// Stop button. Without one installed, the partial turn runs on and only its answer is
    /// silenced; his whole thought still follows as one turn.
    pub fn on_withdraw(&self, stop_running_turn: Arc<dyn Fn() + Send + Sync>) {
        if let Ok(mut hook) = self.withdraw.lock() {
            *hook = Some(stop_running_turn);
        }
    }

    /// Worst-case delay between an interruption and silence: one device callback period.
    pub fn stop_latency_secs(&self) -> f32 {
        self.playout.stop_latency_secs()
    }

    pub fn queued_speech_secs(&self) -> f32 {
        self.playout.queued_secs()
    }
}

/// [`VoiceController::send_to_speaker`]: ask the gate, then send what it lets through.
fn send_to_speaker(
    shared: &Shared,
    gate: &Mutex<AnswerGate<SpeakMsg>>,
    tx: &Sender<SpeakMsg>,
    generation: u64,
    text: String,
    announcement: bool,
) {
    let result = if announcement { next_result() } else { 0 };
    offer_to_speaker(shared, gate, tx, SpeakMsg { generation, text, notice: announcement.then_some(0), result });
}

/// [`send_to_speaker`] for a message already built.
fn offer_to_speaker(shared: &Shared, gate: &Mutex<AnswerGate<SpeakMsg>>, tx: &Sender<SpeakMsg>, msg: SpeakMsg) {
    let his_floor = shared.his_floor.load(Ordering::Relaxed);
    let announcement = msg.notice.is_some();
    let now = match gate.lock() {
        Ok(mut gate) if announcement => gate.offer_announcement(msg, his_floor),
        Ok(mut gate) => gate.offer(msg, his_floor),
        // A poisoned gate must not silence Rich for the rest of the session.
        Err(_) => Some(msg),
    };
    if let Some(msg) = now {
        dispatch(shared, tx, msg);
    }
}

/// [`VoiceController::speak_result`]: the result in sentences, each offered as an announcement,
/// all under ONE result number, so a Stop during any of them ends the rest of it.
fn send_result(shared: &Shared, gate: &Mutex<AnswerGate<SpeakMsg>>, tx: &Sender<SpeakMsg>, generation: u64, text: &str) {
    let mut chunker = SentenceChunker::new();
    let mut sentences = chunker.push(text);
    sentences.extend(chunker.flush());
    let result = next_result();
    for text in sentences {
        offer_to_speaker(shared, gate, tx, SpeakMsg { generation, text, notice: Some(0), result });
    }
}

/// **He pressed Stop** ([`VoiceController::force_barge_in`]): the answer's generation ends,
/// nothing more of it is said, and the device queue is cut. An announcement that was waiting
/// behind it and he never heard is held again and said next; the one sounding as he pressed is
/// what he stopped. Returns the samples dropped.
fn stop_pressed(shared: &Shared, gate: &Mutex<AnswerGate<SpeakMsg>>, out: &dyn SpeechQueue) -> usize {
    shared.generation.fetch_add(1, Ordering::Relaxed);
    // He stopped this answer: nothing more of it is said, including sentences the model is
    // still writing.
    if let Ok(mut gate) = gate.lock() {
        gate.supersede();
    }
    let dropped = out.stop_now();
    reclaim_unheard(shared, gate, out.played(), true);
    dropped
}

/// **The device queue was just cut and the generation ended** (a barge-in, a Stop, or his words
/// starting a new turn): every announcement sent and not heard to its last sample goes back to
/// the front of the gate, to be said when the floor is free (review rv-20261015T132600Z-fdc3dafb,
/// finding 1: a "joined your team" notice queued behind Rich's reply was dropped with it). The old
/// copies carry the ended generation, so the speaker drops them. Returns how many.
fn reclaim_unheard(shared: &Shared, gate: &Mutex<AnswerGate<SpeakMsg>>, played: u64, he_stopped: bool) -> usize {
    let again = match shared.unheard.lock() {
        Ok(mut u) => u.reclaim(played, he_stopped, |a, b| a.result == b.result),
        Err(_) => return 0,
    };
    let n = again.len();
    if n > 0 {
        if let Ok(mut gate) = gate.lock() {
            gate.hold_again(again);
        }
        eprintln!(
            "[richos-voice] {} {n} announcement(s) not yet heard were cut; held to be said again",
            wall_clock_utc()
        );
    }
    n
}

/// Drop `held` (the microphone) FIRST, then join the workers.
///
/// The recognition and submission threads drain their queues, so a join can last as long as a
/// transcription or an assistant turn. Holding the capture stream through that wait kept the
/// microphone open after the CEO had already said stop listening. The capture callback only
/// sends on channels, so closing it first cannot strand a worker: the channels just end.
fn release_then_join<T>(held: T, threads: Vec<std::thread::JoinHandle<()>>) {
    drop(held);
    for t in threads {
        let _ = t.join();
    }
}

/// How long switching voice off waits for his last open recording to be closed and handed over:
/// one capture callback (a 16.000 ms frame) plus one supervisor tick (25 ms) is the expected
/// cost; this bounds a dead capture stream, it is not an estimate.
const FINISH_WAIT: Duration = Duration::from_millis(500);

impl Drop for VoiceController {
    fn drop(&mut self) {
        // **HIS LAST WORDS FIRST** (review rv-20261013T183507Z). What he said in the last 0.800 s
        // is still an open recording inside the capture callback, and destroying the stream
        // destroyed it. Ask the callback to close it, wait until the supervisor has handed it to
        // the recognizer, and only then shut down; `TurnDesk`'s drop then waits for its words and
        // sends the turn whole.
        self.shared.finish_capture.store(true, Ordering::SeqCst);
        let asked = Instant::now();
        while !self.shared.drained_after_finish.load(Ordering::SeqCst) && asked.elapsed() < FINISH_WAIT {
            std::thread::sleep(Duration::from_millis(5));
        }
        self.shared.running.store(false, Ordering::SeqCst);
        // Silence Rich immediately, then let the channels close and the threads finish.
        self.playout.stop_now();
        self.shared.generation.fetch_add(1, Ordering::SeqCst);
        if let Ok(mut m) = self.machine.lock() {
            m.stop();
        }
        // Close the speaker channel first: the speaker thread blocks on recv() and can only
        // exit when every sender is gone. Joining before this would deadlock.
        self.speak_tx.take();
        release_then_join(self.capture.take(), self.threads.drain(..).collect());
    }
}

/// **WHETHER TO TELL HIM RICH COULD NOT LISTEN WHILE SPEAKING, AND HOW OFTEN.**
///
/// A struct rather than a `bool` in [`supervise`] for the reason [`RecognizerDesk`] is one:
/// what has to be provable is not the sentence but the CADENCE. A latch living as a local
/// inside a spawned thread is reachable only by a test that owns a microphone, which means in
/// practice it is reachable by no test at all.
///
/// ## ONCE PER VOICE SESSION — changed from once-per-run, and the number is the reason
///
/// It used to clear whenever an utterance got through, on the theory that being heard proves
/// the run is over. That theory assumed the discards were EVENTS — the CEO interrupting now
/// and then. On his actual hardware they are not.
///
/// Measured on this Mac, 2026-09-17 (`docs/verification/2026-09-17-aec-erle-on-the-ceo-rig.md`):
/// Mac mini Speakers out, Elgato Wave:3 in, steady-state ERLE of **−0.5 … +0.7 dB** across five
/// live runs — nothing measurably removed — against a residual that would have to sit at
/// **−52.04 dBFS** ([`crate::aec::CONFIDENT_LEAK_RMS`]) while the microphone reads −42 … −46 dBFS
/// during Rich's speech. The canceller does not reach confidence there, so Rich's own voice comes
/// back into the microphone, opens an utterance, and is discarded as taint **after every single
/// spoken answer**. Cleared-on-heard therefore means: he speaks, he is heard, the
/// latch clears, Rich answers, the echo is discarded, the notice fires. Once per answer, forever.
///
/// The crate's own doctrine on every other notice here is that a line each time is a drip that
/// trains him to ignore the one that matters. A drip about a STANDING property of the room is
/// worse than that — it is a drip that can never stop, because nothing he does changes it.
///
/// So it is said once, the first time the session hits it, and then never again. What is lost is
/// real and is named rather than glossed: a second genuine talk-over later in the same session
/// is silent. That is the honest trade on hardware where the app cannot tell a second talk-over
/// from the echo of Rich's next sentence.
///
/// Pure: no clock, no channel, no observer.
///
/// ## AND IT IS NOW ONE BUDGET FOR THE WHOLE FAMILY — defect 1 of the candidate-.6 walk
///
/// Ray walked the first spoken answer of a voice session on the CEO's screen on 2026-09-17 and
/// **three** notice cards stacked up in his thread while he had said nothing since 21:31:09
/// (`docs/verification/2026-09-17-nightly-1.2.0-20260917.6-onscreen-audit.md` defect 1,
/// frame `a6-08`):
///
/// ```text
///   "While I was speaking, I couldn't tell your voice from my own — …"
///   "That didn't come through as speech, so I haven't sent anything. I'm still listening."
///   "I didn't catch that, so I haven't sent anything. I'm still listening."
/// ```
///
/// **Every one of the three latches was working exactly as written.** Each honored "at most
/// once"; there were three of them, in two different threads, that had never been introduced.
/// *"The brief's expectation — 'On speakers you should see it at most once per voice session'
/// — is literally true of that one notice and not true of what a person sees."*
///
/// ## WHERE THE THREE PATHS MEET — read from source, and the answer was NOWHERE
///
/// | sentence | emitted at | precondition |
/// |---|---|---|
/// | [`VoiceNotice::CouldNotListenWhileSpeaking`] | [`supervise`], `CapMsg::Discarded { tainted: true }` | supervisor thread |
/// | [`VoiceNotice::HeardNoVoice`] | [`RecognizerDesk::handle`] | recognizer thread |
/// | [`VoiceNotice::DidNotCatchThat`] | [`RecognizerDesk::handle`] | recognizer thread |
/// | [`VoiceNotice::SoundButNoWords`] | [`RecognizerDesk::handle`] | recognizer thread |
///
/// **A CORRECTION TO THE OBVIOUS THEORY, because it is wrong and it would have produced a
/// no-op fix.** The natural reading of Ray's three cards is that echo-discarded audio produces
/// all three. It cannot: on the tainted branch of [`CaptureBrain::push_residual`] the
/// `Utterance` value is **dropped** and only `CapMsg::Discarded` is sent, `supervise`'s
/// `Discarded` arm never touches `utt_tx`, and [`RecognizerDesk::handle`] is reachable only
/// through `utt_rx`. So audio discarded as Rich's own echo can never reach whisper and can
/// never raise either recognizer notice. Suppressing them "on the discard path" would suppress
/// nothing, because there is no such path. Pinned by
/// `echo_discarded_audio_can_never_raise_a_recognizer_notice`.
///
/// What actually produced cards 2 and 3 is the defect audit-5 already filed as its #8 —
/// *"One non-speech sound produced two notice cards, back to back"* — an utterance the app
/// ADMITTED (so Rich was not audible for it) whose audio failed the pre-whisper gate and whose
/// successor failed the post-whisper one. Two refusals, two independent latches, two cards, one
/// noise. Card 1 is the echo discard; cards 2 and 3 are that pair.
///
/// ## THE RULE: ONE CARD AT A TIME, REPLACED ONLY BY A STRICTLY STRONGER ONE
///
/// Not a flat "one per session" budget, because that would silence the escalation this crate
/// deliberately has: a run of three discards is supposed to hand over from the short line to
/// [`VoiceNotice::SoundButNoWords`], which says more. So each sentence carries an AUTHORITY and
/// may only be said if it outranks whatever is currently standing:
///
/// ```text
///   3  CouldNotListenWhileSpeaking   the room and the hardware; a standing property
///   2  SoundButNoWords               a RUN of refusals; stronger than any single one
///   1  HeardNoVoice, DidNotCatchThat one refusal, pre- or post-whisper
/// ```
///
/// Equal rank never replaces, which is the double-card fix on its own. Lower rank never
/// replaces, which is the third-card fix. The escalation 1 → 2 still happens, once.
///
/// [`VoiceNotice::ReplyCutOff`] is deliberately **not** a member and there is no method here
/// that can take it: it describes the ANSWER dying rather than audio being refused, it is the
/// one notice that is SPOKEN as well as shown, and it is latched per turn by its own caller. A
/// budget an echo discard could spend on his behalf must never be able to silence the one line
/// he hears with his ears.
///
/// ## WHAT CLEARS IT
///
/// [`Self::recovered`], called when an utterance is admitted AND transcribed to something
/// meaningful — a real turn, which is positive proof the input works. It clears the standing
/// card so a later genuine refusal speaks again. It does **not** clear the half-duplex latch:
/// that one is session-scoped for the measured reason above, and
/// `being_heard_does_not_re_arm_the_notice_because_the_condition_is_not_an_event` holds it.
///
/// [`Self::reply_invited`], called when the half-duplex line is SHOWN: the line asked him to
/// say it again, so its own card stops outranking a refusal of that repeat. Its latch stays.
#[derive(Debug, Clone, Default)]
pub struct RefusalNotices {
    /// The card currently standing, and therefore the authority a new one has to beat. `None`
    /// between a recovery and the next refusal.
    standing: Option<VoiceNotice>,
    /// [`VoiceNotice::CouldNotListenWhileSpeaking`] has been said in this voice session. A
    /// SECOND latch rather than a special case of `standing`, because the two have different
    /// lifetimes on purpose: `standing` clears on recovery and this never does.
    half_duplex_said: bool,
}

impl RefusalNotices {
    pub fn new() -> Self {
        RefusalNotices::default()
    }

    /// Where each member of the family sits. Private, and there is no variant for
    /// [`VoiceNotice::ReplyCutOff`] because there is no method that accepts it.
    fn authority(n: VoiceNotice) -> u8 {
        match n {
            VoiceNotice::CouldNotListenWhileSpeaking => 3,
            VoiceNotice::SoundButNoWords => 2,
            VoiceNotice::HeardNoVoice | VoiceNotice::DidNotCatchThat => 1,
            // Unreachable: no public method passes it. Ranked 0 so that if a future edit ever
            // does route it here it is refused loudly by the test suite rather than quietly
            // silencing something.
            VoiceNotice::ReplyCutOff => 0,
        }
    }

    /// The whole decision, in one place. `Some(n)` means say it; `None` means a card of at
    /// least this authority is already standing.
    fn claim(&mut self, n: VoiceNotice) -> Option<VoiceNotice> {
        let mine = RefusalNotices::authority(n);
        if mine == 0 {
            return None;
        }
        let standing = self.standing.map(RefusalNotices::authority).unwrap_or(0);
        if mine <= standing {
            return None;
        }
        self.standing = Some(n);
        Some(n)
    }

    /// An utterance that carried Rich's own audible answer was discarded because the canceller
    /// could not vouch for the residual. `Some` at most once per voice session.
    pub fn half_duplex(&mut self) -> Option<VoiceNotice> {
        if self.half_duplex_said {
            return None;
        }
        let said = self.claim(VoiceNotice::CouldNotListenWhileSpeaking);
        if said.is_some() {
            self.half_duplex_said = true;
        }
        said
    }

    /// The pre-whisper gate refused the audio: it carried no voice.
    pub fn heard_no_voice(&mut self) -> Option<VoiceNotice> {
        self.claim(VoiceNotice::HeardNoVoice)
    }

    /// The post-whisper filter refused the transcript: a voice, but no words.
    pub fn did_not_catch_that(&mut self) -> Option<VoiceNotice> {
        self.claim(VoiceNotice::DidNotCatchThat)
    }

    /// A RUN of refusals reached [`SILENT_DISCARD_RUN`] — the stronger statement, which takes
    /// over from whichever single-refusal line is standing.
    pub fn sound_but_no_words(&mut self) -> Option<VoiceNotice> {
        self.claim(VoiceNotice::SoundButNoWords)
    }

    /// An utterance was admitted and understood: the input demonstrably works, so the standing
    /// card is spent and the next genuine refusal may speak. The session-scoped half-duplex
    /// latch is deliberately untouched.
    pub fn recovered(&mut self) {
        self.standing = None;
    }

    /// **The half-duplex line has just been SHOWN, asking him to say it again: what he says next
    /// may be refused out loud.** The line claims its authority-3 card at the discard, so the
    /// recognizer's lines cannot stack on it while it waits to be shown (`HalfDuplexDelivery`).
    /// Once shown it has said what it had to say, and left standing it outranked every refusal
    /// of the very repeat it asked for: a word whisper heard as `[BLANK_AUDIO]` got no line at
    /// all, his words lost again in silence (review rv-20261021T225454Z-d47fcfe5-d9e7). So the
    /// card it holds is released here and the next refusal speaks. The session-scoped
    /// half-duplex latch is untouched, so the line itself still comes at most once.
    pub fn reply_invited(&mut self) {
        if self.standing == Some(VoiceNotice::CouldNotListenWhileSpeaking) {
            self.standing = None;
        }
    }

    /// What is standing right now. The diagnostic that makes "why did it not say anything"
    /// answerable without re-reading the callers.
    pub fn standing(&self) -> Option<VoiceNotice> {
        self.standing
    }

    /// Has the half-duplex line been said in this voice session?
    pub fn half_duplex_already_said(&self) -> bool {
        self.half_duplex_said
    }
}

/// **THE HALF-DUPLEX LINE IS SHOWN WHEN RICH CAN TAKE THE REPLY, NOT WHEN IT IS EARNED.**
///
/// [`VoiceNotice::CouldNotListenWhileSpeaking`] ends "please say it again now". It is earned at
/// a tainted discard, which happens while Rich is still audible, and while he is audible a
/// short reply is discarded the same way. Shown at once (as until 2026-10-21), it invited the
/// user to speak into the very window that had just lost their words: measured on the
/// committed echo-path recording, the first discard came 2.608 s into an answer with
/// 22.336 s of it still to play (review rv-20261021T175303Z-70f43b54-a22b). So the supervisor
/// OWES the line at the discard ([`Self::owe`]) and delivers it on the first tick a reply would
/// be heard ([`Self::due`]): Rich no longer audible, the echo lookback past and no tainted
/// recording still open ([`CaptureBrain::takes_a_reply`]). The budget it spends is still
/// claimed at the discard, so the recognizer's lines cannot stack on top of it meanwhile.
#[derive(Debug, Default)]
pub struct HalfDuplexDelivery {
    owed: Option<VoiceNotice>,
}

impl HalfDuplexDelivery {
    /// The line was earned (a tainted discard claimed it). Held until [`Self::due`].
    pub fn owe(&mut self, n: VoiceNotice) {
        self.owed = Some(n);
    }

    /// The line to show now, if one is owed and a reply starting now would be heard: Rich is no
    /// longer audible (`rich_audible`) AND capture would take it (`capture_takes_a_reply`, the
    /// capture callback's latest [`CaptureBrain::takes_a_reply`]); `None` otherwise. Rich falling
    /// silent is not enough: for 0.416 s after it an utterance still carries his echo in its
    /// pre-roll, and an open tainted recording absorbs whatever is said until its hangover ends
    /// (review rv-20261021T205407Z-bcae8bd4-dcd6).
    pub fn due(&mut self, rich_audible: bool, capture_takes_a_reply: bool) -> Option<VoiceNotice> {
        if rich_audible || !capture_takes_a_reply {
            return None;
        }
        self.owed.take()
    }
}

/// **THE CLOSING LINE OF A VOICE SESSION, printed on every path out of [`supervise`].**
///
/// Defect 4 of Ray's candidate-.6 walk, verbatim: *"The new timestamped logging covers the
/// answer window but not the voice session around it: starting voice prints one untimestamped
/// line, **ending voice prints nothing at all**."* His walk record has to say
/// `(ending voice printed NOTHING to the log - defect 4)` because there was nothing to quote.
///
/// **A `Drop` guard rather than a line before the `}`, and that is the whole reason it is a
/// type.** [`supervise`] has TWO exits: the `while` condition going false, and an early
/// `return` on `TryRecvError::Disconnected` when the capture thread's sender is gone. A
/// statement at the bottom of the function covers the first and silently misses the second —
/// and the second is the one that fires when the audio device disappears, which is exactly the
/// session end an operator most needs a line for.
struct SessionLog {
    /// When the supervisor came up, on the same clock as every other line it prints.
    opened_wall: String,
    /// The monotonic counterpart. `Instant`, never the wall clock, for the reason
    /// [`AudibleWindow`] uses one: a span computed across a system-clock step is not a span.
    opened: Instant,
}

impl SessionLog {
    fn open() -> SessionLog {
        SessionLog { opened_wall: wall_clock_utc(), opened: Instant::now() }
    }
}

impl Drop for SessionLog {
    fn drop(&mut self) {
        eprintln!(
            "[richos-voice] {} voice session CLOSED — open {} since {}; the microphone is shut",
            wall_clock_utc(),
            session_span(self.opened.elapsed()),
            self.opened_wall,
        );
    }
}

/// **Where a finished turn goes: announced once, whole, then handed to the submit thread.**
///
/// The supervisor owns one. A `Drop` guard for the reason [`SessionLog`] is one: [`supervise`]
/// has two exits, and on both of them a turn he finished speaking but whose end-of-turn quiet had
/// not yet arrived — voice switched off within 2.000 s of his last word — is still his turn and
/// still reaches the conversation, as an utterance recognized before the toggle always did.
struct TurnDesk {
    assembler: TurnAssembler,
    /// The recognizer's result for each admitted fragment, in admission order.
    words_rx: Receiver<Option<(String, bool)>>,
    submit_tx: Sender<(String, bool)>,
    observer: Arc<dyn VoiceObserver>,
    /// When the last admitted utterance ended, to report how long the turn waited after it.
    last_end: Option<Instant>,
    /// Turns handed to the submit thread so far; the submit thread numbers what it receives
    /// the same way, which is how a withdrawn turn is recognized there.
    sent: u64,
    /// The last turn handed over, while Rich has not yet made a sound answering it.
    unanswered: Option<Unanswered>,
    /// Turn numbers withdrawn before the submit thread reached them: it skips them.
    withdrawn: Arc<Mutex<HashSet<u64>>>,
    /// `rich://turn-started` events so far (`VoiceController::turn_started`).
    turns_started: Arc<AtomicU64>,
    /// The number of the last turn the submit thread is finished with: run, refused by the
    /// shell, or skipped as withdrawn ([`submit_turns`]).
    submits_done: Arc<AtomicU64>,
    /// The shell's stop for the running turn, for the withdrawal the voice-off flush makes.
    stop: WithdrawHook,
    /// The floor as the submit thread sees it ([`spoken_turn_withdrawn`]).
    floor: FloorHold,
}

/// A turn handed over that Rich has not yet begun to answer aloud.
struct Unanswered {
    seq: u64,
    turn: crate::turn::Turn,
    /// `turns_started` when it was handed over: one more means it is the turn that started.
    turns_started_at_send: u64,
}

impl TurnDesk {
    /// What the recognizer made of each fragment since the last look. `true` when any of it
    /// started a new turn of his (`TurnAssembler::recognized`).
    fn collect(&mut self) -> bool {
        let mut starts_a_turn = false;
        while let Ok(words) = self.words_rx.try_recv() {
            starts_a_turn |= self.assembler.recognized(words);
        }
        starts_a_turn
    }

    /// **His new words superseded an answer to a turn he had not finished** (review
    /// rv-20261013T184256Z, finding 4). That turn's words are carried into the one he is saying
    /// now, so the request that stands is his whole thought, and the partial one is withdrawn:
    /// the submit thread skips it if it has not reached it. Returns `Some(true)` when the partial
    /// turn is the one RUNNING right now and must be stopped, `Some(false)` when there is nothing
    /// to stop, `None` when no unanswered turn of his was waiting.
    fn withdraw_unanswered(&mut self, turn_active: bool) -> Option<bool> {
        let u = self.unanswered.take()?;
        self.assembler.carry(u.turn);
        if let Ok(mut w) = self.withdrawn.lock() {
            w.insert(u.seq);
        }
        let started = self.turns_started.load(Ordering::SeqCst);
        Some(turn_active && started == u.turns_started_at_send + 1)
    }

    /// **His new words started a turn: does the partial one he had not finished join it?** Yes
    /// while an answer to it is `outstanding`, and also while that partial turn is still waiting
    /// to start: the submit thread has not finished with it and no turn has started since it was
    /// handed over. It is waiting behind earlier work (he switched Talk to Rich on while it ran),
    /// so nothing of Rich's is outstanding yet, and without this it would run later as a request
    /// of its own (review rv-20261013T210558Z, finding 2). A turn the shell already finished
    /// with without starting it (the bug report took the words) is not waiting and does not
    /// join. Returns what [`Self::withdraw_unanswered`] returns, or `None` when nothing joins.
    fn continue_thought(&mut self, outstanding: bool, turn_active: bool) -> Option<bool> {
        let started = self.turns_started.load(Ordering::SeqCst);
        let done = self.submits_done.load(Ordering::SeqCst);
        let waiting = self
            .unanswered
            .as_ref()
            .is_some_and(|u| started == u.turns_started_at_send && done < u.seq);
        if outstanding || waiting {
            self.withdraw_unanswered(turn_active)
        } else {
            None
        }
    }

    /// Whether he has the floor, also published to the submit thread, where a spoken turn
    /// waiting to start waits for it ([`spoken_turn_withdrawn`]). The supervisor calls this
    /// wherever it publishes the floor: as his utterance starts, and at the end of each tick,
    /// after a continuation has withdrawn the turn it joins.
    fn publish_floor(&self) -> bool {
        let his = self.assembler.has_floor();
        self.floor.set(his);
        his
    }

    /// Hand over the turn if he is finished. Called once per supervisor tick, after `collect`.
    fn release(&mut self) {
        if let Some(turn) = self.assembler.take_turn() {
            self.send(turn, "his turn is over");
        }
    }

    fn send(&mut self, turn: crate::turn::Turn, why: &str) {
        let waited_ms = self.last_end.map(|t| t.elapsed().as_millis() as u64).unwrap_or(0);
        eprintln!(
            "[richos-voice] {} TURN to Rich — {why}: {} fragment(s), {:.3} s of audio, handed over {waited_ms} ms after the last one ended (end of turn = {:.3} s of silence)",
            wall_clock_utc(),
            turn.fragments,
            turn.audio_ms as f32 / 1000.0,
            crate::turn::end_of_turn_secs(),
        );
        // ONE transcript event per TURN: the window draws one user message for it, and it is the
        // message the turn's own ledger record then replaces.
        self.observer.on_voice_event(&VoiceEvent::Transcript {
            text: turn.text.clone(),
            duration_ms: turn.audio_ms,
            latency_ms: waited_ms,
            at: now_millis(),
        });
        self.sent += 1;
        if turn.at_limit {
            // Ended by the time limit (`turn::MAX_TURN_FRAMES`): it goes ahead as it is. Waiting
            // for the floor, or for his next words to carry it, would wait on the very input that
            // never stops (review rv-20261019T160520Z, finding 2).
            self.floor.let_through(self.sent);
            self.unanswered = None;
        } else {
            self.unanswered = Some(Unanswered {
                seq: self.sent,
                turn: turn.clone(),
                turns_started_at_send: self.turns_started.load(Ordering::SeqCst),
            });
        }
        if self.submit_tx.send((turn.text, turn.rich_audible)).is_err() {
            // Only when the submit thread is already gone, which is voice shutting down.
            eprintln!("[richos-voice] the turn could not be handed over: the submit thread has ended");
        }
    }
}

/// How long the toggle-off waits for ONE fragment still at the recognizer. A guard against a hung
/// decoder, not an estimate: recognition measured 0.672-0.805 s per utterance on the CEO's Mac
/// (nightly 54 log), and the longest fragment is 30.000 s of audio.
const LAST_WORDS_WAIT: Duration = Duration::from_secs(30);

impl Drop for TurnDesk {
    fn drop(&mut self) {
        let mut starts_a_turn = self.collect();
        // **HIS LAST WORDS ARE PART OF THE TURN, even when he switches voice off before they are
        // recognized** (review rv-20261013T163358Z, finding 1). Without this wait the words already
        // recognized went out as one turn and the fragment still at the recognizer went out after
        // it as another — the instruction without its condition. The recognizer is still running
        // here (`supervise` still holds `utt_tx`; parameters outlive this local), so every
        // admitted fragment comes back, and the turn goes out once, whole. The shutdown join
        // already waits on recognition, so this adds no new wait to switching voice off.
        while self.assembler.awaiting_recognition() > 0 {
            match self.words_rx.recv_timeout(LAST_WORDS_WAIT) {
                Ok(words) => {
                    starts_a_turn |= self.assembler.recognized(words);
                }
                Err(_) => {
                    eprintln!(
                        "[richos-voice] {} fragment(s) did not come back from the recognizer within {:.0} s of voice going off; the turn goes out without them",
                        self.assembler.awaiting_recognition(),
                        LAST_WORDS_WAIT.as_secs_f32()
                    );
                    break;
                }
            }
        }
        // **AND A CONTINUATION IS STILL ONE THOUGHT** (review rv-20261013T202747Z, finding 1). If
        // these last words go on from a turn Rich had not yet begun to answer aloud, that turn is
        // carried into this one and withdrawn, exactly as the supervisor does while voice is on.
        // `turn_active` is passed as true because voice-off has already reset the state machine;
        // the stop is asked only when that partial turn is the last one started, and a turn that
        // already finished answers it with "nothing running".
        if starts_a_turn {
            if let Some(stop_it) = self.withdraw_unanswered(true) {
                eprintln!(
                    "[richos-voice] {} voice is going off; his earlier turn joins his last words",
                    wall_clock_utc()
                );
                if stop_it {
                    if let Some(stop) = self.stop.lock().ok().and_then(|h| h.clone()) {
                        stop();
                    }
                }
            }
        }
        if let Some(turn) = self.assembler.flush() {
            self.send(turn, "voice is going off");
        }
        // Voice is off: nothing is his floor any more, and a turn waiting on it goes ahead.
        self.floor.set(false);
    }
}

/// The spoken-turn callback [`VoiceController::start`] is given.
type SubmitFn = Arc<dyn Fn(String, bool) + Send + Sync>;
/// Turn numbers withdrawn before they ran, shared by [`TurnDesk`] and the submit thread.
type WithdrawnTurns = Arc<Mutex<HashSet<u64>>>;
/// The turn the submit thread is inside `submit` with, where withdrawals are recorded, and
/// whether he has the floor.
type Submitting = std::cell::RefCell<Option<(u64, WithdrawnTurns, FloorHold)>>;

/// **He has the floor, as the submit thread sees it** (review rv-20261013T214648Z). Set by
/// [`TurnDesk::publish_floor`] whenever the supervisor publishes the floor, and cleared for good
/// when the desk is dropped, so nothing waiting on it outlives voice.
#[derive(Default)]
struct HisFloor {
    /// He has the floor, and how many spoken turns are waiting for it.
    held: Mutex<(bool, usize)>,
    changed: std::sync::Condvar,
    /// The last turn number ended by the time limit (`turn::MAX_TURN_FRAMES`): it and those
    /// before it go ahead without waiting for the floor ([`Self::let_through`]).
    limit_seq: AtomicU64,
}

type FloorHold = Arc<HisFloor>;

impl HisFloor {
    fn set(&self, his: bool) {
        let mut held = self.held.lock().unwrap_or_else(|p| p.into_inner());
        if held.0 != his {
            held.0 = his;
            self.changed.notify_all();
        }
    }

    /// Turn `seq` was ended by the time limit: it does not wait for the floor. An input that
    /// holds the floor for ever is exactly the input that limit exists for.
    fn let_through(&self, seq: u64) {
        let _held = self.held.lock().unwrap_or_else(|p| p.into_inner());
        self.limit_seq.fetch_max(seq, Ordering::SeqCst);
        self.changed.notify_all();
    }

    /// Returns once he does not have the floor, or at once for a turn ended by the time limit.
    /// `true` when it had to wait.
    fn wait_until_free(&self, seq: u64) -> bool {
        let through = || seq <= self.limit_seq.load(Ordering::SeqCst);
        let mut held = self.held.lock().unwrap_or_else(|p| p.into_inner());
        let waited = held.0 && !through();
        held.1 += 1;
        self.changed.notify_all();
        while held.0 && !through() {
            held = self.changed.wait(held).unwrap_or_else(|p| p.into_inner());
        }
        held.1 -= 1;
        waited
    }

    /// Blocks until a spoken turn is waiting for him (`Ok`), or `ran` says one started instead
    /// (`Err` with its words). The fact a test waits on, rather than a clock: a test callback
    /// that runs a turn calls [`Self::nudge`] after sending on `ran`, so neither fact is missed.
    #[cfg(test)]
    fn turn_waits_or_ran(&self, ran: &Receiver<String>) -> Result<(), String> {
        let mut held = self.held.lock().unwrap_or_else(|p| p.into_inner());
        loop {
            if let Ok(text) = ran.try_recv() {
                return Err(text);
            }
            if held.1 > 0 {
                return Ok(());
            }
            held = self.changed.wait(held).unwrap_or_else(|p| p.into_inner());
        }
    }

    /// Wakes [`Self::turn_waits_or_ran`]. Takes the lock, so the wakeup cannot fall between its
    /// look at `ran` and its wait.
    #[cfg(test)]
    fn nudge(&self) {
        let _held = self.held.lock().unwrap_or_else(|p| p.into_inner());
        self.changed.notify_all();
    }

    #[cfg(test)]
    fn turns_waiting(&self) -> usize {
        self.held.lock().unwrap_or_else(|p| p.into_inner()).1
    }
}

thread_local! {
    static SUBMITTING: Submitting = const { std::cell::RefCell::new(None) };
}

/// **The submit thread: one turn at a time, in order.** Each turn is numbered the way
/// [`TurnDesk`] numbers what it hands over, so a withdrawn one is recognized here and skipped.
/// One that is withdrawn after `submit` was called (it was waiting inside it for earlier work)
/// is answered by [`spoken_turn_withdrawn`].
fn submit_turns(
    submit_rx: Receiver<(String, bool)>,
    withdrawn: WithdrawnTurns,
    floor: FloorHold,
    done: Arc<AtomicU64>,
    submit: SubmitFn,
) {
    let mut seq = 0u64;
    while let Ok((text, rich_audible)) = submit_rx.recv() {
        seq += 1;
        if withdrawn.lock().map(|w| w.contains(&seq)).unwrap_or(false) {
            // He went on before it started: his whole thought follows as one turn.
            eprintln!("[richos-voice] {} turn {seq} withdrawn before it started: his next turn carries its words", wall_clock_utc());
        } else {
            SUBMITTING.with(|s| *s.borrow_mut() = Some((seq, withdrawn.clone(), floor.clone())));
            submit(text, rich_audible);
            SUBMITTING.with(|s| *s.borrow_mut() = None);
        }
        done.store(seq, Ordering::SeqCst);
    }
}

/// **Was the spoken turn this callback is submitting withdrawn while it waited?** For the
/// `submit` callback given to [`VoiceController::start`], to ask once it holds what it waited
/// for (the shell's spine) and before the turn starts. Talk to Rich switched on while earlier
/// work runs leaves a spoken turn waiting there for the whole of that work; if he goes on in the
/// meantime his whole thought is the next turn, and this one must not run as well (review
/// rv-20261013T210558Z, finding 2). `false` on any other thread.
///
/// **AND IT WAITS WHILE HE HAS THE FLOOR** (review rv-20261013T214648Z). His resumed speech
/// claims the floor the moment it starts, but it withdraws the waiting turn only once its words
/// are recognized, seconds later. Earlier work that ends in between would otherwise let the
/// partial request run on its own. So the answer waits until his open fragment is resolved: its
/// words withdrew this turn (`true`), or it was a cough or refused and the floor is back (`false`,
/// and the turn runs unchanged). Voice going off clears the floor ([`TurnDesk`]'s drop).
pub fn spoken_turn_withdrawn() -> bool {
    SUBMITTING.with(|s| {
        let s = s.borrow();
        let Some((seq, w, floor)) = s.as_ref() else { return false };
        if floor.wait_until_free(*seq) {
            eprintln!(
                "[richos-voice] {} turn {seq} waited before it started: he was speaking again",
                wall_clock_utc()
            );
        }
        w.lock().map(|w| w.contains(seq)).unwrap_or(false)
    })
}

/// The shell's stop for the running turn, installed by [`VoiceController::on_withdraw`].
type WithdrawHook = Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>;

/// What the supervisor needs to withdraw a turn he had not finished (finding 4).
struct Withdrawal {
    withdrawn: Arc<Mutex<HashSet<u64>>>,
    turns_started: Arc<AtomicU64>,
    submits_done: Arc<AtomicU64>,
    hook: WithdrawHook,
    floor: FloorHold,
}

/// **The supervisor's half of the floor rule** (`turn.rs`): the shared [`AnswerGate`] and the
/// channel its held sentences go out on when he gives the floor back.
struct Floor {
    gate: Arc<Mutex<AnswerGate<SpeakMsg>>>,
    speak_tx: Sender<SpeakMsg>,
    /// How many held sentences were last reported, so the log says HELD once per change.
    held_logged: usize,
}

impl Floor {
    fn held(&self) -> usize {
        self.gate.lock().map(|g| g.held()).unwrap_or(0)
    }

    fn publish(&mut self, shared: &Shared, his_floor: bool) {
        shared.his_floor.store(his_floor, Ordering::Relaxed);
        let held = self.held();
        if held > self.held_logged {
            eprintln!(
                "[richos-voice] {} answer HELD — he has the floor; {held} sentence(s) of Rich's waiting",
                wall_clock_utc()
            );
        }
        self.held_logged = held;
    }

    fn release(&mut self, shared: &Shared, his_floor: bool) {
        let ready = match self.gate.lock() {
            Ok(mut gate) => gate.release_marked(his_floor),
            Err(_) => Vec::new(),
        };
        if ready.is_empty() {
            return;
        }
        eprintln!(
            "[richos-voice] {} answer RELEASED — he gave the floor back; {} held sentence(s) to the speaker",
            wall_clock_utc(),
            ready.len()
        );
        // An answer's sentence from a generation that is over is not said. An announcement held
        // across that answer belongs to no generation: it is said now, in the current one.
        let current = shared.generation.load(Ordering::Relaxed);
        for (mut msg, announcement) in ready {
            if announcement {
                msg.generation = current;
            } else if msg.generation != current {
                continue;
            }
            dispatch(shared, &self.speak_tx, msg);
        }
        self.held_logged = 0;
    }

    /// **His words just started a new turn** (step 1b of [`supervise`]). An answer still unsaid
    /// (the turn is running, sentences of Rich's answer are held for him, or one is with the
    /// speaker) was to a thought he had not finished: its generation ends, it is superseded, and
    /// what is already queued at the device is cut. An announcement is not an answer: held or
    /// with the speaker it does not count here (review rv-20261015T131103Z-6b6f899d, finding 1),
    /// and if the cut takes it before he heard it, it is held again and said once the floor is
    /// free (review rv-20261015T132600Z-fdc3dafb, finding 1). Returns whether an answer was
    /// outstanding, and the samples cut.
    fn his_new_turn(&mut self, shared: &Shared, turn_active: bool, out: &dyn SpeechQueue) -> (bool, usize) {
        let held_answer = self.gate.lock().map(|g| g.held_answer()).unwrap_or(0);
        let outstanding = turn_active
            || held_answer > 0
            || shared.pending_answer.load(Ordering::Relaxed) > 0;
        if outstanding {
            shared.generation.fetch_add(1, Ordering::Relaxed);
            self.supersede("he went on talking before Rich had said it");
            let cut = out.stop_now();
            reclaim_unheard(shared, &self.gate, out.played(), false);
            return (true, cut);
        }
        (false, 0)
    }

    /// **He talked over Rich** (`CapMsg::BargeIn`): cut the device queue, end the generation,
    /// supersede the answer. An announcement he had not heard yet, even one sounding as he
    /// started, is held again and said after him: his voice is not a request to drop it. Returns
    /// the samples dropped.
    fn barge_in(&mut self, shared: &Shared, out: &dyn SpeechQueue) -> usize {
        let dropped = out.stop_now();
        shared.generation.fetch_add(1, Ordering::Relaxed);
        // He took the floor back from this answer: nothing more of it is said, including the
        // sentences the model is still writing. Ends when the next turn starts.
        self.supersede("he interrupted it");
        reclaim_unheard(shared, &self.gate, out.played(), false);
        dropped
    }

    fn supersede(&mut self, why: &str) {
        let (was_muted, dropped) = match self.gate.lock() {
            Ok(mut g) => (g.muted(), g.supersede()),
            Err(_) => (true, 0),
        };
        // Announcements still held stay counted, so HELD is not logged again for them.
        self.held_logged = self.held();
        // Said once per answer: a tap-to-stop reaches here twice (the button, then the frame it
        // causes), and a second line for the same answer says nothing new.
        if !was_muted || dropped > 0 {
            eprintln!(
                "[richos-voice] {} answer SUPERSEDED — {why}; {dropped} held sentence(s) dropped, and the rest of that answer stays on screen only until the next turn starts",
                wall_clock_utc()
            );
        }
    }
}

/// A voice session's length as `M:SS.mmm`, for the closing line.
///
/// Pure, and separate from the guard, so the arithmetic is testable without a thread: the
/// whole point of printing a duration is that somebody reads it against two timestamps, and a
/// duration that disagrees with them is worse than no duration.
pub fn session_span(d: Duration) -> String {
    let ms = d.as_millis();
    format!("{}:{:02}.{:03}", ms / 60_000, (ms / 1000) % 60, ms % 1000)
}

/// The supervisor loop: owns the state machine, emits every UI event, dispatches work.
#[allow(clippy::too_many_arguments)]
fn supervise(
    shared: Arc<Shared>,
    machine: Arc<Mutex<VoiceStateMachine>>,
    playout: Arc<Playout>,
    observer: Arc<dyn VoiceObserver>,
    cap_rx: Receiver<CapMsg>,
    utt_tx: Sender<Box<AdmittedUtterance>>,
    words_rx: Receiver<Option<(String, bool)>>,
    submit_tx: Sender<(String, bool)>,
    mut floor: Floor,
    withdrawal: Withdrawal,
    shared_aec: Arc<AecShared>,
    input_latency: crate::capture::InputLatency,
    notices: Arc<Mutex<RefusalNotices>>,
) {
    // Held for the whole function so that EVERY return from it prints the closing line —
    // see `SessionLog`.
    let _session = SessionLog::open();
    // His turn, assembled from its fragments (`turn.rs`). Declared AFTER `_session` so it is
    // dropped first: the turn a toggle-off flushes is logged before the session's closing line.
    let mut turns = TurnDesk {
        assembler: TurnAssembler::new(),
        words_rx,
        submit_tx,
        observer: observer.clone(),
        last_end: None,
        sent: 0,
        unanswered: None,
        withdrawn: withdrawal.withdrawn.clone(),
        turns_started: withdrawal.turns_started.clone(),
        submits_done: withdrawal.submits_done.clone(),
        stop: withdrawal.hook.clone(),
        floor: withdrawal.floor.clone(),
    };
    // Rich's first sound for an answer: from then on, what he says is a new turn, not the rest of
    // the one Rich is answering.
    let mut was_playing = false;
    let mut half_duplex_line = HalfDuplexDelivery::default();
    let mut last_state = VoiceState::Off;
    // Rich's audible window, and the monotonic clock that drives it. `Instant`, never the wall
    // clock: a window that closes early because the system clock stepped is the defect again.
    let mut audible = AudibleWindow::new();
    let started_at = Instant::now();
    let mut last_level_emit = Instant::now() - LEVEL_EMIT_EVERY;
    let mut last_level = -1.0f32;
    let mut no_audio = false;
    let mut no_audio_changed = false;

    while shared.running.load(Ordering::SeqCst) {
        // Read BEFORE the drain: if the capture callback had finished by now, everything it sent
        // was sent before that, so this drain takes it all (`VoiceController::drop`).
        let capture_had_finished = shared.capture_finished.load(Ordering::SeqCst);
        // 1. Drain the audio thread's messages.
        loop {
            match cap_rx.try_recv() {
                Ok(CapMsg::Started { tainted, rich_audible }) => {
                    // **PRINTED WHETHER TRUE OR FALSE** — Ray's walk could not tell an admitted
                    // utterance from a discarded one in the log, because only discards printed.
                    // An invariant you can only ever see violated is not one you can audit.
                    eprintln!(
                        "[richos-voice] {} utterance START tainted={tainted} heard-rich={rich_audible} (Rich audible={})",
                        wall_clock_utc(),
                        shared.speaking.load(Ordering::Relaxed)
                    );
                    if !tainted {
                        if let Ok(mut m) = machine.lock() {
                            m.utterance_started();
                        }
                        // Begun while Rich was silent: the CEO has the floor from this frame.
                        turns.assembler.his_utterance_started();
                    } else {
                        turns.assembler.utterance_started();
                    }
                    floor.publish(&shared, turns.publish_floor());
                }
                Ok(CapMsg::Utterance(u)) => {
                    eprintln!(
                        "[richos-voice] {} utterance END ADMITTED — {:.3} s, heard-rich={}, on to the recognizer (Rich audible={})",
                        wall_clock_utc(),
                        u.utterance.samples.len() as f32 / crate::vad::SAMPLE_RATE as f32,
                        u.rich_audible,
                        shared.speaking.load(Ordering::Relaxed)
                    );
                    if let Ok(mut m) = machine.lock() {
                        m.utterance_ended();
                    }
                    // Recorded BEFORE the recognizer can have it, so its words can never arrive
                    // ahead of the admission they belong to.
                    turns.assembler.utterance_admitted(
                        (u.utterance.duration_secs() * 1000.0).round() as u64,
                    );
                    turns.last_end = Some(Instant::now());
                    let _ = utt_tx.send(u);
                }
                Ok(CapMsg::Discarded { tainted }) => {
                    eprintln!(
                        "[richos-voice] {} utterance END DISCARDED tainted={tainted} (Rich audible={})",
                        wall_clock_utc(),
                        shared.speaking.load(Ordering::Relaxed)
                    );
                    if let Ok(mut m) = machine.lock() {
                        m.utterance_ended();
                    }
                    turns.assembler.utterance_discarded();
                    if tainted {
                        // **THE OLD TEXT HERE SAID `(no AEC)` AND THAT WAS FALSE.** The
                        // canceller is running on this exact frame — `CaptureBrain::push_frame`
                        // subtracts the echo before the VAD, the endpointer or the recorder
                        // see anything. What is true is narrower and it is the fact that
                        // decides the CEO's experience: the canceller has not yet MEASURED
                        // its residual low enough, for long enough, to be trusted, so the
                        // fallback taint rule is the one in force.
                        //
                        // The difference is not pedantry. Audit-3 read `(no AEC)` off the
                        // running app, concluded the feature was absent, and filed the boot
                        // line's `aec=PBFDAF` as a contradiction. Both lines were describing
                        // the same working canceller in two different states, and one of them
                        // was lying about which.
                        //
                        // **AND THE REPLACEMENT OVER-PROMISED IN ITS TURN, which is why this
                        // line changed again on 2026-09-17.** It said the canceller "has not
                        // proven itself YET" and that barge-in needed 5.008 s "UNTIL IT DOES".
                        // Both words promise a convergence that, on the CEO's own hardware,
                        // never arrives: measured that day across five live runs, the
                        // steady-state ERLE is −0.5 … +0.7 dB and the microphone reads −42 … −46
                        // dBFS while Rich speaks, against the −52.04 dBFS the threshold demands
                        // (`docs/verification/2026-09-17-aec-erle-on-the-ceo-rig.md`). So the
                        // line now reports the GAP — measured ERLE, measured residual, the
                        // threshold it has to reach — and lets the reader see how far short it
                        // is, instead of asserting that it is nearly there.
                        // **AND IT SAYS WHEN IT MEASURED NOTHING, rather than printing a zero.**
                        // Defect 3 of the candidate-.6 walk: all four discards printed
                        // `erle=0.0 dB, residual 0.0 dBFS`, and 0.0 dBFS is FULL SCALE. Both were
                        // sentinels — `AecMetrics::erle_measured` and `::leak_measured` name the
                        // exact conditions — but the line could not say so, because both numbers
                        // arrived as bare `f32`. Candidate .5's identical line carried `-2.9 dB`
                        // and `-45.8 dBFS`, real readings, so a reader had no way to tell the two
                        // cases apart by form. The window widening is what changed which case
                        // shows up: these discards now happen in synthesis gaps and past the end
                        // of playout, where the far end is inactive and the accumulators say
                        // nothing at all.
                        //
                        // **AND THE BARGE-IN CLAUSE CHANGED AGAIN ON 2026-09-18, because it was
                        // still quoting 5.008 s after that stopped being the rule.** The line
                        // told the CEO he needed 5.008 s of talking over Rich; he tried, five
                        // separate times at -18.1 dBFS against a -46.7 dBFS echo, and was
                        // discarded every time, because nobody produces 5 s of UNBROKEN speech
                        // (his longest run was 66 frames = 1.056 s). Barge-in now needs 15 of any
                        // 25 frames of NEAR-END speech — 0.240 s of evidence inside 0.400 s — and
                        // that is what the line says. The confidence figures before the semicolon
                        // stay, because the TAINT rule still rests on `confident()`: this notice
                        // is printed on a discard, and a discard is the taint rule's decision.
                        // What changed is only what would have rescued it.
                        let erle = measured_or_not(shared_aec.erle_db(), "dB");
                        let leak = measured_or_not(shared_aec.leak_dbfs(), "dBFS");
                        eprintln!(
                            "[richos-voice] discarded audio captured while Rich was speaking — the echo canceller cannot vouch for this audio (erle={erle}, residual {leak} vs the {:.1} dBFS it must hold for {:.3} s after {:.3} s of Rich speaking); barge-in needs {} of {} frames ({:.3} s of near-end speech inside {:.3} s) to rescue it, and on this frame it did not have them",
                            dbfs(crate::aec::CONFIDENT_LEAK_RMS),
                            crate::aec::blocks_to_secs(crate::aec::CONFIDENCE_HOLD_BLOCKS),
                            crate::aec::blocks_to_secs(crate::aec::CONFIDENCE_WARMUP_BLOCKS),
                            crate::bargein::AEC_BARGE_IN_REQUIRED_FRAMES,
                            crate::bargein::AEC_BARGE_IN_WINDOW_FRAMES,
                            crate::vad::frames_to_secs(crate::bargein::AEC_BARGE_IN_REQUIRED_FRAMES),
                            crate::vad::frames_to_secs(crate::bargein::AEC_BARGE_IN_WINDOW_FRAMES),
                        );
                        // AND HE IS TOLD, once per voice session — see `RefusalNotices`.
                        // What he is told asserts nothing about whether he spoke, because on
                        // this path the app cannot know: `tainted` requires `!confident`, and
                        // `!confident` is the canceller saying it cannot separate the voices.
                        // ONE BUDGET, SHARED WITH THE RECOGNIZER THREAD. This is the
                        // strongest line in the family, so it stands and the two
                        // single-refusal lines cannot add themselves on top of it — which is
                        // precisely the three-cards-for-a-silent-man defect. See
                        // `RefusalNotices`.
                        //
                        // OWED HERE, SHOWN WHEN RICH IS NO LONGER AUDIBLE: the line ends "please
                        // say it again now", and while Rich is still audible a short reply is
                        // discarded the same way. See `HalfDuplexDelivery`.
                        if let Some(n) = notices.lock().ok().and_then(|mut b| b.half_duplex()) {
                            half_duplex_line.owe(n);
                        }
                    }
                }
                Ok(CapMsg::NoAudio { silent }) => {
                    shared.no_audio.store(silent, Ordering::Relaxed);
                    no_audio = silent;
                    no_audio_changed = true;
                    if silent {
                        eprintln!(
                            "[richos-voice] input silent: nothing above {:.2} dBFS for {:.3} s on an OPEN stream - muted mic, gain at zero, or a denied permission",
                            crate::noaudio::dbfs(crate::noaudio::SILENCE_RMS),
                            no_audio_window_secs()
                        );
                    } else {
                        eprintln!("[richos-voice] input is live again");
                    }
                }
                Ok(CapMsg::BargeIn { mid_utterance }) => {
                    let dropped = floor.barge_in(&shared, &*playout);
                    shared.speaking.store(false, Ordering::Relaxed);
                    if let Ok(mut m) = machine.lock() {
                        m.barge_in();
                        if !mid_utterance {
                            // He cut Rich off without saying anything — there is no utterance
                            // to end, so clear `hearing` now or the panel sits in it forever.
                            m.utterance_ended();
                        }
                    }
                    eprintln!(
                        "[richos-voice] barge-in: cut Rich, dropped {dropped} samples ({:.3} s of queued speech)",
                        dropped as f32 / playout.device_rate.max(1) as f32
                    );
                }
                Ok(CapMsg::TurnQuiet) => {
                    eprintln!(
                        "[richos-voice] {} end-of-turn quiet — {:.3} s of silence since his last word; {} fragment(s) held, {} at the recognizer",
                        wall_clock_utc(),
                        crate::turn::end_of_turn_secs(),
                        turns.assembler.fragments_held(),
                        turns.assembler.awaiting_recognition(),
                    );
                    turns.assembler.quiet();
                }
                Ok(CapMsg::TurnLimit) => {
                    eprintln!(
                        "[richos-voice] {} end of turn at the {:.3} s limit, without {:.3} s of silence; {} fragment(s) held, {} at the recognizer",
                        wall_clock_utc(),
                        crate::vad::frames_to_secs(crate::turn::MAX_TURN_FRAMES),
                        crate::turn::end_of_turn_secs(),
                        turns.assembler.fragments_held(),
                        turns.assembler.awaiting_recognition(),
                    );
                    turns.assembler.time_limit();
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }

        if capture_had_finished {
            shared.drained_after_finish.store(true, Ordering::SeqCst);
        }

        // 1b. What the recognizer made of each fragment, in admission order — then, if he is
        //     finished, his turn. After the capture messages, so a fragment admitted on this
        //     tick is already counted as awaiting before its words could be looked for.
        //
        //     Words of his that START a new turn while an answer is still unsaid (a turn in flight,
        //     or sentences held for him) supersede that answer: it was to a thought he had not
        //     finished. It stays on screen; the rest of it is not spoken.
        if turns.collect() {
            // Outstanding: the turn is still running, sentences are held for him in the gate, or a
            // sentence is still with the speaker (in synthesis, or synthesized and waiting for him).
            let turn_active = machine.lock().map(|m| m.turn_active()).unwrap_or(false);
            let (outstanding, cut) = floor.his_new_turn(&shared, turn_active, &*playout);
            if cut > 0 {
                eprintln!(
                    "[richos-voice] {:.3} s of the answer already queued was cut",
                    cut as f32 / playout.device_rate.max(1) as f32
                );
            }
            // ONE THOUGHT, ONE REQUEST: the turn he had not finished joins this one.
            if let Some(stop_it) = turns.continue_thought(outstanding, turn_active) {
                eprintln!(
                    "[richos-voice] {} his earlier turn joins the one he is saying now; {}",
                    wall_clock_utc(),
                    if stop_it { "it is running, so it is stopped" } else { "it is not running" }
                );
                if stop_it {
                    let hook = withdrawal.hook.lock().ok().and_then(|h| h.clone());
                    match hook {
                        Some(stop) => stop(),
                        None => eprintln!("[richos-voice] no stop is installed; the partial turn runs on, silenced"),
                    }
                }
            }
        }
        turns.release();
        // 1c. The floor. While he has it Rich's sentences wait; when he gives it back they go to
        //     the speaker in order (unless the answer was superseded, which dropped them).
        let his_floor = turns.publish_floor();
        floor.publish(&shared, his_floor);
        floor.release(&shared, his_floor);

        // 2. Reconcile playout with the state machine, and decide whether Rich is AUDIBLE.
        //
        //    **THE QUEUE IS NOT THE GROUND TRUTH FOR THAT, and this line used to say it was.**
        //    `playout.is_playing()` is `queued_samples() > 0`, which is false before the first
        //    sample of an answer, false in every gap between spoken sentences, and false while
        //    the device is still emitting the last one. See `AudibleWindow`.
        let queued = playout.is_playing();
        let owed = shared.pending_speech.load(Ordering::Relaxed);
        let hold = audible_hold_secs(playout.audible_tail_secs(), input_latency.secs());
        let now_ms = started_at.elapsed().as_millis() as u64;
        let (answer_edge, chunk_edge) = audible.observe(queued, owed > 0, hold, now_ms);
        if !audible.is_open() {
            // The answer is over: the next one has made no sound until its first sample.
            shared.answer_sounded.store(false, Ordering::Relaxed);
        }
        // Audible only once a sample of this answer has reached the device: a sentence still in
        // synthesis has made no sound, and speech that starts then is his (`turn::rich_audible`).
        let playing = rich_audible(audible.is_open(), shared.answer_sounded.load(Ordering::Relaxed));
        shared.speaking.store(playing, Ordering::Relaxed);
        if let Some(n) = half_duplex_line.due(playing, shared.takes_a_reply.load(Ordering::Relaxed)) {
            // It asks him to say it again: keep what he says next however short, and let a
            // refusal of it be said (`RefusalNotices::reply_invited`).
            shared.reply_invited.store(true, Ordering::Relaxed);
            if let Ok(mut b) = notices.lock() {
                b.reply_invited();
            }
            observer.on_voice_event(&VoiceEvent::Notice { message: n.ceo_message().to_string(), at: now_millis() });
        }
        if playing && !was_playing {
            // Rich has begun answering aloud: his next words are a new turn.
            turns.unanswered = None;
        }
        was_playing = playing;

        // **THE BOUNDARIES, WITH A WALL CLOCK.** Ray's candidate-.5 walk could not say from the
        // record whether the admitted utterance began in a gap between spoken sentences or
        // after playout had ended, because nothing here was timestamped and the chunk
        // boundaries were never printed at all. Both are printed now, in the ledger's own
        // format, so the next walk reads the answer off the log instead of inferring it.
        match chunk_edge {
            Some(Edge::Rose) => eprintln!(
                "[richos-voice] {} playout chunk START — {:.3} s queued, {owed} sentence(s) still in synthesis",
                wall_clock_utc(),
                playout.queued_secs()
            ),
            Some(Edge::Fell) => eprintln!(
                "[richos-voice] {} playout chunk END — queue empty, {owed} sentence(s) still in synthesis",
                wall_clock_utc()
            ),
            None => {}
        }
        match answer_edge {
            Some(Edge::Rose) => eprintln!(
                "[richos-voice] {} ANSWER AUDIBLE — half-duplex window OPEN (hold after the last sample {:.0} ms = out {:.1} + in {:.1} + slicer {:.1})",
                wall_clock_utc(),
                hold * 1000.0,
                playout.audible_tail_secs() * 1000.0,
                input_latency.secs() * 1000.0,
                frames_to_secs(1) * 1000.0
            ),
            Some(Edge::Fell) => eprintln!(
                "[richos-voice] {} answer over — half-duplex window CLOSED, the microphone is the CEO's again",
                wall_clock_utc()
            ),
            None => {}
        }

        if let Ok(mut m) = machine.lock() {
            if playing {
                m.playout_started();
            } else {
                m.playout_drained();
            }
        }

        // 3. Emit. State changes always; level at most 10 Hz and only when it moved.
        let state = machine.lock().map(|m| m.state()).unwrap_or(VoiceState::Off);
        let armed = machine.lock().map(|m| m.barge_in_armed()).unwrap_or(false);
        let level = if state.mic_is_hot() { shared.level() } else { 0.0 };
        let state_changed = state != last_state;
        let level_due = last_level_emit.elapsed() >= LEVEL_EMIT_EVERY && (level - last_level).abs() > 0.02;
        // A dead input holds the level at 0.0 and the state at Listening, so NEITHER of the
        // two existing emit triggers would ever fire - the warning would be computed and
        // never delivered. The transition is its own trigger.
        if state_changed || level_due || no_audio_changed {
            observer.on_voice_event(&VoiceEvent::State {
                state,
                level,
                barge_in_armed: armed,
                no_audio,
                at: now_millis(),
            });
            last_state = state;
            last_level = level;
            last_level_emit = Instant::now();
            no_audio_changed = false;
        }

        std::thread::sleep(TICK);
    }

    // Voice mode is off: say so once, unambiguously. A stale "listening" left on screen
    // would be a lie about a hot mic.
    observer.on_voice_event(&VoiceEvent::State {
        state: VoiceState::Off,
        level: 0.0,
        barge_in_armed: false,
        no_audio: false,
        at: now_millis(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::endpoint::EndReason;
    use crate::voiced::fixtures::{framed, hiss, silence, steady_tone, synthetic_voice};

    /// Wrap a buffer as a finished utterance. `speech_frames`/`total_frames` are set to what
    /// the endpointer WOULD have counted — deliberately generous, because the point of these
    /// tests is that the desk does not trust that count. It was the count being wrong that
    /// put words in the CEO's mouth.
    fn utterance(samples: Vec<f32>) -> AdmittedUtterance {
        admitted(samples, false)
    }

    /// The same, with the provenance the capture path would have attached. `rich_audible: true`
    /// is an utterance the app ADMITTED that was nevertheless recorded while Rich was audible —
    /// a barge-in, or a confident canceller. See [`AdmittedUtterance::rich_audible`].
    fn admitted(samples: Vec<f32>, rich_audible: bool) -> AdmittedUtterance {
        let total = (samples.len() / crate::vad::VAD_FRAME_SAMPLES) as u32;
        AdmittedUtterance {
            utterance: Box::new(Utterance {
                samples,
                reason: EndReason::Silence,
                speech_frames: total,
                total_frames: total,
                continues_cut: false,
            }),
            rich_audible,
        }
    }

    fn messages(rec: &Recorder) -> Vec<String> {
        rec.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                VoiceEvent::Error { message, .. } => Some(message.clone()),
                _ => None,
            })
            .collect()
    }

    /// `rich://voice-notice` only. A SEPARATE helper from `messages()` on purpose: the whole
    /// point of audit-3 §4 #4's fix is which CHANNEL the sentence goes down, and a helper
    /// that merged the two would pass whether the line was a notice or an error.
    fn notices(rec: &Recorder) -> Vec<String> {
        rec.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                VoiceEvent::Notice { message, .. } => Some(message.clone()),
                _ => None,
            })
            .collect()
    }

    fn transcripts(rec: &Recorder) -> Vec<String> {
        rec.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                VoiceEvent::Transcript { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    /// INVARIANT — **THE ONE THIS WHOLE CHANGE EXISTS FOR.** Audio that carried no voice
    /// never reaches the recognizer, so there is no transcript to submit, however plausible
    /// whisper would have made it sound. The transcriber PANICS if it is called: this is a
    /// proof of ordering, not an assertion about it.
    #[test]
    fn audio_that_carried_no_voice_never_reaches_whisper_and_never_becomes_a_turn() {
        for (what, audio) in [
            ("digital silence", silence(3.0)),
            ("a quiet room at the VAD's own floor", hiss(3.0, -46.0, 7)),
            ("a room 18 dB above it", hiss(3.0, -28.0, 9)),
            ("a fan", steady_tone(3.0, -40.0)),
        ] {
            let rec = Recorder::default();
            let mut desk = RecognizerDesk::new();
            desk.handle(
                &utterance(audio),
                &rec,
                |_| panic!("{what} reached whisper — the gate is not in front of it"),
                |t, _| panic!("{what} was submitted as the CEO's message: {t:?}"),
            );
            assert!(transcripts(&rec).is_empty(), "{what} produced a transcript event");
            assert_eq!(
                messages(&rec),
                vec![VoiceNotice::HeardNoVoice.ceo_message().to_string()],
                "{what} was dropped silently — a silent drop is its own kind of lying"
            );
        }
    }

    /// INVARIANT: a voice DOES get through, all the way to `submit`. Without this the suite
    /// above would pass by refusing everything, which is the false green this repository has
    /// been bitten by repeatedly.
    #[test]
    fn a_voice_reaches_whisper_and_is_submitted_as_a_turn() {
        let rec = Recorder::default();
        let mut desk = RecognizerDesk::new();
        let sent = Mutex::new(Vec::<String>::new());
        let saw_whisper = std::sync::atomic::AtomicBool::new(false);
        desk.handle(
            &utterance(framed(synthetic_voice(1.5, 190.0, 130.0, -26.0), -55.0, 3)),
            &rec,
            |samples| {
                saw_whisper.store(true, Ordering::Relaxed);
                assert!(!samples.is_empty(), "whisper was handed nothing");
                Ok(("Renegotiate Acme and get me the number by Thursday.".to_string(), 470))
            },
            |t, _| sent.lock().unwrap().push(t),
        );
        assert!(saw_whisper.load(Ordering::Relaxed), "a real voice never reached whisper");
        assert_eq!(
            sent.into_inner().unwrap(),
            vec!["Renegotiate Acme and get me the number by Thursday.".to_string()]
        );
        // A FRAGMENT, not yet his turn: the thread is drawn from the turn's own transcript event,
        // once, by `TurnDesk` — `one_thought_with_a_pause_is_one_message_and_one_turn` below.
        assert!(transcripts(&rec).is_empty(), "a fragment was announced as a message of its own");
        assert!(messages(&rec).is_empty(), "a working turn raised a notice");
    }

    /// INVARIANT — THE 2026-10-13 DEFECT, at the desk where a turn leaves voice. Two recognized
    /// fragments with a pause between them reach the window as ONE message and the spine as ONE
    /// turn, and only after the end-of-turn quiet. Before the fix each fragment was announced and
    /// submitted on its own the moment whisper returned it, which is how his first two words became
    /// a turn while he was still saying the sentence after them (nightly 54).
    #[test]
    fn one_thought_with_a_pause_is_one_message_and_one_turn() {
        let rec = Arc::new(Recorder::default());
        let (words_tx, words_rx) = channel();
        let (submit_tx, submit_rx) = channel();
        let mut desk = TurnDesk {
            assembler: TurnAssembler::new(),
            words_rx,
            submit_tx,
            observer: rec.clone(),
            last_end: None,
            sent: 0,
            unanswered: None,
            withdrawn: Default::default(),
            turns_started: Default::default(),
            submits_done: Default::default(),
            stop: Default::default(),
            floor: Default::default(),
        };
        desk.assembler.utterance_started();
        desk.assembler.utterance_admitted(2512);
        words_tx.send(Some(("So here is the first part".to_string(), false))).unwrap();
        desk.collect();
        desk.release();
        desk.assembler.utterance_started();
        desk.collect();
        desk.release();
        desk.assembler.utterance_admitted(2640);
        words_tx.send(Some(("and here is the rest of it.".to_string(), false))).unwrap();
        desk.collect();
        desk.release();
        assert!(transcripts(&rec).is_empty(), "a fragment reached the window before he was finished");
        assert!(submit_rx.try_recv().is_err(), "a fragment became a turn before he was finished");

        desk.assembler.quiet();
        desk.collect();
        desk.release();
        let whole = "So here is the first part and here is the rest of it.".to_string();
        assert_eq!(transcripts(&rec), vec![whole.clone()]);
        assert_eq!(submit_rx.try_recv().unwrap(), (whole, false));
        assert!(submit_rx.try_recv().is_err(), "one thought became more than one turn");
    }

    /// INVARIANT — review rv-20261013T183507Z / rv-20261013T184256Z finding 1. Voice switched off
    /// while he is still talking: the open recording is closed and ADMITTED, so it reaches the
    /// recognizer and the turn. Before, it was destroyed with the capture stream.
    #[test]
    fn voice_off_while_he_is_talking_keeps_what_he_had_said() {
        let mut brain = CaptureBrain::new();
        let mut audio = hiss(1.0, -55.0, 41);
        audio.extend(synthetic_voice(1.0, 190.0, 130.0, -26.0));
        let (mut started, mut admitted) = (0, 0);
        for f in audio.chunks_exact(crate::vad::VAD_FRAME_SAMPLES) {
            for m in brain.push_frame(f, false, false) {
                match m {
                    CapMsg::Started { .. } => started += 1,
                    CapMsg::Utterance(_) => admitted += 1,
                    _ => {}
                }
            }
        }
        assert_eq!((started, admitted), (1, 0), "the replay did not leave him mid-sentence");
        let out = brain.finish();
        assert!(
            matches!(out.as_slice(), [CapMsg::Utterance(u)] if u.utterance.reason == EndReason::Stopped),
            "voice off lost what he was saying: {out:?}"
        );
        assert!(brain.finish().is_empty(), "a second finish made something out of nothing");
    }

    /// INVARIANT — review rv-20261013T163358Z finding 1. Voice switched off while his LAST fragment
    /// is still at the recognizer: the turn waits for it and goes out ONCE, whole. Before the fix
    /// the first fragment went out alone and the second followed as a turn of its own.
    #[test]
    fn voice_off_while_his_last_fragment_is_at_the_recognizer_still_sends_one_turn() {
        let rec = Arc::new(Recorder::default());
        let (words_tx, words_rx) = channel();
        let (submit_tx, submit_rx) = channel();
        let late = words_tx.clone();
        {
            let mut desk = TurnDesk {
                assembler: TurnAssembler::new(),
                words_rx,
                submit_tx,
                observer: rec.clone(),
                last_end: None,
                sent: 0,
                unanswered: None,
                withdrawn: Default::default(),
                turns_started: Default::default(),
                submits_done: Default::default(),
                stop: Default::default(),
                floor: Default::default(),
            };
            desk.assembler.his_utterance_started();
            desk.assembler.utterance_admitted(1200);
            words_tx.send(Some(("Send the draft".to_string(), false))).unwrap();
            desk.assembler.his_utterance_started();
            desk.assembler.utterance_admitted(1400);
            // The recognizer finishes the second fragment only after voice has gone off.
            std::thread::spawn(move || {
                // load-bound: the pass verdict waits for the words themselves (up to LAST_WORDS_WAIT, 30 s); this delay only puts them after the desk's own collect, so it is what makes the old code fail, and a slow host can only make the pass slower.
                std::thread::sleep(Duration::from_millis(150));
                late.send(Some(("after I approve it.".to_string(), false))).unwrap();
            });
        }
        assert_eq!(
            submit_rx.try_recv().unwrap(),
            ("Send the draft after I approve it.".to_string(), false),
            "his last words were split off his turn"
        );
        assert!(submit_rx.try_recv().is_err(), "one thought became more than one turn");
        assert_eq!(transcripts(&rec).len(), 1);
    }

    /// INVARIANT — review rv-20261013T184256Z finding 4. He paused for longer than the window,
    /// so "Send the draft" was handed over; he went on before Rich said a word of the answer.
    /// The request that stands is the whole thought, and the partial one is withdrawn: stopped
    /// when it is the turn running, skipped when the submit thread has not reached it.
    #[test]
    fn a_continued_thought_is_one_request_and_the_partial_one_is_withdrawn() {
        let rec = Arc::new(Recorder::default());
        let (words_tx, words_rx) = channel();
        let (submit_tx, submit_rx) = channel();
        let mut desk = TurnDesk {
            assembler: TurnAssembler::new(),
            words_rx,
            submit_tx,
            observer: rec.clone(),
            last_end: None,
            sent: 0,
            unanswered: None,
            withdrawn: Default::default(),
            turns_started: Default::default(),
            submits_done: Default::default(),
            stop: Default::default(),
            floor: Default::default(),
        };
        desk.assembler.his_utterance_started();
        desk.assembler.utterance_admitted(1500);
        words_tx.send(Some(("Send the draft".to_string(), false))).unwrap();
        desk.collect();
        desk.assembler.quiet();
        desk.release();
        assert_eq!(submit_rx.try_recv().unwrap().0, "Send the draft");
        // The spine starts it (`rich://turn-started`).
        desk.turns_started.fetch_add(1, Ordering::SeqCst);
        // He goes on before Rich has said anything.
        desk.assembler.his_utterance_started();
        desk.assembler.utterance_admitted(1200);
        words_tx.send(Some(("after I approve it.".to_string(), false))).unwrap();
        assert!(desk.collect(), "his continuation did not start a new turn");
        assert_eq!(desk.withdraw_unanswered(true), Some(true), "the running partial turn was not stopped");
        assert!(desk.withdrawn.lock().unwrap().contains(&1));
        desk.assembler.quiet();
        desk.release();
        assert_eq!(
            submit_rx.try_recv().unwrap().0,
            "Send the draft after I approve it.",
            "the request that stands is not his whole thought"
        );
        // Nothing to withdraw twice, and a turn not yet started is skipped rather than stopped.
        assert_eq!(desk.withdraw_unanswered(true), Some(false), "a turn that never started was stopped");
    }

    /// INVARIANT — review rv-20261013T210558Z, finding 2. He switches Talk to Rich on while
    /// earlier work holds the spine. "Send the draft" is handed over after a long pause and the
    /// submit callback waits for the spine; he goes on with "after I approve it." Nothing of
    /// Rich's is outstanding (no voice turn has started), yet the partial turn is withdrawn,
    /// and once the earlier work lets go of the spine the callback (as `main.rs` does) asks
    /// whether its turn was withdrawn and drops it. The only request that runs is his whole
    /// thought. A turn the shell already finished with (the bug report took it) is not carried.
    #[test]
    fn a_partial_turn_waiting_behind_earlier_work_never_runs_once_withdrawn() {
        let (words_tx, words_rx) = channel();
        let (submit_tx, submit_rx) = channel();
        let withdrawn: Arc<Mutex<HashSet<u64>>> = Default::default();
        let done: Arc<AtomicU64> = Default::default();
        let mut desk = TurnDesk {
            assembler: TurnAssembler::new(),
            words_rx,
            submit_tx,
            observer: Arc::new(Recorder::default()),
            last_end: None,
            sent: 0,
            unanswered: None,
            withdrawn: withdrawn.clone(),
            turns_started: Default::default(),
            submits_done: done.clone(),
            stop: Default::default(),
            floor: Default::default(),
        };
        // The shell's spine, already held by the earlier work.
        let spine = Arc::new(Mutex::new(Vec::<String>::new()));
        let earlier_work = spine.lock().unwrap();
        let callback_spine = spine.clone();
        let (entered_tx, entered_rx) = channel();
        let submit: SubmitFn = Arc::new(move |text: String, _| {
            entered_tx.send(text.clone()).unwrap();
            let mut ran = callback_spine.lock().unwrap();
            if spoken_turn_withdrawn() {
                return;
            }
            ran.push(text);
        });
        let worker =
            std::thread::spawn(move || submit_turns(submit_rx, withdrawn, Default::default(), done, submit));

        desk.assembler.his_utterance_started();
        desk.assembler.utterance_admitted(1500);
        words_tx.send(Some(("Send the draft".to_string(), false))).unwrap();
        desk.collect();
        desk.assembler.quiet();
        desk.release();
        // The callback is inside, waiting for the spine.
        assert_eq!(entered_rx.recv().unwrap(), "Send the draft");

        desk.assembler.his_utterance_started();
        desk.assembler.utterance_admitted(1500);
        words_tx.send(Some(("after I approve it.".to_string(), false))).unwrap();
        assert!(desk.collect(), "his continuation did not start a new turn");
        assert_eq!(
            desk.continue_thought(false, false),
            Some(false),
            "the partial turn waiting for the spine was not withdrawn"
        );
        desk.assembler.quiet();
        desk.release();

        drop(earlier_work);
        drop(desk);
        worker.join().unwrap();
        assert_eq!(
            *spine.lock().unwrap(),
            ["Send the draft after I approve it."],
            "the withdrawn partial request ran"
        );

        // A turn the shell finished with without starting it (the bug report took the words) is
        // not waiting: what he says after it is a turn of its own, not carried into it.
        let (words_tx, words_rx) = channel();
        let mut desk = TurnDesk {
            assembler: TurnAssembler::new(),
            words_rx,
            submit_tx: channel().0,
            observer: Arc::new(Recorder::default()),
            last_end: None,
            sent: 0,
            unanswered: None,
            withdrawn: Default::default(),
            turns_started: Default::default(),
            submits_done: Arc::new(AtomicU64::new(1)),
            stop: Default::default(),
            floor: Default::default(),
        };
        desk.assembler.his_utterance_started();
        desk.assembler.utterance_admitted(1500);
        words_tx.send(Some(("It froze when I opened settings".to_string(), false))).unwrap();
        desk.collect();
        desk.assembler.quiet();
        desk.release();
        desk.assembler.his_utterance_started();
        desk.assembler.utterance_admitted(1500);
        words_tx.send(Some(("Send the draft".to_string(), false))).unwrap();
        assert!(desk.collect());
        assert_eq!(desk.continue_thought(false, false), None, "words the shell already handled were carried");
    }

    /// INVARIANT — review rv-20261019T160520Z, finding 2. A turn ended by the time limit
    /// (`turn::MAX_TURN_FRAMES`) reaches the model through the real submit thread and
    /// [`spoken_turn_withdrawn`] even though the input that never stops has already opened his
    /// next utterance and holds the floor, and his next words do not withdraw it to carry it.
    /// Before this the submit waited on that floor, so the bound ended nothing.
    #[test]
    fn a_turn_ended_by_the_time_limit_runs_while_the_floor_is_still_held() {
        let (words_tx, words_rx) = channel();
        let (submit_tx, submit_rx) = channel();
        let mut desk = TurnDesk {
            assembler: TurnAssembler::new(),
            words_rx,
            submit_tx,
            observer: Arc::new(Recorder::default()),
            last_end: None,
            sent: 0,
            unanswered: None,
            withdrawn: Default::default(),
            turns_started: Default::default(),
            submits_done: Default::default(),
            stop: Default::default(),
            floor: Default::default(),
        };
        let (ran_tx, ran_rx) = channel();
        let callback_floor = desk.floor.clone();
        let submit: SubmitFn = Arc::new(move |text: String, _| {
            if !spoken_turn_withdrawn() {
                ran_tx.send(text).unwrap();
            }
            callback_floor.nudge();
        });
        let (withdrawn, hold, done) = (desk.withdrawn.clone(), desk.floor.clone(), desk.submits_done.clone());
        let worker = std::thread::spawn(move || submit_turns(submit_rx, withdrawn, hold, done, submit));

        // 60.000 s of input: the limit closes it, and the next utterance opens before the
        // recognizer has returned the last fragment.
        desk.assembler.his_utterance_started();
        desk.assembler.utterance_admitted(30_000);
        desk.assembler.time_limit();
        desk.assembler.his_utterance_started();
        assert!(desk.publish_floor(), "premise: the next utterance holds the floor");
        words_tx.send(Some(("everything up to the limit".to_string(), false))).unwrap();
        assert!(desk.collect());
        desk.release();
        assert!(desk.publish_floor(), "premise: the floor is still held while the turn goes out");
        match desk.floor.turn_waits_or_ran(&ran_rx) {
            Err(ran) => assert_eq!(ran, "everything up to the limit"),
            Ok(()) => panic!("the turn ended by the time limit is waiting for a floor the input never gives back"),
        }
        // His next words are a turn of their own: nothing to carry, nothing withdrawn.
        assert_eq!(desk.continue_thought(true, true), None, "the limited turn was carried into the next one");

        desk.floor.set(false);
        drop(desk);
        worker.join().unwrap();
    }

    /// INVARIANT — review rv-20261013T214648Z. "Send the draft" waits for the spine behind
    /// earlier work; he starts speaking again, and the earlier work lets go of the spine while
    /// that speech is still being recorded and recognized. The partial request does not start on
    /// its own in those seconds: it waits for his fragment, and his words then withdraw it, so
    /// the only request that runs is his whole thought. The way out: when that fragment turns out
    /// to be nothing (a cough, refused by the recognizer) the partial request runs unchanged.
    #[test]
    fn a_waiting_partial_turn_does_not_start_while_his_resumed_speech_is_open() {
        // The fixture's run, up to the moment the spine is let go during his resumed speech.
        // Returns the desk, the recognizer's channel, what ran, and the worker.
        type Resumed = (TurnDesk, Sender<Option<(String, bool)>>, Receiver<String>, std::thread::JoinHandle<()>);
        fn resumed_while_waiting() -> Resumed {
            let (words_tx, words_rx) = channel();
            let (submit_tx, submit_rx) = channel();
            let mut desk = TurnDesk {
                assembler: TurnAssembler::new(),
                words_rx,
                submit_tx,
                observer: Arc::new(Recorder::default()),
                last_end: None,
                sent: 0,
                unanswered: None,
                withdrawn: Default::default(),
                turns_started: Default::default(),
                submits_done: Default::default(),
                stop: Default::default(),
                floor: Default::default(),
            };
            let spine = Arc::new(Mutex::new(()));
            let earlier_work = spine.lock().unwrap();
            let (entered_tx, entered_rx) = channel();
            let (ran_tx, ran_rx) = channel();
            let callback_spine = spine.clone();
            let callback_floor = desk.floor.clone();
            let submit: SubmitFn = Arc::new(move |text: String, _| {
                if entered_tx.send(text.clone()).is_err() {
                    // Only the first turn is watched for; nobody listens for the joined one.
                }
                let _spine = callback_spine.lock().unwrap();
                if spoken_turn_withdrawn() {
                    return;
                }
                ran_tx.send(text).unwrap();
                callback_floor.nudge();
            });
            let (withdrawn, hold, done) =
                (desk.withdrawn.clone(), desk.floor.clone(), desk.submits_done.clone());
            let worker = std::thread::spawn(move || submit_turns(submit_rx, withdrawn, hold, done, submit));

            desk.assembler.his_utterance_started();
            desk.publish_floor();
            desk.assembler.utterance_admitted(1500);
            words_tx.send(Some(("Send the draft".to_string(), false))).unwrap();
            assert!(desk.collect());
            desk.assembler.quiet();
            desk.release();
            desk.publish_floor();
            assert_eq!(entered_rx.recv().unwrap(), "Send the draft", "the partial turn never reached the spine");

            // He resumes (supervise's `CapMsg::Started` arm), and while that fragment is still
            // being recorded the earlier work finishes.
            desk.assembler.his_utterance_started();
            assert!(desk.publish_floor(), "his resumed speech did not take the floor");
            drop(earlier_work);
            if let Err(ran) = desk.floor.turn_waits_or_ran(&ran_rx) {
                panic!("the partial request {ran:?} started on its own while he was still speaking");
            }
            // His fragment ends and goes to the recognizer: the turn is still waiting for it.
            desk.assembler.utterance_admitted(2000);
            assert!(desk.publish_floor(), "his fragment at the recognizer gave up the floor");
            assert_eq!(desk.floor.turns_waiting(), 1, "the partial turn stopped waiting while his words were at the recognizer");
            assert!(ran_rx.try_recv().is_err(), "the partial request started while his words were at the recognizer");
            (desk, words_tx, ran_rx, worker)
        }

        // His words come back and go on from the waiting turn: it is withdrawn, never run.
        let (mut desk, words_tx, ran, worker) = resumed_while_waiting();
        words_tx.send(Some(("after I approve it.".to_string(), false))).unwrap();
        assert!(desk.collect());
        assert_eq!(desk.continue_thought(false, false), Some(false), "the waiting turn was not withdrawn");
        desk.assembler.quiet();
        desk.release();
        desk.publish_floor();
        assert_eq!(
            ran.recv().unwrap(),
            "Send the draft after I approve it.",
            "the request that ran is not his whole thought"
        );
        drop(desk);
        worker.join().unwrap();
        assert!(ran.try_recv().is_err(), "a second request ran beside his whole thought");

        // His fragment was nothing (refused by the recognizer): the floor is back, and the
        // waiting turn runs, unchanged.
        let (mut desk, words_tx, ran, worker) = resumed_while_waiting();
        words_tx.send(None).unwrap();
        assert!(!desk.collect());
        desk.release();
        assert!(!desk.publish_floor(), "a refused fragment kept the floor");
        assert_eq!(
            ran.recv().unwrap(),
            "Send the draft",
            "the waiting turn never ran after his fragment came to nothing"
        );
        drop(desk);
        worker.join().unwrap();
    }

    /// INVARIANT: switching voice off within the end-of-turn window still delivers what he said,
    /// as an utterance recognized before the toggle always was.
    #[test]
    fn voice_off_before_the_quiet_still_delivers_his_words() {
        let rec = Arc::new(Recorder::default());
        let (words_tx, words_rx) = channel();
        let (submit_tx, submit_rx) = channel();
        {
            let mut desk = TurnDesk {
                assembler: TurnAssembler::new(),
                words_rx,
                submit_tx,
                observer: rec.clone(),
                last_end: None,
                sent: 0,
                unanswered: None,
                withdrawn: Default::default(),
                turns_started: Default::default(),
                submits_done: Default::default(),
                stop: Default::default(),
                floor: Default::default(),
            };
            desk.assembler.utterance_started();
            desk.assembler.utterance_admitted(1800);
            // Recognized but not yet collected when voice goes off.
            words_tx.send(Some(("Send the deck to Dana.".to_string(), false))).unwrap();
        }
        assert_eq!(submit_rx.try_recv().unwrap(), ("Send the deck to Dana.".to_string(), false));
        assert_eq!(transcripts(&rec).len(), 1);
    }

    /// INVARIANT — review rv-20261013T202747Z finding 1. He paused for longer than the window, so
    /// "Send the draft" was handed over and started; he went on, and switched voice off while the
    /// rest was still being captured. The voice-off flush does what a normal continuation does:
    /// the earlier turn is carried into the one going out and withdrawn, so the request that
    /// stands is his whole thought, never the instruction without its condition.
    #[test]
    fn voice_off_during_a_continuation_still_withdraws_the_partial_turn() {
        let rec = Arc::new(Recorder::default());
        let (words_tx, words_rx) = channel();
        let (submit_tx, submit_rx) = channel();
        let withdrawn: Arc<Mutex<HashSet<u64>>> = Default::default();
        let stops = Arc::new(AtomicU32::new(0));
        let counted = stops.clone();
        let stop: WithdrawHook = Arc::new(Mutex::new(Some(Arc::new(move || {
            counted.fetch_add(1, Ordering::SeqCst);
        }))));
        {
            let mut desk = TurnDesk {
                assembler: TurnAssembler::new(),
                words_rx,
                submit_tx,
                observer: rec.clone(),
                last_end: None,
                sent: 0,
                unanswered: None,
                withdrawn: withdrawn.clone(),
                turns_started: Default::default(),
                submits_done: Default::default(),
                stop,
                floor: Default::default(),
            };
            desk.assembler.his_utterance_started();
            desk.assembler.utterance_admitted(1500);
            words_tx.send(Some(("Send the draft".to_string(), false))).unwrap();
            desk.collect();
            desk.assembler.quiet();
            desk.release();
            assert_eq!(submit_rx.try_recv().unwrap().0, "Send the draft");
            desk.turns_started.fetch_add(1, Ordering::SeqCst);
            // He goes on; voice goes off before the supervisor has looked at the words.
            desk.assembler.his_utterance_started();
            desk.assembler.utterance_admitted(1200);
            words_tx.send(Some(("after I approve it.".to_string(), false))).unwrap();
        }
        assert_eq!(
            submit_rx.try_recv().unwrap().0,
            "Send the draft after I approve it.",
            "voice-off sent the continuation without the instruction it continues"
        );
        assert!(submit_rx.try_recv().is_err(), "one thought became more than one request");
        assert!(withdrawn.lock().unwrap().contains(&1), "the partial turn was left standing");
        assert_eq!(stops.load(Ordering::SeqCst), 1, "the running partial turn was not stopped");
    }

    /// INVARIANT — review rv-20261013T202747Z finding 2. He talked past the 30.000 s window and
    /// his last word ended 2 frames (0.032 s) after the cut. The recorder admits that tail as the
    /// rest of his sentence; the recognizer must not then refuse it for failing to prove a voice
    /// on its own, because the cut fragment it continues already did.
    #[test]
    fn the_tail_of_a_cut_utterance_reaches_whisper() {
        use crate::endpoint::{UtteranceRecorder, MAX_UTTERANCE_FRAMES, SILENCE_HANGOVER_FRAMES};
        use crate::vad::VAD_FRAME_SAMPLES;
        let speech_frames = MAX_UTTERANCE_FRAMES as usize + 2;
        let voice = synthetic_voice(
            (speech_frames * VAD_FRAME_SAMPLES) as f32 / crate::vad::SAMPLE_RATE as f32,
            190.0,
            130.0,
            -26.0,
        );
        let mut rec = UtteranceRecorder::new();
        let mut out = Vec::new();
        for f in voice.chunks_exact(VAD_FRAME_SAMPLES) {
            out.extend(rec.push_frame(f, true));
        }
        let silent = vec![0.0f32; VAD_FRAME_SAMPLES];
        for _ in 0..SILENCE_HANGOVER_FRAMES {
            out.extend(rec.push_frame(&silent, false));
        }
        assert_eq!(out.len(), 2, "the replay did not produce a cut and its tail");
        assert_eq!(out[0].reason, EndReason::MaxLength);
        assert_eq!(out[1].speech_frames, 2);

        let events = Recorder::default();
        let mut desk = RecognizerDesk::new();
        let heard = Mutex::new(Vec::<String>::new());
        for (u, words) in out.into_iter().zip(["the first thirty seconds", "today."]) {
            desk.handle(
                &AdmittedUtterance { utterance: Box::new(u), rich_audible: false },
                &events,
                |_| Ok((words.to_string(), 300)),
                |t, _| heard.lock().unwrap().push(t),
            );
        }
        assert_eq!(
            heard.into_inner().unwrap(),
            vec!["the first thirty seconds".to_string(), "today.".to_string()],
            "the last word after the cut never reached whisper"
        );
    }

    /// INVARIANT — review rv-20261013T204600Z. "Send the draft ... now": after an ordinary
    /// 1.408 s pause (88 x 256 / 16000) his last word is 14 or 18 speech frames (0.224 s or
    /// 0.288 s), under the 19-frame (0.304 s) minimum. It continues a turn whose first fragment
    /// was admitted, so it is admitted too and the turn is the whole sentence, whether the turn
    /// ends in 2.000 s of silence or voice goes off right after the word. The word still has to
    /// prove its own voice at the recognizer.
    #[test]
    fn a_short_last_word_after_a_natural_pause_joins_his_turn() {
        use crate::endpoint::MIN_SPEECH_FRAMES;
        use crate::turn::TurnAssembler;
        use crate::vad::{SAMPLE_RATE, VAD_FRAME_SAMPLES};
        let secs = |frames: usize| (frames * VAD_FRAME_SAMPLES) as f32 / SAMPLE_RATE as f32;
        let silent = |frames: usize| vec![0.0f32; frames * VAD_FRAME_SAMPLES];

        struct Run {
            brain: CaptureBrain,
            desk: RecognizerDesk,
            turns: TurnAssembler,
            discards: usize,
            requests: Vec<String>,
        }
        impl Run {
            fn handle(&mut self, msgs: Vec<CapMsg>, words: &str) {
                for m in msgs {
                    match m {
                        CapMsg::Started { tainted: false, .. } => self.turns.his_utterance_started(),
                        CapMsg::Started { .. } => panic!("his own words were marked as Rich's echo"),
                        CapMsg::Utterance(u) => {
                            self.turns.utterance_admitted(0);
                            let mut heard = None;
                            self.desk.handle(
                                &u,
                                &Recorder::default(),
                                |_| Ok((words.to_string(), 300)),
                                |t, a| heard = Some((t, a)),
                            );
                            self.turns.recognized(heard);
                        }
                        CapMsg::Discarded { .. } => {
                            self.discards += 1;
                            self.turns.utterance_discarded();
                        }
                        CapMsg::TurnQuiet => self.turns.quiet(),
                        _ => {}
                    }
                }
                if let Some(t) = self.turns.take_turn() {
                    self.requests.push(t.text);
                }
            }
            fn feed(&mut self, audio: &[f32], words: &str) {
                for f in audio.chunks_exact(VAD_FRAME_SAMPLES) {
                    let msgs = self.brain.push_frame(f, false, false);
                    self.handle(msgs, words);
                }
            }
        }

        for word_frames in [14usize, 18] {
            assert!(word_frames < MIN_SPEECH_FRAMES as usize);
            for voice_off in [false, true] {
                let mut run = Run {
                    brain: CaptureBrain::new(),
                    desk: RecognizerDesk::new(),
                    turns: TurnAssembler::new(),
                    discards: 0,
                    requests: Vec::new(),
                };
                run.feed(&silent(64), "");
                run.feed(&synthetic_voice(secs(64), 190.0, 130.0, -18.1), "Send the draft");
                // His first fragment closes 0.800 s into this pause and is recognized here.
                run.feed(&silent(88), "Send the draft");
                run.feed(&synthetic_voice(secs(word_frames), 190.0, 130.0, -18.1), "now");
                if voice_off {
                    let msgs = run.brain.finish();
                    run.handle(msgs, "now");
                    if let Some(t) = run.turns.flush() {
                        run.requests.push(t.text);
                    }
                } else {
                    run.feed(&silent(180), "now");
                }
                assert_eq!(
                    (run.requests, run.discards),
                    (vec!["Send the draft now".to_string()], 0),
                    "{word_frames} frames ({:.3} s), voice off = {voice_off}: his last word was lost",
                    secs(word_frames)
                );
            }
        }
    }

    /// `Shared` as the controller builds it, with nothing running yet.
    fn test_shared() -> Arc<Shared> {
        Arc::new(Shared {
            level: AtomicU32::new(0),
            speaking: AtomicBool::new(false),
            his_floor: AtomicBool::new(false),
            answer_sounded: AtomicBool::new(false),
            his_onset: AtomicBool::new(false),
            takes_a_reply: AtomicBool::new(false),
            reply_invited: AtomicBool::new(false),
            finish_capture: AtomicBool::new(false),
            capture_finished: AtomicBool::new(false),
            drained_after_finish: AtomicBool::new(false),
            pending_speech: AtomicUsize::new(0),
            pending_answer: AtomicUsize::new(0),
            unheard: Mutex::new(Unheard::default()),
            generation: AtomicU64::new(0),
            no_audio: AtomicBool::new(false),
            running: AtomicBool::new(true),
        })
    }

    /// The device queue without a sound card: `play` hands samples to the "device" as its
    /// callback would; `stop_now` drops what is still queued, as `Playout::stop_now` does.
    #[derive(Default)]
    struct TestDevice {
        queued: AtomicU64,
        played: AtomicU64,
    }

    impl TestDevice {
        fn play(&self, n: u64) {
            let n = n.min(self.queued.load(Ordering::SeqCst));
            self.queued.fetch_sub(n, Ordering::SeqCst);
            self.played.fetch_add(n, Ordering::SeqCst);
        }
    }

    impl SpeechQueue for TestDevice {
        fn queue(&self, mono: &[f32]) -> u64 {
            let queued = self.queued.fetch_add(mono.len() as u64, Ordering::SeqCst) + mono.len() as u64;
            self.played.load(Ordering::SeqCst) + queued
        }
        fn stop_now(&self) -> usize {
            self.queued.swap(0, Ordering::SeqCst) as usize
        }
        fn played(&self) -> u64 {
            self.played.load(Ordering::SeqCst)
        }
    }

    /// Samples per sentence the test synthesizer makes, in two blocks of half each.
    const TEST_SENTENCE: u64 = 200;

    /// The speaker thread's loop (`speak_rx.recv()` then `speak_one`) over everything queued to
    /// it. Returns the sentences whose every block reached the device queue.
    fn run_speaker(shared: &Arc<Shared>, rx: &Receiver<SpeakMsg>, device: &TestDevice) -> Vec<String> {
        let mut queued = Vec::new();
        for msg in rx.try_iter() {
            let mut whole = false;
            speak_one(shared, &msg, device, &mut |_text, emit| {
                let half = vec![0.0f32; (TEST_SENTENCE / 2) as usize];
                whole = emit(&half) && emit(&half);
                Ok(())
            })
            .unwrap();
            if whole {
                queued.push(msg.text.clone());
            }
        }
        queued
    }

    const JOINED: &str = "Will (Marketing Director) is on your team now, ready to take work.";

    /// One test rig: the shared state, the device, the gate, the speaker channel and the floor.
    type Rig = (Arc<Shared>, TestDevice, Arc<Mutex<AnswerGate<SpeakMsg>>>, Sender<SpeakMsg>, Receiver<SpeakMsg>, Floor);

    fn rig() -> Rig {
        let shared = test_shared();
        let gate: Arc<Mutex<AnswerGate<SpeakMsg>>> = Arc::new(Mutex::new(AnswerGate::new()));
        let (speak_tx, speak_rx) = channel::<SpeakMsg>();
        let floor = Floor { gate: gate.clone(), speak_tx: speak_tx.clone(), held_logged: 0 };
        (shared, TestDevice::default(), gate, speak_tx, speak_rx, floor)
    }

    /// INVARIANT — review rv-20261015T132600Z-fdc3dafb, finding 1 (fixture
    /// `work-result-in-flight.py`). A joined notice ALREADY SENT to the speaker, queued behind
    /// the last sentence of the reply, and he starts speaking before hearing it: his words start
    /// a new turn, which ends the reply's generation. The reply's sentence is not said; the notice
    /// is, once, after he has finished and never while he has the floor. Through the real path:
    /// `send_to_speaker` (the gate, then `dispatch` into the speaker channel),
    /// `Floor::his_new_turn` (supervisor step 1b), `Floor::publish` and `release` (step 1c), and
    /// the speaker's own `speak_one` reading the channel.
    #[test]
    fn a_notice_already_with_the_speaker_is_said_after_his_words_start_a_new_turn() {
        let (shared, device, gate, speak_tx, speak_rx, mut floor) = rig();

        // The tail of the reply is with the speaker; the notice arrives and queues behind it.
        send_to_speaker(&shared, &gate, &speak_tx, 0, "The tail of the earlier reply.".into(), false);
        send_to_speaker(&shared, &gate, &speak_tx, 0, JOINED.into(), true);
        assert_eq!(shared.pending_speech.load(Ordering::SeqCst), 2, "both are with the speaker");

        floor.publish(&shared, true); // He starts speaking.
        // The recognizer delivers his new question; no model turn is running.
        let (outstanding, _) = floor.his_new_turn(&shared, false, &device);
        assert!(outstanding, "the reply's sentence was still owed, so his words end its generation");
        floor.release(&shared, true);
        // The speaker works through its queue while he still has the floor.
        assert_eq!(run_speaker(&shared, &speak_rx, &device), Vec::<String>::new(), "said over him");

        floor.publish(&shared, false); // He finishes.
        floor.release(&shared, false);
        assert_eq!(
            run_speaker(&shared, &speak_rx, &device),
            vec![JOINED.to_string()],
            "the joined notice must be said once, and the superseded reply never"
        );
        assert_eq!(shared.generation.load(Ordering::SeqCst), 1);

        // Heard to its last sample: nothing is owed and nothing is said again.
        device.play(TEST_SENTENCE);
        shared.unheard.lock().unwrap().heard(device.played());
        assert!(shared.unheard.lock().unwrap().is_empty(), "heard, so not said again");
        assert_eq!(shared.pending_speech.load(Ordering::SeqCst), 0);
        assert_eq!(shared.pending_answer.load(Ordering::SeqCst), 0);
    }

    /// INVARIANT — the rest of the speech path past the gate (same review). A notice is said until
    /// he has heard it to its last sample, whatever cuts the device queue first:
    /// 1. he talks over it while it is SOUNDING (`Floor::barge_in`): cut, then said whole after him;
    /// 2. he presses Stop on the reply while the notice waits behind it at the device
    ///    (`stop_pressed`): said right after the reply stops, and the reply never;
    /// 3. he presses Stop while the notice itself is sounding: that is what he stopped, so it is
    ///    not said again (the way out);
    /// 4. a notice alone with the speaker does not make his new words end any generation.
    #[test]
    fn a_notice_cut_at_the_device_is_said_again_until_heard() {
        // 1. A barge-in while the notice sounds.
        let (shared, device, gate, speak_tx, speak_rx, mut floor) = rig();
        send_to_speaker(&shared, &gate, &speak_tx, 0, JOINED.into(), true);
        assert_eq!(run_speaker(&shared, &speak_rx, &device), vec![JOINED.to_string()]);
        device.play(TEST_SENTENCE / 2); // Half of it has been heard.
        floor.publish(&shared, true);
        assert_eq!(floor.barge_in(&shared, &device), (TEST_SENTENCE / 2) as usize);
        floor.release(&shared, true);
        assert_eq!(run_speaker(&shared, &speak_rx, &device), Vec::<String>::new(), "said over him");
        floor.publish(&shared, false);
        floor.release(&shared, false);
        assert_eq!(run_speaker(&shared, &speak_rx, &device), vec![JOINED.to_string()], "cut mid-playback and lost");
        device.play(TEST_SENTENCE);
        shared.unheard.lock().unwrap().heard(device.played());
        assert!(shared.unheard.lock().unwrap().is_empty());

        // 2. Stop on the reply, the notice queued behind it at the device.
        let (shared, device, gate, speak_tx, speak_rx, mut floor) = rig();
        send_to_speaker(&shared, &gate, &speak_tx, 0, "Here is the plan for Acme.".into(), false);
        send_to_speaker(&shared, &gate, &speak_tx, 0, JOINED.into(), true);
        assert_eq!(run_speaker(&shared, &speak_rx, &device).len(), 2);
        device.play(TEST_SENTENCE / 2); // The reply is sounding.
        assert_eq!(stop_pressed(&shared, &gate, &device), (TEST_SENTENCE * 3 / 2) as usize);
        // The model is still writing the reply he stopped.
        send_to_speaker(&shared, &gate, &speak_tx, 0, "More of the plan.".into(), false);
        floor.publish(&shared, false);
        floor.release(&shared, false);
        assert_eq!(
            run_speaker(&shared, &speak_rx, &device),
            vec![JOINED.to_string()],
            "lost with the reply he stopped, or the stopped reply went on"
        );

        // 3. Stop while the notice itself sounds.
        let (shared, device, gate, speak_tx, speak_rx, mut floor) = rig();
        send_to_speaker(&shared, &gate, &speak_tx, 0, JOINED.into(), true);
        assert_eq!(run_speaker(&shared, &speak_rx, &device), vec![JOINED.to_string()]);
        device.play(TEST_SENTENCE / 2);
        stop_pressed(&shared, &gate, &device);
        floor.publish(&shared, false);
        floor.release(&shared, false);
        assert_eq!(run_speaker(&shared, &speak_rx, &device), Vec::<String>::new(), "Stop could not end it");
        assert!(shared.unheard.lock().unwrap().is_empty());

        // 4. Only the notice is with the speaker when his words start a new turn.
        let (shared, device, gate, speak_tx, speak_rx, mut floor) = rig();
        send_to_speaker(&shared, &gate, &speak_tx, 0, JOINED.into(), true);
        floor.publish(&shared, true);
        assert_eq!(floor.his_new_turn(&shared, false, &device), (false, 0), "a notice is not an answer");
        floor.publish(&shared, false);
        assert_eq!(run_speaker(&shared, &speak_rx, &device), vec![JOINED.to_string()]);
    }

    /// INVARIANT — reviews rv-20261015T153739Z-e8097367-ffad and rv-20261015T162000Z-1749e1d8-d766,
    /// finding 1 of each (fixtures `stop_multisentence_result.py`, `stop-between-sentences.py`).
    /// Stop at ANY moment a job's result is being said ends the rest of THAT RESULT: during any
    /// of its sentences, between two of them, and while the next one is still being synthesized.
    /// A separate result behind it is not what he stopped, and is still said, once; so is one
    /// whose own synthesis is under way after the stopped result's last sentence has played.
    /// Both pacings of the speaker: synthesis slower than playback (the device runs dry between
    /// sentences), and synthesis ahead of playback (every sentence already queued at the device).
    /// Through the shipping path: `send_result` (what `speak_result` runs), the speaker's own
    /// `speak_one`, `stop_pressed`, and the floor's release.
    #[test]
    fn stop_during_a_spoken_result_ends_the_rest_of_that_result() {
        use std::cell::Cell;
        /// Where he presses Stop; `usize` is the sentence, 0..=2 the result, 3 the separate one.
        #[derive(Clone, Copy, Debug, PartialEq)]
        enum At {
            /// Being synthesized; nothing of it has reached the device.
            Synthesizing(usize),
            /// Half of it has played.
            Sounding(usize),
            /// All of it has played; the next has not begun.
            Played(usize),
        }
        const REPORT: &str = "The sales report is ready. Revenue increased by ten percent. Costs held steady.";

        // What is said after he presses Stop at `at`. `ahead`: every sentence is synthesized and
        // queued at the device before any of it plays.
        let said_after_stop = |at: At, ahead: bool| -> Vec<String> {
            let (shared, device, gate, speak_tx, speak_rx, mut floor) = rig();
            send_result(&shared, &gate, &speak_tx, 0, REPORT);
            send_result(&shared, &gate, &speak_tx, 0, JOINED);
            let stopped = Cell::new(false);
            let press = |now: At| {
                if now == at && !stopped.get() {
                    stop_pressed(&shared, &gate, &device);
                    stopped.set(true);
                }
            };
            let msgs: Vec<SpeakMsg> = speak_rx.try_iter().collect();
            assert_eq!(msgs.len(), 4, "three sentences of the report and the separate result");
            let synthesize = |i: usize, msg: &SpeakMsg| {
                speak_one(&shared, msg, &device, &mut |_, emit| {
                    press(At::Synthesizing(i));
                    let half = vec![0.0f32; (TEST_SENTENCE / 2) as usize];
                    let _whole = emit(&half) && emit(&half);
                    Ok(())
                })
                .unwrap();
            };
            if ahead {
                for (i, msg) in msgs.iter().enumerate() {
                    synthesize(i, msg);
                }
            }
            for (i, msg) in msgs.iter().enumerate() {
                if !ahead {
                    synthesize(i, msg);
                }
                if stopped.get() {
                    break;
                }
                device.play(TEST_SENTENCE / 2);
                press(At::Sounding(i));
                if stopped.get() {
                    break;
                }
                device.play(TEST_SENTENCE / 2);
                press(At::Played(i));
                if stopped.get() {
                    break;
                }
            }
            assert!(stopped.get(), "{at:?} (ahead = {ahead}) was never reached");
            floor.publish(&shared, false);
            floor.release(&shared, false);
            let said = run_speaker(&shared, &speak_rx, &device);
            device.play(TEST_SENTENCE * 4);
            shared.unheard.lock().unwrap().heard(device.played());
            assert!(shared.unheard.lock().unwrap().is_empty(), "{at:?} (ahead = {ahead}): heard, so not said again");
            said
        };

        let mut points = Vec::new();
        for i in 0..3 {
            if i > 0 {
                points.push((At::Synthesizing(i), false)); // The device is dry; the next is being made.
            }
            points.push((At::Sounding(i), false));
            points.push((At::Sounding(i), true));
            points.push((At::Played(i), false)); // Between sentences, before the next is begun.
            points.push((At::Played(i), true));
        }
        points.push((At::Synthesizing(3), false)); // The report is over; the separate result is being made.
        let wrong: Vec<String> = points
            .into_iter()
            .map(|(at, ahead)| (at, ahead, said_after_stop(at, ahead)))
            .filter(|(_, _, said)| *said != vec![JOINED.to_string()])
            .map(|(at, ahead, said)| format!("Stop at {at:?} (ahead = {ahead}) then said {said:?}"))
            .collect();
        assert!(wrong.is_empty(), "after Stop, only the separate result is to be said:\n{}", wrong.join("\n"));
    }

    /// INVARIANT — review rv-20261015T131103Z-6b6f899d, finding 1 (fixture
    /// `work_result_recognition.rs`). A job's result ("Will is on your team now") that arrives
    /// while he is speaking waits for him, and it is NOT lost when his words are recognized and
    /// start a new turn: it is said once he has finished, never while he has the floor, and never
    /// ahead of a sentence of Rich's. Driven through the supervisor's own tick: the recognizer's
    /// words reach `TurnDesk::collect`, then `Floor::his_new_turn`, then the floor's release.
    /// Two cases: the review's (he had stopped an earlier reply; no turn running), and a turn of
    /// Rich's running with a sentence of its answer held ahead of the notice.
    #[test]
    fn a_result_held_while_he_speaks_is_said_after_his_words_start_a_new_turn() {
        let joined = "Will (Marketing Director) is on your team now, ready to take work.";
        for turn_active in [false, true] {
            let shared = test_shared();
            let device = TestDevice::default();
            let gate: Arc<Mutex<AnswerGate<SpeakMsg>>> = Arc::new(Mutex::new(AnswerGate::new()));
            let (speak_tx, speak_rx) = channel::<SpeakMsg>();
            let mut floor = Floor { gate: gate.clone(), speak_tx, held_logged: 0 };
            let (words_tx, words_rx) = channel();
            let (submit_tx, submit_rx) = channel();
            let mut desk = TurnDesk {
                assembler: TurnAssembler::new(),
                words_rx,
                submit_tx,
                observer: Arc::new(Recorder::default()),
                last_end: None,
                sent: 0,
                unanswered: None,
                withdrawn: Default::default(),
                turns_started: Default::default(),
                submits_done: Default::default(),
                stop: Default::default(),
                floor: Default::default(),
            };
            // One supervisor tick, steps 1b and 1c, as `supervise` runs them.
            let tick = |desk: &mut TurnDesk, floor: &mut Floor| {
                if desk.collect() {
                    let (outstanding, _) = floor.his_new_turn(&shared, turn_active, &device);
                    desk.continue_thought(outstanding, turn_active);
                }
                desk.release();
                let his_floor = desk.publish_floor();
                floor.publish(&shared, his_floor);
                floor.release(&shared, his_floor);
            };
            // `send_to_speaker`: the generation at the send, the floor as published.
            let offer = |text: &str, announcement: bool| {
                let msg = SpeakMsg {
                    generation: shared.generation.load(Ordering::Relaxed),
                    text: text.into(),
                    notice: announcement.then_some(0),
                    result: if announcement { next_result() } else { 0 },
                };
                let his = shared.his_floor.load(Ordering::Relaxed);
                let mut g = gate.lock().unwrap();
                let now = if announcement { g.offer_announcement(msg, his) } else { g.offer(msg, his) };
                assert!(now.is_none(), "turn_active = {turn_active}: said over him");
            };

            if !turn_active {
                gate.lock().unwrap().supersede(); // He tapped Stop on an earlier reply.
            }
            desk.assembler.his_utterance_started();
            tick(&mut desk, &mut floor);
            if turn_active {
                offer("Here is the plan for Acme.", false);
            }
            offer(joined, true);
            tick(&mut desk, &mut floor);
            assert!(speak_rx.try_recv().is_err(), "turn_active = {turn_active}: said while he has the floor");

            // His words are recognized and start a new turn.
            desk.assembler.utterance_admitted(1200);
            words_tx.send(Some(("What should we do next?".to_string(), false))).unwrap();
            tick(&mut desk, &mut floor);
            assert!(speak_rx.try_recv().is_err(), "turn_active = {turn_active}: said while he has the floor");

            // He finishes: his turn goes to Rich, and the floor is free.
            desk.assembler.quiet();
            tick(&mut desk, &mut floor);
            assert_eq!(submit_rx.try_recv().unwrap().0, "What should we do next?");
            gate.lock().unwrap().next_turn(); // `turn_started`
            tick(&mut desk, &mut floor);

            let current = shared.generation.load(Ordering::Relaxed);
            let said: Vec<(String, bool)> =
                speak_rx.try_iter().map(|m| (m.text, m.generation == current)).collect();
            assert_eq!(
                said,
                vec![(joined.to_string(), true)],
                "turn_active = {turn_active}: the notice must be said once, in the current generation, and the superseded answer never"
            );
        }
    }

    /// INVARIANT — review rv-20261013T210558Z, finding 1. "Send the draft ... now" with a
    /// thinking pause LONGER than the 2.000 s end of turn: 138 or 184 frames (2.208 s, 2.944 s),
    /// so "Send the draft" was handed over, and then a 14-frame (0.224 s) last word before Rich
    /// has said anything. That word still continues his turn: it is admitted however short, and
    /// the request that stands is the whole sentence, with voice on and when voice goes off right
    /// after it. Once Rich has answered aloud the 0.304 s minimum is back, so the same short
    /// sound after his answer is still dropped.
    #[test]
    fn a_short_last_word_after_the_end_of_turn_joins_his_turn_until_rich_answers() {
        use crate::endpoint::MIN_SPEECH_FRAMES;
        use crate::vad::{SAMPLE_RATE, VAD_FRAME_SAMPLES};
        let secs = |frames: usize| (frames * VAD_FRAME_SAMPLES) as f32 / SAMPLE_RATE as f32;
        let silent = |frames: usize| vec![0.0f32; frames * VAD_FRAME_SAMPLES];
        let voice = |frames: usize| synthetic_voice(secs(frames), 190.0, 130.0, -18.1);

        struct Run {
            brain: CaptureBrain,
            recognizer: RecognizerDesk,
            desk: TurnDesk,
            words_tx: Sender<Option<(String, bool)>>,
            discards: usize,
        }
        impl Run {
            fn handle(&mut self, msgs: Vec<CapMsg>, words: &str) {
                for m in msgs {
                    match m {
                        CapMsg::Started { tainted: false, .. } => self.desk.assembler.his_utterance_started(),
                        CapMsg::Started { .. } => self.desk.assembler.utterance_started(),
                        CapMsg::Utterance(u) => {
                            self.desk.assembler.utterance_admitted(0);
                            let mut heard = None;
                            self.recognizer.handle(
                                &u,
                                &Recorder::default(),
                                |_| Ok((words.to_string(), 300)),
                                |t, a| heard = Some((t, a)),
                            );
                            self.words_tx.send(heard).unwrap();
                        }
                        CapMsg::Discarded { .. } => {
                            self.discards += 1;
                            self.desk.assembler.utterance_discarded();
                        }
                        CapMsg::TurnQuiet => self.desk.assembler.quiet(),
                        _ => {}
                    }
                }
                // The supervisor's tick: his words that start a turn while the partial one is
                // still unanswered carry it, and the partial one is withdrawn.
                if self.desk.collect() {
                    self.desk.withdraw_unanswered(false);
                }
                self.desk.release();
            }
            fn feed(&mut self, audio: &[f32], speaking: bool, words: &str) {
                for f in audio.chunks_exact(VAD_FRAME_SAMPLES) {
                    let msgs = self.brain.push_frame(f, speaking, false);
                    self.handle(msgs, words);
                }
            }
        }

        assert!(14 < MIN_SPEECH_FRAMES as usize);
        for (pause_frames, rich_answered, voice_off) in [
            (138usize, false, false),
            (138, false, true),
            (184, false, false),
            (184, false, true),
            (138, true, false),
        ] {
            let (words_tx, words_rx) = channel();
            let (submit_tx, submit_rx) = channel();
            let withdrawn: Arc<Mutex<HashSet<u64>>> = Default::default();
            let mut run = Run {
                brain: CaptureBrain::new(),
                recognizer: RecognizerDesk::new(),
                desk: TurnDesk {
                    assembler: TurnAssembler::new(),
                    words_rx,
                    submit_tx,
                    observer: Arc::new(Recorder::default()),
                    last_end: None,
                    sent: 0,
                    unanswered: None,
                    withdrawn: withdrawn.clone(),
                    turns_started: Default::default(),
                    submits_done: Default::default(),
                    stop: Default::default(),
                    floor: Default::default(),
                },
                words_tx,
                discards: 0,
            };
            run.feed(&silent(64), false, "");
            run.feed(&voice(64), false, "Send the draft");
            run.feed(&silent(pause_frames), false, "Send the draft");
            if rich_answered {
                // Rich answers aloud (0.512 s), then 1.024 s of quiet before the short sound.
                run.feed(&silent(32), true, "");
                run.feed(&silent(64), false, "");
            }
            run.feed(&voice(14), false, "now");
            if voice_off {
                let msgs = run.brain.finish();
                run.handle(msgs, "now");
            } else {
                run.feed(&silent(180), false, "now");
            }
            let discards = run.discards;
            // Dropping the desk is voice going off: it flushes whatever is still held.
            drop(run);
            let mut standing = Vec::new();
            let mut seq = 0u64;
            while let Ok((text, _)) = submit_rx.try_recv() {
                seq += 1;
                if !withdrawn.lock().unwrap().contains(&seq) {
                    standing.push(text);
                }
            }
            let expected = if rich_answered { (vec!["Send the draft"], 1) } else { (vec!["Send the draft now"], 0) };
            assert_eq!(
                (standing.iter().map(String::as_str).collect::<Vec<_>>(), discards),
                expected,
                "pause {:.3} s, Rich answered = {rich_answered}, voice off = {voice_off}",
                secs(pause_frames)
            );
        }
    }

    /// INVARIANT: a ONE-WORD decision still gets through. `stt.rs` keeps its noise list
    /// narrow precisely so "Yes." survives; an audio gate that swallowed it would have
    /// undone that from the other side.
    #[test]
    fn a_one_word_decision_still_reaches_the_spine() {
        let rec = Recorder::default();
        let mut desk = RecognizerDesk::new();
        let sent = Mutex::new(Vec::<String>::new());
        desk.handle(
            &utterance(framed(synthetic_voice(0.30, 210.0, 150.0, -26.0), -55.0, 5)),
            &rec,
            |_| Ok(("Yes.".to_string(), 320)),
            |t, _| sent.lock().unwrap().push(t),
        );
        assert_eq!(sent.into_inner().unwrap(), vec!["Yes.".to_string()]);
    }

    /// INVARIANT: the audio gate is added IN FRONT of the noise-phrase filter, not instead
    /// of it. A real voice whose transcript comes back as whisper's silence noise is still
    /// discarded, and still never submitted.
    #[test]
    fn the_noise_phrase_filter_still_applies_to_audio_that_did_carry_a_voice() {
        let rec = Recorder::default();
        let mut desk = RecognizerDesk::new();
        desk.handle(
            &utterance(framed(synthetic_voice(1.2, 190.0, 130.0, -26.0), -55.0, 11)),
            &rec,
            |_| Ok(("Thanks for watching!".to_string(), 300)),
            |t, _| panic!("whisper's silence noise was submitted: {t:?}"),
        );
        assert!(transcripts(&rec).is_empty());
    }

    /// INVARIANT: the refusal is said ONCE per run and again after a recovery. A line per
    /// refusal would arrive every ~1.104 s from an open mic in a quiet room, and a notice
    /// that repeats forever trains him to ignore the one that matters.
    #[test]
    fn the_refusal_is_said_once_per_run_and_again_after_the_room_recovers() {
        let rec = Recorder::default();
        let mut desk = RecognizerDesk::new();
        let refuse = |desk: &mut RecognizerDesk, rec: &Recorder| {
            desk.handle(&utterance(hiss(2.0, -40.0, 13)), rec, |_| panic!("reached whisper"), |_, _| {});
        };
        refuse(&mut desk, &rec);
        refuse(&mut desk, &rec);
        refuse(&mut desk, &rec);
        assert_eq!(messages(&rec).len(), 1, "the notice became a drip");

        // He speaks; the room is proven to work; the latch clears.
        desk.handle(
            &utterance(framed(synthetic_voice(1.2, 190.0, 130.0, -26.0), -55.0, 17)),
            &rec,
            |_| Ok(("Approved.".to_string(), 300)),
            |_, _| {},
        );
        refuse(&mut desk, &rec);
        assert_eq!(messages(&rec).len(), 2, "the refusal went silent after a good turn");
    }

    /// INVARIANT — **audit-3 §4 #4, the exact frame `a3-15`.** One 2.3 s utterance, whisper
    /// returns `(clears throat)`, and the window must not sit there unchanged.
    ///
    /// The numbers are the audit's: a 2.3 s recording, and whisper's real return value. What
    /// is asserted is all three halves of the defect — he is TOLD, nothing was SENT, and the
    /// operator's log line still carries the transcript (that last one by the `eprintln`
    /// beside the emit, which this test cannot read and does not claim to).
    #[test]
    fn a_single_discarded_transcript_says_so_instead_of_changing_nothing() {
        let rec = Recorder::default();
        let mut desk = RecognizerDesk::new();
        desk.handle(
            &utterance(framed(synthetic_voice(2.3, 190.0, 130.0, -26.0), -55.0, 23)),
            &rec,
            |_| Ok(("(clears throat)".to_string(), 310)),
            |t, _| panic!("a discarded transcript was submitted as his message: {t:?}"),
        );
        assert_eq!(
            notices(&rec),
            vec![VoiceNotice::DidNotCatchThat.ceo_message().to_string()],
            "the discard was silent — the defect audit-3 §4 #4 recorded at frame a3-15"
        );
        assert!(transcripts(&rec).is_empty(), "whisper's noise reached the thread");
        assert!(
            messages(&rec).is_empty(),
            "a working microphone raised a voice-ERROR; nothing failed and voice did not stop"
        );
    }

    /// POSITIVE CONTROL for the test above. Without it that test would pass just as well if
    /// the notice fired on EVERY utterance, which would be a worse product than the silence
    /// it replaced.
    #[test]
    fn an_utterance_that_becomes_a_turn_raises_no_notice_at_all() {
        let rec = Recorder::default();
        let mut desk = RecognizerDesk::new();
        let sent = Mutex::new(Vec::<String>::new());
        desk.handle(
            &utterance(framed(synthetic_voice(2.3, 190.0, 130.0, -26.0), -55.0, 23)),
            &rec,
            |_| Ok(("Book the flight for Tuesday.".to_string(), 310)),
            |t, _| sent.lock().unwrap().push(t),
        );
        assert_eq!(sent.into_inner().unwrap(), vec!["Book the flight for Tuesday.".to_string()]);
        assert!(notices(&rec).is_empty(), "a good turn apologized for itself");
        assert!(messages(&rec).is_empty());
    }

    /// INVARIANT: a RUN of discards produces two sentences in total, in order, and never two
    /// in one breath. The short line speaks first because it is the first thing he needs; at
    /// [`SILENT_DISCARD_RUN`] the stronger line takes over and the short one stands aside.
    ///
    /// The count is the thing being pinned. Before this change the run said NOTHING until the
    /// third; a fix that made it say something every time would be a drip, which is the
    /// failure `HeardNoVoice` documents at length.
    #[test]
    fn a_run_of_discards_speaks_once_then_escalates_once_and_never_both_at_once() {
        let rec = Recorder::default();
        let mut desk = RecognizerDesk::new();
        let discard = |desk: &mut RecognizerDesk, rec: &Recorder, seed: u64| {
            desk.handle(
                &utterance(framed(synthetic_voice(1.2, 190.0, 130.0, -26.0), -55.0, seed)),
                rec,
                |_| Ok(("[BLANK_AUDIO]".to_string(), 300)),
                |t, _| panic!("submitted: {t:?}"),
            );
        };

        discard(&mut desk, &rec, 31);
        assert_eq!(notices(&rec).len(), 1, "the first discard was silent");
        assert!(messages(&rec).is_empty(), "the first discard escalated immediately");

        discard(&mut desk, &rec, 37);
        assert_eq!(notices(&rec).len(), 1, "the second discard repeated the line — a drip");
        assert!(messages(&rec).is_empty());

        // SILENT_DISCARD_RUN == 3: the stronger line, and NOT a second short one.
        discard(&mut desk, &rec, 41);
        assert_eq!(
            messages(&rec),
            vec![VoiceNotice::SoundButNoWords.ceo_message().to_string()],
            "the run never escalated"
        );
        assert_eq!(notices(&rec).len(), 1, "both lines fired at once — he was told twice");

        discard(&mut desk, &rec, 43);
        assert_eq!(notices(&rec).len(), 1, "the run kept talking after it had said its piece");
        assert_eq!(messages(&rec).len(), 1);
    }

    /// INVARIANT: the latch clears on a good turn, so the next bad one speaks again. Same
    /// contract as `the_refusal_is_said_once_per_run_and_again_after_the_room_recovers`,
    /// which is the neighboring refusal path — the two now behave the same way, and that
    /// symmetry IS the fix.
    #[test]
    fn a_good_turn_clears_the_latch_so_the_next_discard_speaks_again() {
        let rec = Recorder::default();
        let mut desk = RecognizerDesk::new();
        let discard = |desk: &mut RecognizerDesk, rec: &Recorder, seed: u64| {
            desk.handle(
                &utterance(framed(synthetic_voice(1.2, 190.0, 130.0, -26.0), -55.0, seed)),
                rec,
                |_| Ok(("(clears throat)".to_string(), 300)),
                |_, _| {},
            );
        };
        discard(&mut desk, &rec, 51);
        assert_eq!(notices(&rec).len(), 1);

        desk.handle(
            &utterance(framed(synthetic_voice(1.2, 190.0, 130.0, -26.0), -55.0, 53)),
            &rec,
            |_| Ok(("Approved.".to_string(), 300)),
            |_, _| {},
        );
        discard(&mut desk, &rec, 59);
        assert_eq!(notices(&rec).len(), 2, "the discard went silent after a good turn");
    }

    /// INVARIANT — **audit-3 §4 #1: a discard during Rich's playout is no longer silent.** The
    /// first one of the voice session says so; the rest are stderr only.
    #[test]
    fn the_first_discard_during_playout_says_so_and_the_rest_do_not() {
        let mut b = RefusalNotices::new();
        assert_eq!(
            b.half_duplex(),
            Some(VoiceNotice::CouldNotListenWhileSpeaking),
            "the first one was silent — the defect itself"
        );
        assert!(b.half_duplex_already_said());
        assert_eq!(b.half_duplex(), None, "the notice became a drip");
        assert_eq!(b.half_duplex(), None);
    }

    /// INVARIANT — **and this is the assertion that reversed on 2026-09-17.** It used to read
    /// `being_heard_clears_the_talked_over_latch`: an admitted utterance reset the latch, so
    /// the next discard spoke again.
    ///
    /// **Ray's candidate-.4 walk is what made that wrong.** On the CEO's rig the discards are
    /// not interruptions, they are Rich's own voice returning through the speakers, and they
    /// happen after EVERY spoken answer. So the old cycle was: he speaks → heard → latch clears
    /// → Rich answers → echo discarded → notice. One notice per answer, for the life of the
    /// session, about a condition nothing he does can change. The measurement behind "nothing
    /// he does can change it" is in `RefusalNotices`' own doc comment: single-digit ERLE on a
    /// path that would need the residual 6 dB under the VAD's speech floor.
    ///
    /// So being heard no longer clears it, and the latch has no clearing path at all.
    #[test]
    fn being_heard_does_not_re_arm_the_notice_because_the_condition_is_not_an_event() {
        let mut b = RefusalNotices::new();
        assert!(b.half_duplex().is_some());
        // `recovered()` is the strongest recovery signal there is — an utterance admitted AND
        // understood — and it deliberately does NOT re-arm this one. A per-answer line about a
        // standing property of the room is the defect this prevents.
        b.recovered();
        assert!(b.half_duplex_already_said(), "the latch cleared itself somehow");
        assert_eq!(b.half_duplex(), None, "one notice per spoken answer, forever");
        // POSITIVE CONTROL: recovery DID clear the standing card, so the other family members
        // can still speak. Without this the test above would pass on a budget that had simply
        // jammed shut.
        assert_eq!(b.standing(), None);
        assert_eq!(b.heard_no_voice(), Some(VoiceNotice::HeardNoVoice));
    }

    /// INVARIANT: a fresh latch says nothing until something is actually discarded. Without
    /// this, a latch that returned `true` on construction would pass the two tests above.
    #[test]
    fn a_fresh_latch_has_nothing_to_say() {
        let b = RefusalNotices::new();
        assert!(!b.half_duplex_already_said());
        assert_eq!(b.standing(), None);
        assert!(!RefusalNotices::default().half_duplex_already_said());
        assert_eq!(RefusalNotices::default().standing(), None);
    }

    /// **NEGATIVE PROBE — candidate .4's blocker: the notice must not tell him he spoke.**
    ///
    /// Ray proved on the CEO's screen that this line fires on Rich's own echo with the CEO
    /// silent (volume 85 → notice; the identical turn at volume 0 → no discard, no notice).
    /// The sentence that shipped opened *"You started talking while I was still speaking"*,
    /// which is a statement about something HE did, emitted on the one path where the canceller
    /// has declined to vouch for its residual.
    ///
    /// This asserts the property rather than the sentence: **every reference to the CEO is
    /// conditional.** A future edit may reword freely and may not reintroduce an assertion
    /// about him.
    #[test]
    fn the_notice_makes_no_unconditional_claim_about_the_ceo() {
        let m = VoiceNotice::CouldNotListenWhileSpeaking.ceo_message();
        let lower = m.to_lowercase();
        // The conditional is the whole correction, so it is required by name.
        assert!(lower.contains("if you said something"), "{m}");
        // None of these can be true of a path where `confident == false`. The first is the
        // exact sentence Ray's walk refused.
        for accusation in [
            "you started talking",
            "you were talking",
            "you spoke",
            "you interrupted",
            "you talked over",
            "your voice reached",
        ] {
            assert!(!lower.contains(accusation), "{m} asserts {accusation:?} and cannot know it");
        }
        // What it IS allowed to assert: Rich's own behavior, which this file does know.
        assert!(lower.contains("while i was speaking"), "{m}");
    }

    /// INVARIANT: the line states the CONSEQUENCE and then WHAT TO DO about it (2026-10-21: a
    /// new user was left with a loss and no next step). The step is the one the open
    /// microphone supports, saying it again now, and never a work-around such as waiting
    /// for Rich to finish.
    #[test]
    fn every_input_refusal_names_a_next_step() {
        for notice in [VoiceNotice::HeardNoVoice, VoiceNotice::DidNotCatchThat,
                       VoiceNotice::SoundButNoWords, VoiceNotice::CouldNotListenWhileSpeaking] {
            assert!(notice.ceo_message().contains("say it again"), "{notice:?}: {}", notice.ceo_message());
        }
    }

    #[test]
    fn the_half_duplex_line_states_the_consequence_and_what_to_do() {
        let m = VoiceNotice::CouldNotListenWhileSpeaking.ceo_message();
        // The fact he is missing: those words did not arrive, and nothing was sent in his
        // name. The second half is what makes Rich answering a fragment make sense.
        assert!(m.contains("didn't reach me"), "{m}");
        assert!(m.contains("haven't sent anything"), "{m}");
        // The next step, conditional like everything else it says about him.
        assert!(m.contains("If you did, please say it again now."), "{m}");
        assert!(m.ends_with("I'm listening."), "{m}");
        // Never an instruction to work around the limitation.
        for workaround in ["wait ", "until i finish", "headphones", "speak louder"] {
            assert!(!m.to_lowercase().contains(workaround), "{m} contains {workaround:?}");
        }
        // No machinery, same bar as every other notice here.
        assert!(!m.chars().any(|c| c.is_ascii_digit()), "{m}");
        assert!(!m.to_lowercase().contains("aec"), "{m}");
        assert!(!m.to_lowercase().contains("cancel"), "{m}");
        assert!(!m.to_lowercase().contains("barge"), "{m}");
    }

    /// **THE PUBLISHED ERLE CAN BE NEGATIVE.** The packed `AtomicU32` this replaced computed
    /// `(erle_db.max(0.0) as u32) << 1`, so the −0.1 dB measured live on the CEO's rig reached
    /// the operator log as `erle=0 dB` — indistinguishable from "has not started".
    #[test]
    fn a_negative_erle_survives_publication() {
        let s = AecShared::default();
        s.store(false, -0.14, true, 0.0148, true);
        assert!(!s.confident());
        assert!((s.erle_db().unwrap() - (-0.14)).abs() < 0.001, "{:?}", s.erle_db());
        // 0.0148 rms = 20*log10(0.0148) = -36.59 dBFS — the live figure from the walk's rig.
        assert!((s.leak_dbfs().unwrap() - (-36.59)).abs() < 0.02, "{:?}", s.leak_dbfs());
        // POSITIVE CONTROL: the same path carries a positive reading unharmed.
        s.store(true, 28.0, true, 0.00126, true);
        assert!(s.confident());
        assert!((s.erle_db().unwrap() - 28.0).abs() < 0.001, "{:?}", s.erle_db());
    }

    /// **DEFECT 3 OF THE CANDIDATE-.6 WALK: A SENTINEL PRINTED AS A MEASUREMENT.**
    ///
    /// All four discards printed `erle=0.0 dB, residual 0.0 dBFS`. Both figures were "nothing
    /// measured" and neither could say so, because both arrived as a bare `f32`:
    ///
    /// - `EchoCanceller::erle_db` returns a hard `0.0` until both smoothed powers exceed
    ///   `1e-12`, and those only accumulate on far-end-active unfrozen blocks.
    /// - `residual_typ_rms` starts at `1.0`, and `20*log10(1.0)` = **0.0 dBFS — full scale**,
    ///   the loudest reading the unit can express.
    ///
    /// A NEVER-STORED `AecShared` is the strongest form of the case: nothing has been published
    /// at all, and the old accessors answered `0.0` and `0.0` to that.
    #[test]
    fn an_unmeasured_canceller_says_not_measured_and_never_zero() {
        let s = AecShared::default();
        assert_eq!(s.erle_db(), None, "a cold AecShared must not report an ERLE");
        assert_eq!(s.leak_dbfs(), None, "a cold AecShared must not report a residual");
        assert_eq!(measured_or_not(s.erle_db(), "dB"), "not measured");
        assert_eq!(measured_or_not(s.leak_dbfs(), "dBFS"), "not measured");

        // A LIVE canceller that has genuinely measured 0.0 dB of enhancement — the OTHER
        // meaning of the same three characters — still prints the number. That is the positive
        // control that stops this being "suppress zeros".
        s.store(false, 0.0, true, 1.0, true);
        assert_eq!(s.erle_db(), Some(0.0));
        assert_eq!(measured_or_not(s.erle_db(), "dB"), "0.0 dB");
        assert_eq!(measured_or_not(s.leak_dbfs(), "dBFS"), "0.0 dBFS");

        // And a canceller whose ERLE is measured while its residual tracker is not — the two
        // flags are independent because the two accumulators are.
        s.store(false, -2.9, true, 1.0, false);
        assert_eq!(measured_or_not(s.erle_db(), "dB"), "-2.9 dB");
        assert_eq!(measured_or_not(s.leak_dbfs(), "dBFS"), "not measured");
    }

    /// The same rule at the canceller's own boundary: a cold [`crate::aec::EchoCanceller`] must
    /// report `erle_measured == false`, and a warm one `true`. Pinned HERE as well as in
    /// `aec.rs` because this file is what prints the figure, and a print site that trusts a
    /// flag needs the flag's meaning held somewhere it can see.
    #[test]
    fn the_cancellers_own_measured_flags_start_false_and_become_true() {
        let (mut aec, ring) = crate::aec::EchoCanceller::new();
        let cold = aec.metrics();
        assert!(!cold.erle_measured, "a cold canceller has measured no ERLE");
        assert!(!cold.leak_measured, "a cold canceller has no seeded residual");
        assert_eq!(cold.erle_text(), "not measured");
        assert_eq!(cold.leak_text(), "not measured");
        // 1.0 rms is the unseeded initial value and it is 0.0 dBFS — the number the walk saw.
        assert!((20.0 * cold.leak_floor_rms.max(1e-9).log10()).abs() < 1e-6, "{cold:?}");

        // Now give it a real far end: a loud reference and a mic carrying an echo of it.
        let mut phase = 0.0f32;
        for _ in 0..200 {
            let mut refblk = [0.0f32; crate::aec::AEC_BLOCK];
            for x in refblk.iter_mut() {
                *x = 0.3 * (phase).sin();
                phase += 2.0 * std::f32::consts::PI * 440.0 / crate::vad::SAMPLE_RATE as f32;
            }
            ring.push(&refblk);
            let mut mic = refblk;
            for x in mic.iter_mut() {
                *x *= 0.5;
            }
            aec.process_block(&mut mic);
        }
        let warm = aec.metrics();
        assert!(warm.erle_measured, "far-active blocks must make the ERLE a measurement");
        assert!(warm.leak_measured, "far-active blocks must seed the residual tracker");
        assert_ne!(warm.erle_text(), "not measured");
        assert_ne!(warm.leak_text(), "not measured");
    }

    /// **THE PROVENANCE IS WRITTEN WHERE TAINT IS, AND IT SURVIVES TAINT'S TWO ESCAPES.**
    ///
    /// The gap (`c712ccd5`): `PromptReceived` carried `source: Source::Jam` and nothing else, so
    /// an echo-born spoken turn and a genuine one were indistinguishable after the fact.
    ///
    /// The subtle half, and the reason this cannot be derived from `tainted`: taint is
    /// `echo_is_in_the_recording && !barged && !confident`, so **a barge-in sets `tainted` to
    /// false on audio Rich was audible for**. That utterance is then ADMITTED, becomes a turn,
    /// and is exactly the turn a later investigation would ask about. `rich_audible` records the
    /// observation without the escapes.
    #[test]
    fn an_admitted_utterance_records_whether_rich_was_audible_for_it() {
        use crate::vad::VAD_FRAME_SAMPLES;
        let loud = vec![0.2f32; VAD_FRAME_SAMPLES];
        let quiet = vec![0.0f32; VAD_FRAME_SAMPLES];

        // ---- (a) Rich silent throughout: admitted, and the record says so -----------------
        let mut brain = CaptureBrain::new();
        let mut out = Vec::new();
        for _ in 0..(ECHO_LOOKBACK_FRAMES + 5) {
            out.extend(brain.push_frame(&quiet, false, false));
        }
        for _ in 0..120 {
            out.extend(brain.push_frame(&loud, false, false));
        }
        for _ in 0..80 {
            out.extend(brain.push_frame(&quiet, false, false));
        }
        let admitted: Vec<bool> = out
            .iter()
            .filter_map(|m| match m {
                CapMsg::Utterance(u) => Some(u.rich_audible),
                _ => None,
            })
            .collect();
        assert_eq!(admitted, vec![false], "Rich was never audible and the record claims he was");

        // ---- (b) a BARGE-IN over Rich: admitted BECAUSE the taint was cleared, and the
        //          record must still say his voice was in the room ---------------------------
        //
        // **THE "TAP TO STOP" MUST LAND WHILE THE RECORDER IS ALREADY RUNNING**, and finding
        // that out was itself worth the test. Pressing it in silence clears `barged` again on
        // the SAME frame: the onset takes `SPEECH_ONSET_FRAMES` = 7 frames to confirm, so
        // `recording` is still false, and the chain's last arm (`!speaking && !recording`) runs
        // and forgets the interruption. Measured, not reasoned: a first attempt pressed it on
        // frame 1 and the utterance came back `Discarded { tainted: true }`.
        let mut brain = CaptureBrain::new();
        let mut out = Vec::new();
        for _ in 0..(ECHO_LOOKBACK_FRAMES + 5) {
            out.extend(brain.push_frame(&quiet, false, false));
        }
        // The CEO starts talking in a quiet room: born clean.
        for _ in 0..60 {
            out.extend(brain.push_frame(&loud, false, false));
        }
        // Rich starts answering over him, and he taps to stop — mid-utterance, which is the
        // only shape that keeps `barged` set.
        out.extend(brain.push_frame(&loud, true, true));
        for _ in 0..40 {
            out.extend(brain.push_frame(&loud, true, false));
        }
        for _ in 0..80 {
            out.extend(brain.push_frame(&quiet, false, false));
        }

        let started: Vec<(bool, bool)> = out
            .iter()
            .filter_map(|m| match m {
                CapMsg::Started { tainted, rich_audible } => Some((*tainted, *rich_audible)),
                _ => None,
            })
            .collect();
        assert_eq!(
            started,
            vec![(false, false)],
            "premise: the utterance must be born clean, in a quiet room"
        );
        assert!(
            out.iter().any(|m| matches!(m, CapMsg::BargeIn { mid_utterance: true })),
            "premise: the tap-to-stop must have registered WHILE recording, or `barged` is \
             cleared again and taint comes back"
        );
        let discarded = out.iter().filter(|m| matches!(m, CapMsg::Discarded { .. })).count();
        assert_eq!(discarded, 0, "premise: the barge-in must have ADMITTED this utterance");
        let admitted: Vec<bool> = out
            .iter()
            .filter_map(|m| match m {
                CapMsg::Utterance(u) => Some(u.rich_audible),
                _ => None,
            })
            .collect();
        assert_eq!(
            admitted,
            vec![true],
            "the one turn worth investigating recorded that Rich was NOT audible for it — this \
             is the case `tainted` alone cannot express, because the barge-in cleared it"
        );
    }

    /// The other write point: Rich becomes audible PARTWAY THROUGH an utterance that began in
    /// silence. `rich_audible` is re-evaluated for as long as the utterance is alive, with no
    /// conditions — unlike taint, which stops re-evaluating once `barged` or `confident`.
    #[test]
    fn rich_becoming_audible_mid_utterance_is_recorded_even_when_taint_is_cleared() {
        use crate::vad::VAD_FRAME_SAMPLES;
        let loud = vec![0.2f32; VAD_FRAME_SAMPLES];
        let quiet = vec![0.0f32; VAD_FRAME_SAMPLES];
        let mut brain = CaptureBrain::new();
        let mut out = Vec::new();

        // Quiet room; the CEO starts talking and the utterance is born clean.
        for _ in 0..(ECHO_LOOKBACK_FRAMES + 5) {
            out.extend(brain.push_frame(&quiet, false, false));
        }
        for _ in 0..60 {
            out.extend(brain.push_frame(&loud, false, false));
        }
        // He taps to stop WHILE talking, which sets `barged` for the rest of the utterance and
        // stops taint ever coming back — see the note in the test above about why the frame it
        // lands on matters.
        out.extend(brain.push_frame(&loud, false, true));
        for _ in 0..20 {
            out.extend(brain.push_frame(&loud, false, false));
        }
        // NOW Rich becomes audible, mid-sentence, with taint permanently cleared.
        for _ in 0..40 {
            out.extend(brain.push_frame(&loud, true, false));
        }
        for _ in 0..80 {
            out.extend(brain.push_frame(&quiet, false, false));
        }

        let started: Vec<(bool, bool)> = out
            .iter()
            .filter_map(|m| match m {
                CapMsg::Started { tainted, rich_audible } => Some((*tainted, *rich_audible)),
                _ => None,
            })
            .collect();
        assert_eq!(
            started,
            vec![(false, false)],
            "premise: the utterance must be born clean, with Rich inaudible"
        );
        let admitted: Vec<bool> = out
            .iter()
            .filter_map(|m| match m {
                CapMsg::Utterance(u) => Some(u.rich_audible),
                _ => None,
            })
            .collect();
        assert_eq!(
            admitted,
            vec![true],
            "Rich spoke over the middle of it and the record does not say so"
        );
    }

    /// **THE PROVENANCE REACHES `submit`, which is the only place it can reach the ledger.**
    /// By the time anything downstream holds the text, every fact about the audio is gone.
    #[test]
    fn the_provenance_travels_with_the_words_to_the_submit_callback() {
        let rec = Recorder::default();
        let mut desk = RecognizerDesk::new();
        let seen = Mutex::new(Vec::<(String, bool)>::new());

        desk.handle(
            &admitted(framed(synthetic_voice(1.5, 190.0, 130.0, -26.0), -55.0, 91), true),
            &rec,
            |_| Ok(("Stop counting.".to_string(), 300)),
            |t, rich| seen.lock().unwrap().push((t, rich)),
        );
        // POSITIVE CONTROL in the same test: the other value survives the same path, so this
        // cannot be passing on a hard-coded `true`.
        desk.handle(
            &admitted(framed(synthetic_voice(1.5, 190.0, 130.0, -26.0), -55.0, 93), false),
            &rec,
            |_| Ok(("Book the flight.".to_string(), 300)),
            |t, rich| seen.lock().unwrap().push((t, rich)),
        );

        assert_eq!(
            seen.into_inner().unwrap(),
            vec![
                ("Stop counting.".to_string(), true),
                ("Book the flight.".to_string(), false),
            ]
        );
    }

    /// **DEFECT 1 OF THE CANDIDATE-.6 WALK, REPLAYED: THREE CARDS BECOME ONE.**
    ///
    /// Ray's first-spoken-answer sequence, in the order his log and his screen recorded it
    /// (`docs/verification/2026-09-17-nightly-1.2.0-20260917.6-onscreen-audit.md`, frame
    /// `a6-08`, observation A at 21:31:28.632Z → 21:31:45.124Z):
    ///
    /// 1. The CEO speaks; the utterance is admitted and becomes a turn.
    /// 2. Rich answers aloud. Utterances born inside the audible window are tainted and
    ///    discarded — the supervisor's path — and the half-duplex line is said.
    /// 3. An admitted utterance from outside the window carries no voice: the pre-whisper gate
    ///    refuses it. **This was card 2.**
    /// 4. Its successor carries a voice and whisper returns `[BLANK_AUDIO]`: the post-whisper
    ///    filter refuses it. **This was card 3.**
    ///
    /// Both emitters spend ONE budget, so steps 3 and 4 are outranked by the card standing from
    /// step 2 and say nothing. **Exactly one card, and it is the honest half-duplex one.**
    #[test]
    fn rays_first_spoken_answer_produces_exactly_one_notice_card() {
        let rec = Recorder::default();
        let mut desk = RecognizerDesk::new();
        let budget = desk.notices();

        // 1. He speaks and is understood.
        desk.handle(
            &utterance(framed(synthetic_voice(1.6, 190.0, 130.0, -26.0), -55.0, 71)),
            &rec,
            |_| Ok(("Please count slowly out loud from 1 to 20.".to_string(), 300)),
            |_, _| {},
        );
        assert_eq!(notices(&rec).len(), 0);
        assert_eq!(messages(&rec).len(), 0);

        // 2. Rich answers; his own voice comes back and is discarded as taint. This is the
        //    supervisor's emitter, reproduced here verbatim from `supervise`'s
        //    `CapMsg::Discarded { tainted: true }` arm — the SAME shared budget, the SAME
        //    channel, the SAME sentence — because the defect is only visible when both
        //    emitters' output is counted together.
        let half = budget.lock().unwrap().half_duplex();
        assert_eq!(half, Some(VoiceNotice::CouldNotListenWhileSpeaking));
        rec.on_voice_event(&VoiceEvent::Notice {
            message: half.unwrap().ceo_message().to_string(),
            at: now_millis(),
        });

        // 3. Card 2's cause: an ADMITTED utterance that carried no voice. `hiss` never reaches
        //    whisper, which the panicking transcriber proves.
        desk.handle(&utterance(hiss(2.0, -40.0, 73)), &rec, |_| panic!("reached whisper"), |_, _| {});

        // 4. Card 3's cause: a voice whisper could not turn into words.
        desk.handle(
            &utterance(framed(synthetic_voice(2.3, 190.0, 130.0, -26.0), -55.0, 79)),
            &rec,
            |_| Ok(("[BLANK_AUDIO]".to_string(), 310)),
            |t, _| panic!("submitted: {t:?}"),
        );

        // ONE card in total, counting BOTH channels — `HeardNoVoice` rides `voice-error` and
        // the other two ride `voice-notice`, and a count of one channel would have missed the
        // defect entirely.
        let all: Vec<String> =
            notices(&rec).into_iter().chain(messages(&rec)).collect();
        assert_eq!(
            all.len(),
            1,
            "three apologies for one silence again — defect 1. cards: {all:#?}"
        );
        assert_eq!(all[0], VoiceNotice::CouldNotListenWhileSpeaking.ceo_message());
    }

    /// **THE "BEFORE" OF THE TEST ABOVE, KEPT AS A FACT RATHER THAN A CLAIM.**
    ///
    /// The test above asserts one card. On its own it would pass on an implementation that had
    /// simply stopped saying anything, and it would also pass if the three-card shape had never
    /// existed. This reproduces the shipped-in-candidate-.6 arrangement — three independent "at
    /// most once" latches, which is what three separate [`RefusalNotices`] are — and counts
    /// what a person saw: **three**.
    ///
    /// So the fix is the SHARING, not the latching. Each latch was already correct.
    #[test]
    fn three_independent_budgets_are_what_produced_three_cards() {
        let (mut a, mut b, mut c) =
            (RefusalNotices::new(), RefusalNotices::new(), RefusalNotices::new());
        let said: Vec<VoiceNotice> = [a.half_duplex(), b.heard_no_voice(), c.did_not_catch_that()]
            .into_iter()
            .flatten()
            .collect();
        assert_eq!(
            said,
            vec![
                VoiceNotice::CouldNotListenWhileSpeaking,
                VoiceNotice::HeardNoVoice,
                VoiceNotice::DidNotCatchThat,
            ],
            "the three-card shape this fix removes is not reproducible, so the fix proves nothing"
        );
        assert_eq!(said.len(), 3);

        // AND THE SAME THREE REQUESTS AGAINST ONE BUDGET: exactly one, the strongest.
        let mut one = RefusalNotices::new();
        let shared: Vec<VoiceNotice> =
            [one.half_duplex(), one.heard_no_voice(), one.did_not_catch_that()]
                .into_iter()
                .flatten()
                .collect();
        assert_eq!(shared, vec![VoiceNotice::CouldNotListenWhileSpeaking]);
        assert_eq!(shared.len(), 1, "3 -> 1 is the whole change");
    }

    /// POSITIVE CONTROL #1 for the test above: **a genuine admitted non-speech utterance still
    /// gets its one card.** Without this, the fix would be indistinguishable from switching the
    /// recognizer's two notices off, which is the silence audit-3 §4 #4 was filed to end.
    #[test]
    fn a_genuine_admitted_non_speech_utterance_still_says_its_one_line() {
        let rec = Recorder::default();
        let mut desk = RecognizerDesk::new();
        desk.handle(
            &utterance(framed(synthetic_voice(2.3, 190.0, 130.0, -26.0), -55.0, 83)),
            &rec,
            |_| Ok(("(clears throat)".to_string(), 310)),
            |t, _| panic!("submitted: {t:?}"),
        );
        assert_eq!(
            notices(&rec),
            vec![VoiceNotice::DidNotCatchThat.ceo_message().to_string()],
            "the one card a real refusal is owed went missing"
        );
        assert!(messages(&rec).is_empty());
    }

    /// POSITIVE CONTROL #2: **a second voice session says the half-duplex line again.** The
    /// latch is session-scoped, and a voice session is one [`RefusalNotices`]; nothing about
    /// this fix makes the line a once-per-install event.
    #[test]
    fn a_second_voice_session_says_the_half_duplex_line_again() {
        let mut first = RefusalNotices::new();
        assert!(first.half_duplex().is_some());
        assert_eq!(first.half_duplex(), None, "twice in one session");

        let mut second = RefusalNotices::new();
        assert_eq!(
            second.half_duplex(),
            Some(VoiceNotice::CouldNotListenWhileSpeaking),
            "a new voice session inherited a spent latch"
        );
    }

    /// POSITIVE CONTROL #3: **the 1 → 2 escalation inside a run survives the budget.** A run of
    /// three refusals must still hand over from the short line to the stronger one exactly once,
    /// which is why the rule is "strictly stronger replaces" rather than "one per session".
    #[test]
    fn the_stronger_line_still_takes_over_from_the_short_one_exactly_once() {
        let mut b = RefusalNotices::new();
        assert_eq!(b.did_not_catch_that(), Some(VoiceNotice::DidNotCatchThat));
        assert_eq!(b.did_not_catch_that(), None, "equal rank replaced what was standing");
        assert_eq!(b.heard_no_voice(), None, "the sibling at the same rank added a second card");
        assert_eq!(b.sound_but_no_words(), Some(VoiceNotice::SoundButNoWords), "no escalation");
        assert_eq!(b.sound_but_no_words(), None, "the stronger line became a drip");
        assert_eq!(b.did_not_catch_that(), None, "the short line came back after the strong one");
        // And the strongest of all still outranks the run line.
        assert_eq!(b.half_duplex(), Some(VoiceNotice::CouldNotListenWhileSpeaking));
        // NEGATIVE PROBE: nothing outranks the top of the family, so nothing follows it.
        assert_eq!(b.sound_but_no_words(), None);
        assert_eq!(b.heard_no_voice(), None);
        assert_eq!(b.standing(), Some(VoiceNotice::CouldNotListenWhileSpeaking));
    }

    /// **THE PREMISE CORRECTION, PINNED.** The obvious reading of three stacked cards is that
    /// echo-discarded audio raises all three, and it is wrong: suppressing the two recognizer
    /// notices "on the discard path" would suppress nothing, because a tainted discard never
    /// reaches the recognizer at all.
    ///
    /// The proof is the recorder that `handle` is never called for: `CaptureBrain` emits
    /// `CapMsg::Discarded { tainted: true }` and **drops** the `Utterance`, so the only value
    /// that could have carried that audio to whisper no longer exists. This drives the real
    /// capture brain over frames of Rich's own audible window and asserts the shape of what
    /// comes out.
    #[test]
    fn echo_discarded_audio_can_never_raise_a_recognizer_notice() {
        use crate::vad::VAD_FRAME_SAMPLES;
        let mut brain = CaptureBrain::new();
        let mut out = Vec::new();
        // Loud, obviously-voiced frames with Rich AUDIBLE and no canceller confidence: the exact
        // taint condition `speaking && !barged && !confident`.
        let loud = vec![0.2f32; VAD_FRAME_SAMPLES];
        let quiet = vec![0.0f32; VAD_FRAME_SAMPLES];
        for _ in 0..120 {
            out.extend(brain.push_frame(&loud, true, false));
        }
        // Silence long enough to close the utterance — SILENCE_HANGOVER_FRAMES is 50 (0.800 s),
        // so 80 frames is comfortably past it. `speaking` stays true: this is Rich's window.
        for _ in 0..80 {
            out.extend(brain.push_frame(&quiet, true, false));
        }

        let started_tainted: Vec<bool> = out
            .iter()
            .filter_map(|m| match m {
                CapMsg::Started { tainted, .. } => Some(*tainted),
                _ => None,
            })
            .collect();
        let discarded: Vec<bool> = out
            .iter()
            .filter_map(|m| match m {
                CapMsg::Discarded { tainted } => Some(*tainted),
                _ => None,
            })
            .collect();
        let admitted = out.iter().filter(|m| matches!(m, CapMsg::Utterance(_))).count();

        assert!(!started_tainted.is_empty(), "premise: no utterance was born at all");
        assert!(started_tainted.iter().all(|t| *t), "premise: the audio was not tainted");
        assert!(discarded.iter().any(|t| *t), "premise: nothing was discarded as taint");
        assert_eq!(
            admitted, 0,
            "a tainted utterance reached `utt_tx`, so it CAN reach whisper — the structural              claim this test exists to hold is broken and the notice suppression that rests on              it has to be rethought"
        );
    }

    /// POSITIVE CONTROL for the test above: the SAME audio with Rich silent IS admitted, and
    /// therefore does reach the recognizer. Without it, the assertion `admitted == 0` would pass
    /// just as well on a capture brain that admitted nothing whatsoever.
    #[test]
    fn the_same_audio_with_rich_silent_is_admitted_and_does_reach_the_recognizer() {
        use crate::vad::VAD_FRAME_SAMPLES;
        let mut brain = CaptureBrain::new();
        let mut out = Vec::new();
        let loud = vec![0.2f32; VAD_FRAME_SAMPLES];
        let quiet = vec![0.0f32; VAD_FRAME_SAMPLES];
        // Rich inaudible for longer than ECHO_LOOKBACK_FRAMES first, so the 0.416 s of pre-roll
        // the recorder reaches back through is clean too.
        for _ in 0..(ECHO_LOOKBACK_FRAMES + 5) {
            out.extend(brain.push_frame(&quiet, false, false));
        }
        for _ in 0..120 {
            out.extend(brain.push_frame(&loud, false, false));
        }
        for _ in 0..80 {
            out.extend(brain.push_frame(&quiet, false, false));
        }
        let admitted = out.iter().filter(|m| matches!(m, CapMsg::Utterance(_))).count();
        assert_eq!(admitted, 1, "the same audio with Rich silent was not admitted either");
    }

    /// **DEFECT 4: ENDING VOICE PRINTED NOTHING.** The closing line carries a span, and a span
    /// a reader cannot reconcile with the two timestamps beside it is worse than none. The
    /// arithmetic, re-derived here rather than trusted: 62 317 ms is 1 minute (60 000 ms) plus
    /// 2 317 ms, so `1:02.317`.
    #[test]
    fn the_session_span_reads_as_minutes_seconds_milliseconds() {
        assert_eq!(session_span(Duration::from_millis(62_317)), "1:02.317");
        assert_eq!(session_span(Duration::from_millis(0)), "0:00.000");
        // The candidate-.6 walk's own voice session: 21:30:2x to 21:36:26, call it 5:58.004.
        assert_eq!(session_span(Duration::from_millis(358_004)), "5:58.004");
        // 3 599 999 ms is one millisecond under an hour, and minutes do not wrap at 60.
        assert_eq!(session_span(Duration::from_millis(3_599_999)), "59:59.999");
        assert_eq!(session_span(Duration::from_millis(3_600_000)), "60:00.000");
    }

    /// INVARIANT: every notice in the enum is a DIFFERENT sentence. Four states that mean
    /// four different things about his microphone must not collapse into one.
    #[test]
    fn no_two_voice_notices_say_the_same_thing() {
        let all = [
            VoiceNotice::SoundButNoWords.ceo_message(),
            VoiceNotice::HeardNoVoice.ceo_message(),
            VoiceNotice::ReplyCutOff.ceo_message(),
            VoiceNotice::DidNotCatchThat.ceo_message(),
            VoiceNotice::CouldNotListenWhileSpeaking.ceo_message(),
        ];
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "two notices say the same thing");
                }
            }
        }
    }

    /// INVARIANT: the two refusal paths say DIFFERENT sentences, because they are different
    /// facts about the room. `HeardNoVoice` means no voice was measured at all; this one
    /// means a voice WAS measured and the words did not survive. Collapsing them would tell
    /// him his microphone is dead when it is working perfectly.
    #[test]
    fn the_two_refusal_paths_do_not_say_the_same_thing() {
        let caught = VoiceNotice::DidNotCatchThat.ceo_message();
        for other in [
            VoiceNotice::HeardNoVoice.ceo_message(),
            VoiceNotice::SoundButNoWords.ceo_message(),
            VoiceNotice::ReplyCutOff.ceo_message(),
        ] {
            assert_ne!(caught, other);
        }
        // House discipline, asserted rather than trusted: it closes the "was anything sent?"
        // question, says what to do about it (2026-10-21), and ends with the status.
        assert!(caught.contains("haven't sent anything"));
        assert!(caught.contains("Please say it again."), "{caught}");
        assert!(caught.ends_with("I'm still listening."));
        // NAMES NO MACHINERY — the same bar `the_silent_discard_notice_names_no_control_and_no
        // _machinery` holds `SoundButNoWords` to. The transcript that was thrown away is the
        // operator's `eprintln`, never his sentence.
        assert!(!caught.contains('/'), "{caught}");
        assert!(!caught.chars().any(|c| c.is_ascii_digit()), "{caught}");
        assert!(!caught.to_lowercase().contains("whisper"), "{caught}");
        assert!(!caught.to_lowercase().contains("transcri"), "{caught}");
    }

    /// INVARIANT: the two CEO-facing voice notices are different sentences answering
    /// different questions — "nothing was sent" versus "I heard you and got no words". A
    /// single shared line would tell him the wrong thing in one of the two cases.
    #[test]
    fn the_two_voice_notices_say_different_things() {
        let a = VoiceNotice::HeardNoVoice.ceo_message();
        let b = VoiceNotice::SoundButNoWords.ceo_message();
        assert_ne!(a, b);
        assert!(a.contains("haven't sent anything"), "the refusal must close the send question: {a}");
        assert!(a.contains("please say it again"), "the refusal must say what to do: {a}");
        assert!(!a.contains('/') && !a.contains("dBFS"), "machinery leaked to the CEO: {a}");
        assert!(!b.contains('/') && !b.contains("dBFS"), "machinery leaked to the CEO: {b}");
    }


    // =====================================================================================
    // ROW 3.30, ANSWER 1 — A VOICE TURN CUT MID-SENTENCE SAYS SO AUDIBLY
    // =====================================================================================

    /// **THE TEST THIS ROW EXISTS FOR.** The notice reaches the SPEAKER, not only the panel.
    ///
    /// In voice mode the CEO's eyes are not on the screen — that is what voice mode is. A
    /// notice that only raises a `VoiceEvent` leaves him listening to silence and deciding
    /// for himself whether Rich is thinking or gone, which is the failure the row describes
    /// in its own words: *"the CEO is speaking to a system that has stopped listening and
    /// does not know it."*
    ///
    /// The speak sink is a parameter precisely so this can be asserted. A test over
    /// `VoiceEvent` alone would stay green through the entire defect.
    #[test]
    fn a_cut_off_answer_is_spoken_aloud_and_not_only_shown() {
        let rec = Recorder::default();
        let spoken = Mutex::new(Vec::<String>::new());
        CutOffDesk::handle(None, &rec, |t| spoken.lock().unwrap().push(t.to_string()));

        let said = spoken.into_inner().unwrap();
        assert_eq!(
            said,
            vec![VoiceNotice::ReplyCutOff.ceo_message().to_string()],
            "the sentence has to be QUEUED FOR THE SPEAKER, not merely raised as an event"
        );
        assert_eq!(messages(&rec), vec![VoiceNotice::ReplyCutOff.ceo_message().to_string()]);
    }

    /// INVARIANT: the reason is spoken as its OWN sentence after the notice, and the panel
    /// gets both in one line.
    ///
    /// Two sinks, two shapes, on purpose: the ear takes sentences one at a time and the eye
    /// takes a paragraph at once.
    #[test]
    fn the_reason_is_spoken_after_the_notice_and_shown_beside_it() {
        let rec = Recorder::default();
        let spoken = Mutex::new(Vec::<String>::new());
        // The exact sentence `richos-core`'s `UpstreamFault::Overloaded.ceo_message()`
        // authors. Voice does not know where it came from and does not parse it.
        let reason = "Anthropic's servers are at capacity, so that request never reached \
                      Claude. This one ends when their capacity frees up and nothing on this \
                      machine brings it back sooner.";
        CutOffDesk::handle(Some(reason), &rec, |t| spoken.lock().unwrap().push(t.to_string()));

        let said = spoken.into_inner().unwrap();
        assert_eq!(said.len(), 2, "notice first, reason second: {said:?}");
        assert_eq!(said[0], VoiceNotice::ReplyCutOff.ceo_message());
        assert_eq!(said[1], reason);

        let shown = messages(&rec);
        assert_eq!(shown.len(), 1, "the panel gets ONE line");
        assert!(shown[0].starts_with(VoiceNotice::ReplyCutOff.ceo_message()));
        assert!(shown[0].contains("at capacity"));
    }

    /// POSITIVE CONTROL for the reason path: an ABSENT or blank reason produces exactly one
    /// spoken sentence and no empty utterance.
    ///
    /// Without this, `speak("")` would hand the synthesizer nothing and the CEO would hear a
    /// pause where a sentence should be — a silent failure inside the fix for a silent
    /// failure.
    #[test]
    fn a_missing_or_blank_reason_never_becomes_an_empty_utterance() {
        for reason in [None, Some(""), Some("   "), Some("\n")] {
            let rec = Recorder::default();
            let spoken = Mutex::new(Vec::<String>::new());
            CutOffDesk::handle(reason, &rec, |t| spoken.lock().unwrap().push(t.to_string()));
            let said = spoken.into_inner().unwrap();
            assert_eq!(said.len(), 1, "reason {reason:?} produced {said:?}");
            assert!(!said[0].trim().is_empty());
            assert_eq!(messages(&rec).len(), 1);
        }
    }

    /// INVARIANT: the cut-off notice is its own sentence, different from every other voice
    /// notice, and it carries no machinery.
    ///
    /// The three notices answer three different questions — "I got no words out of your
    /// audio", "nothing was sent", "the answer stopped". One shared line would tell him the
    /// wrong thing in two of the three cases.
    #[test]
    fn the_cut_off_notice_is_its_own_sentence_and_carries_no_machinery() {
        let cut = VoiceNotice::ReplyCutOff.ceo_message();
        for other in [VoiceNotice::SoundButNoWords.ceo_message(), VoiceNotice::HeardNoVoice.ceo_message()] {
            assert_ne!(cut, other);
        }
        // It states the CONSEQUENCE — what he heard is all there is — because the alternative
        // leaves him waiting for a sentence that is never coming.
        assert!(cut.contains("all I got out"), "the consequence is stated: {cut}");
        // And it ends on a status rather than an instruction, like the other two: the
        // affordance for asking again is the open microphone and there is no button.
        assert!(cut.contains("still listening"), "it ends with a status: {cut}");
        assert!(!cut.contains('/') && !cut.contains("dBFS"), "machinery leaked to the CEO: {cut}");
        assert!(!cut.contains("529") && !cut.contains("req_"), "operator detail is unspeakable: {cut}");
    }

    /// INVARIANT: an operator-shaped reason is spoken as it stands and NOT rewritten — but
    /// nothing in the product ever hands one here, and this test says which is which.
    ///
    /// `CutOffDesk` deliberately does not sanitize: a filter would be a second opinion about
    /// what is CEO-facing, and `upstream.rs` already owns that decision (its `summary()` is
    /// the operator's line and its `ceo_message()` is his). The guard that matters is on the
    /// CALLER, and it is asserted in `richos-core`'s own suites — `UpstreamRecord` keeps the
    /// request id in a field that the CEO view redacts, and `finish_upstream_failure` never
    /// puts `summary()` on a CEO path.
    #[test]
    fn the_desk_relays_the_reason_verbatim_and_owns_no_opinion_about_it() {
        let rec = Recorder::default();
        let spoken = Mutex::new(Vec::<String>::new());
        CutOffDesk::handle(Some("a sentence with EXACT wording"), &rec, |t| {
            spoken.lock().unwrap().push(t.to_string())
        });
        assert_eq!(spoken.into_inner().unwrap()[1], "a sentence with EXACT wording");
    }

    #[derive(Default)]
    struct Recorder {
        events: Mutex<Vec<VoiceEvent>>,
    }
    impl VoiceObserver for Recorder {
        fn on_voice_event(&self, event: &VoiceEvent) {
            self.events.lock().unwrap().push(event.clone());
        }
    }

    /// INVARIANT: three discarded utterances cannot happen faster than 3.312 s, so the
    /// silent-discard notice can never fire on a cough, a chair or one stray word — and it
    /// fires many times over inside the 25+ seconds of silent "listening…" that this notice
    /// exists to end.
    ///
    /// The two frame counts are `endpoint.rs`'s, and the arithmetic is redone here from the
    /// constants rather than quoted from its table: a comment that agrees with a number is
    /// not evidence that the number is right.
    #[test]
    fn three_silent_discards_take_at_least_three_and_a_third_seconds() {
        let min_speech = crate::endpoint::MIN_SPEECH_FRAMES as f32 * crate::vad::VAD_FRAME_SAMPLES as f32
            / crate::vad::SAMPLE_RATE as f32;
        let hangover = crate::endpoint::SILENCE_HANGOVER_FRAMES as f32 * crate::vad::VAD_FRAME_SAMPLES as f32
            / crate::vad::SAMPLE_RATE as f32;
        assert!((min_speech - 0.304).abs() < 1e-6, "19 x 256 / 16000 = 0.304, got {min_speech}");
        assert!((hangover - 0.800).abs() < 1e-6, "50 x 256 / 16000 = 0.800, got {hangover}");

        let one_utterance = min_speech + hangover;
        let floor = SILENT_DISCARD_RUN as f32 * one_utterance;
        assert!((one_utterance - 1.104).abs() < 1e-6, "one utterance floor: {one_utterance}");
        assert!((floor - 3.312).abs() < 1e-5, "three in a row: {floor}");

        // Below the run length it says nothing at all, which is the half that keeps a cough
        // quiet. Above it, one line — and 3.312 s fits inside the measured 25 s seven times.
        assert!(floor < 25.0);
        assert_eq!(SILENT_DISCARD_RUN, 3);
    }

    /// The notice explains the condition, keeps listening and asks for a repeat without
    /// inventing a control or exposing recognition machinery. The affordance suite treats
    /// the spoken repeat as the action, through the existing voice control.
    #[test]
    fn the_silent_discard_notice_names_no_control_and_no_machinery() {
        let m = VoiceNotice::SoundButNoWords.ceo_message();
        assert!(!m.contains('/'), "{m}");
        assert!(!m.chars().any(|c| c.is_ascii_digit()), "{m}");
        assert!(!m.to_lowercase().contains("whisper"), "{m}");
        assert!(!m.to_lowercase().contains("transcri"), "{m}");
        // What it DOES say: sound arrived, words did not, and voice was not switched off
        // behind his back.
        assert!(m.contains("hear sound"), "{m}");
        assert!(m.contains("I'm still listening"), "{m}");
        assert!(m.contains("Please say it again"), "{m}");
    }

    /// INVARIANT: the diagnostics line reports MEASURED device facts and the exact barge-in
    /// frame math — it is the line that goes in the brief, so it must not contain estimates.
    #[test]
    fn the_diagnostics_line_carries_the_exact_frame_math() {
        let d = Diagnostics {
            input_source: "microphone".into(),
            input_rate: 48_000,
            input_channels: 2,
            output_device: "External Headphones".into(),
            output_rate: 48_000,
            output_channels: 2,
            stt_model: "small.en".into(),
            stt_binary: "/opt/homebrew/bin/whisper-cli".into(),
            echo_gate: "PBFDAF 2048 taps (128 ms tail)".into(),
            echo_cancellation: true,
            barge_in_frames: BARGE_IN_DEBOUNCE_FRAMES,
            barge_in_secs: frames_to_secs(BARGE_IN_DEBOUNCE_FRAMES),
            barge_in_window_frames: crate::bargein::AEC_BARGE_IN_WINDOW_FRAMES,
            barge_in_required_frames: crate::bargein::AEC_BARGE_IN_REQUIRED_FRAMES,
            barge_in_window_secs: frames_to_secs(crate::bargein::AEC_BARGE_IN_WINDOW_FRAMES),
            barge_in_required_secs: frames_to_secs(crate::bargein::AEC_BARGE_IN_REQUIRED_FRAMES),
            barge_in_near_end_gated: true,
        };
        let s = d.summary();
        // Printed so `--nocapture` shows the line a reviewer would read at boot, rather than
        // leaving them to reassemble it from the assertions below.
        println!("{s}");

        // **WITH A CANCELLER PRESENT the line reports the rule that IS RUNNING, and that is the
        // near-end-gated window from the first frame.** These assertions used to require
        // `313 frames`, `5.008 s` and the words *"until the canceller proves itself"*, and they
        // were right about the code as it then stood and wrong about the CEO's rig: `confident()`
        // is unreachable there by measurement, so *"until"* named a wait that never ends and the
        // 5.008 s figure was the permanent behavior dressed as a start-up condition. Barge-in no
        // longer depends on confidence (`BargeInMonitor::set_near_end_gated`), so the line no
        // longer says it does.
        assert!(s.contains("15 of 25 frames"), "{s}");
        assert!(s.contains("0.240 s"), "{s}");
        assert!(s.contains("0.400 s"), "{s}");
        assert!(s.contains("near-end"), "{s}");
        assert!(
            !s.contains("until the canceller proves itself"),
            "the line still promises a convergence the CEO's hardware cannot reach: {s}"
        );
        assert!(
            !s.contains("5.008"),
            "the consecutive fallback is NOT what runs with a canceller present, so quoting it \
             here is the exact misreading audit-3 §4 #1 made: {s}"
        );

        // **AND WITH NO CANCELLER the 5.008 s rule is still the truth, so the line still says
        // it.** One flag, two lines, and this is the positive control that proves the flag is
        // actually consulted rather than the wording being hard-coded.
        let none = Diagnostics { barge_in_near_end_gated: false, echo_cancellation: false, ..d.clone() };
        let sn = none.summary();
        println!("{sn}");
        assert!(sn.contains("313 consecutive frames"), "{sn}");
        assert!(sn.contains("5.008 s"), "{sn}");
        assert!(sn.contains("NO canceller"), "{sn}");
        assert!(!sn.contains("15 of 25"), "{sn}");

        assert!(
            d.echo_cancellation,
            "echo cancellation SHIPS — this assertion used to read the other way and had \
             outlived the aec.rs landing"
        );
    }

    /// INVARIANT — **THE FRAME MATH, RE-DERIVED HERE RATHER THAN TRUSTED.** Every figure the
    /// boot line prints, and every figure audit-3 §4 #1 reasoned about, computed from the
    /// constants by the one helper: `frames × 256 ÷ 16000`, shown rather than asserted.
    ///
    /// This exists because the audit had to take two printed numbers on faith to write its
    /// finding, and one of the two was a fallback wearing a capability's clothes.
    #[test]
    fn every_number_the_boot_line_prints_is_the_frame_math_and_not_a_round_figure() {
        assert_eq!(crate::vad::SAMPLE_RATE, 16_000);
        assert_eq!(crate::vad::VAD_FRAME_SAMPLES, 256);

        // The fallback debounce, in force until the canceller earns better.
        assert_eq!(BARGE_IN_DEBOUNCE_FRAMES, 313);
        assert!((frames_to_secs(313) - 5.008).abs() < 1e-6, "{}", frames_to_secs(313));

        // What it shortens to, and how much of that window must be speech.
        assert_eq!(crate::bargein::AEC_BARGE_IN_WINDOW_FRAMES, 25);
        assert!((frames_to_secs(25) - 0.400).abs() < 1e-6);
        assert!(
            (frames_to_secs(crate::bargein::AEC_BARGE_IN_REQUIRED_FRAMES) - 0.240).abs() < 1e-6
        );

        // The filter, and the price of confidence: 2.000 s of warm-up, then 2.000 s held.
        assert_eq!(crate::aec::AEC_TAPS, 2048);
        assert!((crate::aec::filter_tail_secs() - 0.128).abs() < 1e-6);
        assert!(
            (crate::aec::blocks_to_secs(crate::aec::CONFIDENCE_WARMUP_BLOCKS) - 2.000).abs() < 1e-6
        );
        assert!(
            (crate::aec::blocks_to_secs(crate::aec::CONFIDENCE_HOLD_BLOCKS) - 2.000).abs() < 1e-6
        );
        assert!(
            (crate::aec::blocks_to_secs(
                crate::aec::CONFIDENCE_WARMUP_BLOCKS + crate::aec::CONFIDENCE_HOLD_BLOCKS
            ) - 4.000)
                .abs()
                < 1e-6,
            "confidence costs 4.000 s of Rich ACTUALLY SPEAKING, not 4.000 s of wall clock"
        );
    }

    /// INVARIANT — **THE QUESTION AUDIT-3 §4 #1 LEFT OPEN, ANSWERED WITHOUT A MICROPHONE.**
    ///
    /// The audit could not tell whether the CEO's sentence was cut only by the discard window
    /// or ALSO by an end-of-utterance cut, and named the experiment that would settle it:
    /// "one person speaking one uninterrupted sentence longer than four seconds while Rich is
    /// silent". It is an acoustic experiment for a fact that is not acoustic — the endpointer
    /// decides it, and it decides it the same way every time.
    ///
    /// **There is no four-second cut.** The only two limits an uninterrupted utterance can
    /// meet:
    ///   * `SILENCE_HANGOVER_FRAMES` =   50 × 256 ÷ 16000 =  0.800 s of SILENCE ends it, and
    ///     an uninterrupted sentence by definition never contains that much;
    ///   * `MAX_UTTERANCE_FRAMES`    = 1875 × 256 ÷ 16000 = 30.000 s, the ceiling.
    ///
    /// So a sentence "longer than four seconds" is not a special case at all, and the cut the
    /// CEO saw was the discard window and nothing else.
    #[test]
    fn nothing_cuts_an_uninterrupted_sentence_at_four_seconds() {
        let hangover = frames_to_secs(crate::endpoint::SILENCE_HANGOVER_FRAMES);
        let ceiling = frames_to_secs(crate::endpoint::MAX_UTTERANCE_FRAMES);
        assert!((hangover - 0.800).abs() < 1e-6, "{hangover}");
        assert!((ceiling - 30.000).abs() < 1e-6, "{ceiling}");
        // Nothing between the hangover and the ceiling ends an utterance that keeps
        // producing speech, so every length the audit wondered about survives.
        for secs in [4.1_f32, 5.0, 10.0, 29.0] {
            assert!(
                secs > hangover && secs < ceiling,
                "{secs} s would be cut, which would make the audit's worry real"
            );
        }
    }

    /// INVARIANT: a voice-start failure is a calm CEO line, whatever failed underneath.
    #[test]
    fn every_start_failure_has_a_calm_ceo_line() {
        let errs = [
            VoiceStartError::Capture(capture::CaptureError::NoInputDevice),
            VoiceStartError::Stt(stt::SttError::ModelNotFound("/x/ggml-small.en.bin".into())),
            VoiceStartError::Playout(crate::playout::PlayoutError::NoOutputDevice),
        ];
        for e in errs {
            let msg = e.ceo_message();
            assert!(!msg.is_empty());
            assert!(!msg.contains("ggml"), "machinery leaked: {msg}");
            assert!(!msg.contains('/'), "a path leaked to the CEO: {msg}");
        }
    }

    /// INVARIANT: the controller is `Send + Sync`, because it lives in Tauri managed state.
    /// cpal's macOS `Stream` is Send by construction in 0.17 (dedicated-thread architecture);
    /// if that ever changes this test fails at COMPILE time rather than at the toggle.
    #[test]
    fn the_controller_can_live_in_tauri_managed_state() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<VoiceController>();
        assert_send_sync::<Playout>();
    }

    /// INVARIANT: the generation counter is what makes a barge-in stick. A sentence
    /// synthesized across an interruption must be discarded, not played late.
    #[test]
    fn a_sentence_synthesised_across_a_barge_in_is_discarded() {
        let gen = AtomicU64::new(7);
        let queued_at = gen.load(Ordering::Relaxed);
        // …the CEO interrupts while `say` is running…
        gen.fetch_add(1, Ordering::Relaxed);
        assert_ne!(queued_at, gen.load(Ordering::Relaxed), "stale audio would have played");
    }

    /// LIVE (opt-in): a MEASURED barge-in on the real output device. Rich starts talking for
    /// real, gets cut, and the numbers are checked — dropped audio, an empty queue that stays
    /// empty, and a stop latency of one device callback period.
    ///
    /// Behind RICHOS_VOICE_LIVE_AUDIO=1 because this one is genuinely audible: about a second
    /// of Rich's voice comes out of the speakers before it is cut.
    ///
    /// **Reported `ignored`, never `ok`, when the opt-in is absent** — the gate used to be an
    /// early `return`, and a test that returns is reported `ok`. See `build.rs`.
    #[test]
    #[cfg(target_os = "macos")]
    #[cfg_attr(
        not(live_audio),
        ignore = "LIVE AUDIO: audible, ~1 s out of the speakers. RICHOS_VOICE_LIVE_AUDIO=1 to run."
    )]
    fn live_barge_in_actually_silences_the_real_output_device() {
        crate::live_audio::require_opt_in();
        let dir = std::env::temp_dir().join("richos-voice-barge-test");
        std::fs::create_dir_all(&dir).unwrap();
        // A silent "microphone": the capture path runs for real, it just hears nothing, so
        // this test isolates the INTERRUPTION from the recognition.
        let quiet = dir.join("quiet.wav");
        crate::wav::write_pcm16_mono(&quiet, &vec![0.0f32; 16_000], crate::vad::SAMPLE_RATE).unwrap();

        let observer = Arc::new(Recorder::default());
        let ctl = VoiceController::start(
            VoiceOptions { source: AudioSource::Wav(quiet), scratch_dir: dir.clone() },
            observer.clone(),
            Arc::new(|_t: String, _: bool| {}),
        )
        .expect("voice mode should start");

        // Rich says something long enough to be worth interrupting.
        ctl.turn_started();
        ctl.speak_delta(
            "Three things this morning. Acme came back on the renegotiation and the number is \
             softer than it looks. Finance found a gap in the Q4 forecast. Partnerships want a \
             call about the economics before Thursday. ",
        );
        ctl.speak_end();
        ctl.turn_ended();

        // Wait until there is real audio worth interrupting. The FIRST sentence alone is only
        // ~1.3 s — the queue keeps growing behind it because synthesis outruns playback
        // (rtf ~0.074), which is the pipelining working, so wait for the depth rather than
        // for the first sample.
        let deadline = Instant::now() + Duration::from_secs(15);
        while ctl.queued_speech_secs() <= 2.0 && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let queued_before = ctl.queued_speech_secs();
        assert!(queued_before > 2.0, "Rich never got going: {queued_before:.3} s queued");

        // Let him actually be audible for a beat, then cut him.
        std::thread::sleep(Duration::from_millis(900));
        let cut_at = Instant::now();
        let dropped = ctl.force_barge_in();
        let cut_took = cut_at.elapsed();

        let rate = ctl.diagnostics().output_rate as f32;
        let dropped_secs = dropped as f32 / rate;
        let stop_latency = ctl.stop_latency_secs();
        println!(
            "barge-in: dropped {} samples = {:.3} s of Rich; call took {:.4} s; device stop latency {:.4} s ({} frames @ {} Hz)",
            dropped,
            dropped_secs,
            cut_took.as_secs_f32(),
            stop_latency,
            (stop_latency * rate).round() as u32,
            rate as u32
        );

        assert!(dropped_secs > 1.0, "nothing meaningful was cut: {dropped_secs:.3} s");
        assert!(
            stop_latency > 0.0 && stop_latency < 0.100,
            "stop latency implausible: {stop_latency:.4} s"
        );
        // The device must genuinely fall silent and STAY silent — a queue that refills would
        // mean a sentence synthesized across the cut still got through.
        std::thread::sleep(Duration::from_millis(600));
        assert_eq!(ctl.queued_speech_secs(), 0.0, "Rich started talking again after being cut");
        assert_eq!(ctl.state(), VoiceState::Listening, "state did not return to listening");

        drop(ctl);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// LIVE: the full local loop with audio INJECTED at the capture source — real VAD, real
    /// endpointing, real whisper.cpp, real `say`, real output device. The only thing this
    /// does not exercise is the microphone driver, which this machine does not have (see
    /// capture.rs). Rich's "reply" here is a fixed string, so the Claude leg is out of
    /// scope for a unit test; `voice_loop` (the example) does that end of it.
    ///
    /// **Reported `ignored`, never `ok`, when the opt-in is absent** — the gate used to be an
    /// early `return`, and a test that returns is reported `ok`. See `build.rs`.
    #[test]
    #[cfg(target_os = "macos")]
    #[cfg_attr(
        not(live_audio),
        ignore = "LIVE AUDIO: needs an output device AND whisper.cpp. RICHOS_VOICE_LIVE_AUDIO=1 to run."
    )]
    fn live_injected_audio_completes_the_whole_local_loop() {
        crate::live_audio::require_opt_in();
        let dir = std::env::temp_dir().join("richos-voice-loop-test");
        std::fs::create_dir_all(&dir).unwrap();
        let wav_path = dir.join("ceo.wav");

        // Synthesize a "CEO" utterance with a DIFFERENT voice than Rich's, at 16 kHz.
        let out = std::process::Command::new("/usr/bin/say")
            .args(["-v", "Samantha", "-o"])
            .arg(&wav_path)
            .arg("--data-format=LEI16@16000")
            .arg("Rich, what is the status of the voice pipeline today?")
            .output()
            .expect("say");
        assert!(out.status.success());

        let observer = Arc::new(Recorder::default());
        let heard: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let heard2 = heard.clone();
        let ctl = VoiceController::start(
            VoiceOptions { source: AudioSource::Wav(wav_path), scratch_dir: dir.clone() },
            observer.clone(),
            Arc::new(move |text: String, _rich_audible: bool| {
                heard2.lock().unwrap().push(text);
            }),
        )
        .expect("voice mode should start with an injected source");

        // 3.1 s of audio + 0.800 s hangover + ~0.5 s whisper.
        std::thread::sleep(Duration::from_millis(6000));
        let said = heard.lock().unwrap().clone();
        assert!(!said.is_empty(), "the loop never produced a transcript");
        assert!(
            said[0].to_lowercase().contains("voice pipeline"),
            "transcript did not match what was spoken: {said:?}"
        );

        // Now Rich answers, streamed as deltas exactly as the spine does.
        ctl.turn_started();
        ctl.speak_delta("Good morning. ");
        ctl.speak_delta("The voice pipeline is up. ");
        ctl.speak_end();
        ctl.turn_ended();
        std::thread::sleep(Duration::from_millis(1500));
        assert!(ctl.queued_speech_secs() > 0.0 || ctl.state() == VoiceState::Speaking,
            "Rich never produced audio");
        drop(ctl);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// **LIVE, END TO END: THE 2026-09-04 DEFECT, REPRODUCED AND THEN REFUSED.**
    ///
    /// Everything above proves a piece. This drives the WHOLE pipeline — real capture path,
    /// real VAD, real endpointer, real recognizer thread, real submit callback — and proves
    /// both directions on the same run:
    ///
    /// 1. **The failure, reproduced.** 12.000 s of -30 dBFS room hiss. That is 6.5 dB above
    ///    the VAD's initial threshold (`max(0.005 x 3, 0.005)` = 0.015 = -36.48 dBFS), so
    ///    every frame reads as speech until the 625-frame (10.000 s) stuck-floor escape lets
    ///    the floor learn the room — and then the 50-frame (0.800 s) hangover closes an
    ///    utterance of roughly 10.8 s. That utterance is EXACTLY what published v1.0.1 would
    ///    have handed to whisper and submitted under the CEO's name. It must produce no turn,
    ///    and it must say so.
    /// 2. **The fix not being a mute button.** A real `say` utterance through the identical
    ///    path must still land as a turn with the right words in it.
    ///
    /// Opt-in (`RICHOS_VOICE_LIVE_AUDIO=1`) because it opens a real output device and takes
    /// ~20 s of wall clock. It never plays anything: no `speak_delta` is called, so the test
    /// is silent even on a machine with speakers.
    ///
    /// **Reported `ignored`, never `ok`, when the opt-in is absent** — the gate used to be an
    /// early `return`, and a test that returns is reported `ok`. That mattered more here than
    /// anywhere else in the crate: this is the test that holds the 2026-09-04 defect shut, and
    /// it was printing `ok` without running on every machine but one. See `build.rs`.
    #[test]
    #[cfg(target_os = "macos")]
    #[cfg_attr(
        not(live_audio),
        ignore = "LIVE AUDIO: needs an output device AND whisper.cpp, ~20 s. RICHOS_VOICE_LIVE_AUDIO=1 to run."
    )]
    fn live_a_silent_channel_produces_no_turn_and_a_real_utterance_still_does() {
        crate::live_audio::require_opt_in();
        let dir = std::env::temp_dir().join("richos-voice-silent-channel-test");
        std::fs::create_dir_all(&dir).unwrap();

        // ---- 1. the quiet room that v1.0.1 spoke for him from -----------------------------
        let room = dir.join("quiet-room.wav");
        crate::wav::write_pcm16_mono(&room, &hiss(12.0, -30.0, 4_242), crate::vad::SAMPLE_RATE)
            .expect("write the room fixture");

        let observer = Arc::new(Recorder::default());
        let sent: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = sent.clone();
        let ctl = VoiceController::start(
            VoiceOptions { source: AudioSource::Wav(room), scratch_dir: dir.clone() },
            observer.clone(),
            Arc::new(move |text: String, _: bool| sink.lock().unwrap().push(text)),
        )
        .expect("voice mode should start with an injected source");
        // 12.000 s of audio + 0.800 s hangover + the gate; generous, because a false pass
        // here would be a test that agreed with the bug.
        std::thread::sleep(Duration::from_millis(16_000));
        let said = sent.lock().unwrap().clone();
        assert!(said.is_empty(), "a silent channel was submitted as the CEO's message: {said:?}");
        let notices = messages(&observer);
        assert!(
            notices.contains(&VoiceNotice::HeardNoVoice.ceo_message().to_string()),
            "the utterance either never reached the desk (so this test proves nothing) or was \
             dropped silently. Notices seen: {notices:?}"
        );
        drop(ctl);

        // ---- 2. and a real utterance still gets through ------------------------------------
        let spoken = dir.join("ceo.wav");
        let out = std::process::Command::new("/usr/bin/say")
            .args(["-v", "Samantha", "-o"])
            .arg(&spoken)
            .arg("--data-format=LEI16@16000")
            .arg("Rich, what is the status of the voice pipeline today?")
            .output()
            .expect("say");
        assert!(out.status.success());

        let observer2 = Arc::new(Recorder::default());
        let sent2: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let sink2 = sent2.clone();
        let ctl2 = VoiceController::start(
            VoiceOptions { source: AudioSource::Wav(spoken), scratch_dir: dir.clone() },
            observer2.clone(),
            Arc::new(move |text: String, _: bool| sink2.lock().unwrap().push(text)),
        )
        .expect("voice mode should start with an injected source");
        std::thread::sleep(Duration::from_millis(8_000));
        let said2 = sent2.lock().unwrap().clone();
        assert!(!said2.is_empty(), "the gate swallowed a real utterance — it is a mute button");
        assert!(
            said2[0].to_lowercase().contains("voice pipeline"),
            "transcript did not match what was spoken: {said2:?}"
        );
        drop(ctl2);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Turning voice off must close the microphone before it waits for queued work.
    #[test]
    fn the_microphone_is_released_before_the_workers_are_joined() {
        struct Mic(Arc<AtomicBool>);
        impl Drop for Mic {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let released = Arc::new(AtomicBool::new(false));
        let seen_by_worker = Arc::new(AtomicBool::new(false));
        let (r, seen) = (released.clone(), seen_by_worker.clone());
        // A worker that is "mid-turn" until the microphone is gone; joining first would hang.
        let worker = std::thread::spawn(move || {
            // load-bound: the deadline only bounds a hang; the verdict is the released flag, never elapsed time
            let deadline = Instant::now() + Duration::from_secs(5);
            while !r.load(Ordering::SeqCst) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
            seen.store(r.load(Ordering::SeqCst), Ordering::SeqCst);
        });
        release_then_join(Mic(released), vec![worker]);
        assert!(seen_by_worker.load(Ordering::SeqCst), "the worker finished before the microphone was released");
    }
}

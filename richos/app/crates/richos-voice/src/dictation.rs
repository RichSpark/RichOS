//! **Dictation in any app: every rule, with no device, no AppKit and no clock.**
//!
//! The dictation tool (`richos --richos-dictation`, `src-tauri/src/dictation/`) catches one key,
//! records, transcribes with the model the person chose and pastes at the cursor of whatever app
//! is in front. Every DECISION it makes lives here, so each one is proved by a unit test the way
//! `vad.rs` and `noaudio.rs` are: which key events are his key, the session's phases, the
//! too-short tap, the speech-evidence gate, the annotation rule, the no-audio-with-words rule,
//! the decode bound, which model runs, the spacing rule, the clipboard-restore rule and the
//! no-text-box rule. The tool's macOS edges are thin and decide nothing.
//!
//! Built to `richos-hq/docs/plans/2026-10-08-dictation-anywhere.md` (revision 2), section 2,
//! section 3 and section 5; the problem names match round 19's drawn bar lines
//! (`design/mockups/rounds/round-19/dictation.html`, `PROBLEMS`).

use crate::noaudio::{NoAudioDetector, LIVE_RMS};
use crate::vad::{rms, SAMPLE_RATE};
use crate::voiced::VoiceEvidence;
use std::time::Duration;

// =============================================================================================
// THE KEY
// =============================================================================================

/// The key a fresh install uses: F1, his habit ("tap F1 once, speak, tap it again").
pub const DEFAULT_KEY: u8 = 1;

/// `kVK_F1` .. `kVK_F19` (`HIToolbox/Events.h`), indexed by F-number minus one. The plain
/// function-key codes: what a key sends with fn held, with "Use F1, F2, etc. as standard function
/// keys" on, or from a non-Apple keyboard in function-key mode.
const F_KEY_CODES: [u16; 19] = [
    122, 120, 99, 118, 96, 97, 98, 100, 101, 109, 103, 111, // F1 .. F12
    105, 107, 113, 106, 64, 79, 80, // F13 .. F19
];

/// `kVK_F13`. What his Keychron's F1 reaches macOS as through his Karabiner rule, and what
/// open-wispr listens for today. F13 does nothing on macOS, so F1 also accepts it and his Mac
/// needs no change at all (plan section 3, the table's last row).
pub const KEY_CODE_F13: u16 = 105;

/// `NX_SUBTYPE_AUX_CONTROL_BUTTONS`: the system-defined event subtype the Apple top row's
/// brightness and media keys arrive as.
pub const AUX_CONTROL_BUTTONS: i16 = 8;

/// `NX_KEYTYPE_*` (`IOKit/hidsystem/ev_keymap.h`) for the top-row keys whose code is the same on
/// every Apple keyboard. F3 to F6 differ between keyboard generations, so they are matched by
/// their plain code only (plan section 3).
const TOP_ROW: [(u8, &[i64]); 8] = [
    (1, &[3]),       // NX_KEYTYPE_BRIGHTNESS_DOWN
    (2, &[2]),       // NX_KEYTYPE_BRIGHTNESS_UP
    (7, &[18, 20]),  // NX_KEYTYPE_PREVIOUS, NX_KEYTYPE_REWIND
    (8, &[16]),      // NX_KEYTYPE_PLAY
    (9, &[17, 19]),  // NX_KEYTYPE_NEXT, NX_KEYTYPE_FAST
    (10, &[7]),      // NX_KEYTYPE_MUTE
    (11, &[1]),      // NX_KEYTYPE_SOUND_DOWN
    (12, &[0]),      // NX_KEYTYPE_SOUND_UP
];

/// The plain key code of F`n`, or `None` outside F1 to F19.
pub fn f_key_code(n: u8) -> Option<u16> {
    (1..=19).contains(&n).then(|| F_KEY_CODES[n as usize - 1])
}

/// One event as the tap sees it, reduced to the three numbers the match reads. Nothing else of
/// a keystroke is ever looked at, kept or logged (plan section 3, "Privacy of the tap").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    /// `kCGEventKeyDown` / `kCGEventKeyUp`: the key code and whether it is an auto-repeat.
    Key { code: u16, down: bool, repeat: bool },
    /// `NSEventTypeSystemDefined`: its subtype and its `data1` word.
    System { subtype: i16, data1: i64 },
}

impl KeyEvent {
    /// A top-row key as the system-defined event a physical Apple keyboard sends: `data1` holds
    /// the key type in its high 16 bits, and the key state (0x0A down, 0x0B up) and the repeat
    /// bit in its low 16. The inverse of what [`judge_key`] reads, for tests and for the guest's
    /// `post_key` example.
    pub fn top_row(key_type: i64, down: bool, repeat: bool) -> KeyEvent {
        let state: i64 = if down { 0x0A } else { 0x0B };
        KeyEvent::System { subtype: AUX_CONTROL_BUTTONS, data1: (key_type << 16) | (state << 8) | i64::from(repeat) }
    }
}

/// What the tap does with one event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyVerdict {
    /// His key went down (not an auto-repeat): start or finish a dictation, and swallow it.
    Toggle,
    /// His key, but its key-up or an auto-repeat: swallow it, act on nothing. So the app in
    /// front never sees F1 and the screen brightness does not change.
    Swallow,
    /// Not his key: returned untouched.
    Pass,
}

/// **Is this event the chosen key `f` (an F-number)?** Every event that physical key can produce
/// on a Mac keyboard matches: its plain code, F13 when the key is F1, and for F1, F2 and F7 to
/// F12 the top-row system-defined event.
pub fn judge_key(f: u8, event: &KeyEvent) -> KeyVerdict {
    let Some(code) = f_key_code(f) else { return KeyVerdict::Pass };
    let (ours, down, repeat) = match *event {
        KeyEvent::Key { code: c, down, repeat } => (c == code || (f == 1 && c == KEY_CODE_F13), down, repeat),
        KeyEvent::System { subtype, data1 } => {
            if subtype != AUX_CONTROL_BUTTONS {
                return KeyVerdict::Pass;
            }
            let key_type = (data1 >> 16) & 0xFFFF;
            let flags = data1 & 0xFFFF;
            let state = (flags >> 8) & 0xFF;
            let types = TOP_ROW.iter().find(|(n, _)| *n == f).map(|(_, t)| *t).unwrap_or(&[]);
            (types.contains(&key_type) && (state == 0x0A || state == 0x0B), state == 0x0A, flags & 1 == 1)
        }
    };
    match (ours, down && !repeat) {
        (false, _) => KeyVerdict::Pass,
        (true, true) => KeyVerdict::Toggle,
        (true, false) => KeyVerdict::Swallow,
    }
}

/// **Key capture: which F-key did he just press?** (plan section 2 row 11, "Press a different
/// key"; the app asks the tool over the socket, and the tool's tap answers with this instead of
/// acting on the key, so an Apple top-row key is captured as the key it is.)
///
/// Read on a key-down that is not an auto-repeat; anything else is `None`. A plain function-key
/// code is the F-key it names, F13 included (only a CHOSEN F1 also answers to F13; a key he
/// presses here is recorded as itself). A top-row system-defined event is the F-key it sits on:
/// F1, F2 and F7 to F12, from the same table [`judge_key`] matches with, so a key captured here
/// is always a key the tap then matches. Every other key is `None`: the window, which sees it
/// too, says why it cannot be used.
pub fn captured_key(event: &KeyEvent) -> Option<u8> {
    match *event {
        KeyEvent::Key { code, down: true, repeat: false } => {
            F_KEY_CODES.iter().position(|c| *c == code).map(|i| i as u8 + 1)
        }
        KeyEvent::System { subtype, data1 } if subtype == AUX_CONTROL_BUTTONS => {
            let key_type = (data1 >> 16) & 0xFFFF;
            let flags = data1 & 0xFFFF;
            let down = (flags >> 8) & 0xFF == 0x0A;
            if !down || flags & 1 == 1 {
                return None;
            }
            TOP_ROW.iter().find(|(_, types)| types.contains(&key_type)).map(|(n, _)| *n)
        }
        _ => None,
    }
}

// =============================================================================================
// THE SESSION
// =============================================================================================

/// A tap shorter than this is not a dictation: "I didn't catch anything." (plan section 2,
/// "the too-short tap (under 450 ms)"; round 19 `stop()`, `if (dur < 450)`).
pub const SHORT_TAP_MS: u64 = 450;

/// The problems a dictation can end in, named for round 19's drawn bar lines (`PROBLEMS` in
/// `dictation.html`) and for the two lines Iris draws in slice 0. The tool reports the name; the
/// bar (slice 3) says the sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// "I didn't catch anything. Tap F1 and talk." A too-short tap, audio with no measured
    /// voice, or a transcript with nothing meaningful in it.
    DidNotCatch,
    /// "I can't hear anything. Check that your microphone is on."
    NoSound,
    /// "No text box was selected, so I copied your words. Press ⌘V to paste them."
    NoTextBox,
    /// "I need the microphone to hear you."
    NoMicrophone,
    /// "I can't type into other apps yet, so I copied your words."
    NoAccessibility,
    /// Iris's slice 0 line: neither speech model is downloaded yet.
    ModelMissing,
    /// Iris's slice 0 line: the words could not be written down (whisper-cli failed or passed
    /// its bound).
    CouldNotWrite,
    /// "Voice mode is still listening, so I didn't start. Tap F1 again in a moment." An app did
    /// not answer will-listen within the bound: the microphone stays closed (the handover fails
    /// closed, never open), and the press is dropped.
    VoiceStillListening,
}

impl Problem {
    /// The stable name in the tool's `state` message and in `dictation.log`.
    pub fn tag(self) -> &'static str {
        match self {
            Problem::DidNotCatch => "did-not-catch",
            Problem::NoSound => "no-sound",
            Problem::NoTextBox => "no-text-box",
            Problem::NoMicrophone => "no-microphone",
            Problem::NoAccessibility => "no-accessibility",
            Problem::ModelMissing => "model-missing",
            Problem::CouldNotWrite => "could-not-write",
            Problem::VoiceStillListening => "voice-still-listening",
        }
    }
}

/// Where a dictation is. Idle, listening, writing, done, problem: the five phases of plan
/// section 1, and of round 19's bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Listening { started_ms: u64 },
    Writing,
    Done,
    Problem(Problem),
}

impl Phase {
    pub fn tag(self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Listening { .. } => "listening",
            Phase::Writing => "writing",
            Phase::Done => "done",
            Phase::Problem(_) => "problem",
        }
    }
}

/// What one tap of his key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapAction {
    /// Open the microphone. Voice mode is told first (`will-listen`), so the two never listen
    /// at once (plan section 2).
    StartListening,
    /// Close the microphone and judge what was heard.
    StopListening { duration_ms: u64 },
    /// The words are being written down: a tap now does nothing (round 19 `tap()`, `if
    /// (D.phase === "write") return;`).
    Ignore,
}

/// The session's state machine: which phase, and what a tap does in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Session {
    phase: Phase,
}

impl Default for Session {
    fn default() -> Self {
        Session { phase: Phase::Idle }
    }
}

impl Session {
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// One tap at `now_ms`. From idle, done or a problem it starts listening; while listening it
    /// stops; while writing it is ignored.
    pub fn tap(&mut self, now_ms: u64) -> TapAction {
        match self.phase {
            Phase::Writing => TapAction::Ignore,
            Phase::Listening { started_ms } => {
                self.phase = Phase::Writing;
                TapAction::StopListening { duration_ms: now_ms.saturating_sub(started_ms) }
            }
            Phase::Idle | Phase::Done | Phase::Problem(_) => {
                self.phase = Phase::Listening { started_ms: now_ms };
                TapAction::StartListening
            }
        }
    }

    /// The microphone stopped delivering anything while listening ([`Recording::no_audio`]).
    /// Listening ends as a second tap would end it.
    pub fn input_died(&mut self, now_ms: u64) -> Option<u64> {
        match self.phase {
            Phase::Listening { started_ms } => {
                self.phase = Phase::Writing;
                Some(now_ms.saturating_sub(started_ms))
            }
            _ => None,
        }
    }

    /// The dictation ended: words in place, or a problem.
    pub fn finish(&mut self, outcome: Result<(), Problem>) {
        self.phase = match outcome {
            Ok(()) => Phase::Done,
            Err(p) => Phase::Problem(p),
        };
    }

    /// Back to idle (the bar went away).
    pub fn reset(&mut self) {
        self.phase = Phase::Idle;
    }
}

/// The audio of one dictation, as it arrives in exact 16 kHz frames, with the two facts about it
/// the stop decision needs: whether the input ever proved live, and whether it has gone dead.
///
/// The same [`NoAudioDetector`] voice mode uses, fed the RMS of the same frames that are
/// buffered (collector-path parity, `noaudio.rs`).
#[derive(Debug, Default)]
pub struct Recording {
    samples: Vec<f32>,
    any_live: bool,
    detector: NoAudioDetector,
}

impl Recording {
    pub fn new() -> Self {
        Recording::default()
    }

    /// One exact frame. Returns `true` the moment the input is judged dead (3.008 s of digital
    /// silence, `NO_AUDIO_FRAMES`), once.
    pub fn push(&mut self, frame: &[f32]) -> bool {
        self.samples.extend_from_slice(frame);
        let level = rms(frame);
        if level >= LIVE_RMS {
            self.any_live = true;
        }
        let changed = self.detector.observe(level, false);
        changed && self.detector.no_audio()
    }

    pub fn no_audio(&self) -> bool {
        self.detector.no_audio()
    }

    pub fn any_live(&self) -> bool {
        self.any_live
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// A borrowed bounded preview window. The full recording remains untouched
    /// for final recognition; the owner makes at most one short-lived copy.
    pub fn preview_samples(&self) -> &[f32] {
        &self.samples[self.samples.len().saturating_sub(8 * SAMPLE_RATE as usize)..]
    }

    pub fn into_samples(self) -> Vec<f32> {
        self.samples
    }

    pub fn secs(&self) -> f32 {
        self.samples.len() as f32 / SAMPLE_RATE as f32
    }
}

/// What to do with a finished recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    /// Transcribe it and write the words; afterwards, if `then` is set, show that problem
    /// ("Problems never lose words", round 19 NOTES: words said before the input died are
    /// written first, then the bar says it cannot hear anything).
    Transcribe { then: Option<Problem> },
    /// Nothing to write: show this problem. No transcript is ever produced.
    Refuse(Problem),
}

/// **THE STOP DECISION, before any transcript exists** (plan section 2 and Frank's M5).
///
/// - The input died with no live frame ever: "I can't hear anything", nothing transcribed.
/// - Shorter than [`SHORT_TAP_MS`]: "I didn't catch anything".
/// - No measured voice ([`VoiceEvidence`], `voiced.rs`): "I didn't catch anything", or "I can't
///   hear anything" when the input also died. This is the gate voice mode applies before
///   whisper ever runs (`controller.rs`, `RecognizerDesk::handle`), and the reason "you" and
///   "thank you" are not on the noise list: a tap, a pause to think and a tap would otherwise
///   paste "Thank you." into his email.
/// - Otherwise transcribe, and when the input died after words, say so afterwards.
pub fn judge_recording(duration_ms: u64, recording: &Recording) -> Stopped {
    judge(duration_ms, recording.no_audio(), recording.any_live(), || {
        VoiceEvidence::measure(recording.samples()).carried_speech()
    })
}

fn judge(duration_ms: u64, no_audio: bool, any_live: bool, carried_speech: impl FnOnce() -> bool) -> Stopped {
    if no_audio && !any_live {
        return Stopped::Refuse(Problem::NoSound);
    }
    if duration_ms < SHORT_TAP_MS {
        return Stopped::Refuse(Problem::DidNotCatch);
    }
    if !carried_speech() {
        return Stopped::Refuse(if no_audio { Problem::NoSound } else { Problem::DidNotCatch });
    }
    Stopped::Transcribe { then: no_audio.then_some(Problem::NoSound) }
}

/// **THE WORDS TO PASTE**, from whisper's cleaned stdout, or "I didn't catch anything".
///
/// `[BLANK_AUDIO]`, `(upbeat music)` and every other bracketed or parenthesized annotation is
/// removed first: `clean_transcript` leaves them in, and a dictation with a pause in it would
/// otherwise paste `[BLANK_AUDIO]` into Mail (Frank's M5). Then the narrow noise filter voice
/// mode uses ([`crate::stt::is_meaningful`]).
pub fn judge_transcript(transcript: &str) -> Result<String, Problem> {
    let words = crate::stt::strip_annotations(transcript).split_whitespace().collect::<Vec<_>>().join(" ");
    if !crate::stt::is_meaningful(&words) {
        return Err(Problem::DidNotCatch);
    }
    Ok(words)
}

/// The fixed part of the decode bound.
pub const DECODE_BOUND_BASE: Duration = Duration::from_secs(60);

/// **How long whisper-cli may take**: 60 s plus 1.5 times the recording's length, so a long
/// dictation is never cut by the bound (Frank's minor 6; plan section 11, "No time limit on a
/// dictation"). A 10-minute dictation gets 60 + 900 = 960 s, 16 minutes.
pub fn decode_bound(samples: usize) -> Duration {
    let millis = samples as u64 * 1000 / u64::from(SAMPLE_RATE);
    DECODE_BOUND_BASE + Duration::from_millis(millis * 3 / 2)
}

// =============================================================================================
// WHICH MODEL RUNS
// =============================================================================================

/// More accurate, the default: `large-v3-turbo-q5_0`, the transcription model setup installs.
pub const MORE_ACCURATE: &str = crate::stt::TRANSCRIPTION_MODEL_ID;
/// Faster: `small.en`.
pub const FASTER: &str = crate::stt::FALLBACK_MODEL_ID;

/// Which model one dictation uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelPick {
    /// The one he chose.
    Chosen(String),
    /// His choice is not verified yet (the background download is still running), so the other
    /// one, which is. The log says so.
    Fallback { wanted: String, using: String },
    /// Neither is: Iris's model-missing line.
    Missing,
}

impl ModelPick {
    pub fn id(&self) -> Option<&str> {
        match self {
            ModelPick::Chosen(id) | ModelPick::Fallback { using: id, .. } => Some(id),
            ModelPick::Missing => None,
        }
    }
}

/// **The model for this dictation** (plan section 2, "Which model runs"): his choice if it is
/// verified, otherwise the other of the two if that is, otherwise none. `verified` is
/// `stt::model_verified(id).is_ok()` in the tool.
pub fn pick_model(chosen: &str, verified: impl Fn(&str) -> bool) -> ModelPick {
    if verified(chosen) {
        return ModelPick::Chosen(chosen.to_string());
    }
    let other = if chosen == FASTER { MORE_ACCURATE } else { FASTER };
    if verified(other) {
        return ModelPick::Fallback { wanted: chosen.to_string(), using: other.to_string() };
    }
    ModelPick::Missing
}

// =============================================================================================
// PUTTING THE WORDS WHERE THE CURSOR IS
// =============================================================================================

/// How the words reach the app in front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Insert {
    /// Clipboard, Command-V, clipboard restored: open-wispr's own rule.
    Paste,
    /// Nothing to type into: the words stay on the clipboard and the bar says so.
    CopyOnly,
}

/// **Is there nothing to type into?** Only when Accessibility reports no focused element AND the
/// app in front has no focused window, or Finder is in front with no Finder window focused (the
/// desktop). Every other case pastes, whether or not Accessibility can describe what is focused:
/// Chromium and Electron apps often build their accessibility tree only for an assistive client,
/// and Terminal may not report a settable value (Frank's M7, taken as he wrote it).
///
/// The window half is measured, not assumed: in walk-326edd632dac Chrome was in front with its
/// text box focused and the system-wide `AXFocusedUIElement` answered nothing, so a rule on the
/// focused element alone copied instead of pasting in every Chromium app. Chrome's windows are
/// ordinary windows, and Accessibility reports them without an assistive client.
///
/// Finder describes its whole tree, so in Finder only a focused text box (a rename field, the
/// search field, Go to Folder) is a place to type: with the desktop in front Finder answers a
/// focused element (its icon view, `AXScrollArea`) AND a window (walk-36699815244d, where the
/// words were pasted at the desktop), so `focused_role` is what tells the desktop from a field.
pub fn insert_plan(focused_element: bool, window_focused: bool, finder_in_front: bool, focused_role: Option<&str>) -> Insert {
    let nothing_focused = !focused_element && !window_focused;
    let finder_not_a_text_box = finder_in_front && !focused_role.is_some_and(typing_role);
    if nothing_focused || finder_not_a_text_box {
        Insert::CopyOnly
    } else {
        Insert::Paste
    }
}

/// An Accessibility role that takes typed text.
pub fn typing_role(role: &str) -> bool {
    matches!(role, "AXTextField" | "AXTextArea" | "AXComboBox")
}

/// **The spacing rule** (plan section 5, point 3): a space before the words when the character
/// before the cursor is not whitespace, and a space after when the next character is a letter or
/// a digit. `None` is the start or the end of the text. Applied only where Accessibility answers;
/// elsewhere the words go in as they are.
pub fn spaced(words: &str, before: Option<char>, after: Option<char>) -> String {
    let mut out = String::with_capacity(words.len() + 2);
    if before.is_some_and(|c| !c.is_whitespace()) {
        out.push(' ');
    }
    out.push_str(words);
    if after.is_some_and(|c| c.is_alphanumeric()) {
        out.push(' ');
    }
    out
}

/// How long after the paste the clipboard is put back: open-wispr's `defaultRestoreDelay`.
pub const RESTORE_AFTER: Duration = Duration::from_millis(1000);

/// **Put his clipboard back?** Only if nothing wrote to it since the tool did: the pasteboard's
/// change count is still the one the tool's own write produced (open-wispr `TextInserter`,
/// `guard pasteboard.changeCount == writeChangeCount`). Something he copied in that second is
/// his, and is never overwritten.
pub fn restore_clipboard(change_count_now: i64, change_count_after_write: i64) -> bool {
    change_count_now == change_count_after_write
}

/// Identity evidence is computed by the native edge, never guessed from text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldIdentity {
    Same,
    ChangedOrLost,
    UnreadableFromStart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryPermission {
    Paste,
    Recover,
    DiscardSecure,
}

pub fn delivery_permission(same_app: bool, same_window: bool, field: FieldIdentity,
                           secure_destination: bool, navigated: bool) -> DeliveryPermission {
    if !same_app || !same_window {
        return DeliveryPermission::Recover;
    }
    if secure_destination {
        return DeliveryPermission::DiscardSecure;
    }
    match field {
        FieldIdentity::ChangedOrLost => DeliveryPermission::Recover,
        FieldIdentity::UnreadableFromStart if navigated => DeliveryPermission::Recover,
        _ => DeliveryPermission::Paste,
    }
}

/// One completed text item. No audio, durable storage or whole-window content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub original: String,
    pub text: String,
    pub destination: String,
    pub company: Option<String>,
    pub completed_ms: u64,
    pub range: Option<(isize, isize)>,
}

#[derive(Debug)]
struct RecentDraft {
    draft: Draft,
    expires: Duration,
    unresolved: bool,
}

pub const DRAFT_LIFETIME: Duration = Duration::from_secs(30 * 60);

/// One protected recovery item, plus only the current delivered cleanup subject
/// when an earlier unresolved item occupies Recover. Time is monotonic and supplied
/// by the owner, so tests need no sleeps and a wall-clock adjustment cannot extend
/// retention. The dictation key never consults this state before recording.
#[derive(Debug, Default)]
pub struct Drafts {
    held: Option<RecentDraft>,
    current: Option<RecentDraft>,
    cleanup_eligible: bool,
    clipboard_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retention {
    pub earlier_unresolved: bool,
    pub replaced_clipboard_only: bool,
}

impl Drafts {
    pub fn protects_unresolved(&mut self, now: Duration) -> bool {
        self.expire(now);
        self.held.as_ref().is_some_and(|h| h.unresolved)
    }
    pub fn until_expiry(&self, now: Duration) -> Option<Duration> {
        self.held.iter().chain(self.current.iter()).map(|h| h.expires.saturating_sub(now)).min()
    }
    pub fn recovery_lifetime(&self, now: Duration) -> Option<Duration> {
        self.held.as_ref().map(|h| h.expires.saturating_sub(now))
    }
    pub fn recording_started(&mut self, now: Duration) {
        self.expire(now);
        self.current = None;
        self.cleanup_eligible = false;
    }

    pub fn complete(&mut self, draft: Draft, unresolved: bool, copied: bool, now: Duration) -> Retention {
        self.expire(now);
        let earlier_unresolved = self.held.as_ref().is_some_and(|h| h.unresolved);
        let replaced_clipboard_only = earlier_unresolved && unresolved && copied && self.clipboard_only;
        let item = RecentDraft { draft, expires: now + DRAFT_LIFETIME, unresolved };
        self.cleanup_eligible = !unresolved;
        self.current = None;
        if earlier_unresolved {
            if unresolved {
                self.clipboard_only |= copied;
            } else {
                self.current = Some(item);
            }
        } else {
            self.held = Some(item);
            self.clipboard_only = false;
        }
        Retention { earlier_unresolved, replaced_clipboard_only }
    }

    pub fn expire(&mut self, now: Duration) {
        if self.held.as_ref().is_some_and(|h| now >= h.expires) {
            self.held = None;
            self.clipboard_only = false;
        }
        if self.current.as_ref().is_some_and(|h| now >= h.expires) {
            self.current = None;
        }
    }

    pub fn recovery(&mut self, now: Duration) -> Option<&Draft> {
        self.expire(now);
        self.held.as_ref().map(|h| &h.draft)
    }

    pub fn cleanup_subject(&mut self, now: Duration) -> Option<&Draft> {
        self.expire(now);
        if !self.cleanup_eligible {
            return None;
        }
        self.current.as_ref().or_else(|| self.held.as_ref().filter(|h| !h.unresolved)).map(|h| &h.draft)
    }

    pub fn copied_recovery(&mut self) {
        self.held = None;
        self.clipboard_only = false;
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn remove_company(&mut self, company: &str) {
        if self.held.as_ref().is_some_and(|h| h.draft.company.as_deref() == Some(company)) {
            self.held = None;
            self.clipboard_only = false;
        }
        if self.current.as_ref().is_some_and(|h| h.draft.company.as_deref() == Some(company)) {
            self.current = None;
        }
    }
}

/// The plain-text pasteboard type the words are written as.
pub const PLAIN_TEXT_TYPE: &str = "public.utf8-plain-text";
/// The nspasteboard.org marker that tells clipboard managers to leave an item out of their
/// history, so a dictation is not recorded by one (Frank's minor 11).
pub const TRANSIENT_TYPE: &str = "org.nspasteboard.TransientType";

/// Every type the written item carries: the words, and the transient marker.
pub const WRITTEN_TYPES: [&str; 2] = [PLAIN_TEXT_TYPE, TRANSIENT_TYPE];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_reads_the_latest_eight_seconds_without_shortening_the_final_recording() {
        let mut recording = Recording::new();
        let samples: Vec<f32> = (0..(10 * SAMPLE_RATE)).map(|n| n as f32).collect();
        recording.push(&samples);
        assert_eq!(recording.preview_samples(), &samples[2 * SAMPLE_RATE as usize..]);
        assert_eq!(recording.into_samples(), samples);
    }

    #[test]
    fn destination_changes_and_secure_fields_fail_safely_without_regressing_unreadable_windows() {
        use DeliveryPermission::*;
        use FieldIdentity::*;
        assert_eq!(delivery_permission(true, true, UnreadableFromStart, false, false), Paste);
        assert_eq!(delivery_permission(true, true, UnreadableFromStart, false, true), Recover);
        assert_eq!(delivery_permission(true, true, Same, false, true), Paste);
        assert_eq!(delivery_permission(true, true, ChangedOrLost, false, false), Recover);
        assert_eq!(delivery_permission(true, false, Same, false, false), Recover);
        assert_eq!(delivery_permission(false, true, Same, false, false), Recover);
        assert_eq!(delivery_permission(true, true, Same, true, false), DiscardSecure);
        assert_eq!(delivery_permission(false, false, Same, true, false), Recover,
                   "a different password app must not discard speech meant for the old destination");
    }

    fn draft(text: &str) -> Draft {
        Draft { original: text.into(), text: text.into(), destination: "Mail".into(),
                company: Some("company-a".into()), completed_ms: 42, range: Some((5, text.len() as isize)) }
    }

    #[test]
    fn unresolved_draft_is_protected_while_latest_delivered_text_remains_available_for_cleanup() {
        let mut drafts = Drafts::default();
        let now = Duration::ZERO;
        drafts.complete(draft("first failed"), true, true, now);
        drafts.recording_started(now);
        drafts.complete(draft("latest delivered"), false, false, now);
        assert_eq!(drafts.recovery(now).unwrap().text, "first failed");
        assert_eq!(drafts.cleanup_subject(now).unwrap().text, "latest delivered");
        drafts.recording_started(now);
        assert!(drafts.cleanup_subject(now).is_none());
        assert_eq!(drafts.recovery(now).unwrap().text, "first failed");
    }

    #[test]
    fn repeated_unresolved_results_report_the_bounded_clipboard_replacement_and_never_evict_first() {
        let mut drafts = Drafts::default();
        let now = Duration::ZERO;
        assert!(!drafts.complete(draft("first"), true, true, now).earlier_unresolved);
        assert_eq!(drafts.complete(draft("second"), true, true, now),
                   Retention { earlier_unresolved: true, replaced_clipboard_only: false });
        assert!(!drafts.complete(draft("copy failed"), true, false, now).replaced_clipboard_only);
        assert!(drafts.complete(draft("third copied"), true, true, now).replaced_clipboard_only);
        assert_eq!(drafts.recovery(now).unwrap().text, "first");
        drafts.copied_recovery();
        assert!(drafts.recovery(now).is_none());
        drafts.complete(draft("next"), true, false, now);
        assert_eq!(drafts.recovery(now).unwrap().text, "next");
    }

    #[test]
    fn draft_expiry_is_not_extended_and_clear_or_company_removal_removes_owned_text() {
        let mut drafts = Drafts::default();
        drafts.complete(draft("first"), true, true, Duration::ZERO);
        drafts.complete(draft("later delivered"), false, false, Duration::from_secs(60));
        assert!(drafts.recovery(DRAFT_LIFETIME).is_none());
        assert_eq!(drafts.cleanup_subject(DRAFT_LIFETIME).unwrap().text, "later delivered");
        drafts.remove_company("other-company");
        assert!(drafts.cleanup_subject(DRAFT_LIFETIME).is_some());
        drafts.remove_company("company-a");
        assert!(drafts.cleanup_subject(DRAFT_LIFETIME).is_none());
        drafts.complete(draft("another"), false, false, DRAFT_LIFETIME);
        drafts.clear();
        assert!(drafts.recovery(DRAFT_LIFETIME).is_none());
        assert!(drafts.cleanup_subject(DRAFT_LIFETIME).is_none());
    }
    use crate::vad::VAD_FRAME_SAMPLES;

    // ---- the key ---------------------------------------------------------------------------

    fn down(code: u16) -> KeyEvent {
        KeyEvent::Key { code, down: true, repeat: false }
    }

    /// INVARIANT: the codes are the `kVK_F*` constants, one by one, and nothing outside F1..F19.
    #[test]
    fn the_function_key_codes_are_the_hitoolbox_constants() {
        let want = [
            (1, 122), (2, 120), (3, 99), (4, 118), (5, 96), (6, 97), (7, 98), (8, 100), (9, 101),
            (10, 109), (11, 103), (12, 111), (13, 105), (14, 107), (15, 113), (16, 106), (17, 64),
            (18, 79), (19, 80),
        ];
        for (n, code) in want {
            assert_eq!(f_key_code(n), Some(code), "F{n}");
        }
        assert_eq!(f_key_code(0), None);
        assert_eq!(f_key_code(20), None);
    }

    /// INVARIANT: each row of the plan's section 3 table matches F1, and the key is swallowed on
    /// down AND up so the app in front never sees it.
    #[test]
    fn every_shape_the_f1_key_sends_is_f1() {
        // Apple keyboard with fn, standard-function-keys mode, or a non-Apple keyboard: 122.
        assert_eq!(judge_key(1, &down(122)), KeyVerdict::Toggle);
        // His Keychron through Karabiner: F13.
        assert_eq!(judge_key(1, &down(KEY_CODE_F13)), KeyVerdict::Toggle);
        // Apple keyboard default, or a non-Apple keyboard in media mode: brightness down.
        assert_eq!(judge_key(1, &KeyEvent::top_row(3, true, false)), KeyVerdict::Toggle);
        // Their key-ups are swallowed and act on nothing.
        assert_eq!(judge_key(1, &KeyEvent::Key { code: 122, down: false, repeat: false }), KeyVerdict::Swallow);
        assert_eq!(judge_key(1, &KeyEvent::Key { code: 105, down: false, repeat: false }), KeyVerdict::Swallow);
        assert_eq!(judge_key(1, &KeyEvent::top_row(3, false, false)), KeyVerdict::Swallow);
    }

    /// INVARIANT (slice 2, key capture): every F-key down is captured as itself, plain code or
    /// Apple top row; a key-up, an auto-repeat, a typing key and any other system-defined event
    /// are no choice. And whatever is captured is a key the tap then matches as his key.
    #[test]
    fn key_capture_records_the_f_key_as_it_is() {
        for n in 1..=19u8 {
            let code = f_key_code(n).unwrap();
            assert_eq!(captured_key(&down(code)), Some(n), "F{n} by its plain code");
            assert_eq!(judge_key(n, &down(code)), KeyVerdict::Toggle, "F{n} captured is F{n} matched");
        }
        assert_eq!(captured_key(&down(KEY_CODE_F13)), Some(13), "F13 is recorded as F13, not F1");
        for (f, t) in [(1u8, 3i64), (2, 2), (7, 18), (7, 20), (8, 16), (9, 17), (9, 19), (10, 7), (11, 1), (12, 0)] {
            let event = KeyEvent::top_row(t, true, false);
            assert_eq!(captured_key(&event), Some(f), "top-row type {t}");
            assert_eq!(judge_key(f, &event), KeyVerdict::Toggle, "top-row type {t} captured is matched");
        }
        assert_eq!(captured_key(&KeyEvent::Key { code: 122, down: false, repeat: false }), None, "a key-up");
        assert_eq!(captured_key(&KeyEvent::Key { code: 122, down: true, repeat: true }), None, "an auto-repeat");
        assert_eq!(captured_key(&KeyEvent::top_row(3, false, false)), None, "a top-row key-up");
        assert_eq!(captured_key(&KeyEvent::top_row(3, true, true)), None, "a top-row auto-repeat");
        assert_eq!(captured_key(&down(0)), None, "the A key types text");
        assert_eq!(captured_key(&down(53)), None, "Escape");
        assert_eq!(captured_key(&KeyEvent::top_row(22, true, false)), None, "the keyboard light is no F-key here");
        assert_eq!(captured_key(&KeyEvent::System { subtype: 7, data1: 3 << 16 | 0x0A << 8 }), None, "another subtype");
    }

    /// INVARIANT: an auto-repeat never toggles a second time; holding the key is one tap.
    #[test]
    fn an_auto_repeat_is_swallowed_and_never_toggles() {
        assert_eq!(judge_key(1, &KeyEvent::Key { code: 122, down: true, repeat: true }), KeyVerdict::Swallow);
        assert_eq!(judge_key(1, &KeyEvent::top_row(3, true, true)), KeyVerdict::Swallow);
    }

    /// INVARIANT: every other event passes untouched: other keys, F13 when the key is not F1,
    /// brightness up when the key is F1, and system-defined events of any other subtype.
    #[test]
    fn everything_that_is_not_his_key_passes() {
        assert_eq!(judge_key(1, &down(0)), KeyVerdict::Pass, "the A key");
        assert_eq!(judge_key(1, &down(120)), KeyVerdict::Pass, "F2");
        assert_eq!(judge_key(5, &down(KEY_CODE_F13)), KeyVerdict::Pass, "F13 only stands for F1");
        assert_eq!(judge_key(1, &KeyEvent::top_row(2, true, false)), KeyVerdict::Pass, "brightness up");
        assert_eq!(judge_key(1, &KeyEvent::System { subtype: 7, data1: 3 << 16 | 0x0A << 8 }), KeyVerdict::Pass);
        // A system-defined event whose state is neither down nor up is not a key.
        assert_eq!(judge_key(1, &KeyEvent::System { subtype: 8, data1: 3 << 16 }), KeyVerdict::Pass);
        assert_eq!(judge_key(0, &down(122)), KeyVerdict::Pass, "no key chosen matches nothing");
    }

    /// INVARIANT: the top-row table, key by key; F3 to F6 match their plain code only, and F13
    /// to F19 are plain codes.
    #[test]
    fn the_top_row_table() {
        for (f, types) in [(2u8, &[2i64][..]), (7, &[18, 20]), (8, &[16]), (9, &[17, 19]), (10, &[7]), (11, &[1]), (12, &[0])] {
            for t in types {
                assert_eq!(judge_key(f, &KeyEvent::top_row(*t, true, false)), KeyVerdict::Toggle, "F{f} type {t}");
            }
            assert_eq!(judge_key(f, &down(f_key_code(f).unwrap())), KeyVerdict::Toggle, "F{f} plain");
        }
        for f in 3..=6u8 {
            assert_eq!(judge_key(f, &down(f_key_code(f).unwrap())), KeyVerdict::Toggle, "F{f} plain");
            for t in 0..32 {
                assert_eq!(judge_key(f, &KeyEvent::top_row(t, true, false)), KeyVerdict::Pass, "F{f} type {t}");
            }
        }
        assert_eq!(judge_key(19, &down(80)), KeyVerdict::Toggle);
    }

    // ---- the session -----------------------------------------------------------------------

    /// INVARIANT: tap, tap is listen then write; a tap while writing does nothing; a tap after
    /// done or a problem starts the next dictation.
    #[test]
    fn the_session_walks_its_phases() {
        let mut s = Session::default();
        assert_eq!(s.phase(), Phase::Idle);
        assert_eq!(s.tap(1_000), TapAction::StartListening);
        assert_eq!(s.phase(), Phase::Listening { started_ms: 1_000 });
        assert_eq!(s.tap(4_250), TapAction::StopListening { duration_ms: 3_250 });
        assert_eq!(s.phase(), Phase::Writing);
        assert_eq!(s.tap(4_400), TapAction::Ignore);
        s.finish(Ok(()));
        assert_eq!(s.phase(), Phase::Done);
        assert_eq!(s.tap(9_000), TapAction::StartListening);
        assert_eq!(s.input_died(12_008), Some(3_008));
        s.finish(Err(Problem::NoSound));
        assert_eq!(s.phase(), Phase::Problem(Problem::NoSound));
        assert_eq!(s.input_died(13_000), None, "only a listening session can lose its input");
        assert_eq!(s.tap(14_000), TapAction::StartListening);
    }

    /// INVARIANT: under 450 ms is "I didn't catch anything" whatever was heard; 450 ms is not.
    #[test]
    fn a_short_tap_is_did_not_catch() {
        assert_eq!(judge(449, false, true, || true), Stopped::Refuse(Problem::DidNotCatch));
        assert_eq!(judge(0, false, true, || true), Stopped::Refuse(Problem::DidNotCatch));
        assert_eq!(judge(450, false, true, || true), Stopped::Transcribe { then: None });
    }

    /// INVARIANT: audio with no measured voice is refused BEFORE any transcript exists. The real
    /// gate, on real samples: 2 s of room tone carries no speech.
    #[test]
    fn no_measured_voice_is_refused_before_whisper() {
        let mut rec = Recording::new();
        let mut rng = crate::voiced::fixtures::rng(7);
        for _ in 0..(2 * SAMPLE_RATE as usize / VAD_FRAME_SAMPLES) {
            let frame: Vec<f32> = (0..VAD_FRAME_SAMPLES).map(|_| rng() * 0.003).collect();
            rec.push(&frame);
        }
        assert!(rec.any_live(), "room tone is a live input");
        assert!(!rec.no_audio());
        assert_eq!(judge_recording(2_000, &rec), Stopped::Refuse(Problem::DidNotCatch));
        // A voice with moving pitch, the fixture voice mode's own tests use, is transcribed.
        let mut spoken = Recording::new();
        for frame in crate::voiced::fixtures::synthetic_voice(1.5, 190.0, 130.0, -26.0).chunks_exact(VAD_FRAME_SAMPLES) {
            spoken.push(frame);
        }
        assert_eq!(judge_recording(1_500, &spoken), Stopped::Transcribe { then: None });
        // And the order: the evidence closure is never asked about a short tap.
        assert_eq!(judge(100, false, true, || panic!("measured a too-short tap")), Stopped::Refuse(Problem::DidNotCatch));
    }

    /// INVARIANT (minor 9): the input dying after words still writes the words, then says it
    /// cannot hear; dying with no live frame ever says only that.
    #[test]
    fn no_audio_after_words_writes_the_words_first() {
        assert_eq!(judge(5_000, true, true, || true), Stopped::Transcribe { then: Some(Problem::NoSound) });
        assert_eq!(judge(5_000, true, false, || panic!("measured a dead input")), Stopped::Refuse(Problem::NoSound));
        assert_eq!(judge(5_000, true, true, || false), Stopped::Refuse(Problem::NoSound));
    }

    /// INVARIANT: the recording reports a dead input exactly once, after 188 frames (3.008 s) of
    /// digital silence, the WAV source's own tail (`capture.rs`).
    #[test]
    fn the_recording_reports_a_dead_input_once() {
        let mut rec = Recording::new();
        let live = vec![0.01f32; VAD_FRAME_SAMPLES];
        let dead = vec![0.0f32; VAD_FRAME_SAMPLES];
        assert!(!rec.push(&live));
        let fired: Vec<usize> = (0..400).filter(|_| rec.push(&dead)).collect();
        assert_eq!(fired, vec![crate::noaudio::NO_AUDIO_FRAMES as usize - 1], "frame index of the one report");
        assert!(rec.no_audio() && rec.any_live());
        assert_eq!(rec.samples().len(), 401 * VAD_FRAME_SAMPLES, "every frame is kept for transcription");
    }

    /// INVARIANT (M5): annotations never reach the paste, whatever their brackets; a transcript of
    /// nothing but annotations or noise phrases is "I didn't catch anything".
    #[test]
    fn annotations_are_stripped_and_noise_is_refused() {
        assert_eq!(judge_transcript("Hi Dana, [BLANK_AUDIO] thanks for sending it."), Ok("Hi Dana, thanks for sending it.".into()));
        assert_eq!(judge_transcript("(upbeat music) Talk soon."), Ok("Talk soon.".into()));
        assert_eq!(judge_transcript("[BLANK_AUDIO]"), Err(Problem::DidNotCatch));
        assert_eq!(judge_transcript("(upbeat music)"), Err(Problem::DidNotCatch));
        assert_eq!(judge_transcript("  "), Err(Problem::DidNotCatch));
        assert_eq!(judge_transcript("Thanks for watching!"), Err(Problem::DidNotCatch));
        // A real short word survives, as in voice mode.
        assert_eq!(judge_transcript("Yes."), Ok("Yes.".into()));
    }

    /// INVARIANT (minor 6): the bound is 60 s plus 1.5 times the length. 10 minutes of audio is
    /// 9,600,000 samples, 600 s, so 60 + 900 = 960 s.
    #[test]
    fn the_decode_bound_scales_with_the_recording() {
        assert_eq!(decode_bound(0), Duration::from_secs(60));
        assert_eq!(decode_bound(16_000), Duration::from_millis(61_500));
        assert_eq!(decode_bound(10 * 60 * 16_000), Duration::from_secs(960));
    }

    /// INVARIANT: his choice when verified; the other when only it is, said as a fallback;
    /// nothing when neither is.
    #[test]
    fn the_model_is_his_choice_then_the_other_then_none() {
        assert_eq!(MORE_ACCURATE, "large-v3-turbo-q5_0");
        assert_eq!(FASTER, "small.en");
        let both = |_: &str| true;
        let only_fast = |id: &str| id == FASTER;
        let none = |_: &str| false;
        assert_eq!(pick_model(MORE_ACCURATE, both), ModelPick::Chosen(MORE_ACCURATE.into()));
        assert_eq!(pick_model(FASTER, both), ModelPick::Chosen(FASTER.into()));
        assert_eq!(
            pick_model(MORE_ACCURATE, only_fast),
            ModelPick::Fallback { wanted: MORE_ACCURATE.into(), using: FASTER.into() }
        );
        assert_eq!(
            pick_model(FASTER, |id: &str| id == MORE_ACCURATE),
            ModelPick::Fallback { wanted: FASTER.into(), using: MORE_ACCURATE.into() }
        );
        assert_eq!(pick_model(MORE_ACCURATE, none), ModelPick::Missing);
        assert_eq!(ModelPick::Missing.id(), None);
    }

    // ---- the words at the cursor ----------------------------------------------------------

    /// INVARIANT (M7): only nothing focused at all (no element and no window), or Finder's
    /// desktop, keeps the words on the clipboard. Finder with a window focused, and every other
    /// app, pastes.
    #[test]
    fn only_no_focus_or_the_desktop_is_nothing_to_type_into() {
        assert_eq!(insert_plan(false, false, false, None), Insert::CopyOnly, "nothing focused");
        assert_eq!(insert_plan(true, false, true, None), Insert::CopyOnly, "the desktop");
        assert_eq!(insert_plan(true, true, true, Some("AXScrollArea")), Insert::CopyOnly, "the desktop as walk-36699815244d saw it: an icon view and a window");
        assert_eq!(insert_plan(false, true, true, None), Insert::CopyOnly, "a Finder window with nothing focused in it");
        assert_eq!(insert_plan(true, true, true, Some("AXTextField")), Insert::Paste, "a Finder window's rename field");
        assert_eq!(insert_plan(true, true, true, Some("AXTextArea")), Insert::Paste);
        assert_eq!(insert_plan(true, false, false, None), Insert::Paste, "any other app, described or not");
        assert_eq!(insert_plan(true, true, false, Some("AXScrollArea")), Insert::Paste, "another app's role is not judged (Chromium, Electron)");
        assert_eq!(insert_plan(true, true, false, None), Insert::Paste, "an app that describes its focus");
    }

    /// INVARIANT (M7, measured in walk-326edd632dac): an app whose window is focused but which
    /// reports no focused element without an assistive client (Chrome, Electron) still pastes.
    #[test]
    fn a_focused_window_without_a_described_element_pastes() {
        assert_eq!(insert_plan(false, true, false, None), Insert::Paste);
    }

    /// INVARIANT: round 19's spacing (`insertAt`): a space before unless at the start or after
    /// whitespace, a space after only before a letter or digit.
    #[test]
    fn the_spacing_rule() {
        assert_eq!(spaced("Talk soon.", None, None), "Talk soon.");
        assert_eq!(spaced("Talk soon.", Some('.'), None), " Talk soon.");
        assert_eq!(spaced("Talk soon.", Some(' '), None), "Talk soon.");
        assert_eq!(spaced("Talk soon.", Some('\n'), None), "Talk soon.");
        assert_eq!(spaced("two", Some('e'), Some('t')), " two ");
        assert_eq!(spaced("two", Some(' '), Some('7')), "two ");
        assert_eq!(spaced("two", Some(' '), Some(',')), "two");
        assert_eq!(spaced("two", Some(' '), Some(' ')), "two");
    }

    /// INVARIANT: his clipboard comes back only if nothing else wrote to it since the tool did.
    #[test]
    fn the_clipboard_is_restored_only_if_untouched() {
        assert!(restore_clipboard(42, 42));
        assert!(!restore_clipboard(43, 42), "he copied something in that second: it is his");
        assert_eq!(RESTORE_AFTER, Duration::from_secs(1));
    }

    /// INVARIANT (minor 11): the written item carries the transient marker beside the words.
    #[test]
    fn the_written_item_is_marked_transient() {
        assert_eq!(TRANSIENT_TYPE, "org.nspasteboard.TransientType");
        assert!(WRITTEN_TYPES.contains(&TRANSIENT_TYPE));
        assert!(WRITTEN_TYPES.contains(&PLAIN_TEXT_TYPE));
    }
}

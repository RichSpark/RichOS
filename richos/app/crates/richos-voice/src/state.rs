//! The voice-mode state machine — what the CEO is told the microphone is doing.
//!
//! The UX direction §4.1 is unambiguous about why this matters: with no echo cancellation shipping in
//! v1, *"the CEO always knows whether the mic is hot — this is the single most important
//! voice-UX requirement."* So mic state is not a cosmetic label derived from whatever the
//! audio threads happen to be doing; it is an explicit machine with tested transitions.
//!
//! Four facts drive it, and they are genuinely independent:
//!
//! | fact | set by |
//! |---|---|
//! | voice mode is on | the CEO's `◉` toggle |
//! | a CEO utterance is in flight | the endpointer |
//! | a spine turn is in flight | `rich://turn-started` / terminal event |
//! | Rich has audio playing out | the playout queue |
//!
//! [`VoiceState`] is a VIEW over those four, in priority order, so the panel can never show
//! two things at once and can never flicker between them mid-sentence:
//!
//! ```text
//!   off?         -> Off
//!   speaking?    -> Speaking     (wins: a pause between Rich's sentences is still "speaking")
//!   hearing?     -> Hearing      (the CEO is mid-utterance)
//!   turn active? -> Thinking     (Rich has it, hasn't opened his mouth yet)
//!   otherwise    -> Listening
//! ```

/// What the CEO is shown. The UI renders `Listening` and `Speaking` as the two unmistakable
/// states of the UX direction §4.1 sketch; `Hearing` is Listening with the level meter live, and
/// `Thinking` reuses the existing "Rich is working" affordance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceState {
    /// Voice mode is off. The mic is CLOSED — no capture stream exists at all.
    Off,
    /// Mic open, room quiet. Rich hears you.
    Listening,
    /// Mic open, the CEO is talking right now.
    Hearing,
    /// The CEO finished; Rich has the turn and has not started speaking.
    Thinking,
    /// Rich is speaking. Barge-in is armed.
    Speaking,
}

impl VoiceState {
    /// The stable string the webview keys off. Kept here so Rust and JS cannot drift.
    pub fn as_str(self) -> &'static str {
        match self {
            VoiceState::Off => "off",
            VoiceState::Listening => "listening",
            VoiceState::Hearing => "hearing",
            VoiceState::Thinking => "thinking",
            VoiceState::Speaking => "speaking",
        }
    }

    /// Is the microphone open in this state? The honest answer behind the UI's mic dot.
    pub fn mic_is_hot(self) -> bool {
        !matches!(self, VoiceState::Off)
    }
}

/// The four independent facts, and the transitions allowed to change them.
#[derive(Debug, Clone, Default)]
pub struct VoiceStateMachine {
    on: bool,
    hearing: bool,
    turn_active: bool,
    speaking: bool,
}

impl VoiceStateMachine {
    pub fn new() -> Self {
        VoiceStateMachine::default()
    }

    pub fn state(&self) -> VoiceState {
        if !self.on {
            return VoiceState::Off;
        }
        if self.speaking {
            return VoiceState::Speaking;
        }
        if self.hearing {
            return VoiceState::Hearing;
        }
        if self.turn_active {
            return VoiceState::Thinking;
        }
        VoiceState::Listening
    }

    pub fn is_on(&self) -> bool {
        self.on
    }

    /// A spine turn is in flight: an answer may still be coming.
    pub fn turn_active(&self) -> bool {
        self.turn_active
    }

    /// Barge-in is armed exactly when Rich has audio playing. Nothing else can arm it —
    /// speech during Rich's silence is a normal utterance, not an interruption.
    pub fn barge_in_armed(&self) -> bool {
        self.on && self.speaking
    }

    /// The CEO tapped `◉` on.
    pub fn start(&mut self) {
        *self = VoiceStateMachine { on: true, ..Default::default() };
    }

    /// The CEO tapped `◉` off. Everything in flight is abandoned; the mic closes.
    pub fn stop(&mut self) {
        *self = VoiceStateMachine::default();
    }

    /// The endpointer saw speech onset.
    pub fn utterance_started(&mut self) {
        if self.on {
            self.hearing = true;
        }
    }

    /// The utterance ended — accepted (sent to whisper) or discarded (a cough).
    pub fn utterance_ended(&mut self) {
        self.hearing = false;
    }

    /// A spine turn started (`rich://turn-started`). Also covers a turn the CEO started by
    /// TYPING while voice mode was on: Rich still answers aloud.
    pub fn turn_started(&mut self) {
        if self.on {
            self.turn_active = true;
        }
    }

    /// A spine turn reached its terminal event. Rich may still be speaking out the tail of
    /// the reply, so this alone never returns the panel to Listening.
    pub fn turn_ended(&mut self) {
        self.turn_active = false;
    }

    /// The playout queue went from empty to non-empty: Rich's voice is audible.
    pub fn playout_started(&mut self) {
        if self.on {
            self.speaking = true;
        }
    }

    /// The playout queue drained AND no further audio is pending.
    pub fn playout_drained(&mut self) {
        self.speaking = false;
    }

    /// The CEO talked over Rich for the full debounce (or hit "tap to stop"). Rich's audio
    /// is cut and the CEO is already mid-utterance — the frames he interrupted with are the
    /// start of what he is saying (see `endpoint::UtteranceRecorder::take_in_flight`).
    ///
    /// Returns false if nothing was interrupted, so a caller cannot silently double-fire.
    pub fn barge_in(&mut self) -> bool {
        if !self.barge_in_armed() {
            return false;
        }
        self.speaking = false;
        self.hearing = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// INVARIANT: with voice mode off the mic is never hot and nothing else can turn it on.
    #[test]
    fn nothing_makes_the_mic_hot_while_voice_mode_is_off() {
        let mut m = VoiceStateMachine::new();
        assert_eq!(m.state(), VoiceState::Off);
        assert!(!m.state().mic_is_hot());
        m.utterance_started();
        m.turn_started();
        m.playout_started();
        assert_eq!(m.state(), VoiceState::Off, "state leaked while voice mode was off");
        assert!(!m.barge_in_armed());
    }

    /// INVARIANT: the whole happy path — tap on, talk, Rich thinks, Rich speaks, back to
    /// listening — visits exactly the states the UI promises, in order.
    #[test]
    fn the_happy_path_visits_listening_hearing_thinking_speaking_listening() {
        let mut m = VoiceStateMachine::new();
        m.start();
        assert_eq!(m.state(), VoiceState::Listening);
        m.utterance_started();
        assert_eq!(m.state(), VoiceState::Hearing);
        m.utterance_ended();
        m.turn_started();
        assert_eq!(m.state(), VoiceState::Thinking);
        m.playout_started();
        assert_eq!(m.state(), VoiceState::Speaking);
        m.turn_ended();
        assert_eq!(m.state(), VoiceState::Speaking, "the turn ended but Rich is still talking");
        m.playout_drained();
        assert_eq!(m.state(), VoiceState::Listening);
    }

    /// INVARIANT: the panel does not flip to "listening" in the gap between Rich's
    /// sentences. The turn ending is not the same fact as the audio running out.
    #[test]
    fn the_panel_stays_speaking_between_richs_sentences() {
        let mut m = VoiceStateMachine::new();
        m.start();
        m.turn_started();
        m.playout_started();
        m.turn_ended(); // the reply text is complete; the tail is still queued as audio
        for _ in 0..10 {
            assert_eq!(m.state(), VoiceState::Speaking);
        }
        m.playout_drained();
        assert_eq!(m.state(), VoiceState::Listening);
    }

    /// INVARIANT: barge-in is armed if and only if Rich has audio playing.
    #[test]
    fn barge_in_is_armed_only_while_rich_has_audio_playing() {
        let mut m = VoiceStateMachine::new();
        m.start();
        assert!(!m.barge_in_armed(), "listening is not an interruption opportunity");
        m.turn_started();
        assert!(!m.barge_in_armed(), "thinking silently is not an interruption opportunity");
        m.playout_started();
        assert!(m.barge_in_armed());
        m.playout_drained();
        assert!(!m.barge_in_armed());
    }

    /// INVARIANT: a barge-in lands the CEO straight into Hearing — he is already talking,
    /// so the panel must not blink through Listening first.
    #[test]
    fn a_barge_in_goes_straight_from_speaking_to_hearing() {
        let mut m = VoiceStateMachine::new();
        m.start();
        m.turn_started();
        m.playout_started();
        assert_eq!(m.state(), VoiceState::Speaking);
        assert!(m.barge_in());
        assert_eq!(m.state(), VoiceState::Hearing);
    }

    /// INVARIANT: cutting Rich off WITHOUT saying anything returns to Listening. `barge_in`
    /// optimistically assumes the CEO is mid-sentence (a debounce barge-in always is), so a
    /// silent "tap to stop" must be followed by `utterance_ended` — otherwise the panel sits
    /// in Hearing forever with no utterance coming to clear it. Caught by a LIVE barge-in
    /// run on the real output device, 2026-08-24.
    #[test]
    fn tapping_stop_without_saying_anything_returns_to_listening() {
        let mut m = VoiceStateMachine::new();
        m.start();
        m.turn_started();
        m.playout_started();
        assert!(m.barge_in());
        assert_eq!(m.state(), VoiceState::Hearing, "barge_in assumes he is talking");
        m.utterance_ended(); // …he wasn't.
        m.turn_ended();
        m.playout_drained();
        assert_eq!(m.state(), VoiceState::Listening, "stuck in Hearing with nothing to end it");
    }

    /// INVARIANT: barge-in cannot double-fire, so one interruption can never cut two turns.
    #[test]
    fn barge_in_cannot_double_fire() {
        let mut m = VoiceStateMachine::new();
        m.start();
        m.playout_started();
        assert!(m.barge_in());
        assert!(!m.barge_in());
    }

    /// INVARIANT: typing while voice mode is on still makes Rich answer aloud — voice is a
    /// MODE of the one conversation, not a separate input channel.
    #[test]
    fn typing_during_voice_mode_still_produces_a_spoken_reply() {
        let mut m = VoiceStateMachine::new();
        m.start();
        // No utterance at all: the CEO typed.
        m.turn_started();
        assert_eq!(m.state(), VoiceState::Thinking);
        m.playout_started();
        assert_eq!(m.state(), VoiceState::Speaking);
    }

    /// INVARIANT: tapping voice mode off abandons everything in flight — no residual
    /// "speaking" state can survive into the next session and lie about a hot mic.
    #[test]
    fn tapping_off_abandons_everything_in_flight() {
        let mut m = VoiceStateMachine::new();
        m.start();
        m.utterance_started();
        m.turn_started();
        m.playout_started();
        m.stop();
        assert_eq!(m.state(), VoiceState::Off);
        assert!(!m.barge_in_armed());
        m.start();
        assert_eq!(m.state(), VoiceState::Listening, "stale state survived a restart");
    }

    /// INVARIANT: a failed turn returns the CEO to Listening, not to a stuck "thinking"
    /// spinner — the mic stays honest even when the compute lease dies.
    #[test]
    fn a_failed_turn_returns_to_listening_not_a_stuck_spinner() {
        let mut m = VoiceStateMachine::new();
        m.start();
        m.turn_started();
        assert_eq!(m.state(), VoiceState::Thinking);
        m.turn_ended(); // rich://turn-error is terminal too
        assert_eq!(m.state(), VoiceState::Listening);
    }

    /// INVARIANT: the JS-facing state strings are stable — the webview keys off them.
    #[test]
    fn the_state_strings_the_webview_keys_off_are_stable() {
        assert_eq!(VoiceState::Off.as_str(), "off");
        assert_eq!(VoiceState::Listening.as_str(), "listening");
        assert_eq!(VoiceState::Hearing.as_str(), "hearing");
        assert_eq!(VoiceState::Thinking.as_str(), "thinking");
        assert_eq!(VoiceState::Speaking.as_str(), "speaking");
    }
}

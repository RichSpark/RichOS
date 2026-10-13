//! **When the CEO has finished his TURN, as opposed to when one stretch of his speech ended.**
//!
//! Until 2026-10-13 these were the same event. The endpointer closes an utterance after
//! [`SILENCE_HANGOVER_FRAMES`] of silence (50 x 256 / 16000 = 0.800 s), and every utterance it
//! closed went straight to the recognizer and from there straight to the spine as a turn of its
//! own. A person thinking aloud pauses for longer than 0.800 s all the time, so a single thought
//! became several turns, and Rich answered the first fragment while the CEO was still saying
//! the second.
//!
//! ## What the CEO's own log shows (nightly 54, his folder a, 2026-10-13)
//!
//! ```text
//!   15:18:50.093Z utterance START                     his first two words
//!   15:18:52.306Z utterance END ADMITTED  2.512 s
//!   15:18:52.877Z PromptReceived (ledger)             turn 1 = those two words
//!   15:18:53.024Z utterance START                     the sentence his screenshot marks
//!   15:18:55.338Z utterance END ADMITTED  2.640 s
//!   15:18:56.596Z ANSWER AUDIBLE                      Rich, answering the two words
//!   15:18:57.937Z PromptReceived (ledger)             turn 2 = the sentence
//! ```
//!
//! (His words are not quoted here: this repository is public and they are his conversation.)
//!
//! His pause between the two fragments, from the frame math rather than the log stamps alone:
//!
//! ```text
//!   last speech of fragment 1 = END - hangover  = 52.306 - 0.800 = 51.506 s
//!   first speech of fragment 2 = START - onset  = 53.024 - 0.112 = 52.912 s
//!   pause                                       = 52.912 - 51.506 = 1.406 s   (> 0.800 s)
//! ```
//!
//! So 1.406 s of thinking ended his turn, and the answer to his first two words was in his ear
//! 1.258 s after he finished his second sentence (56.596 - 55.338), while he was still mid-thought.
//!
//! ## The rule now
//!
//! An utterance still ends at 0.800 s, because that is what lets whisper start on it while he
//! is still thinking. What changed is that an utterance is a FRAGMENT of a turn: its words are
//! held here, and the turn is handed to the spine only once
//!
//! 1. he has been silent for [`END_OF_TURN_FRAMES`] since his last speech frame
//!    (125 x 256 / 16000 = 2.000 s, reported by the capture path as `CapMsg::TurnQuiet`),
//! 2. no utterance is open, and
//! 3. every fragment he said has come back from the recognizer.
//!
//! Every fragment in between is joined, in order, into ONE turn.
//!
//! **Why 2.000 s.** It covers his measured 1.406 s mid-thought pause with 0.594 s to spare. It
//! costs far less than 2.000 s per answer, because whisper runs on each fragment as soon as its
//! own 0.800 s hangover closes it (measured 0.672 - 0.805 s per utterance on his Mac, same log),
//! so the recognition is done well inside the wait:
//!
//! ```text
//!   before: last word -> turn  = 0.800 hangover + recognition       ~ 1.371 s  (51.506 -> 52.877)
//!   now:    last word -> turn  = max(2.000, 0.800 + recognition)    = 2.000 s
//!   cost per answer                                                 ~ 0.629 s
//! ```
//!
//! A pause LONGER than 2.000 s still ends the turn. That is not hidden: the CEO's next words
//! then become a turn of their own, and what keeps Rich from talking over them is the floor
//! rule in the controller, not this window.
//!
//! Pure: no clock, no thread, no channel. The supervisor feeds it events in the order it sees
//! them, so the whole rule is unit-testable and replayable over a recording.

use crate::endpoint::SILENCE_HANGOVER_FRAMES;
use crate::vad::frames_to_secs;
use std::collections::VecDeque;

/// Silence, counted from his last speech frame, after which his turn is over.
/// 125 x 256 / 16000 = 2.000 s.
pub const END_OF_TURN_FRAMES: u32 = 125;

/// The part of [`END_OF_TURN_FRAMES`] that runs AFTER the endpointer has closed the utterance.
/// The endpointer's own [`SILENCE_HANGOVER_FRAMES`] are the first 0.800 s of the same silence,
/// so the capture path only has to count the rest: 125 - 50 = 75 frames, 75 x 256 / 16000 =
/// 1.200 s after the utterance's END.
pub const TURN_QUIET_FRAMES: u32 = END_OF_TURN_FRAMES - SILENCE_HANGOVER_FRAMES;

/// [`END_OF_TURN_FRAMES`] in seconds, from the one frame-math helper. Never typed.
pub fn end_of_turn_secs() -> f32 {
    frames_to_secs(END_OF_TURN_FRAMES)
}

/// One whole turn, ready for the spine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    /// Every recognized fragment, in the order he said them, joined by one space.
    pub text: String,
    /// Rich was audible during ANY fragment of it — see `AdmittedUtterance::rich_audible`. A
    /// turn is as suspect as its most suspect fragment.
    pub rich_audible: bool,
    /// How many recognized fragments it was assembled from.
    pub fragments: u32,
    /// The recorded audio behind those fragments, in milliseconds.
    pub audio_ms: u64,
}

/// Holds his fragments until his turn is over. See the module documentation.
#[derive(Debug, Default)]
pub struct TurnAssembler {
    words: Vec<String>,
    rich_audible: bool,
    audio_ms: u64,
    /// An utterance is being recorded right now.
    open: bool,
    /// Admitted utterances still at the recognizer, oldest first, with their audio length.
    awaiting: VecDeque<u64>,
    /// [`END_OF_TURN_FRAMES`] of silence have passed since the last utterance ended, and no
    /// utterance has started since.
    quiet: bool,
}

impl TurnAssembler {
    pub fn new() -> Self {
        TurnAssembler::default()
    }

    /// The endpointer opened an utterance. He is talking: whatever quiet had been counted is
    /// void.
    pub fn utterance_started(&mut self) {
        self.open = true;
        self.quiet = false;
    }

    /// The utterance closed and went to the recognizer. `audio_ms` is its recorded length.
    pub fn utterance_admitted(&mut self, audio_ms: u64) {
        self.open = false;
        self.awaiting.push_back(audio_ms);
    }

    /// The utterance closed and was thrown away before the recognizer (a cough, or audio the
    /// half-duplex rule refused). Nothing of it joins the turn.
    pub fn utterance_discarded(&mut self) {
        self.open = false;
    }

    /// The recognizer finished one admitted utterance, in the order they were admitted. `None`
    /// is a refusal — no voice in it, or whisper's own noise — and adds nothing to the turn.
    pub fn recognized(&mut self, words: Option<(String, bool)>) {
        let audio_ms = self.awaiting.pop_front().unwrap_or(0);
        if let Some((text, rich_audible)) = words {
            let text = text.trim();
            if !text.is_empty() {
                self.words.push(text.to_string());
                self.rich_audible |= rich_audible;
                self.audio_ms += audio_ms;
            }
        }
    }

    /// The capture path counted [`END_OF_TURN_FRAMES`] of silence since his last speech.
    pub fn quiet(&mut self) {
        self.quiet = true;
    }

    /// Recognized words that are not yet a turn. Diagnostics only.
    pub fn fragments_held(&self) -> usize {
        self.words.len()
    }

    /// Admitted utterances still at the recognizer. Diagnostics only.
    pub fn awaiting_recognition(&self) -> usize {
        self.awaiting.len()
    }

    /// The turn, if he is finished: quiet for [`END_OF_TURN_FRAMES`], nothing open, nothing at
    /// the recognizer, and at least one fragment with words in it.
    pub fn take_turn(&mut self) -> Option<Turn> {
        if !self.quiet || self.open || !self.awaiting.is_empty() {
            return None;
        }
        self.take()
    }

    /// Voice is going off. Whatever he said that came back as words is still his turn, exactly
    /// as an utterance recognized after the toggle was always submitted before this existed;
    /// a fragment still at the recognizer is not waited for here.
    pub fn flush(&mut self) -> Option<Turn> {
        self.open = false;
        self.take()
    }

    fn take(&mut self) -> Option<Turn> {
        if self.words.is_empty() {
            return None;
        }
        let words = std::mem::take(&mut self.words);
        let turn = Turn {
            text: words.join(" "),
            rich_audible: self.rich_audible,
            fragments: words.len() as u32,
            audio_ms: self.audio_ms,
        };
        self.rich_audible = false;
        self.audio_ms = 0;
        Some(turn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vad::frames_for_secs;

    fn words(text: &str) -> Option<(String, bool)> {
        Some((text.to_string(), false))
    }

    /// INVARIANT: the two frame counts are what the frame math produces from their intended
    /// durations, and the two windows together are exactly the end-of-turn silence.
    #[test]
    fn the_end_of_turn_window_is_the_frame_math() {
        assert_eq!(END_OF_TURN_FRAMES, frames_for_secs(2.0)); // 125.0 exactly
        assert!((end_of_turn_secs() - 2.000).abs() < 1e-6);
        assert_eq!(TURN_QUIET_FRAMES, 75);
        assert!((frames_to_secs(TURN_QUIET_FRAMES) - 1.200).abs() < 1e-6);
        assert_eq!(SILENCE_HANGOVER_FRAMES + TURN_QUIET_FRAMES, END_OF_TURN_FRAMES);
    }

    /// INVARIANT: his measured pause (1.406 s, nightly 54) is inside the window, so a thought
    /// with that pause in it is one turn.
    #[test]
    fn his_measured_pause_is_inside_the_window() {
        let pause = 52.912_f32 - 51.506;
        assert!((pause - 1.406).abs() < 1e-3, "{pause}");
        assert!(pause < end_of_turn_secs(), "the window would end his turn mid-thought");
    }

    /// INVARIANT — THE DEFECT. Two fragments with a pause between them are ONE turn, joined in
    /// the order he said them, and nothing is released before the quiet.
    #[test]
    fn two_fragments_with_a_pause_between_them_are_one_turn() {
        let mut t = TurnAssembler::new();
        t.utterance_started();
        t.utterance_admitted(2512);
        t.recognized(words("So here is the first part"));
        assert_eq!(t.take_turn(), None, "a fragment became a turn before he was finished");
        t.utterance_started();
        assert_eq!(t.take_turn(), None);
        t.utterance_admitted(2640);
        t.recognized(words("and here is the rest of it."));
        assert_eq!(t.take_turn(), None, "released without the end-of-turn quiet");
        t.quiet();
        let turn = t.take_turn().expect("he was finished and nothing was released");
        assert_eq!(turn.text, "So here is the first part and here is the rest of it.");
        assert_eq!(turn.fragments, 2);
        assert_eq!(turn.audio_ms, 2512 + 2640);
        assert_eq!(t.take_turn(), None, "one turn was released twice");
    }

    /// INVARIANT: the quiet does not release a turn while a fragment is still at the recognizer
    /// — its words would arrive as a turn of their own a moment later, which is the defect again.
    #[test]
    fn the_turn_waits_for_the_recognizer() {
        let mut t = TurnAssembler::new();
        t.utterance_started();
        t.utterance_admitted(1500);
        t.recognized(words("First part"));
        t.utterance_started();
        t.utterance_admitted(1500);
        t.quiet();
        assert_eq!(t.take_turn(), None, "released while the last fragment was being recognized");
        t.recognized(words("second part."));
        assert_eq!(t.take_turn().unwrap().text, "First part second part.");
    }

    /// INVARIANT: speech after the quiet voids it. A quiet that arrived and was then followed by
    /// a new utterance does not release the turn mid-utterance.
    #[test]
    fn talking_again_voids_the_quiet() {
        let mut t = TurnAssembler::new();
        t.utterance_started();
        t.utterance_admitted(1500);
        t.recognized(words("One"));
        t.quiet();
        t.utterance_started();
        assert_eq!(t.take_turn(), None, "released while he was talking");
        t.utterance_admitted(1500);
        t.recognized(words("two."));
        assert_eq!(t.take_turn(), None, "the old quiet was reused");
        t.quiet();
        assert_eq!(t.take_turn().unwrap().text, "One two.");
    }

    /// INVARIANT: a refused fragment adds nothing and blocks nothing, and a turn of nothing but
    /// refusals is no turn at all.
    #[test]
    fn refusals_add_nothing_and_block_nothing() {
        let mut t = TurnAssembler::new();
        t.utterance_started();
        t.utterance_admitted(900);
        t.recognized(None);
        t.quiet();
        assert_eq!(t.take_turn(), None, "a turn of nothing was released");

        t.utterance_started();
        t.utterance_admitted(1200);
        t.recognized(words("Book it."));
        t.utterance_started();
        t.utterance_discarded();
        t.utterance_started();
        t.utterance_admitted(800);
        t.recognized(None);
        t.quiet();
        let turn = t.take_turn().unwrap();
        assert_eq!(turn.text, "Book it.");
        assert_eq!(turn.audio_ms, 1200, "a refused fragment's audio was counted");
    }

    /// INVARIANT: provenance is the most suspect fragment's.
    #[test]
    fn a_turn_is_as_suspect_as_its_most_suspect_fragment() {
        let mut t = TurnAssembler::new();
        t.utterance_started();
        t.utterance_admitted(1000);
        t.recognized(words("Yes"));
        t.utterance_started();
        t.utterance_admitted(1000);
        t.recognized(Some(("and no".to_string(), true)));
        t.quiet();
        assert!(t.take_turn().unwrap().rich_audible);
        t.utterance_started();
        t.utterance_admitted(1000);
        t.recognized(words("Next."));
        t.quiet();
        assert!(!t.take_turn().unwrap().rich_audible, "the last turn's provenance leaked");
    }

    /// INVARIANT: switching voice off keeps what was already understood, as before.
    #[test]
    fn voice_off_keeps_what_was_understood() {
        let mut t = TurnAssembler::new();
        t.utterance_started();
        t.utterance_admitted(1000);
        t.recognized(words("Send the deck."));
        t.utterance_started();
        assert_eq!(t.flush().unwrap().text, "Send the deck.");
        assert_eq!(t.flush(), None);
    }
}

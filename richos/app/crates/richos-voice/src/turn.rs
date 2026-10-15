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
//! rule below ([`AnswerGate`]), not this window.
//!
//! ## The floor: Rich does not START talking while the CEO has it
//!
//! In the same log, Rich's answer became audible 3.719 s after the turn was handed over
//! (52.877 -> 56.596). A window covers the pauses it covers; a person who thinks for longer than
//! it and then goes on is mid-thought again exactly when that answer arrives. So the CEO HOLDS
//! THE FLOOR ([`TurnAssembler::has_floor`]) from the moment an utterance he began opens, while
//! any fragment of his is at the recognizer, and while any recognized words of his are not yet a
//! turn. While he holds it:
//!
//! - **Rich's sentences wait.** They are held, in order, and are not handed to the speaker, so
//!   nothing of Rich is audible and the half-duplex rule does not discard what the CEO is saying.
//!   When he gives the floor back without having said anything (a cough, a refused fragment) the
//!   held answer is spoken, unchanged.
//! - **Words of his that start a new turn while an answer is still unsaid SUPERSEDE it.** That
//!   answer was to a thought he had not finished; it stays on screen, and the rest of it is not
//!   spoken. His new words become the next turn, and Rich answers that one aloud.
//!
//! The same mute follows a barge-in: once he has taken the floor back from an answer, the rest of
//! that answer does not resume over him. Every mute ends when the next turn starts.
//!
//! Pure: no clock, no thread, no channel. The supervisor feeds it events in the order it sees
//! them, so the whole rule is unit-testable and replayable over a recording.

use crate::endpoint::SILENCE_HANGOVER_FRAMES;
use crate::vad::frames_to_secs;
use std::collections::VecDeque;

/// Silence, counted from his last speech frame, after which his turn is over.
/// 125 x 256 / 16000 = 2.000 s.
pub const END_OF_TURN_FRAMES: u32 = 125;

/// After an utterance that ended in silence, how long after its END the turn's quiet arrives: the
/// endpointer's own [`SILENCE_HANGOVER_FRAMES`] are the first 0.800 s of the same silence, so
/// 125 - 50 = 75 frames, 75 x 256 / 16000 = 1.200 s. The capture path counts silence since the
/// last speech frame rather than frames since the END, so an utterance cut at the 30.000 s window
/// while he is still talking waits the full 2.000 s (review rv-20261013T163358Z, finding 2).
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
    /// The open utterance began while Rich was NOT audible, so it is presumed to be the CEO
    /// rather than Rich's own echo. An utterance born while Rich was audible never holds the
    /// floor: on speakers Rich's own voice opens one after nearly every sentence (the half-duplex
    /// discards in `controller.rs`), and letting it hold the floor would make Rich stall on his
    /// own echo.
    his_open: bool,
}

impl TurnAssembler {
    pub fn new() -> Self {
        TurnAssembler::default()
    }

    /// The endpointer opened an utterance. He is talking: whatever quiet had been counted is
    /// void.
    pub fn utterance_started(&mut self) {
        self.open = true;
        self.his_open = false;
        self.quiet = false;
    }

    /// The same, for an utterance that began while Rich was not audible: the CEO now holds the
    /// floor ([`Self::has_floor`]).
    pub fn his_utterance_started(&mut self) {
        self.utterance_started();
        self.his_open = true;
    }

    /// The utterance closed and went to the recognizer. `audio_ms` is its recorded length.
    pub fn utterance_admitted(&mut self, audio_ms: u64) {
        self.open = false;
        self.his_open = false;
        self.awaiting.push_back(audio_ms);
    }

    /// The utterance closed and was thrown away before the recognizer (a cough, or audio the
    /// half-duplex rule refused). Nothing of it joins the turn.
    pub fn utterance_discarded(&mut self) {
        self.open = false;
        self.his_open = false;
    }

    /// The recognizer finished one admitted utterance, in the order they were admitted. `None`
    /// is a refusal — no voice in it, or whisper's own noise — and adds nothing to the turn.
    ///
    /// Returns `true` when these words START a new turn: nothing of his was held before them. That
    /// is the moment an answer still unsaid stops being an answer to what he is saying.
    pub fn recognized(&mut self, words: Option<(String, bool)>) -> bool {
        let audio_ms = self.awaiting.pop_front().unwrap_or(0);
        if let Some((text, rich_audible)) = words {
            let text = text.trim();
            if !text.is_empty() {
                let starts_a_turn = self.words.is_empty();
                self.words.push(text.to_string());
                self.rich_audible |= rich_audible;
                self.audio_ms += audio_ms;
                return starts_a_turn;
            }
        }
        false
    }

    /// **The CEO has the floor**: an utterance he began is open, a fragment of his is at the
    /// recognizer, or words of his are not yet a turn. Rich does not start speaking while this is
    /// true ([`AnswerGate`]).
    pub fn has_floor(&self) -> bool {
        self.his_open || !self.awaiting.is_empty() || !self.words.is_empty()
    }

    /// The capture path counted [`END_OF_TURN_FRAMES`] of silence since his last speech.
    pub fn quiet(&mut self) {
        self.quiet = true;
    }

    /// **His earlier turn joins this one** (review rv-20261013T184256Z, finding 4). He paused
    /// for longer than the window, so `earlier` was handed over; then he went on before Rich
    /// had said a word of the answer. The words he is saying now are the rest of the same thought,
    /// so the turn that stands is the whole of it: `earlier` first, then what follows.
    pub fn carry(&mut self, earlier: Turn) {
        self.words.insert(0, earlier.text);
        self.rich_audible |= earlier.rich_audible;
        self.audio_ms += earlier.audio_ms;
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
        self.his_open = false;
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

/// **Is Rich audible, in the sense the capture path's taint rule needs?** His answer's window is
/// open AND at least one sample of that answer has reached the device.
///
/// The window (`controller::AudibleWindow`) opens the moment a sentence is OWED, before `say` has
/// produced a sample, which is right for the gaps between sentences of an answer he is already
/// hearing. Before the FIRST sample it is wrong: nothing of Rich is in the room, so speech that
/// begins then is the CEO's, and calling it Rich's made it tainted, kept it from claiming the floor,
/// and let the sentence in synthesis play over him (review rv-20261013T173416Z, finding 1: he
/// resumed 128 ms into a 336 ms synthesis and three words were discarded).
pub fn rich_audible(window_open: bool, answer_sounded: bool) -> bool {
    window_open && answer_sounded
}

/// **May a sentence Rich has finished synthesizing be played now?** Not while the CEO has the
/// floor. The gate in front of synthesis cannot see speech that starts DURING synthesis, so the
/// speaker asks again here, after it, and waits; a superseded sentence is then dropped by its
/// generation. It also waits while his onset is building (`CaptureBrain::his_onset_building`):
/// the floor is only his 0.112 s into a word, and a sentence that sounded inside those 0.112 s
/// would take the word from him (review rv-20261013T184256Z, finding 2). That signal stays up
/// while his recording is open, so the supervisor's 25 ms publication of the floor leaves no gap
/// between the two (review rv-20261013T192726Z).
pub fn may_sound(his_floor: bool, his_onset: bool) -> bool {
    !his_floor && !his_onset
}

/// **Rich's sentences, held while the CEO has the floor.** See the module documentation.
///
/// Generic over what a sentence is, so the controller can keep its own message type in it and
/// the rule stays testable without a speaker thread. Pure: the caller says whether he has the
/// floor; this decides what may be spoken now.
#[derive(Debug)]
pub struct AnswerGate<T> {
    /// Held sentences in order, each with whether it is an announcement (a job's result, which
    /// belongs to no answer) rather than a sentence of the answer.
    held: VecDeque<(T, bool)>,
    /// The answer in progress was superseded or interrupted: nothing more of it is spoken.
    muted: bool,
}

impl<T> Default for AnswerGate<T> {
    fn default() -> Self {
        AnswerGate { held: VecDeque::new(), muted: false }
    }
}

impl<T> AnswerGate<T> {
    pub fn new() -> Self {
        AnswerGate::default()
    }

    /// A sentence of Rich's is ready. `Some` means speak it now. It is held instead while he has
    /// the floor, and also while earlier sentences are still held, so an answer is never spoken
    /// out of order. A muted answer's sentences are dropped.
    pub fn offer(&mut self, sentence: T, his_floor: bool) -> Option<T> {
        if self.muted {
            return None;
        }
        self.hold_or_pass(sentence, his_floor, false)
    }

    /// A sentence that belongs to no answer: a job's result, said on its own ("Will is on your
    /// team now"). The mute is about the answer he stopped or went on past, and this is not part
    /// of it, so it is spoken even after he stopped Rich and no turn has started since (review
    /// rv-20261015T125238Z-37f970a3, finding 1). Everything else is [`AnswerGate::offer`]: held
    /// while he has the floor, and behind sentences already held, so it never talks over him and
    /// never jumps ahead of Rich. Once held it outlives the answer it waited behind: see
    /// [`AnswerGate::supersede`].
    pub fn offer_announcement(&mut self, sentence: T, his_floor: bool) -> Option<T> {
        self.hold_or_pass(sentence, his_floor, true)
    }

    fn hold_or_pass(&mut self, sentence: T, his_floor: bool, announcement: bool) -> Option<T> {
        if his_floor || !self.held.is_empty() {
            self.held.push_back((sentence, announcement));
            return None;
        }
        Some(sentence)
    }

    /// The held sentences, in order, once he has given the floor back. Nothing while he has it.
    pub fn release(&mut self, his_floor: bool) -> Vec<T> {
        self.release_marked(his_floor).into_iter().map(|(sentence, _)| sentence).collect()
    }

    /// [`AnswerGate::release`], each sentence with whether it is an announcement. The caller
    /// needs that because an announcement may have been held across a superseded answer, whose
    /// generation is over, and it is still to be said.
    pub fn release_marked(&mut self, his_floor: bool) -> Vec<(T, bool)> {
        if his_floor {
            return Vec::new();
        }
        self.held.drain(..).collect()
    }

    /// He went on, or took the floor back, before this answer was said: drop the answer's held
    /// sentences and speak nothing more of it. Returns how many were dropped. A held
    /// announcement is not part of the answer and stays held, in order, until the floor is free
    /// (review rv-20261015T131103Z-6b6f899d, finding 1: his question while a hiring finished
    /// was recognized, superseded the answer, and the "joined your team" notice went with it).
    pub fn supersede(&mut self) -> usize {
        self.muted = true;
        let before = self.held.len();
        self.held.retain(|(_, announcement)| *announcement);
        before - self.held.len()
    }

    /// Announcements that had already left the gate and were cut before he heard them (see
    /// [`Unheard::reclaim`]) are held again, at the front and in their order: they were sent
    /// before anything held now. Not dropped by the mute, like any announcement.
    pub fn hold_again(&mut self, announcements: Vec<T>) {
        for sentence in announcements.into_iter().rev() {
            self.held.push_front((sentence, true));
        }
    }

    /// A new turn started: its answer is his to hear.
    pub fn next_turn(&mut self) {
        self.muted = false;
    }

    pub fn held(&self) -> usize {
        self.held.len()
    }

    /// Held sentences of the ANSWER, announcements not counted: what his new words would leave
    /// unsaid.
    pub fn held_answer(&self) -> usize {
        self.held.iter().filter(|(_, announcement)| !announcement).count()
    }

    pub fn muted(&self) -> bool {
        self.muted
    }
}

/// **Announcements past the gate that he has not heard yet** (a job's result, "Will is on your
/// team now"). [`AnswerGate`] keeps one only while it waits for him; once sent it is with the
/// speaker (queued, being synthesized, or queued at the device), and an interruption there used
/// to lose it: the generation that ends Rich's answer ended the announcement behind it too
/// (review rv-20261015T132600Z-fdc3dafb, finding 1). This keeps each one until its last sample has
/// actually played, so an interruption can hand it back to be said again.
///
/// Positions are the device's own count of samples: a block queued at the device ends at
/// "samples played so far + samples queued", and it has been heard once that many have played.
/// Only an interruption empties the device queue, and every interruption calls
/// [`Unheard::reclaim`], so a position is never compared across one.
#[derive(Debug)]
pub struct Unheard<T> {
    next_id: u64,
    sent: Vec<Sent<T>>,
}

#[derive(Debug)]
struct Sent<T> {
    id: u64,
    item: T,
    /// Where its first sample sits in the device's count, once it is queued there.
    start: Option<u64>,
    /// Where its last sample queued so far ends.
    through: u64,
    /// Its last block is queued: once `through` has played, he has heard all of it.
    complete: bool,
}

impl<T> Default for Unheard<T> {
    fn default() -> Self {
        Unheard { next_id: 1, sent: Vec::new() }
    }
}

impl<T> Unheard<T> {
    /// It goes to the speaker now. Returns its id, which the speaker reports against.
    pub fn sent(&mut self, item: T) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.sent.push(Sent { id, item, start: None, through: 0, complete: false });
        id
    }

    /// A block of it reached the device queue, from `start` to `end` in the device's count.
    pub fn queued(&mut self, id: u64, start: u64, end: u64) {
        if let Some(s) = self.sent.iter_mut().find(|s| s.id == id) {
            s.start.get_or_insert(start);
            s.through = end;
        }
    }

    /// Its last block is queued at the device.
    pub fn complete(&mut self, id: u64) {
        if let Some(s) = self.sent.iter_mut().find(|s| s.id == id) {
            s.complete = true;
        }
    }

    /// It will never reach the device (the speaker is gone, or synthesis failed and he was told).
    pub fn forget(&mut self, id: u64) {
        self.sent.retain(|s| s.id != id);
    }

    /// Forget every one that has played to its last sample.
    pub fn heard(&mut self, played: u64) {
        self.sent.retain(|s| !(s.complete && played >= s.through));
    }

    /// **The device queue was just cut** (he talked over Rich, his words started a new turn, or he
    /// pressed Stop). Every announcement not fully played is returned, in order, to be held and
    /// said again; the rest are forgotten. `he_stopped`: the one that was SOUNDING when he pressed
    /// Stop is what he stopped, so it is not said again, or Stop could never end it; nor is any
    /// later part of the same announcement (`same`: two items are sentences of one job result),
    /// or Stop would end one sentence and the rest of the result would be said after it (review
    /// rv-20261015T153739Z-e8097367-ffad, finding 1). A separate announcement is still returned.
    pub fn reclaim(&mut self, played: u64, he_stopped: bool, same: impl Fn(&T, &T) -> bool) -> Vec<T> {
        let sent: Vec<Sent<T>> = self.sent.drain(..).collect();
        let sounding = |s: &Sent<T>| s.start.is_some_and(|start| played > start);
        let stopped: Vec<&T> =
            if he_stopped { sent.iter().filter(|s| sounding(s)).map(|s| &s.item).collect() } else { Vec::new() };
        let cut: Vec<bool> = sent
            .iter()
            .map(|s| {
                let heard = s.complete && played >= s.through;
                let stopped_by_him = he_stopped && (sounding(s) || stopped.iter().any(|t| same(t, &s.item)));
                heard || stopped_by_him
            })
            .collect();
        sent.into_iter().zip(cut).filter(|(_, cut)| !cut).map(|(s, _)| s.item).collect()
    }

    /// How many are with the speaker or the device and not yet heard.
    pub fn len(&self) -> usize {
        self.sent.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sent.is_empty()
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

    /// INVARIANT: the floor is his from his first word until his words are a turn, and an
    /// utterance born while Rich was audible (his own echo, on speakers) never holds it.
    #[test]
    fn the_floor_is_his_from_his_first_word_until_his_turn_is_handed_over() {
        let mut t = TurnAssembler::new();
        assert!(!t.has_floor());
        t.utterance_started();
        assert!(!t.has_floor(), "an utterance born while Rich was audible held the floor");
        t.utterance_discarded();
        t.his_utterance_started();
        assert!(t.has_floor(), "he is talking and the floor is not his");
        t.utterance_admitted(1000);
        assert!(t.has_floor(), "his words are at the recognizer and the floor was given away");
        assert!(t.recognized(words("Let me think")), "his first words did not start a turn");
        assert!(t.has_floor(), "his words are not yet a turn and the floor was given away");
        t.his_utterance_started();
        t.utterance_admitted(1000);
        assert!(!t.recognized(words("about it.")), "a second fragment started a second turn");
        t.quiet();
        assert!(t.take_turn().is_some());
        assert!(!t.has_floor(), "the turn was handed over and he still holds the floor");
    }

    /// INVARIANT: a refused fragment gives the floor back and starts nothing.
    #[test]
    fn a_refused_fragment_gives_the_floor_back() {
        let mut t = TurnAssembler::new();
        t.his_utterance_started();
        t.utterance_admitted(600);
        assert!(!t.recognized(None));
        assert!(!t.has_floor());
    }

    /// INVARIANT: while he has the floor Rich's sentences wait, in order, and are said, unchanged,
    /// when he gives it back without having said anything.
    #[test]
    fn rich_waits_while_he_has_the_floor_and_then_says_it_all_in_order() {
        let mut g = AnswerGate::new();
        assert_eq!(g.offer("one", false), Some("one"));
        assert_eq!(g.offer("two", true), None);
        assert_eq!(g.offer("three", false), None, "a later sentence overtook a held one");
        assert!(g.release(true).is_empty(), "released while he still had the floor");
        assert_eq!(g.release(false), vec!["two", "three"]);
        assert_eq!(g.offer("four", false), Some("four"));
    }

    /// INVARIANT: a superseded answer is never said, not what is held and not what arrives after,
    /// until the next turn starts.
    #[test]
    fn a_superseded_answer_is_never_said() {
        let mut g = AnswerGate::new();
        assert_eq!(g.offer("held", true), None);
        assert_eq!(g.supersede(), 1);
        assert!(g.release(false).is_empty(), "a superseded sentence was released");
        assert_eq!(g.offer("late", false), None, "the rest of a superseded answer was said");
        g.next_turn();
        assert_eq!(g.offer("next turn", false), Some("next turn"));
    }

    /// INVARIANT — the review's case (rv-20261015T125238Z-37f970a3, finding 1, fixture
    /// `work-result-gate.rs`). He tapped Stop on a spoken reply and waited; no turn started. A
    /// job's result that arrives then is still said; the stopped answer is still not; and the
    /// result waits while he has the floor.
    #[test]
    fn a_result_after_he_stopped_rich_is_still_said_and_never_over_him() {
        let mut g = AnswerGate::new();
        g.supersede(); // `force_barge_in`: tap-to-stop. No `next_turn` follows.
        let joined = "Will (Marketing Director) is on your team now, ready to take work.";
        assert_eq!(g.offer_announcement(joined, false), Some(joined), "the joined notice was dropped");
        assert_eq!(g.offer("rest of the stopped reply", false), None, "the stopped reply was said");

        assert_eq!(g.offer_announcement(joined, true), None, "the notice talked over him");
        assert!(g.release(true).is_empty(), "released while he still had the floor");
        assert_eq!(g.release(false), vec![joined], "the held notice was lost");

        // Behind a sentence of Rich's that is waiting for him, never ahead of it.
        let mut g = AnswerGate::new();
        assert_eq!(g.offer("Rich's sentence", true), None);
        assert_eq!(g.offer_announcement(joined, false), None, "the notice jumped ahead of Rich");
        assert_eq!(g.release(false), vec!["Rich's sentence", joined]);
    }

    /// INVARIANT: an earlier turn carried into his continuation comes first, and the turn that
    /// stands holds both.
    #[test]
    fn a_carried_turn_comes_first() {
        let mut t = TurnAssembler::new();
        t.his_utterance_started();
        t.utterance_admitted(1000);
        t.recognized(words("after I approve it."));
        t.carry(Turn { text: "Send the draft".into(), rich_audible: false, fragments: 1, audio_ms: 1500 });
        t.quiet();
        let turn = t.take_turn().unwrap();
        assert_eq!(turn.text, "Send the draft after I approve it.");
        assert_eq!(turn.audio_ms, 2500);
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

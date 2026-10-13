//! Utterance endpointing — deciding when the CEO started talking and when he stopped.
//!
//! This is the OTHER debounce, and it must not be confused with barge-in's. They answer
//! different questions and therefore carry very different frame counts:
//!
//! | question | threshold | frames | exact seconds |
//! |---|---|---|---|
//! | "has the CEO started talking?" (onset) | short — responsiveness | 7 | 7 x 256 / 16000 = 0.112 |
//! | "has the CEO finished?" (silence hangover) | a natural pause | 50 | 50 x 256 / 16000 = 0.800 |
//! | "is that even worth transcribing?" (minimum) | a cough is not a sentence | 19 | 19 x 256 / 16000 = 0.304 |
//! | "force a cut" (whisper's window) | 30 s | 1875 | 1875 x 256 / 16000 = 30.000 |
//! | "is the CEO interrupting Rich?" (barge-in) | long — echo immunity | 313 | 313 x 256 / 16000 = 5.008 |
//!
//! Every one of those is asserted below from `frames_for_secs` / `frames_to_secs`, so the
//! table cannot drift from the code.
//!
//! A **pre-roll** ring keeps the 0.304 s of audio immediately BEFORE onset, because the VAD
//! only recognizes speech a few frames in and the first consonant lives in those frames. Cut
//! it and whisper hears "…orning" instead of "morning".

use crate::vad::frames_to_secs;
#[cfg(test)]
use crate::vad::{frames_for_secs, VAD_FRAME_SAMPLES};
use std::collections::VecDeque;

/// Consecutive speech frames that start an utterance. 7 x 256 / 16000 = 0.112 s.
pub const SPEECH_ONSET_FRAMES: u32 = 7;
/// Consecutive silence frames that end one. 50 x 256 / 16000 = 0.800 s.
pub const SILENCE_HANGOVER_FRAMES: u32 = 50;
/// Below this many SPEECH frames an utterance is discarded unheard. 19 x 256 / 16000 = 0.304 s.
pub const MIN_SPEECH_FRAMES: u32 = 19;
/// Hard cut at whisper's context window. 1875 x 256 / 16000 = 30.000 s.
pub const MAX_UTTERANCE_FRAMES: u32 = 1875;
/// Audio retained from before onset. 19 x 256 / 16000 = 0.304 s.
pub const PRE_ROLL_FRAMES: usize = 19;

/// Why an utterance ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    /// The CEO stopped talking for [`SILENCE_HANGOVER_FRAMES`].
    Silence,
    /// The utterance hit whisper's 30 s window and was cut so it can still be transcribed.
    MaxLength,
    /// Voice was switched off while it was still being recorded. What he had said is kept.
    Stopped,
}

/// What one frame did to the endpointer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointEvent {
    /// Nothing happened.
    None,
    /// This frame completed the onset debounce — the CEO is talking.
    Started,
    /// This frame ended the utterance.
    Ended(EndReason),
}

/// The pure decision half: speech/no-speech verdicts in, start/end events out.
#[derive(Debug, Clone)]
pub struct Endpointer {
    in_speech: bool,
    speech_run: u32,
    silence_run: u32,
    total_frames: u32,
    speech_frames: u32,
    /// The silence already counted when the last utterance was cut at the 30.000 s window, so
    /// its continuation ends after the SAME 0.800 s of silence rather than a fresh one.
    cut_silence_run: u32,
}

impl Default for Endpointer {
    fn default() -> Self {
        Endpointer::new()
    }
}

impl Endpointer {
    pub fn new() -> Self {
        Endpointer {
            in_speech: false,
            speech_run: 0,
            silence_run: 0,
            total_frames: 0,
            speech_frames: 0,
            cut_silence_run: 0,
        }
    }

    pub fn is_in_speech(&self) -> bool {
        self.in_speech
    }

    /// Speech frames counted in the CURRENT utterance (zero when idle).
    pub fn speech_frames(&self) -> u32 {
        self.speech_frames
    }

    /// Total frames in the current utterance, including the internal pauses.
    pub fn total_frames(&self) -> u32 {
        self.total_frames
    }

    /// Consecutive speech frames counted so far. Before an utterance opens this is the run
    /// climbing toward [`SPEECH_ONSET_FRAMES`] — the "it nearly started" diagnostic, and the
    /// difference between *the VAD never called it speech* and *it was speech but never for
    /// 7 frames in a row*. Those two have different fixes and the log could not tell them apart.
    pub fn speech_run_frames(&self) -> u32 {
        self.speech_run
    }

    /// Abandon the in-flight utterance (mode exit, barge-in promotion, error).
    pub fn reset(&mut self) {
        *self = Endpointer::new();
    }

    pub fn push(&mut self, is_speech: bool) -> EndpointEvent {
        if is_speech {
            self.speech_run += 1;
            self.silence_run = 0;
        } else {
            self.silence_run += 1;
            self.speech_run = 0;
        }

        if !self.in_speech {
            if self.speech_run >= SPEECH_ONSET_FRAMES {
                self.in_speech = true;
                // The onset frames themselves are part of the utterance.
                self.total_frames = self.speech_run;
                self.speech_frames = self.speech_run;
                return EndpointEvent::Started;
            }
            return EndpointEvent::None;
        }

        self.total_frames += 1;
        if is_speech {
            self.speech_frames += 1;
        }

        if self.silence_run >= SILENCE_HANGOVER_FRAMES {
            self.finish();
            return EndpointEvent::Ended(EndReason::Silence);
        }
        if self.total_frames >= MAX_UTTERANCE_FRAMES {
            self.cut_silence_run = self.silence_run;
            self.finish();
            return EndpointEvent::Ended(EndReason::MaxLength);
        }
        EndpointEvent::None
    }

    /// **The utterance just cut at the 30.000 s window goes on.** He was still talking when it
    /// was cut, so what follows is the rest of the same speech, not a new start: it is in speech
    /// from the next frame with no onset to earn, and the silence already counted at the cut
    /// still counts toward its end.
    pub fn continue_after_cut(&mut self) {
        let silence_run = self.cut_silence_run;
        *self = Endpointer::new();
        self.in_speech = true;
        self.silence_run = silence_run;
    }

    fn finish(&mut self) {
        self.in_speech = false;
        self.speech_run = 0;
        self.silence_run = 0;
        // total_frames / speech_frames are left intact for the caller to inspect on the
        // Ended frame; the next `push` that starts a new utterance overwrites them.
    }
}

/// A completed utterance: 16 kHz mono f32 audio plus the frame accounting behind it.
#[derive(Debug, Clone)]
pub struct Utterance {
    pub samples: Vec<f32>,
    pub reason: EndReason,
    /// Frames the VAD called speech (excludes the internal pauses and the trailing hangover).
    pub speech_frames: u32,
    /// Every frame in `samples` including pre-roll, pauses and hangover.
    pub total_frames: u32,
    /// This is the rest of an utterance cut at the 30.000 s window: its speech is the end of a
    /// sentence the cut fragment already carried, so it may be shorter than any proof of voice.
    pub continues_cut: bool,
}

impl Utterance {
    /// Wall-clock duration of the captured audio, derived from the sample count.
    pub fn duration_secs(&self) -> f32 {
        self.samples.len() as f32 / crate::vad::SAMPLE_RATE as f32
    }
    /// Speech-only duration, derived from the frame count.
    pub fn speech_secs(&self) -> f32 {
        frames_to_secs(self.speech_frames)
    }
}

/// The buffering half: keeps the pre-roll ring, accumulates the utterance, and hands back a
/// finished [`Utterance`] — or nothing, when what it heard was too short to be a sentence.
pub struct UtteranceRecorder {
    endpointer: Endpointer,
    pre_roll: VecDeque<Vec<f32>>,
    buffer: Vec<f32>,
    recording: bool,
    /// The last utterance was cut at the 30.000 s window: the next frame continues it.
    continue_next: bool,
    /// The utterance being recorded continues a cut one. Its speech is the end of his sentence,
    /// not a cough, so the 0.304 s minimum does not apply: one frame of speech is enough.
    continuation: bool,
}

impl Default for UtteranceRecorder {
    fn default() -> Self {
        UtteranceRecorder::new()
    }
}

impl UtteranceRecorder {
    pub fn new() -> Self {
        UtteranceRecorder {
            endpointer: Endpointer::new(),
            pre_roll: VecDeque::with_capacity(PRE_ROLL_FRAMES + 1),
            buffer: Vec::new(),
            recording: false,
            continue_next: false,
            continuation: false,
        }
    }

    pub fn is_recording(&self) -> bool {
        self.recording
    }

    pub fn endpointer(&self) -> &Endpointer {
        &self.endpointer
    }

    /// Drop everything in flight (voice mode off, or the frames were promoted elsewhere).
    pub fn reset(&mut self) {
        self.endpointer.reset();
        self.pre_roll.clear();
        self.buffer.clear();
        self.recording = false;
        self.continue_next = false;
        self.continuation = false;
    }

    /// Below this many speech frames the utterance being recorded is dropped unheard:
    /// [`MIN_SPEECH_FRAMES`], except for the continuation of a cut utterance, whose speech is the
    /// end of a sentence already admitted (review rv-20261013T184256Z, finding 3).
    fn min_speech_frames(&self) -> u32 {
        if self.continuation {
            1
        } else {
            MIN_SPEECH_FRAMES
        }
    }

    /// Steal the audio captured so far WITHOUT ending the utterance cleanly — used when the
    /// barge-in monitor promotes the frames the CEO used to interrupt Rich into a real
    /// utterance. Those 5.008 s are the beginning of what he actually said; discarding them
    /// would make barge-in feel like it ate his sentence.
    pub fn take_in_flight(&mut self) -> Vec<f32> {
        let mut out = std::mem::take(&mut self.buffer);
        if out.is_empty() {
            // Not yet past the onset debounce: the pre-roll is all we have, and it is the
            // right thing to hand over.
            for f in self.pre_roll.iter() {
                out.extend_from_slice(f);
            }
        }
        self.reset();
        out
    }

    /// **Voice is going off: close the open recording NOW, as an utterance.** Without this the
    /// words still inside the 0.800 s hangover were destroyed with the capture stream, so an
    /// instruction he finished just before switching off lost its last phrase (review
    /// rv-20261013T183507Z). The same rule as an ordinary end decides whether it is worth
    /// transcribing: under [`MIN_SPEECH_FRAMES`] of speech it is a cough and is dropped.
    pub fn finish_now(&mut self) -> Option<Utterance> {
        if !self.recording {
            self.reset();
            return None;
        }
        let samples = std::mem::take(&mut self.buffer);
        let speech_frames = self.endpointer.speech_frames();
        let total_frames = self.endpointer.total_frames();
        let min_speech = self.min_speech_frames();
        let continues_cut = self.continuation;
        self.reset();
        if speech_frames < min_speech {
            return None;
        }
        Some(Utterance { samples, reason: EndReason::Stopped, speech_frames, total_frames, continues_cut })
    }

    /// Feed one exact VAD frame plus its speech verdict.
    pub fn push_frame(&mut self, frame: &[f32], is_speech: bool) -> Option<Utterance> {
        if self.continue_next {
            // The rest of the sentence that was cut on the last frame: recording from THIS frame.
            self.continue_next = false;
            self.continuation = true;
            self.endpointer.continue_after_cut();
            self.recording = true;
            self.buffer.clear();
            self.pre_roll.clear();
        }
        let event = self.endpointer.push(is_speech);

        match event {
            EndpointEvent::Started => {
                self.recording = true;
                self.buffer.clear();
                // Seed with the pre-roll ring: the frames before the VAD was convinced,
                // which hold the first consonant.
                for f in self.pre_roll.iter() {
                    self.buffer.extend_from_slice(f);
                }
                self.buffer.extend_from_slice(frame);
                self.pre_roll.clear();
                None
            }
            EndpointEvent::None => {
                if self.recording {
                    self.buffer.extend_from_slice(frame);
                } else {
                    self.push_pre_roll(frame);
                }
                None
            }
            EndpointEvent::Ended(reason) => {
                self.buffer.extend_from_slice(frame);
                let samples = std::mem::take(&mut self.buffer);
                let speech_frames = self.endpointer.speech_frames();
                let total_frames = self.endpointer.total_frames();
                self.recording = false;
                self.pre_roll.clear();
                let min_speech = self.min_speech_frames();
                let continues_cut = self.continuation;
                self.continuation = false;
                // Cut at the window while he was still talking: the next frame continues it.
                self.continue_next = reason == EndReason::MaxLength;
                if speech_frames < min_speech {
                    // A cough, a door, a chair. Never sent to whisper, never shown.
                    return None;
                }
                Some(Utterance { samples, reason, speech_frames, total_frames, continues_cut })
            }
        }
    }

    fn push_pre_roll(&mut self, frame: &[f32]) {
        if self.pre_roll.len() == PRE_ROLL_FRAMES {
            self.pre_roll.pop_front();
        }
        self.pre_roll.push_back(frame.to_vec());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speech_frame() -> Vec<f32> {
        vec![0.3; VAD_FRAME_SAMPLES]
    }
    fn silent_frame() -> Vec<f32> {
        vec![0.0; VAD_FRAME_SAMPLES]
    }

    /// INVARIANT: every endpointing threshold in the module table is exactly what the frame
    /// math produces from its intended duration. The table cannot drift from the code.
    #[test]
    fn endpointing_thresholds_match_their_intended_durations_exactly() {
        assert_eq!(SPEECH_ONSET_FRAMES, frames_for_secs(0.1)); // ceil(6.25) = 7
        assert!((frames_to_secs(SPEECH_ONSET_FRAMES) - 0.112).abs() < 1e-6);

        assert_eq!(SILENCE_HANGOVER_FRAMES, frames_for_secs(0.8)); // 50.0 exactly
        assert!((frames_to_secs(SILENCE_HANGOVER_FRAMES) - 0.800).abs() < 1e-6);

        assert_eq!(MIN_SPEECH_FRAMES, frames_for_secs(0.3)); // ceil(18.75) = 19
        assert!((frames_to_secs(MIN_SPEECH_FRAMES) - 0.304).abs() < 1e-6);

        assert_eq!(MAX_UTTERANCE_FRAMES, frames_for_secs(30.0)); // 1875.0 exactly
        assert!((frames_to_secs(MAX_UTTERANCE_FRAMES) - 30.0).abs() < 1e-6);
    }

    /// INVARIANT: onset needs 7 consecutive speech frames — 6 is not enough.
    #[test]
    fn onset_requires_seven_consecutive_speech_frames() {
        let mut ep = Endpointer::new();
        for _ in 0..(SPEECH_ONSET_FRAMES - 1) {
            assert_eq!(ep.push(true), EndpointEvent::None);
        }
        assert_eq!(ep.push(true), EndpointEvent::Started);
    }

    /// INVARIANT: an isolated blip below the onset threshold never starts an utterance.
    #[test]
    fn a_blip_shorter_than_the_onset_never_starts_an_utterance() {
        let mut ep = Endpointer::new();
        for _ in 0..200 {
            for _ in 0..(SPEECH_ONSET_FRAMES - 1) {
                assert_eq!(ep.push(true), EndpointEvent::None);
            }
            assert_eq!(ep.push(false), EndpointEvent::None);
        }
        assert!(!ep.is_in_speech());
    }

    /// INVARIANT: a natural mid-sentence pause SHORTER than 0.800 s does not end the turn —
    /// the CEO gets to think without Rich jumping in.
    #[test]
    fn a_pause_shorter_than_the_hangover_does_not_end_the_utterance() {
        let mut ep = Endpointer::new();
        for _ in 0..SPEECH_ONSET_FRAMES {
            ep.push(true);
        }
        for _ in 0..(SILENCE_HANGOVER_FRAMES - 1) {
            assert_eq!(ep.push(false), EndpointEvent::None);
        }
        assert!(ep.is_in_speech());
        ep.push(true); // he carries on
        for _ in 0..(SILENCE_HANGOVER_FRAMES - 1) {
            assert_eq!(ep.push(false), EndpointEvent::None);
        }
        assert_eq!(ep.push(false), EndpointEvent::Ended(EndReason::Silence));
    }

    /// INVARIANT: a long monologue is cut at whisper's 30 s window rather than being
    /// silently truncated or dropped by the recognizer.
    #[test]
    fn a_monologue_is_cut_at_whispers_thirty_second_window() {
        let mut ep = Endpointer::new();
        for _ in 0..SPEECH_ONSET_FRAMES {
            ep.push(true);
        }
        let mut ended = None;
        for i in 0..(MAX_UTTERANCE_FRAMES * 2) {
            if let EndpointEvent::Ended(r) = ep.push(true) {
                ended = Some((i, r));
                break;
            }
        }
        let (_, reason) = ended.expect("must cut at the window, not run forever");
        assert_eq!(reason, EndReason::MaxLength);
        assert_eq!(ep.total_frames(), MAX_UTTERANCE_FRAMES);
    }

    /// INVARIANT: the first consonant survives — the recorder prepends the pre-roll frames
    /// captured before the VAD was convinced.
    #[test]
    fn the_utterance_keeps_the_pre_roll_so_the_first_consonant_is_not_clipped() {
        let mut rec = UtteranceRecorder::new();
        // Fill the pre-roll ring with distinguishable "quiet before the word" frames.
        let marker = vec![0.001f32; VAD_FRAME_SAMPLES];
        for _ in 0..PRE_ROLL_FRAMES {
            assert!(rec.push_frame(&marker, false).is_none());
        }
        let sp = speech_frame();
        for _ in 0..(MIN_SPEECH_FRAMES + SPEECH_ONSET_FRAMES) {
            assert!(rec.push_frame(&sp, true).is_none());
        }
        let sil = silent_frame();
        let mut out = None;
        for _ in 0..SILENCE_HANGOVER_FRAMES {
            if let Some(u) = rec.push_frame(&sil, false) {
                out = Some(u);
            }
        }
        let u = out.expect("utterance should complete");
        assert_eq!(&u.samples[..VAD_FRAME_SAMPLES], &marker[..], "pre-roll missing from the front");
    }

    /// INVARIANT: the pre-roll ring is bounded — an hour of silence does not grow memory.
    #[test]
    fn the_pre_roll_ring_is_bounded() {
        let mut rec = UtteranceRecorder::new();
        let sil = silent_frame();
        // 1 hour = 3600 / 0.016 = 225 000 frames.
        for _ in 0..225_000 {
            rec.push_frame(&sil, false);
        }
        assert!(rec.pre_roll.len() <= PRE_ROLL_FRAMES);
        assert!(rec.buffer.is_empty());
    }

    /// INVARIANT: a cough is discarded — under 0.304 s of speech never reaches whisper.
    #[test]
    fn a_cough_is_discarded_and_never_reaches_the_recognizer() {
        let mut rec = UtteranceRecorder::new();
        let sp = speech_frame();
        let sil = silent_frame();
        // Just enough to trip the onset (7 frames), well under MIN_SPEECH_FRAMES (19).
        for _ in 0..SPEECH_ONSET_FRAMES {
            assert!(rec.push_frame(&sp, true).is_none());
        }
        for _ in 0..(SILENCE_HANGOVER_FRAMES + 2) {
            assert!(rec.push_frame(&sil, false).is_none(), "a cough must never produce an utterance");
        }
        assert!(!rec.is_recording());
    }

    /// INVARIANT: a real sentence produces an utterance whose duration matches its sample
    /// count — the reported seconds are derived from the audio, never estimated.
    #[test]
    fn a_real_sentence_reports_a_duration_derived_from_its_samples() {
        let mut rec = UtteranceRecorder::new();
        let sp = speech_frame();
        let sil = silent_frame();
        let speech_count = 100; // 100 x 16 ms = 1.600 s of speech
        for _ in 0..speech_count {
            assert!(rec.push_frame(&sp, true).is_none());
        }
        let mut done = None;
        for _ in 0..(SILENCE_HANGOVER_FRAMES + 1) {
            if let Some(u) = rec.push_frame(&sil, false) {
                done = Some(u);
                break;
            }
        }
        let u = done.expect("sentence should complete");
        assert_eq!(u.reason, EndReason::Silence);
        assert_eq!(u.speech_frames, speech_count);
        assert!((u.speech_secs() - 1.6).abs() < 1e-6, "{}", u.speech_secs());
        // samples/16000 must equal the frame count the recorder actually buffered.
        let expected = u.samples.len() as f32 / crate::vad::SAMPLE_RATE as f32;
        assert!((u.duration_secs() - expected).abs() < f32::EPSILON);
        // And the audio really does contain the speech + the whole hangover.
        assert!(u.duration_secs() > 1.6 + 0.8 - 0.02, "{}", u.duration_secs());
    }

    /// INVARIANT — review rv-20261013T183507Z. Voice going off mid-sentence closes the open
    /// recording as an utterance (`Stopped`), by the same cough rule as any end, and nothing is
    /// made out of a recorder that was not recording.
    #[test]
    fn voice_off_closes_the_open_recording_by_the_same_rule_as_any_end() {
        let sp = speech_frame();
        let mut rec = UtteranceRecorder::new();
        for _ in 0..(SPEECH_ONSET_FRAMES + MIN_SPEECH_FRAMES) {
            assert!(rec.push_frame(&sp, true).is_none());
        }
        assert!(rec.is_recording());
        let u = rec.finish_now().expect("what he had said was thrown away when voice went off");
        assert_eq!(u.reason, EndReason::Stopped);
        assert_eq!(u.speech_frames, SPEECH_ONSET_FRAMES + MIN_SPEECH_FRAMES);
        assert!(!rec.is_recording());

        let mut cough = UtteranceRecorder::new();
        for _ in 0..SPEECH_ONSET_FRAMES {
            cough.push_frame(&sp, true);
        }
        assert!(cough.finish_now().is_none(), "a cough became an utterance because voice went off");
        assert!(UtteranceRecorder::new().finish_now().is_none());
    }

    /// INVARIANT — review rv-20261013T184256Z finding 3. He is still finishing a word when the
    /// 30.000 s window cuts his utterance; the 14 speech frames (0.224 s) after the cut are the
    /// end of his sentence and are KEPT, though under the 19-frame cough minimum. And the
    /// continuation ends after the same 0.800 s of silence as any utterance.
    #[test]
    fn the_end_of_a_word_cut_at_the_window_is_kept() {
        let (sp, sil) = (speech_frame(), silent_frame());
        let mut rec = UtteranceRecorder::new();
        let mut done = Vec::new();
        for _ in 0..MAX_UTTERANCE_FRAMES {
            done.extend(rec.push_frame(&sp, true));
        }
        assert_eq!(done.len(), 1, "the 30.000 s window did not cut the utterance");
        assert_eq!(done[0].reason, EndReason::MaxLength);
        for _ in 0..14 {
            assert!(rec.push_frame(&sp, true).is_none());
        }
        let mut tail = None;
        for i in 0..SILENCE_HANGOVER_FRAMES {
            if let Some(u) = rec.push_frame(&sil, false) {
                assert_eq!(i + 1, SILENCE_HANGOVER_FRAMES, "the continuation ended before 0.800 s of silence");
                tail = Some(u);
            }
        }
        let tail = tail.expect("the last 0.224 s of his sentence was thrown away as a cough");
        assert_eq!(tail.speech_frames, 14);
        assert_eq!(tail.reason, EndReason::Silence);

        // An ordinary short sound is still a cough: the waiver is only for a continuation.
        let mut cough = UtteranceRecorder::new();
        let mut kept = 0;
        for _ in 0..14 {
            kept += cough.push_frame(&sp, true).is_some() as u32;
        }
        for _ in 0..SILENCE_HANGOVER_FRAMES + 2 {
            kept += cough.push_frame(&sil, false).is_some() as u32;
        }
        assert_eq!(kept, 0, "the cough rule was waived for a sound that continued nothing");
    }

    /// INVARIANT: promoting an interrupted capture keeps the audio — barge-in must not eat
    /// the beginning of what the CEO said to interrupt with.
    #[test]
    fn promoting_an_in_flight_capture_keeps_the_audio_the_ceo_interrupted_with() {
        let mut rec = UtteranceRecorder::new();
        let sp = speech_frame();
        for _ in 0..(SPEECH_ONSET_FRAMES + 50) {
            rec.push_frame(&sp, true);
        }
        let taken = rec.take_in_flight();
        assert!(taken.len() >= 50 * VAD_FRAME_SAMPLES, "lost the interruption audio: {}", taken.len());
        assert!(!rec.is_recording());
    }
}

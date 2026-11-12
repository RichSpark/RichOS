//! **Where the speech is in a finished dictation, and the pieces it is decoded in.**
//!
//! Whisper decodes 30 seconds at a time. RichScribe used to hand it the whole recording and
//! let it step through in fixed 30-second windows, and two things followed, both measured
//! (`docs/measurements/richscribe-2026-11-12/`): a window that held only the room after the
//! last word came back as "Thank you.", and words at a window's edge were lost.
//!
//! This module is the standard answer, WhisperX's "cut and merge" (also faster-whisper's
//! batched pipeline): a speech detector finds the speech, the speech is cut into pieces that
//! each fit one window, a cut falls in a pause wherever there is one, and each piece is
//! decoded on its own, with the flags one utterance has always been decoded with. No window
//! is ever empty and no word ever straddles two.
//!
//! - [`SpeechDetector`]: Silero VAD 5.1.2, the detector those pipelines use, run in process
//!   through the ONNX Runtime this crate already links for Rich's voice. The model is
//!   compiled in (2.3 MB, MIT), so nothing is downloaded or installed.
//! - [`stretches`]: the probabilities as stretches of speech. Silero's own
//!   `get_speech_timestamps`, pure.
//! - [`pieces`]: the stretches as pieces of the RECORDING: its own audio from a little before
//!   a piece's first word to a little after its last, pauses inside it untouched. Pure.
//!
//! Every decision is a pure function over numbers, so each is proved without a model, a
//! decoder or a microphone, the way `vad.rs` and `voiced.rs` are.

use crate::vad::SAMPLE_RATE;
use ort::session::Session;
use ort::value::Tensor;
use std::ops::Range;

/// Silero VAD 5.1.2, `src/silero_vad/data/silero_vad.onnx` at tag `v5.1.2` of
/// `snakers4/silero-vad` (MIT, `third_party/silero-vad/`).
pub const SILERO: &[u8] = include_bytes!("../third_party/silero-vad/silero_vad.onnx");

/// The SHA-256 of [`SILERO`].
pub const SILERO_SHA256: &str = "2623a2953f6ff3d2c1e61740c6cdb7168133479b267dfef114a4a3cc5bdd788f";

/// Samples per probability: 512 / 16000 = 32 ms, the only frame Silero takes at 16 kHz.
pub const FRAME_SAMPLES: usize = 512;

/// Samples of the previous frame the model is shown again in front of each frame.
const CONTEXT_SAMPLES: usize = 64;

/// The model's recurrent state: 2 x 1 x 128.
const STATE_VALUES: usize = 2 * 128;

/// **The probability at which speech starts.** Silero's own default is 0.5; this is the value
/// the settings table measured for this decoder's detector on 2026-09-10 (section 3), kept so
/// a low-gain microphone's quiet first words are still speech.
pub const SPEECH_AT: f32 = 0.3;

/// The probability under which speech may end: Silero's rule, 0.15 below [`SPEECH_AT`].
pub const SILENCE_UNDER: f32 = SPEECH_AT - 0.15;

/// A pause shorter than this does not end a stretch: 10 frames, 0.32 s.
pub const MIN_PAUSE_FRAMES: usize = 10;

/// A stretch shorter than this is not speech: 3 frames, 0.096 s.
pub const MIN_SPEECH_FRAMES: usize = 3;

/// **The most audio one piece may hold**: 29.5 s, inside Whisper's 30-second window with half
/// a second to spare.
pub const PIECE_SAMPLES: usize = 29 * SAMPLE_RATE as usize + SAMPLE_RATE as usize / 2;

/// **How much of the room is kept around a piece's speech**: 0.3 s on each side, the
/// recording's own sound, so a soft first consonant or a trailing one the detector was late
/// or early on is still inside the piece.
pub const ROOM_SAMPLES: usize = SAMPLE_RATE as usize * 3 / 10;

/// **A pause this long always ends a piece**: 2 s. A piece holds one stretch of talking, and
/// a silence that long inside one window is where the decoder stops early or invents a line.
pub const LONG_PAUSE_SAMPLES: usize = 2 * SAMPLE_RATE as usize;

/// One run of the detector over one recording.
pub struct SpeechDetector {
    session: Session,
}

impl SpeechDetector {
    /// The compiled-in model, ready to run. One thread: a frame is 576 samples.
    pub fn load() -> Result<SpeechDetector, String> {
        let session = Session::builder()
            .map_err(|e| format!("create the speech detector's session builder: {e}"))?
            .with_intra_threads(1)
            .map_err(|e| format!("configure the speech detector's threads: {e}"))?
            .with_inter_threads(1)
            .map_err(|e| format!("configure the speech detector's threads: {e}"))?
            .commit_from_memory(SILERO)
            .map_err(|e| format!("load the speech detector: {e}"))?;
        Ok(SpeechDetector { session })
    }

    /// **The probability of speech in each 32 ms frame** of a 16 kHz recording, in order. A
    /// last partial frame is padded with silence, as Silero's own reader does.
    pub fn probabilities(&mut self, samples: &[f32]) -> Result<Vec<f32>, String> {
        let mut state = vec![0.0f32; STATE_VALUES];
        let mut context = [0.0f32; CONTEXT_SAMPLES];
        let mut out = Vec::with_capacity(samples.len() / FRAME_SAMPLES + 1);
        for frame in samples.chunks(FRAME_SAMPLES) {
            let mut input = Vec::with_capacity(CONTEXT_SAMPLES + FRAME_SAMPLES);
            input.extend_from_slice(&context);
            input.extend_from_slice(frame);
            input.resize(CONTEXT_SAMPLES + FRAME_SAMPLES, 0.0);
            context.copy_from_slice(&input[FRAME_SAMPLES..]);
            let input = Tensor::from_array((vec![1_i64, (CONTEXT_SAMPLES + FRAME_SAMPLES) as i64], input.into_boxed_slice()))
                .map_err(|e| format!("create the speech detector's input: {e}"))?;
            let state_in = Tensor::from_array((vec![2_i64, 1, 128], std::mem::take(&mut state).into_boxed_slice()))
                .map_err(|e| format!("create the speech detector's state: {e}"))?;
            let rate = Tensor::from_array((Vec::<i64>::new(), vec![i64::from(SAMPLE_RATE)].into_boxed_slice()))
                .map_err(|e| format!("create the speech detector's sample rate: {e}"))?;
            let outputs = self.session
                .run(ort::inputs!["input" => input, "state" => state_in, "sr" => rate])
                .map_err(|e| format!("run the speech detector: {e}"))?;
            let (_, probability) = outputs["output"].try_extract_tensor::<f32>()
                .map_err(|e| format!("read the speech detector's probability: {e}"))?;
            let (_, next) = outputs["stateN"].try_extract_tensor::<f32>()
                .map_err(|e| format!("read the speech detector's state: {e}"))?;
            if probability.len() != 1 || next.len() != STATE_VALUES {
                return Err(format!("the speech detector returned {} probabilities and {} state values", probability.len(), next.len()));
            }
            out.push(probability[0]);
            state = next.to_vec();
        }
        Ok(out)
    }
}

/// **The stretches of speech**, as sample ranges, from one probability per frame.
///
/// Silero's `get_speech_timestamps`: speech starts at the first frame at or above
/// [`SPEECH_AT`]; it ends where the probability fell under [`SILENCE_UNDER`] and stayed there
/// for [`MIN_PAUSE_FRAMES`]; a stretch shorter than [`MIN_SPEECH_FRAMES`] is dropped. Speech
/// still running when the recording ends runs to its end.
pub fn stretches(probabilities: &[f32], samples: usize) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut quiet_since: Option<usize> = None;
    let keep = |from: usize, to: usize, out: &mut Vec<Range<usize>>| {
        if to - from >= MIN_SPEECH_FRAMES {
            out.push(from * FRAME_SAMPLES..(to * FRAME_SAMPLES).min(samples));
        }
    };
    for (frame, &p) in probabilities.iter().enumerate() {
        if p >= SPEECH_AT {
            quiet_since = None;
            start.get_or_insert(frame);
        } else if p < SILENCE_UNDER {
            if let Some(from) = start {
                let since = *quiet_since.get_or_insert(frame);
                if frame + 1 - since >= MIN_PAUSE_FRAMES {
                    keep(from, since, &mut out);
                    start = None;
                    quiet_since = None;
                }
            }
        }
    }
    if let Some(from) = start {
        keep(from, quiet_since.unwrap_or(probabilities.len()), &mut out);
    }
    out
}

/// **The pieces a recording is decoded in**, as sample ranges of the recording itself.
///
/// Speech separated by less than [`LONG_PAUSE_SAMPLES`] is one stretch of talking, pauses
/// included. A stretch of talking that fits a window is one piece. One that does not is cut
/// window by window, each cut in the final third of its window: in the middle of the last
/// pause there, so a cut that can fall between two phrases does, and each window is at least
/// two thirds full. Only talking with no pause in that whole third (ten seconds without a
/// stop) is cut at its least speech-like frame, which is where the breath is. A piece then
/// takes up to [`ROOM_SAMPLES`] of the recording at each end it does not share with the next
/// piece, never past the middle of the silence between them. No speech is ever left out.
pub fn pieces(stretches: &[Range<usize>], probabilities: &[f32], samples: usize) -> Vec<Range<usize>> {
    // The most talking a piece holds, so that with its room it still fits one window.
    let budget = PIECE_SAMPLES - 2 * ROOM_SAMPLES;
    let mut talking: Vec<Range<usize>> = Vec::new();
    for stretch in stretches {
        match talking.last_mut() {
            Some(before) if stretch.start - before.end < LONG_PAUSE_SAMPLES => before.end = stretch.end,
            _ => talking.push(stretch.clone()),
        }
    }
    let mut cut: Vec<Range<usize>> = Vec::new();
    for stretch in &talking {
        let mut from = stretch.start;
        while stretch.end - from > budget {
            let to = window_cut(probabilities, from + budget * 2 / 3, from + budget);
            cut.push(from..to);
            from = to;
        }
        cut.push(from..stretch.end);
    }
    (0..cut.len()).map(|n| {
        let piece = &cut[n];
        let before = if n == 0 { 0 } else { cut[n - 1].end + (piece.start - cut[n - 1].end) / 2 };
        let after = if n + 1 == cut.len() { samples } else { piece.end + (cut[n + 1].start - piece.end) / 2 };
        piece.start.saturating_sub(ROOM_SAMPLES).max(before)..(piece.end + ROOM_SAMPLES).min(after)
    }).collect()
}

/// **Where a full window is cut**, as a sample in `from..to`: the middle of the last real
/// pause there ([`MIN_PAUSE_FRAMES`] or more of quiet, the same pause [`stretches`] ends a
/// stretch on). With no real pause there, the middle of the longest dip. With no quiet frame
/// at all, the start of the least speech-like frame.
///
/// A dip of one frame inside a word is not a pause, so it never wins over one (review of
/// 2026-11-12: a 32 ms dip three seconds after a half-second pause used to take the cut).
fn window_cut(probabilities: &[f32], from: usize, to: usize) -> usize {
    let (lo, hi) = (from.div_ceil(FRAME_SAMPLES), (to / FRAME_SAMPLES).min(probabilities.len()));
    let quiet = |frame: usize| probabilities[frame] < SILENCE_UNDER;
    // Every run of quiet frames that touches the range: where it is inside the range, and
    // how long it really is, counted past the range's edges.
    let mut runs: Vec<(Range<usize>, usize)> = Vec::new();
    let mut frame = lo;
    while frame < hi {
        if !quiet(frame) {
            frame += 1;
            continue;
        }
        let (mut first, mut last) = (frame, frame);
        while first > 0 && quiet(first - 1) {
            first -= 1;
        }
        while last + 1 < probabilities.len() && quiet(last + 1) {
            last += 1;
        }
        runs.push((frame..(last + 1).min(hi), last + 1 - first));
        frame = last + 1;
    }
    let chosen = runs.iter().rev().find(|(_, length)| *length >= MIN_PAUSE_FRAMES)
        .or_else(|| runs.iter().rev().max_by_key(|(_, length)| *length));
    match chosen {
        Some((inside, _)) => (inside.start + inside.len() / 2) * FRAME_SAMPLES,
        None => (lo..hi).min_by(|a, b| probabilities[*a].total_cmp(&probabilities[*b])).map_or(to, |frame| frame * FRAME_SAMPLES),
    }
}

/// **The pieces of one recording**: the detector, [`stretches`] and [`pieces`] in one call.
/// Empty when the detector finds no speech at all.
pub fn pieces_of(samples: &[f32]) -> Result<Vec<Range<usize>>, String> {
    let probabilities = SpeechDetector::load()?.probabilities(samples)?;
    Ok(pieces(&stretches(&probabilities, samples.len()), &probabilities, samples.len()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: usize = SAMPLE_RATE as usize;

    /// Probabilities for a timeline of (seconds, probability) runs.
    fn timeline(runs: &[(f32, f32)]) -> (Vec<f32>, usize) {
        let mut p = Vec::new();
        for (secs, value) in runs {
            p.extend(std::iter::repeat_n(*value, (secs * SECOND as f32 / FRAME_SAMPLES as f32).round() as usize));
        }
        let samples = p.len() * FRAME_SAMPLES;
        (p, samples)
    }

    fn secs(r: &Range<usize>) -> (f32, f32) {
        (r.start as f32 / SECOND as f32, r.end as f32 / SECOND as f32)
    }

    /// INVARIANT: the frame math the constants are named for.
    #[test]
    fn the_frame_math_is_what_the_names_say() {
        assert_eq!(FRAME_SAMPLES as f32 / SAMPLE_RATE as f32, 0.032);
        assert_eq!((MIN_PAUSE_FRAMES * FRAME_SAMPLES) as f32 / SAMPLE_RATE as f32, 0.32);
        assert_eq!((MIN_SPEECH_FRAMES * FRAME_SAMPLES) as f32 / SAMPLE_RATE as f32, 0.096);
        assert_eq!(PIECE_SAMPLES as f32 / SAMPLE_RATE as f32, 29.5);
        assert_eq!(ROOM_SAMPLES as f32 / SAMPLE_RATE as f32, 0.3);
        assert_eq!(LONG_PAUSE_SAMPLES as f32 / SAMPLE_RATE as f32, 2.0);
    }

    /// INVARIANT: the compiled-in detector is the file its publisher ships.
    #[test]
    fn the_speech_detector_is_the_published_file() {
        use sha2::{Digest, Sha256};
        assert_eq!(SILERO.len(), 2_327_524);
        let digest: String = Sha256::digest(SILERO).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(digest, SILERO_SHA256);
    }

    /// INVARIANT: speech starts at the threshold, a short dip does not end it, a real pause
    /// does (at the frame the dip began), a blip is not speech, and speech that is still
    /// running at the end of the recording runs to its end.
    #[test]
    fn stretches_follow_sileros_rule() {
        let (p, n) = timeline(&[(1.0, 0.02), (2.0, 0.9), (0.16, 0.05), (2.0, 0.9), (1.0, 0.05), (0.064, 0.9), (1.0, 0.05), (3.0, 0.8)]);
        let found = stretches(&p, n);
        assert_eq!(found.len(), 2, "{found:?}");
        let (a, b) = secs(&found[0]);
        assert!((a - 1.0).abs() < 0.04 && (b - 5.16).abs() < 0.04, "the dip of 0.16 s is inside the first stretch: {a} {b}");
        let (c, d) = secs(&found[1]);
        assert!((c - 7.22).abs() < 0.07 && (d - n as f32 / SECOND as f32).abs() < 0.001, "the last stretch runs to the end: {c} {d}");
        // Between the two thresholds nothing starts and nothing ends.
        let (p, n) = timeline(&[(1.0, 0.2), (1.0, 0.9), (2.0, 0.2), (1.0, 0.9)]);
        assert_eq!(stretches(&p, n).len(), 1);
        assert!(stretches(&timeline(&[(5.0, 0.01)]).0, 5 * SECOND).is_empty());
    }

    /// INVARIANT: a recording shorter than a window is one piece, with the room around its
    /// speech and never more of the recording than there is.
    #[test]
    fn a_short_dictation_is_one_piece_with_its_room() {
        let (p, n) = timeline(&[(3.0, 0.01), (10.0, 0.9), (5.0, 0.01)]);
        let found = pieces(&stretches(&p, n), &p, n);
        assert_eq!(found.len(), 1);
        let (a, b) = secs(&found[0]);
        assert!((a - 2.7).abs() < 0.04 && (b - 13.3).abs() < 0.04, "0.3 s of room on each side: {a} {b}");
        // Less room than that in the recording: all of it, and not a sample more.
        let (p, n) = timeline(&[(0.2, 0.01), (10.0, 0.9), (0.1, 0.01)]);
        assert_eq!(pieces(&stretches(&p, n), &p, n).as_slice(), std::slice::from_ref(&(0..n)));
        assert!(pieces(&[], &[], 0).is_empty());
    }

    /// Every piece fits one window, no piece overlaps the next, and every stretch of speech
    /// is inside the pieces from its first sample to its last.
    fn assert_whole_and_windowed(speech: &[Range<usize>], found: &[Range<usize>]) {
        for piece in found {
            assert!(piece.len() <= PIECE_SAMPLES, "a piece is one window: {:?}", secs(piece));
        }
        for pair in found.windows(2) {
            assert!(pair[0].end <= pair[1].start, "pieces do not overlap: {:?} {:?}", secs(&pair[0]), secs(&pair[1]));
        }
        for stretch in speech {
            let covered: usize = found.iter().map(|piece| piece.end.min(stretch.end).saturating_sub(piece.start.max(stretch.start))).sum();
            assert_eq!(covered, stretch.len(), "no speech is left out: {:?}", secs(stretch));
        }
    }

    /// INVARIANT: a long dictation is cut only in its pauses, every piece fits one window and
    /// is as full as its pauses allow, and no speech is left out.
    #[test]
    fn a_long_dictation_is_cut_in_its_pauses_and_every_piece_fits_a_window() {
        // Twelve 8-second sentences, 0.7 s apart, then 12 s of nothing.
        let mut runs = vec![(0.5, 0.01)];
        for _ in 0..12 { runs.push((8.0, 0.9)); runs.push((0.7, 0.02)); }
        runs.push((12.0, 0.01));
        let (p, n) = timeline(&runs);
        let speech = stretches(&p, n);
        assert_eq!(speech.len(), 12);
        let found = pieces(&speech, &p, n);
        assert_eq!(found.len(), 4, "three sentences to a window: {found:?}");
        assert_whole_and_windowed(&speech, &found);
        for stretch in &speech {
            assert_eq!(found.iter().filter(|piece| piece.start <= stretch.start && stretch.end <= piece.end).count(), 1,
                "every sentence is whole in exactly one piece");
        }
        // Only 0.3 s of the 12 s after the last word goes to the decoder.
        let (_, end) = secs(found.last().unwrap());
        let last_word = speech.last().unwrap().end as f32 / SECOND as f32;
        assert!((end - last_word - 0.3).abs() < 0.001, "{end} {last_word}");
    }

    /// INVARIANT: a long pause ends a piece even when both sides would fit one window, and
    /// only 0.3 s of it reaches the decoder on each side.
    #[test]
    fn a_long_pause_ends_a_piece() {
        let (p, n) = timeline(&[(0.5, 0.01), (6.0, 0.9), (12.0, 0.01), (5.0, 0.9), (1.9, 0.02), (3.0, 0.9), (0.5, 0.01)]);
        let speech = stretches(&p, n);
        assert_eq!(speech.len(), 3);
        let found = pieces(&speech, &p, n);
        assert_eq!(found.len(), 2, "the 12 s pause splits; the 1.9 s pause does not: {found:?}");
        assert_whole_and_windowed(&speech, &found);
        assert!((found[0].end - speech[0].end) as f32 / SECOND as f32 <= 0.3 + 0.001);
        assert!((speech[1].start - found[1].start) as f32 / SECOND as f32 <= 0.3 + 0.001);
    }

    /// INVARIANT: when the next phrase would overflow the window and the pause before it is
    /// in the window's final third, the cut is that pause. When that pause is too early, the
    /// window is filled and the cut is the least speech-like frame of its final third.
    #[test]
    fn a_full_window_is_cut_at_a_late_pause_or_at_the_least_speechlike_frame() {
        // 24 s, a pause, then 20 s: the pause is in the final third (24 of 28.9), so cut there.
        let (p, n) = timeline(&[(24.0, 0.9), (0.5, 0.02), (20.0, 0.9)]);
        let speech = stretches(&p, n);
        let found = pieces(&speech, &p, n);
        assert_eq!(found.len(), 2);
        assert!(found[0].end > speech[0].end && found[0].end < speech[1].start, "cut in the pause: {found:?}");
        assert_whole_and_windowed(&speech, &found);

        // 10 s, a pause, then 40 s with one dip at 25 s: the pause is too early, so the window
        // is filled and cut at the dip.
        let (mut p, n) = timeline(&[(10.0, 0.9), (0.5, 0.02), (40.0, 0.9)]);
        let dip = 25 * SECOND / FRAME_SAMPLES;
        p[dip] = 0.4;
        let speech = stretches(&p, n);
        assert_eq!(speech.len(), 2);
        let found = pieces(&speech, &p, n);
        assert_eq!(found[0], 0..dip * FRAME_SAMPLES, "{found:?}");
        assert_whole_and_windowed(&speech, &found);
    }

    /// INVARIANT: a one-frame dip inside a word never takes the cut from a real pause, even
    /// when the dip is later; with no real pause the longest dip does; and the last of two
    /// real pauses does, so the window is as full as a clean cut allows.
    #[test]
    fn a_real_pause_takes_the_cut_over_a_later_dip() {
        // Talking from 0, a 0.5 s pause at 24 s, a single quiet frame at 27.5 s, talking to 45 s.
        let (mut p, n) = timeline(&[(24.0, 0.9), (0.5, 0.02), (20.5, 0.9)]);
        let dip = (27 * SECOND + SECOND / 2) / FRAME_SAMPLES;
        p[dip] = 0.05;
        let found = pieces(&stretches(&p, n), &p, n);
        let cut = found[0].end as f32 / SECOND as f32;
        assert!(cut > 24.0 && cut < 24.5, "the cut is in the half-second pause, not at the dip: {cut}");

        // No real pause in the final third: the longer of two dips.
        let (mut p, n) = timeline(&[(45.0, 0.9)]);
        let (short, long) = (22 * SECOND / FRAME_SAMPLES, 26 * SECOND / FRAME_SAMPLES);
        p[short] = 0.05;
        p[long..long + 4].fill(0.05);
        p[long + 60] = 0.05;
        let found = pieces(&stretches(&p, n), &p, n);
        assert_eq!(found[0].end, (long + 2) * FRAME_SAMPLES, "the middle of the four-frame dip");

        // Two real pauses in the final third: the later one.
        let (p, n) = timeline(&[(21.0, 0.9), (0.5, 0.02), (4.0, 0.9), (0.5, 0.02), (20.0, 0.9)]);
        let found = pieces(&stretches(&p, n), &p, n);
        let cut = found[0].end as f32 / SECOND as f32;
        assert!(cut > 25.5 && cut < 26.1, "{cut}");
    }

    /// INVARIANT: speech with no pause in it for longer than a window is cut where the
    /// detector is least sure, in the final third of the window, and nothing is dropped.
    #[test]
    fn unbroken_speech_longer_than_a_window_is_cut_at_its_least_speechlike_frame() {
        let (mut p, n) = timeline(&[(70.0, 0.9)]);
        let breath = 25 * SECOND / FRAME_SAMPLES;
        p[breath] = 0.4;                                  // above the thresholds: not a pause, only a dip
        let speech = stretches(&p, n);
        assert_eq!(speech.as_slice(), std::slice::from_ref(&(0..n)));
        let found = pieces(&speech, &p, n);
        assert_eq!(found[0], 0..breath * FRAME_SAMPLES, "the first cut is at the dip");
        assert_eq!(found.last().unwrap().end, n);
        for pair in found.windows(2) {
            assert_eq!(pair[0].end, pair[1].start, "unbroken speech is cut, never dropped");
        }
        assert_whole_and_windowed(&speech, &found);
    }

    /// INVARIANT: the real detector hears nothing in room noise and in silence.
    #[test]
    fn the_detector_finds_no_speech_in_a_quiet_room() {
        use crate::voiced::fixtures::{hiss, silence};
        for room in [hiss(6.0, -60.0, 11), hiss(6.0, -45.0, 12), silence(6.0)] {
            let mut detector = SpeechDetector::load().expect("the compiled-in detector loads");
            let probabilities = detector.probabilities(&room).expect("the detector runs");
            assert_eq!(probabilities.len(), room.len().div_ceil(FRAME_SAMPLES));
            assert!(stretches(&probabilities, room.len()).is_empty(),
                "highest probability {:?}", probabilities.iter().cloned().fold(0.0f32, f32::max));
        }
    }
}

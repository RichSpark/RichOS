//! Voice activity detection over fixed frames — and the FRAME MATH everything else quotes.
//!
//! The pipeline runs at whisper's native rate so no resampling happens between the VAD and
//! the recognizer: **16 000 Hz, mono, f32 in [-1.0, 1.0]**. One VAD frame is 256 samples.
//!
//! ```text
//!   frame duration = VAD_FRAME_SAMPLES / SAMPLE_RATE
//!                  = 256 / 16000
//!                  = 0.016 s  (16.000 ms exactly)
//! ```
//!
//! Every duration in this crate is expressed as a FRAME COUNT and re-derived back to seconds
//! in a unit test, because "about 5 seconds" is not a specification. The earlier voice pilot's tuned
//! barge-in debounce was 313 frames; 313 x 256 / 16000 = 5.008 s — see `bargein.rs`.
//!
//! The detector itself is deliberately plain: short-term energy (RMS) against a floor that
//! adapts to the room. It is NOT a neural VAD and does not pretend to be — it decides
//! "is this frame louder than the room" and nothing more. Everything that needs robustness
//! (barge-in) gets it from frame-count debouncing, not from detector cleverness.

/// The one sample rate the whole voice path runs at (whisper.cpp's native input rate).
pub const SAMPLE_RATE: u32 = 16_000;

/// Samples per VAD frame. 256 / 16000 = 16.000 ms.
pub const VAD_FRAME_SAMPLES: usize = 256;

/// A continuous "speech" run longer than this means the detector is stuck on steady noise,
/// not listening to a person. 625 x 256 / 16000 = 10.000 s. See [`VadConfig::stuck_speech_frames`].
pub const STUCK_SPEECH_FRAMES: u32 = 625;

/// **After the last frame of clear speech, the floor may not RISE for this long.** 50 x 256 /
/// 16000 = 0.800 s, the endpointer's own silence hangover: anything shorter is still inside one
/// stretch of his speech, and a quiet frame there is a soft consonant or the tail of a word, not
/// the room. "Clear" is a frame above the threshold by [`VadConfig::speech_ratio`] again (3 x 3 =
/// 9 x the floor, 19.1 dB): every syllable of a voice is, and room noise that only grazes the
/// threshold is not, so such noise is still learned 0.800 s after his last word (second review
/// rv-20261019T123341Z: renewed by every speech frame, a room at -36.0 dBFS kept the hold alive
/// forever and his turn was never sent).
///
/// The defect it closes (nightly 65, the CEO's recording of 2026-10-18 at 8:27): a frame inside
/// his speech that the VAD did not call speech still raised the floor, at
/// [`VadConfig::floor_attack`] (0.05 per frame: 1 / 0.05 = 20 frames = 0.320 s time constant),
/// toward its own level. A higher floor calls more of his quieter frames non-speech, and they
/// raise it further. His second spoken message, replayed from that recording through the shipped
/// capture path ([`crate::controller::CaptureBrain`], no device): the threshold was 0.00529 RMS (-45.5 dBFS) when
/// the utterance started and 0.1387 (-17.2 dBFS) 8.848 s later; the utterance ended in the middle
/// of his sentence, and the 119 frames at -30 dBFS or louder that he said in the next 4.368 s were
/// none of them called speech, so they never reached the recognizer. Its transcript was, word for
/// word, the message the app sent for him; with this hold the same replay keeps his whole sentence.
///
/// **Only while it is switched on ([`Vad::set_hold_inside_speech`]), and voice mode switches it on
/// only while Rich is silent.** While Rich is audible the same rise is what keeps his own echo
/// from reading as speech: with the hold on then, the committed echo-path fixture at +6.3 dB had
/// Rich cut himself off (`barge_in_on_the_ceos_rig.rs`, `richs_own_echo_never_interrupts_him_at_any_volume`).
pub const FLOOR_RISE_HOLD_FRAMES: u32 = 50;

/// Frame duration in milliseconds, derived — never hard-coded as "16".
pub const fn frame_ms() -> f32 {
    (VAD_FRAME_SAMPLES as f32 * 1000.0) / SAMPLE_RATE as f32
}

/// Convert a duration in seconds to the number of whole VAD frames that COVERS it
/// (ceiling: a debounce must never fire early). The inverse is [`frames_to_secs`].
pub fn frames_for_secs(secs: f32) -> u32 {
    let exact = (secs * SAMPLE_RATE as f32) / VAD_FRAME_SAMPLES as f32;
    exact.ceil() as u32
}

/// The EXACT duration a frame count represents. This is the number that goes in commit
/// messages and briefs — the requested duration is an intention, this is the truth.
pub fn frames_to_secs(frames: u32) -> f32 {
    (frames as f32 * VAD_FRAME_SAMPLES as f32) / SAMPLE_RATE as f32
}

/// Root-mean-square amplitude of one frame, in the same [-1.0, 1.0] units as the samples.
pub fn rms(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let sum: f32 = frame.iter().map(|s| s * s).sum();
    (sum / frame.len() as f32).sqrt()
}

/// **The input meter's 0..1 level for one frame's RMS**: -60 dBFS is 0.0, 0 dBFS is 1.0, so a
/// normal speaking voice lands in the upper half of the meter instead of a barely-visible
/// sliver. One function, read by voice mode's panel ([`Vad::level`]) and by the dictation bar's
/// meter (`dictation_bar::MeterGate`), so the two meters can never disagree about a voice.
pub fn display_level(rms: f32) -> f32 {
    let db = 20.0 * rms.max(1e-6).log10();
    ((db + 60.0) / 60.0).clamp(0.0, 1.0)
}

/// Tuning for [`Vad`]. Defaults are the open-mic-in-a-quiet-office numbers; every one of
/// them is a frame count or an amplitude, never a vague adjective.
#[derive(Debug, Clone, Copy)]
pub struct VadConfig {
    /// Absolute RMS below which a frame can never be speech, whatever the noise floor says.
    /// Guards against the adaptive floor collapsing to zero in a silent room and then
    /// calling faint fan noise "speech".
    pub absolute_floor: f32,
    /// A frame is speech when its RMS exceeds `noise_floor * speech_ratio`.
    pub speech_ratio: f32,
    /// How fast the noise floor tracks upward (per adapting frame), 0..1.
    pub floor_attack: f32,
    /// How fast the noise floor tracks downward (per adapting frame), 0..1.
    pub floor_release: f32,
    /// Escape hatch for the speech-gated floor. The floor normally adapts ONLY on
    /// non-speech frames, so a long sentence can never mute the speaker mid-word — but that
    /// alone deadlocks in a room whose steady noise is already above the initial threshold:
    /// every frame reads as speech, so the floor never learns the room. A continuous
    /// "speech" run longer than this is not a person, it is a noise source, so adaptation
    /// resumes. 625 x 256 / 16000 = 10.000 s — far longer than any unbroken human phrase
    /// (real speech has inter-word gaps two orders of magnitude shorter).
    pub stuck_speech_frames: u32,
}

impl Default for VadConfig {
    fn default() -> Self {
        VadConfig {
            // -46 dBFS. Below this a 16 ms frame is room tone, not a CEO.
            absolute_floor: 0.005,
            speech_ratio: 3.0,
            floor_attack: 0.05,
            floor_release: 0.01,
            stuck_speech_frames: STUCK_SPEECH_FRAMES,
        }
    }
}

/// Frame-by-frame speech/no-speech decision with an adaptive noise floor.
#[derive(Debug, Clone)]
pub struct Vad {
    cfg: VadConfig,
    noise_floor: f32,
    last_rms: f32,
    speech_run: u32,
    /// Frames since the last frame clearly above the threshold (by [`VadConfig::speech_ratio`]
    /// again), saturating; compared with [`FLOOR_RISE_HOLD_FRAMES`].
    since_speech: u32,
    /// [`FLOOR_RISE_HOLD_FRAMES`] applies: off until [`Vad::set_hold_inside_speech`] turns it on.
    hold_inside_speech: bool,
}

impl Default for Vad {
    fn default() -> Self {
        Vad::new(VadConfig::default())
    }
}

impl Vad {
    pub fn new(cfg: VadConfig) -> Self {
        Vad {
            cfg,
            noise_floor: cfg.absolute_floor,
            last_rms: 0.0,
            speech_run: 0,
            since_speech: u32::MAX,
            hold_inside_speech: false,
        }
    }

    /// Whether a quiet frame within [`FLOOR_RISE_HOLD_FRAMES`] of speech is kept from raising the
    /// floor. Voice mode turns it on for every frame on which Rich is not audible.
    pub fn set_hold_inside_speech(&mut self, on: bool) {
        self.hold_inside_speech = on;
    }

    /// The current adaptive noise floor (RMS units) — exposed for diagnostics/tests.
    pub fn noise_floor(&self) -> f32 {
        self.noise_floor
    }

    /// The RMS a frame must EXCEED right now to be called speech — the adaptive floor times
    /// [`VadConfig::speech_ratio`], never below [`VadConfig::absolute_floor`].
    ///
    /// Exists because "which gate ate the onset" was unanswerable from the outside: the
    /// threshold is the one number that decides it and it was computed inline in
    /// [`Vad::push_frame`] and thrown away. Derived from the same expression that decides the
    /// verdict, so the two cannot drift — see `the_threshold_accessor_is_the_expression_that_decides`.
    pub fn speech_threshold(&self) -> f32 {
        (self.noise_floor * self.cfg.speech_ratio).max(self.cfg.absolute_floor)
    }

    /// RMS of the most recently pushed frame — the UI's live input-level meter reads this.
    pub fn last_rms(&self) -> f32 {
        self.last_rms
    }

    /// A 0..1 display level for the UI meter. Log-ish so a normal speaking voice lands in
    /// the upper half of the meter instead of a barely-visible sliver.
    pub fn level(&self) -> f32 {
        display_level(self.last_rms)
    }

    /// Classify one frame. Frames shorter/longer than [`VAD_FRAME_SAMPLES`] are accepted
    /// (RMS is length-normalized) but the caller should be feeding exact frames so the
    /// debounce frame math means what it says.
    pub fn push_frame(&mut self, frame: &[f32]) -> bool {
        let r = rms(frame);
        self.last_rms = r;
        let threshold = self.speech_threshold();
        let is_speech = r > threshold;
        if is_speech {
            self.speech_run += 1;
        } else {
            self.speech_run = 0;
        }
        // Only a frame CLEARLY above the threshold, by the same ratio again, renews the hold: a
        // voice is, and room noise that only grazes the threshold is not (`FLOOR_RISE_HOLD_FRAMES`).
        if r > threshold * self.cfg.speech_ratio {
            self.since_speech = 0;
        } else {
            self.since_speech = self.since_speech.saturating_add(1);
        }
        // Track the room on non-speech frames, so a long sentence can never drag the floor
        // up to the level of the voice and mute the speaker mid-word. The `stuck` escape
        // (see VadConfig) is the only thing that lets a speech-classified frame adapt it.
        let stuck = self.speech_run > self.cfg.stuck_speech_frames;
        // And, with the hold on, a non-speech frame within 0.800 s of speech is still his speech:
        // it may lower the floor but never raise it (`FLOOR_RISE_HOLD_FRAMES`).
        let rising = r > self.noise_floor;
        let held = rising && self.hold_inside_speech && self.since_speech <= FLOOR_RISE_HOLD_FRAMES;
        if stuck || (!is_speech && !held) {
            let rate = if rising { self.cfg.floor_attack } else { self.cfg.floor_release };
            self.noise_floor += (r - self.noise_floor) * rate;
            self.noise_floor = self.noise_floor.max(1e-5);
        }
        is_speech
    }

    /// Consecutive frames currently classified as speech — diagnostics, and the input to the
    /// stuck-floor escape.
    pub fn speech_run(&self) -> u32 {
        self.speech_run
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// INVARIANT: one VAD frame is exactly 16.000 ms at 16 kHz — 256 / 16000 = 0.016 s.
    #[test]
    fn one_vad_frame_is_exactly_sixteen_milliseconds() {
        assert_eq!(VAD_FRAME_SAMPLES, 256);
        assert_eq!(SAMPLE_RATE, 16_000);
        assert!((frame_ms() - 16.0).abs() < f32::EPSILON, "frame_ms() = {}", frame_ms());
    }

    /// INVARIANT: a requested duration is rounded UP to whole frames, so a debounce always
    /// covers at least the duration asked for and never fires early.
    #[test]
    fn frames_for_secs_rounds_up_so_a_debounce_never_fires_early() {
        // 5.0 s -> 5.0 * 16000 / 256 = 312.5 -> 313 frames.
        assert_eq!(frames_for_secs(5.0), 313);
        assert!(frames_to_secs(313) >= 5.0);
        // 0.8 s -> 0.8 * 16000 / 256 = 50.0 exactly -> 50 frames, no inflation.
        assert_eq!(frames_for_secs(0.8), 50);
        assert!((frames_to_secs(50) - 0.8).abs() < 1e-6);
    }

    /// INVARIANT: frames_to_secs is the exact inverse of the frame math, not an estimate.
    /// 313 x 256 / 16000 = 5.008 — the number quoted in briefs.
    #[test]
    fn frames_to_secs_reproduces_the_pilots_five_point_zero_zero_eight_seconds() {
        assert!((frames_to_secs(313) - 5.008).abs() < 1e-6, "{}", frames_to_secs(313));
    }

    #[test]
    fn rms_of_silence_is_zero_and_of_full_scale_dc_is_one() {
        assert_eq!(rms(&[0.0; VAD_FRAME_SAMPLES]), 0.0);
        assert!((rms(&[1.0; VAD_FRAME_SAMPLES]) - 1.0).abs() < 1e-6);
        assert_eq!(rms(&[]), 0.0);
    }

    fn tone(amp: f32) -> Vec<f32> {
        (0..VAD_FRAME_SAMPLES)
            .map(|i| amp * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / SAMPLE_RATE as f32).sin())
            .collect()
    }

    /// INVARIANT: a silent room never reads as speech, however long it stays silent.
    #[test]
    fn silence_is_never_speech_even_after_the_floor_settles() {
        let mut vad = Vad::default();
        for _ in 0..500 {
            assert!(!vad.push_frame(&[0.0; VAD_FRAME_SAMPLES]));
        }
    }

    /// INVARIANT: a normal speaking level over quiet room tone reads as speech.
    #[test]
    fn a_voice_over_room_tone_reads_as_speech() {
        let mut vad = Vad::default();
        for _ in 0..200 {
            vad.push_frame(&tone(0.002)); // room tone, well under the absolute floor
        }
        assert!(vad.push_frame(&tone(0.2)), "0.2 amplitude tone should be speech");
    }

    /// INVARIANT: the noise floor only adapts on NON-speech frames — a long utterance can
    /// never raise the floor up to its own level and mute the speaker mid-sentence.
    #[test]
    fn the_noise_floor_does_not_climb_during_a_long_utterance() {
        let mut vad = Vad::default();
        let before = vad.noise_floor();
        // 600 frames = 9.600 s, just under the 625-frame (10.000 s) stuck escape.
        for _ in 0..600 {
            assert!(vad.push_frame(&tone(0.25)), "should stay speech for the whole utterance");
        }
        assert!(
            (vad.noise_floor() - before).abs() < 1e-6,
            "floor moved during speech: {} -> {}",
            before,
            vad.noise_floor()
        );
    }

    /// INVARIANT: a quiet frame INSIDE speech never raises the floor. Within 0.800 s of his last
    /// speech frame, a frame the VAD did not call speech is a soft consonant or a word's tail, and
    /// letting it raise the floor is the ratchet that stopped hearing him mid-sentence (nightly
    /// 65, [`FLOOR_RISE_HOLD_FRAMES`]). After 0.800 s without speech the room is learned again.
    #[test]
    fn a_quiet_frame_inside_speech_never_raises_the_floor() {
        assert_eq!(FLOOR_RISE_HOLD_FRAMES, crate::endpoint::SILENCE_HANGOVER_FRAMES);
        assert!((frames_to_secs(FLOOR_RISE_HOLD_FRAMES) - 0.800).abs() < 1e-6);
        let mut vad = Vad::default();
        vad.set_hold_inside_speech(true);
        let soft = tone(0.014); // RMS 0.0099: above the 0.005 floor, under its 0.015 threshold
        assert!(vad.push_frame(&tone(0.2)), "premise: he is speaking");
        let floor = vad.noise_floor();
        for i in 1..=FLOOR_RISE_HOLD_FRAMES {
            assert!(!vad.push_frame(&soft), "premise: the soft frame is not speech");
            assert_eq!(vad.noise_floor(), floor, "frame {i} after speech raised the floor");
        }
        assert!(!vad.push_frame(&soft));
        assert!(vad.noise_floor() > floor, "0.800 s after speech the room must be learned again");

        // Switched off (Rich is audible), the same frame raises the floor at once, as it always did.
        let mut off = Vad::default();
        assert!(off.push_frame(&tone(0.2)));
        let floor = off.noise_floor();
        assert!(!off.push_frame(&soft));
        assert!(off.noise_floor() > floor, "with the hold off nothing may change");
    }

    /// INVARIANT: the hold is one way. A silent frame inside speech still LOWERS the floor.
    #[test]
    fn a_silent_frame_inside_speech_still_lowers_the_floor() {
        let mut vad = Vad::default();
        vad.set_hold_inside_speech(true);
        assert!(vad.push_frame(&tone(0.2)), "premise: he is speaking");
        let floor = vad.noise_floor();
        assert!(!vad.push_frame(&[0.0; VAD_FRAME_SAMPLES]));
        assert!(vad.noise_floor() < floor, "silence inside speech must still lower the floor");
    }

    /// INVARIANT: the stuck-floor escape is exactly 10.000 s of continuous "speech" — long
    /// enough that no unbroken human phrase reaches it.
    #[test]
    fn the_stuck_floor_escape_is_ten_seconds_of_continuous_speech() {
        assert_eq!(STUCK_SPEECH_FRAMES, frames_for_secs(10.0));
        assert!((frames_to_secs(STUCK_SPEECH_FRAMES) - 10.0).abs() < 1e-6);
        // And it is far longer than the longest debounce anything else in the crate uses.
        assert!(STUCK_SPEECH_FRAMES > crate::bargein::BARGE_IN_DEBOUNCE_FRAMES);
    }

    /// INVARIANT: in a room whose steady noise is ALREADY above the initial threshold, the
    /// detector does not deadlock. Without the escape it would: every frame reads as speech,
    /// so the floor never adapts, so every frame reads as speech forever. It must recover —
    /// and it must take no longer than the escape plus its attack time.
    #[test]
    fn a_steady_hiss_above_the_initial_threshold_stops_reading_as_speech() {
        let mut vad = Vad::new(VadConfig { absolute_floor: 0.0005, ..VadConfig::default() });
        let noisy = tone(0.01); // RMS ~0.00707, well above the 0.0015 initial threshold
        assert!(vad.push_frame(&noisy), "premise: the hiss starts out looking like speech");

        let mut recovered_at = None;
        for i in 1..4000 {
            if !vad.push_frame(&noisy) {
                recovered_at = Some(i);
                break;
            }
        }
        let at = recovered_at.expect("the floor never adapted — the deadlock is back");
        assert!(at > STUCK_SPEECH_FRAMES, "adapted during what could have been real speech: {at}");
        assert!(at < STUCK_SPEECH_FRAMES + 200, "took too long to learn the room: {at} frames");

        // And it STAYS non-speech: the room is learned, not momentarily dipped under.
        for _ in 0..1000 {
            assert!(!vad.push_frame(&noisy), "hiss started reading as speech again");
        }
        // A real voice still cuts through the learned floor.
        assert!(vad.push_frame(&tone(0.3)));
    }

    /// INVARIANT: [`Vad::speech_threshold`] is the SAME expression `push_frame` decides with —
    /// a diagnostic that can disagree with the decision is worse than no diagnostic, because
    /// it is what the next engineer will reason from. Checked across an adapting floor, not at
    /// one settled value.
    #[test]
    fn the_threshold_accessor_is_the_expression_that_decides() {
        let mut vad = Vad::default();
        let mut checked = 0;
        for i in 0..600 {
            // A level that crosses the moving threshold in both directions as the floor learns.
            let amp = if i % 7 == 0 { 0.02 } else { 0.004 + (i % 13) as f32 * 0.001 };
            let f = tone(amp);
            let t = vad.speech_threshold();
            let expected = rms(&f) > t;
            assert_eq!(vad.push_frame(&f), expected, "frame {i}: rms {} vs threshold {t}", rms(&f));
            checked += 1;
        }
        assert_eq!(checked, 600);
    }

    /// INVARIANT: the UI meter is monotonic in loudness and bounded to 0..1.
    #[test]
    fn level_meter_is_monotonic_and_bounded() {
        let mut quiet = Vad::default();
        quiet.push_frame(&tone(0.01));
        let mut loud = Vad::default();
        loud.push_frame(&tone(0.5));
        assert!(loud.level() > quiet.level());
        assert!((0.0..=1.0).contains(&loud.level()));
        let mut silent = Vad::default();
        silent.push_frame(&[0.0; VAD_FRAME_SAMPLES]);
        assert_eq!(silent.level(), 0.0);
    }
}

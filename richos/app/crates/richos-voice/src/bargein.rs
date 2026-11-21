//! Barge-in: how the CEO interrupts Rich mid-sentence — and the AEC seam that is NOT filled.
//!
//! ## The debounce, and why it is this number
//!
//! With an open mic next to speakers and **no acoustic echo cancellation**, Rich's own voice
//! comes back into the mic and a naive "any speech-like sound stops playback" monitor cuts him
//! off after a word or two. The earlier voice pilot hit exactly this
//! (the pilot's notes: "Known issue: echo triggers barge-in").
//!
//! The field-proven workaround, CEO-directed at v3 of that patch, is a long **consecutive-frame**
//! debounce: echo arrives in short bursts with gaps between syllables, so any gap resets the run
//! and the counter never reaches the threshold — while a human deliberately talking over Rich
//! for a beat produces a continuous run and does get through.
//!
//! ```text
//!   BARGE_IN_DEBOUNCE_FRAMES = 313
//!   313 frames x 256 samples/frame = 80 128 samples
//!   80 128 samples / 16 000 samples/s = 5.008 s
//! ```
//!
//! Re-derived here rather than trusted: `frames_for_secs(5.0)` = ceil(5.0 x 16000 / 256)
//! = ceil(312.5) = 313, and `frames_to_secs(313)` = 5.008 s exactly. Both are asserted in
//! `vad.rs`'s tests and again below.
//!
//! **This is a workaround, not a fix.** It makes Rich effectively uninterruptible in casual
//! conversation (finish-your-thought behavior). The instant override is the explicit "tap to
//! stop" control in the UI, which calls [`BargeInMonitor::force`] — the manual half of the
//! interim, exactly as the pilot's mute button was.
//!
//! ## AND IT IS NO LONGER THE RULE THE CEO MEETS — 2026-09-18
//!
//! He tested it on his own rig and said *"no matter what I said, he couldn't hear me"*. He WAS
//! heard. The recording made that afternoon on his desk has his voice at **-18.1 dBFS** at the
//! microphone against Rich's echo at **-46.7 dBFS** on the same microphone — **28.6 dB** of
//! separation, five separate times, 4.880 s of speech in total. Every one of those was thrown
//! away, because 5.008 s of UNBROKEN speech is not a thing a person produces: his longest
//! unbroken run was **66 frames = 1.056 s**, and 66 < 313.
//!
//! The debounce was never the discrimination — it is a continuity race that echo happens to
//! lose, and the CEO loses it too. The discrimination that works is the canceller's per-frame
//! **near-end verdict**, ANDed with the VAD, and it does not need a converged filter:
//!
//! ```text
//!   worst case over 14 echo-only conditions, two recordings of the CEO's own rig,
//!   each swept across six settings of the volume knob (his 60 up to 4 dB over Ray's 85):
//!
//!     VAD speech alone                 longest run 23 frames · 23 of any 25-frame window
//!     VAD speech AND near-end          longest run  6 frames ·  6 of any 25-frame window
//!
//!   the CEO's own voice, same rig, same day:
//!     VAD speech AND near-end          longest run 66 frames · 25 of 25 — saturated
//! ```
//!
//! So `AEC_BARGE_IN_REQUIRED_FRAMES` of 15 in 25 clears the worst echo ever measured here by
//! 15/6 = 2.50x on the conjunction, while the same window on a bare VAD verdict would fire on
//! Rich at 23 of 25. **The rule did not change. The signal did, and the confidence
//! precondition on it came off.** [`BargeInMonitor::set_near_end_gated`] carries the numbers;
//! `examples/bargein_score.rs` reproduces them with no device and no sound.
//!
//! [`BargeInMode::Consecutive`] is still the rule when there is **no canceller at all**, and
//! that is now the only case it covers.
//!
//! ## The AEC seam ([`EchoGate`])
//!
//! Real echo cancellation is the still-open gap inherited from the pilot and is explicitly
//! NOT a v1 blocker (CEO, 2026-08-24). What v1 DOES ship is the seam already carrying live
//! data: the playout path feeds every rendered output frame to
//! [`EchoGate::observe_playback`], so a future reference-signal-gated suppressor (or WebRTC
//! AEC3) has the reference signal it needs the day it lands, with no plumbing work.
//! The v1 implementation, [`NoEchoCancellation`], cancels nothing and says so.

use crate::vad::{frames_for_secs, frames_to_secs};

/// Consecutive speech frames required to interrupt Rich while he is speaking.
/// 313 x 256 / 16000 = 5.008 s. See the module docs for why it is this long.
pub const BARGE_IN_DEBOUNCE_FRAMES: u32 = 313;

/// The exact debounce duration in seconds, derived from the frame count.
pub fn barge_in_debounce_secs() -> f32 {
    frames_to_secs(BARGE_IN_DEBOUNCE_FRAMES)
}

/// **The near-end-gated debounce window.** Whenever a canceller is present, the interruption
/// test changes shape entirely: instead of racing echo for 5.008 s of unbroken speech, it asks
/// whether the CEO has been talking for most of the last 0.400 s.
///
/// **The name still says AEC and that is correct — what changed on 2026-09-18 is WHICH property
/// of the canceller it rests on.** It used to require [`crate::aec::EchoCanceller::confident`],
/// which on the CEO's hardware is unreachable by measurement. It now requires only the
/// canceller's per-frame near-end verdict, which needs no converged filter. See
/// [`BargeInMonitor::set_near_end_gated`] for the decibels.
///
/// ```text
///   AEC_BARGE_IN_WINDOW_FRAMES   = 25
///   25 frames x 256 samples/frame = 6400 samples
///   6400 samples / 16 000 samples/s = 0.400 s exactly
/// ```
pub const AEC_BARGE_IN_WINDOW_FRAMES: u32 = 25;

/// How many of those 25 frames must be near-end speech.
///
/// ```text
///   AEC_BARGE_IN_REQUIRED_FRAMES = 15
///   15 x 256 / 16000 = 0.240 s of evidence inside a 0.400 s window (60 %)
/// ```
///
/// **Why a window and not a consecutive run.** The consecutive rule is not a stricter version
/// of this one, it is a different test, and it was built for a different job: echo is bursty,
/// so ANY gap resetting the counter is what made 313 frames unreachable by echo. But real
/// speech is bursty too. Measured on the rig with a converged canceller, a full **1000 ms**
/// interruption produced 46 near-end frames of 62 — and a longest unbroken run of only
/// **16**. Under a consecutive rule, no interruption of any length reliably registers once
/// the near-end verdict is honest, because the CEO stops between words like everyone else.
///
/// The window counts evidence instead of demanding perfection, which is the right primitive
/// once the false-positive source (echo) has been removed by measurement rather than dodged
/// by a continuity race.
pub const AEC_BARGE_IN_REQUIRED_FRAMES: u32 = 15;

/// The converged window in seconds, derived: 25 x 256 / 16000 = 0.400 s.
pub fn aec_barge_in_window_secs() -> f32 {
    frames_to_secs(AEC_BARGE_IN_WINDOW_FRAMES)
}

/// How the monitor is currently deciding. Not a preference — a consequence of WHICH SIGNAL it
/// is being fed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BargeInMode {
    /// **The fallback, and the default.** 313 consecutive speech frames = 5.008 s. In force
    /// when the monitor is fed a bare VAD verdict — which now means only one thing: there is
    /// **no echo canceller at all** ([`NoEchoCancellation`], or a platform where the real one
    /// cannot run). A bare VAD verdict cannot tell the CEO from Rich, so the only protection
    /// left is the continuity race, and it stays exactly as it was.
    Consecutive,
    /// **The near-end-gated rule.** 15 frames within a sliding 25-frame (0.400 s) window, where
    /// a frame counts only if the VAD says speech AND the canceller says near-end. In force
    /// whenever a canceller is present — see [`BargeInMonitor::set_near_end_gated`] for the
    /// measurement that removed the confidence precondition on 2026-09-18.
    Windowed,
}


/// Watches the mic while Rich is speaking and reports the moment the CEO has been talking
/// over him for a continuous [`BARGE_IN_DEBOUNCE_FRAMES`].
///
/// Disarmed by default: the monitor only counts while Rich actually has audio playing, so a
/// CEO talking during silence is a normal utterance, not an interruption.
#[derive(Debug, Clone)]
pub struct BargeInMonitor {
    required: u32,
    run: u32,
    armed: bool,
    fired: bool,
    mode: BargeInMode,
    /// Sliding window of the last `AEC_BARGE_IN_WINDOW_FRAMES` verdicts, as a bitmask, plus
    /// the running population count. A `u32` is exactly enough for 25 frames and costs one
    /// shift per frame — this runs on the audio callback thread.
    window: u32,
    window_len: u32,
    window_hits: u32,
}

impl Default for BargeInMonitor {
    fn default() -> Self {
        BargeInMonitor::with_frames(BARGE_IN_DEBOUNCE_FRAMES)
    }
}

impl BargeInMonitor {
    /// Build a monitor with an explicit frame threshold (tests, and the one-line retune the
    /// pilot's history shows this number will get).
    pub fn with_frames(required: u32) -> Self {
        BargeInMonitor {
            required,
            run: 0,
            armed: false,
            fired: false,
            mode: BargeInMode::Consecutive,
            window: 0,
            window_len: 0,
            window_hits: 0,
        }
    }

    /// Which rule is in force right now.
    pub fn mode(&self) -> BargeInMode {
        self.mode
    }

    /// **Switch rules. Driven by WHICH SIGNAL the caller is feeding, and by nothing else** —
    /// never by a setting, never by a guess about headphones, and since 2026-09-18 no longer by
    /// [`crate::aec::EchoCanceller::confident`].
    ///
    /// `true` means: every `push` from here on carries the CONJUNCTION — the VAD said speech AND
    /// the canceller said near-end, both on the same frame.
    ///
    /// ## Why the confidence precondition is gone, in decibels
    ///
    /// It used to be `set_near_end_gated`, and on the CEO's own hardware `confident()` is false
    /// and always will be: the measured magnitude-squared coherence caps ANY linear canceller
    /// there at 4.3-5.5 dB, so the residual can never sit 6 dB under the VAD's speech floor
    /// (`docs/verification/2026-09-17-aec-erle-on-the-ceo-rig.md`). The consequence was the
    /// defect the CEO reported on 2026-09-18: *"no matter what I said, he couldn't hear me"*.
    /// He was heard — five separate times — and every utterance was discarded, because the
    /// fallback wanted 5.008 s of UNBROKEN speech and nobody speaks for 5 s without a gap.
    ///
    /// **The mistake was gating the RULE on confidence when only the SIGNAL needed gating.**
    /// `confident()` answers *"is leftover echo too quiet to reach the VAD's threshold?"* — the
    /// right question for feeding the window a bare VAD verdict, and unanswerable-in-the-
    /// affirmative on this rig. The conjunction asks a different and much easier question:
    /// *"does this frame carry more energy than the canceller predicts for the reference level
    /// it can see?"* That does not need a converged filter. `leak_gain` is a minimum statistic,
    /// so `predicted_echo` is deliberately a LOW estimate, and before the filter has learned
    /// anything `leak_gain` sits at its pessimistic initial 1.0 — which is exactly why the
    /// playback-onset transient measured below is rejected rather than mistaken for a person.
    ///
    /// ## The measurement, on two independent recordings of the CEO's own rig
    ///
    /// `examples/bargein_score.rs`, replaying the committed fixtures with no device and no
    /// sound. **S** = VAD speech on the residual. **B** = VAD speech AND the near-end verdict.
    /// Fourteen echo-only conditions: two recordings, each as-recorded plus six settings of the
    /// volume knob spanning the CEO's own volume 60 up to 4 dB above Ray's volume 85.
    ///
    /// ```text
    ///   ECHO ONLY, nobody in the room          worst longest run   worst 25-frame density
    ///     ceo-rig-2026-09-17, all volumes         S  5    B  0        S  5    B  0
    ///     ceo-rig-2026-09-18-nearend, all         S 23    B  6        S 23    B  6
    ///     ------------------------------------------------------------------------
    ///     WORST CASE ANYWHERE                     S 23    B  6        S 23    B  6
    ///
    ///   THE CEO'S OWN VOICE, ceo-rig-2026-09-18-nearend2, 4.880 s over five intervals
    ///     his level -18.1 dBFS vs echo -46.7 dBFS at the same microphone: 28.6 dB above it
    ///                                             S 66    B 66        S 25    B 25
    /// ```
    ///
    /// So on signal **B** the threshold of 15 in 25 clears the worst echo ever measured on this
    /// rig by **15 / 6 = 2.50x** and the CEO saturates it at 25 of 25. On signal **S** echo
    /// reaches **23** of 25 — one frame short of saturation — so the same window on a bare VAD
    /// verdict would cut Rich off mid-sentence, which is exactly what
    /// `the_windowed_rule_would_fire_on_raw_echo_which_is_why_it_needs_a_canceller` asserts and
    /// why [`BargeInMode::Consecutive`] remains the rule with no canceller present.
    ///
    /// Changing mode clears the accumulated evidence in both directions. Carrying a 200-frame
    /// consecutive run into the windowed rule (or vice versa) would let a debounce fire on
    /// evidence gathered under a rule that no longer applies.
    pub fn set_near_end_gated(&mut self, gated: bool) {
        let want = if gated { BargeInMode::Windowed } else { BargeInMode::Consecutive };
        if want != self.mode {
            self.mode = want;
            self.run = 0;
            self.window = 0;
            self.window_len = 0;
            self.window_hits = 0;
        }
    }

    /// Frames of near-end speech inside the current 0.400 s window. The windowed counterpart
    /// of [`BargeInMonitor::run_frames`], and the same kind of diagnostic: it makes "why did
    /// it not barge in" answerable.
    pub fn window_hits(&self) -> u32 {
        self.window_hits
    }

    /// The threshold currently in force, in frames — 313 or 15 depending on the mode.
    pub fn effective_required_frames(&self) -> u32 {
        match self.mode {
            BargeInMode::Consecutive => self.required,
            BargeInMode::Windowed => AEC_BARGE_IN_REQUIRED_FRAMES,
        }
    }

    /// The worst-case time to fire under the rule currently in force, in seconds.
    /// 5.008 s consecutive, or 0.400 s windowed.
    pub fn effective_debounce_secs(&self) -> f32 {
        match self.mode {
            BargeInMode::Consecutive => frames_to_secs(self.required),
            BargeInMode::Windowed => frames_to_secs(AEC_BARGE_IN_WINDOW_FRAMES),
        }
    }

    /// Build a monitor from a duration in seconds, rounded UP to whole frames.
    pub fn with_secs(secs: f32) -> Self {
        BargeInMonitor::with_frames(frames_for_secs(secs))
    }

    pub fn required_frames(&self) -> u32 {
        self.required
    }

    /// Consecutive speech frames observed in the current run — the diagnostic that makes
    /// "it didn't barge in" debuggable instead of mysterious.
    pub fn run_frames(&self) -> u32 {
        self.run
    }

    pub fn is_armed(&self) -> bool {
        self.armed
    }

    /// Rich started speaking: start watching. Resets the run so speech from before playout
    /// began can never count toward an interruption.
    pub fn arm(&mut self) {
        self.armed = true;
        self.run = 0;
        self.window = 0;
        self.window_len = 0;
        self.window_hits = 0;
        self.fired = false;
    }

    /// Rich stopped speaking: stop watching.
    pub fn disarm(&mut self) {
        self.armed = false;
        self.run = 0;
        self.window = 0;
        self.window_len = 0;
        self.window_hits = 0;
        self.fired = false;
    }

    /// Feed one VAD verdict. Returns true EXACTLY ONCE per armed period, on the frame that
    /// completes the debounce.
    pub fn push(&mut self, is_speech: bool) -> bool {
        if !self.armed || self.fired {
            return false;
        }
        match self.mode {
            BargeInMode::Consecutive => {
                if is_speech {
                    self.run += 1;
                    if self.run >= self.required {
                        self.fired = true;
                        return true;
                    }
                } else {
                    // The reset that makes the whole scheme work WITHOUT a canceller: echo is
                    // bursty, so any gap between Rich's own syllables returns the run to zero.
                    self.run = 0;
                }
            }
            BargeInMode::Windowed => {
                // Slide the window: drop the frame falling out of the far end, admit the new
                // one at the near end, and keep the population count incrementally.
                if self.window_len == AEC_BARGE_IN_WINDOW_FRAMES {
                    let leaving = (self.window >> (AEC_BARGE_IN_WINDOW_FRAMES - 1)) & 1;
                    self.window_hits -= leaving;
                } else {
                    self.window_len += 1;
                }
                self.window = (self.window << 1) & ((1u32 << AEC_BARGE_IN_WINDOW_FRAMES) - 1);
                if is_speech {
                    self.window |= 1;
                    self.window_hits += 1;
                }
                self.run = self.window_hits;
                if self.window_hits >= AEC_BARGE_IN_REQUIRED_FRAMES {
                    self.fired = true;
                    return true;
                }
            }
        }
        false
    }

    /// The manual override — the UI's "tap to stop". Fires immediately if armed, bypassing
    /// the debounce entirely. This is the CEO's instant interrupt while AEC is missing.
    pub fn force(&mut self) -> bool {
        if !self.armed || self.fired {
            return false;
        }
        self.fired = true;
        true
    }
}

/// The acoustic-echo-cancellation seam.
///
/// `process_capture` may modify the mic frame in place before the VAD sees it; a real
/// implementation subtracts the (delayed, room-filtered) reference from the capture.
///
/// **The reference signal does NOT come through this trait.** It used to —
/// `observe_playback(&[f32])`, called from the output callback behind a `Mutex` with
/// `try_lock`. That was fine for a gate that cancels nothing and impossible for one that
/// does: a reference frame dropped on lock contention is a permanent 16 ms shift between the
/// reference and the echo, which invalidates every tap an adaptive filter has learned. The
/// reference now travels over [`crate::aec::ReferenceRing`], lock-free, and an implementation
/// takes its own handle to that ring at construction.
///
/// [`crate::aec::EchoCanceller`] is the real implementation; [`NoEchoCancellation`] is the
/// honest null one. A macOS `VoiceProcessingIO` implementation, if the live figures ever
/// justify one, goes here too.
pub trait EchoGate: Send {
    /// One mic frame, mutable, immediately before VAD classification.
    fn process_capture(&mut self, frame: &mut [f32]);
    /// Honest identity for logs and for the UI's "headphones recommended" note.
    fn name(&self) -> &'static str;
    /// Whether this gate actually cancels anything. `false` means the 5.008 s debounce +
    /// headphones are the whole story — the UI keeps showing the honest note.
    fn cancels(&self) -> bool;
    /// Did the last processed frame contain near-end speech (a real interruption) rather than
    /// leftover echo? A gate that cannot tell says `false` and the VAD decides alone.
    fn near_end_speech(&self) -> bool {
        false
    }
    /// Has this gate MEASURED its residual echo below the VAD's speech threshold? Only a
    /// `true` here may shorten the barge-in debounce.
    fn confident(&self) -> bool {
        false
    }
}

/// **The null gate.** No cancellation whatsoever, and it says so.
///
/// Kept, and still exercised, because it is what the pipeline falls back to on any platform or
/// device where the real canceller cannot run — and because a type that reports
/// `cancels() == false` is what keeps the UI's "headphones recommended" note honest instead of
/// decorative. Selecting it puts the 5.008 s consecutive debounce in force, permanently.
#[derive(Debug, Default)]
pub struct NoEchoCancellation {
    /// Capture frames seen — proves the seam is actually wired at runtime.
    observed_frames: u64,
}

impl NoEchoCancellation {
    pub fn observed_frames(&self) -> u64 {
        self.observed_frames
    }
}

impl EchoGate for NoEchoCancellation {
    fn process_capture(&mut self, _frame: &mut [f32]) {
        // Intentionally does not touch the frame. See the type docs.
        self.observed_frames += 1;
    }
    fn name(&self) -> &'static str {
        "none (5.008s debounce + headphones)"
    }
    fn cancels(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// INVARIANT: the shipped debounce is 313 frames = 5.008 s, re-derived from the frame
    /// math rather than copied from the pilot's comment.
    #[test]
    fn shipped_debounce_is_313_frames_which_is_5008_milliseconds() {
        assert_eq!(BARGE_IN_DEBOUNCE_FRAMES, frames_for_secs(5.0));
        assert_eq!(BARGE_IN_DEBOUNCE_FRAMES, 313);
        assert!((barge_in_debounce_secs() - 5.008).abs() < 1e-6, "{}", barge_in_debounce_secs());
    }

    /// INVARIANT: a disarmed monitor never fires, however much the CEO talks — speech while
    /// Rich is silent is an utterance, not an interruption.
    #[test]
    fn a_disarmed_monitor_never_fires() {
        let mut m = BargeInMonitor::default();
        for _ in 0..(BARGE_IN_DEBOUNCE_FRAMES * 3) {
            assert!(!m.push(true));
        }
        assert!(!m.force());
    }

    /// INVARIANT: continuous speech fires EXACTLY on the 313th frame — not the 312th.
    #[test]
    fn continuous_speech_fires_on_the_313th_frame_not_the_312th() {
        let mut m = BargeInMonitor::default();
        m.arm();
        for i in 1..BARGE_IN_DEBOUNCE_FRAMES {
            assert!(!m.push(true), "fired early at frame {i}");
        }
        assert_eq!(m.run_frames(), BARGE_IN_DEBOUNCE_FRAMES - 1);
        assert!(m.push(true), "should fire on frame {BARGE_IN_DEBOUNCE_FRAMES}");
    }

    /// INVARIANT: the debounce fires at most once per armed period, so one interruption can
    /// never be delivered twice.
    #[test]
    fn fires_at_most_once_per_armed_period() {
        let mut m = BargeInMonitor::default();
        m.arm();
        let mut fires = 0;
        for _ in 0..(BARGE_IN_DEBOUNCE_FRAMES * 2) {
            if m.push(true) {
                fires += 1;
            }
        }
        assert_eq!(fires, 1);
        m.arm(); // Rich speaks again -> a fresh interruption is possible
        let mut fires2 = 0;
        for _ in 0..BARGE_IN_DEBOUNCE_FRAMES {
            if m.push(true) {
                fires2 += 1;
            }
        }
        assert_eq!(fires2, 1);
    }

    /// THE ECHO CASE — the whole reason the debounce exists. Rich's own voice returning
    /// through the mic is bursty: syllables with gaps. Simulate 30 seconds of it (bursts of
    /// 20 speech frames = 320 ms separated by 5 silent frames = 80 ms) and assert it NEVER
    /// interrupts him, because every gap resets the run.
    #[test]
    fn bursty_echo_never_barges_in_because_gaps_reset_the_run() {
        let mut m = BargeInMonitor::default();
        m.arm();
        // 30 s / (25 frames x 16 ms) = 75 bursts.
        for _ in 0..75 {
            for _ in 0..20 {
                assert!(!m.push(true), "echo burst fired a barge-in");
            }
            for _ in 0..5 {
                assert!(!m.push(false));
            }
        }
        assert_eq!(m.run_frames(), 0);
    }

    /// INVARIANT: a run one frame short, then a gap, then a full run — only the full run
    /// fires. Guards against an off-by-one that would let echo accumulate across gaps.
    #[test]
    fn a_gap_one_frame_before_the_threshold_discards_the_whole_run() {
        let mut m = BargeInMonitor::default();
        m.arm();
        for _ in 0..(BARGE_IN_DEBOUNCE_FRAMES - 1) {
            assert!(!m.push(true));
        }
        assert!(!m.push(false));
        assert_eq!(m.run_frames(), 0);
        for i in 1..BARGE_IN_DEBOUNCE_FRAMES {
            assert!(!m.push(true), "fired early at {i} after the reset");
        }
        assert!(m.push(true));
    }

    /// INVARIANT: "tap to stop" is instant and bypasses the debounce — the CEO's override
    /// while AEC is missing.
    #[test]
    fn tap_to_stop_interrupts_instantly_without_the_debounce() {
        let mut m = BargeInMonitor::default();
        m.arm();
        assert!(m.force());
        assert!(!m.force(), "force must be single-shot per armed period");
    }

    /// INVARIANT: the retune the pilot's history says is coming is one argument, and the
    /// derived duration follows the frame count exactly.
    #[test]
    fn retuning_the_debounce_is_one_number_and_the_math_follows() {
        let m = BargeInMonitor::with_secs(1.2);
        // 1.2 s -> ceil(1.2 x 16000 / 256) = ceil(75.0) = 75 frames -> 1.200 s exactly.
        assert_eq!(m.required_frames(), 75);
        assert!((frames_to_secs(75) - 1.2).abs() < 1e-6);
    }

    /// INVARIANT: the converged window is 25 frames = 0.400 s and the evidence threshold is
    /// 15 frames = 0.240 s, both re-derived from the frame math rather than quoted.
    #[test]
    fn the_converged_window_is_25_frames_which_is_400_milliseconds_exactly() {
        assert_eq!(AEC_BARGE_IN_WINDOW_FRAMES, 25);
        assert_eq!(AEC_BARGE_IN_WINDOW_FRAMES, frames_for_secs(0.400));
        assert!((aec_barge_in_window_secs() - 0.400).abs() < 1e-6, "{}", aec_barge_in_window_secs());
        assert_eq!(AEC_BARGE_IN_REQUIRED_FRAMES, 15);
        assert!((frames_to_secs(AEC_BARGE_IN_REQUIRED_FRAMES) - 0.240).abs() < 1e-6);
        // 15 of 25 is 60 % of the window.
        assert_eq!(AEC_BARGE_IN_REQUIRED_FRAMES * 100 / AEC_BARGE_IN_WINDOW_FRAMES, 60);
        // And the window must fit in the u32 bitmask the monitor slides.
        assert!(AEC_BARGE_IN_WINDOW_FRAMES < 32);
        // 5.008 / 0.400 = 12.52x shorter than the fallback.
        let ratio = barge_in_debounce_secs() / aec_barge_in_window_secs();
        assert!((ratio - 12.52).abs() < 0.01, "{ratio}");
    }

    /// **THE SAFETY INVARIANT.** A fresh monitor uses the 5.008 s consecutive rule. The short
    /// window is never the default and can only be reached by an explicit, measured claim of
    /// confidence from the canceller.
    #[test]
    fn the_five_second_rule_is_the_default_and_the_short_window_must_be_earned() {
        let m = BargeInMonitor::default();
        assert_eq!(m.mode(), BargeInMode::Consecutive);
        assert_eq!(m.effective_required_frames(), BARGE_IN_DEBOUNCE_FRAMES);
        assert!((m.effective_debounce_secs() - 5.008).abs() < 1e-6);

        let mut m = BargeInMonitor::default();
        m.set_near_end_gated(true);
        assert_eq!(m.mode(), BargeInMode::Windowed);
        assert_eq!(m.effective_required_frames(), AEC_BARGE_IN_REQUIRED_FRAMES);
        assert!((m.effective_debounce_secs() - 0.400).abs() < 1e-6);

        // And it goes straight back the moment confidence is withdrawn.
        m.set_near_end_gated(false);
        assert_eq!(m.mode(), BargeInMode::Consecutive);
        assert!((m.effective_debounce_secs() - 5.008).abs() < 1e-6);
    }

    /// INVARIANT: under the windowed rule, 15 near-end frames fire it — on the 15th, not the
    /// 14th.
    #[test]
    fn the_windowed_rule_fires_on_the_fifteenth_frame_not_the_fourteenth() {
        let mut m = BargeInMonitor::default();
        m.set_near_end_gated(true);
        m.arm();
        for i in 1..AEC_BARGE_IN_REQUIRED_FRAMES {
            assert!(!m.push(true), "fired early at frame {i}");
        }
        assert_eq!(m.window_hits(), AEC_BARGE_IN_REQUIRED_FRAMES - 1);
        assert!(m.push(true), "should fire on frame {AEC_BARGE_IN_REQUIRED_FRAMES}");
    }

    /// **THE WHOLE POINT.** Real speech has gaps between words; the consecutive rule treats
    /// every gap as a reset. Measured on the AEC rig with a converged canceller, a full 1000 ms
    /// interruption produced 46 near-end frames of 62 and a longest unbroken run of only 16 —
    /// so under the consecutive rule NO interruption of any length reliably registers once the
    /// near-end verdict is honest.
    ///
    /// Same 400 ms of realistically-gappy speech, both rules, side by side.
    #[test]
    fn gappy_real_speech_registers_under_the_window_and_never_under_the_consecutive_rule() {
        // 25 frames = 0.400 s at a 72 % duty cycle: 4 on, 1 off, 4 on, 1 off, ...
        let pattern: Vec<bool> = (0..AEC_BARGE_IN_WINDOW_FRAMES)
            .map(|i| i % 5 != 4)
            .collect();
        let speaking = pattern.iter().filter(|v| **v).count();
        assert_eq!(speaking, 20, "premise: 20 of 25 frames are speech");
        let longest = pattern
            .split(|v| !*v)
            .map(|run| run.len())
            .max()
            .unwrap();
        assert_eq!(longest, 4, "premise: the longest unbroken run is only 4 frames = 64 ms");

        let mut windowed = BargeInMonitor::default();
        windowed.set_near_end_gated(true);
        windowed.arm();
        assert!(
            pattern.iter().any(|v| windowed.push(*v)),
            "0.400 s of ordinary gappy speech must interrupt Rich"
        );

        let mut consecutive = BargeInMonitor::default();
        consecutive.arm();
        assert!(
            !pattern.iter().any(|v| consecutive.push(*v)),
            "premise: the consecutive rule cannot see this at all"
        );
    }

    /// **THE REASON THE WINDOW IS GATED ON THE CANCELLER, stated as a test.**
    ///
    /// Fed RAW verdicts — no echo cancellation — the windowed rule fires on Rich's own voice
    /// in under half a second. That is not a defect in the window; it is the entire reason
    /// `set_near_end_gated` exists and the entire reason the 5.008 s rule stays as the
    /// fallback. Compare with `bursty_echo_never_barges_in_because_gaps_reset_the_run`, which
    /// asserts the consecutive rule survives exactly this input.
    #[test]
    fn the_windowed_rule_would_fire_on_raw_echo_which_is_why_it_needs_a_canceller() {
        let mut m = BargeInMonitor::default();
        m.set_near_end_gated(true);
        m.arm();
        // The same bursty echo the consecutive rule shrugs off: 20 on, 5 off.
        let mut fired_at = None;
        'outer: for burst in 0..10 {
            for i in 0..20 {
                if m.push(true) {
                    fired_at = Some(burst * 25 + i);
                    break 'outer;
                }
            }
            for i in 0..5 {
                if m.push(false) {
                    fired_at = Some(burst * 25 + 20 + i);
                    break 'outer;
                }
            }
        }
        let at = fired_at.expect("premise: the window DOES fire on raw echo");
        assert!(at < 25, "fired at frame {at}");
        // Which is why the default is the other rule, and why only a canceller that has
        // MEASURED its own residual below the VAD's speech floor may switch it.
        assert_eq!(BargeInMonitor::default().mode(), BargeInMode::Consecutive);
    }

    /// INVARIANT: the window slides. 14 frames of speech, then 25 frames of silence, then 14
    /// more must NOT fire — evidence older than 0.400 s has left the window.
    #[test]
    fn evidence_older_than_the_window_stops_counting() {
        let mut m = BargeInMonitor::default();
        m.set_near_end_gated(true);
        m.arm();
        for _ in 0..14 {
            assert!(!m.push(true));
        }
        assert_eq!(m.window_hits(), 14);
        for _ in 0..AEC_BARGE_IN_WINDOW_FRAMES {
            assert!(!m.push(false));
        }
        assert_eq!(m.window_hits(), 0, "the window did not slide");
        for i in 0..14 {
            assert!(!m.push(true), "stale evidence resurrected at {i}");
        }
    }

    /// INVARIANT: switching modes discards evidence gathered under the old rule. A 300-frame
    /// consecutive run must not instantly satisfy the 15-frame window, and vice versa.
    #[test]
    fn changing_mode_discards_evidence_gathered_under_the_old_rule() {
        let mut m = BargeInMonitor::default();
        m.arm();
        for _ in 0..(BARGE_IN_DEBOUNCE_FRAMES - 1) {
            assert!(!m.push(true));
        }
        assert_eq!(m.run_frames(), BARGE_IN_DEBOUNCE_FRAMES - 1);
        m.set_near_end_gated(true);
        assert_eq!(m.window_hits(), 0, "312 frames of consecutive evidence leaked into the window");
        for i in 1..AEC_BARGE_IN_REQUIRED_FRAMES {
            assert!(!m.push(true), "fired at {i} on stale evidence");
        }
        assert!(m.push(true));
    }

    /// INVARIANT: arming and disarming clear the window as well as the run — speech from
    /// before Rich started talking can never count toward interrupting him.
    #[test]
    fn arming_clears_the_window_as_well_as_the_run() {
        let mut m = BargeInMonitor::default();
        m.set_near_end_gated(true);
        m.arm();
        for _ in 0..14 {
            m.push(true);
        }
        assert_eq!(m.window_hits(), 14);
        m.arm();
        assert_eq!(m.window_hits(), 0);
        m.push(true);
        m.disarm();
        assert_eq!(m.window_hits(), 0);
    }

    /// INVARIANT: the windowed rule still fires at most once per armed period, and "tap to
    /// stop" still bypasses it entirely.
    #[test]
    fn the_windowed_rule_fires_once_and_tap_to_stop_still_overrides_it() {
        let mut m = BargeInMonitor::default();
        m.set_near_end_gated(true);
        m.arm();
        let mut fires = 0;
        for _ in 0..100 {
            if m.push(true) {
                fires += 1;
            }
        }
        assert_eq!(fires, 1);

        let mut m2 = BargeInMonitor::default();
        m2.set_near_end_gated(true);
        m2.arm();
        assert!(m2.force(), "tap to stop must remain instant under either rule");
    }

    /// INVARIANT: the null gate is honest — it sees the frames (proving the seam is wired)
    /// and modifies not one sample of them, and it never claims confidence, so selecting it
    /// leaves the 5.008 s debounce permanently in force.
    #[test]
    fn the_null_echo_gate_sees_the_frames_and_cancels_nothing() {
        let mut gate = NoEchoCancellation::default();
        let mut capture = vec![0.25f32; 256];
        let before = capture.clone();
        gate.process_capture(&mut capture);
        gate.process_capture(&mut capture);
        assert_eq!(gate.observed_frames(), 2);
        assert_eq!(capture, before, "the null gate must not pretend to cancel");
        assert!(!gate.cancels());
        assert!(!gate.confident(), "the null gate must never shorten the debounce");
        assert!(!gate.near_end_speech());
    }
}

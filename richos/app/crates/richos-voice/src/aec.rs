//! **Acoustic echo cancellation** — the gap that has been open since the Buzz huddles.
//!
//! ## What was actually wrong, and why the 5 s window was the honest answer before this
//!
//! With an open mic beside open speakers, Rich's own voice returns into the microphone. A
//! VAD cannot tell "the CEO is interrupting" from "Rich is hearing himself" — both are
//! speech-shaped energy. The field-proven workaround (v3 of the pilot's barge-in patch,
//! 2026-07-23) exploits one incidental property of echo: it is *bursty*, with gaps between
//! syllables, so a long CONSECUTIVE-frame debounce never completes on echo. 313 frames x 256
//! samples / 16 000 Hz = 5.008 s. It works, and the price is that "no, stop" — 700 ms of
//! speech — is discarded.
//!
//! That is a **continuity race**, not a discrimination. This module replaces it with an
//! actual discrimination: subtract Rich's voice from the microphone signal, then look at what
//! is left.
//!
//! ## The approach, and why not the other two
//!
//! **Chosen: a partitioned-block frequency-domain adaptive filter (PBFDAF), written here.**
//! This is the same family of algorithm as `speexdsp`'s MDF and the linear stage of WebRTC's
//! AEC3, re-derived rather than vendored.
//!
//! - *License.* Nothing is vendored, so there is nothing to name in the open-source audit.
//!   This file is AGPL-3.0-only with the rest of the crate, and the audit is the reason to
//!   say so here: a header that names a license the tree does not use is the one place a
//!   reader will trust over the manifest. `webrtc-audio-processing` would have brought
//!   BSD-3-Clause C++ plus a meson/autotools build into a signed, notarized bundle;
//!   `speexdsp` would have brought BSD-3-Clause C. Both are perfectly usable licenses — the
//!   cost is not the license text, it is the build system and the audit surface.
//! - *Testability.* Every line here runs under `cargo test -p richos-voice` with no device,
//!   so ERLE is a reproducible CI number rather than a story about a room.
//! - *Portability.* Windows is a named v1 packaging target. This works there unchanged.
//!
//! **Rejected: macOS `kAudioUnitSubType_VoiceProcessingIO`.** It is genuinely the cheapest
//! path *if* it fits, and `coreaudio-rs` 0.14.2 (already in the tree under `cpal`) exposes
//! `IOType::VoiceProcessingIO`. Four things killed it, in order of weight:
//!
//! 1. **It would force AGC and noise suppression onto the transcription path.** VPIO is a
//!    telephony unit: its NS cannot be disabled on macOS (there is no `AVAudioSession` to
//!    turn it off through, and `kAUVoiceIOProperty_BypassVoiceProcessing` bypasses the echo
//!    canceller too — it is all or nothing). Dictation and call transcription feed on this
//!    same audio. Trading a barge-in bug for a fabrication bug is the one outcome the brief
//!    names as unacceptable, and it would be *unmeasurable in advance* because the processing
//!    is opaque.
//! 2. **It is a duplex unit.** Its echo reference is what *it* renders, so `playout.rs` would
//!    have to render through it as well — replacing the gapless queue whose stop latency is
//!    the measured basis of barge-in.
//! 3. **cpal cannot ask for it.** `cpal-0.17.3/src/host/coreaudio/macos/device.rs:206-212`
//!    hard-codes `IOType::HalOutput` for input and `IOType::DefaultOutput` for output, with
//!    no seam. Using VPIO means hand-writing the AudioUnit I/O for both directions.
//! 4. **macOS only, and untestable.** Zero of it would run in `cargo test`.
//!
//! It remains the right *second* implementation if the measured figures below are not enough
//! in the CEO's actual room. The `EchoGate` trait is where it would go — though see the next
//! section, which is the reason it would not help either.
//!
//! ## WHAT THIS ACHIEVES, AND WHERE IT DOES NOT — measured, both
//!
//! On a linear echo path (`examples/aec_rig.rs`, 20 s, 50.0 ms bulk delay, 5-tap room,
//! -54 dBFS room noise):
//!
//! ```text
//!   steady-state ERLE                 28.0 dB   (residual -58.4 dBFS vs VAD floor -46.0 dBFS)
//!   time to 20 dB ERLE                4 s
//!   near-end false positives after
//!     confidence                      0
//!   shortest interruption registering  400 ms   (was 5008 ms)
//!   transparency while Rich is silent  bit-identical
//!   cost                              56.8 us per 16 000 us block = 0.355 % of one core
//! ```
//!
//! On the CEO's ACTUAL hardware — Mac mini built-in speakers out, Elgato Wave:3 USB in —
//! it achieves 3-7 dB, and so would every other linear canceller in existence.
//! `examples/aec_probe.rs` establishes why, and the answer is not the algorithm:
//!
//! ```text
//!   echo present and dominant         microphone -42.3 dBFS vs room noise -67.2 dBFS
//!   round-trip delay                  28-36 ms, ENVELOPE correlation 0.973
//!   WAVEFORM correlation at the best
//!     sample-resolution alignment     0.174
//!   BEST POSSIBLE ERLE FOR ANY
//!     LINEAR CANCELLER (coherence)    5.5 dB full band, 8.7 dB over 300-3400 Hz
//! ```
//!
//! Magnitude-squared coherence is an upper bound on what ANY linear filter can do, so WebRTC's
//! AEC3, `speexdsp` and the linear stage of `VoiceProcessingIO` all hit the same wall here.
//! Clock drift (~1 dB), loudspeaker overdrive (flat across a 20 dB level sweep) and a reverb
//! tail longer than the filter (ceiling saturates: 4.2/5.5/6.6/7.1/7.5 dB at 32/64/128/256/512
//! ms of analysis) were each tested and eliminated.
//!
//! **The consequence, stated plainly: [`EchoCanceller::confident`] returns false on that
//! hardware, so the 5.008 s debounce stays in force there.** That is the design working. The
//! canceller measured its own residual, found it could not support a 0.400 s window, and
//! declined to shorten it. On headphones — where nothing comes back into the microphone — it
//! is confident immediately, which is the case where the 5 s wait was pure cost.
//!
//! The way forward is in the two correlations: the echo's LOUDNESS is predictable (0.973) even
//! though its waveform is not (0.174), and barge-in only ever needed one bit per frame. A
//! magnitude-domain echo-presence DETECTOR — never a suppressor, since suppression is what eats
//! consonants — measured 0.7 % false positives against 73.6 % detection on the real recording
//! during this work. It is deliberately NOT in this crate: it also showed an unexplained 64 %
//! false-positive mode on a synthetic path, and the thing that decides whether Rich cuts himself
//! off mid-sentence does not ship with an unexplained failure mode.
//!
//! ## Structure
//!
//! ```text
//!   output callback ──► RateConverter (device Hz -> 16 kHz) ──► ReferenceRing (lock-free)
//!                                                                      │
//!   mic frame (256 @ 16 kHz) ──► process_capture ◄─────────────────────┘
//!                                     │
//!                    ┌────────────────┼─────────────────┐
//!                    ▼                ▼                 ▼
//!             delay estimator    PBFDAF filter     leak tracker
//!             (envelope xcorr)   (2048 taps)       (minimum statistics)
//!                                     │                 │
//!                                     ▼                 ▼
//!                              residual e = d - y   near-end verdict
//!                                     │             + confidence
//!                                     ▼
//!                              THE FRAME THE VAD SEES
//! ```
//!
//! ## The transparency guarantee — why dictation cannot be harmed
//!
//! The only thing this module ever does to the microphone signal is subtract a linear
//! estimate of Rich's own voice. There is no gate, no expander, no spectral suppressor and
//! no AGC — deliberately, because those are what eat consonants.
//!
//! When Rich has been silent for one full filter tail (2048 samples = 128.0 ms) every
//! reference partition is exactly zero, so the estimate is exactly zero and the mic frame is
//! passed through **bit-identical**. That is not a hope about WER, it is an arithmetic
//! property, and `silence_from_rich_leaves_the_microphone_bit_identical` asserts it. Dictation
//! and call transcription happen while Rich is *not* speaking, so they are provably untouched.
//!
//! During double-talk the residual is measured against raw capture by the rig
//! (`examples/aec_transcribe.rs`), because there the claim is empirical, not arithmetic.

use crate::fft::{Fft, C};
use crate::vad::{SAMPLE_RATE, VAD_FRAME_SAMPLES};
use crate::wav::RateConverter;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

/// One AEC block is exactly one VAD frame, so no buffering is introduced anywhere and the
/// crate's frame math keeps meaning what it says. 256 / 16 000 = 16.000 ms.
pub const AEC_BLOCK: usize = VAD_FRAME_SAMPLES;

/// Overlap-save transform length: twice the block.
pub const AEC_FFT: usize = 2 * AEC_BLOCK;

/// Filter partitions. The tail this buys is the *spread* of the echo path (room reverb), not
/// the bulk delay — the delay estimator removes that separately.
///
/// ```text
///   8 partitions x 256 samples = 2048 taps
///   2048 / 16 000              = 0.128 s = 128.000 ms of reverb tail
/// ```
pub const AEC_PARTITIONS: usize = 8;

/// Adaptive filter length in samples: 8 x 256 = 2048 = 128.000 ms.
pub const AEC_TAPS: usize = AEC_PARTITIONS * AEC_BLOCK;

/// Reference ring capacity in samples, a power of two for cheap masking.
/// 32 768 / 16 000 = 2.048 s — comfortably more than the delay search range plus jitter.
pub const REF_RING_SAMPLES: usize = 32_768;

/// Longest bulk delay the estimator will look for.
/// 32 blocks x 256 / 16 000 = 0.512 s. A speaker-to-mic acoustic round trip on a desktop is
/// tens of milliseconds; half a second is generous headroom for a slow USB device.
pub const MAX_DELAY_BLOCKS: usize = 32;

/// Blocks of envelope history before a delay estimate is attempted at all.
/// 64 x 256 / 16 000 = 1.024 s — enough for a lag-32 correlation to mean something, and short
/// enough that the filter is not converging against the wrong alignment for long.
pub const DELAY_MIN_HISTORY_BLOCKS: usize = 64;

/// Envelope history the delay estimator correlates over.
/// 256 blocks x 256 / 16 000 = 4.096 s.
pub const DELAY_HISTORY_BLOCKS: usize = 256;

/// How often the bulk delay is re-estimated.
/// 64 blocks x 256 / 16 000 = 1.024 s.
pub const DELAY_REESTIMATE_BLOCKS: u32 = 64;

/// Normalized LMS step size. `0 < mu < 2` is the stability range for a per-bin normalized
/// frequency-domain update. 0.5 was chosen by measurement, not taste: it is the value at
/// which the rig's steady-state ERLE stops improving, and going higher buys nothing while
/// costing robustness against the double-talk this whole module exists to survive.
pub const AEC_STEP_SIZE: f32 = 0.5;

/// A reference block quieter than this is silence, and the filter neither adapts nor lets the
/// leak tracker learn from it. -60 dBFS.
pub const FAR_END_ACTIVE_RMS: f32 = 0.001;

/// How far above the tracked residual-echo floor a frame must sit to be called near-end
/// speech rather than leftover echo. 3.0 = +9.54 dB.
///
/// This is the CONSERVATIVE threshold, and it is deliberately conservative: it decides whether
/// to interrupt Rich, and a false positive there is Rich cutting himself off mid-sentence.
pub const NEAR_END_MARGIN: f32 = 3.0;

/// The margin at which the filter STOPS ADAPTING — the same 3.0 (+9.54 dB) the near-end
/// verdict uses. What actually protects the filter is not a tighter threshold, it is the
/// HANGOVER below.
///
/// A tighter margin was tried first, and measured, and rejected twice:
///   - 1.5 (+3.52 dB) with the freeze always armed: deadlock. The filter has learned nothing,
///     so its predicted echo is far too low, so every loud block looks like near-end speech,
///     so it freezes, so it never learns. ERLE 28.0 dB -> 2.0 dB; never became confident.
///   - 2.0 (+6.02 dB), gated on 6 dB of ERLE so the deadlock could not happen: still starved
///     steady-state adaptation. ERLE 28.0 dB -> 14.0 dB, and because the residual then sat at
///     -44.3 dBFS — ABOVE the VAD's -46.0 dBFS speech floor — echo started reading as speech:
///     0 near-end false positives became 28.
///
/// The lesson is that `leak_gain` is a MINIMUM statistic, so `predicted_echo` is deliberately a
/// low estimate. Thresholds close to it fire constantly on ordinary variation. Nine and a half
/// dB above a minimum is a suspicion; six is a coin toss.
pub const ADAPT_FREEZE_MARGIN: f32 = 3.0;

/// **The hold-over does not engage until there is a converged filter to protect.**
///
/// This gate is not optional and the reason is worth stating exactly, because it was measured
/// three times before it was understood.
///
/// The near-end threshold and the freeze threshold are the SAME (3.0). The only thing the
/// freeze adds is the 0.400 s hold-over — and a hold-over is lethal during convergence. Before
/// the filter has learned the path, `predicted_echo` is far below the real residual, so
/// suspicions fire constantly; with a hold-over, each one freezes the next 25 blocks and the
/// filter is frozen essentially forever. Measured with the hold-over always armed: ERLE 28.0 dB
/// -> 3.2 dB, never confident, no interruption of any length registering.
///
/// 6 dB is the point at which the filter is demonstrably doing real work — a quarter of the
/// echo power already gone — and therefore worth defending. Below it, adapting is all upside:
/// there is nothing to lose and the barge-in debounce is still the 5.008 s fallback anyway.
pub const ADAPT_PROTECT_ERLE_DB: f32 = 6.0;

/// Blocks to keep adaptation frozen after the last suspicion of near-end speech.
/// 25 x 256 / 16 000 = 0.400 s. A double-talk detector cannot see a word's onset until the
/// word has started, so without a hold-over the filter always adapts on the first frames of
/// every sentence the CEO says.
pub const ADAPT_FREEZE_HANGOVER_BLOCKS: u32 = 25;

/// **The confidence threshold, and the number the short debounce rests on.**
///
/// `vad::VadConfig::absolute_floor` is 0.005 RMS (-46.02 dBFS): below that the VAD will never
/// call a frame speech, whatever the room does. The canceller is "confident" when its tracked
/// residual-echo floor sits at least 6 dB below that — i.e. when leftover echo is, by
/// measurement, incapable of reaching the threshold that decides a barge-in.
///
/// ```text
///   absolute_floor           = 0.005     RMS = -46.02 dBFS
///   6 dB below               = 0.005 / 2 = 0.0025 RMS = -52.04 dBFS
/// ```
pub const CONFIDENT_LEAK_RMS: f32 = 0.0025;

/// Consecutive blocks the confidence condition must hold before it is believed.
/// 125 x 256 / 16 000 = 2.000 s. A momentary dip in residual during convergence is not the
/// same thing as a converged filter, and the barge-in debounce is downstream of this answer.
pub const CONFIDENCE_HOLD_BLOCKS: u32 = 125;

/// Far-end blocks that must have been observed before confidence can be claimed at all.
/// 125 x 256 / 16 000 = 2.000 s of Rich actually speaking. Before that the filter has not
/// been shown enough of the echo path to have an opinion.
pub const CONFIDENCE_WARMUP_BLOCKS: u32 = 125;

/// Consecutive blocks of measured divergence before the filter is wiped.
/// 12 x 256 / 16 000 = 0.192 s. A single bad block is a transient, not a broken echo path.
pub const DIVERGENCE_BLOCKS: u32 = 12;

/// Continuous near-end-detected blocks, while the far end is active, after which the leak
/// floor is assumed stale rather than the CEO assumed to be still talking.
/// 125 x 256 / 16 000 = 2.000 s. Directly analogous to `vad::STUCK_SPEECH_FRAMES`, and for
/// the same reason: a detector that can never be wrong can never recover.
pub const LEAK_STUCK_BLOCKS: u32 = 125;

/// Exact seconds a block count represents — the same discipline as `vad::frames_to_secs`.
pub fn blocks_to_secs(blocks: u32) -> f32 {
    (blocks as f32 * AEC_BLOCK as f32) / SAMPLE_RATE as f32
}

/// The filter tail in seconds, derived rather than quoted: 2048 / 16 000 = 0.128 s.
pub fn filter_tail_secs() -> f32 {
    AEC_TAPS as f32 / SAMPLE_RATE as f32
}

// ---------------------------------------------------------------------------------------
// The reference transport
// ---------------------------------------------------------------------------------------

/// A lock-free single-producer/single-consumer ring carrying the playout reference signal
/// from the **output** callback thread to the **capture** callback thread.
///
/// `playout.rs` used to hand the reference to the echo gate behind a `Mutex` with `try_lock`,
/// dropping the frame on contention. Its own module docs called that out: *"A real AEC cannot
/// live behind a `Mutex` like this."* They were right — a dropped reference frame is not a
/// lost 16 ms, it is a permanent 16 ms shift between the reference and the echo, which
/// invalidates the filter's entire converged state.
///
/// No `unsafe`. Each slot is an `AtomicU32` holding `f32::to_bits`; the producer publishes the
/// write index with `Release` after storing the samples and the consumer acquires it, so the
/// samples are visible before the index that claims they exist. On arm64 a relaxed 32-bit
/// atomic store is a plain `str`, so the per-sample cost is the cost of the write itself.
///
/// When the consumer falls behind, the producer overwrites: staleness is worse than loss in a
/// real-time path. The consumer DETECTS that positively (`avail > capacity`) and reports it,
/// so an overrun forces a delay re-estimate rather than silently corrupting alignment.
pub struct ReferenceRing {
    slots: Vec<AtomicU32>,
    mask: usize,
    write: AtomicU64,
    read: AtomicU64,
    overrun_samples: AtomicU64,
}

impl std::fmt::Debug for ReferenceRing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReferenceRing")
            .field("capacity", &self.slots.len())
            .field("written", &self.write.load(Ordering::Relaxed))
            .field("read", &self.read.load(Ordering::Relaxed))
            .field("overrun_samples", &self.overrun_samples.load(Ordering::Relaxed))
            .finish()
    }
}

impl ReferenceRing {
    pub fn new(capacity: usize) -> ReferenceRing {
        assert!(capacity.is_power_of_two(), "ring capacity must be a power of two");
        ReferenceRing {
            slots: (0..capacity).map(|_| AtomicU32::new(0)).collect(),
            mask: capacity - 1,
            write: AtomicU64::new(0),
            read: AtomicU64::new(0),
            overrun_samples: AtomicU64::new(0),
        }
    }

    pub fn capacity(&self) -> usize {
        self.slots.len()
    }

    /// **Producer side — the output callback.** Never blocks, never allocates.
    pub fn push(&self, samples: &[f32]) {
        let w = self.write.load(Ordering::Relaxed);
        for (i, s) in samples.iter().enumerate() {
            self.slots[(w as usize).wrapping_add(i) & self.mask].store(s.to_bits(), Ordering::Relaxed);
        }
        self.write.store(w + samples.len() as u64, Ordering::Release);
    }

    /// **Consumer side — the capture callback.** Appends everything available to `out` and
    /// returns how many samples were LOST to overrun before this read (0 in the normal case).
    pub fn drain(&self, out: &mut Vec<f32>) -> u64 {
        let w = self.write.load(Ordering::Acquire);
        let mut r = self.read.load(Ordering::Relaxed);
        let cap = self.slots.len() as u64;
        let mut lost = 0u64;
        if w.saturating_sub(r) > cap {
            // The producer lapped us. Skip to the oldest sample still actually present.
            lost = w - r - cap;
            r = w - cap;
            self.overrun_samples.fetch_add(lost, Ordering::Relaxed);
        }
        while r < w {
            out.push(f32::from_bits(self.slots[(r as usize) & self.mask].load(Ordering::Relaxed)));
            r += 1;
        }
        self.read.store(r, Ordering::Release);
        lost
    }

    pub fn written(&self) -> u64 {
        self.write.load(Ordering::Relaxed)
    }

    pub fn overrun_samples(&self) -> u64 {
        self.overrun_samples.load(Ordering::Relaxed)
    }
}

/// The output callback's end of the reference path: rate conversion plus the ring push, with
/// every buffer preallocated so the callback never allocates.
///
/// This lives here rather than in `playout.rs` so the conversion and the ring stay one unit:
/// the reference must reach the canceller at exactly 16 kHz with a continuous phase, and
/// splitting those two facts across two files is how they drift apart.
pub struct ReferenceSink {
    ring: Arc<ReferenceRing>,
    rc: RateConverter,
    scratch: Vec<f32>,
}

impl ReferenceSink {
    pub fn new(ring: Arc<ReferenceRing>, device_rate: u32) -> ReferenceSink {
        ReferenceSink {
            ring,
            rc: RateConverter::new(device_rate, SAMPLE_RATE),
            scratch: Vec::with_capacity(4096),
        }
    }

    /// One block of exactly what went to the speakers, mono, at the device rate.
    pub fn submit(&mut self, mono_at_device_rate: &[f32]) {
        self.scratch.clear();
        self.rc.push(mono_at_device_rate, &mut self.scratch);
        self.ring.push(&self.scratch);
    }
}

// ---------------------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------------------

/// Everything the canceller knows about itself, in measured units. Reported, never guessed.
#[derive(Debug, Clone, Copy, Default)]
pub struct AecMetrics {
    /// Echo Return Loss Enhancement in dB: `10*log10(mic power / residual power)`, smoothed,
    /// accumulated only while the far end is active and no near-end speech is detected.
    pub erle_db: f32,
    /// Bulk delay the estimator settled on, in whole blocks.
    pub delay_blocks: usize,
    /// The same delay in milliseconds, derived.
    pub delay_ms: f32,
    /// Normalized correlation of the winning delay lag, 0..1. Below ~0.3 the estimate is not
    /// trustworthy and the canceller says so rather than pretending.
    pub delay_confidence: f32,
    /// **Typical** residual level while Rich is audible — how much echo still gets through,
    /// averaged rather than minimized. This is the number `CONFIDENT_LEAK_RMS` is compared
    /// against, and the number that has to sit below the VAD's speech floor for the short
    /// barge-in window to be allowed.
    pub leak_floor_rms: f32,
    /// Blocks in which the far end was active — the filter's actual training time.
    pub far_end_blocks: u64,
    /// Reference samples lost to ring overrun. Non-zero means the capture thread stalled.
    pub reference_overruns: u64,
    /// Blocks where the aligned reference was not yet available. Non-zero means the bulk
    /// delay is shorter than one block, which no real acoustic path is.
    pub reference_underruns: u64,
    /// Times the filter was reset because it was making the signal worse, not better.
    pub divergence_resets: u64,
    /// Is the residual echo floor provably below the VAD's speech threshold?
    pub confident: bool,
    /// **WHETHER [`Self::erle_db`] IS A MEASUREMENT AT ALL.**
    ///
    /// `erle_db` returns a hard `0.0` until both smoothed powers are above `1e-12`, and those
    /// only accumulate on blocks that are far-end-active with the adaptation unfrozen. So
    /// `0.0` has always had two meanings — *"the canceller is removing exactly nothing"* and
    /// *"no block has yet said anything"* — and the operator log printed the same three
    /// characters for both. Ray's candidate-.6 walk read `erle=0.0 dB` off four discards and
    /// had to guess which one it was (`docs/verification/2026-09-17-nightly-1.2.0-20260917.6-onscreen-audit.md`
    /// defect 3); candidate .5's identical line carried `-2.9 dB`, a real reading, so the two
    /// were indistinguishable in form.
    ///
    /// This is the bit that separates them, and it is read off the same condition `erle_db`
    /// uses rather than re-derived beside it.
    pub erle_measured: bool,
    /// **WHETHER [`Self::leak_floor_rms`] IS A MEASUREMENT AT ALL.**
    ///
    /// `residual_typ_rms` starts at `1.0` and stays there until the first far-active block
    /// seeds it. `1.0` rms is `20*log10(1.0)` = **0.0 dBFS — full scale**, so an unseeded
    /// tracker publishes the loudest reading the format can express and it looks exactly like
    /// a measurement. That is the other half of defect 3: `residual 0.0 dBFS` was the initial
    /// value, printed as though the microphone were clipping.
    ///
    /// It is `residual_seeded` itself, not a threshold test on the value — the flag the
    /// canceller already keeps for exactly this question.
    pub leak_measured: bool,
}

impl AecMetrics {
    /// `erle_db` as text: the number when it is a measurement, the words `not measured`
    /// when it is the sentinel. **Never `0.0 dB` standing in for "nothing measured yet".**
    pub fn erle_text(&self) -> String {
        if self.erle_measured {
            format!("{:.1} dB", self.erle_db)
        } else {
            "not measured".to_string()
        }
    }

    /// `leak_floor_rms` in dBFS as text, under the same rule. An unseeded tracker reads
    /// `0.0 dBFS` — full scale — which is a claim about a clipping microphone rather
    /// than a reading of anything.
    pub fn leak_text(&self) -> String {
        if self.leak_measured {
            format!("{:.1} dBFS", 20.0 * self.leak_floor_rms.max(1e-9).log10())
        } else {
            "not measured".to_string()
        }
    }

    /// One line for stderr and for the rig's report.
    pub fn summary(&self) -> String {
        format!(
            "ERLE {} · delay {} blk ({:.1} ms, conf {:.2}) · leak {:.5} rms ({}) · far-end {} blk · confident={} · overrun {} · underrun {} · resets {}",
            self.erle_text(),
            self.delay_blocks,
            self.delay_ms,
            self.delay_confidence,
            self.leak_floor_rms,
            self.leak_text(),
            self.far_end_blocks,
            self.confident,
            self.reference_overruns,
            self.reference_underruns,
            self.divergence_resets,
        )
    }
}

/// Everything one block decided, in the units it decided it in. The echo-cancellation
/// counterpart of `Vad::speech_run` and `BargeInMonitor::run_frames`: the diagnostic that
/// makes "why did it not barge in" a question with an answer instead of a mystery.
#[derive(Debug, Clone, Copy, Default)]
pub struct BlockStats {
    /// RMS of the aligned reference — how loud Rich is, as the mic will shortly hear him.
    pub reference_rms: f32,
    /// Peak-decay envelope of the reference, which is what the echo actually follows.
    pub reference_env: f32,
    /// RMS of the microphone frame as it arrived.
    pub mic_rms: f32,
    /// RMS of the filter's estimate of the echo.
    pub echo_estimate_rms: f32,
    /// RMS of what is left after subtraction — the frame the VAD will see.
    pub residual_rms: f32,
    /// How much residual echo is EXPECTED at this reference level.
    pub predicted_echo_rms: f32,
    /// Was the far end active enough to learn from?
    pub far_active: bool,
    /// Was near-end speech (a real interruption) declared?
    pub near_end: bool,
    /// Did the filter adapt on this block?
    pub adapted: bool,
}

// ---------------------------------------------------------------------------------------
// The canceller
// ---------------------------------------------------------------------------------------

/// A partitioned-block frequency-domain adaptive filter with bulk-delay alignment, a
/// minimum-statistics residual-echo tracker, and a near-end (double-talk) verdict.
pub struct EchoCanceller {
    fft: Fft,
    ring: Arc<ReferenceRing>,

    /// Reference samples in stream order, oldest first, with `ref_base` their absolute index.
    ref_line: VecDeque<f32>,
    ref_base: u64,
    /// Absolute index of the next capture sample to be processed.
    cap_abs: u64,

    /// Spectra of the last `AEC_PARTITIONS` reference blocks, newest at index 0.
    x_spectra: Vec<Vec<C>>,
    /// Filter weights, one spectrum per partition.
    weights: Vec<Vec<C>>,
    /// Smoothed per-bin reference power, the NLMS normaliser.
    bin_power: Vec<f32>,

    // Preallocated scratch — nothing in `process_capture` allocates after warm-up.
    scratch_a: Vec<C>,
    scratch_b: Vec<C>,
    x_time: Vec<f32>,
    y_block: Vec<f32>,
    drain_buf: Vec<f32>,


    // Delay estimation.
    ref_env: VecDeque<f32>,
    cap_env: VecDeque<f32>,
    delay_blocks: usize,
    delay_confidence: f32,
    blocks_since_delay: u32,
    /// A delay estimate awaiting a second, agreeing observation before it is trusted.
    delay_candidate: Option<usize>,

    // Leak / near-end / confidence state.
    /// Peak-decay envelope of the aligned reference. Echo follows the reference ENVELOPE,
    /// not its instantaneous level, because the room keeps ringing after Rich stops.
    ref_env_level: f32,
    /// **The scale-invariant leak estimate**: residual RMS divided by reference envelope,
    /// tracked by minimum statistics. See `process_block` for why an absolute floor was
    /// wrong.
    leak_gain: f32,
    leak_floor_rms: f32,
    /// **Smoothed TYPICAL residual level while Rich is audible** — an average, deliberately
    /// not a minimum. This is what `confident()` compares against the VAD's speech threshold,
    /// because the claim being made is "leftover echo cannot reach the threshold that decides
    /// a barge-in", and a minimum statistic answers a different and much easier question.
    residual_typ_rms: f32,
    residual_seeded: bool,
    near_end: bool,
    near_end_run: u32,
    /// Blocks of adaptation freeze still owed after the last suspicion of near-end speech.
    freeze_hangover: u32,
    diverging_run: u32,
    confident_run: u32,
    /// Set once `confident_run` has ever reached the hold. See `confident()`.
    confident_latched: bool,
    /// Has the filter ever reached `ADAPT_PROTECT_ERLE_DB`? Latched; cleared only by a reset.
    protect_latched: bool,
    far_end_blocks: u64,
    reference_underruns: u64,
    divergence_resets: u64,

    // Smoothed powers for ERLE.
    d_power_smooth: f32,
    e_power_smooth: f32,

    blocks: u64,
    last: BlockStats,
}

impl EchoCanceller {
    /// Build a canceller and the ring the playout side writes into.
    pub fn new() -> (EchoCanceller, Arc<ReferenceRing>) {
        let ring = Arc::new(ReferenceRing::new(REF_RING_SAMPLES));
        (EchoCanceller::with_ring(ring.clone()), ring)
    }

    pub fn with_ring(ring: Arc<ReferenceRing>) -> EchoCanceller {
        let bins = AEC_FFT;
        EchoCanceller {
            fft: Fft::new(AEC_FFT),
            ring,
            ref_line: VecDeque::with_capacity(REF_RING_SAMPLES),
            ref_base: 0,
            cap_abs: 0,
            x_spectra: vec![vec![C::ZERO; bins]; AEC_PARTITIONS],
            weights: vec![vec![C::ZERO; bins]; AEC_PARTITIONS],
            bin_power: vec![0.0; bins],
            scratch_a: vec![C::ZERO; bins],
            scratch_b: vec![C::ZERO; bins],
            x_time: vec![0.0; AEC_FFT],
            y_block: vec![0.0; AEC_BLOCK],
            drain_buf: Vec::with_capacity(4096),
            ref_env: VecDeque::with_capacity(DELAY_HISTORY_BLOCKS),
            cap_env: VecDeque::with_capacity(DELAY_HISTORY_BLOCKS),
            delay_blocks: 0,
            delay_confidence: 0.0,
            blocks_since_delay: 0,
            delay_candidate: None,
            // Start pessimistic: full scale. Confidence must be EARNED downward, never
            // assumed, so a canceller that has seen nothing never claims to be working.
            ref_env_level: 0.0,
            leak_gain: 1.0,
            leak_floor_rms: 1.0,
            residual_typ_rms: 1.0,
            residual_seeded: false,
            near_end: false,
            near_end_run: 0,
            freeze_hangover: 0,
            diverging_run: 0,
            confident_run: 0,
            confident_latched: false,
            protect_latched: false,
            far_end_blocks: 0,
            reference_underruns: 0,
            divergence_resets: 0,
            d_power_smooth: 0.0,
            e_power_smooth: 0.0,
            blocks: 0,
            last: BlockStats::default(),
        }
    }

    /// What the most recent block decided, in measured units.
    pub fn last_block(&self) -> BlockStats {
        self.last
    }

    /// The scale-invariant leak estimate: the fraction of the reference envelope that still
    /// reaches the microphone after cancellation. `20*log10` of it is the total echo
    /// suppression, ERL and ERLE combined.
    pub fn leak_gain(&self) -> f32 {
        self.leak_gain
    }

    /// Was near-end speech (the CEO, not the echo) present in the last processed block?
    pub fn near_end_speech(&self) -> bool {
        self.near_end
    }

    /// **Is the residual echo provably too quiet to trigger a barge-in?**
    ///
    /// Three conditions, all of them measured:
    /// 1. The filter has seen at least [`CONFIDENCE_WARMUP_BLOCKS`] (2.000 s) of Rich actually
    ///    speaking, so it has had something to learn from.
    /// 2. **Every one of [`CONFIDENCE_HOLD_BLOCKS`] consecutive far-active blocks** had its own
    ///    residual below [`CONFIDENT_LEAK_RMS`] — 6 dB under the VAD's absolute speech
    ///    threshold. See [`EchoCanceller::confidence_condition`] for why this is the block's own
    ///    residual and no longer a smoothed estimate of it, and what the estimate cost on the
    ///    CEO's hardware.
    /// 3. No reference has been lost to a ring overrun, which would mean the alignment the
    ///    whole estimate rests on is stale.
    ///
    /// ## IT LATCHES, and that is not laziness — it is what makes barge-in possible at all
    ///
    /// Condition 2 is evaluated on the residual, and **the CEO's own voice is in the residual**:
    /// he is near-end, the canceller removes only Rich, so when he speaks the residual rises and
    /// the run resets. Without the latch, `confident()` goes false the instant he opens his
    /// mouth — and `confident()` is precisely what licenses the 0.400 s barge-in window he is
    /// opening his mouth to use. The window would therefore never be in force at the one moment
    /// it exists for.
    ///
    /// That is not a hypothetical. It was measured as a test failure:
    /// `barge_in_composition::a_four_hundred_millisecond_interruption_cuts_rich_off_and_becomes_a_turn`
    /// went red on the unlatched form with *"0.400 s of the CEO talking over Rich did not
    /// interrupt him"*, which is the feature removing itself.
    ///
    /// **What the latch asserts is a property of the ECHO PATH**, not of this second: the
    /// speakers, the room and the microphone are what they are, and they do not change because
    /// somebody started talking. When they DO change — a device swap, or a filter that has
    /// diverged far enough to be wiped — [`EchoCanceller::reset_filter`] clears the latch with
    /// everything else the filter had learned, which is the honest place for it to be cleared.
    /// A ring overrun withdraws it immediately and without a reset, because then the alignment
    /// the whole measurement rests on is stale.
    pub fn confident(&self) -> bool {
        self.confident_latched && self.ring.overrun_samples() == 0
    }

    /// The instantaneous form of the confidence test, before the hold. Split out so the hold
    /// is visibly a debounce rather than buried in a boolean.
    /// **THE CLAIM, MEASURED — not an estimate of it.** `e_rms` is THIS block's residual, the
    /// audio that actually reached the VAD, the endpointer and the recorder.
    ///
    /// ## What this used to be, and why an estimate could not carry the claim
    ///
    /// It read `self.residual_typ_rms < CONFIDENT_LEAK_RMS` — a smoothed average, filtered to
    /// exclude blocks where the near-end detector suspected the CEO was talking. Both
    /// properties are right for what that estimate is FOR (predicting how much echo to expect
    /// at a given reference level), and both are fatal when it is asked to decide confidence on
    /// a path where the canceller removes nothing:
    ///
    /// - **the `!near_end` filter throws away the loud blocks.** Uncancelled echo is speech, so
    ///   it trips the near-end detector — and those are precisely the blocks the threshold is
    ///   about. What survives into the average is the quiet ones.
    /// - **the average is frozen while Rich is silent**, so a value that dipped under the
    ///   threshold as a sentence trailed off keeps satisfying the condition through the whole
    ///   gap before the next one.
    ///
    /// ## Measured on the CEO's hardware, 2026-09-17 — three variants, five runs each
    ///
    /// Mac mini Speakers out, Elgato Wave:3 in, `examples/aec_live --secs 30`, full traces in
    /// `docs/verification/2026-09-17-aec-erle-on-the-ceo-rig.md`. Steady-state ERLE was −0.5 …
    /// +0.7 dB in every run — the canceller removes nothing measurable on this path — so every
    /// CONFIDENT below is false, and each one retires the taint rule and shortens the barge-in
    /// debounce from 5.008 s to 0.400 s on a signal still carrying Rich at full strength.
    ///
    /// ```text
    ///   shipped (smoothed estimate)                   2 of 5 CONFIDENT   ~5.1 s
    ///   + estimate measured only while far-active     4 of 5 CONFIDENT   ~5.1 s
    ///   + the hold counted only while far-active      4 of 5 CONFIDENT   5.4-19.7 s
    ///   this: the BLOCK's own residual                0 of 5 CONFIDENT
    /// ```
    ///
    /// The two middle rows are recorded because they are the evidence: tightening WHERE and
    /// WHEN the estimate was taken made it worse, which is what identified the estimator itself
    /// as the problem rather than its sampling.
    ///
    /// **What each false CONFIDENT cost, in the same runs:** 60 and 62 near-end false positives
    /// per ~1218 blocks, and **3 occasions in 30 seconds where Rich would have cut himself
    /// off** — the one regression the CEO's brief names: *"a regression here makes Rich
    /// interrupt himself mid-sentence, which is worse than the current cost."*
    ///
    /// ## Why the direct form is not merely stricter, it is the right claim
    ///
    /// [`EchoCanceller::confident`] means: leftover echo is, by measurement, incapable of
    /// reaching the threshold that decides a barge-in. That sentence is about what the residual
    /// DOES, block by block, and [`CONFIDENCE_HOLD_BLOCKS`] consecutive far-active blocks under
    /// [`CONFIDENT_LEAK_RMS`] is that sentence written down. No smoothing, no exclusions,
    /// nothing to be skewed.
    ///
    /// **Headphones are unaffected**, which is the case this must not break: the microphone
    /// never hears Rich, so every block's residual is the room noise floor — measured at
    /// −67.9 dBFS on this machine, 15.9 dB under the threshold — and confidence still arrives
    /// after 2.000 s of far-active blocks.
    ///
    /// **And the CEO's own voice makes it conservative in the safe direction.** If he talks over
    /// Rich the residual rises, the run resets, and the long debounce comes back until he stops.
    /// That is the correct bias: the failure it prevents is Rich interrupting himself.
    fn confidence_condition(&self, e_rms: f32) -> bool {
        self.far_end_blocks >= CONFIDENCE_WARMUP_BLOCKS as u64
            && e_rms < CONFIDENT_LEAK_RMS
            && self.ring.overrun_samples() == 0
    }

    pub fn metrics(&self) -> AecMetrics {
        AecMetrics {
            erle_db: self.erle_db(),
            erle_measured: self.erle_measured(),
            leak_measured: self.residual_seeded,
            delay_blocks: self.delay_blocks,
            delay_ms: (self.delay_blocks * AEC_BLOCK) as f32 * 1000.0 / SAMPLE_RATE as f32,
            delay_confidence: self.delay_confidence,
            leak_floor_rms: self.residual_typ_rms,
            far_end_blocks: self.far_end_blocks,
            reference_overruns: self.ring.overrun_samples(),
            reference_underruns: self.reference_underruns,
            divergence_resets: self.divergence_resets,
            confident: self.confident(),
        }
    }

    /// Echo Return Loss Enhancement, in dB. Zero until there is something to report — ask
    /// [`Self::erle_measured`] which of the two a `0.0` is before printing it.
    pub fn erle_db(&self) -> f32 {
        if !self.erle_measured() {
            return 0.0;
        }
        10.0 * (self.d_power_smooth / self.e_power_smooth).log10()
    }

    /// Has any block contributed to the ERLE accumulators yet? The ONE condition
    /// [`Self::erle_db`] branches on, exposed rather than duplicated — a second copy of it
    /// beside a print site is the way the two drift apart.
    pub fn erle_measured(&self) -> bool {
        self.d_power_smooth > 1e-12 && self.e_power_smooth > 1e-12
    }

    /// Drop everything learned. Used on a device change or a hard alignment break — the
    /// filter's converged state describes a path that no longer exists.
    pub fn reset_filter(&mut self) {
        for w in self.weights.iter_mut() {
            w.iter_mut().for_each(|c| *c = C::ZERO);
        }
        for x in self.x_spectra.iter_mut() {
            x.iter_mut().for_each(|c| *c = C::ZERO);
        }
        self.bin_power.iter_mut().for_each(|p| *p = 0.0);
        self.leak_gain = 1.0;
        self.leak_floor_rms = 1.0;
        self.residual_typ_rms = 1.0;
        self.residual_seeded = false;
        self.confident_run = 0;
        self.confident_latched = false;
        self.protect_latched = false;
        self.d_power_smooth = 0.0;
        self.e_power_smooth = 0.0;
    }

    /// Pull whatever the playout thread has produced into the aligned delay line.
    fn ingest_reference(&mut self) {
        self.drain_buf.clear();
        let lost = self.ring.drain(&mut self.drain_buf);
        if lost > 0 {
            // A positive signal that alignment is broken, not an inference from silence.
            self.ref_base += lost;
            self.blocks_since_delay = DELAY_REESTIMATE_BLOCKS;
        }
        let drained = std::mem::take(&mut self.drain_buf);
        self.ref_line.extend(drained.iter().copied());
        self.drain_buf = drained;

        // Bound the delay line: keep the search range plus the filter tail plus slack.
        let keep = MAX_DELAY_BLOCKS * AEC_BLOCK + AEC_TAPS + 4 * AEC_BLOCK;
        while self.ref_line.len() > keep {
            self.ref_line.pop_front();
            self.ref_base += 1;
        }
    }

    /// Reference sample at an absolute stream index; silence outside what we hold.
    #[inline]
    fn ref_at(&self, abs: i64) -> f32 {
        if abs < self.ref_base as i64 {
            return 0.0;
        }
        let i = (abs - self.ref_base as i64) as usize;
        self.ref_line.get(i).copied().unwrap_or(0.0)
    }

    /// Normalized cross-correlation of the two block envelopes; picks the lag at which the
    /// microphone envelope best follows the reference envelope.
    ///
    /// Envelope correlation rather than sample correlation on purpose: it is immune to the
    /// phase scrambling of the speaker/room/mic chain, costs one multiply per block per lag,
    /// and only has to be right to within a block — the filter's 128 ms tail absorbs the
    /// remainder.
    fn estimate_delay(&mut self) {
        let n = self.ref_env.len().min(self.cap_env.len());
        if n < DELAY_MIN_HISTORY_BLOCKS {
            return;
        }
        let refv: Vec<f32> = self.ref_env.iter().copied().collect();
        let capv: Vec<f32> = self.cap_env.iter().copied().collect();

        // Mean-remove so a constant noise floor cannot manufacture correlation.
        let rm = refv.iter().sum::<f32>() / n as f32;
        let cm = capv.iter().sum::<f32>() / n as f32;
        let r: Vec<f32> = refv.iter().map(|v| v - rm).collect();
        let c: Vec<f32> = capv.iter().map(|v| v - cm).collect();

        let mut best_lag = self.delay_blocks;
        let mut best = 0.0f32;
        for lag in 0..MAX_DELAY_BLOCKS.min(n / 2) {
            // capture[i] should look like reference[i - lag].
            let mut num = 0.0f32;
            let mut er = 0.0f32;
            let mut ec = 0.0f32;
            for i in lag..n {
                let a = r[i - lag];
                let b = c[i];
                num += a * b;
                er += a * a;
                ec += b * b;
            }
            let denom = (er * ec).sqrt();
            if denom <= 1e-12 {
                continue;
            }
            let corr = num / denom;
            if corr > best {
                best = corr;
                best_lag = lag;
            }
        }
        self.delay_confidence = best;
        if best > 0.3 {
            // Back off one block so the filter's tail brackets the true delay rather than
            // starting exactly on it — an echo arriving slightly EARLIER than estimated would
            // otherwise be outside the filter entirely and could never be cancelled.
            let want = best_lag.saturating_sub(1);
            // HYSTERESIS. A candidate must survive two consecutive estimates before it is
            // applied. Without this the estimate flickers between adjacent blocks — the true
            // delay here is 800 samples = 3.125 blocks, so lag 3 and lag 4 score almost
            // identically — and every flicker rotates the weight array and zeroes a
            // partition. Measured: ERLE sat at ~4 dB for the first 6 s, climbing only once
            // the estimate happened to settle.
            if want == self.delay_blocks {
                self.delay_candidate = None;
            } else if self.delay_candidate == Some(want) {
                self.delay_candidate = None;
                self.retune_delay(want);
            } else {
                self.delay_candidate = Some(want);
            }
        }
    }

    /// Move the bulk delay WITHOUT throwing away what the filter has learned.
    ///
    /// Partition `p` models physical delays `(delay_blocks + p) * 256 + k`. Increasing
    /// `delay_blocks` by one therefore means the same physical echo is now described by
    /// partition `p - 1`, so the whole weight array shifts by the difference and the
    /// partitions that shift in are zeroed.
    ///
    /// This exists because the naive alternative — accept the new delay and let the filter
    /// re-converge — costs everything. Measured on the rig: the first delay estimate lands at
    /// t = 2 s, and without this the ERLE curve went 3.5 dB -> -0.7 dB and did not climb back
    /// above zero until t = 6 s. During those four seconds Rich is uninterruptible.
    fn retune_delay(&mut self, want: usize) {
        let old = self.delay_blocks as i64;
        let shift = want as i64 - old;
        self.delay_blocks = want;
        if shift == 0 {
            return;
        }
        if shift.unsigned_abs() as usize >= AEC_PARTITIONS {
            // The move is bigger than the whole filter: nothing learned is reusable.
            self.reset_filter();
            return;
        }
        let n = shift.unsigned_abs() as usize;
        if shift > 0 {
            // Delay grew: partition p's content belongs at p - shift.
            self.weights.rotate_left(n);
            for w in self.weights.iter_mut().skip(AEC_PARTITIONS - n) {
                w.iter_mut().for_each(|c| *c = C::ZERO);
            }
        } else {
            self.weights.rotate_right(n);
            for w in self.weights.iter_mut().take(n) {
                w.iter_mut().for_each(|c| *c = C::ZERO);
            }
        }
    }

    /// **The block.** `mic` is exactly [`AEC_BLOCK`] samples; the residual is written back in
    /// place. Returns true if near-end speech (a real interruption) is present.
    pub fn process_block(&mut self, mic: &mut [f32]) -> bool {
        debug_assert_eq!(mic.len(), AEC_BLOCK);
        self.ingest_reference();

        let c = self.cap_abs as i64;
        let d_start = c - (self.delay_blocks * AEC_BLOCK) as i64;

        // Overlap-save needs the previous block and the current block of reference.
        let want_from = d_start - AEC_BLOCK as i64;
        let want_to = d_start + AEC_BLOCK as i64;
        let have_to = (self.ref_base + self.ref_line.len() as u64) as i64;
        if want_to > have_to {
            // The aligned reference has not arrived yet. Physically this means the acoustic
            // round trip is under one block (16 ms), which no speaker-to-mic path is. Count
            // it and pass the frame through untouched rather than subtracting garbage.
            self.reference_underruns += 1;
        }
        {
            // Take the buffer out so the aligned read can borrow `self` immutably; it goes
            // straight back, so no allocation happens on the hot path.
            let mut x_time = std::mem::take(&mut self.x_time);
            for (i, slot) in x_time.iter_mut().enumerate() {
                *slot = self.ref_at(want_from + i as i64);
            }
            self.x_time = x_time;
        }

        // Push the new reference spectrum, newest first.
        self.x_spectra.rotate_right(1);
        {
            let fft = &self.fft;
            let dst = &mut self.x_spectra[0];
            fft.load_real(&self.x_time, dst);
            fft.forward(dst);
        }

        // Estimate the echo: Y = sum_p W[p] . X[p].
        for s in self.scratch_a.iter_mut() {
            *s = C::ZERO;
        }
        for p in 0..AEC_PARTITIONS {
            let w = &self.weights[p];
            let x = &self.x_spectra[p];
            for k in 0..AEC_FFT {
                self.scratch_a[k] = self.scratch_a[k].add(w[k].mul(x[k]));
            }
        }
        self.fft.inverse(&mut self.scratch_a);
        // Overlap-save: the valid linear-convolution output is the SECOND half.
        for i in 0..AEC_BLOCK {
            self.y_block[i] = self.scratch_a[AEC_BLOCK + i].re;
        }

        // The residual — the only thing that ever touches the microphone signal.
        let mut d_pow = 0.0f32;
        let mut e_pow = 0.0f32;
        let mut y_pow = 0.0f32;
        for i in 0..AEC_BLOCK {
            let d = mic[i];
            let y = self.y_block[i];
            let e = d - y;
            d_pow += d * d;
            y_pow += y * y;
            e_pow += e * e;
            mic[i] = e;
        }
        let inv = 1.0 / AEC_BLOCK as f32;
        let d_rms = (d_pow * inv).sqrt();
        let e_rms = (e_pow * inv).sqrt();

        // Reference level for this block, at the aligned position.
        let mut x_pow = 0.0f32;
        for i in 0..AEC_BLOCK {
            let v = self.x_time[AEC_BLOCK + i];
            x_pow += v * v;
        }
        let x_rms = (x_pow * inv).sqrt();
        let far_active = x_rms > FAR_END_ACTIVE_RMS;

        // ---- envelopes for delay estimation -------------------------------------------
        // The raw MIC envelope, not the residual: once the filter converges the residual no
        // longer resembles the reference, and correlating against it would destroy the very
        // alignment that produced the convergence.
        if self.ref_env.len() == DELAY_HISTORY_BLOCKS {
            self.ref_env.pop_front();
            self.cap_env.pop_front();
        }
        // Envelope at zero delay, so the correlation lag IS the delay.
        let mut raw_x_pow = 0.0f32;
        for i in 0..AEC_BLOCK {
            let v = self.ref_at(c + i as i64);
            raw_x_pow += v * v;
        }
        self.ref_env.push_back((raw_x_pow * inv).sqrt());
        self.cap_env.push_back(d_rms);
        self.blocks_since_delay += 1;
        if self.blocks_since_delay >= DELAY_REESTIMATE_BLOCKS {
            self.blocks_since_delay = 0;
            self.estimate_delay();
        }

        // ---- the reference ENVELOPE -----------------------------------------------------
        // Echo follows the envelope of Rich's voice, not its instantaneous level: the room
        // keeps ringing for the length of its reverb tail after he stops. A peak-decay
        // follower with the filter's own tail as its time constant is the honest shape.
        //
        //   decay 0.72 per block over 8 blocks (one 128 ms filter tail):
        //   0.72^8 = 0.072, i.e. -22.8 dB across the tail.
        self.ref_env_level = x_rms.max(self.ref_env_level * 0.72);

        // ---- near-end (double-talk) verdict ---------------------------------------------
        //
        // **This is the part that was wrong the first time, and the way it was wrong matters.**
        //
        // The first version tracked an ABSOLUTE floor of the residual and called anything
        // materially above it near-end speech. That cannot work, because residual echo scales
        // with how loud Rich is. Minimum statistics learn the floor during his quiet passages;
        // during his loud ones the residual is legitimately many dB higher and reads as a
        // person who is not there. Measured on the rig: **415 false positives in 750 blocks** —
        // and because a near-end verdict freezes adaptation, the filter then spent 55 % of its
        // life unable to learn, which is why ERLE stalled at 8.8 dB.
        //
        // The fix is to make the estimate SCALE-INVARIANT. What is stable about an echo path
        // is not the residual level, it is the residual level *relative to the reference*:
        //
        //     leak_gain = residual_rms / reference_envelope
        //
        // That is a property of the room and the filter, not of how loud Rich happens to be
        // talking this second. Minimum-statistics tracking of THAT is well posed, and the
        // predicted residual echo for any block is `leak_gain * reference_envelope`.
        let far_env = self.ref_env_level > FAR_END_ACTIVE_RMS;
        let predicted_echo = self.leak_gain * self.ref_env_level;
        let speech_floor = crate::vad::VadConfig::default().absolute_floor;
        if far_env {
            self.near_end = e_rms > NEAR_END_MARGIN * predicted_echo && e_rms > speech_floor;
            // The SEPARATE, more sensitive test that stops the filter learning. See
            // `ADAPT_FREEZE_MARGIN`: a suspicion is enough to stop adapting, where interrupting
            // Rich needs proof. Gated on the filter having something worth protecting — see
            // `ADAPT_PROTECT_ERLE_DB` for the deadlock this avoids.
            let suspicion = e_rms > ADAPT_FREEZE_MARGIN * predicted_echo && e_rms > speech_floor;
            // The hold-over is only armed once there is a converged filter to protect — see
            // `ADAPT_PROTECT_ERLE_DB`. Until then a suspicion stops adaptation for exactly the
            // block it occurred on, which is the behavior that converges.
            // LATCHED. Once the filter has proven itself worth protecting it stays protected
            // until it is reset, because `erle_db()` only accumulates on blocks where the
            // filter is NOT frozen — so an un-latched gate disarms itself exactly when
            // double-talk starts, which is the one moment it is needed. Measured: whisper's
            // word error rate on cancelled double-talk was 2.4 points worse than clean with
            // the gate un-latched, and 0.0 points worse with it latched.
            if self.erle_db() > ADAPT_PROTECT_ERLE_DB {
                self.protect_latched = true;
            }
            let protect = self.protect_latched;
            self.freeze_hangover = if suspicion {
                if protect { ADAPT_FREEZE_HANGOVER_BLOCKS } else { 1 }
            } else {
                self.freeze_hangover.saturating_sub(1)
            };
        } else {
            self.near_end = false;
            self.near_end_run = 0;
            self.freeze_hangover = 0;
        }
        // A LEAKY counter, not an unbroken run: up on a near-end verdict, down on anything
        // else, floored at zero.
        //
        // An unbroken run was tried first and never fired, because real signals flicker: with
        // the echo path changed to 4x louder, 246 of 313 blocks read as near-end but the
        // longest unbroken stretch never reached 125, so the escape hatch stayed shut and the
        // detector called echo "the CEO" indefinitely. The leaky form separates the two cases
        // cleanly: 246 up and 67 down nets +179 and escapes, while a person genuinely talking
        // over Rich for 2 s at a normal 60 % duty cycle nets only +25 and does not.
        if self.near_end {
            self.near_end_run += 1;
        } else {
            self.near_end_run = self.near_end_run.saturating_sub(1);
        }

        // A "near-end" verdict that never ends is a stale estimate, not a monologue. Same
        // escape, and the same reasoning, as `vad::VadConfig::stuck_speech_frames`.
        if self.near_end_run > LEAK_STUCK_BLOCKS {
            self.leak_gain = (e_rms / self.ref_env_level.max(1e-7)).clamp(1e-6, 4.0);
            self.leak_floor_rms = e_rms;
            self.near_end_run = 0;
            self.near_end = false;
        }

        // ---- leak gain: minimum statistics ------------------------------------------------
        // Fast down, very slow up. Convergence is tracked immediately; near-end speech barely
        // moves it, which is what stops the detector from de-sensitizing itself against the
        // very voice it exists to detect.
        if far_env {
            self.far_end_blocks += 1;
            let observed = e_rms / self.ref_env_level.max(1e-7);
            let rate = if observed < self.leak_gain { 0.2 } else { 0.0005 };
            self.leak_gain += (observed - self.leak_gain) * rate;
            self.leak_gain = self.leak_gain.clamp(1e-6, 4.0);
            // The absolute residual-echo level this implies, which is what `confident()`
            // compares against the VAD's speech threshold. Asymmetric for the same reason as
            // everything else here: converging DOWN is news and is tracked immediately;
            // drifting UP might be near-end speech and is treated with suspicion.
            //
            // The rates matter to a figure the CEO feels. A symmetric 0.02 was tried first and
            // meant the estimate needed ln(0.0025)/ln(0.98) = 296 blocks = 4.7 s just to decay
            // from its pessimistic initial value of 1.0 — on top of the deliberate 2.000 s
            // warm-up and 2.000 s hold. Time-to-confident was 7.14 s. At 0.2 down the decay is
            // 27 blocks = 0.43 s and time-to-confident is dominated by the warm-up and hold,
            // which are deliberate.
            let floor = self.leak_gain * self.ref_env_level;
            let rate = if floor < self.leak_floor_rms { 0.2 } else { 0.02 };
            self.leak_floor_rms += (floor - self.leak_floor_rms) * rate;

            // The TYPICAL residual, over blocks where we do not suspect the CEO is talking.
            // Symmetric and slow (0.02 = a ~0.8 s time constant), so it is a genuine average
            // rather than a floor, and an occasional missed near-end block barely moves it.
            //
            // **It is a REPORTED number and no longer a verdict.** Until 2026-09-17
            // `confidence_condition` compared this estimate against `CONFIDENT_LEAK_RMS`, and
            // on a path with no cancellation the `!self.near_end` filter on this very line is
            // what made that unsafe: uncancelled echo trips the near-end detector, so the loud
            // blocks — the only ones the threshold is about — are the ones excluded from the
            // average. See `confidence_condition` for the measurement that replaced it.
            if !self.near_end {
                if self.residual_seeded {
                    self.residual_typ_rms += (e_rms - self.residual_typ_rms) * 0.02;
                } else {
                    // SEEDED FROM THE FIRST REAL OBSERVATION, not from an arbitrary
                    // pessimistic 1.0. Starting at full scale is not "safe", it is just slow:
                    // it costs ln(0.0025)/ln(0.98) = 296 blocks = 4.74 s of decay before the
                    // estimate says anything about this room, and every one of those seconds
                    // is a second the CEO cannot interrupt Rich in under five. Time-to-
                    // confident measured 7.92 s with the sentinel and 3.97 s seeded — and the
                    // seeded value is the TRUE residual, which is what the threshold is about.
                    self.residual_typ_rms = e_rms;
                    self.residual_seeded = true;
                }
            }
        }

        // **THE HOLD IS COUNTED IN EVIDENCE, NOT IN WALL CLOCK.**
        //
        // This used to run on every block, and `CONFIDENCE_HOLD_BLOCKS` therefore meant 2.000 s
        // of elapsed time rather than 2.000 s of anything being tested. `residual_typ_rms` is
        // only updated while the reference is active, so during a silence it is FROZEN at its
        // last value — and a frozen number that happens to sit under the threshold satisfies
        // `confidence_condition()` 125 times in a row without a single new observation.
        //
        // Rich is silent for a long time in every real session: between sentences, while the
        // next one synthesizes, and for the whole of the CEO's turn. The gap at the end of a
        // sentence is the worst case, because the residual is decaying with the echo tail as
        // the reference falls away, so the value that gets frozen is close to the room noise
        // floor rather than close to the echo.
        //
        // **This was measured before it was written, and the first attempt at a fix made it
        // worse, which is what identified the mechanism.** On the CEO's hardware, five runs of
        // `examples/aec_live --secs 30` each (Mac mini Speakers, Elgato Wave:3; traces in
        // `docs/verification/2026-09-17-aec-erle-on-the-ceo-rig.md`):
        //
        // ```text
        //   shipped                                         2 of 5 runs CONFIDENT at ~5.1 s
        //   + residual measured only while far-active        4 of 5 runs CONFIDENT at ~5.1 s
        //   + this: the hold counted only while far-active   see the doc's table
        // ```
        //
        // Tightening WHERE the residual is measured made the freeze happen more often, not
        // less. Both changes are needed and neither is sufficient: one decides what the number
        // means, this one decides when it is allowed to count toward a verdict.
        //
        // Silence neither helps nor hurts — the run is HELD, not reset. Resetting would demand
        // 2.000 s of unbroken loud speech, which natural speech does not contain, and would
        // make the short barge-in window unreachable on headphones too. Holding means the
        // canceller earns confidence from 2.000 s of blocks in which its claim was actually
        // under test.
        if far_active {
            if self.confidence_condition(e_rms) {
                self.confident_run = self.confident_run.saturating_add(1);
                if self.confident_run >= CONFIDENCE_HOLD_BLOCKS {
                    self.confident_latched = true;
                }
            } else {
                self.confident_run = 0;
            }
        }

        self.last = BlockStats {
            reference_rms: x_rms,
            reference_env: self.ref_env_level,
            mic_rms: d_rms,
            echo_estimate_rms: (y_pow * inv).sqrt(),
            residual_rms: e_rms,
            predicted_echo_rms: predicted_echo,
            far_active,
            near_end: self.near_end,
            // Recomputed rather than referenced: the adapt gate runs after this point.
            adapted: far_active && self.freeze_hangover == 0,
        };

        // ---- ERLE, measured only where it means something -------------------------------
        if far_active && self.freeze_hangover == 0 {
            let a = 0.99f32;
            self.d_power_smooth = a * self.d_power_smooth + (1.0 - a) * d_pow;
            self.e_power_smooth = a * self.e_power_smooth + (1.0 - a) * e_pow;
        }

        // ---- divergence guard ------------------------------------------------------------
        // If the "canceller" is adding energy rather than removing it, its state describes a
        // path that no longer exists and the only cure is to forget it.
        //
        // But it must be a SUSTAINED signal, not a single bad block. A converging filter
        // overshoots on transients — a plosive, the start of a word — and an instantaneous
        // guard fires on those, wipes the filter, and pins ERLE at the level a filter reaches
        // in the handful of blocks between resets. Measured before this debounce existed:
        // 38 resets in 20 s and ERLE stuck at 4.0 dB.
        //
        // Same discipline as every other decision in this crate: a frame count, re-derived.
        // 12 blocks x 256 / 16 000 = 0.192 s of continuous divergence.
        if far_active && e_pow > 4.0 * d_pow && y_pow > d_pow {
            self.diverging_run += 1;
        } else {
            self.diverging_run = 0;
        }
        if self.diverging_run >= DIVERGENCE_BLOCKS {
            self.diverging_run = 0;
            self.divergence_resets += 1;
            self.reset_filter();
        }

        // ---- adapt ------------------------------------------------------------------------
        // NEVER while near-end speech is even SUSPECTED, and not for 0.400 s afterwards.
        // Adapting on the CEO's voice makes the filter model HIM, and it then subtracts part of
        // him from himself — which does not sound like a bug, it sounds like a transcription
        // error. That is the worst kind, because nothing announces it.
        let adapting = far_active && self.freeze_hangover == 0;
        if adapting {
            self.adapt(&mic[..AEC_BLOCK]);
        }

        self.cap_abs += AEC_BLOCK as u64;
        self.blocks += 1;
        self.near_end
    }

    /// One normalized frequency-domain LMS update.
    fn adapt(&mut self, residual: &[f32]) {
        // E = FFT([0 ... 0, e]) — the zero-padded first half is the overlap-save gradient
        // constraint on the ERROR side, and it has to be exactly zero (see the fft tests).
        for (i, slot) in self.x_time.iter_mut().enumerate() {
            *slot = if i < AEC_BLOCK { 0.0 } else { residual[i - AEC_BLOCK] };
        }
        {
            let fft = &self.fft;
            let dst = &mut self.scratch_b;
            fft.load_real(&self.x_time, dst);
            fft.forward(dst);
        }

        // ---- the NLMS normaliser, and the stability proof it has to satisfy ----------
        //
        // The update is `W_p += step * conj(X_p) * E` for every partition, so the change it
        // makes to the echo estimate is
        //
        //     dY = sum_p dW_p . X_p = step * E * sum_p |X_p|^2 = step * E * Px
        //
        // Normalizing by `Px = sum_p |X_p|^2` therefore gives `dY = mu * E` exactly, which is
        // stable for `0 < mu < 2`. Normalizing by anything SMALLER overshoots by the ratio.
        //
        // That is the bug this `max` exists to prevent. A plain first-order smoother starts at
        // zero, so on the first adapting block `smoothed = 0.1 * Px` and the effective step is
        // `10 * mu = 3.0` — an overshoot of 3x per block, which diverges immediately (measured:
        // 147 divergence resets and -9.5 dB "ERLE" in a 20 s run before this line existed).
        //
        // Taking the max of the smoothed and the instantaneous power guarantees the
        // denominator is never below `Px`, so the effective step is never above `mu`, while the
        // smoothed term still supplies the memory that stops a momentary dip in reference
        // level from producing a huge step.
        let eps = 1e-6f32;
        for k in 0..AEC_FFT {
            let mut px = 0.0f32;
            for x in self.x_spectra.iter() {
                px += x[k].norm_sq();
            }
            self.bin_power[k] = (0.9 * self.bin_power[k] + 0.1 * px).max(px);
        }

        for p in 0..AEC_PARTITIONS {
            for k in 0..AEC_FFT {
                let step = AEC_STEP_SIZE / (self.bin_power[k] + eps);
                // conj(X) * E — correlate the reference with what is left over.
                let g = self.scratch_b[k].mul_conj(self.x_spectra[p][k]).scale(step);
                self.weights[p][k] = self.weights[p][k].add(g);
            }
        }

        // ---- the gradient constraint, on EVERY partition, every block --------------------
        //
        // Each partition's impulse response must be zero outside its 256-sample support. If
        // it is not, the frequency-domain product stops being a LINEAR convolution and starts
        // being a circular one: taps beyond 256 wrap round and multiply samples from the far
        // end of the window, injecting garbage into the very output the next gradient is
        // computed from.
        //
        // The usual economy is to constrain one partition per block, round-robin. It was
        // tried here first and it is not good enough at this filter length: with 8 partitions
        // a given one is unconstrained for 7 blocks out of 8, and the wraparound accumulated
        // in that time pinned steady-state ERLE at **8.9 dB** on the rig's 5-tap synthetic
        // path — a path a 2048-tap filter should annihilate.
        //
        // Constraining all 8 costs 16 extra 512-point transforms per block. That is affordable
        // and it was MEASURED rather than assumed before being accepted; see the rig's COST
        // section and `examples/aec_rig.rs`.
        for p in 0..AEC_PARTITIONS {
            self.scratch_a.copy_from_slice(&self.weights[p]);
            self.fft.inverse(&mut self.scratch_a);
            for s in self.scratch_a.iter_mut().skip(AEC_BLOCK) {
                *s = C::ZERO;
            }
            for s in self.scratch_a.iter_mut().take(AEC_BLOCK) {
                s.im = 0.0;
            }
            self.fft.forward(&mut self.scratch_a);
            self.weights[p].copy_from_slice(&self.scratch_a);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// INVARIANT: the block IS the VAD frame, so the canceller adds no buffering and every
    /// frame-count debounce in the crate keeps meaning what it says.
    #[test]
    fn one_aec_block_is_one_vad_frame_and_the_tail_is_128_milliseconds() {
        assert_eq!(AEC_BLOCK, VAD_FRAME_SAMPLES);
        assert_eq!(AEC_BLOCK, 256);
        assert_eq!(AEC_FFT, 512);
        assert_eq!(AEC_TAPS, 2048);
        // 2048 / 16000 = 0.128 s exactly.
        assert!((filter_tail_secs() - 0.128).abs() < 1e-9, "{}", filter_tail_secs());
        // 125 blocks x 256 / 16000 = 2.000 s exactly.
        assert!((blocks_to_secs(CONFIDENCE_WARMUP_BLOCKS) - 2.0).abs() < 1e-9);
        assert!((blocks_to_secs(LEAK_STUCK_BLOCKS) - 2.0).abs() < 1e-9);
        // 32 blocks x 256 / 16000 = 0.512 s of delay search.
        assert!((blocks_to_secs(MAX_DELAY_BLOCKS as u32) - 0.512).abs() < 1e-9);
    }

    /// INVARIANT: the confidence threshold really is 6 dB below the VAD's speech floor —
    /// the whole justification for shortening the debounce, re-derived rather than quoted.
    #[test]
    fn the_confidence_threshold_is_six_db_under_the_vads_speech_floor() {
        let floor = crate::vad::VadConfig::default().absolute_floor;
        assert_eq!(floor, 0.005);
        assert!((CONFIDENT_LEAK_RMS - floor / 2.0).abs() < 1e-9);
        let db = 20.0 * (CONFIDENT_LEAK_RMS / floor).log10();
        assert!((db + 6.0206).abs() < 1e-3, "margin is {db} dB, not -6.02 dB");
    }

    /// INVARIANT: the ring carries every sample, in order, across an arbitrary split between
    /// producer writes and consumer reads.
    #[test]
    fn the_reference_ring_carries_every_sample_in_order() {
        let ring = ReferenceRing::new(1024);
        let mut out = Vec::new();
        let mut expected = Vec::new();
        let mut n = 0.0f32;
        for push in [1usize, 100, 7, 300, 55] {
            let block: Vec<f32> = (0..push)
                .map(|_| {
                    n += 1.0;
                    n
                })
                .collect();
            expected.extend_from_slice(&block);
            ring.push(&block);
            assert_eq!(ring.drain(&mut out), 0, "unexpected overrun");
        }
        assert_eq!(out, expected);
        assert_eq!(ring.overrun_samples(), 0);
    }

    /// INVARIANT: an overrun is DETECTED and counted, never silently absorbed. Silent loss
    /// would shift the reference against the echo forever and the filter would describe a
    /// path that never existed.
    #[test]
    fn a_ring_overrun_is_reported_rather_than_hidden() {
        let ring = ReferenceRing::new(16);
        ring.push(&(0..40).map(|i| i as f32).collect::<Vec<_>>());
        let mut out = Vec::new();
        let lost = ring.drain(&mut out);
        assert_eq!(lost, 24, "40 pushed into 16 slots should lose the oldest 24");
        assert_eq!(out.len(), 16);
        assert_eq!(out[0], 24.0, "the survivor window is the NEWEST samples");
        assert_eq!(ring.overrun_samples(), 24);
    }

    /// INVARIANT: draining an empty ring yields nothing and reports no loss.
    #[test]
    fn an_empty_ring_drains_to_nothing() {
        let ring = ReferenceRing::new(64);
        let mut out = vec![9.0f32];
        assert_eq!(ring.drain(&mut out), 0);
        assert_eq!(out, vec![9.0]);
    }

    /// INVARIANT: the sink converts the device rate to 16 kHz with a continuous phase, so
    /// 1 s of 48 kHz output becomes 16 000 reference samples, not 16 031.
    #[test]
    fn the_reference_sink_delivers_sixteen_thousand_samples_per_second() {
        let ring = Arc::new(ReferenceRing::new(REF_RING_SAMPLES));
        let mut sink = ReferenceSink::new(ring.clone(), 48_000);
        let block = vec![0.1f32; 512];
        // 48000 / 512 = 93.75 callbacks per second; run 93.75 x 2 = 187.5 -> 187 callbacks.
        for _ in 0..187 {
            sink.submit(&block);
        }
        let written = ring.written() as f64;
        let expected = 187.0 * 512.0 / 3.0; // 31 914.67
        assert!(
            (written - expected).abs() <= 2.0,
            "sink drifted: {written} vs {expected}"
        );
    }

    /// A synthetic echo path: a delay, four reflections, and a room-noise floor.
    ///
    /// The reflections are there because a filter that merely learned "the mic is the
    /// reference times k" would pass a single-tap test and fail in a room. The NOISE is there
    /// for the opposite reason: without it the synthetic path is perfectly modellable and
    /// ERLE runs away to 80+ dB, which is not a number any room will ever produce and would
    /// make these tests assert a fantasy. -54 dBFS is a quiet office with a fan — under the
    /// VAD's -46 dBFS speech floor, and enough to cap ERLE somewhere believable.
    fn echo_path(reference: &[f32], delay: usize) -> Vec<f32> {
        let taps: [(usize, f32); 5] =
            [(0, 0.50), (37, -0.22), (101, 0.13), (238, -0.07), (601, 0.04)];
        let mut out = vec![0.0f32; reference.len()];
        for (offset, gain) in taps {
            for i in 0..reference.len() {
                let src = i as i64 - delay as i64 - offset as i64;
                if src >= 0 {
                    out[i] += reference[src as usize] * gain;
                }
            }
        }
        let mut s: u32 = 0x5EED_1234;
        for v in out.iter_mut() {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            *v += ((s as f32 / u32::MAX as f32) * 2.0 - 1.0) * 0.002;
        }
        out
    }

    fn speechish(n: usize, seed: u32) -> Vec<f32> {
        // Noise shaped by a slow envelope and a couple of formant-ish resonances: broadband
        // and non-stationary, which is what an adaptive filter finds hard.
        let mut s = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
        let mut rnd = || {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            (s as f32 / u32::MAX as f32) * 2.0 - 1.0
        };
        let mut y1 = 0.0f32;
        let mut y2 = 0.0f32;
        (0..n)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                let env = (0.5 + 0.5 * (2.0 * std::f32::consts::PI * 2.7 * t).sin()).powf(1.5);
                let x = rnd();
                y1 = 0.92 * y1 + 0.08 * x;
                y2 = 0.55 * y2 + 0.45 * (x - y1);
                (y1 * 1.6 + y2 * 0.5) * env * 0.35
            })
            .collect()
    }

    /// Run reference+mic through the canceller in 256-sample blocks, returning the residual,
    /// the near-end verdict per block, and the block at which the canceller first declared
    /// itself confident. That last one matters: before it, the 5.008 s consecutive debounce is
    /// still in force, so a near-end false positive there costs nothing.
    fn run(
        aec: &mut EchoCanceller,
        ring: &ReferenceRing,
        reference: &[f32],
        mic: &[f32],
    ) -> (Vec<f32>, Vec<bool>, Option<usize>) {
        let blocks = reference.len().min(mic.len()) / AEC_BLOCK;
        let mut residual = Vec::with_capacity(blocks * AEC_BLOCK);
        let mut near = Vec::with_capacity(blocks);
        let mut confident_at = None;
        for b in 0..blocks {
            let r = &reference[b * AEC_BLOCK..(b + 1) * AEC_BLOCK];
            ring.push(r);
            let mut frame = [0.0f32; AEC_BLOCK];
            frame.copy_from_slice(&mic[b * AEC_BLOCK..(b + 1) * AEC_BLOCK]);
            near.push(aec.process_block(&mut frame));
            residual.extend_from_slice(&frame);
            if confident_at.is_none() && aec.confident() {
                confident_at = Some(b);
            }
        }
        (residual, near, confident_at)
    }

    fn power(x: &[f32]) -> f32 {
        x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32
    }

    fn erle_db(mic: &[f32], residual: &[f32]) -> f32 {
        10.0 * (power(mic) / power(residual).max(1e-20)).log10()
    }

    /// **THE HEADLINE MEASUREMENT, offline and reproducible.** Rich's voice through a
    /// synthetic room, no near-end speaker: how much of it survives?
    ///
    /// The figure is computed over the SECOND HALF only, because the first half includes
    /// convergence and averaging it in would flatter the steady state.
    #[test]
    fn the_canceller_removes_the_echo_it_was_built_to_remove() {
        let (mut aec, ring) = EchoCanceller::new();
        let n = SAMPLE_RATE as usize * 20; // 20 s
        let reference = speechish(n, 11);
        let mic = echo_path(&reference, 800); // 800 / 16000 = 50.0 ms of bulk delay
        let (residual, near, confident_at) = run(&mut aec, &ring, &reference, &mic);

        let half = residual.len() / 2;
        let steady = erle_db(&mic[half..], &residual[half..]);
        assert!(steady > 20.0, "steady-state ERLE only {steady:.1} dB");

        // The canceller's own smoothed figure must agree with the one measured externally
        // from the two signals — otherwise the number it reports to the UI is decorative.
        let reported = aec.erle_db();
        assert!(
            (reported - steady).abs() < 8.0,
            "reported ERLE {reported:.1} dB disagrees with measured {steady:.1} dB"
        );

        // **THE FALSE-POSITIVE PROPERTY, stated exactly.** With nobody else in the room there
        // is no near-end speech, ever — but the promise is not "never wrong", it is "never
        // wrong once it says it can be believed".
        //
        // Before `confident()`, the barge-in monitor is still running the 313-frame /
        // 5.008 s consecutive rule, so a scattered near-end verdict there costs nothing at
        // all: it cannot complete that debounce. AFTER `confident()` the monitor switches to
        // the 15-of-25 window, and a false positive genuinely is Rich interrupting himself.
        // So that is where the assertion belongs.
        let at = confident_at.expect("should reach confidence");
        // Measured 6.22 s here. The floor is deliberate and structural: 2.000 s of
        // `CONFIDENCE_WARMUP_BLOCKS` so the filter has seen enough of the echo path to have an
        // opinion, plus 2.000 s of `CONFIDENCE_HOLD_BLOCKS` so a transient dip in residual
        // cannot be mistaken for convergence. The remainder is the filter actually converging.
        //
        // This is measured in blocks where RICH IS AUDIBLE, and it is paid ONCE per voice
        // session, not once per turn — the canceller lives as long as voice mode is on. Until
        // it elapses the 5.008 s rule and "tap to stop" are in force, exactly as before.
        assert!(
            blocks_to_secs(at as u32) < 8.0,
            "took {:.2} s to become confident",
            blocks_to_secs(at as u32)
        );
        assert!(
            blocks_to_secs(at as u32)
                >= blocks_to_secs(CONFIDENCE_WARMUP_BLOCKS + CONFIDENCE_HOLD_BLOCKS),
            "confidence arrived before the warm-up and hold could possibly have elapsed"
        );
        let after = near[at..].iter().filter(|n| **n).count();
        assert_eq!(after, 0, "heard a person who was not there, {after} times, AFTER claiming confidence");

        assert!(aec.confident(), "should be confident: {}", aec.metrics().summary());
    }

    /// INVARIANT: the bulk delay is found, not assumed. 800 samples = 50.0 ms; the estimator
    /// works in whole blocks and deliberately backs off one, so 800/256 = 3.125 -> lag 3 ->
    /// reported 2. What matters is that the true delay lands inside the filter's 128 ms tail.
    #[test]
    fn the_bulk_delay_is_measured_and_brackets_the_true_delay() {
        let (mut aec, ring) = EchoCanceller::new();
        let n = SAMPLE_RATE as usize * 12;
        let reference = speechish(n, 5);
        let mic = echo_path(&reference, 800);
        run(&mut aec, &ring, &reference, &mic);

        let m = aec.metrics();
        assert!(m.delay_confidence > 0.3, "no confident delay: {}", m.summary());
        let est = (m.delay_blocks * AEC_BLOCK) as i64;
        assert!(est <= 800, "estimate {est} overshot the true 800-sample delay");
        assert!(800 - est < AEC_TAPS as i64, "true delay is outside the filter tail");
    }

    /// **THE TRANSPARENCY GUARANTEE.** After one full filter tail of silence from Rich, the
    /// microphone signal is passed through BIT-IDENTICAL — not "almost", not "within 1e-6".
    /// This is what makes it arithmetically impossible for the canceller to harm dictation or
    /// call transcription, both of which happen while Rich is not speaking.
    #[test]
    fn silence_from_rich_leaves_the_microphone_bit_identical() {
        let (mut aec, ring) = EchoCanceller::new();
        // First let it converge on a real echo, so the weights are emphatically non-zero.
        let n = SAMPLE_RATE as usize * 8;
        let reference = speechish(n, 3);
        let mic = echo_path(&reference, 800);
        run(&mut aec, &ring, &reference, &mic);
        assert!(
            aec.weights.iter().any(|w| w.iter().any(|c| c.norm_sq() > 1e-12)),
            "premise: the filter must have learned something"
        );

        // Now Rich falls silent. Feed a full tail of silent reference to flush the partitions.
        let flush = AEC_PARTITIONS + 2;
        for _ in 0..flush {
            ring.push(&[0.0f32; AEC_BLOCK]);
            let mut frame = [0.0f32; AEC_BLOCK];
            aec.process_block(&mut frame);
        }

        // Every subsequent frame must come out EXACTLY as it went in.
        let voice = speechish(AEC_BLOCK * 200, 77);
        for b in 0..200 {
            let src = &voice[b * AEC_BLOCK..(b + 1) * AEC_BLOCK];
            ring.push(&[0.0f32; AEC_BLOCK]);
            let mut frame = [0.0f32; AEC_BLOCK];
            frame.copy_from_slice(src);
            aec.process_block(&mut frame);
            for (i, (got, want)) in frame.iter().zip(src.iter()).enumerate() {
                assert_eq!(
                    got.to_bits(),
                    want.to_bits(),
                    "block {b} sample {i}: the canceller altered audio while Rich was silent"
                );
            }
        }
    }

    /// **THE ONE THAT DECIDES THE PRODUCT.** Rich is talking, his echo is in the mic, and the
    /// CEO says something short. The canceller must call it near-end speech quickly.
    ///
    /// The interruption here is 400 ms — the value `bargein.rs` adopts for the converged
    /// debounce. It must be detected in the great majority of its blocks, and there must be
    /// no near-end verdict at all in the seconds of echo-only that precede it.
    #[test]
    fn a_short_interruption_is_detected_while_echo_only_never_is() {
        let (mut aec, ring) = EchoCanceller::new();
        let n = SAMPLE_RATE as usize * 20;
        let reference = speechish(n, 9);
        let mut mic = echo_path(&reference, 800);

        // The CEO speaks for 400 ms starting at 15.0 s, at a normal conversational level.
        let start = SAMPLE_RATE as usize * 15;
        let dur = (SAMPLE_RATE as f32 * 0.400) as usize;
        assert_eq!(dur, 6400, "400 ms at 16 kHz");
        let ceo = speechish(dur, 42);
        let ceo_rms = (power(&ceo)).sqrt();
        for i in 0..dur {
            mic[start + i] += ceo[i];
        }

        let (_res, near, confident_at) = run(&mut aec, &ring, &reference, &mic);
        let confident_at = confident_at.expect("should reach confidence well before 15 s");

        let b0 = start / AEC_BLOCK;
        let b1 = (start + dur) / AEC_BLOCK;
        let during = near[b0..b1].iter().filter(|n| **n).count();
        let window = b1 - b0;
        assert_eq!(window, 25, "400 ms = 25 blocks of 16.000 ms");
        assert!(
            during * 100 / window >= 60,
            "only {during}/{window} blocks of a 400 ms interruption were detected"
        );

        // And NOT ONE false positive between the moment it claimed confidence and the moment
        // the CEO actually spoke.
        let quiet_from = confident_at;
        assert!(quiet_from < b0, "confidence arrived after the interruption");
        let false_pos = near[quiet_from..b0].iter().filter(|n| **n).count();
        assert_eq!(
            false_pos, 0,
            "echo alone was mistaken for the CEO {false_pos} times — Rich would interrupt himself"
        );
        assert!(ceo_rms > 0.01, "premise: the CEO is audible at all ({ceo_rms})");
    }

    /// INVARIANT: the filter does NOT adapt on the CEO's voice. Adapting during double-talk
    /// makes the filter model the near-end speaker, which both destroys cancellation and
    /// starts subtracting the CEO from himself. Measured as: ERLE after a long double-talk
    /// passage is no worse than before it.
    #[test]
    fn double_talk_does_not_wreck_the_converged_filter() {
        let (mut aec, ring) = EchoCanceller::new();
        let reference = speechish(SAMPLE_RATE as usize * 12, 21);
        let mic = echo_path(&reference, 800);
        run(&mut aec, &ring, &reference, &mic);
        let before = aec.erle_db();
        assert!(before > 15.0, "premise: converged first ({before:.1} dB)");

        // 4 s of the CEO talking straight over Rich.
        let dt_ref = speechish(SAMPLE_RATE as usize * 4, 22);
        let mut dt_mic = echo_path(&dt_ref, 800);
        let ceo = speechish(dt_mic.len(), 23);
        for i in 0..dt_mic.len() {
            dt_mic[i] += ceo[i] * 1.5;
        }
        run(&mut aec, &ring, &dt_ref, &dt_mic);

        // Then echo-only again: has the filter survived?
        let after_ref = speechish(SAMPLE_RATE as usize * 6, 24);
        let after_mic = echo_path(&after_ref, 800);
        let (res, _, _) = run(&mut aec, &ring, &after_ref, &after_mic);
        let half = res.len() / 2;
        let after = erle_db(&after_mic[half..], &res[half..]);
        assert!(
            after > before - 6.0,
            "double-talk cost {:.1} dB of ERLE ({before:.1} -> {after:.1})",
            before - after
        );
    }

    /// INVARIANT: confidence is EARNED. A canceller that has seen nothing must never claim
    /// the residual echo is quiet — that claim is what shortens the barge-in debounce.
    #[test]
    fn a_cold_canceller_is_never_confident() {
        let (aec, _ring) = EchoCanceller::new();
        assert!(!aec.confident());
        assert_eq!(aec.metrics().erle_db, 0.0);
        assert!(aec.metrics().leak_floor_rms >= CONFIDENT_LEAK_RMS);

        // Nor after silence alone: silence teaches the filter nothing about the echo path.
        let (mut aec, ring) = EchoCanceller::new();
        for _ in 0..1000 {
            ring.push(&[0.0f32; AEC_BLOCK]);
            let mut f = [0.0f32; AEC_BLOCK];
            aec.process_block(&mut f);
        }
        assert!(!aec.confident(), "silence must not buy confidence");
        assert_eq!(aec.metrics().far_end_blocks, 0);
    }

    /// INVARIANT: headphones. The reference is loud but nothing comes back into the mic, so
    /// there is nothing to cancel — and the canceller must be confident anyway, because
    /// "no echo reaches the mic" is the strongest possible version of "residual echo cannot
    /// trigger a barge-in". This is the case where the 5 s window was pure cost.
    #[test]
    fn headphones_leave_nothing_to_cancel_and_the_canceller_says_so() {
        let (mut aec, ring) = EchoCanceller::new();
        let reference = speechish(SAMPLE_RATE as usize * 6, 31);
        // Mic hears only quiet room tone: -66 dBFS, well under the VAD's -46 dBFS floor.
        let room: Vec<f32> = speechish(reference.len(), 32).iter().map(|s| s * 0.0015).collect();
        let (_res, near, _) = run(&mut aec, &ring, &reference, &room);
        assert!(aec.confident(), "headphones case: {}", aec.metrics().summary());
        assert_eq!(near.iter().filter(|n| **n).count(), 0, "room tone read as the CEO");
    }

    /// INVARIANT: a reference that stops mid-conversation does not leave the near-end
    /// detector latched on. With no far end there is no echo, so there is nothing to
    /// discriminate and the verdict is false by construction — the ordinary VAD path owns
    /// speech while Rich is silent, exactly as it did before this module existed.
    #[test]
    fn with_rich_silent_the_near_end_verdict_is_false_by_construction() {
        let (mut aec, ring) = EchoCanceller::new();
        let loud = speechish(AEC_BLOCK * 300, 51);
        for b in 0..300 {
            ring.push(&[0.0f32; AEC_BLOCK]);
            let mut f = [0.0f32; AEC_BLOCK];
            f.copy_from_slice(&loud[b * AEC_BLOCK..(b + 1) * AEC_BLOCK]);
            assert!(!aec.process_block(&mut f), "near-end fired with no far end at block {b}");
        }
    }

    /// INVARIANT: an echo path that CHANGES (the CEO turns the volume up, or moves the mic)
    /// is re-learned rather than latching the near-end detector on forever. Without the
    /// `LEAK_STUCK_BLOCKS` escape the stale-low leak floor would call the new, louder echo
    /// "the CEO" indefinitely — and every one of those is Rich interrupting himself.
    #[test]
    fn a_changed_echo_path_is_relearned_rather_than_latching_the_detector() {
        let (mut aec, ring) = EchoCanceller::new();
        let r1 = speechish(SAMPLE_RATE as usize * 12, 61);
        let m1 = echo_path(&r1, 800);
        run(&mut aec, &ring, &r1, &m1);
        assert!(aec.confident(), "premise: converged on the first path");

        // The path changes hard: four times louder, different delay.
        let r2 = speechish(SAMPLE_RATE as usize * 20, 62);
        let m2: Vec<f32> = echo_path(&r2, 1500).iter().map(|s| s * 4.0).collect();
        let (_res, near, _) = run(&mut aec, &ring, &r2, &m2);

        // It is allowed to be wrong at the moment of the change — it must not STAY wrong.
        let tail = near.len() * 3 / 4;
        let late = near[tail..].iter().filter(|n| **n).count();
        assert!(
            late * 100 / (near.len() - tail) < 20,
            "still calling echo 'the CEO' in {late}/{} of the final blocks",
            near.len() - tail
        );
    }

    /// INVARIANT: `process_block` does not allocate after warm-up. An allocation on the
    /// capture callback thread is a click. Checked structurally — the scratch buffers are
    /// sized at construction and the only growable containers are bounded by `ingest_reference`.
    #[test]
    fn the_hot_path_buffers_are_preallocated_and_bounded() {
        let (mut aec, ring) = EchoCanceller::new();
        let r = speechish(SAMPLE_RATE as usize * 6, 71);
        let m = echo_path(&r, 800);
        run(&mut aec, &ring, &r, &m);

        assert_eq!(aec.scratch_a.len(), AEC_FFT);
        assert_eq!(aec.scratch_b.len(), AEC_FFT);
        assert_eq!(aec.x_time.len(), AEC_FFT);
        assert_eq!(aec.y_block.len(), AEC_BLOCK);
        assert_eq!(aec.weights.len(), AEC_PARTITIONS);
        assert!(aec.weights.iter().all(|w| w.len() == AEC_FFT));
        // The reference delay line is BOUNDED — a playout thread running fast can never grow
        // it without limit.
        let keep = MAX_DELAY_BLOCKS * AEC_BLOCK + AEC_TAPS + 4 * AEC_BLOCK;
        assert!(aec.ref_line.len() <= keep, "delay line grew to {}", aec.ref_line.len());
        assert!(aec.ref_env.len() <= DELAY_HISTORY_BLOCKS);
        assert!(aec.cap_env.len() <= DELAY_HISTORY_BLOCKS);
    }
}

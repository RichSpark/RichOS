//! **RICH'S OWN ANSWER MUST NEVER BECOME THE CEO'S MESSAGE.** The defect, replayed.
//!
//! On 2026-09-17 Ray walked candidate .5 on the CEO's own rig — Mac mini Speakers out, an
//! Elgato Wave:3 in, output volume 85 — and the ledger recorded this
//! (`docs/verification/2026-09-17-nightly-1.2.0-20260917.5-onscreen-audit-2.md` §2, Observation
//! C, verbatim timestamps):
//!
//! ```text
//!   20:06:35.672  PromptReceived  source=jam  "Please count slowly out loud from 1 to 20."
//!   20:06:39.432  AssistantDelta  seq 2   "One"
//!   20:06:40.179  AssistantDelta  seq 3   "... two... three... four... five"
//!   20:06:40.680  TurnCompleted   end_turn      <- the MODEL turn ends; the SPEAKING does not
//!   20:06:43.693  PromptReceived  source=jam  "1, 2, 3, 4, 5."    <- RICH, sent as the CEO
//!   20:06:50.525  AssistantDelta  seq 5   "Six... se"
//! ```
//!
//! Rich's own counting, heard back through the Wave:3, recognized, submitted as the CEO's
//! message and answered. A model turn spent, and words he never said sitting permanently in a
//! user bubble in his thread.
//!
//! ## What this file replays, and what it deliberately does not claim
//!
//! Rich's voice here is the CEO's REAL echo path: `tests/fixtures/echo-path/ceo-rig-2026-09-17`
//! is the microphone and the playout reference recorded on his desk that day, in the exact
//! coordinate system `EchoCanceller` uses. On that path the canceller measurably does not reach
//! confidence (`tests/echo_path_replay.rs`), so the taint rule is the one in force — which is
//! precisely the situation the walk was in.
//!
//! **Ray's audit says plainly what the record cannot settle:** app.log carried no timestamps, no
//! playout boundaries and no per-utterance taint flag, so "the utterance began in a gap between
//! spoken sentences" and "it began after playout had been marked ended" are indistinguishable
//! from the evidence. This file therefore does not pick one. It pins the two INDEPENDENT holes
//! that each admit Rich's voice on their own, and shows the shipped rule falling through each:
//!
//! | hole | the shipped rule | what closes it |
//! |---|---|---|
//! | taint decided ONCE, at `Started` | an utterance born in Rich's silence stays untainted however much of him lands in it | re-evaluating taint for as long as the utterance is alive |
//! | `speaking` was `queued_samples() > 0` | false in every synthesis gap and after the last sample leaves the queue | [`AudibleWindow`] — queued, OR owed, OR within the measured tail |
//!
//! And it keeps the positive controls the crate's negative tests always need: a genuine
//! utterance after the answer is still admitted, and a barge-in still rescues the CEO's words.

use richos_voice::aec::{EchoCanceller, ReferenceRing, AEC_BLOCK, FAR_END_ACTIVE_RMS};
use richos_voice::controller::{audible_hold_secs, AudibleWindow, CapMsg, CaptureBrain};
use richos_voice::endpoint::{
    UtteranceRecorder, PRE_ROLL_FRAMES, SILENCE_HANGOVER_FRAMES, SPEECH_ONSET_FRAMES,
};
use richos_voice::vad::{frames_to_secs, Vad, SAMPLE_RATE, VAD_FRAME_SAMPLES};
use richos_voice::wav;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

const FIXTURE: &str = "tests/fixtures/echo-path/ceo-rig-2026-09-17";

/// The CEO's rig, measured by `playout::tests::live_macos_output_device_drains_the_queue` on
/// 2026-09-17: output device latency 10.7 ms (512 frames / 48 000 Hz = 10.667 ms) and an input
/// latency of the same order. Used only to size the tail this file models; the shipping code
/// reads both off the running devices and never from a constant.
const MEASURED_OUT_TAIL_SECS: f32 = 0.0107;
const MEASURED_IN_LATENCY_SECS: f32 = 0.0107;

/// **WHERE RICH ACTUALLY IS INSIDE THE FIXTURE.** The recording is 9.536 s long and opens with
/// 1.3 s of room before anything is played: measured per 0.1 s, the reference sits at the
/// float floor until 1.3 s and the microphone at -56 to -63 dBFS, then Rich runs from 1.3 s to
/// about 9.0 s with the microphone at -40 to -51 dBFS. Slicing from 0 would hand these scenes
/// a second of silence and call it Rich, which would make every assertion below meaningless.
const RICH_STARTS_AT_SECS: f32 = 1.35;

/// **THE FIXTURE ALONE CANNOT BIRTH THIS DEFECT, AND THAT IS A FINDING, NOT A WORKAROUND.**
///
/// Replayed at the level it was recorded at, the microphone track sits at -40 to -51 dBFS and
/// the canceller takes another 6.3 dB off it over the far-active blocks
/// (`tests/echo_path_replay.rs`). The VAD's speech test is
/// `rms > max(noise_floor * 3.0, absolute_floor)` with `absolute_floor = 0.005` = -46.02 dBFS
/// (`vad.rs:86-91`, `vad.rs:139`), so the residual lands UNDER the floor and Rich's own voice
/// never produces a single speech frame. Driven unscaled, the scenes below simply end when the
/// question does — the defect does not appear, because the VAD never hears Rich at all.
///
/// The live walk plainly did hear him: the app printed two tainted discards during it, at
/// residual **-45.8 dBFS** and **-44.7 dBFS** (audit §2 Observation A), which is ABOVE the
/// floor. The walk ran at output volume 85; the fixture was recorded at whatever the CEO's
/// setting was that morning, and the two are not the same level.
///
/// So the mic track is scaled to put its POST-CANCELLATION residual where the running app
/// measured its own, and the factor is derived at run time from that measurement rather than
/// typed in — see [`mic_gain_for_the_walks_volume`]. The echo path, its coherence and its
/// cancellability are unchanged by a scalar; only the volume knob moves.
const LIVE_RESIDUAL_DBFS: f32 = -45.25; // the mean of the -45.8 and -44.7 the app printed

fn read_raw(suffix: &str) -> Vec<f32> {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push(format!("{FIXTURE}{suffix}"));
    let bytes = std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    let pcm = wav::read_pcm16(&bytes).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    assert_eq!(pcm.sample_rate, SAMPLE_RATE, "{} is not 16 kHz", p.display());
    wav::to_mono(&pcm.samples, pcm.channels)
}

/// **THE VOLUME KNOB, DERIVED.** Replay the fixture through the canceller as recorded, measure
/// the residual RMS over the blocks where the reference was actually active, and return the
/// scalar that puts it at [`LIVE_RESIDUAL_DBFS`] — the level the CEO's own app printed while
/// it was discarding audio during Ray's walk.
///
/// Nothing here is chosen by ear or by eye: the input is the recording, the target is a figure
/// the running app measured, and the result is printed by
/// `the_scaling_is_derived_from_the_apps_own_measurement` so a reviewer reads the number
/// instead of trusting it.
fn mic_gain_for_the_walks_volume() -> f32 {
    static G: OnceLock<f32> = OnceLock::new();
    *G.get_or_init(|| {
        let mic = read_raw("-mic.wav");
        let reference = read_raw("-reference.wav");
        let (mut aec, ring) = EchoCanceller::new();
        let n = mic.len().min(reference.len());
        let (mut power, mut blocks) = (0.0f64, 0usize);
        for b in 0..(n / AEC_BLOCK) {
            let lo = b * AEC_BLOCK;
            ring.push(&reference[lo..lo + AEC_BLOCK]);
            let mut buf = [0.0f32; AEC_BLOCK];
            buf.copy_from_slice(&mic[lo..lo + AEC_BLOCK]);
            aec.process_block(&mut buf);
            if aec.last_block().reference_rms > FAR_END_ACTIVE_RMS {
                power += (rms(&buf) * rms(&buf)) as f64;
                blocks += 1;
            }
        }
        assert!(blocks > 0, "the reference is never active — this is not the echo-path fixture");
        let residual = (power / blocks as f64).sqrt() as f32;
        let target = 10f32.powf(LIVE_RESIDUAL_DBFS / 20.0);
        target / residual.max(1e-9)
    })
}

/// The fixture at the walk's volume. Cached: these scenes read it dozens of times.
fn read(suffix: &str) -> Vec<f32> {
    static MIC: OnceLock<Vec<f32>> = OnceLock::new();
    static REF: OnceLock<Vec<f32>> = OnceLock::new();
    match suffix {
        "-mic.wav" => MIC
            .get_or_init(|| {
                let g = mic_gain_for_the_walks_volume();
                read_raw("-mic.wav").iter().map(|s| s * g).collect()
            })
            .clone(),
        // The reference is what went to the DAC and is NOT scaled: scaling the microphone
        // alone is exactly what turning the speakers up does to this pair.
        "-reference.wav" => REF.get_or_init(|| read_raw("-reference.wav")).clone(),
        other => panic!("no such fixture track: {other}"),
    }
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

/// The quietest second of the CEO's own recording — his room, not a model of one. Used
/// wherever this file needs "nobody is making a sound", so even the silence in these
/// scenarios is his.
fn room_floor(frames: usize) -> Vec<f32> {
    let mic = read("-mic.wav");
    let window = SAMPLE_RATE as usize; // 1.000 s
    let mut best = (f32::MAX, 0usize);
    let mut at = 0usize;
    while at + window <= mic.len() {
        let r = rms(&mic[at..at + window]);
        if r < best.0 {
            best = (r, at);
        }
        at += VAD_FRAME_SAMPLES;
    }
    let src = &mic[best.1..best.1 + window];
    (0..frames * VAD_FRAME_SAMPLES).map(|i| src[i % src.len()]).collect()
}

/// A voice that is NOT Rich: broadband, enveloped, reproducible. Identical in construction to
/// `barge_in_composition`'s `speechish`, so the two suites argue about the same kind of signal.
/// It stands in for the CEO in every scenario below, and it is never played through the
/// speakers — nothing of it ever reaches the reference.
fn a_voice(frames: usize, seed: u32) -> Vec<f32> {
    let n = frames * VAD_FRAME_SAMPLES;
    let mut s = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    let (mut y1, mut y2) = (0.0f32, 0.0f32);
    (0..n)
        .map(|i| {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            let x = (s as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let t = i as f32 / SAMPLE_RATE as f32;
            let env = (0.5 + 0.5 * (2.0 * std::f32::consts::PI * 2.7 * t).sin()).powf(1.5);
            y1 = 0.92 * y1 + 0.08 * x;
            y2 = 0.55 * y2 + 0.45 * (x - y1);
            (y1 * 1.6 + y2 * 0.5) * env * 0.35
        })
        .collect()
}

/// One replayable scene, one VAD frame per entry. `mic` and `reference` are in the canceller's
/// coordinate system: index `i` means the same instant in both, exactly as the fixture does.
#[derive(Default)]
struct Scene {
    mic: Vec<f32>,
    reference: Vec<f32>,
    /// The playout queue is non-empty — what the SHIPPED `speaking` flag was.
    queued: Vec<bool>,
    /// A sentence is between the speaker channel and the queue: `say` is still synthesizing.
    owed: Vec<bool>,
    /// Frame-level labels, for failure messages that name the moment rather than an index.
    label: Vec<&'static str>,
}

impl Scene {
    fn frames(&self) -> usize {
        self.queued.len()
    }

    /// Nobody is making a sound: the CEO's own room floor, no reference, no queue, nothing owed.
    fn quiet(&mut self, frames: usize, label: &'static str) {
        let floor = room_floor(frames);
        self.push(&floor, &vec![0.0; frames * VAD_FRAME_SAMPLES], false, false, frames, label);
    }

    /// Somebody who is not Rich is talking, into a room where nothing is playing.
    fn a_person_talks(&mut self, frames: usize, seed: u32, label: &'static str) {
        let v = a_voice(frames, seed);
        self.push(&v, &vec![0.0; frames * VAD_FRAME_SAMPLES], false, false, frames, label);
    }

    /// Rich speaks one sentence through the speakers, taken from the CEO's own recording at
    /// `from` frames in. `owed_after` says whether another sentence is already in synthesis —
    /// which is the fact the shipped flag had no way to see.
    fn rich_speaks(&mut self, from: usize, frames: usize, owed_after: bool, label: &'static str) {
        let mic = read("-mic.wav");
        let reference = read("-reference.wav");
        let lo = from * VAD_FRAME_SAMPLES;
        let hi = (lo + frames * VAD_FRAME_SAMPLES).min(mic.len()).min(reference.len());
        let n = (hi - lo) / VAD_FRAME_SAMPLES;
        assert!(n > 0, "the fixture is shorter than this scene needs");
        self.push(
            &mic[lo..lo + n * VAD_FRAME_SAMPLES],
            &reference[lo..lo + n * VAD_FRAME_SAMPLES],
            true,
            owed_after,
            n,
            label,
        );
    }

    /// The gap between two spoken sentences: `say` is spawning, the queue is EMPTY, and the
    /// room is quiet — except for the first frame, which still carries the device tail of the
    /// sentence that just ended (10.7 ms measured, under one 16.000 ms frame).
    fn synthesis_gap(&mut self, from: usize, frames: usize, label: &'static str) {
        assert!(frames >= 1);
        let mic = read("-mic.wav");
        let lo = from * VAD_FRAME_SAMPLES;
        self.push(&mic[lo..lo + VAD_FRAME_SAMPLES], &vec![0.0; VAD_FRAME_SAMPLES], false, true, 1, label);
        let floor = room_floor(frames - 1);
        self.push(&floor, &vec![0.0; (frames - 1) * VAD_FRAME_SAMPLES], false, true, frames - 1, label);
    }

    fn push(
        &mut self,
        mic: &[f32],
        reference: &[f32],
        queued: bool,
        owed: bool,
        frames: usize,
        label: &'static str,
    ) {
        assert_eq!(mic.len(), frames * VAD_FRAME_SAMPLES);
        assert_eq!(reference.len(), frames * VAD_FRAME_SAMPLES);
        self.mic.extend_from_slice(mic);
        self.reference.extend_from_slice(reference);
        for _ in 0..frames {
            self.queued.push(queued);
            self.owed.push(owed);
            self.label.push(label);
        }
    }

    /// The SHIPPED `speaking` signal, frame by frame: `playout.is_playing()`, which is
    /// `queued_samples() > 0` and nothing else.
    fn shipped_speaking(&self) -> Vec<bool> {
        self.queued.clone()
    }

    /// The signal the supervisor computes NOW: [`AudibleWindow`] over queued, owed and the
    /// measured hold. Evaluated once per frame here rather than once per 25 ms tick, which is
    /// strictly the harder test — a coarser sampler can only hold the window open for longer.
    fn audible_speaking(&self) -> Vec<bool> {
        let hold = audible_hold_secs(MEASURED_OUT_TAIL_SECS, MEASURED_IN_LATENCY_SECS);
        let mut w = AudibleWindow::new();
        (0..self.frames())
            .map(|i| {
                let now_ms = (i as f32 * frames_to_secs(1) * 1000.0) as u64;
                w.observe(self.queued[i], self.owed[i], hold, now_ms);
                w.is_open()
            })
            .collect()
    }
}

/// What one replay produced.
#[derive(Debug, Default, PartialEq)]
struct Outcome {
    /// Utterances handed to the recognizer — each one becomes a message in the CEO's thread.
    admitted: usize,
    /// Seconds of audio in each admitted utterance, in order.
    admitted_secs: Vec<f32>,
    tainted_discards: usize,
    other_discards: usize,
    barge_ins: usize,
    /// **`utterance START` lines — the artifact Ray's walk found MISSING.** Counted separately
    /// from admission on purpose: a start that is later discarded as echo is still the app
    /// noticing a sound, and it is the only thing the notice layer can hang a card on. Zero
    /// starts is a different defect from zero admissions and the record conflated them.
    starts: usize,
    /// Of those, the ones born `tainted=true` — Rich was audible, so nothing of his is
    /// submitted. This is the count that must be non-zero for a real interruption during an
    /// answer: heard, acknowledged, not submitted.
    tainted_starts: usize,
}

fn tally(msgs: &[CapMsg], out: &mut Outcome) {
    for m in msgs {
        match m {
            CapMsg::Started { tainted, .. } => {
                out.starts += 1;
                if *tainted {
                    out.tainted_starts += 1;
                }
            }
            CapMsg::Utterance(u) => {
                out.admitted += 1;
                out.admitted_secs.push(u.samples.len() as f32 / SAMPLE_RATE as f32);
            }
            CapMsg::Discarded { tainted: true } => out.tainted_discards += 1,
            CapMsg::Discarded { tainted: false } => out.other_discards += 1,
            CapMsg::BargeIn { .. } => out.barge_ins += 1,
            _ => {}
        }
    }
}

/// Replay a scene through the REAL `CaptureBrain` — the exact struct the audio callback runs.
fn replay(scene: &Scene, speaking: &[bool], forced_at: Option<usize>) -> Outcome {
    let (aec, ring) = EchoCanceller::new();
    let mut brain = CaptureBrain::with_aec(aec);
    let mut out = Outcome::default();
    for i in 0..scene.frames() {
        let lo = i * AEC_BLOCK;
        ring.push(&scene.reference[lo..lo + AEC_BLOCK]);
        let forced = forced_at == Some(i);
        let msgs = brain.push_frame(&scene.mic[lo..lo + AEC_BLOCK], speaking[i], forced);
        tally(&msgs, &mut out);
    }
    assert!(!brain.aec_confident(), "the canceller became confident on the CEO's own echo path");
    out
}

// =============================================================================================
// THE SHIPPED RULE, RECONSTRUCTED
// =============================================================================================

/// **THE RULE AS IT SHIPPED IN CANDIDATE .5**, reconstructed term for term from the public
/// parts, so the same audio can be run under both and the difference attributed.
///
/// `echo_path_replay.rs` sets the precedent: a negative test that cannot also show the OLD
/// behavior proves nothing, because a fixture that quietly stopped containing the defect would
/// pass it forever.
///
/// What is reproduced exactly: the canceller in front of everything (so the VAD, the endpointer
/// and the recorder see the residual), `confident()` from that same canceller, and
/// `tainted = speaking && !barged && !confident` evaluated ONCE, on the frame the recorder
/// starts.
///
/// What is omitted, and why it cannot change a verdict here: the barge-in monitor, the
/// silent-input watch and the level meter. The monitor is the only one that could — it clears
/// the taint when it fires — and `the_shipped_rule_never_fired_a_barge_in_in_this_scene` asserts
/// on the REAL brain that it fires zero times in every scene in this file. Ray's walk says the
/// same of the live run: *"Rich was not cut off. He finished."*
struct ShippedBrain {
    vad: Vad,
    recorder: UtteranceRecorder,
    aec: EchoCanceller,
    residual: Vec<f32>,
    tainted: bool,
    was_recording: bool,
}

impl ShippedBrain {
    fn new() -> (ShippedBrain, Arc<ReferenceRing>) {
        let (aec, ring) = EchoCanceller::new();
        (
            ShippedBrain {
                vad: Vad::default(),
                recorder: UtteranceRecorder::new(),
                aec,
                residual: vec![0.0; VAD_FRAME_SAMPLES],
                tainted: false,
                was_recording: false,
            },
            ring,
        )
    }

    fn push_frame(&mut self, frame: &[f32], speaking: bool, out: &mut Outcome) {
        self.residual.clear();
        self.residual.extend_from_slice(frame);
        let mut buf = std::mem::take(&mut self.residual);
        self.aec.process_block(&mut buf);
        let confident = self.aec.confident();
        let is_speech = self.vad.push_frame(&buf);

        let finished = self.recorder.push_frame(&buf, is_speech);
        let recording = self.recorder.is_recording();
        let started = recording && !self.was_recording;
        let stopped = !recording && self.was_recording;
        self.was_recording = recording;
        self.residual = buf;

        if started {
            // THE SHIPPED LINE, verbatim from controller.rs before 2026-09-17:
            //     self.tainted = speaking && !self.barged && !confident;
            // `barged` is false throughout — see the struct docs.
            self.tainted = speaking && !confident;
        }

        if let Some(u) = finished {
            if self.tainted {
                out.tainted_discards += 1;
            } else {
                out.admitted += 1;
                out.admitted_secs.push(u.samples.len() as f32 / SAMPLE_RATE as f32);
            }
            self.tainted = false;
        } else if stopped {
            out.other_discards += 1;
            self.tainted = false;
        }
    }
}

fn replay_shipped(scene: &Scene) -> Outcome {
    let (mut brain, ring) = ShippedBrain::new();
    let speaking = scene.shipped_speaking();
    let mut out = Outcome::default();
    for i in 0..scene.frames() {
        let lo = i * AEC_BLOCK;
        ring.push(&scene.reference[lo..lo + AEC_BLOCK]);
        brain.push_frame(&scene.mic[lo..lo + AEC_BLOCK], speaking[i], &mut out);
    }
    out
}

// =============================================================================================
// THE SCENES
// =============================================================================================

fn frames_for(secs: f32) -> usize {
    (secs * SAMPLE_RATE as f32 / VAD_FRAME_SAMPLES as f32).round() as usize
}

/// **THE WALK'S OWN SHAPE.** A question is asked out loud; before the 0.800 s hangover has
/// closed that utterance, Rich's answer starts coming out of the speakers and lands inside it.
///
/// This is the shape of Ray's own talk-over and of the CEO's rig generally: *the answer is
/// spoken into the same microphone*. `SILENCE_HANGOVER_FRAMES` is 50 frames — 50 x 256 / 16000 =
/// 0.800 s — so an utterance stays open across a pause far longer than the 0.3 s used here, and
/// everything that arrives in the meantime goes into the same WAV and the same transcript.
fn a_question_then_rich_answers_into_it() -> Scene {
    let r0 = frames_for(RICH_STARTS_AT_SECS);
    let sentence = frames_for(1.2);
    let gap = frames_for(0.35);
    let mut s = Scene::default();
    s.quiet(frames_for(1.5), "the room, settling the VAD floor");
    s.a_person_talks(frames_for(0.8), 0xA11CE, "somebody asks a question");
    s.quiet(frames_for(0.3), "a pause, well inside the 0.800 s hangover");
    // Rich answers, as three `say` invocations with synthesis gaps between them.
    s.rich_speaks(r0, sentence, true, "Rich, sentence 1");
    s.synthesis_gap(r0 + sentence, gap, "say spawning, sentence 2");
    s.rich_speaks(r0 + sentence, sentence, true, "Rich, sentence 2");
    s.synthesis_gap(r0 + 2 * sentence, gap, "say spawning, sentence 3");
    s.rich_speaks(r0 + 2 * sentence, sentence, false, "Rich, sentence 3");
    s.quiet(frames_for(2.0), "the answer is over");
    s
}

/// **AN UTTERANCE BORN WHILE A SENTENCE IS STILL IN SYNTHESIS.** The answer's first sentence
/// has been handed to the speaker thread, `say` is spawning, the playout queue is EMPTY — and
/// something in the room is heard in that window.
///
/// The shipped flag reported RICH IS NOT SPEAKING for the whole of it, because it was
/// `queued_samples() > 0` and nothing else. Deltas chunk on runs of `.`/`!`/`?`
/// (`chunk.rs:126-129`), so the walk's *"One... two... three..."* became roughly twenty
/// one-word sentences and therefore twenty separate spawns, each with a window of its own.
///
/// **This is the FIRST of those windows, deliberately.** Between two spoken sentences the same
/// thing is true, but a mid-answer scene would have Rich's previous sentence already holding an
/// utterance open, which tests the taint re-evaluation rather than the `owed` term. This one
/// isolates `owed`: nothing has played yet, so there is nothing else to attribute the verdict
/// to. That every mid-answer gap is covered too is
/// `the_window_is_open_for_every_frame_of_the_answer_including_the_gaps`.
fn an_utterance_born_while_say_is_still_spawning() -> Scene {
    let mut s = Scene::default();
    s.quiet(frames_for(1.5), "the room, settling the VAD floor");
    // The first sentence has been SENT. No sound exists yet, and a voice is heard.
    let spawning = frames_for(0.8);
    let v = a_voice(spawning, 0xB0B);
    s.push(&v, &vec![0.0; spawning * VAD_FRAME_SAMPLES], false, true, spawning, "say is spawning");
    s.rich_speaks(frames_for(RICH_STARTS_AT_SECS), frames_for(1.5), false, "Rich, sentence 1");
    s.quiet(frames_for(2.0), "the answer is over");
    s
}

// =============================================================================================
// THE TESTS
// =============================================================================================

/// **THE CALIBRATION, PRINTED.** The scalar that stands in for the walk's output volume is
/// derived from the app's own measurement; this is where a reviewer reads it, and where a
/// fixture re-recorded at a different level would announce itself.
#[test]
fn the_scaling_is_derived_from_the_apps_own_measurement() {
    let g = mic_gain_for_the_walks_volume();
    let raw = read_raw("-mic.wav");
    let scaled = read("-mic.wav");
    println!(
        "[measured] fixture mic {:.1} dBFS overall -> x{g:.2} -> {:.1} dBFS, so the residual \
         lands at {LIVE_RESIDUAL_DBFS} dBFS, where the app measured its own during the walk",
        20.0 * rms(&raw).log10(),
        20.0 * rms(&scaled).log10()
    );
    assert!(g > 1.0, "the fixture is already at or above the walk's level (x{g:.2})");
    assert!(g < 32.0, "x{g:.2} is not a volume difference, it is a different recording");
    // And the scaled residual really does clear the VAD's absolute floor, which is the whole
    // point: below it, Rich's voice produces no speech frame and no scene here means anything.
    let floor_dbfs = 20.0 * 0.005f32.log10();
    assert!(
        LIVE_RESIDUAL_DBFS > floor_dbfs,
        "{LIVE_RESIDUAL_DBFS} dBFS is under the VAD's {floor_dbfs:.2} dBFS absolute floor"
    );
}

/// **POSITIVE CONTROL, AND IT COMES FIRST.** The reconstruction of the shipped rule must
/// actually reproduce the defect on this fixture, or the regression below passes for the wrong
/// reason forever.
///
/// Under the rule that shipped, the utterance is ADMITTED — handed to whisper and submitted as
/// the CEO's message — and it is long enough to contain seconds of Rich. That is the bubble Ray
/// photographed in `a5-07`.
#[test]
fn the_shipped_rule_submits_richs_own_answer_as_the_ceos_message() {
    let scene = a_question_then_rich_answers_into_it();
    let out = replay_shipped(&scene);
    assert_eq!(
        out.admitted, 1,
        "the shipped rule no longer admits the utterance, so the regression test below proves \
         nothing — re-derive the scene or re-record the fixture ({out:?})"
    );
    // **AND IT CONTAINS RICH.** Everything before him is bounded and known: 0.304 s of
    // pre-roll (PRE_ROLL_FRAMES = 19, 19 x 256 / 16000 = 0.304), the 0.800 s question and the
    // 0.300 s pause — 1.404 s in total. Every millisecond past that was captured while Rich
    // was coming out of the speakers, so the bar is that sum and not a round number.
    //
    // Measured here: 2.224 s admitted, so 0.820 s of the WAV that whisper was handed is Rich.
    let before_rich = frames_to_secs(PRE_ROLL_FRAMES as u32) + 0.800 + 0.300;
    let secs = out.admitted_secs[0];
    let of_rich = secs - before_rich;
    println!("[measured] {secs:.3} s submitted as the CEO's message, {of_rich:.3} s of it Rich");
    assert!(
        of_rich > 0.5,
        "{secs:.3} s admitted and only {of_rich:.3} s of it captured while Rich was audible — \
         too little of him in the WAV for this scene to still be the defect"
    );
}

/// **THE REGRESSION.** The same audio, the same fixture, the same canceller — and the same
/// `CaptureBrain` the audio callback actually runs. Nothing reaches the recognizer.
#[test]
fn richs_own_answer_is_never_submitted_as_the_ceos_message() {
    let scene = a_question_then_rich_answers_into_it();
    let out = replay(&scene, &scene.audible_speaking(), None);
    assert_eq!(
        out.admitted, 0,
        "{} s of audio containing Rich's own voice was submitted as the CEO's message ({out:?})",
        out.admitted_secs.iter().map(|s| format!("{s:.3}")).collect::<Vec<_>>().join(", ")
    );
    assert_eq!(out.tainted_discards, 1, "the utterance was dropped for some other reason ({out:?})");
}

/// **THE SAME REGRESSION WITH THE OLD `speaking` SIGNAL**, which is how this file attributes the
/// fix rather than merely observing it.
///
/// Feed the NEW brain the SHIPPED flag — queue depth only, false in every gap — and it still
/// refuses, because taint is re-evaluated for as long as the utterance is alive. Each half of
/// the fix closes this scene on its own; they are not one change wearing two names.
#[test]
fn re_evaluating_taint_closes_this_scene_even_on_the_old_speaking_signal() {
    let scene = a_question_then_rich_answers_into_it();
    let out = replay(&scene, &scene.shipped_speaking(), None);
    assert_eq!(out.admitted, 0, "{out:?}");
    assert_eq!(out.tainted_discards, 1, "{out:?}");
}

/// **THE OTHER HOLE, SHOWN BOTH WAYS.** An utterance that begins between two spoken sentences.
///
/// With the shipped flag the queue is empty for the whole gap, so it is born untainted and
/// admitted. With the audible window — queued OR owed OR within the measured tail — Rich's
/// answer is in progress for that entire gap and it is refused.
///
/// **What this costs, stated plainly:** if the voice in that gap really was the CEO, his words
/// are thrown away too. On a path where the canceller has declined to vouch for its residual
/// the app cannot tell the two apart, and that is exactly what
/// `VoiceNotice::CouldNotListenWhileSpeaking` says to him, conditionally, once per session.
#[test]
fn an_utterance_born_while_say_is_spawning_is_admitted_by_the_old_flag_and_refused_by_the_window() {
    let scene = an_utterance_born_while_say_is_still_spawning();

    // THE SHIPPED RULE, on the shipped flag: born untainted because the queue was empty, kept
    // alive by the 0.800 s hangover across Rich's sentence, and submitted whole.
    let old = replay_shipped(&scene);
    assert_eq!(
        old.admitted, 1,
        "the shipped rule no longer admits this, so the assertion below proves nothing ({old:?})"
    );
    let before_rich = frames_to_secs(PRE_ROLL_FRAMES as u32) + 0.800;
    assert!(
        old.admitted_secs[0] > before_rich + 0.5,
        "{:.3} s admitted, of which at most {before_rich:.3} s precedes Rich — the WAV the \
         shipped rule submitted has to contain him for this to be the defect",
        old.admitted_secs[0]
    );

    // THE WINDOW: `owed` is true for the whole of that window, so nothing is born untainted.
    let new = replay(&scene, &scene.audible_speaking(), None);
    assert_eq!(new.admitted, 0, "{new:?}");
    assert_eq!(new.tainted_discards, 1, "{new:?}");
}

/// **THE POSITIVE CONTROL THE WHOLE FIX HANGS ON.** A test suite that refused everything would
/// pass every assertion above and would have broken voice mode completely.
///
/// The CEO speaks after Rich has finished — past the last sample, past the measured device
/// tail, past the 0.416 s of pre-roll the recorder reaches back through
/// (PRE_ROLL_FRAMES + SPEECH_ONSET_FRAMES = 19 + 7 = 26, and 26 x 256 / 16000 = 0.416 s). His
/// utterance is admitted, whole.
#[test]
fn the_ceo_speaking_after_the_answer_ends_is_still_heard() {
    let mut s = Scene::default();
    s.quiet(frames_for(1.5), "the room, settling the VAD floor");
    s.rich_speaks(0, frames_for(1.2), false, "Rich answers");
    s.quiet(frames_for(0.6), "past the tail and past the 0.416 s look-back");
    s.a_person_talks(frames_for(1.0), 0x0CE0, "the CEO replies");
    s.quiet(frames_for(1.2), "he stops");

    let out = replay(&s, &s.audible_speaking(), None);
    assert_eq!(out.admitted, 1, "the CEO was not heard after Rich finished ({out:?})");
    assert_eq!(out.tainted_discards, 0, "{out:?}");
}

/// **AND THE LOOK-BACK IS NOT LONGER THAN IT CLAIMS.** 0.416 s is a real cost to the CEO — he
/// has to leave that much air after Rich stops — so it is pinned rather than left to drift
/// upward the next time someone widens the window "to be safe".
#[test]
fn the_look_back_is_the_0_416_s_the_recorder_actually_reaches_back_through() {
    let frames = PRE_ROLL_FRAMES as u32 + SPEECH_ONSET_FRAMES;
    assert_eq!(frames, 26, "19 + 7");
    assert!(
        (frames_to_secs(frames) - 0.416).abs() < 1e-6,
        "{} s, not 26 x 256 / 16000 = 0.416 s",
        frames_to_secs(frames)
    );
    // And the hangover it has to survive is a different number entirely — the two get confused.
    assert!((frames_to_secs(SILENCE_HANGOVER_FRAMES) - 0.800).abs() < 1e-6);
}

/// **BARGE-IN STILL RESCUES THE CEO'S WORDS.** "Tap to stop" is authoritative: it clears the
/// taint mid-utterance, and the continuous re-evaluation must not put it back — otherwise the
/// words he is saying at that instant are thrown away, which is the fix eating the feature.
#[test]
fn a_barge_in_mid_answer_still_delivers_the_words_the_ceo_is_saying() {
    let mut s = Scene::default();
    s.quiet(frames_for(1.5), "the room, settling the VAD floor");
    s.rich_speaks(0, frames_for(0.5), true, "Rich starts answering");
    // The CEO talks over him, and taps stop a quarter of a second in.
    let over = frames_for(1.4);
    let v = a_voice(over, 0x570B);
    let mic = read("-mic.wav");
    let reference = read("-reference.wav");
    let lo = frames_for(0.5) * VAD_FRAME_SAMPLES;
    let mixed: Vec<f32> = (0..over * VAD_FRAME_SAMPLES)
        .map(|i| v[i] + mic.get(lo + i).copied().unwrap_or(0.0))
        .collect();
    let refslice: Vec<f32> = (0..over * VAD_FRAME_SAMPLES)
        .map(|i| reference.get(lo + i).copied().unwrap_or(0.0))
        .collect();
    s.push(&mixed, &refslice, true, false, over, "the CEO talks over Rich");
    s.quiet(frames_for(1.2), "he stops");

    let tap_at = frames_for(1.5) + frames_for(0.5) + frames_for(0.25);
    let out = replay(&s, &s.audible_speaking(), Some(tap_at));
    assert_eq!(out.barge_ins, 1, "tap to stop did not cut Rich off ({out:?})");
    assert_eq!(
        out.admitted, 1,
        "the CEO tapped stop and his words were still thrown away as echo ({out:?})"
    );
}

/// **NO BARGE-IN FIRES IN THE REPLAYED SCENES**, which is what lets `ShippedBrain` leave the
/// monitor out and still be a faithful reconstruction. Asserted on the REAL brain, over the
/// real fixture — and it matches Ray's observation of the live run: *"Rich was not cut off."*
#[test]
fn the_shipped_rule_never_fired_a_barge_in_in_this_scene() {
    for scene in
        [a_question_then_rich_answers_into_it(), an_utterance_born_while_say_is_still_spawning()]
    {
        for speaking in [scene.shipped_speaking(), scene.audible_speaking()] {
            let out = replay(&scene, &speaking, None);
            assert_eq!(out.barge_ins, 0, "{out:?}");
        }
    }
}

// =============================================================================================
// THE WINDOW ITSELF
// =============================================================================================

/// **THE INVARIANT, PROVED OVER THE WHOLE ANSWER RATHER THAN AT ONE MOMENT.**
///
/// Ray could not say from the record whether the utterance began in a gap or after playout was
/// marked ended. This is why that question no longer decides anything: there is no frame
/// between the first sentence being handed to `say` and the end of the measured tail on which
/// the window reports Rich silent. Every candidate birthplace is inside it.
#[test]
fn the_window_is_open_for_every_frame_of_the_answer_including_the_gaps() {
    let scene = a_question_then_rich_answers_into_it();
    let audible = scene.audible_speaking();
    let shipped = scene.shipped_speaking();

    let first = scene.label.iter().position(|l| l.starts_with("Rich")).unwrap();
    let last = scene.label.iter().rposition(|l| l.starts_with("Rich")).unwrap();

    let holes: Vec<&str> =
        (first..=last).filter(|&i| !audible[i]).map(|i| scene.label[i]).collect();
    assert!(holes.is_empty(), "the window closed inside the answer at: {holes:?}");

    // And the shipped flag genuinely had holes there, or this test is measuring nothing.
    let shipped_holes = (first..=last).filter(|&i| !shipped[i]).count();
    assert!(
        shipped_holes > 0,
        "the shipped flag had no gap in this scene, so it is not the scene the defect needs"
    );
    let secs = shipped_holes as f32 * frames_to_secs(1);
    assert!(
        secs > frames_to_secs(SPEECH_ONSET_FRAMES),
        "{secs:.3} s of holes is under the {:.3} s onset — too short to birth an utterance in",
        frames_to_secs(SPEECH_ONSET_FRAMES)
    );
}

/// **THE WINDOW CLOSES.** A hold that never ended would be a microphone that never listens
/// again, which is a worse defect than the one being fixed. It closes, and it closes within
/// the hold it reports.
#[test]
fn the_window_closes_once_the_measured_hold_has_elapsed() {
    let hold = audible_hold_secs(MEASURED_OUT_TAIL_SECS, MEASURED_IN_LATENCY_SECS);
    let mut w = AudibleWindow::new();
    // Rich plays up to and including ms 99 — that is the last LIVE observation, and the hold
    // is measured from it, not from the first silent one.
    let last_live_ms = 99;
    for ms in 0..=last_live_ms {
        w.observe(true, false, hold, ms);
    }
    assert!(w.is_open());
    // The queue empties and nothing is owed. It holds, then it lets go.
    let mut closed_at = None;
    for ms in (last_live_ms + 1)..1000 {
        w.observe(false, false, hold, ms);
        if !w.is_open() && closed_at.is_none() {
            closed_at = Some(ms - last_live_ms);
        }
    }
    let closed = closed_at.expect("the half-duplex window never closed — the mic is deaf");
    let hold_ms = (hold * 1000.0).ceil() as u64;
    assert_eq!(closed, hold_ms, "closed after {closed} ms, and the reported hold is {hold_ms} ms");
}

/// **A SENTENCE STILL IN SYNTHESIS HOLDS THE WINDOW OPEN ON ITS OWN**, with nothing queued —
/// the term that did not exist before and the one that covers `say`'s spawn.
#[test]
fn synthesis_alone_holds_the_window_open() {
    let hold = audible_hold_secs(MEASURED_OUT_TAIL_SECS, MEASURED_IN_LATENCY_SECS);
    let mut w = AudibleWindow::new();
    // The very first sentence has been SENT; nothing has played yet.
    w.observe(false, true, hold, 0);
    assert!(w.is_open(), "the window did not open until sound existed");
    // 400 ms of `say` spawning, queue empty throughout.
    for ms in 1..400 {
        w.observe(false, true, hold, ms);
        assert!(w.is_open(), "the window closed at {ms} ms while a sentence was still owed");
    }
}

/// **THE HOLD IS MEASURED, AND ITS FLOOR IS THE TICK.** Every term is a reading off a running
/// device; the floor exists because a hold evaluated once per 25 ms tick cannot be enforced any
/// finer than that, and claiming otherwise would be a timing assertion the code cannot keep.
#[test]
fn the_hold_is_the_sum_of_the_measured_terms_floored_at_one_tick() {
    // On the CEO's rig: 10.7 + 10.7 + 16.0 = 37.4 ms, which is over the 25 ms floor.
    let his = audible_hold_secs(MEASURED_OUT_TAIL_SECS, MEASURED_IN_LATENCY_SECS);
    let expected = MEASURED_OUT_TAIL_SECS + MEASURED_IN_LATENCY_SECS + frames_to_secs(1);
    assert!((his - expected).abs() < 1e-6, "{his} vs {expected}");
    assert!(his > 0.025, "{his} is under one tick, so the floor would be what applies");

    // A device that reports nothing yet still gets the floor rather than zero.
    assert!((audible_hold_secs(0.0, 0.0) - 0.025).abs() < 1e-6);

    // And a slow USB interface widens it rather than being clamped to something convenient.
    let slow = audible_hold_secs(0.085, 0.040);
    assert!((slow - (0.085 + 0.040 + frames_to_secs(1))).abs() < 1e-6, "{slow}");
}

// =============================================================================================
// WHAT IS LEFT FOR THE NOTICE TO SAY
// =============================================================================================

/// **COULD THE APP TELL THE CEO'S VOICE FROM RICH'S ECHO BY LEVEL? MEASURED, ON HIS PATH.**
///
/// With the half-duplex window covering the whole answer, every tainted discard on an
/// unconfident canceller is audio that overlapped Rich. Two different things produce it — his
/// echo, and the CEO genuinely talking over Rich without meeting the 5.008 s debounce — and the
/// brief asks whether the second can be separated from the first, so the notice could fire only
/// on a voiced residual that stands above the echo the canceller expects.
///
/// **Voicing cannot separate them**: Rich's echo IS voiced speech, so `voiced.rs`'s pitch and
/// harmonicity evidence says yes to both. That leaves LEVEL, and level has to clear the
/// residual's own block-to-block spread or the test fires on Rich alone.
///
/// This measures that spread over the CEO's own recording with no near-end talker present at
/// all: every decibel of it is Rich, so anything inside it is indistinguishable from him by
/// construction. The figure is printed, and the test asserts only that it is large — the exact
/// value belongs in the log, not in an assertion that would break on a new fixture.
#[test]
fn separating_the_ceo_from_richs_echo_by_level_needs_a_margin_this_large() {
    let mic = read("-mic.wav");
    let reference = read("-reference.wav");
    let (mut aec, ring) = EchoCanceller::new();
    let n = mic.len().min(reference.len());
    let mut residuals: Vec<f32> = Vec::new();
    for b in 0..(n / AEC_BLOCK) {
        let lo = b * AEC_BLOCK;
        ring.push(&reference[lo..lo + AEC_BLOCK]);
        let mut buf = [0.0f32; AEC_BLOCK];
        buf.copy_from_slice(&mic[lo..lo + AEC_BLOCK]);
        aec.process_block(&mut buf);
        if aec.last_block().reference_rms > FAR_END_ACTIVE_RMS {
            residuals.push(20.0 * rms(&buf).max(1e-9).log10());
        }
    }
    assert!(residuals.len() > 100, "{} far-active blocks", residuals.len());
    residuals.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let pct = |q: f32| residuals[((residuals.len() - 1) as f32 * q) as usize];
    let (p50, p90, p99, max) = (pct(0.50), pct(0.90), pct(0.99), *residuals.last().unwrap());
    let spread = max - p50;
    println!(
        "[measured] Rich's echo alone, {} far-active blocks: median {p50:.1} dBFS, p90 {p90:.1}, \
         p99 {p99:.1}, peak {max:.1} — spread above the median {spread:.1} dB",
        residuals.len()
    );
    println!(
        "[measured] so a level test would have to sit at least {spread:.1} dB above the median \
         before it stopped firing on Rich alone, i.e. the CEO would have to reach {:.1} dBFS at \
         the microphone while the echo sits at {p50:.1} dBFS",
        p50 + spread
    );
    assert!(
        spread > 9.0,
        "the residual's own spread is only {spread:.1} dB, so a level discriminator may now be \
         worth building — re-derive the decision recorded in the verification note"
    );
}

// =============================================================================================
// THE NEAR-END TALKER — the case observation C could not test
// =============================================================================================
//
// **WHY THIS SECTION EXISTS, AND WHAT IT CORRECTS.**
//
// Ray's candidate-.6 walk (`docs/verification/2026-09-17-nightly-1.2.0-20260917.6-onscreen-
// audit.md`, observation C) recorded that a deliberate talk-over produced *"NO utterance START
// logged for it at all"*, and that was read as a defect at the 7-frame onset. The walk's own
// command log, in the same file, is the correction:
//
// ```text
//   21:35:01.262  say -v Samantha "Excuse me Rich, you can stop counting now."   (playback 4)
// ```
//
// `say` renders to the DEFAULT OUTPUT DEVICE — the same Mac mini speakers Rich's answer was
// coming out of, at the same output volume 85. So the "interruption" took the identical
// loudspeaker-to-microphone path as the echo and arrived at the Wave:3 at the echo's own level.
// Audit-5 printed that level while discarding it: **residual -44.7 dBFS**
// (`2026-09-17-nightly-1.2.0-20260917.5-onscreen-audit-2.md:449`), and
// `separating_the_ceo_from_richs_echo_by_level_needs_a_margin_this_large` above measures Rich's
// echo ALONE on this rig at median -48.6, p90 -40.2, p99 -36.2, peak -34.3 dBFS. -44.7 sits
// inside that distribution. The harness was not a person interrupting; it was a second echo.
//
// A person interrupting is NEAR-END: his voice reaches the microphone directly, never through
// the speakers, and therefore never enters the reference the canceller subtracts. That is the
// case the whole half-duplex design turns on, and no test drove it at a near-field level while
// Rich was audible. These do.
//
// Nothing here plays a sound: `say -o` renders to a file.

/// Ray's own sentence, in Ray's own voice, synthesized OFFLINE to 16 kHz mono — the pipeline's
/// native rate, so nothing is resampled between the synthesizer and the VAD.
///
/// `-o <file>` makes `say` write a WAV instead of playing it, so this suite makes no sound at
/// any volume. Same mechanism `voiced_acceptance.rs` uses, and for the same reason: a model of
/// a voice can be fitted to its own detector, so the speech here is real speech.
fn say_offline(voice: &str, text: &str) -> Vec<f32> {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!("richos-voice-near-end-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path =
        dir.join(format!("{}.wav", SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
    let out = std::process::Command::new("/usr/bin/say")
        .args(["-v", voice, "-o"])
        .arg(&path)
        .arg("--data-format=LEI16@16000")
        .arg(text)
        .output()
        .unwrap_or_else(|e| panic!("could not run /usr/bin/say: {e}"));
    assert!(
        out.status.success(),
        "`say -v {voice}` failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let bytes = std::fs::read(&path).unwrap();
    let pcm = wav::read_pcm16(&bytes).unwrap();
    assert_eq!(pcm.sample_rate, SAMPLE_RATE, "say did not honor --data-format");
    let _ = std::fs::remove_file(&path);
    wav::to_mono(&pcm.samples, pcm.channels)
}

fn dbfs(x: &[f32]) -> f32 {
    20.0 * rms(x).max(1e-12).log10()
}

/// Scale a signal so its RMS over its whole length sits at `target` dBFS.
fn at_dbfs(mut xs: Vec<f32>, target: f32) -> Vec<f32> {
    let k = 10f32.powf(target / 20.0) / rms(&xs).max(1e-12);
    for x in xs.iter_mut() {
        *x *= k;
    }
    xs
}

impl Scene {
    /// **A SOUND THAT REACHES THE MICROPHONE WITHOUT PASSING THROUGH THE SPEAKERS.** Summed
    /// into the microphone track and into NOTHING ELSE: the reference is untouched, so the
    /// canceller holds no copy of it to subtract and cannot attenuate it by even the 3-7 dB it
    /// manages on Rich. That asymmetry IS the difference between a person at the desk and
    /// `say` through the app's own output device.
    fn near_end_talker(&mut self, at_frame: usize, signal: &[f32]) {
        let lo = at_frame * VAD_FRAME_SAMPLES;
        assert!(lo + signal.len() <= self.mic.len(), "the talker runs off the end of the scene");
        for (i, s) in signal.iter().enumerate() {
            self.mic[lo + i] += s;
        }
    }

    /// Rich answering out loud for at least `secs`, as a chain of `say` invocations with the
    /// synthesis gaps between them — the shape of the walk's *"one... two... three..."*, whose
    /// deltas chunk into roughly one sentence per number (`chunk.rs:126-129`). The fixture
    /// holds one sentence and the walk's answer ran 16.5 s, so it is re-entered at rotating
    /// offsets rather than replayed from the same sample, which would make the echo periodic.
    fn rich_answers_for(&mut self, secs: f32, label: &'static str) -> std::ops::Range<usize> {
        let r0 = frames_for(RICH_STARTS_AT_SECS);
        let sentence = frames_for(1.2);
        let gap = frames_for(0.35);
        let start = self.frames();
        let mut done = 0.0;
        let mut k = 0usize;
        while done < secs {
            let from = r0 + (k % 5) * frames_for(0.3);
            self.rich_speaks(from, sentence, true, label);
            done += frames_to_secs(sentence as u32);
            self.synthesis_gap(from + sentence, gap, label);
            done += frames_to_secs(gap as u32);
            k += 1;
        }
        start..self.frames()
    }
}

/// **THE MEASUREMENT BEHIND EVERY LEVEL IN THIS SECTION, PRINTED.** Nothing below picks a
/// near-field level by feel; this is where a reviewer reads the anchors, and where a re-recorded
/// fixture or a changed convention announces itself.
#[test]
fn the_levels_this_suite_reasons_about_are_measured_and_printed() {
    let ray = say_offline("Samantha", "Excuse me Rich, you can stop counting now.");
    let mic = read("-mic.wav");
    println!(
        "[measured] Ray's sentence, as synthesized: {:.1} dBFS, {:.3} s",
        dbfs(&ray),
        ray.len() as f32 / SAMPLE_RATE as f32
    );
    println!("[measured] the fixture's mic track at the walk's volume: {:.1} dBFS", dbfs(&mic));
    println!("[measured] the CEO's quietest second of room: {:.1} dBFS", dbfs(&room_floor(63)));
    println!("[measured] a_voice(), this file's CEO stand-in: {:.1} dBFS", dbfs(&a_voice(60, 1)));
    println!("[record] Rich's echo residual here: median -48.6, p90 -40.2, p99 -36.2, peak -34.3 dBFS");
    println!("[record] Ray's say-through-the-speakers talk-over, as the app measured it: -44.7 dBFS");
    println!("[record] echo at the mic before cancellation, continuous playback: -40.2 dBFS");
    println!("[record] the VAD's absolute speech floor: 0.005 rms = {:.2} dBFS", 20.0 * 0.005f32.log10());
}

/// **THE SWEEP THAT ANSWERS THE QUESTION.** One near-end talker, superimposed on the microphone
/// 11 s into Rich's audible answer exactly as Ray's was, at every level from far below the echo
/// up to a normal speaking voice. For each: did an `utterance START` happen at all?
#[test]
fn the_level_at_which_a_talker_over_rich_starts_an_utterance() {
    let ray = say_offline("Samantha", "Excuse me Rich, you can stop counting now.");
    let talker_secs = ray.len() as f32 / SAMPLE_RATE as f32;
    println!(
        "[sweep] talker: Ray's own sentence, {talker_secs:.3} s, superimposed 11.0 s into the answer"
    );
    println!(
        "[sweep]         OVER RICH'S AUDIBLE ANSWER          | CONTROL: THE SAME TALKER, RICH SILENT"
    );
    println!(
        "[sweep]  level  start  taint  adm  disc  barge  run | start  adm  disc  run"
    );
    let mut levels: Vec<f32> = (0..=24).map(|i| -52.0 + i as f32).collect();
    levels.extend([-24.0, -20.0, -16.0, -12.0]);
    for level in levels {
        let talker = at_dbfs(ray.clone(), level);

        // Over the answer, exactly where Ray's was: 11.0 s in, Rich still speaking.
        let mut s = Scene::default();
        s.quiet(frames_for(1.5), "the room, settling the VAD floor");
        let answer = s.rich_answers_for(18.0, "Rich answers, out loud");
        s.quiet(frames_for(2.0), "the answer is over");
        let at = answer.start + frames_for(11.0);
        s.near_end_talker(at, &talker);
        let speaking = s.audible_speaking();
        let (o, run) = replay_tracing_onset(&s, &speaking, at..at + frames_for(talker_secs));

        // THE CONTROL. Identical talker, identical position in an identical-length scene, but
        // Rich never plays: the same 18 s is the CEO's own room. The only variable between the
        // two halves of this row is whether Rich is audible, so the difference between them is
        // the whole cost the half-duplex window imposes on being heard.
        let mut c = Scene::default();
        c.quiet(frames_for(1.5), "the room, settling the VAD floor");
        let start_c = c.frames();
        c.quiet(answer.len(), "Rich says nothing at all");
        c.quiet(frames_for(2.0), "still nothing");
        let at_c = start_c + frames_for(11.0);
        c.near_end_talker(at_c, &talker);
        let speaking_c = c.audible_speaking();
        let (oc, run_c) =
            replay_tracing_onset(&c, &speaking_c, at_c..at_c + frames_for(talker_secs));

        println!(
            "[sweep] {level:6.1}  {:5}  {:5}  {:3}  {:4}  {:5}  {run:>3} | {:5}  {:3}  {:4}  {run_c:>3}",
            o.starts,
            o.tainted_starts,
            o.admitted,
            o.tainted_discards + o.other_discards,
            o.barge_ins,
            oc.starts,
            oc.admitted,
            oc.tainted_discards + oc.other_discards,
        );
    }
}

/// **THE PINNING TEST, AND THERE IS NO RED HALF OF IT.**
///
/// The brief this came from asked for the reproduced failure as a red-then-green test. There is
/// no red: at a near-field level the onset already works, over an audible answer, on the CEO's
/// own echo path, with the canceller unconfident. So what is pinned is the pair of facts that
/// together explain Ray's walk without a defect in the onset rule —
///
/// 1. **Ray's talk-over level starts nothing, and that is CORRECT.** It arrived at the
///    microphone at -44.7 dBFS because it came out of the same speakers as Rich, so it is
///    indistinguishable by level from the echo (median -48.6, peak -34.3 dBFS). Anything that
///    fired here would fire on Rich's own voice.
/// 2. **A near-end voice at a near-field level starts a TAINTED utterance.** Heard, so the
///    notice layer is reached and the half-duplex card can fire once; tainted, so nothing of
///    Rich's is ever submitted — the invariant from `c712ccd5` is untouched.
///
/// If a future change to the VAD, the onset or the window breaks (2), this test goes red and the
/// defect Ray looked for will genuinely exist. If a future change makes (1) start an utterance,
/// Rich is about to start transcribing himself again.
#[test]
fn a_near_end_voice_over_richs_answer_starts_a_tainted_utterance_and_is_never_submitted() {
    let ray = say_offline("Samantha", "Excuse me Rich, you can stop counting now.");
    let talker_secs = ray.len() as f32 / SAMPLE_RATE as f32;

    // The level Ray's harness actually reached at the microphone, as the running app measured
    // it during audit-5 — not a level chosen here.
    const HARNESS_THROUGH_THE_SPEAKERS_DBFS: f32 = -44.7;
    // A voice at the microphone. `a_voice`'s own level, which this file has used as the CEO
    // stand-in since the window work, and which `the_levels_...` prints: -24.8 dBFS. Stated as
    // the measurement rather than as a constant so a change to `a_voice` cannot silently
    // weaken this test.
    let near_field_dbfs = dbfs(&a_voice(60, 1));
    assert!(
        near_field_dbfs < -15.0 && near_field_dbfs > -30.0,
        "the CEO stand-in has moved to {near_field_dbfs:.1} dBFS — re-derive this test's premise"
    );

    let scene_with = |level: f32| {
        let mut s = Scene::default();
        s.quiet(frames_for(1.5), "the room, settling the VAD floor");
        let answer = s.rich_answers_for(18.0, "Rich answers, out loud");
        s.quiet(frames_for(2.0), "the answer is over");
        let at = answer.start + frames_for(11.0);
        s.near_end_talker(at, &at_dbfs(ray.clone(), level));
        (s, at)
    };

    // (1) Rich's answer alone, with NO talker at all — the control that proves the scene is not
    // simply starting utterances on its own.
    let mut alone = Scene::default();
    alone.quiet(frames_for(1.5), "the room, settling the VAD floor");
    alone.rich_answers_for(18.0, "Rich answers, out loud");
    alone.quiet(frames_for(2.0), "the answer is over");
    let bare = replay(&alone, &alone.audible_speaking(), None);
    assert_eq!(bare.admitted, 0, "Rich's own answer became a message: {bare:?}");

    // (2) The harness's own talk-over level. Nothing starts, and nothing should: it is the echo.
    let (s, at) = scene_with(HARNESS_THROUGH_THE_SPEAKERS_DBFS);
    let (harness, _) =
        replay_tracing_onset(&s, &s.audible_speaking(), at..at + frames_for(talker_secs));
    assert_eq!(
        harness.starts, bare.starts,
        "audio at the echo's own level started an utterance the bare answer did not — a gate \
         that fires on this fires on Rich: {harness:?} vs {bare:?}"
    );
    assert_eq!(harness.admitted, 0, "{harness:?}");

    // (3) A near-end voice. It MUST be heard, it MUST be born tainted, and it MUST now BARGE IN
    //     and be submitted — which is the opposite of what this step asserted until 2026-09-18.
    //
    // **THE ASSERTIONS HERE USED TO READ `admitted == 0` AND `barge_ins == 0`, AND THEY WERE A
    // FAITHFUL DESCRIPTION OF THE DEFECT.** They said: a person at a near-field level talking
    // over Rich for 2.841 s is heard, tainted, discarded, and does not interrupt him — because
    // the unconfident-canceller rule wanted 5.008 s of UNBROKEN speech. The CEO met exactly that
    // behavior on 2026-09-18 and said *"no matter what I said, he couldn't hear me"*. He was
    // heard, five times, at -18.1 dBFS against a -46.7 dBFS echo on the same microphone, and
    // thrown away every time; his longest unbroken run was 66 frames = 1.056 s against 313.
    //
    // Barge-in is now gated on the canceller's per-frame NEAR-END VERDICT rather than on
    // `confident()` (`bargein::BargeInMonitor::set_near_end_gated`), so 2.841 s of near-field
    // speech reaches the near-end threshold and fires. A barge-in is a positive signal that the CEO
    // is the one talking, so `CaptureBrain` clears the taint and the words become his next turn —
    // that clause is not new, it has simply never been reachable on this path before.
    //
    // **THE INVARIANT THIS FILE EXISTS FOR IS UNTOUCHED, AND IT IS STEPS (1) AND (2) THAT HOLD
    // IT, NOT THIS ONE.** Rich's own answer alone admits nothing, and audio at the ECHO's own
    // level (-44.7 dBFS, the level Ray's `say`-through-the-speakers talk-over actually reached)
    // admits nothing and starts nothing. Both still pass, unchanged, and the false-positive side
    // is measured on real recordings in `barge_in_on_the_ceos_rig`: zero fires over fourteen
    // echo-only conditions spanning two recordings and six volumes.
    let (s, at) = scene_with(near_field_dbfs);
    let (near, run) =
        replay_tracing_onset(&s, &s.audible_speaking(), at..at + frames_for(talker_secs));
    assert_eq!(
        near.starts,
        bare.starts + 1,
        "a voice at {near_field_dbfs:.1} dBFS over Rich's answer started NO utterance — this is \
         the defect the .6 walk was read as showing, and it is now real: {near:?}"
    );
    assert_eq!(
        near.tainted_starts,
        bare.tainted_starts + 1,
        "the utterance was not born tainted, so Rich's own voice could be submitted: {near:?}"
    );
    assert_eq!(
        near.barge_ins, 1,
        "a person talking over Rich for {talker_secs:.3} s at {near_field_dbfs:.1} dBFS did NOT \
         interrupt him — this is the CEO's 2026-09-18 report, reproduced: {near:?}"
    );
    assert_eq!(
        near.admitted,
        bare.admitted + 1,
        "he interrupted Rich and his words were still thrown away, so the interruption cost him \
         a turn instead of winning one: {near:?}"
    );
    assert_eq!(
        near.tainted_discards, 0,
        "the barge-in did not clear the taint, so the words that stopped Rich were discarded \
         anyway: {near:?}"
    );
    assert!(
        run >= SPEECH_ONSET_FRAMES - 1,
        "the onset run only reached {run} of the {SPEECH_ONSET_FRAMES} frames it needs"
    );
}

/// **WHICH WAY THIS REPLAY ERRS, MEASURED.** `mic_gain_for_the_walks_volume` scales the whole
/// microphone track, which scales the CEO's room noise with it — his room sits at -67.9 dBFS
/// live (`docs/verification/2026-09-17-aec-erle-on-the-ceo-rig.md:86`) and at a good deal more
/// than that here. A louder room means a higher adapted floor, which means a HIGHER onset bar
/// than the live app has. So every level in the sweep above is conservative, and the conclusion
/// drawn from it — that a near-field voice clears the onset comfortably — is safe in the right
/// direction. Asserted rather than asserted-in-prose.
#[test]
fn the_replays_room_is_louder_than_the_ceos_so_its_onset_bar_is_the_pessimistic_one() {
    let replayed_room = dbfs(&room_floor(63));
    const LIVE_ROOM_DBFS: f32 = -67.9;
    assert!(
        replayed_room > LIVE_ROOM_DBFS,
        "the replay's room ({replayed_room:.1} dBFS) is now QUIETER than the CEO's \
         ({LIVE_ROOM_DBFS} dBFS), so the sweep is no longer the pessimistic bound it is read as"
    );

    // What the VAD actually settles to on each. `noise_floor * speech_ratio` versus the absolute
    // floor is the whole of it: at his real room level the ratio term is far under the absolute
    // floor, so the live threshold is PINNED at -46.02 dBFS and cannot be raised by the room.
    let settled = |room: &[f32]| {
        let mut vad = Vad::default();
        for f in room.chunks_exact(VAD_FRAME_SAMPLES) {
            vad.push_frame(f);
        }
        20.0 * vad.speech_threshold().max(1e-12).log10()
    };
    let here = settled(&room_floor(200));
    let live_room: Vec<f32> = {
        let quiet = room_floor(200);
        let k = 10f32.powf(LIVE_ROOM_DBFS / 20.0) / rms(&quiet).max(1e-12);
        quiet.iter().map(|s| s * k).collect()
    };
    let live = settled(&live_room);
    println!(
        "[measured] VAD speech threshold after settling: {here:.2} dBFS on the replay's room \
         ({replayed_room:.1} dBFS), {live:.2} dBFS on the CEO's ({LIVE_ROOM_DBFS} dBFS) — the \
         replay's onset bar is {:.2} dB higher than the live one",
        here - live
    );
    assert!(here > live, "{here} vs {live}");
    assert!(
        (live - 20.0 * 0.005f32.log10()).abs() < 0.01,
        "at the CEO's room level the threshold should be pinned at the absolute floor, got {live}"
    );
}

/// `replay`, plus the peak onset run the endpointer reached inside `window` — the difference
/// between *the VAD never called it speech* and *it was speech but never 7 frames in a row*.
fn replay_tracing_onset(
    scene: &Scene,
    speaking: &[bool],
    window: std::ops::Range<usize>,
) -> (Outcome, u32) {
    let (aec, ring) = EchoCanceller::new();
    let mut brain = CaptureBrain::with_aec(aec);
    let mut out = Outcome::default();
    let mut peak = 0u32;
    for i in 0..scene.frames() {
        let lo = i * AEC_BLOCK;
        ring.push(&scene.reference[lo..lo + AEC_BLOCK]);
        let msgs = brain.push_frame(&scene.mic[lo..lo + AEC_BLOCK], speaking[i], false);
        tally(&msgs, &mut out);
        if window.contains(&i) {
            peak = peak.max(brain.onset_run_frames());
        }
    }
    assert!(!brain.aec_confident(), "the canceller became confident on the CEO's own echo path");
    (out, peak)
}

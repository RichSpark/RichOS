//! **THE CEO SAYS A SENTENCE OVER RICH AND RICH STOPS — pinned on his own recordings.**
//!
//! On 2026-09-18 the CEO tested barge-in on candidate .7 at his desk and said *"didn't work … no
//! matter what I said, he couldn't hear me"* — *"it all works re audio but not while Rich is
//! talking."* He was heard. The candidate's own log printed six utterances during one answer,
//! every one `tainted=true`, every one discarded, and the reason it gave was the true one: the
//! echo canceller could not vouch for the audio, so the fallback rule was in force, and that rule
//! wanted **5.008 s of UNBROKEN speech** (313 x 256 / 16000, re-derived in `bargein.rs`).
//!
//! Nobody speaks for five seconds without a gap. The debounce was never a discrimination between
//! two voices — it is a continuity race that echo happens to lose, and the CEO loses it too.
//!
//! ## The two tests this file is, and why they are one file
//!
//! A rule that stops Rich has exactly two ways to be wrong, they pull in opposite directions, and
//! the CEO has ruled on the trade: *a regression that makes Rich cut himself off mid-sentence is
//! worse than the current cost.* So both are pinned here, on real recordings of his rig, so a
//! future change cannot improve one without the other failing:
//!
//! | test | asks | fails if |
//! |---|---|---|
//! | [`richs_own_echo_never_interrupts_him_at_any_volume`] | the FALSE-POSITIVE side | Rich cuts himself off |
//! | [`the_ceos_own_voice_over_rich_interrupts_him_and_his_words_are_kept`] | the MISS side | the CEO is ignored |
//!
//! Each carries the other's positive control, so neither can pass by doing nothing: the
//! false-positive test proves a barge-in IS reachable on the same recording, and the miss test
//! names the echo-only recording of the same passage at the same volume that fires nothing.
//!
//! ## The fixtures — recorded once each, live, and never again
//!
//! ```text
//!   ceo-rig-2026-09-17            9.536 s   echo only, the CEO's morning volume
//!   ceo-rig-2026-09-18-nearend   25.579 s   echo only, output volume 60 — HIS FAILING TEST'S VOLUME
//!   ceo-rig-2026-09-18-nearend2  24.784 s   THE CEO'S OWN VOICE over Rich, 4.880 s of it
//! ```
//!
//! **ONE OF THOSE SEVEN FILES IS NOT IN THIS REPOSITORY.** `-nearend2-mic.wav` is the microphone
//! track of the third pair and the CEO is audible in it; `richos` is public, so it is committed to
//! the private `richos-hq` repository at `fixtures/echo-path/` and resolved at run time by
//! [`richos_voice::private_fixtures`]. Its `-reference.wav` and both sidecars stay here — the
//! reference is the synthetic `say` signal that went to the DAC, and the sidecars carry his
//! intervals and his levels and not one word he said. Without the private file
//! [`the_ceos_own_voice_over_rich_interrupts_him_and_his_words_are_kept`] reports `ignored` with a
//! reason naming it; the other two tests in this file run everywhere and are unaffected.
//!
//! `-nearend2` is the one that matters and it exists because §53 (richos-hq
//! `wiki/ceo-decisions.md`, 2026-09-18) is right: the Mac's own speakers cannot stand in for the
//! person. Anything played through them takes the same acoustic path as Rich's echo and is
//! indistinguishable from it — Ray's `say`-through-the-speakers talk-over reached the microphone
//! at **-44.7 dBFS**, inside the echo's own distribution. The CEO reached **-18.1 dBFS** against
//! a **-46.7 dBFS** echo on the same microphone: **28.6 dB** above it. Those are not the same
//! experiment and only one of them is barge-in.
//!
//! **Nothing in this file plays a sound.** All three fixtures were recorded by
//! `examples/aec_capture --save`, one playback each, at output volume 60 read and never set.

use richos_voice::aec::EchoCanceller;
use richos_voice::bargein::{
    BargeInMode, AEC_BARGE_IN_REQUIRED_FRAMES, AEC_BARGE_IN_WINDOW_FRAMES,
    BARGE_IN_DEBOUNCE_FRAMES,
};
use richos_voice::controller::{CapMsg, CaptureBrain};
use richos_voice::private_fixtures;
use richos_voice::vad::{frames_to_secs, SAMPLE_RATE, VAD_FRAME_SAMPLES};
use richos_voice::wav;
use std::path::{Path, PathBuf};

const ECHO_ONLY_MORNING: &str = "tests/fixtures/echo-path/ceo-rig-2026-09-17";
const ECHO_ONLY_VOL60: &str = "tests/fixtures/echo-path/ceo-rig-2026-09-18-nearend";
const HIS_VOICE: &str = "tests/fixtures/echo-path/ceo-rig-2026-09-18-nearend2";

/// **THE `-nearend2` PAIR IS SPLIT ACROSS TWO REPOSITORIES, AND ONLY THE MICROPHONE TRACK MOVES.**
///
/// `-nearend2-mic.wav` is the CEO's own voice. `richos` is public, so it is committed to the
/// private `richos-hq` repository instead and found through [`private_fixtures`]; its sidecars and
/// its `-reference.wav` (the synthetic `say` signal that went to the DAC) stay here, because
/// nothing in them is him. The split changes nothing this file measures — the same bytes are
/// opened, from a different directory — and the one test that needs those bytes reports
/// `ignored, PRIVATE FIXTURE …` rather than `ok` on a machine that does not have them.
///
/// **`richs_own_echo_never_interrupts_him_at_any_volume` needs none of this** and runs everywhere:
/// its two recordings contain no near-end voice, which is why they are public in the first place.
const HIS_VOICE_MIC_IS_PRIVATE: &str = private_fixtures::CEO_MIC_TRACK;

/// **WHERE THE CEO ACTUALLY SPEAKS IN `-nearend2`, and how those numbers were arrived at.**
///
/// Not by ear and not by eye. Per 256-sample frame, microphone RMS minus the reference
/// PEAK-DECAY envelope in dB — the same x0.72 decay per block the canceller itself uses, because
/// echo follows the reference envelope and not its instantaneous level. That ratio is a property
/// of the acoustic path, so on a recording with no near-end voice it is bounded: measured over the
/// 1441 far-active frames of `-nearend` (the same rig, the same passage, the same output volume
/// 60, recorded 14 minutes earlier with nobody speaking) it is p50 -26.9, p90 -18.2, p99 -8.3,
/// **max -2.5 dB**. 205 frames of `-nearend2` exceed that MAXIMUM, bridged across gaps of up to
/// 0.400 s so one sentence is not split by its own inter-word pauses.
///
/// The full derivation, and what it deliberately gets wrong, is in the committed sidecar
/// `-nearend2-nearend.json`. The short version of the caveat: his QUIETEST speech (word onsets,
/// trailing syllables, breath) sits below the baseline maximum and is therefore NOT inside these
/// intervals, so **the complement of this list is not a clean echo-only set and this file never
/// uses it as one.** False positives are measured only on the two recordings that contain no
/// near-end voice at all.
const HIS_INTERVALS: &[(f32, f32)] = &[
    (3.568, 5.472),
    (6.256, 7.360),
    (14.320, 15.152),
    (16.384, 16.672),
    (20.912, 21.664),
];

fn read(prefix: &str, suffix: &str) -> Vec<f32> {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push(format!("{prefix}{suffix}"));
    read_wav(&p)
}

fn read_wav(p: &Path) -> Vec<f32> {
    let bytes = std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    let pcm = wav::read_pcm16(&bytes).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    assert_eq!(pcm.sample_rate, SAMPLE_RATE, "{} is not 16 kHz", p.display());
    wav::to_mono(&pcm.samples, pcm.channels)
}

/// **The CEO's microphone track, from the private repository.** Panics with the full reason if it
/// is absent — see [`private_fixtures::require_ceo_mic_track`] for why a panic and not a `return`.
/// Every caller is behind `#[cfg_attr(not(private_fixtures), ignore = …)]`, so this is reached on
/// a machine without it only via `--include-ignored`.
fn read_his_mic() -> Vec<f32> {
    let p = private_fixtures::require_ceo_mic_track();
    println!("[fixture] the CEO's microphone track, private: {}", p.display());
    read_wav(&p)
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

fn dbfs(x: &[f32]) -> f32 {
    20.0 * rms(x).max(1e-12).log10()
}

/// Render one sentence with `say -o <file>` — **A FILE. This makes no sound at any volume**, and
/// it is the same mechanism `self_voice_replay.rs` and `voiced_acceptance.rs` use, for the same
/// reason: a hand-built model of a voice can be fitted to its own detector.
fn say_offline(voice: &str, text: &str) -> Vec<f32> {
    let dir = std::env::temp_dir().join(format!("richos-bargein-rig-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{}.wav", voice.replace(' ', "_")));
    let out = std::process::Command::new("/usr/bin/say")
        .args(["-v", voice, "-o"])
        .arg(&path)
        .arg("--data-format=LEI16@16000")
        .arg(text)
        .output()
        .unwrap_or_else(|e| panic!("could not run /usr/bin/say: {e}"));
    assert!(out.status.success(), "`say -v {voice}` failed: {}", String::from_utf8_lossy(&out.stderr));
    let bytes = std::fs::read(&path).unwrap();
    let pcm = wav::read_pcm16(&bytes).unwrap();
    assert_eq!(pcm.sample_rate, SAMPLE_RATE);
    let _ = std::fs::remove_file(&path);
    // And the DIRECTORY, not just the file — it left an empty one per test process behind in
    // $TMPDIR, forever. `remove_dir` only succeeds when the directory is empty, so it can never
    // take anything else with it, and `create_dir_all` above re-creates it for the next call.
    let _ = std::fs::remove_dir(&dir);
    wav::to_mono(&pcm.samples, pcm.channels)
}

fn at_dbfs(mut xs: Vec<f32>, target: f32) -> Vec<f32> {
    let k = 10f32.powf(target / 20.0) / rms(&xs).max(1e-12);
    for x in xs.iter_mut() {
        *x *= k;
    }
    xs
}

/// **THE VOLUME KNOB, DERIVED AT RUN TIME RATHER THAN TYPED.** Returns the scalar that, applied
/// to the microphone track alone, puts the post-cancellation residual at `target_dbfs`. Scaling
/// the mic and not the reference is exactly what turning the speakers up does to a recorded pair:
/// the echo path, its coherence and its cancellability are unchanged by a scalar.
///
/// Same construction as `self_voice_replay.rs::mic_gain_for_the_walks_volume`, so the two files
/// agree by sharing a method rather than by sharing a constant.
fn mic_gain_for_residual(mic: &[f32], reference: &[f32], target_dbfs: f32) -> f32 {
    let (mut aec, ring) = EchoCanceller::new();
    let n = mic.len().min(reference.len());
    let (mut power, mut blocks) = (0.0f64, 0usize);
    for b in 0..(n / VAD_FRAME_SAMPLES) {
        let lo = b * VAD_FRAME_SAMPLES;
        ring.push(&reference[lo..lo + VAD_FRAME_SAMPLES]);
        let mut buf = [0.0f32; VAD_FRAME_SAMPLES];
        buf.copy_from_slice(&mic[lo..lo + VAD_FRAME_SAMPLES]);
        aec.process_block(&mut buf);
        if aec.last_block().reference_rms > richos_voice::aec::FAR_END_ACTIVE_RMS {
            power += (rms(&buf) * rms(&buf)) as f64;
            blocks += 1;
        }
    }
    assert!(blocks > 0, "the reference is never active — this is not an echo-path pair");
    10f32.powf(target_dbfs / 20.0) / (power / blocks as f64).sqrt().max(1e-9) as f32
}

/// What one full-pipeline replay observed. Counts, and WHERE the first barge-in landed, because
/// "it fired" and "it fired inside the sentence he actually said" are different claims.
#[derive(Debug, Default)]
struct Outcome {
    starts: usize,
    barge_ins: usize,
    admitted: usize,
    tainted_discards: usize,
    other_discards: usize,
    first_barge_frame: Option<usize>,
    confident_ever: bool,
    mode_ever_consecutive: bool,
}

/// **Drive the SHIPPED `CaptureBrain` over a recorded pair, frame by frame.** Not a model of the
/// pipeline: the real canceller, the real VAD, the real endpointer, the real monitor, in the
/// coordinate system `aec_capture` recorded the pair in.
///
/// `speaking` is Rich-is-audible, and it is derived the way the app derives it rather than set to
/// `true` throughout: the window opens on the first frame the reference is active and stays open
/// through the last one plus the audible tail, so the inter-word gaps inside Rich's own answer are
/// INSIDE the window. That is the `AudibleWindow` property `self_voice_replay.rs` established, and
/// getting it wrong in the lenient direction would hand this test an easier problem.
fn replay(mic: &[f32], reference: &[f32], extra: Option<&[f32]>, at_frame: usize) -> Outcome {
    let mut mic = mic.to_vec();
    if let Some(sig) = extra {
        let lo = at_frame * VAD_FRAME_SAMPLES;
        assert!(lo + sig.len() <= mic.len(), "the talker runs off the end of the recording");
        for (i, s) in sig.iter().enumerate() {
            mic[lo + i] += s;
        }
    }
    let n = mic.len().min(reference.len());
    let frames = n / VAD_FRAME_SAMPLES;

    // Where the reference is active, established in one pass so `speaking` can span the gaps.
    let active: Vec<bool> = (0..frames)
        .map(|b| {
            let lo = b * VAD_FRAME_SAMPLES;
            rms(&reference[lo..lo + VAD_FRAME_SAMPLES]) > richos_voice::aec::FAR_END_ACTIVE_RMS
        })
        .collect();
    let first = active.iter().position(|a| *a).expect("the reference is never active");
    let last = frames - 1 - active.iter().rev().position(|a| *a).unwrap();
    // The measured device tail on this rig: 512 frames / 48 000 Hz = 10.667 ms, so one VAD frame
    // covers it. Rounded UP to a whole frame, never down.
    let tail_frames = 1usize;

    let (aec, ring) = EchoCanceller::new();
    let mut brain = CaptureBrain::with_aec(aec);
    let mut out = Outcome::default();
    for b in 0..frames {
        let lo = b * VAD_FRAME_SAMPLES;
        ring.push(&reference[lo..lo + VAD_FRAME_SAMPLES]);
        let speaking = b >= first && b <= last + tail_frames;
        for m in brain.push_frame(&mic[lo..lo + VAD_FRAME_SAMPLES], speaking, false) {
            match m {
                CapMsg::Started { .. } => out.starts += 1,
                CapMsg::BargeIn { .. } => {
                    out.barge_ins += 1;
                    if out.first_barge_frame.is_none() {
                        out.first_barge_frame = Some(b);
                    }
                }
                CapMsg::Utterance(_) => out.admitted += 1,
                CapMsg::Discarded { tainted } => {
                    if tainted {
                        out.tainted_discards += 1
                    } else {
                        out.other_discards += 1
                    }
                }
                _ => {}
            }
        }
        out.confident_ever |= brain.aec_confident();
        out.mode_ever_consecutive |= brain.barge_in_mode() == BargeInMode::Consecutive;
    }
    out
}

/// The six volume settings every false-positive claim here is made across. Derived targets for the
/// post-cancellation residual, spanning the two levels that were actually measured on this rig and
/// going 4 dB beyond the louder of them, because a rule that only survives the loudest level ever
/// recorded has no headroom.
///
/// ```text
///   -52.00  the 2026-09-17 pair as recorded
///   -49.50  the 2026-09-18 pair as recorded, output volume 60 — the CEO's failing test
///   -47.00  between them
///   -45.25  Ray's walk at output volume 85 (the app printed -45.8 and -44.7 dBFS; this is the mean)
///   -43.00  louder still
///   -41.00  4 dB above anything ever measured here
/// ```
///
/// The VAD's absolute speech floor is 0.005 rms = -46.02 dBFS, so this range STRADDLES it: below
/// it Rich's residual cannot trip a speech frame and above it it can. A rule chosen only on one
/// side of that crossing would be chosen against the easy case.
const VOLUMES: &[f32] = &[-52.00, -49.50, -47.00, -45.25, -43.00, -41.00];

/// **THE FALSE-POSITIVE SIDE — and the CEO has ruled that this is the side that matters more:**
/// *a regression that makes Rich cut himself off mid-sentence is worse than the current cost.*
///
/// Two independent recordings of his rig with **no near-end voice of any kind in them**, each
/// replayed at six volumes. Fourteen conditions. Rich must never interrupt himself, and none of
/// his own voice may ever be submitted as the CEO's message — which is the `c712ccd5` invariant
/// this whole fixture family exists for.
///
/// **THE POSITIVE CONTROL IS THE LAST BLOCK, and without it this test could pass by doing
/// nothing:** the same recording, at the same volume, with a near-field voice summed into the
/// microphone track, DOES barge in. So the zeroes above are the rule declining, not the harness
/// failing to drive it.
#[test]
fn richs_own_echo_never_interrupts_him_at_any_volume() {
    for prefix in [ECHO_ONLY_MORNING, ECHO_ONLY_VOL60] {
        let mic0 = read(prefix, "-mic.wav");
        let reference = read(prefix, "-reference.wav");
        for &vol in VOLUMES {
            let g = mic_gain_for_residual(&mic0, &reference, vol);
            let mic: Vec<f32> = mic0.iter().map(|s| s * g).collect();
            let o = replay(&mic, &reference, None, 0);
            println!(
                "[measured] {prefix} at residual {vol:.2} dBFS (mic x{g:.4} = {:+.1} dB): {o:?}",
                20.0 * g.log10()
            );
            assert_eq!(
                o.barge_ins, 0,
                "RICH CUT HIMSELF OFF. {prefix} at residual {vol:.2} dBFS, nobody in the room: {o:?}"
            );
            assert_eq!(
                o.admitted, 0,
                "Rich's own voice was submitted as the CEO's message. {prefix} at {vol:.2} dBFS: {o:?}"
            );
            // **CONFIDENCE IS REPORTED HERE AND DELIBERATELY NOT ASSERTED, AND FINDING THAT OUT
            // CORRECTED SOMETHING I BELIEVED.** This assertion originally read
            // `assert!(!o.confident_ever)`, on the strength of the record's statement that
            // `confident()` is unreachable on this hardware. It failed on the first condition it
            // reached: the 2026-09-18 pair scaled DOWN to a -52.00 dBFS residual DOES become
            // confident, because `CONFIDENT_LEAK_RMS` is -52.04 dBFS and that target sits a
            // hair's breadth from it.
            //
            // So "unreachable on this hardware" is true at the volumes the CEO and Ray actually
            // used and is NOT a property of the hardware — it is a property of the hardware AND
            // the volume knob. That is worth having straight, and it changes nothing about safety:
            // confidence only ever makes the TAINT rule more permissive, and since 2026-09-18
            // barge-in does not consult it at all. Which is exactly why the two assertions that
            // matter — no barge-in, nothing admitted — are made across every volume regardless of
            // what confidence happened to do.
            println!("[measured]   confident_ever={} at this volume", o.confident_ever);
            assert!(
                !o.mode_ever_consecutive,
                "the monitor fell back to the {BARGE_IN_DEBOUNCE_FRAMES}-frame consecutive rule \
                 with a canceller present, which is the behavior that ignored the CEO: {o:?}"
            );
        }
    }

    // ---- THE POSITIVE CONTROL -------------------------------------------------------------
    // Same recording, same volume, one near-field voice summed into the MICROPHONE track and
    // nothing else (the reference is untouched, so the canceller holds no copy of it). If this
    // does not fire, every zero above is meaningless.
    let mic0 = read(ECHO_ONLY_VOL60, "-mic.wav");
    let reference = read(ECHO_ONLY_VOL60, "-reference.wav");
    let g = mic_gain_for_residual(&mic0, &reference, -45.25);
    let mic: Vec<f32> = mic0.iter().map(|s| s * g).collect();
    let talker = at_dbfs(
        say_offline("Samantha", "Rich, hold on a moment, that is not what I asked you."),
        -22.0,
    );
    let at = (12.0 * SAMPLE_RATE as f32 / VAD_FRAME_SAMPLES as f32) as usize;
    let control = replay(&mic, &reference, Some(&talker), at);
    println!("[control] the same recording with a -22.0 dBFS near-field voice at 12.0 s: {control:?}");
    assert!(
        control.barge_ins >= 1,
        "the positive control did not fire, so the fourteen zeroes above prove nothing: {control:?}"
    );
}

/// **THE MISS SIDE — the CEO's acceptance criterion, in his own voice, on his own rig.**
///
/// *"the CEO says one ordinary sentence over Rich, at his normal voice, from where he sits, on Mac
/// mini speakers with the Elgato Wave:3 across the desk, and Rich stops."*
///
/// He did exactly that at 08:56:22Z on 2026-09-18, into `-nearend2`. This replays that recording
/// through the shipped pipeline and requires a barge-in inside the first thing he said — and
/// requires his words to survive it, because an interruption that stops Rich and then throws the
/// interrupting words away has cost him a turn instead of winning one.
///
/// **THE NEGATIVE CONTROL IS A WHOLE OTHER TEST AND IT IS NAMED HERE ON PURPOSE:**
/// `richs_own_echo_never_interrupts_him_at_any_volume` replays the SAME PASSAGE at the SAME
/// VOLUME on the SAME RIG recorded 14 minutes earlier with nobody speaking, and fires nothing.
/// One recording differs from the other by one thing — him — and so does the verdict.
///
/// **THE RECORDING OF HIM IS NOT IN THIS REPOSITORY.** It is his voice and `richos` is public, so
/// `-nearend2-mic.wav` lives in the private `richos-hq` repository. Without it this test reports
/// `ignored, PRIVATE FIXTURE: …` and is counted in libtest's own `N ignored` column — never `ok`,
/// because a test that did not run must not print `ok` (`build.rs`). To run it, point
/// `RICHOS_PRIVATE_FIXTURES` at `richos-hq/fixtures/echo-path`.
#[test]
#[cfg_attr(
    not(private_fixtures),
    ignore = "PRIVATE FIXTURE: ceo-rig-2026-09-18-nearend2-mic.wav is a recording of the CEO's \
              own voice and is not committed to this public repository. It is in the private \
              richos-hq repository at fixtures/echo-path/. Set RICHOS_PRIVATE_FIXTURES to that \
              directory to run this."
)]
fn the_ceos_own_voice_over_rich_interrupts_him_and_his_words_are_kept() {
    let mic = read_his_mic();
    let reference = read(HIS_VOICE, "-reference.wav");
    // The pair shares one index (`-nearend2-session.json`: "reference[i] is what went to the
    // speakers for mic[i]"), and the two halves now come from two different repositories. If they
    // ever stop being the same recording, every number below is about nothing — so it is checked
    // here rather than assumed, and it is the cheapest check in the file.
    assert_eq!(
        mic.len(),
        reference.len(),
        "the private microphone track ({} samples) and the committed reference track ({} \
         samples) are not the same recording — a wrong or stale file is in {}",
        mic.len(),
        reference.len(),
        private_fixtures::ENV_DIR
    );

    // THE PREMISE, RE-DERIVED RATHER THAN QUOTED: his voice really is in this recording, and the
    // separation really is what the sidecar says. If a fixture is ever replaced these fail first
    // and say so, instead of the verdict below quietly becoming a statement about something else.
    let his: Vec<f32> = HIS_INTERVALS
        .iter()
        .flat_map(|(a, b)| {
            mic[(a * SAMPLE_RATE as f32) as usize..(b * SAMPLE_RATE as f32) as usize].to_vec()
        })
        .collect();
    let his_dbfs = dbfs(&his);
    let echo_mic = read(ECHO_ONLY_VOL60, "-mic.wav");
    let echo_dbfs = dbfs(&echo_mic[(1.5 * SAMPLE_RATE as f32) as usize..(24.9 * SAMPLE_RATE as f32) as usize]);
    let total: f32 = HIS_INTERVALS.iter().map(|(a, b)| b - a).sum();
    println!(
        "[measured] the CEO at the microphone {his_dbfs:.1} dBFS over {total:.3} s in {} intervals; \
         Rich's echo alone on the same rig, same volume, same passage {echo_dbfs:.1} dBFS; \
         separation {:.1} dB",
        HIS_INTERVALS.len(),
        his_dbfs - echo_dbfs
    );
    assert!(
        his_dbfs - echo_dbfs > 20.0,
        "the separation between the CEO and Rich's echo in this fixture has fallen to \
         {:.1} dB — re-derive this test's premise before trusting its verdict",
        his_dbfs - echo_dbfs
    );

    let o = replay(&mic, &reference, None, 0);
    println!("[measured] the full pipeline over his recording: {o:?}");

    // (1) RICH STOPS. This is the sentence the CEO said did not happen.
    assert!(
        o.barge_ins >= 1,
        "the CEO talked over Rich for {total:.3} s at {his_dbfs:.1} dBFS and Rich did not stop — \
         this is his 2026-09-18 report, reproduced: {o:?}"
    );

    // (2) HE STOPS HIM DURING SOMETHING HE ACTUALLY SAID, not at some unrelated moment. The
    //     window allows the debounce's own length after his first interval opens, because a rule
    //     that needs 0.240 s of evidence cannot fire before it has seen 0.240 s of him.
    let first = o.first_barge_frame.expect("barge_ins >= 1 but no frame was recorded");
    let at_secs = frames_to_secs(first as u32);
    let (open, close) = HIS_INTERVALS[0];
    // **THE WINDOW OPENS EARLIER THAN `open`, AND IT HAS TO.** `HIS_INTERVALS` marks only the
    // frames that clear the echo-only baseline's MAXIMUM, so it necessarily starts LATE: the
    // quiet leading edge of his first word is real speech that the marker does not claim. The
    // rule may therefore legitimately have its 15 frames of evidence before `open`, which is why
    // the lower bound below is `open` minus one window and not `open` itself.
    let earliest = open - frames_to_secs(AEC_BARGE_IN_WINDOW_FRAMES);
    let deadline = close + frames_to_secs(AEC_BARGE_IN_WINDOW_FRAMES);
    println!("[measured] first barge-in at {at_secs:.3} s; his marked speech opens at {open:.3} s");
    assert!(
        at_secs >= earliest && at_secs <= deadline,
        "the barge-in landed at {at_secs:.3} s, outside his first interval {open:.3}..{close:.3} s \
         with one {:.3} s window of slack at each end — so it fired on something other than him",
        frames_to_secs(AEC_BARGE_IN_WINDOW_FRAMES)
    );

    // (3) AND IT IS FAST — which is the entire complaint being answered.
    //
    // **THIS ASSERTION USED TO HAVE A LOWER BOUND OF 0.240 s AND THAT WAS WRONG, for the reason
    // above.** Measured latency from the conservative marker is 0.176 s = 11 frames, which is
    // FEWER than the 15 frames of evidence the rule requires — not because anything
    // short-circuits the window, but because he was already speaking 4 frames before the marker
    // admits it. A floor measured from a deliberately-late reference point is not a floor. What
    // is worth pinning is the ceiling, because that is the CEO's actual complaint.
    let latency = at_secs - open;
    println!(
        "[measured] barge-in {latency:.3} s after his marked speech opens. The rule's own evidence \
         requirement is {:.3} s ({AEC_BARGE_IN_REQUIRED_FRAMES} x 256 / 16000) and the rule that \
         ignored him needed {:.3} s ({BARGE_IN_DEBOUNCE_FRAMES} x 256 / 16000)",
        frames_to_secs(AEC_BARGE_IN_REQUIRED_FRAMES),
        frames_to_secs(BARGE_IN_DEBOUNCE_FRAMES)
    );
    assert!(
        latency < 1.0,
        "it took {latency:.3} s to notice him. The CEO's test was one ordinary sentence; a rule \
         that needs most of a second of it is the old defect with a smaller number"
    );

    // (4) HIS WORDS SURVIVE. A barge-in clears the taint, so what he said becomes his next turn.
    //
    // **`>= 1` AND NOT `== 5`, AND THE REASON IS A LIMIT OF THE REPLAY, NOT OF THE FIX.** He spoke
    // five times and this replay admits ONE of them, with three tainted discards after it. That is
    // correct here and would not happen live. `BargeInMonitor` fires at most once per armed period
    // and the armed period is the whole answer — in the running app the first barge-in STOPS
    // playout, so `speaking` falls, the monitor disarms, and everything he says next is an ordinary
    // utterance admitted on the normal path. A recording cannot stop: the reference plays to the
    // end regardless, so Rich stays "audible" for another 18 s that he would never have spent
    // talking, and his later sentences keep meeting the half-duplex taint rule.
    //
    // So the three later discards are the fixture's fault and the one barge-in is the answer to
    // his complaint. Pinning `== 1` here would pin the artifact.
    assert!(
        o.admitted >= 1,
        "he stopped Rich and every word he said was still discarded, so interrupting cost him a \
         turn instead of winning one: {o:?}"
    );
    println!(
        "[note] {} of his {} sentences admitted in replay, {} tainted discards after the barge-in. \
         Live, the barge-in stops playout and disarms the monitor, so the rest arrive as ordinary \
         utterances; a recording plays to the end and cannot.",
        o.admitted,
        HIS_INTERVALS.len(),
        o.tainted_discards
    );

    // (5) AND NONE OF IT DEPENDED ON CONFIDENCE, which is the whole change. On this hardware the
    //     coherence of the echo path caps any linear canceller at 4.3-5.5 dB, so `confident()` is
    //     unreachable — and that used to route his rig to the 5.008 s continuity race.
    assert!(
        !o.confident_ever,
        "the canceller became confident, so this is no longer a test of the case the CEO is in: {o:?}"
    );
    assert!(
        !o.mode_ever_consecutive,
        "the monitor used the {BARGE_IN_DEBOUNCE_FRAMES}-frame consecutive rule at some point with \
         a canceller present: {o:?}"
    );
}

/// INVARIANT: every duration this file reasons about is the frame math, re-derived here from the
/// constants rather than copied out of a comment — the bar `bargein.rs` sets for itself.
#[test]
fn every_duration_in_this_file_is_the_frame_math() {
    assert_eq!(SAMPLE_RATE, 16_000);
    assert_eq!(VAD_FRAME_SAMPLES, 256);
    // 25 x 256 / 16000 = 0.400 s exactly.
    assert_eq!(AEC_BARGE_IN_WINDOW_FRAMES, 25);
    assert!((frames_to_secs(AEC_BARGE_IN_WINDOW_FRAMES) - 0.400).abs() < 1e-6);
    // 15 x 256 / 16000 = 0.240 s exactly.
    assert_eq!(AEC_BARGE_IN_REQUIRED_FRAMES, 15);
    assert!((frames_to_secs(AEC_BARGE_IN_REQUIRED_FRAMES) - 0.240).abs() < 1e-6);
    // 313 x 256 / 16000 = 5.008 s exactly — the rule that ignored the CEO, now reached only
    // when there is no canceller at all.
    assert_eq!(BARGE_IN_DEBOUNCE_FRAMES, 313);
    assert!((frames_to_secs(BARGE_IN_DEBOUNCE_FRAMES) - 5.008).abs() < 1e-6);
    // The improvement, stated as the ratio it is: 5.008 / 0.400 = 12.52x.
    let ratio = frames_to_secs(BARGE_IN_DEBOUNCE_FRAMES) / frames_to_secs(AEC_BARGE_IN_WINDOW_FRAMES);
    assert!((ratio - 12.52).abs() < 0.01, "{ratio}");

    // And the intervals this file pins are inside the fixture they name.
    //
    // **MEASURED ON THE `-reference` TRACK, WHICH IS PUBLIC, SO THIS RUNS EVERYWHERE.** The
    // microphone half of this pair is the CEO's voice and lives in the private repository; its
    // LENGTH is not a private fact, and the two halves are one recording sharing one index
    // (`-nearend2-session.json`). The substitution is therefore exact, and it is not taken on
    // trust: when the private track IS present the equality is asserted below, and
    // `the_ceos_own_voice_over_rich_interrupts_him_and_his_words_are_kept` asserts it again
    // before using either. Deriving this bound from the private file instead would have made a
    // frame-math test unrunnable on every machine but one, to learn a duration.
    let reference = read(HIS_VOICE, "-reference.wav");
    let secs = reference.len() as f32 / SAMPLE_RATE as f32;
    for (a, b) in HIS_INTERVALS {
        assert!(*a < *b, "interval {a}..{b} is empty or inverted");
        assert!(*b <= secs, "interval {a}..{b} runs past the {secs:.3} s fixture");
    }

    // THE ARITHMETIC THE SIDECAR GOT WRONG, PINNED HERE SO IT CANNOT COME BACK. Every interval is
    // a whole number of 256-sample frames, and they sum to 305 — not the 299 that
    // `-nearend2-nearend.json` carried until 2026-09-18. 119 + 69 + 52 + 18 + 47 = 305, and
    // 305 x 256 / 16000 = 4.880 s, which is the total the same sidecar states.
    let mut total_frames = 0usize;
    for (a, b) in HIS_INTERVALS {
        let n = (b - a) * SAMPLE_RATE as f32 / VAD_FRAME_SAMPLES as f32;
        assert!(
            (n - n.round()).abs() < 1e-3,
            "interval {a}..{b} is {n} frames — not a whole number of VAD frames"
        );
        total_frames += n.round() as usize;
    }
    assert_eq!(total_frames, 305);
    let total_secs = total_frames as f32 * VAD_FRAME_SAMPLES as f32 / SAMPLE_RATE as f32;
    assert!((total_secs - 4.880).abs() < 1e-6, "{total_secs}");

    // When the private track is here, the length substitution above is proven rather than argued.
    if let Ok(p) = private_fixtures::ceo_mic_track() {
        assert_eq!(
            read_wav(&p).len(),
            reference.len(),
            "the private microphone track and the committed reference track are not the same \
             recording, so the duration bound above was measured on the wrong file"
        );
    } else {
        println!(
            "[note] {HIS_VOICE_MIC_IS_PRIVATE} is not on this machine, so the duration bound \
             above was measured on the committed -reference track alone (same recording, same \
             index). Everything else in this test is frame math and needs no fixture at all."
        );
    }
}

/// **WHAT MADE THE APP SAY "I COULDN'T TELL YOUR VOICE FROM MY OWN" — and what it now tells him.**
///
/// On 2026-10-21 a new user on 1.2.1 got `VoiceNotice::CouldNotListenWhileSpeaking` right after
/// Rich spoke a cut-off notice through his Mac's speakers. This replays the shipped `CaptureBrain`
/// over the committed echo-path fixtures to show the two ways that line is reached and what it
/// says, with no sound played anywhere (CEO §53):
///
/// 1. **Rich's own echo, nobody speaking.** Over the echo-only recordings at the six volumes, an
///    utterance opened by Rich's residual is discarded tainted, which is the one thing that
///    raises the line (`supervise`, `CapMsg::Discarded { tainted: true }`). Measured and printed
///    per condition; at least one condition must reach it, or this is not the incident's path.
/// 2. **A short word of his own over Rich** (private fixture): 0.192 s of the CEO's recorded
///    voice from `-nearend2-mic.wav`, at its recorded near-field level (about -18 dBFS, see
///    `the_ceos_own_voice_over_rich_interrupts_him_and_his_words_are_kept`), summed into the
///    echo-only track while Rich speaks, at a volume where the echo alone raises nothing. 0.192 s
///    is 12 frames, under the 15 a barge-in needs, so the word cannot stop Rich and is discarded:
///    this is the case where he DID say something and lost it.
///
/// In both, the line he is shown must say what to do: say it again now.
#[test]
fn richs_own_echo_raises_the_half_duplex_line_and_it_says_what_to_do() {
    use richos_voice::controller::{RefusalNotices, VoiceNotice};
    let line = VoiceNotice::CouldNotListenWhileSpeaking.ceo_message();
    assert!(line.contains("please say it again now"), "the line gives no next step: {line}");

    let (mut raised, mut conditions) = (0, 0);
    for prefix in [ECHO_ONLY_MORNING, ECHO_ONLY_VOL60] {
        let mic0 = read(prefix, "-mic.wav");
        let reference = read(prefix, "-reference.wav");
        for &vol in VOLUMES {
            let g = mic_gain_for_residual(&mic0, &reference, vol);
            let mic: Vec<f32> = mic0.iter().map(|s| s * g).collect();
            let o = replay(&mic, &reference, None, 0);
            conditions += 1;
            // `supervise` claims the line on the first tainted discard of a voice session.
            let mut notices = RefusalNotices::new();
            let said = (0..o.tainted_discards).find_map(|_| notices.half_duplex());
            println!(
                "[measured] {prefix} at residual {vol:.2} dBFS, nobody speaking: {} tainted \
                 discard(s) -> {}",
                o.tainted_discards,
                said.map_or("no line".to_string(), |n| n.ceo_message().to_string())
            );
            if let Some(n) = said {
                assert_eq!(n, VoiceNotice::CouldNotListenWhileSpeaking);
                raised += 1;
            }
        }
    }
    assert!(raised >= 1, "Rich's own echo raised the line in no condition: this is not the incident's path");
    println!("[measured] Rich's own echo alone raised the line in {raised} of {conditions} conditions");
}

/// The second case of the test above, on the private recording of the CEO's own voice.
#[test]
#[cfg_attr(
    not(private_fixtures),
    ignore = "PRIVATE FIXTURE: ceo-rig-2026-09-18-nearend2-mic.wav is a recording of the CEO's \
              own voice and is not committed to this public repository. It is in the private \
              richos-hq repository at fixtures/echo-path/. Set RICHOS_PRIVATE_FIXTURES to that \
              directory to run this."
)]
fn a_short_word_of_his_over_rich_is_lost_and_the_line_tells_him_to_say_it_again() {
    use richos_voice::controller::{RefusalNotices, VoiceNotice};
    let his_mic = read_his_mic();
    // 12 frames from the middle of his first marked interval: under the 15 a barge-in needs.
    let (open, close) = HIS_INTERVALS[0];
    let mid = (((open + close) / 2.0) * SAMPLE_RATE as f32) as usize;
    let word: Vec<f32> = his_mic[mid..mid + 12 * VAD_FRAME_SAMPLES].to_vec();
    println!("[measured] his word: {:.3} s at {:.1} dBFS", word.len() as f32 / SAMPLE_RATE as f32, dbfs(&word));

    // The morning recording at Ray's walk volume: measured 2026-10-21, its echo alone opens no
    // utterance at any of the six volumes, so a discard here is his word's and nothing else's.
    let mic0 = read(ECHO_ONLY_MORNING, "-mic.wav");
    let reference = read(ECHO_ONLY_MORNING, "-reference.wav");
    let g = mic_gain_for_residual(&mic0, &reference, VOLUMES[3]);
    let mic: Vec<f32> = mic0.iter().map(|s| s * g).collect();
    let alone = replay(&mic, &reference, None, 0);
    assert_eq!(alone.tainted_discards, 0, "the echo alone already raises the line here: {alone:?}");
    let at = (4.0 * SAMPLE_RATE as f32 / VAD_FRAME_SAMPLES as f32) as usize;
    let o = replay(&mic, &reference, Some(&word), at);
    println!("[measured] his word over Rich at 4.0 s: {o:?}");
    assert_eq!(o.barge_ins, 0, "a 12-frame word stopped Rich; the premise moved: {o:?}");
    assert!(o.tainted_discards >= 1, "his word was not discarded, so it is not the lost-word case: {o:?}");
    let said = RefusalNotices::new().half_duplex().expect("the first tainted discard of a session says the line");
    assert_eq!(said, VoiceNotice::CouldNotListenWhileSpeaking);
    assert!(said.ceo_message().contains("please say it again now"), "{}", said.ceo_message());
}

/// **THE LINE THAT SAYS "SAY IT AGAIN NOW" IS SHOWN ONLY ONCE RICH CAN TAKE THE REPLY.**
///
/// Review rv-20261021T175303Z-70f43b54-a22b: on this recording the first tainted discard comes
/// early in Rich's answer, with most of it still to play, and the supervisor used to show the
/// line right there, inviting a reply into the window that had just discarded one. This drives
/// the shipped `CaptureBrain` frame by frame over the echo-only recording at every residual
/// level, feeds each tainted discard to the same `RefusalNotices` + `HalfDuplexDelivery` pair
/// `supervise` uses, asks for the line on every frame exactly as the supervisor's tick does,
/// and requires that it is shown, and only on a frame where Rich is no longer audible, after
/// his last audible frame. Run red on 2026-10-21 with `HalfDuplexDelivery::due` made to ignore
/// `rich_audible` (the old show-at-once behavior): at residual -52.00 dBFS the line was shown at
/// frame 162 (2.592 s) with Rich audible until 24.960 s.
#[test]
fn the_half_duplex_line_is_shown_only_once_rich_can_take_the_reply() {
    use richos_voice::controller::{HalfDuplexDelivery, RefusalNotices, VoiceNotice};
    let mic0 = read(ECHO_ONLY_VOL60, "-mic.wav");
    let reference = read(ECHO_ONLY_VOL60, "-reference.wav");
    for &vol in VOLUMES {
        let g = mic_gain_for_residual(&mic0, &reference, vol);
        let mic: Vec<f32> = mic0.iter().map(|s| s * g).collect();
        let n = mic.len().min(reference.len());
        let frames = n / VAD_FRAME_SAMPLES;
        let active: Vec<bool> = (0..frames)
            .map(|b| {
                let lo = b * VAD_FRAME_SAMPLES;
                rms(&reference[lo..lo + VAD_FRAME_SAMPLES]) > richos_voice::aec::FAR_END_ACTIVE_RMS
            })
            .collect();
        let first = active.iter().position(|a| *a).expect("the reference is never active");
        let last = frames - 1 - active.iter().rev().position(|a| *a).unwrap();
        let (aec, ring) = EchoCanceller::new();
        let mut brain = CaptureBrain::with_aec(aec);
        let mut notices = RefusalNotices::new();
        let mut delivery = HalfDuplexDelivery::default();
        let (mut discarded_at, mut shown) = (None, None);
        // A tail of silence after the recording: the app keeps listening after Rich stops.
        for b in 0..frames + 50 {
            let speaking = b >= first && b <= last + 1;
            let frame: Vec<f32> = if b < frames {
                let lo = b * VAD_FRAME_SAMPLES;
                ring.push(&reference[lo..lo + VAD_FRAME_SAMPLES]);
                mic[lo..lo + VAD_FRAME_SAMPLES].to_vec()
            } else {
                ring.push(&[0.0; VAD_FRAME_SAMPLES]);
                vec![0.0; VAD_FRAME_SAMPLES]
            };
            for m in brain.push_frame(&frame, speaking, false) {
                if let CapMsg::Discarded { tainted: true } = m {
                    discarded_at.get_or_insert(b);
                    if let Some(n) = notices.half_duplex() {
                        delivery.owe(n);
                    }
                }
            }
            if let Some(n) = delivery.due(speaking) {
                assert_eq!(n, VoiceNotice::CouldNotListenWhileSpeaking);
                assert!(shown.is_none(), "the line was shown twice at residual {vol:.2}");
                shown = Some((b, speaking));
            }
        }
        println!(
            "[measured] residual {vol:.2} dBFS: first tainted discard at {:?} s, line shown at {:?} s, Rich audible until {:.3} s",
            discarded_at.map(|f| frames_to_secs(f as u32)),
            shown.map(|(f, _)| frames_to_secs(f as u32)),
            frames_to_secs((last + 1) as u32)
        );
        let discarded = discarded_at.expect("premise: Rich's echo raised no tainted discard on this recording");
        let (at, audible) = shown.expect("the owed line was never shown");
        assert!(!audible, "the line was shown while Rich was audible, at frame {at}, residual {vol:.2}");
        assert!(at > last + 1, "the line was shown at frame {at}, before Rich's last audible frame {}", last + 1);
        assert!(at > discarded);
    }
}

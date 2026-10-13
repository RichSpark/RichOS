//! **A sentence with a pause in the middle of it is ONE turn — replayed on the CEO's own rig.**
//!
//! The defect (nightly 54, 2026-10-13, his folder a): he pressed Talk to Rich, said two words,
//! thought for 1.406 s, and said the sentence his screenshot marks. The endpointer
//! closed the first stretch at 0.800 s of silence, the app sent it as a turn, and Rich's answer to
//! those two words reached his ear 1.258 s after he finished the second sentence. His log and his
//! ledger, and the arithmetic, are in `src/turn.rs`.
//!
//! This file drives the SHIPPED composition — [`CaptureBrain`] (canceller, VAD, endpointer, and
//! the end-of-turn count) feeding a [`TurnAssembler`] exactly as `supervise` feeds it — over a
//! microphone track built from the committed echo-path fixture:
//!
//! - **the room**: the first 0.700 s of `ceo-rig-2026-09-18-nearend-mic.wav`, the CEO's own room
//!   on his own microphone before playback began (`-nearend-session.json`: `lead_ms` 700), tiled.
//!   Its reference track is measured silent there, so nothing of Rich is in it.
//! - **the near-end voice**, superimposed at his measured near-field level, **-18.1 dBFS**
//!   (`-nearend2-nearend.json`): in the public test a synthetic voiced signal, and in the private
//!   test his OWN recorded voice, his first two intervals from `-nearend2-mic.wav`. Those were
//!   recorded while Rich was speaking, so Rich's echo is under them at -46.7 dBFS, 28.6 dB below
//!   him; it is left in, not filtered, and the gap between them is his room, not that recording.
//! - **the pause between the two stretches is his measured 1.406 s.**
//!
//! Rich is silent throughout (`speaking = false`, a zero reference), because that is the case the
//! CEO met: the answer had not started when his turn was taken from him.
//!
//! No sound is played anywhere. Everything here is samples in memory (CEO §53).

use richos_voice::aec::EchoCanceller;
use richos_voice::controller::{CapMsg, CaptureBrain};
use richos_voice::endpoint::{SILENCE_HANGOVER_FRAMES, SPEECH_ONSET_FRAMES};
use richos_voice::private_fixtures;
use richos_voice::turn::{end_of_turn_secs, Turn, TurnAssembler, END_OF_TURN_FRAMES};
use richos_voice::vad::{frames_to_secs, SAMPLE_RATE, VAD_FRAME_SAMPLES};
use richos_voice::wav;
use std::path::{Path, PathBuf};

const ROOM: &str = "tests/fixtures/echo-path/ceo-rig-2026-09-18-nearend";
/// `-nearend-session.json`: playback began 0.700 s into the recording.
const LEAD_SECS: f32 = 0.700;
/// `-nearend2-nearend.json`: his level at the microphone.
const HIS_LEVEL_DBFS: f32 = -18.1;
/// His measured pause, nightly 54: (53.024 - 0.112) - (52.306 - 0.800).
const HIS_PAUSE_SECS: f32 = 1.406;
/// His first two intervals in `-nearend2-mic.wav` (`-nearend2-nearend.json`).
const HIS_FIRST: (f32, f32) = (3.568, 5.472);
const HIS_SECOND: (f32, f32) = (6.256, 7.360);

fn read_wav(p: &Path) -> Vec<f32> {
    let bytes = std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    let pcm = wav::read_pcm16(&bytes).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    assert_eq!(pcm.sample_rate, SAMPLE_RATE, "{} is not 16 kHz", p.display());
    wav::to_mono(&pcm.samples, pcm.channels)
}

fn committed(suffix: &str) -> Vec<f32> {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push(format!("{ROOM}{suffix}"));
    read_wav(&p)
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|s| s * s).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

fn dbfs(x: &[f32]) -> f32 {
    20.0 * rms(x).max(1e-12).log10()
}

fn secs_to_samples(s: f32) -> usize {
    (s * SAMPLE_RATE as f32).round() as usize
}

/// His room, `secs` long: the pre-playback lead of the echo-only recording, tiled. The reference
/// over that lead is checked silent, so the room carries none of Rich.
fn room(secs: f32) -> Vec<f32> {
    let mic = committed("-mic.wav");
    let reference = committed("-reference.wav");
    let lead = secs_to_samples(LEAD_SECS);
    assert!(
        rms(&reference[..lead]) == 0.0,
        "the reference is not silent over the first {LEAD_SECS} s, so the room would carry Rich"
    );
    let tone = &mic[..lead];
    tone.iter().copied().cycle().take(secs_to_samples(secs)).collect()
}

/// `voice` scaled to his measured near-field level and laid over `room`.
fn superimpose(room: &mut [f32], at: usize, voice: &[f32]) {
    let gain = 10f32.powf(HIS_LEVEL_DBFS / 20.0) / rms(voice).max(1e-12);
    for (i, s) in voice.iter().enumerate() {
        room[at + i] += s * gain;
    }
}

/// A voiced stretch: a 130 Hz glottal-like harmonic series under a 4 Hz syllable envelope. Not a
/// person — the public stand-in for one, used only where his recording cannot be.
fn synthetic_voice(secs: f32, seed: u32) -> Vec<f32> {
    let n = secs_to_samples(secs);
    let sr = SAMPLE_RATE as f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / sr;
            let f0 = 130.0 + 12.0 * (2.0 * std::f32::consts::PI * 0.7 * t + seed as f32).sin();
            let mut v = 0.0;
            for h in 1..=12 {
                v += (2.0 * std::f32::consts::PI * f0 * h as f32 * t).sin() / h as f32;
            }
            let syllable = 0.55 + 0.45 * (2.0 * std::f32::consts::PI * 4.0 * t).sin();
            v * syllable
        })
        .collect()
}

/// What a replay saw.
#[derive(Debug, Default)]
struct Replay {
    admitted: usize,
    turns: Vec<(usize, Turn)>,
    quiet_frames: Vec<usize>,
}

/// THE SHIPPED COMPOSITION over a microphone track. Every admitted utterance is "recognized" at
/// once as `fragment N`, which is the recognizer finishing early — the case that released a turn
/// soonest before the fix and so the one that can hide nothing.
fn replay(mic: &[f32]) -> Replay {
    let (aec, ring) = EchoCanceller::new();
    let mut brain = CaptureBrain::with_aec(aec);
    let mut turns = TurnAssembler::new();
    let mut r = Replay::default();
    let silence = vec![0.0f32; VAD_FRAME_SAMPLES];
    for (b, frame) in mic.chunks_exact(VAD_FRAME_SAMPLES).enumerate() {
        ring.push(&silence);
        for m in brain.push_frame(frame, false, false) {
            match m {
                CapMsg::Started { .. } => turns.utterance_started(),
                CapMsg::Utterance(u) => {
                    r.admitted += 1;
                    turns.utterance_admitted((u.utterance.duration_secs() * 1000.0) as u64);
                    turns.recognized(Some((format!("fragment {}", r.admitted), false)));
                }
                CapMsg::Discarded { .. } => turns.utterance_discarded(),
                CapMsg::TurnQuiet => {
                    r.quiet_frames.push(b);
                    turns.quiet();
                }
                _ => {}
            }
            if let Some(t) = turns.take_turn() {
                r.turns.push((b, t));
            }
        }
    }
    r
}

/// The verdict both tests share. `last_speech_end` is the sample at which his voice stops.
fn assert_one_turn_after_he_is_done(r: &Replay, last_speech_end: usize, what: &str) {
    println!("[replay] {what}: {r:?}");
    // POSITIVE CONTROL. His pause is longer than the endpointer's 0.800 s, so it DOES split the
    // recording into two utterances — which is exactly what made two turns before the fix. If this
    // fails the fixture no longer contains the defect and the verdict below proves nothing.
    assert_eq!(
        r.admitted, 2,
        "{what}: the {HIS_PAUSE_SECS:.3} s pause should split the speech into two utterances (the \
         endpointer closes one at {:.3} s); it did not, so this replay cannot show the defect",
        frames_to_secs(SILENCE_HANGOVER_FRAMES)
    );
    // THE DEFECT: one thought, one turn.
    assert_eq!(r.turns.len(), 1, "{what}: one thought with a pause in it became {} turns", r.turns.len());
    let (frame, turn) = &r.turns[0];
    assert_eq!(turn.text, "fragment 1 fragment 2", "{what}: the turn lost or reordered a fragment");
    assert_eq!(turn.fragments, 2);
    // NOTHING DURING THE PAUSE: no end-of-turn quiet while he was thinking.
    assert_eq!(r.quiet_frames.len(), 1, "{what}: the quiet fired during his pause: {:?}", r.quiet_frames);
    // RICH SILENT UNTIL HE IS DONE: the turn — the only thing that can start an answer — is handed
    // over no sooner than 2.000 s after his last speech, measured in frames on this replay.
    let released_at = (frame + 1) * VAD_FRAME_SAMPLES;
    let after = (released_at as f32 - last_speech_end as f32) / SAMPLE_RATE as f32;
    println!(
        "[measured] {what}: turn handed over {after:.3} s after his last speech (end of turn = \
         {END_OF_TURN_FRAMES} x 256 / 16000 = {:.3} s)",
        end_of_turn_secs()
    );
    assert!(
        after >= end_of_turn_secs() - frames_to_secs(SPEECH_ONSET_FRAMES),
        "{what}: the turn was handed over {after:.3} s after his last speech, before the {:.3} s end of turn",
        end_of_turn_secs()
    );
    // And not much later than that: the window is the only wait, so the cost per answer is the
    // window and nothing else. A VAD hangover on the room's own tail can add a few frames.
    assert!(after < end_of_turn_secs() + 0.5, "{what}: the turn waited {after:.3} s");
}

/// THE PUBLIC REPLAY: his room, a synthetic voice at his level, his pause. Runs everywhere.
#[test]
fn a_pause_in_the_middle_of_a_sentence_is_one_turn_on_his_room() {
    let first = synthetic_voice(1.904, 1);
    let second = synthetic_voice(1.104, 2);
    let lead = 1.0;
    let total = lead + 1.904 + HIS_PAUSE_SECS + 1.104 + 4.0;
    let mut mic = room(total);
    let a = secs_to_samples(lead);
    superimpose(&mut mic, a, &first);
    let b = a + first.len() + secs_to_samples(HIS_PAUSE_SECS);
    superimpose(&mut mic, b, &second);
    println!(
        "[fixture] room {:.1} dBFS, voice {HIS_LEVEL_DBFS} dBFS, pause {HIS_PAUSE_SECS} s",
        dbfs(&room(LEAD_SECS))
    );
    let r = replay(&mic);
    assert_one_turn_after_he_is_done(&r, b + second.len(), "synthetic voice on his room");
}

/// THE PRIVATE REPLAY: his OWN recorded voice — his first two intervals from the near-end
/// recording, at the level he arrived at — on his room, with his pause between them.
///
/// `-nearend2-mic.wav` lives in the private `richos-hq` repository (`src/private_fixtures.rs`).
/// Without it this reports `ignored, PRIVATE FIXTURE: …`, never `ok`.
#[test]
#[cfg_attr(
    not(private_fixtures),
    ignore = "PRIVATE FIXTURE: ceo-rig-2026-09-18-nearend2-mic.wav is a recording of the CEO's \
              own voice and is not committed to this public repository. It is in the private \
              richos-hq repository at fixtures/echo-path/. Set RICHOS_PRIVATE_FIXTURES to that \
              directory to run this."
)]
fn a_pause_in_the_middle_of_his_own_sentence_is_one_turn() {
    let his = read_wav(&private_fixtures::require_ceo_mic_track());
    let cut = |(a, b): (f32, f32)| his[secs_to_samples(a)..secs_to_samples(b)].to_vec();
    let (first, second) = (cut(HIS_FIRST), cut(HIS_SECOND));
    println!(
        "[fixture] his voice {:.1} / {:.1} dBFS as recorded, {:.3} s and {:.3} s",
        dbfs(&first),
        dbfs(&second),
        first.len() as f32 / SAMPLE_RATE as f32,
        second.len() as f32 / SAMPLE_RATE as f32
    );
    let lead = 1.0;
    let total = lead + (first.len() + second.len()) as f32 / SAMPLE_RATE as f32 + HIS_PAUSE_SECS + 4.0;
    let mut mic = room(total);
    let a = secs_to_samples(lead);
    // His own recording carries his own level; it is laid in as recorded rather than rescaled.
    for (i, s) in first.iter().enumerate() {
        mic[a + i] += s;
    }
    let b = a + first.len() + secs_to_samples(HIS_PAUSE_SECS);
    for (i, s) in second.iter().enumerate() {
        mic[b + i] += s;
    }
    let r = replay(&mic);
    assert_one_turn_after_he_is_done(&r, b + second.len(), "his own voice on his room");
}

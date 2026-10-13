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
//! The third test is the FLOOR (`turn.rs`, `AnswerGate`): he thinks for longer than the window, so
//! the first turn IS handed over, and he goes on; Rich's answer to it arrives while he is speaking
//! again, is held, is superseded by his new words, and is never said.
//!
//! No sound is played anywhere. Everything here is samples in memory (CEO §53).

use richos_voice::aec::EchoCanceller;
use richos_voice::controller::{AudibleWindow, CapMsg, CaptureBrain};
use richos_voice::endpoint::{EndReason, SILENCE_HANGOVER_FRAMES, SPEECH_ONSET_FRAMES};
use richos_voice::private_fixtures;
use richos_voice::turn::{end_of_turn_secs, may_sound, rich_audible, AnswerGate, Turn, TurnAssembler, END_OF_TURN_FRAMES};
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
    /// Utterances cut at whisper's 30.000 s window rather than ended by silence.
    cuts: usize,
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
                    if u.utterance.reason == EndReason::MaxLength {
                        r.cuts += 1;
                    }
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

// ------------------------------------------------------------------------------------------------
// THE FLOOR: a pause LONGER than the window, and Rich's answer arriving while he has gone on.
// ------------------------------------------------------------------------------------------------

/// His second pause on nightly 54 was at least 2.058 s (his last word 54.538 s, Rich audible
/// 56.596 s), and whatever came after it was cut off by Rich. A pause like that is longer than the
/// 2.000 s window, so the turn IS handed over during it; this is the case the floor rule exists for.
const LONG_PAUSE_SECS: f32 = 2.950;
/// How long the model takes to send Rich's first sentence after a turn is handed over. His log says
/// 3.719 s (52.877 -> 56.596, synthesis included); 1.504 s here (94 frames) is FASTER, which brings
/// the answer in while he is mid-sentence and so is the harder case.
const MODEL_FRAMES: usize = 94;

/// When each of Rich's sentences reached the speaker, and what the floor did.
#[derive(Debug, Default)]
struct FloorReplay {
    turns: Vec<(usize, Turn)>,
    spoken: Vec<(usize, String)>,
    superseded: usize,
}

/// THE SHIPPED RULES, in `supervise`'s order, over a microphone track: `CaptureBrain`, the
/// `TurnAssembler` with its floor, and the `AnswerGate`. Each handed-over turn starts at once (as
/// `rich://turn-started` does) and its answer's two sentences are offered `MODEL_FRAMES` and
/// `MODEL_FRAMES + 30` frames later; the model finishes the turn 60 frames after its first sentence.
fn replay_with_answers(mic: &[f32]) -> FloorReplay {
    let (aec, ring) = EchoCanceller::new();
    let mut brain = CaptureBrain::with_aec(aec);
    let mut turns = TurnAssembler::new();
    let mut gate: AnswerGate<String> = AnswerGate::new();
    let mut r = FloorReplay::default();
    let mut offers: Vec<(usize, String)> = Vec::new();
    let mut turn_ends: Vec<usize> = Vec::new();
    let mut fragments = 0;
    let silence = vec![0.0f32; VAD_FRAME_SAMPLES];
    for (b, frame) in mic.chunks_exact(VAD_FRAME_SAMPLES).enumerate() {
        ring.push(&silence);
        for m in brain.push_frame(frame, false, false) {
            match m {
                CapMsg::Started { tainted: false, .. } => turns.his_utterance_started(),
                CapMsg::Started { tainted: true, .. } => turns.utterance_started(),
                CapMsg::Utterance(u) => {
                    fragments += 1;
                    turns.utterance_admitted((u.utterance.duration_secs() * 1000.0) as u64);
                    let starts = turns.recognized(Some((format!("fragment {fragments}"), false)));
                    let turn_active = turn_ends.iter().any(|&end| b < end);
                    if starts && (turn_active || gate.held() > 0) {
                        gate.supersede();
                        r.superseded += 1;
                    }
                }
                CapMsg::Discarded { .. } => turns.utterance_discarded(),
                CapMsg::TurnQuiet => turns.quiet(),
                _ => {}
            }
        }
        if let Some(t) = turns.take_turn() {
            let k = r.turns.len() + 1;
            r.turns.push((b, t));
            gate.next_turn();
            offers.push((b + MODEL_FRAMES, format!("answer {k} sentence 1")));
            offers.push((b + MODEL_FRAMES + 30, format!("answer {k} sentence 2")));
            turn_ends.push(b + MODEL_FRAMES + 60);
        }
        let floor = turns.has_floor();
        for (_, s) in offers.iter().filter(|(at, _)| *at == b) {
            if let Some(now) = gate.offer(s.clone(), floor) {
                r.spoken.push((b, now));
            }
        }
        for now in gate.release(floor) {
            r.spoken.push((b, now));
        }
    }
    r
}

/// THE FLOOR, on his room with a voice at his level: he thinks for longer than the window, the
/// first turn is handed over, and he goes on. Rich's answer to the first turn arrives while he is
/// mid-sentence: it is held, then superseded by his new words, and never said. Rich speaks only
/// the answer to his last turn, and only once he is done.
#[test]
fn rich_never_starts_talking_while_he_has_gone_on() {
    let lead = 1.0;
    let parts = [synthetic_voice(1.904, 1), synthetic_voice(1.104, 2), synthetic_voice(1.904, 3)];
    let gaps = [HIS_PAUSE_SECS, LONG_PAUSE_SECS];
    let total = lead + 1.904 + 1.104 + 1.904 + HIS_PAUSE_SECS + LONG_PAUSE_SECS + 6.0;
    let mut mic = room(total);
    let mut at = secs_to_samples(lead);
    let mut voiced = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        superimpose(&mut mic, at, part);
        voiced.push((at, at + part.len()));
        at += part.len() + gaps.get(i).map(|g| secs_to_samples(*g)).unwrap_or(0);
    }
    let r = replay_with_answers(&mic);
    println!("[replay] the floor: {r:?}");
    let secs = |frame: usize| ((frame + 1) * VAD_FRAME_SAMPLES) as f32 / SAMPLE_RATE as f32;

    // The 1.406 s pause is inside the window and the 2.950 s pause is not: two turns, the first
    // holding both stretches before the long pause.
    let texts: Vec<&str> = r.turns.iter().map(|(_, t)| t.text.as_str()).collect();
    assert_eq!(texts, vec!["fragment 1 fragment 2", "fragment 3"]);

    // POSITIVE CONTROL: the first answer really did arrive while he was speaking again, so the
    // rule below is being tested and not passed by timing.
    let (third_start, third_end) = voiced[2];
    let first_offer = r.turns[0].0 + MODEL_FRAMES;
    let offered_at = first_offer * VAD_FRAME_SAMPLES;
    assert!(
        offered_at > third_start && offered_at < third_end,
        "the first answer was offered at {:.3} s, outside his third stretch ({:.3}-{:.3} s): this \
         replay does not put Rich's answer over him",
        secs(first_offer),
        third_start as f32 / SAMPLE_RATE as f32,
        third_end as f32 / SAMPLE_RATE as f32
    );

    // THE RULE: nothing of the first answer is ever said, and it was superseded exactly once.
    assert_eq!(r.superseded, 1, "the first answer was not superseded by his new words");
    let said: Vec<&str> = r.spoken.iter().map(|(_, s)| s.as_str()).collect();
    assert_eq!(said, vec!["answer 2 sentence 1", "answer 2 sentence 2"], "Rich said: {said:?}");

    // RICH SILENT UNTIL HE IS DONE: nothing is said while any of his voice is in the room, and the
    // first word of Rich's comes after his last word plus the whole end-of-turn window.
    for (frame, s) in &r.spoken {
        let sample = (frame + 1) * VAD_FRAME_SAMPLES;
        for (a, b) in &voiced {
            assert!(sample < *a || sample > *b, "Rich said {s:?} at {:.3} s while he was talking", secs(*frame));
        }
    }
    let first_word = r.spoken[0].0;
    let after = secs(first_word) - third_end as f32 / SAMPLE_RATE as f32;
    println!(
        "[measured] Rich's first word {after:.3} s after the CEO's last ({:.3} s window + {:.3} s model)",
        end_of_turn_secs(),
        frames_to_secs(MODEL_FRAMES as u32)
    );
    assert!(after >= end_of_turn_secs(), "Rich spoke {after:.3} s after his last word");
}

// ------------------------------------------------------------------------------------------------
// A STRETCH CUT AT WHISPER'S 30.000 s WINDOW IS NOT HIM STOPPING (review rv-20261013T163358Z, 2).
// ------------------------------------------------------------------------------------------------

/// Words: the synthetic voice with a 0.120 s gap in every 0.600 s, so the VAD hears speech with
/// the gaps between words that speech has, rather than one 30 s tone it would learn as noise.
fn spoken_words(secs: f32, seed: u32) -> Vec<f32> {
    let period = secs_to_samples(0.600);
    let gap = secs_to_samples(0.120);
    synthetic_voice(secs, seed)
        .into_iter()
        .enumerate()
        .map(|(i, s)| if i % period >= period - gap { 0.0 } else { s })
        .collect()
}

/// He talks for a little over 30 s, so the endpointer cuts the utterance at its 30.000 s window
/// (`endpoint.rs` MAX_UTTERANCE_FRAMES) a few frames before he pauses; then he thinks for his
/// measured 1.406 s and goes on. The cut is not silence, so the quiet is counted from his last
/// speech frame: ONE turn. Timed from the cut, the quiet came 1.200 s after it, inside the pause,
/// and his turn was handed over mid-thought.
#[test]
fn a_cut_at_the_thirty_second_window_is_not_his_turn_ending() {
    let lead = 1.0;
    let long = spoken_words(30.030, 4);
    let after = synthetic_voice(1.0, 5);
    let total = lead + 30.030 + HIS_PAUSE_SECS + 1.0 + 4.0;
    let mut mic = room(total);
    let a = secs_to_samples(lead);
    superimpose(&mut mic, a, &long);
    let b = a + long.len() + secs_to_samples(HIS_PAUSE_SECS);
    superimpose(&mut mic, b, &after);
    let r = replay(&mic);
    println!("[replay] the 30 s cut: {r:?}");
    // POSITIVE CONTROL: the long stretch really was cut at the window, not ended by silence.
    assert_eq!(r.cuts, 1, "the long stretch was not cut at the 30.000 s window; this replay shows nothing");
    assert_one_turn_after_he_is_done(&r, b + after.len(), "a 30 s stretch cut at the window");
}

// ------------------------------------------------------------------------------------------------
// HE GOES ON WHILE RICH'S FIRST SENTENCE IS STILL BEING SYNTHESIZED (review rv-20261013T173416Z, 1).
// ------------------------------------------------------------------------------------------------

/// `say` producing one sentence, as the review modeled it: 21 frames = 0.336 s.
const SYNTH_FRAMES: usize = 21;
/// One synthesized sentence, once it plays: 2.000 s.
const SENTENCE_FRAMES: usize = 125;

/// What the synthesis replay saw.
#[derive(Debug, Default)]
struct SynthReplay {
    turns: Vec<(usize, Turn)>,
    /// Frames at which a sentence reached the device, and which sentence.
    sounded: Vec<(usize, String)>,
    tainted_starts: usize,
    superseded: usize,
}

/// THE SHIPPED RULES with synthesis in the loop: `CaptureBrain` hears the microphone with the
/// `speaking` flag the supervisor would publish (`AudibleWindow` over queued/owed, gated by
/// `rich_audible`), sentences pass the `AnswerGate`, spend `SYNTH_FRAMES` in synthesis, and are
/// played only if `may_sound`; a superseded sentence is dropped by its generation. The first turn's
/// first sentence is offered at `first_offer`; every later sentence `MODEL_FRAMES` after its turn.
fn replay_with_synthesis(mic: &[f32], first_offer: usize) -> SynthReplay {
    let (aec, ring) = EchoCanceller::new();
    let mut brain = CaptureBrain::with_aec(aec);
    let mut turns = TurnAssembler::new();
    let mut gate: AnswerGate<(u64, String)> = AnswerGate::new();
    let mut window = AudibleWindow::new();
    let mut r = SynthReplay::default();
    let mut offers: Vec<(usize, String)> = Vec::new();
    let mut synthesis: Vec<(usize, u64, String)> = Vec::new(); // (ready frame, generation, text)
    let mut playing_until = 0usize;
    let mut sounded = false;
    let mut speaking = false;
    let mut generation = 0u64;
    let mut turn_ends: Vec<usize> = Vec::new();
    let mut fragments = 0;
    let silence = vec![0.0f32; VAD_FRAME_SAMPLES];
    for (b, frame) in mic.chunks_exact(VAD_FRAME_SAMPLES).enumerate() {
        ring.push(&silence);
        for m in brain.push_frame(frame, speaking, false) {
            match m {
                CapMsg::Started { tainted: false, .. } => turns.his_utterance_started(),
                CapMsg::Started { tainted: true, .. } => {
                    r.tainted_starts += 1;
                    turns.utterance_started();
                }
                CapMsg::Utterance(u) => {
                    fragments += 1;
                    turns.utterance_admitted((u.utterance.duration_secs() * 1000.0) as u64);
                    let starts = turns.recognized(Some((format!("fragment {fragments}"), false)));
                    let outstanding = turn_ends.iter().any(|&end| b < end) || gate.held() > 0 || !synthesis.is_empty();
                    if starts && outstanding {
                        gate.supersede();
                        generation += 1;
                        r.superseded += 1;
                    }
                }
                CapMsg::Discarded { .. } => turns.utterance_discarded(),
                CapMsg::TurnQuiet => turns.quiet(),
                _ => {}
            }
        }
        if let Some(t) = turns.take_turn() {
            let k = r.turns.len() + 1;
            let at = if k == 1 { first_offer } else { b + MODEL_FRAMES };
            assert!(at > b, "the first answer was offered before its turn was handed over");
            r.turns.push((b, t));
            gate.next_turn();
            offers.push((at, format!("answer {k} sentence 1")));
            turn_ends.push(at + 60);
        }
        let floor = turns.has_floor();
        for (_, s) in offers.iter().filter(|(at, _)| *at == b) {
            if let Some((g, s)) = gate.offer((generation, s.clone()), floor) {
                synthesis.push((b + SYNTH_FRAMES, g, s));
            }
        }
        for (g, s) in gate.release(floor) {
            synthesis.push((b + SYNTH_FRAMES, g, s));
        }
        // The speaker: drop what was superseded, wait while the floor is his, else play.
        synthesis.retain(|(_, g, _)| *g == generation);
        if let Some(pos) = synthesis.iter().position(|(ready, _, _)| *ready <= b) {
            if may_sound(floor) && b >= playing_until {
                let (_, _, s) = synthesis.remove(pos);
                r.sounded.push((b, s));
                sounded = true;
                playing_until = b + SENTENCE_FRAMES;
            }
        }
        // The supervisor's `speaking` for the next frame.
        window.observe(b < playing_until, !synthesis.is_empty(), 0.032, (b * 16) as u64);
        if !window.is_open() {
            sounded = false;
        }
        speaking = rich_audible(window.is_open(), sounded);
    }
    r
}

/// He thinks for longer than the window, so his first turn is handed over; Rich's first sentence
/// goes into synthesis; he goes on 128 ms into it with three 0.224 s words and 0.224 s gaps (the
/// review's case). His words must be his: not tainted, holding the floor, superseding the answer
/// in synthesis, which is never played. Rich's only sound is the answer to his last turn, after he
/// is done.
#[test]
fn speech_that_resumes_during_synthesis_is_his_and_rich_waits() {
    let lead = 1.0;
    let first = synthetic_voice(1.904, 1);
    let word = synthetic_voice(0.224, 6);
    let total = lead + 1.904 + LONG_PAUSE_SECS + 1.120 + 6.0;
    let mut mic = room(total);
    let a = secs_to_samples(lead);
    superimpose(&mut mic, a, &first);
    let resume = a + first.len() + secs_to_samples(LONG_PAUSE_SECS);
    for i in 0..3 {
        superimpose(&mut mic, resume + i * secs_to_samples(0.448), &word);
    }
    let last_word_end = resume + 2 * secs_to_samples(0.448) + word.len();
    // The first sentence is offered 8 frames (0.128 s) + its synthesis before he resumes, so he
    // resumes 128 ms into its synthesis.
    let first_offer = resume / VAD_FRAME_SAMPLES - 8;
    let r = replay_with_synthesis(&mic, first_offer);
    println!("[replay] resumed during synthesis: {r:?}");
    let secs = |frame: usize| ((frame + 1) * VAD_FRAME_SAMPLES) as f32 / SAMPLE_RATE as f32;

    assert_eq!(r.tainted_starts, 0, "his resumed words were taken for Rich's echo");
    let texts: Vec<&str> = r.turns.iter().map(|(_, t)| t.text.as_str()).collect();
    assert_eq!(texts, vec!["fragment 1", "fragment 2"], "his resumed words did not become his turn");
    assert_eq!(r.superseded, 1, "the answer in synthesis was not superseded by his words");
    let said: Vec<&str> = r.sounded.iter().map(|(_, s)| s.as_str()).collect();
    assert_eq!(said, vec!["answer 2 sentence 1"], "Rich said: {said:?}");
    let first_sound = secs(r.sounded[0].0);
    let after = first_sound - last_word_end as f32 / SAMPLE_RATE as f32;
    println!("[measured] Rich's first sound {after:.3} s after the CEO's last word");
    assert!(after >= end_of_turn_secs(), "Rich sounded {after:.3} s after his last word");
}

//! **Every word of a long stretch of speech reaches the recognizer — replayed on the CEO's rig.**
//!
//! The defect (nightly 65, the CEO's recording of 2026-10-18 at 8:27): the end of what he said
//! was missing from the message sent for him. Replayed through the shipped capture path, the
//! VAD's adaptive floor climbed while he was talking, because quiet frames INSIDE his speech (soft
//! consonants, the tails of words) were allowed to raise it. A higher floor calls more of his
//! frames non-speech, and they raise it further, until nothing he says is speech: the utterance
//! ends 0.800 s later in the middle of his sentence and the rest of it never starts another one.
//! `vad.rs` `ROOM_WINDOW_FRAMES` carries the measurement.
//!
//! The recording here is the committed echo-path fixture `ceo-rig-2026-09-18-nearend-mic.wav`:
//! 23.466 s of continuous speech (an 81-word passage, macOS `say`) played into his room and
//! recorded on his own microphone, so it carries his room's reverberation and his microphone's
//! noise (`-nearend-session.json`). It is not a person and is not used as one: what the VAD and
//! the endpointer see is a long stretch of speech on his rig, and that is the question here. The
//! whole track is raised so the speech sits at his measured near-field level, **-18.1 dBFS**
//! (`-nearend2-nearend.json`), as if the talker were where he sits. The reference track is not
//! used: nothing is playing, so the canceller passes the microphone through bit for bit.
//!
//! Three properties, each of which an earlier round of this fix traded for another (reviews
//! rv-20261019T123341Z, rv-20261019T131907Z, rv-20261019T145840Z, whose generated signals these
//! are): his passage keeps every speech-level frame over silence and over room noise; and his
//! turn still ends in a noisy room, with or without clicks in it.
//!
//! No sound is played anywhere. Everything here is samples in memory (CEO §53).

use richos_voice::aec::EchoCanceller;
use richos_voice::controller::{CapMsg, CaptureBrain};
use richos_voice::turn::TurnAssembler;
use richos_voice::vad::{frames_for_secs, frames_to_secs, rms, SAMPLE_RATE, VAD_FRAME_SAMPLES};
use richos_voice::wav;
use std::path::PathBuf;

const MIC: &str = "tests/fixtures/echo-path/ceo-rig-2026-09-18-nearend-mic.wav";
/// `-nearend-session.json`: the passage starts 1.500 s into the recording and lasts 23.466 s.
const PASSAGE: (f32, f32) = (1.500, 1.500 + 23.466);
/// `-nearend2-nearend.json`: his level at the microphone.
const HIS_LEVEL_DBFS: f32 = -18.1;
/// A frame this loud is unmistakably speech at his level, 12 dB under it: every one of them must
/// be inside an utterance that went to the recognizer.
const CLEARLY_SPEECH_DBFS: f32 = HIS_LEVEL_DBFS - 12.0;
/// The room behind the passage: the fixture's own first 93 frames (1.488 s), before playback.
const ROOM_FRAMES: usize = 93;
/// The reviewers' noisy-room case: 1.600 s of a steady spoken level, then the room.
const SPOKEN_FRAMES: usize = 100;
/// His turn must end within this long after his speech stops (5.000 s: 313 frames). Main, before
/// any of this, ended every room case below within 3.520 s (220 frames).
const TURN_ENDS_WITHIN_SECS: f32 = 5.0;

fn secs_to_samples(s: f32) -> usize {
    (s * SAMPLE_RATE as f32).round() as usize
}

fn dbfs(x: &[f32]) -> f32 {
    20.0 * rms(x).max(1e-12).log10()
}

fn gain_to(db: f32, x: &[f32]) -> f32 {
    10f32.powf(db / 20.0) / rms(x)
}

/// The fixture as recorded, mono.
fn mic() -> Vec<f32> {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push(MIC);
    let bytes = std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    let pcm = wav::read_pcm16(&bytes).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    assert_eq!(pcm.sample_rate, SAMPLE_RATE);
    wav::to_mono(&pcm.samples, pcm.channels)
}

fn passage() -> std::ops::Range<usize> {
    secs_to_samples(PASSAGE.0)..secs_to_samples(PASSAGE.1)
}

/// What one replay through the shipped capture path produced.
struct Replay {
    /// The sample spans of every utterance admitted for the recognizer.
    spans: Vec<(usize, usize)>,
    /// The frames on which his end-of-turn quiet was sent.
    quiet: Vec<usize>,
    /// The frames on which an utterance started.
    starts: Vec<usize>,
    /// The frames on which a turn was handed over, recognition taking 1.600 s per utterance.
    handed_over: Vec<usize>,
}

/// THE SHIPPED CAPTURE PATH, and behind it the turn assembler the supervisor runs, with every
/// admitted utterance coming back from the recognizer 1.600 s (100 frames) after it was admitted.
fn replay(mic: &[f32]) -> Replay {
    const RECOGNITION_FRAMES: usize = 100;
    let (aec, ring) = EchoCanceller::new();
    let mut brain = CaptureBrain::with_aec(aec);
    let mut turns = TurnAssembler::new();
    let silence = vec![0.0f32; VAD_FRAME_SAMPLES];
    let mut r = Replay { spans: Vec::new(), quiet: Vec::new(), starts: Vec::new(), handed_over: Vec::new() };
    let mut at_recognizer: Vec<usize> = Vec::new();
    for (n, frame) in mic.chunks_exact(VAD_FRAME_SAMPLES).enumerate() {
        ring.push(&silence);
        for m in brain.push_frame(frame, false, false) {
            match m {
                CapMsg::Started { .. } => {
                    r.starts.push(n);
                    turns.his_utterance_started();
                }
                CapMsg::Utterance(u) => {
                    let end = (n + 1) * VAD_FRAME_SAMPLES;
                    r.spans.push((end - u.samples.len(), end));
                    turns.utterance_admitted(0);
                    at_recognizer.push(n + RECOGNITION_FRAMES);
                }
                CapMsg::Discarded { .. } => turns.utterance_discarded(),
                CapMsg::TurnQuiet => {
                    r.quiet.push(n);
                    turns.quiet();
                }
                _ => {}
            }
        }
        while at_recognizer.first().is_some_and(|&due| due <= n) {
            at_recognizer.remove(0);
            turns.recognized(Some(("words".to_string(), false)));
        }
        if turns.take_turn().is_some() {
            r.handed_over.push(n);
        }
    }
    r
}

/// The frames of the passage at speech level that no admitted utterance holds. `clean` is the
/// speech alone, at the level it has in `track`, and decides which frames are speech-level.
fn lost_frames(clean: &[f32], spans: &[(usize, usize)]) -> (usize, Vec<usize>) {
    let passage = passage();
    let mut speech = 0;
    let mut lost = Vec::new();
    for (n, frame) in clean.chunks_exact(VAD_FRAME_SAMPLES).enumerate() {
        let (a, b) = (n * VAD_FRAME_SAMPLES, (n + 1) * VAD_FRAME_SAMPLES);
        if !passage.contains(&a) || dbfs(frame) < CLEARLY_SPEECH_DBFS {
            continue;
        }
        speech += 1;
        if !spans.iter().any(|&(s, e)| s <= a && b <= e) {
            lost.push(n);
        }
    }
    (speech, lost)
}

fn assert_nothing_lost(what: &str, clean: &[f32], track: &[f32]) {
    let r = replay(track);
    println!(
        "[replay] {what}: admitted (s) {:?}",
        r.spans.iter().map(|(a, b)| (*a as f32 / 16000.0, *b as f32 / 16000.0)).collect::<Vec<_>>()
    );
    let (speech, lost) = lost_frames(clean, &r.spans);
    // POSITIVE CONTROL: the passage is speech at his level the whole way through, so its last
    // seconds hold plenty of frames this loud. Without them the verdict below proves nothing.
    let last_seconds = secs_to_samples(PASSAGE.1 - 3.0) / VAD_FRAME_SAMPLES;
    assert!(speech > 500, "only {speech} frames of the passage are at speech level");
    assert!(
        clean.chunks_exact(VAD_FRAME_SAMPLES).enumerate().skip(last_seconds).take(150).filter(|(_, f)| dbfs(f) >= CLEARLY_SPEECH_DBFS).count() > 20,
        "the passage's last 3 s carry no speech at his level"
    );
    assert!(
        lost.is_empty(),
        "{what}: {} of {speech} frames of speech never reached the recognizer; the first at {:.3} s and the last at {:.3} s \
         (the end of what was said was cut off)",
        lost.len(),
        frames_to_secs(lost[0] as u32),
        frames_to_secs(*lost.last().unwrap() as u32)
    );
}

#[test]
fn every_word_of_a_long_stretch_of_speech_reaches_the_recognizer() {
    // The fixture at his level, with 2.000 s of its own room after it so his last word can end.
    let mut mic = mic();
    let gain = gain_to(HIS_LEVEL_DBFS, &mic[passage()]);
    let room: Vec<f32> = mic[..secs_to_samples(PASSAGE.0)].to_vec();
    mic.extend(room.iter().cycle().take(secs_to_samples(2.0)));
    let track: Vec<f32> = mic.iter().map(|s| s * gain).collect();
    println!("[fixture] passage {:.1} dBFS over {:.3} s", dbfs(&track[passage()]), PASSAGE.1 - PASSAGE.0);
    assert_nothing_lost("over his quiet room", &track, &track);
}

/// The passage at his level with the room added under all of it at `room_dbfs` (handover review
/// rv-20261019T145840Z, `probe.py --mixed`): his room, recorded before playback, repeated.
fn over_room_noise(room_dbfs: f32) {
    let mic = mic();
    let room = &mic[..ROOM_FRAMES * VAD_FRAME_SAMPLES];
    let gain = gain_to(HIS_LEVEL_DBFS, &mic[passage()]);
    let room_gain = gain_to(room_dbfs, room);
    let clean: Vec<f32> = mic.iter().map(|s| s * gain).collect();
    let mut track = mic.clone();
    track.extend(room.iter().cycle().take(32_000));
    for (i, s) in track.iter_mut().enumerate() {
        *s = *s * gain + room[i % room.len()] * room_gain;
    }
    assert_nothing_lost(&format!("over room noise at {room_dbfs:.1} dBFS"), &clean, &track);
}

/// **Over ordinary room noise, 26 dB under his voice** (rv-20261019T145840Z, finding 1). Requiring
/// three clear frames in a row to renew the floor hold let it lapse inside his speech: 780 of 1287
/// speech-level frames lost from 10.784 s on.
#[test]
fn every_word_reaches_the_recognizer_over_room_noise_at_minus_44_dbfs() {
    over_room_noise(-44.0);
}

/// **Over a noisy room, 18 dB under his voice.** The room starts above the VAD's initial
/// threshold, so every frame read as speech until the 10.000 s stuck escape let the floor learn
/// from them all, his voice included: 706 of 1287 speech-level frames lost from 12.448 s on, at
/// every round of this fix and on main.
#[test]
fn every_word_reaches_the_recognizer_over_room_noise_at_minus_36_dbfs() {
    over_room_noise(-36.0);
}

/// The reviewers' noisy-room case: 1.600 s of a steady spoken level, then his room raised to
/// -36.0 dBFS for 20.000 s, with every 31st frame (every 0.496 s) and the `click_frames - 1` after
/// it raised to RMS `click_rms`: a click of `click_frames` x 16 ms.
fn noisy_room(click_frames: usize, click_rms: f32) -> Vec<f32> {
    let mic = mic();
    let room = &mic[..ROOM_FRAMES * VAD_FRAME_SAMPLES];
    let gain = gain_to(-36.0, room);
    let mut track = vec![0.12f32; SPOKEN_FRAMES * VAD_FRAME_SAMPLES];
    let noise: Vec<f32> = room.iter().cycle().take(1250 * VAD_FRAME_SAMPLES).map(|s| s * gain).collect();
    for (n, frame) in noise.chunks_exact(VAD_FRAME_SAMPLES).enumerate() {
        let g = if n % 31 < click_frames { click_rms / rms(frame) } else { 1.0 };
        track.extend(frame.iter().map(|s| s * g));
    }
    track
}

fn assert_his_turn_ends(what: &str, track: &[f32]) {
    let r = replay(track);
    println!(
        "[replay] {what}: started at frames {:?}, admitted {:?}, end-of-turn quiet at frames {:?}, handed over at {:?}",
        r.starts, r.spans, r.quiet, r.handed_over
    );
    assert_eq!(r.spans.len(), 1, "{what}: his speech never closed as an utterance");
    let within = SPOKEN_FRAMES + frames_for_secs(TURN_ENDS_WITHIN_SECS) as usize;
    assert!(
        r.quiet.first().is_some_and(|&q| q <= within),
        "{what}: his turn did not end within {TURN_ENDS_WITHIN_SECS:.3} s of his last word (quiet at {:?}), so it was not sent",
        r.quiet
    );
    assert_eq!(r.handed_over.len(), 1, "{what}: his turn was not handed over once");
}

/// **His turn still ends in a noisy room** (second review rv-20261019T123341Z, finding 1).
#[test]
fn his_turn_still_ends_when_the_room_is_noisy() {
    assert_his_turn_ends("noisy room", &noisy_room(0, 0.0));
}

/// **And with a click in the room as well** (rv-20261019T131907Z: 16 ms; rv-20261019T145840Z:
/// 48 ms, both at RMS 0.046 and 0.050, 26.7 and 26.0 dB below full scale, every 0.496 s). A click
/// that loud passes for clear speech against a floor that has not learned the room yet, and it
/// comes more often than any hold renewed by "clear speech" lapses.
#[test]
fn his_turn_still_ends_when_the_noisy_room_also_has_clicks() {
    for click_frames in [1, 3] {
        for click_rms in [0.046f32, 0.050] {
            let what = format!("noisy room, {} ms click at RMS {click_rms}", click_frames * 16);
            assert_his_turn_ends(&what, &noisy_room(click_frames, click_rms));
        }
    }
}

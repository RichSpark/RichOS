//! **Every word of a long stretch of speech reaches the recognizer — replayed on the CEO's rig.**
//!
//! The defect (nightly 65, the CEO's recording of 2026-10-18 at 8:27): the end of what he said
//! was missing from the message sent for him. Replayed through the shipped capture path, the
//! VAD's adaptive floor climbed while he was talking, because quiet frames INSIDE his speech (soft
//! consonants, the tails of words) were allowed to raise it. A higher floor calls more of his
//! frames non-speech, and they raise it further, until nothing he says is speech: the utterance
//! ends 0.800 s later in the middle of his sentence and the rest of it never starts another one.
//! `vad.rs` `FLOOR_RISE_HOLD_FRAMES` carries the measurement.
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
//! No sound is played anywhere. Everything here is samples in memory (CEO §53).

use richos_voice::aec::EchoCanceller;
use richos_voice::controller::{CapMsg, CaptureBrain};
use richos_voice::vad::{frames_to_secs, rms, SAMPLE_RATE, VAD_FRAME_SAMPLES};
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

fn secs_to_samples(s: f32) -> usize {
    (s * SAMPLE_RATE as f32).round() as usize
}

fn dbfs(x: &[f32]) -> f32 {
    20.0 * rms(x).max(1e-12).log10()
}

/// The fixture at his level, with 2.000 s of its own room after it so his last word can end.
fn his_rig_track() -> Vec<f32> {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.push(MIC);
    let bytes = std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    let pcm = wav::read_pcm16(&bytes).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    assert_eq!(pcm.sample_rate, SAMPLE_RATE);
    let mut mic = wav::to_mono(&pcm.samples, pcm.channels);
    let (a, b) = (secs_to_samples(PASSAGE.0), secs_to_samples(PASSAGE.1).min(mic.len()));
    let gain = 10f32.powf(HIS_LEVEL_DBFS / 20.0) / rms(&mic[a..b]);
    let room: Vec<f32> = mic[..a].to_vec();
    mic.extend(room.iter().cycle().take(secs_to_samples(2.0)));
    mic.iter().map(|s| s * gain).collect()
}

/// THE SHIPPED CAPTURE PATH: the sample spans of every utterance admitted for the recognizer.
fn admitted_spans(mic: &[f32]) -> Vec<(usize, usize)> {
    let (aec, ring) = EchoCanceller::new();
    let mut brain = CaptureBrain::with_aec(aec);
    let silence = vec![0.0f32; VAD_FRAME_SAMPLES];
    let mut spans = Vec::new();
    for (n, frame) in mic.chunks_exact(VAD_FRAME_SAMPLES).enumerate() {
        ring.push(&silence);
        for m in brain.push_frame(frame, false, false) {
            if let CapMsg::Utterance(u) = m {
                let end = (n + 1) * VAD_FRAME_SAMPLES;
                spans.push((end - u.samples.len(), end));
            }
        }
    }
    spans
}

#[test]
fn every_word_of_a_long_stretch_of_speech_reaches_the_recognizer() {
    let mic = his_rig_track();
    let passage = secs_to_samples(PASSAGE.0)..secs_to_samples(PASSAGE.1);
    println!("[fixture] passage {:.1} dBFS over {:.3} s", dbfs(&mic[passage.clone()]), PASSAGE.1 - PASSAGE.0);
    let spans = admitted_spans(&mic);
    println!("[replay] admitted (s): {:?}", spans.iter().map(|(a, b)| (*a as f32 / 16000.0, *b as f32 / 16000.0)).collect::<Vec<_>>());

    let mut speech = 0;
    let mut lost = Vec::new();
    for (n, frame) in mic.chunks_exact(VAD_FRAME_SAMPLES).enumerate() {
        let (a, b) = (n * VAD_FRAME_SAMPLES, (n + 1) * VAD_FRAME_SAMPLES);
        if !passage.contains(&a) || dbfs(frame) < CLEARLY_SPEECH_DBFS {
            continue;
        }
        speech += 1;
        if !spans.iter().any(|&(s, e)| s <= a && b <= e) {
            lost.push(n);
        }
    }
    // POSITIVE CONTROL: the passage is speech at his level the whole way through, so its last
    // seconds hold plenty of frames this loud. Without them the verdict below proves nothing.
    let last_seconds = secs_to_samples(PASSAGE.1 - 3.0) / VAD_FRAME_SAMPLES;
    assert!(speech > 500, "only {speech} frames of the passage are at speech level");
    assert!(
        mic.chunks_exact(VAD_FRAME_SAMPLES).enumerate().skip(last_seconds).take(150).filter(|(_, f)| dbfs(f) >= CLEARLY_SPEECH_DBFS).count() > 20,
        "the passage's last 3 s carry no speech at his level"
    );
    assert!(
        lost.is_empty(),
        "{} of {speech} frames of speech never reached the recognizer; the first at {:.3} s and the last at {:.3} s \
         (the end of what was said was cut off)",
        lost.len(),
        frames_to_secs(lost[0] as u32),
        frames_to_secs(*lost.last().unwrap() as u32)
    );
}

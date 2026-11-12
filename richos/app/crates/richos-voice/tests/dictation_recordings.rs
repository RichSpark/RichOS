//! **The recognizer a finished dictation runs, over recordings the caller names.**
//!
//! Opt-in, and a measurement rather than a gate: it puts each recording through the dictation
//! tool's own two steps, the voice gate ([`judge_recording`]) and then
//! [`Recognizer::transcribe_dictation_until`], on the pinned decoder and both speech choices,
//! and writes what it heard. Whoever runs it scores the words
//! against what was said. No recording is committed here: a dictation is somebody's voice.
//!
//! ```sh
//! RICHOS_DICTATION_PROBE_LIST=/path/to/list.txt \   # one 16 kHz mono PCM16 WAV path per line
//! RICHOS_DICTATION_PROBE_RUNTIME_BIN=/path/to/runtime/bin \
//! RICHOS_DICTATION_PROBE_OUT=/path/to/heard.json \
//!     cargo test -p richos-voice --test dictation_recordings what_the_dictation -- --ignored
//! ```
//!
//! The results it produced on 2026-11-12 are in `docs/measurements/richscribe-2026-11-12/`.

use richos_voice::dictation::{decode_bound, judge_recording, judge_transcript, Recording, Stopped, FASTER, MORE_ACCURATE};
use richos_voice::vad::VAD_FRAME_SAMPLES;
use richos_voice::stt::{self, Recognizer};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

#[test]
#[ignore = "Requires explicit RICHOS_DICTATION_PROBE_LIST, _RUNTIME_BIN and _OUT"]
fn what_the_dictation_recognizer_hears_in_these_recordings() {
    let required = |key: &str| std::env::var(key).unwrap_or_else(|_| panic!("set {key} explicitly"));
    let list = required("RICHOS_DICTATION_PROBE_LIST");
    let out = PathBuf::from(required("RICHOS_DICTATION_PROBE_OUT"));
    stt::set_delivered_runtime_bin(Some(PathBuf::from(required("RICHOS_DICTATION_PROBE_RUNTIME_BIN"))));
    let scratch = out.with_extension("scratch");
    let stop = AtomicBool::new(false);
    let mut heard = Vec::new();
    for model in [MORE_ACCURATE, FASTER] {
        stt::model_verified(model).unwrap_or_else(|why| panic!("{model} is not verified on this Mac: {why}"));
        let recognizer = Recognizer::for_model(model).unwrap();
        for path in std::fs::read_to_string(&list).unwrap().lines().map(str::trim).filter(|l| !l.is_empty()) {
            let pcm = richos_voice::wav::read_pcm16(&std::fs::read(path).unwrap()).unwrap();
            assert_eq!((pcm.sample_rate, pcm.channels), (16000, 1), "{path} must be 16 kHz mono");
            // A new folder per decode, as the tool gives each dictation its own.
            let dir = scratch.join(format!("{}", heard.len()));
            // The tool's own order: the voice gate first, and nothing is decoded that it refuses.
            let mut recording = Recording::new();
            pcm.samples.chunks(VAD_FRAME_SAMPLES).for_each(|frame| { recording.push(frame); });
            let duration_ms = pcm.samples.len() as u64 * 1000 / 16000;
            let entry = match judge_recording(duration_ms, &recording) {
                Stopped::Refuse(problem) => serde_json::json!({
                    "wav": path, "model": model, "seconds": pcm.samples.len() as f64 / 16000.0,
                    "refused": problem.tag(), "decode_ms": 0, "detect_ms": 0, "pieces": 0, "words": "",
                }),
                Stopped::Transcribe { .. } => {
                    let dictated = recognizer
                        .transcribe_dictation_until(&pcm.samples, &dir, decode_bound(pcm.samples.len()), &stop)
                        .unwrap_or_else(|why| panic!("{path} did not decode with {model}: {why}"));
                    let words = judge_transcript(&dictated.words).unwrap_or_default();
                    serde_json::json!({
                        "wav": path, "model": model, "seconds": pcm.samples.len() as f64 / 16000.0,
                        "decode_ms": dictated.decode_ms, "detect_ms": dictated.detect_ms, "pieces": dictated.pieces, "words": words,
                    })
                }
            };
            heard.push(entry);
            std::fs::write(&out, serde_json::to_vec_pretty(&heard).unwrap()).unwrap();
        }
    }
    std::fs::remove_dir_all(&scratch).unwrap();
}

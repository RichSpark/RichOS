//! One bounded provisional decoder. No microphone, insertion capability, queue,
//! transcript stitching, carried prompt or final-text authority.

use super::scratch::Scratch;
use richos_voice::{dictation::{judge_transcript, Recording, FASTER}, stt::Recognizer, voiced::VoiceEvidence};
use std::{path::Path, sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex}, time::{Duration, Instant}};

pub enum Update {
    Words { text: String, sampled_at: Instant, decode_ms: u64 },
    Paused,
    Unavailable,
    Quiet,
}

// Enough recent words for the three-line display, still only the latest
// provisional window rather than an accumulating transcript.
const PREVIEW_WORD_LIMIT: usize = 32;

pub fn recent_words(text: &str) -> String {
    let words: Vec<_> = text.split_whitespace().collect();
    let tail = words[words.len().saturating_sub(PREVIEW_WORD_LIMIT)..].join(" ");
    if words.len() > PREVIEW_WORD_LIMIT { format!("…{tail}") } else { tail }
}

/// Owned by the ordinary dictation's writing-worker lifecycle. `park_timeout`
/// wakes immediately when that owner unparks it on Stop/Cancel/quit. The only
/// blocking decode uses the existing kill/reap tether and a 1.8-second deadline.
pub fn run(recording: Arc<Mutex<Option<Recording>>>, data_dir: &Path, stop: &AtomicBool,
           mut send: impl FnMut(Update)) {
    if stop.load(Ordering::Acquire) { return; }
    let started = Instant::now();
    let recognizer = match Recognizer::for_model(FASTER) {
        Ok(recognizer) => recognizer,
        Err(_) => { if !stop.load(Ordering::Acquire) { send(Update::Unavailable); } return; }
    };
    let mut due = started + Duration::from_secs(1);
    let mut last_samples = 0;
    loop {
        if stop.load(Ordering::Acquire) { return; }
        if let Some(wait) = due.checked_duration_since(Instant::now()) {
            std::thread::park_timeout(wait);
            continue;
        }
        due += Duration::from_secs(2);
        let sampled_at = Instant::now();
        let samples = {
            // No wait for a busy capture callback, and no decoding/files/UI while
            // holding this lock. There is only the latest window, never backlog.
            let Ok(held) = recording.try_lock() else { continue };
            let Some(recording) = held.as_ref() else { return };
            if recording.samples().len() == last_samples { continue; }
            last_samples = recording.samples().len();
            recording.preview_samples().to_vec()
        };
        if !VoiceEvidence::measure(&samples).carried_speech() { send(Update::Quiet); continue; }
        if stop.load(Ordering::Acquire) { return; }
        let result = Scratch::new(data_dir).map_err(|_| ()).and_then(|scratch|
            recognizer.transcribe_bounded_until(&samples, scratch.path(), Duration::from_millis(1800), stop)
                .map_err(|_| ()));
        if stop.load(Ordering::Acquire) { return; }
        match result {
            Ok((text, decode_ms)) if sampled_at.elapsed() <= Duration::from_secs(2) => {
                match judge_transcript(&text) {
                    Ok(text) => send(Update::Words { text: recent_words(&text), sampled_at, decode_ms }),
                    Err(_) => { send(Update::Paused); return; }
                }
            }
            _ => { send(Update::Paused); return; }
        }
        // A slow startup/tick never causes a burst of catch-up decodes.
        if due < Instant::now() { due = Instant::now() + Duration::from_secs(2); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_keeps_only_recent_words_and_does_not_present_them_as_final_text() {
        assert_eq!(recent_words("one two"), "one two");
        assert_eq!(recent_words("one two three four five six seven eight nine ten eleven twelve"),
                   "one two three four five six seven eight nine ten eleven twelve");
        let full = (1..=32).map(|n| format!("word{n}")).collect::<Vec<_>>().join(" ");
        assert_eq!(recent_words(&full), full);
        assert_eq!(recent_words(&format!("discarded {full}")), format!("…{full}"));
        assert_eq!(recent_words("  one\n\ttwo  "), "one two");
        assert_eq!(recent_words(""), "");
    }

    #[test]
    fn canceled_preview_resolves_no_model_and_publishes_nothing() {
        let stopped = AtomicBool::new(true);
        run(Arc::new(Mutex::new(None)), Path::new("/not-used"), &stopped,
            |_| panic!("a canceled preview must not publish"));
    }

    /// Explicit file-only hardware measurement, using the production capture,
    /// preview, scratch and decoder paths. Reports measurements rather than
    /// treating this synthetic workload as a native UI or physical-mic gate.
    #[test]
    #[ignore = "Requires explicit RICHOS_PREVIEW_PROBE_WAV, _DATA, _OUT, _RUNTIME_BIN and _ENABLED"]
    fn measure_file_preview() {
        use richos_voice::{capture, dictation::{decode_bound, MORE_ACCURATE}, wav};
        let required = |key: &str| std::env::var(key).unwrap_or_else(|_| panic!("set {key} explicitly"));
        let path = std::path::PathBuf::from(required("RICHOS_PREVIEW_PROBE_WAV"));
        let data = std::path::PathBuf::from(required("RICHOS_PREVIEW_PROBE_DATA"));
        let output = std::path::PathBuf::from(required("RICHOS_PREVIEW_PROBE_OUT"));
        let runtime = std::path::PathBuf::from(required("RICHOS_PREVIEW_PROBE_RUNTIME_BIN"));
        let enabled = match required("RICHOS_PREVIEW_PROBE_ENABLED").as_str() {
            "1" => true, "0" => false, _ => panic!("_ENABLED must be 0 or 1"),
        };
        richos_voice::stt::set_delivered_runtime_bin(Some(runtime));
        richos_voice::stt::model_verified(FASTER).unwrap();
        richos_voice::stt::model_verified(MORE_ACCURATE).unwrap();
        let pcm = wav::read_pcm16(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!((pcm.sample_rate, pcm.channels), (16000, 1));
        let expected = pcm.samples.len() / 256 * 256;
        assert!(expected > 0);
        std::fs::create_dir_all(&data).unwrap();
        let recording = Arc::new(Mutex::new(Some(Recording::new())));
        let captured = recording.clone();
        let died = Arc::new(AtomicBool::new(false));
        let input_died = died.clone();
        let (done, finished) = std::sync::mpsc::channel();
        let began = Instant::now();
        let capture = capture::start(&capture::AudioSource::Wav(path), move |frame| {
            let mut held = captured.lock().unwrap();
            let recording = held.as_mut().unwrap();
            if recording.push(frame) { input_died.store(true, Ordering::Release); }
            if recording.samples().len() == expected { done.send(()).ok(); }
        }).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let updates = Arc::new(Mutex::new(Vec::new()));
        let worker = enabled.then(|| {
            let (recording, flag, updates, data) = (recording.clone(), stop.clone(), updates.clone(), data.clone());
            std::thread::spawn(move || run(recording, &data, &flag, |update| {
                let entry = match update {
                    Update::Words { sampled_at, decode_ms, .. } => serde_json::json!({
                        "kind":"words", "at_ms":began.elapsed().as_millis(),
                        "age_ms":sampled_at.elapsed().as_millis(), "decode_ms":decode_ms}),
                    Update::Quiet => serde_json::json!({"kind":"quiet","at_ms":began.elapsed().as_millis()}),
                    Update::Paused => serde_json::json!({"kind":"paused","at_ms":began.elapsed().as_millis()}),
                    Update::Unavailable => serde_json::json!({"kind":"unavailable","at_ms":began.elapsed().as_millis()}),
                };
                updates.lock().unwrap().push(entry);
            }))
        });
        // Hang guard only. Correctness is the complete captured sample prefix,
        // not whether the fixture finished within a performance assertion.
        finished.recv_timeout(Duration::from_secs_f64(pcm.duration_secs() as f64 + 30.)).unwrap();
        let stopped = Instant::now();
        drop(capture);
        stop.store(true, Ordering::Release);
        if let Some(worker) = worker { worker.thread().unpark(); worker.join().unwrap(); }
        let stop_ms = stopped.elapsed().as_millis();
        let samples = recording.lock().unwrap().take().unwrap().into_samples();
        assert!(samples[..expected] == pcm.samples[..expected], "preview must not lose or alter captured speech");
        let peak_bytes = |who| {
            // SAFETY: rusage contains numeric fields and is a valid writable out parameter.
            let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
            unsafe { libc::getrusage(who, &mut usage); }
            usage.ru_maxrss
        };
        let preview_peak = peak_bytes(libc::RUSAGE_CHILDREN);
        let decode = Instant::now();
        let scratch = Scratch::new(&data).unwrap();
        let final_stop = AtomicBool::new(false);
        let (text, decoder_ms) = Recognizer::for_model(MORE_ACCURATE).unwrap()
            .transcribe_bounded_until(&samples, scratch.path(), decode_bound(samples.len()), &final_stop).unwrap();
        drop(scratch);
        let final_ms = decode.elapsed().as_millis();
        let scratch_empty = std::fs::read_dir(data.join("dictation-scratch")).unwrap().next().is_none();
        let report = serde_json::json!({"preview":enabled,"expected_samples":expected,
            "captured_samples":samples.len(),"unchanged_prefix":true,"stop_join_ms":stop_ms,
            "input_died":died.load(Ordering::Acquire),
            "final_decode_ms":final_ms,"decoder_ms":decoder_ms,"final_text":text,
            "updates":*updates.lock().unwrap(),"self_peak_bytes":peak_bytes(libc::RUSAGE_SELF),
            "preview_child_peak_bytes":preview_peak,"all_child_peak_bytes":peak_bytes(libc::RUSAGE_CHILDREN),
            "scratch_empty":scratch_empty,
            "limitations":"Pinned models checked before capture. File injection; no cold-cache, UI delivery, physical microphone, normal RichOS workload or Intel verdict"});
        std::fs::write(output, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
        assert!(scratch_empty, "preview and final decoder scratch must be removed after reaping");
        assert!(!died.load(Ordering::Acquire), "fixture must remain a live input throughout its pauses");
    }

}

//! One bounded provisional decoder. No microphone, insertion capability, queue,
//! transcript stitching, carried prompt or final-text authority.

use super::{scratch::Scratch, timings::decoder_failure_reason};
use richos_voice::{dictation::{judge_transcript, Recording, FASTER}, stt::Recognizer, vad::SAMPLE_RATE, voiced::VoiceEvidence};
use std::{path::Path, sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex}, time::{Duration, Instant}};

pub enum Update {
    Words { text: String, sampled_at: Instant, decode_ms: u64 },
    MissedDeadline { input_ms: u64 },
    Paused { reason: &'static str, input_ms: Option<u64> },
    Unavailable,
    Quiet,
}

// One fixed recent-text allowance; no accumulating provisional transcript.
const PREVIEW_CHARACTER_LIMIT: usize = 200;
pub(super) const DISPLAY_MAX_AGE: Duration = Duration::from_secs(3);
const FIRST_RESULT_MAX_AGE: Duration = Duration::from_secs(2);
const FIRST_DECODER_LIMIT: Duration = Duration::from_millis(1800);
const RESULT_HANDOFF_MARGIN: Duration = Duration::from_millis(150);
const TICK_INTERVAL: Duration = Duration::from_secs(2);
const RECOVERY_WINDOW_SAMPLES: usize = 8 * SAMPLE_RATE as usize;
const RECOVERY_SPEECH_SAMPLES: usize = TICK_INTERVAL.as_secs() as usize * SAMPLE_RATE as usize;

fn preview_limits(has_words: bool, elapsed: Duration) -> (Duration, Duration) {
    let max_age = if has_words { DISPLAY_MAX_AGE - RESULT_HANDOFF_MARGIN } else { FIRST_RESULT_MAX_AGE };
    let max_decode = if has_words { max_age - Duration::from_millis(50) } else { FIRST_DECODER_LIMIT };
    (max_age, max_decode.saturating_sub(elapsed))
}

#[derive(Default)]
struct PreviewProgress {
    has_words: bool,
    missed_deadline: bool,
}

impl PreviewProgress {
    fn published(&mut self) {
        self.has_words = true;
        self.missed_deadline = false;
    }

    fn skip_deadline(&mut self) -> bool {
        if !self.has_words || self.missed_deadline { return false; }
        self.missed_deadline = true;
        true
    }

    fn window<'a>(&self, samples: &'a [f32], new_samples: usize) -> (&'a [f32], usize) {
        if self.missed_deadline {
            let start = samples.len().saturating_sub(RECOVERY_WINDOW_SAMPLES);
            (&samples[start..], new_samples.min(RECOVERY_SPEECH_SAMPLES))
        } else {
            (samples, new_samples)
        }
    }
}

// A short overlap keeps enough audio to recognize speech across adjacent ticks.
// Older words may provide decoder context, but cannot trigger a new decode.
const SPEECH_CHECK_OVERLAP: usize = SAMPLE_RATE as usize / 4;

fn new_speech_audio(samples: &[f32], new_samples: usize) -> &[f32] {
    &samples[samples.len().saturating_sub(new_samples.saturating_add(SPEECH_CHECK_OVERLAP))..]
}

pub fn recent_text(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= PREVIEW_CHARACTER_LIMIT { return text; }
    let start = text.char_indices().rev().nth(PREVIEW_CHARACTER_LIMIT - 2).unwrap().0;
    format!("…{}", &text[start..])
}

/// Owned by the ordinary dictation's writing-worker lifecycle. `park_timeout`
/// wakes immediately when that owner unparks it on Stop/Cancel/quit. The only
/// blocking decode uses the existing kill/reap tether. Later updates may use
/// more of the freshness allowance after the first words have appeared.
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
    let mut progress = PreviewProgress::default();
    loop {
        if stop.load(Ordering::Acquire) { return; }
        if let Some(wait) = due.checked_duration_since(Instant::now()) {
            std::thread::park_timeout(wait);
            continue;
        }
        due += TICK_INTERVAL;
        let sampled_at = Instant::now();
        let (samples, new_samples) = {
            // No wait for a busy capture callback, and no decoding/files/UI while
            // holding this lock. There is only the latest window, never backlog.
            let Ok(held) = recording.try_lock() else { continue };
            let Some(recording) = held.as_ref() else { return };
            let len = recording.samples().len();
            let new_samples = len.saturating_sub(last_samples);
            if new_samples == 0 { continue; }
            last_samples = len;
            let (samples, speech_samples) = progress.window(recording.preview_samples(), new_samples);
            (samples.to_vec(), speech_samples)
        };
        if !VoiceEvidence::measure(new_speech_audio(&samples, new_samples)).carried_speech() {
            send(Update::Quiet);
            continue;
        }
        if stop.load(Ordering::Acquire) { return; }
        let input_ms = samples.len() as u64 * 1000 / u64::from(SAMPLE_RATE);
        let paused = |reason| Update::Paused { reason, input_ms: Some(input_ms) };
        let result = Scratch::new(data_dir).map_err(|_| "scratch_unavailable").and_then(|scratch| {
            let (max_age, budget) = preview_limits(progress.has_words, sampled_at.elapsed());
            if budget.is_zero() { return Err("stale_result"); }
            recognizer.transcribe_preview_until(&samples, scratch.path(), budget, stop)
                .map(|result| (result, max_age))
                .map_err(|error| decoder_failure_reason(&error))
        });
        if stop.load(Ordering::Acquire) { return; }
        match result {
            Ok(((text, decode_ms), max_age)) if sampled_at.elapsed() <= max_age => {
                match judge_transcript(&text) {
                    Ok(text) => {
                        progress.published();
                        send(Update::Words { text: recent_text(&text), sampled_at, decode_ms });
                    }
                    Err(_) => { send(paused("transcript_rejected")); return; }
                }
            }
            Ok(_) => { send(paused("stale_result")); return; }
            Err("decoder_deadline") if progress.skip_deadline() => send(Update::MissedDeadline { input_ms }),
            Err(reason) => { send(paused(reason)); return; }
        }
        // An expired tick has no pending work. Use the next scheduled slot.
        let now = Instant::now();
        if due <= now {
            due += TICK_INTERVAL;
            if due <= now { due = now + TICK_INTERVAL; }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_budget_charges_preparation_and_leaves_time_for_display() {
        assert_eq!(preview_limits(false, Duration::from_millis(100)),
            (Duration::from_secs(2), Duration::from_millis(1700)));
        assert_eq!(preview_limits(true, Duration::from_millis(100)),
            (Duration::from_millis(2850), Duration::from_millis(2700)));
        assert!(preview_limits(true, DISPLAY_MAX_AGE).1.is_zero());
    }

    #[test]
    fn preview_skips_one_deadline_only_between_published_results() {
        let mut progress = PreviewProgress::default();
        assert!(!progress.skip_deadline());
        progress.published();
        assert!(progress.skip_deadline());
        assert!(!progress.skip_deadline());
        progress.published();
        assert!(progress.skip_deadline());
    }

    #[test]
    fn preview_recovery_decodes_less_audio_and_requires_recent_speech() {
        let mut progress = PreviewProgress::default();
        let samples: Vec<_> = (0..2 * RECOVERY_WINDOW_SAMPLES).map(|i| i as f32).collect();
        let new_samples = 2 * RECOVERY_SPEECH_SAMPLES;
        assert_eq!(progress.window(&samples, new_samples), (samples.as_slice(), new_samples));
        progress.published();
        assert!(progress.skip_deadline());
        assert_eq!(progress.window(&samples, new_samples),
            (&samples[RECOVERY_WINDOW_SAMPLES..], RECOVERY_SPEECH_SAMPLES));
        progress.published();
        assert_eq!(progress.window(&samples, new_samples), (samples.as_slice(), new_samples));
    }

    #[test]
    fn old_speech_cannot_trigger_a_decode_during_a_pause() {
        let mut samples = vec![0.0; SPEECH_CHECK_OVERLAP + 12_000];
        let mut phase = 0.0;
        for (i, sample) in samples[..8_000].iter_mut().enumerate() {
            phase += std::f32::consts::TAU * (115.0 + i as f32 / 140.0) / SAMPLE_RATE as f32;
            *sample = 0.1 * phase.sin();
        }
        assert!(VoiceEvidence::measure(&samples).carried_speech());
        assert!(!VoiceEvidence::measure(new_speech_audio(&samples, 4_000)).carried_speech());
        assert!(VoiceEvidence::measure(new_speech_audio(&samples, samples.len())).carried_speech());
    }

    #[test]
    fn speech_at_a_tick_boundary_keeps_its_overlap() {
        let samples = [0.0, 1.0, 2.0].repeat(SPEECH_CHECK_OVERLAP);
        let new_samples = 32;
        assert_eq!(new_speech_audio(&samples, new_samples),
            &samples[samples.len() - SPEECH_CHECK_OVERLAP - new_samples..]);
        assert_eq!(new_speech_audio(&samples, samples.len() * 2), &samples);
    }

    #[test]
    fn preview_keeps_two_hundred_recent_characters_without_changing_final_text() {
        assert_eq!(recent_text("  one\n\ttwo  "), "one two");
        assert_eq!(recent_text(""), "");
        let full = "a".repeat(200);
        assert_eq!(recent_text(&full), full);
        let long = format!("discarded {} approved", "é".repeat(200));
        let preview = recent_text(&long);
        assert_eq!(preview.chars().count(), 200);
        assert!(preview.starts_with('…'));
        assert!(preview.ends_with(" approved"));
        assert!(!preview.contains("discarded"));
        assert!(long.starts_with("discarded"), "the final transcript is untouched");
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
                    Update::Words { text, sampled_at, decode_ms } => serde_json::json!({
                        "kind":"words", "at_ms":began.elapsed().as_millis(),
                        "age_ms":sampled_at.elapsed().as_millis(), "decode_ms":decode_ms,
                        "characters":text.chars().count(), "words":text.split_whitespace().count()}),
                    Update::Quiet => serde_json::json!({"kind":"quiet","at_ms":began.elapsed().as_millis()}),
                    Update::MissedDeadline { input_ms } => serde_json::json!({"kind":"missed","input_ms":input_ms,"at_ms":began.elapsed().as_millis()}),
                    Update::Paused { reason, input_ms } => serde_json::json!({"kind":"paused","reason":reason,"input_ms":input_ms,"at_ms":began.elapsed().as_millis()}),
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

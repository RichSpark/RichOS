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

pub fn recent_words(text: &str) -> String {
    let words: Vec<_> = text.split_whitespace().collect();
    let tail = words[words.len().saturating_sub(10)..].join(" ");
    if words.len() > 10 { format!("…{tail}") } else { tail }
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
                   "…three four five six seven eight nine ten eleven twelve");
        assert_eq!(recent_words(""), "");
    }

    #[test]
    fn canceled_preview_resolves_no_model_and_publishes_nothing() {
        let stopped = AtomicBool::new(true);
        run(Arc::new(Mutex::new(None)), Path::new("/not-used"), &stopped,
            |_| panic!("a canceled preview must not publish"));
    }
}

//! **Rich's sentences are spoken through Pocket TTS with the Peter Yearsley voice** (CEO,
//! 2026-10-14: "Pocket TTS: Peter Yearsley").
//!
//! Two halves. The first runs everywhere: the voice controller's synthesizer IS `RichVoice`, the
//! thing that speaks with Pocket TTS, so no future edit can quietly put `say` back in front of
//! him. The second synthesizes a real sentence with the real, pinned weights and voice; the
//! weights are downloaded rather than committed, so without them it reports `ignored` (see
//! `build.rs` `pocket_voice_cfg`) and never `ok`.

use richos_voice::pocket;
use richos_voice::tts::{self, RichVoice, SpeechSynth};
use std::time::Duration;

/// The synthesizer voice mode builds is `RichVoice`, never `MacSay` directly.
#[test]
fn voice_mode_speaks_through_rich_voice_not_say() {
    let src = include_str!("../src/controller.rs");
    let production = src.split("\n#[cfg(test)]").next().unwrap();
    assert!(
        production.contains("let synth: Arc<dyn SpeechSynth> = Arc::new(RichVoice::waiting_while("),
        "voice mode must speak through RichVoice (Pocket TTS)"
    );
    assert!(!production.contains("Arc::new(MacSay::new())"), "voice mode builds `say` directly again");
}

/// With the pinned bundle on this machine, a sentence Rich says comes out of Pocket TTS as Peter
/// Yearsley: the label names both (it comes from the loaded engine, which verified every pinned
/// sha256, the voice recording's included), and the audio is real audio at the device rate.
#[test]
#[cfg_attr(not(pocket_voice), ignore = "Rich's Pocket TTS bundle is not installed on this machine (build.rs pocket_voice_cfg)")]
fn rich_speaks_through_pocket_tts_as_peter_yearsley() {
    let dir = pocket::bundle_dir().expect("a models directory");
    assert!(pocket::missing_files(&dir).is_empty(), "bundle incomplete at {}", dir.display());

    // Waits for the FACT (loaded, or failed), never for a clock: the load is a background thread.
    tts::warm_rich_voice(); // what the app does when it is up
    let voice = RichVoice::new();
    while !voice.pocket_ready() {
        assert!(!tts::rich_voice_load_failed(), "Pocket TTS failed to load (sha256 or graphs); see stderr");
        std::thread::sleep(Duration::from_millis(50));
    }
    let label = voice.voice_label();
    assert_eq!(label, "Rich's voice");

    let scratch = std::env::temp_dir().join(format!("richos-rich-voice-{}", std::process::id()));
    let speech = voice
        .synthesize("Acme signed at four and a half million. I would push back.", 48_000, &scratch)
        .expect("Rich's voice synthesizes");
    std::fs::remove_dir_all(&scratch).ok();
    assert_eq!(speech.sample_rate, 48_000);
    assert!(speech.duration_secs() > 1.5 && speech.duration_secs() < 10.0, "duration {}", speech.duration_secs());
    let peak = speech.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    assert!(peak > 0.05, "silent audio (peak {peak})");
    assert!(speech.samples.iter().all(|s| s.is_finite()));
    // Speed is MEASURED and printed, never asserted (a verdict on the clock depends on host
    // load). `cargo test --release -p richos-voice --test rich_voice -- --nocapture` shows it.
    eprintln!(
        "measured: {:.2} s of speech ready in {} ms (rtf {:.3}) by {label}",
        speech.duration_secs(),
        speech.synth_ms,
        speech.realtime_factor()
    );
}

/// **The last words of Rich's reply are in what he says** (nightly 65: his written reply ended
/// "It resets Wednesday at 9:00 AM.", and he was heard to say "... at 9" and stop). The reply goes
/// through the chunker the speaker uses, every sentence through Rich's own voice in order, and the
/// last sentence's audio is transcribed: its last words must be "9 AM". Nothing is played; the
/// audio stays in memory (CEO §53). Needs the pinned bundle and whisper's large-v3-turbo-q5_0.
#[test]
#[cfg_attr(not(pocket_voice), ignore = "Rich's Pocket TTS bundle is not installed on this machine (build.rs pocket_voice_cfg)")]
fn the_last_words_of_his_reply_are_in_what_rich_says() {
    use richos_voice::chunk::SentenceChunker;
    use richos_voice::stt::Recognizer;
    use richos_voice::vad::SAMPLE_RATE;

    let reply = "Also, a heads-up: your Claude account has used 95% of this week's limit. It resets Wednesday at 9:00 AM.";
    let mut chunker = SentenceChunker::new();
    let mut sentences = chunker.push(reply);
    sentences.extend(chunker.flush());

    let dir = pocket::bundle_dir().expect("a models directory");
    let synth = pocket::PocketSynth::load(&dir).expect("Rich's voice loads");
    let scratch = std::env::temp_dir().join(format!("richos-rich-voice-tail-{}", std::process::id()));
    let mut last = Vec::new();
    for sentence in &sentences {
        last.clear();
        synth
            .synthesize_stream(sentence, SAMPLE_RATE, &scratch, &mut |pcm| {
                last.extend_from_slice(pcm);
                true
            })
            .expect("Rich's voice synthesizes");
    }
    let ear = Recognizer::for_model("large-v3-turbo-q5_0").expect("whisper large-v3-turbo-q5_0 on this machine");
    let (heard, _) = ear.transcribe(&last, &scratch).expect("the recognizer runs");
    std::fs::remove_dir_all(&scratch).ok();
    let words: String = heard.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect();
    println!("[rich] said {sentences:?}; the last one heard as {heard:?}");
    assert!(
        words.ends_with("wednesdayat9am"),
        "the end of Rich's reply was not said: the last sentence {:?} was heard as {heard:?}",
        sentences.last()
    );
}

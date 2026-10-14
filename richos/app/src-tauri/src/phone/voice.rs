//! Bounded phone audio uses the same offline speech stack as the desktop.
use std::{path::PathBuf, sync::Mutex};
use richos_voice::{stt::Recognizer, tts::RichVoice, voiced::VoiceEvidence, wav};

// Thirty minutes of 16 kHz PCM is 57.6 MB, below the managed edge body ceiling.
pub const MAX_SECONDS: usize = 30 * 60;
pub const MAX_UPLOAD: usize = 60_000_000;
pub const MAX_REPLY: usize = 6_000_000;
pub struct VoiceDesk { recognizer: Option<Recognizer>, busy: Mutex<()>, directory: PathBuf }
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        // What this folder holds is his recorded voice, so a copy left behind on disk is said
        // rather than dropped. Already gone is the one outcome that is not a failure.
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            if error.kind() != std::io::ErrorKind::NotFound {
                eprintln!("[richos] a phone recording's scratch folder could not be removed: {error}");
            }
        }
    }
}
impl VoiceDesk {
    pub fn new(directory: PathBuf) -> Self { Self { recognizer: Recognizer::resolve().ok(), busy: Mutex::new(()), directory } }
    pub fn available(&self) -> bool { self.recognizer.is_some() }
    fn scratch(&self) -> Result<Scratch, String> {
        use std::os::unix::fs::DirBuilderExt;
        let path = self.directory.join(super::hex(&super::random_bytes(12).map_err(|_| "Audio storage is unavailable")?));
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(&path).map_err(|_| "Audio storage is unavailable")?;
        Ok(Scratch(path))
    }
    pub fn transcribe(&self, bytes: &[u8]) -> Result<String, String> {
        let _guard = self.busy.try_lock().map_err(|_| "Rich is already processing audio. Try this recording again shortly.")?;
        let samples = validate(bytes)?;
        if !VoiceEvidence::measure(&samples).carried_speech() { return Err("I could not hear speech in that recording. Your recording is still on your phone.".into()); }
        let recognizer = self.recognizer.as_ref().ok_or("Speech recognition is not ready on this Mac. Whoever set RichOS up needs to prepare it. Your recording is still on your phone.")?;
        let scratch = self.scratch()?;
        let (text, _) = recognizer.transcribe_bounded(&samples, &scratch.0,std::time::Duration::from_secs(90)).map_err(|_| "I could not transcribe that recording. Your recording is still on your phone.")?;
        let text = clean_transcript(&text);
        if text.trim().is_empty() || text.len() > 48000 { return Err("I could not understand that recording. Your recording is still on your phone.".into()); }
        Ok(text)
    }
    pub fn synthesize(&self, text: &str) -> Result<Vec<u8>, String> {
        let _guard = self.busy.try_lock().map_err(|_| "Rich is already processing audio. Try playback again shortly.")?;
        if text.is_empty() || text.chars().count() > 2400 { return Err("This reply is too long for audio playback. The complete reply is in the conversation.".into()); }
        let scratch = self.scratch()?;
        crate::voice_provision::fetch_rich_voice_in_background();
        let speech = RichVoice::new().synthesize_bounded(text,16000,&scratch.0,std::time::Duration::from_secs(20)).map_err(|_| "Audio playback is unavailable. The reply remains in the conversation.")?;
        let bytes = wav::encode_pcm16_mono(&speech.samples,16000);
        if bytes.len() > MAX_REPLY { return Err("This reply is too long for audio playback. The complete reply is in the conversation.".into()); }
        Ok(bytes)
    }
}
// Whisper emits this marker for silent spans within an otherwise useful recording.
// Remove the known marker, not arbitrary brackets or parenthesized spoken content.
fn clean_transcript(text: &str) -> String {
    text.replace("[BLANK_AUDIO]", " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn validate(bytes: &[u8]) -> Result<Vec<f32>, String> {
    if bytes.len() > MAX_UPLOAD { return Err("The recording exceeds the upload limit.".into()); }
    let pcm = wav::read_pcm16(bytes).map_err(|_| "This recording is not a supported WAV file.")?;
    if pcm.channels != 1 || pcm.sample_rate != 16000 || pcm.samples.is_empty() || pcm.samples.len() > MAX_SECONDS * 16000 {
        return Err("This voice message exceeds the 30-minute sending limit. It has been kept on your phone.".into());
    }
    Ok(pcm.samples)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn silent_spans_never_become_conversation_text() {
        assert_eq!(clean_transcript("A quick message. [BLANK_AUDIO] Then another."), "A quick message. Then another.");
        assert_eq!(clean_transcript("[BLANK_AUDIO] [BLANK_AUDIO]"), "");
        assert_eq!(clean_transcript("Keep [the budget] (including travel)."), "Keep [the budget] (including travel).");
    }
    #[test] fn validates_actual_audio_not_claimed_duration() {
        assert!(validate(&wav::encode_pcm16_mono(&vec![0.1;120 * 16000],16000)).is_ok());
        for b in [wav::encode_pcm16_mono(&vec![0.1;MAX_SECONDS * 16000 + 1],16000), wav::encode_pcm16_mono(&[0.1;16],48000), vec![0;MAX_UPLOAD+1], b"not audio".to_vec()] { assert!(validate(&b).is_err()); }
    }
}

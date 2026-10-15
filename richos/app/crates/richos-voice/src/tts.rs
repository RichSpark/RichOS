//! Text to speech — ONE Rich voice, behind a trait so it can be replaced.
//!
//! CEO decision, 2026-08-24: *"ONE bundled Rich voice for v1. Voice choice/cloning is a later
//! feature — do not build selection UI."* So there is no picker, no settings row, and no
//! per-thread voice.
//!
//! **Rich's voice is Pocket TTS ([`RichVoice`], `pocket.rs`) and nothing else** (CEO, 2026-10-15:
//! *"It's Rich's voice. Why the fuck would there be any other names in any log lines???"*). No
//! macOS voice stands in for it: while it is being installed or loaded a sentence waits for it
//! ([`VoiceGate`]), and every log line names it as Rich's voice.
//!
//! [`MacSay`] stays for the echo-path measuring tools (`examples/aec_*.rs`), which need a
//! loudspeaker signal and are never Rich speaking to anyone. Measured on this M4 when it was v1's
//! synthesizer: a 4.358 s sentence synthesizes to 48 kHz PCM in **0.324 s**, a real-time factor
//! of **0.074**.

use crate::pocket::{self, PocketSynth};
use crate::wav;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// [`MacSay`]'s voice for the echo-path measuring tools, in preference order: the SAME voice at
/// increasing quality first, then a fallback for a stripped system. Never Rich's voice.
pub const VOICE_PREFERENCE: &[&str] =
    &["Daniel (Premium)", "Daniel (Enhanced)", "Daniel", "Reed (English (US))", "Samantha"];

/// Words per minute. `say`'s default is faster than a chief of staff should sound.
pub const DEFAULT_RATE_WPM: u32 = 180;

#[derive(Debug)]
pub enum TtsError {
    Io(String),
    Failed { status: String, stderr: String },
    Decode(String),
    /// Rich's voice cannot speak this sentence: its install or load failed, or voice went off
    /// while the sentence waited for it. No other voice stands in.
    Unavailable(String),
}

impl TtsError {
    /// NAMES THE PARTY. Speech synthesis failing is not something the CEO can fix from
    /// inside the app, and the old line did not say so — it read as a fault he might be
    /// expected to chase. Naming the owner is what a NEEDS-SOMEONE-ELSE state owes its
    /// reader. ("text" is load-bearing here: this file's own
    /// `a_synthesis_failure_degrades_to_text_without_leaking_machinery` asserts the CEO is
    /// told the conversation continues.)
    pub fn ceo_message(&self) -> String {
        "My voice isn't working on this machine. Whoever set RichOS up would need to look at \
         that. I'll keep answering in text."
            .into()
    }
}

impl std::fmt::Display for TtsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TtsError::Io(s) => write!(f, "tts io: {s}"),
            TtsError::Failed { status, stderr } => write!(f, "say failed ({status}): {stderr}"),
            TtsError::Decode(s) => write!(f, "tts decode: {s}"),
            TtsError::Unavailable(s) => write!(f, "{s}"),
        }
    }
}

/// One synthesized sentence: mono f32 at the rate that was asked for, plus the measurement.
#[derive(Debug, Clone)]
pub struct Speech {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    /// Wall-clock synthesis time — measured, so the real-time factor in the brief is real.
    pub synth_ms: u64,
}

impl Speech {
    pub fn duration_secs(&self) -> f32 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.samples.len() as f32 / self.sample_rate as f32
    }
    /// synthesis time / audio duration. Below 1.0 means synthesis outruns playback, which is
    /// the precondition for gapless pipelining.
    pub fn realtime_factor(&self) -> f32 {
        let d = self.duration_secs();
        if d <= 0.0 {
            return 0.0;
        }
        (self.synth_ms as f32 / 1000.0) / d
    }
}

/// The replaceable synthesizer. A bundled neural voice implements this and nothing else in
/// the crate changes.
pub trait SpeechSynth: Send + Sync {
    /// Synthesize one sentence as mono f32 at `target_rate` (the output device's rate, so
    /// playout never resamples Rich's voice).
    fn synthesize(&self, text: &str, target_rate: u32, scratch_dir: &Path) -> Result<Speech, TtsError>;
    /// Deliver PCM as it becomes available. Returning false cancels further generation.
    /// The default keeps whole-sentence synthesizers compatible.
    fn synthesize_stream(&self, text: &str, target_rate: u32, scratch_dir: &Path, emit: &mut dyn FnMut(&[f32]) -> bool) -> Result<(), TtsError> {
        let speech = self.synthesize(text, target_rate, scratch_dir)?;
        emit(&speech.samples);
        Ok(())
    }
    /// Honest identity for logs and the brief — never shown to the CEO.
    fn voice_label(&self) -> String;
}

/// macOS `say`, for the echo-path measuring tools only. Never Rich's voice.
pub struct MacSay {
    voice: String,
    rate_wpm: u32,
}

impl MacSay {
    pub fn new() -> MacSay {
        MacSay {
            voice: pick_voice(),
            rate_wpm: std::env::var("RICHOS_TTS_RATE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(DEFAULT_RATE_WPM),
        }
    }

    pub fn voice(&self) -> &str {
        &self.voice
    }
    pub fn rate_wpm(&self) -> u32 {
        self.rate_wpm
    }
}

impl Default for MacSay {
    fn default() -> Self {
        MacSay::new()
    }
}

impl MacSay {
    pub fn synthesize_bounded(&self,text:&str,target_rate:u32,scratch_dir:&Path,timeout:std::time::Duration)->Result<Speech,TtsError> {
        self.synthesize_inner(text,target_rate,scratch_dir,Some(timeout))
    }
    fn synthesize_inner(&self,text:&str,target_rate:u32,scratch_dir:&Path,timeout:Option<std::time::Duration>)->Result<Speech,TtsError> {
        std::fs::create_dir_all(scratch_dir).map_err(|e| TtsError::Io(e.to_string()))?;
        let stamp = format!("{}-{:?}", std::process::id(), std::thread::current().id());
        let txt_path = scratch_dir.join(format!("say-{stamp}.txt"));
        let wav_path = scratch_dir.join(format!("say-{stamp}.wav"));

        // Text goes through a FILE, never argv: a sentence starting with "-" would otherwise
        // be parsed as a flag, and Rich's own words must never become command-line options.
        std::fs::write(&txt_path, text).map_err(|e| TtsError::Io(e.to_string()))?;

        let started = Instant::now();
        let mut command = Command::new("/usr/bin/say");
        command
            .arg("-v")
            .arg(&self.voice)
            .arg("-r")
            .arg(self.rate_wpm.to_string())
            .arg("-f")
            .arg(&txt_path)
            .arg("-o")
            .arg(&wav_path)
            .arg(format!("--data-format=LEI16@{target_rate}"));
        let out=match timeout {Some(timeout)=>crate::stt::bounded_decoder(&mut command,scratch_dir,timeout),None=>command.output()}
            .map_err(|e|TtsError::Io(e.to_string()))?;
        let synth_ms = started.elapsed().as_millis() as u64;
        let _ = std::fs::remove_file(&txt_path);

        if !out.status.success() {
            let _ = std::fs::remove_file(&wav_path);
            return Err(TtsError::Failed {
                status: out.status.to_string(),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            });
        }
        let bytes = std::fs::read(&wav_path).map_err(|e| TtsError::Io(e.to_string()))?;
        let _ = std::fs::remove_file(&wav_path);
        let pcm = wav::read_pcm16(&bytes).map_err(TtsError::Decode)?;
        let (mono, rate) = pcm.into_mono();
        // `say` honours the requested rate, but never assume it: convert if it did not.
        let samples = if rate == target_rate { mono } else { wav::resample(&mono, rate, target_rate) };
        Ok(Speech { samples, sample_rate: target_rate, synth_ms })
    }

}
impl SpeechSynth for MacSay {
    fn synthesize(&self,text:&str,target_rate:u32,scratch_dir:&Path)->Result<Speech,TtsError> {
        self.synthesize_inner(text,target_rate,scratch_dir,None)
    }
    fn voice_label(&self) -> String {
        format!("macOS say · {} · {} wpm", self.voice, self.rate_wpm)
    }
}

// ---------------------------------------------------------------------------------------------
// Rich's voice: Pocket TTS, and nothing else. A sentence waits while it is installed or loaded.
// ---------------------------------------------------------------------------------------------

/// How often a waiting sentence asks whether it is still wanted (voice mode still on). Only a
/// voice-off can end the wait between two state changes, and 100 ms is well inside how fast
/// switching voice off already feels (`FINISH_WAIT` alone is 500 ms).
const WAIT_SLICE: Duration = Duration::from_millis(100);

/// Where Rich's voice has got to on this machine.
enum VoiceStage<T> {
    /// Not loaded, and no install or load running. The files may or may not be on disk.
    Idle,
    /// The app is installing the voice files (`voice_provision.rs`). A sentence waits.
    Installing,
    /// The files are here and the engine is loading. A sentence waits.
    Loading,
    Ready(Arc<T>),
    /// The last load failed. Not retried per sentence; [`warm_rich_voice`] tries again.
    Failed,
}

/// **The one place a sentence waits for Rich's voice** (CEO, 2026-10-15: no other voice, ever;
/// a voice that is not installed gets installed). Generic so the waiting can be proved without
/// the 166 MB of weights: the engine is [`PocketSynth`] in the app and a stand-in in the tests.
pub struct VoiceGate<T> {
    stage: Mutex<VoiceStage<T>>,
    changed: Condvar,
}

impl<T> VoiceGate<T> {
    pub const fn new() -> Self {
        VoiceGate { stage: Mutex::new(VoiceStage::Idle), changed: Condvar::new() }
    }

    fn lock(&self) -> MutexGuard<'_, VoiceStage<T>> {
        self.stage.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// The app has started installing the voice files.
    pub fn installing(&self) {
        let mut stage = self.lock();
        if matches!(*stage, VoiceStage::Idle | VoiceStage::Failed) {
            *stage = VoiceStage::Installing;
        }
        self.changed.notify_all();
    }

    /// The install has ended, either way. A waiting sentence then loads the files if all are here.
    pub fn install_ended(&self) {
        let mut stage = self.lock();
        if matches!(*stage, VoiceStage::Installing) {
            *stage = VoiceStage::Idle;
        }
        self.changed.notify_all();
    }

    /// Claim the load when nothing holds or loads the voice and its files are here. `true`: the
    /// caller loads it and reports with [`VoiceGate::loaded`].
    pub fn claim_load(&self, have_files: bool) -> bool {
        let mut stage = self.lock();
        if have_files && matches!(*stage, VoiceStage::Idle | VoiceStage::Failed) {
            *stage = VoiceStage::Loading;
            return true;
        }
        false
    }

    /// The load has ended: the engine, or `None` when it failed.
    pub fn loaded(&self, engine: Option<T>) {
        *self.lock() = match engine {
            Some(engine) => VoiceStage::Ready(Arc::new(engine)),
            None => VoiceStage::Failed,
        };
        self.changed.notify_all();
    }

    pub fn ready(&self) -> Option<Arc<T>> {
        match &*self.lock() {
            VoiceStage::Ready(engine) => Some(engine.clone()),
            _ => None,
        }
    }

    pub fn failed(&self) -> bool {
        matches!(*self.lock(), VoiceStage::Failed)
    }

    /// The stage in words, for the log lines that name Rich's voice before it can speak.
    pub fn describe(&self) -> &'static str {
        match *self.lock() {
            VoiceStage::Idle => "not installed",
            VoiceStage::Installing => "being installed; Rich waits for it",
            VoiceStage::Loading => "loading; Rich waits for it",
            VoiceStage::Ready(_) => "ready",
            VoiceStage::Failed => "could not load",
        }
    }

    /// **Wait for Rich's voice while it is being installed or loaded**, starting the load when its
    /// files are here and nothing has loaded them. `None` only when it cannot come: no install is
    /// running and the files are absent, the load failed, `deadline` passed, or `keep_waiting`
    /// says the sentence is no longer wanted.
    pub fn wait(
        &self,
        have_files: &dyn Fn() -> bool,
        load: &dyn Fn(),
        deadline: Option<Instant>,
        keep_waiting: &dyn Fn() -> bool,
    ) -> Option<Arc<T>> {
        let mut stage = self.lock();
        loop {
            match &*stage {
                VoiceStage::Ready(engine) => return Some(engine.clone()),
                VoiceStage::Failed => return None,
                VoiceStage::Idle => {
                    if !have_files() {
                        return None;
                    }
                    *stage = VoiceStage::Loading;
                    drop(stage);
                    load();
                    stage = self.lock();
                }
                VoiceStage::Installing | VoiceStage::Loading => {
                    if !keep_waiting() {
                        return None;
                    }
                    let mut slice = WAIT_SLICE;
                    if let Some(deadline) = deadline {
                        let now = Instant::now();
                        if now >= deadline {
                            return None;
                        }
                        slice = slice.min(deadline - now);
                    }
                    stage = match self.changed.wait_timeout(stage, slice) {
                        Ok((guard, _)) => guard,
                        Err(poisoned) => poisoned.into_inner().0,
                    };
                }
            }
        }
    }
}

impl<T> Default for VoiceGate<T> {
    fn default() -> Self {
        VoiceGate::new()
    }
}

/// Rich's voice in this process. One per process: the graphs are ~165 MB and loading them is
/// seconds, so a second voice-mode start must not pay it again.
static POCKET: VoiceGate<PocketSynth> = VoiceGate::new();

fn have_rich_voice_files() -> bool {
    pocket::bundle_dir().is_some_and(|dir| pocket::missing_files(&dir).is_empty())
}

/// Load the voice on a background thread; the gate is already `Loading`.
fn load_rich_voice() {
    let Some(dir) = pocket::bundle_dir() else {
        POCKET.loaded(None);
        return;
    };
    // Loaded in silence (CEO, 2026-10-15: no log lines about the voice). Only a FAULT is written,
    // because a load that fails with nothing on record cannot be diagnosed.
    std::thread::spawn(move || {
        let loaded = PocketSynth::load(&dir);
        if let Err(e) = &loaded {
            eprintln!("[richos-voice] speech synthesis could not load: {e}");
        }
        POCKET.loaded(loaded.ok());
    });
}

/// Start loading Rich's voice in the background if its files are all present and it is not
/// loaded. Returns at once. The app calls it when it is up and right after it installs the voice,
/// so a sentence finds it ready; one that does not waits for it ([`VoiceGate::wait`]).
pub fn warm_rich_voice() {
    if POCKET.claim_load(have_rich_voice_files()) {
        load_rich_voice();
    }
}

/// The app has started installing Rich's voice (`voice_provision.rs`): a sentence waits for it.
pub fn rich_voice_installing() {
    POCKET.installing();
}

/// The app's install of Rich's voice has ended, either way: a waiting sentence loads it.
pub fn rich_voice_install_ended() {
    POCKET.install_ended();
}

/// True when the last load of Rich's voice failed (its reason is on stderr).
pub fn rich_voice_load_failed() -> bool {
    POCKET.failed()
}

/// **Rich's voice: Pocket TTS, and only Pocket TTS** (CEO, 2026-10-14 and 2026-10-15). No other
/// voice ever speaks for him: while his voice is being installed or loaded, a sentence waits for
/// it; when it cannot come (the install failed, the load failed, voice was switched off) the
/// sentence fails and the conversation continues in text.
pub struct RichVoice {
    /// False ends a wait for the voice: voice mode was switched off while the sentence waited.
    keep_waiting: Box<dyn Fn() -> bool + Send + Sync>,
}

impl RichVoice {
    pub fn new() -> RichVoice {
        RichVoice::waiting_while(|| true)
    }

    /// A voice whose sentences stop waiting for it once `keep_waiting` turns false. Creating one
    /// loads nothing: the app loads Rich's voice when it is up (`voice_provision.rs`), never at
    /// a Talk press (CEO, 2026-10-15).
    pub fn waiting_while(keep_waiting: impl Fn() -> bool + Send + Sync + 'static) -> RichVoice {
        RichVoice { keep_waiting: Box::new(keep_waiting) }
    }

    fn voice(&self, deadline: Option<Instant>) -> Result<Arc<PocketSynth>, TtsError> {
        if let Some(engine) = POCKET.ready() {
            return Ok(engine);
        }
        POCKET
            .wait(&have_rich_voice_files, &load_rich_voice, deadline, &*self.keep_waiting)
            .ok_or_else(|| TtsError::Unavailable(format!("speech synthesis unavailable: {}", POCKET.describe())))
    }

    /// A whole reply for a caller with a deadline (the phone channel): the wait for the voice is
    /// bounded by `timeout`; synthesis itself runs in this process and cannot hang like a child.
    pub fn synthesize_bounded(&self, text: &str, target_rate: u32, scratch_dir: &Path, timeout: Duration) -> Result<Speech, TtsError> {
        self.voice(Some(Instant::now() + timeout))?.synthesize(text, target_rate, scratch_dir)
    }

    /// True when Rich's voice is loaded and the next sentence speaks at once.
    pub fn pocket_ready(&self) -> bool {
        POCKET.ready().is_some()
    }
}

impl Default for RichVoice {
    fn default() -> Self {
        RichVoice::new()
    }
}

impl SpeechSynth for RichVoice {
    fn synthesize(&self, text: &str, target_rate: u32, scratch_dir: &Path) -> Result<Speech, TtsError> {
        let asked = Instant::now();
        let pocket = self.voice(None)?;
        let wait_ms = asked.elapsed().as_millis();
        let speech = pocket.synthesize(text, target_rate, scratch_dir)?;
        // THE TIME TO AUDIO, per sentence, measured: a timing figure for QA, naming no voice.
        // `before synthesis` is the time the sentence waited for the voice to be ready.
        eprintln!(
            "[richos-voice] tts: {:.2} s of speech synthesized in {} ms (rtf {:.3}), {wait_ms} ms before synthesis",
            speech.duration_secs(),
            speech.synth_ms,
            speech.realtime_factor()
        );
        Ok(speech)
    }

    fn synthesize_stream(&self, text: &str, target_rate: u32, scratch_dir: &Path, emit: &mut dyn FnMut(&[f32]) -> bool) -> Result<(), TtsError> {
        let asked = Instant::now();
        let pocket = self.voice(None)?;
        let wait_ms = asked.elapsed().as_millis();
        let started = Instant::now();
        let mut first_ms = None;
        let mut samples = 0usize;
        pocket.synthesize_stream(text, target_rate, scratch_dir, &mut |pcm| {
            first_ms.get_or_insert(started.elapsed().as_millis());
            samples += pcm.len();
            emit(pcm)
        })?;
        // The same timing figures the streaming engine used to print, naming no voice.
        eprintln!(
            "[richos-voice] tts: {:.2} s of speech streamed in {} ms, first audio in {} ms, {wait_ms} ms before synthesis",
            samples as f64 / target_rate.max(1) as f64,
            started.elapsed().as_millis(),
            first_ms.unwrap_or(0)
        );
        Ok(())
    }

    fn voice_label(&self) -> String {
        "Rich's voice".into()
    }
}

/// Choose THE voice. `RICHOS_TTS_VOICE` wins; otherwise the first installed entry of
/// [`VOICE_PREFERENCE`]; otherwise "Samantha", which ships on every Mac.
pub fn pick_voice() -> String {
    if let Ok(v) = std::env::var("RICHOS_TTS_VOICE") {
        if !v.trim().is_empty() {
            return v;
        }
    }
    let installed = installed_voices();
    for want in VOICE_PREFERENCE {
        if installed.iter().any(|v| v == want) {
            return (*want).to_string();
        }
    }
    "Samantha".to_string()
}

/// Voice names `say -v '?'` reports. Parses the fixed-width listing: the name runs up to the
/// locale column (two-or-more spaces followed by a `xx_YY` token).
pub fn installed_voices() -> Vec<String> {
    let Ok(out) = Command::new("/usr/bin/say").args(["-v", "?"]).output() else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout).lines().filter_map(parse_voice_line).collect()
}

/// Pull the voice name out of one `say -v '?'` line. Names contain spaces and parentheses
/// ("Daniel (Enhanced)", "Eddy (English (UK))"), so the split point is the locale token.
pub fn parse_voice_line(line: &str) -> Option<String> {
    let idx = line.find(|c: char| c == '#')?;
    let before = &line[..idx];
    // The locale is the last whitespace-separated token before the comment marker.
    let mut parts: Vec<&str> = before.split_whitespace().collect();
    parts.pop()?; // the locale, e.g. en_GB
    let name = parts.join(" ");
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// INVARIANT: the preference list is the SAME voice at three quality tiers first, so a
    /// CEO installing an Enhanced variant upgrades Rich's voice without changing his
    /// identity — and without any code change.
    #[test]
    fn the_voice_preference_upgrades_quality_without_changing_identity() {
        assert_eq!(VOICE_PREFERENCE[0], "Daniel (Premium)");
        assert_eq!(VOICE_PREFERENCE[1], "Daniel (Enhanced)");
        assert_eq!(VOICE_PREFERENCE[2], "Daniel");
        assert!(VOICE_PREFERENCE[..3].iter().all(|v| v.starts_with("Daniel")));
    }

    /// INVARIANT: `say -v '?'` lines parse into names, including the parenthesized ones that
    /// a naive "first token" split would truncate.
    #[test]
    fn voice_lines_parse_including_parenthesised_names() {
        assert_eq!(parse_voice_line("Daniel              en_GB    # Hello!").as_deref(), Some("Daniel"));
        assert_eq!(
            parse_voice_line("Eddy (English (UK)) en_GB    # Hello! My name is Eddy.").as_deref(),
            Some("Eddy (English (UK))")
        );
        assert_eq!(
            parse_voice_line("Daniel (Enhanced)   en_GB    # Hello!").as_deref(),
            Some("Daniel (Enhanced)")
        );
        assert_eq!(parse_voice_line("Bad News            en_US    # Hello!").as_deref(), Some("Bad News"));
        assert_eq!(parse_voice_line("no comment marker here"), None);
    }

    /// INVARIANT: duration and real-time factor are DERIVED from the samples and the
    /// measured clock — never stored, never estimated.
    #[test]
    fn speech_metrics_are_derived_not_stored() {
        let s = Speech { samples: vec![0.0; 48_000], sample_rate: 48_000, synth_ms: 324 };
        assert!((s.duration_secs() - 1.0).abs() < 1e-6);
        assert!((s.realtime_factor() - 0.324).abs() < 1e-6);
        let empty = Speech { samples: vec![], sample_rate: 48_000, synth_ms: 10 };
        assert_eq!(empty.duration_secs(), 0.0);
        assert_eq!(empty.realtime_factor(), 0.0);
    }

    /// INVARIANT: a synthesis failure never leaks machinery to the CEO — and never stops the
    /// conversation, because text still works.
    #[test]
    fn a_synthesis_failure_degrades_to_text_without_leaking_machinery() {
        let e = TtsError::Failed { status: "exit code: 1".into(), stderr: "/usr/bin/say: no voice".into() };
        let msg = e.ceo_message();
        assert!(!msg.contains("/usr/bin"));
        assert!(!msg.contains("exit"));
        assert!(msg.contains("text"), "the CEO must be told the conversation continues: {msg}");
        assert!(e.to_string().contains("no voice"));
    }

    /// LIVE (macOS): the real synthesizer produces real audio, fast enough for gapless
    /// pipelining. Compiled only on macOS. Asserts the real-time factor is well under 1.0 —
    /// the precondition the whole pipelining design rests on.
    ///
    /// **NOT gated on `RICHOS_VOICE_LIVE_AUDIO`, and it should not be.** It opens no device
    /// and plays nothing; it writes a file and measures it. It runs on every macOS machine
    /// and on `app-voice-ci.yml`'s runner, which is why the four device-opening tests are the
    /// only ones `build.rs` ignores.
    ///
    /// A missing `/usr/bin/say` used to `return` here, which reports `ok` — the same defect
    /// the four carried, in the one place the condition is not an opt-in at all. `say` is part
    /// of the sealed macOS system volume, `tts.rs` ships Rich's voice through it, and
    /// `tests/voiced_acceptance.rs:157` and `:227` already `panic!` on its absence in these
    /// words. Its absence is a finding about the machine, never a test worth skipping.
    #[test]
    #[cfg(target_os = "macos")]
    fn live_macos_synthesis_outruns_playback() {
        if !Path::new("/usr/bin/say").exists() {
            panic!("this test's speech comes from /usr/bin/say — refusing to report green without it");
        }
        let dir = std::env::temp_dir().join("richos-voice-tts-test");
        let synth = MacSay::new();
        let speech = synth
            .synthesize("Acme signed at 4.5 million. I would push back.", 48_000, &dir)
            .expect("say should synthesize");
        assert_eq!(speech.sample_rate, 48_000);
        assert!(speech.duration_secs() > 1.0, "suspiciously short: {}", speech.duration_secs());
        assert!(
            speech.realtime_factor() < 0.5,
            "synthesis too slow for gapless pipelining: rtf {:.3}",
            speech.realtime_factor()
        );
        // Real audio, not a silent file.
        let peak = speech.samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.05, "synthesized audio is silent (peak {peak})");
        std::fs::remove_dir_all(&dir).ok();
    }
}

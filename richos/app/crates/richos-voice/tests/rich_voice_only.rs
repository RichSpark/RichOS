//! **Rich speaks only in his own voice** (CEO, 2026-10-15: "It's Rich's voice. Why the fuck would
//! there be any other names in any log lines???", and "if the voice isn't installed at all, it
//! needs to be fucking installed").
//!
//! No macOS `say` voice stands in while Rich's voice is being installed or loaded: the sentence
//! waits for it, and is spoken by the voice the install brought. Nothing names another voice.
//!
//! Its own test binary because it points `RICHOS_MODEL_DIR` at an empty folder for the whole
//! process, which is the state of a Mac where Rich's voice is not installed yet. Every wait here
//! waits for a FACT (a sentence reporting that it is waiting, a result arriving); the only clock
//! is a 60 s bound that turns a hang into a failure.

use richos_voice::tts::{self, RichVoice, SpeechSynth, VoiceGate};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

/// Names of other voices that must never appear where anything says who is speaking.
const OTHER_VOICES: &[&str] = &["say", "Daniel", "Samantha", "Reed", "Peter Yearsley", "Yearsley"];

/// Guards the process environment this test changes (one test in this binary, but declared).
static ENV: Mutex<()> = Mutex::new(());

/// Only a hang trips this; no verdict depends on it.
const HANG: Duration = Duration::from_secs(60);

fn names_another_voice(label: &str) -> Option<&'static str> {
    OTHER_VOICES.iter().copied().find(|name| label.contains(name))
}

#[test]
fn rich_speaks_only_in_his_own_voice_and_waits_for_it() {
    let _env = ENV.lock().unwrap_or_else(|p| p.into_inner());
    let empty = std::env::temp_dir().join(format!("richos-voice-only-{}", std::process::id()));
    std::fs::create_dir_all(&empty).unwrap();
    std::env::set_var("RICHOS_MODEL_DIR", &empty);
    let scratch = empty.join("scratch");

    // NOT INSTALLED: the label names Rich's voice and nothing else, and no other voice speaks.
    let voice = RichVoice::new();
    let label = voice.voice_label();
    assert!(label.starts_with("Rich's voice"), "the log names someone other than Rich: {label}");
    assert_eq!(names_another_voice(&label), None, "the log names another voice: {label}");
    match voice.synthesize("Acme signed at four and a half million.", 48_000, &scratch) {
        Ok(speech) => panic!(
            "Rich's voice is not on this machine, yet {} s of speech came out of another voice",
            speech.duration_secs()
        ),
        Err(e) => assert!(e.ceo_message().contains("text"), "{}", e.ceo_message()),
    }

    // BEING INSTALLED (what the app does when the voice is missing, `voice_provision.rs`): the
    // sentence WAITS for Rich's voice. No other voice fills the wait.
    tts::rich_voice_installing();
    assert_eq!(voice.voice_label(), "Rich's voice");
    let (waiting_tx, waiting_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let waiting_scratch = scratch.clone();
    let speaker = std::thread::spawn(move || {
        let voice = RichVoice::waiting_while(move || {
            waiting_tx.send(()).expect("the test listens until the end");
            true
        });
        let result = voice.synthesize("I would push back.", 48_000, &waiting_scratch);
        done_tx.send(result.map(|s| s.samples.len()).map_err(|e| e.to_string())).unwrap();
    });
    waiting_rx.recv_timeout(HANG).expect("the sentence never waited for Rich's voice");
    assert!(done_rx.try_recv().is_err(), "the sentence finished while Rich's voice was being installed");
    // This test cannot download 166 MB, so the install it watches ends without the files: the
    // sentence then fails into text, still in no other voice.
    tts::rich_voice_install_ended();
    let ended = done_rx.recv_timeout(HANG).expect("the wait ends when the install does");
    assert!(ended.is_err(), "an install that brought no files still produced audio: {ended:?}");
    speaker.join().unwrap();

    // INSTALLED, THEN SPOKEN: the same gate, with a stand-in engine for the 166 MB one. A
    // sentence waiting through the install is answered by the voice the install brought, once
    // it has loaded, and by nothing before that.
    let gate: Arc<VoiceGate<&'static str>> = Arc::new(VoiceGate::new());
    let files = Arc::new(AtomicBool::new(false));
    gate.installing();
    let (waiting_tx, waiting_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let (g, f) = (gate.clone(), files.clone());
    let waiter = std::thread::spawn(move || {
        let loader = g.clone();
        let keep_waiting = move || {
            waiting_tx.send(()).expect("the test listens until the end");
            true
        };
        let got = g.wait(&|| f.load(Ordering::SeqCst), &move || loader.loaded(Some("Rich's voice")), None, &keep_waiting);
        done_tx.send(got.map(|engine| *engine)).unwrap();
    });
    waiting_rx.recv_timeout(HANG).expect("the sentence never waited for the install");
    assert!(done_rx.try_recv().is_err(), "spoke before the install finished");
    files.store(true, Ordering::SeqCst);
    gate.install_ended();
    assert_eq!(done_rx.recv_timeout(HANG).unwrap(), Some("Rich's voice"));
    waiter.join().unwrap();

    // VOICE SWITCHED OFF while a sentence waits: the wait ends at the next ask, so switching
    // voice off never hangs on an install.
    let gate: VoiceGate<&'static str> = VoiceGate::new();
    gate.installing();
    let asked = AtomicUsize::new(0);
    let still_on = || asked.fetch_add(1, Ordering::SeqCst) == 0;
    assert_eq!(gate.wait(&|| false, &|| {}, None, &still_on), None);
    assert_eq!(asked.load(Ordering::SeqCst), 2, "the wait asked once while on and once after off");

    std::fs::remove_dir_all(&empty).ok();
}

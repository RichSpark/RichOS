//! Getting the speech model — the transport half, and it decides NOTHING.
//!
//! **THE SPLIT IS THE DESIGN.** `richos_voice::provision` holds every rule: what the bytes are,
//! whether there is room, whether to resume, whether what arrived is the model RichOS pinned, and
//! what to say when it is not. This file opens the socket, reads chunks, and asks that module
//! about everything. It is the same split `engine/voice/provisioning/model-integrity.js` and
//! `model-fetch.js` already use, and it exists for the same reason: **the failure paths have to be
//! testable without a network**, and they are — `crates/richos-voice/tests/model_provisioning.rs`
//! drives the whole state machine over synthetic bytes and opens no socket at all.
//!
//! So the rule for this file is narrow and worth keeping: **if a line here decides something, it
//! is in the wrong file.**
//!
//! # `reqwest`, and why it is not a new dependency
//!
//! `reqwest 0.13.4` is already in `src-tauri/Cargo.lock`, arriving beneath `tauri-plugin-updater`
//! — which is how the CEO's own app already downloads its updates. Naming it directly downloads no
//! new crate; the `Cargo.lock` diff for this change is the proof and it adds no `[[package]]`.
//! This is the same argument this manifest already makes for `tokio` ("Already in the tree beneath
//! `tauri`… so no new download"), and the reason it is stated rather than assumed is the CEO's
//! standing rule about third-party defaults: nothing here rides on `reqwest`'s behavior being
//! sensible, because every byte it returns is checked against a pinned sha256 before it is
//! allowed to become a model.
//!
//! # What this file will not do
//!
//! - **No daemon, no scheduler, no background retry.** A download happens because the CEO asked
//!   for it, at the voice toggle or on the setup sheet (the video tools are a setup essential
//!   since 2026-10-07, media-tools plan §2), in the foreground, and stops when it stops.
//! - **No automatic retry of a corrupted download.** `Finding::retryable` decides, and it says no
//!   to a hash mismatch: retrying corruption in a loop is how a transient fault becomes a support
//!   conversation.
//! - **No binary from here.** This file fetches pinned WEIGHTS only. The decoder, `whisper-cli`,
//!   arrives another way since 2026-10-07: it is built from pinned whisper.cpp 1.9.1 source into
//!   the engine's runtime (`scripts/build-runtimes.py`, `scripts/runtime-sources.json`), so
//!   first-run setup installs it with the engine at `<engine>/runtime/bin/whisper-cli`, hashed
//!   with every other runtime file, and `richos_voice::stt::resolve_whisper_bin` looks there
//!   first. A machine with no decoder is therefore one with no engine installed yet.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use futures_util::StreamExt;
use richos_voice::provision::{self, FetchPlan, Finding, ModelPhase, Outcome, PartFile, Pin};
use tauri::{AppHandle, Emitter};

/// Where a download's progress goes.
///
/// **A TRAIT, FOR THE REASON `richos_voice::event::VoiceObserver` IS ONE**: the audio pipeline
/// made itself UI-agnostic so the Tauri shell could be one sink and a recording `Vec` another, and
/// this is the same problem one layer up. With `AppHandle` wired directly into the fetch loop the
/// only way to run a real download was to launch the whole app and press a button in a webview —
/// so the one part of this feature that touches the network was the one part no proof could reach
/// except by hand.
///
/// `examples/provision_model.rs` is the reason it exists and the thing it buys: the REAL fetch
/// loop, the REAL `reqwest` client, the REAL pinned bytes, against a directory given on the
/// command line, printing every phase to stderr. Everything in this file except
/// [`TauriModelEmitter`] is then exercised outside a window.
pub trait ModelObserver: Send + Sync {
    fn on_model_event(&self, name: &str, payload: serde_json::Value);
}

/// The shipping sink: straight onto the webview's event bus, verbatim.
pub struct TauriModelEmitter {
    pub app: AppHandle,
}

impl ModelObserver for TauriModelEmitter {
    fn on_model_event(&self, name: &str, payload: serde_json::Value) {
        let _ = self.app.emit(name, payload);
    }
}

// ---------------------------------------------------------------------------------------------
// THE THREE SENTENCES FOR A GAP RICHOS CANNOT CLOSE.
//
// CONSTANTS, AND THE FORM IS LOAD-BEARING. `ui/tests/lib/state-strings.js` derives the
// inventory of everything the CEO can read by scanning a Rust literal's own line plus the two
// above it for `Err(`, `ok_or`, `.into()` or a `const … : &str =`. Written inline inside an
// `unwrap_or_else(|| { … })` these sentences fell OUTSIDE that window — the `.into()` landed on
// the line below the literal — so they became invisible to the affordance rule while remaining
// perfectly visible to the CEO. A sentence the rule cannot see is a sentence that can quietly
// become an instruction with no control beside it.
//
// So they are `const … : &str =` items with the literal starting on the same line, which is the
// shape that scrape explicitly supports, and each is classified in
// `ui/tests/lib/state-registry.js`. Naming them also matches what `main.rs` already does with
// `LEASE_UNAVAILABLE_MESSAGE`.
//
// EVERY ONE NAMES THE PARTY. These are states the CEO cannot fix, and the affordance rule's
// NEEDS-SOMEONE-ELSE clause requires the sentence to say who can — "aren't installed yet"
// implies somebody will and never says who, leaving a reader who cannot install anything
// holding a job with no owner. `SttError::ceo_message` set that precedent and these follow it.

/// No decoder on this machine, or weights that are not the pinned weights: two gaps RichOS
/// cannot close by downloading anything it has a hash for.
const NO_HEARING_HERE: &str = "I can't set up my hearing on this machine. Whoever set RichOS up adds that. \
     I can still read what you type.";

/// The resolver asked for a model that is not in the pin table. RichOS will not download a model
/// it cannot verify, and this is what that refusal reads as. Says nothing about checksums: that is
/// a word that tells him nothing he can act on.
const MODEL_NOT_PINNED: &str = "I can't prove the speech model this machine needs is the genuine one, so I won't download it \
    . Whoever set RichOS up can put that right. I can still read what you type.";

/// `HOME` is unset. Not a launch a double-click produces, so this is a machine somebody
/// configured rather than one he can fix.
const NOWHERE_TO_PUT_IT: &str = "I can't tell where to put my speech model on this machine. Whoever set RichOS up can put \
     that right. I can still read what you type.";

/// What he reads when HE stopped the download. Not a failure, and it must not wear one.
///
/// A const for the same reason the three above are: written inline as an argument to `emit` it
/// sat outside the affordance scrape's window, so the one sentence in this file that tells him
/// his stop was honored would have been the one sentence the rule could not see.
const STOPPED_BY_REQUEST: &str = "I've stopped the download. What arrived is saved, so asking me again picks up where it \
     left off rather than starting over.";

/// Emit at most this many progress events across a whole download.
///
/// MEASURED RATHER THAN CHOSEN BY FEEL. `reqwest`'s stream yields chunks far smaller than the
/// file: at 487,614,201 bytes a per-chunk event would be tens of thousands of IPC messages into a
/// webview whose only job with them is to move a bar a fraction of a pixel. 200 steps is one event
/// per 0.5% — 2,438,071 bytes of `small.en` — which is finer than the bar can render and is a
/// bounded number rather than a function of the transport's chunk size.
const PROGRESS_STEPS: u64 = 200;

/// One download's cancel flag and in-flight guard.
///
/// ONE AT A TIME, and the guard is the point rather than the cancel: two concurrent fetches of the
/// same model would write the same `.part` file from two positions and produce a file that is
/// exactly the right length and hashes to nothing — which the pin would catch, after both
/// transfers had run to completion.
#[derive(Default)]
pub struct ModelFetchState {
    pub in_flight: AtomicBool,
    pub cancel: AtomicBool,
    /// Bytes reported by the last emitted progress event. Read by `speech_model_status` so a
    /// webview that reloaded mid-download can render where it is without waiting for the next
    /// chunk.
    pub received: AtomicU64,
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn emit(
    obs: &dyn ModelObserver,
    phase: ModelPhase,
    model_id: &str,
    received: u64,
    total: u64,
    message: Option<&str>,
    ask_again: bool,
) {
    obs.on_model_event(
        provision::EVENT_VOICE_MODEL,
        provision::model_event_payload(phase, model_id, received, total, message, ask_again, now_millis()),
    );
}

/// Both sentences, to the two places each belongs: the CEO's to the webview, the engineer's to
/// stderr and whatever caught it.
fn report(obs: &dyn ModelObserver, model_id: &str, pin: &Pin, received: u64, finding: &Finding) {
    eprintln!("[richos] voice model: {}", finding.describe(&pin.file, false));
    emit(
        obs,
        ModelPhase::Failed,
        model_id,
        received,
        pin.bytes,
        Some(&finding.ceo_message()),
        // WHETHER THE CONTROL APPEARS, which is not whether the loop retries. See
        // `Finding::worth_asking_again`.
        finding.worth_asking_again(),
    );
}

/// **WHAT WOULD IT TAKE TO MAKE THIS MACHINE HEAR?** — answered without downloading anything.
///
/// This is what the offer is built from. It runs the SAME resolution `start_voice_capture` runs
/// (`stt::readiness`), so the sheet can never offer to fetch a model the microphone path would not
/// then look for.
pub fn offer() -> Option<serde_json::Value> {
    let readiness = richos_voice::stt::readiness();
    if !readiness.provisionable() {
        return None;
    }
    let richos_voice::stt::SpeechReadiness::ModelMissing { model_id, .. } = &readiness else {
        return None;
    };
    // NOT PINNED IS NOT OFFERABLE. RichOS will not download a model it cannot verify, and an
    // offer for one would be a button that refuses itself.
    let pin = provision::pin_for(model_id)?;
    let dir = provision::install_dir()?;
    let free = provision::free_bytes_for(&dir);
    // The room a fetch will ask for: only the missing bytes when a partial will be resumed.
    let need = pin.required_free_bytes_resuming(provision::resumable_bytes(&pin, &dir));
    let part = dir.join(format!("{}.part", pin.file));
    let already = provision::file_bytes(&part).min(pin.bytes);

    // The size comes from the COST table, which is where the brief asked for it and which
    // `the_two_tables_agree_on_every_size` holds equal to the pin the transfer is checked against.
    let costs = richos_voice::hardware::Costs::load();
    let disk_bytes = costs.models.get(model_id).map(|c| c.disk_bytes).unwrap_or(pin.bytes);

    Some(serde_json::json!({
        "modelId": model_id,
        "bytes": disk_bytes,
        "sizeLabel": provision::human(disk_bytes),
        "needFreeBytes": need,
        "freeBytes": free,
        // `None` free space is NOT "no room" — the platform declined to say, and refusing on an
        // answer nobody gave would take voice from a machine that had room all along.
        "enoughRoom": free.map(|f| f >= need).unwrap_or(true),
        "alreadyHave": already,
        "singleWitness": pin.single_witness(),
    }))
}

/// Download the pinned model this machine resolved to, verify it, and install it. Or do none of
/// those things and say which, once.
///
/// **BOUNDED, AND VISIBLE AT EVERY STEP.** At most [`provision::MAX_ATTEMPTS`] attempts, and only
/// for a failure a second attempt could plausibly fix. Every attempt emits; the last failure emits
/// with `retryable` so the UI knows whether to offer a control rather than guessing from prose.
pub async fn fetch_model(obs: &dyn ModelObserver, state: Arc<ModelFetchState>) -> Result<serde_json::Value, String> {
    let readiness = richos_voice::stt::readiness();
    let model_id = match &readiness {
        richos_voice::stt::SpeechReadiness::ModelMissing { model_id, .. } => model_id.clone(),
        richos_voice::stt::SpeechReadiness::Ready(_) => {
            return Ok(serde_json::json!({ "status": "already-ready" }))
        }
        // The two gaps RichOS cannot close by downloading anything. Saying "downloading…" here
        // would be a promise the next screen breaks.
        other => {
            return Err(other.ceo_message().unwrap_or_else(|| NO_HEARING_HERE.to_string()))
        }
    };
    fetch_pinned(obs, state, &model_id).await
}

/// **Download the pinned model `model_id`, verify it, install it** — [`fetch_model`]'s transfer,
/// for a model named by the caller rather than resolved by voice. First-run setup uses it for the
/// transcription model (`richos_voice::stt::TRANSCRIPTION_MODEL_ID`, the CEO's "Both", 2026-10-07).
/// Same pin table, same resume rules, same one-at-a-time guard, same bounded retries.
pub async fn fetch_pinned(
    obs: &dyn ModelObserver,
    state: Arc<ModelFetchState>,
    model_id: &str,
) -> Result<serde_json::Value, String> {
    let model_id = model_id.to_string();
    let pin = provision::pin_for(&model_id)
        // NO PIN, NO DOWNLOAD. RichOS will not fetch a model it cannot verify against
        // `model-pins.json`, and it says so rather than trying and failing later.
        .ok_or_else(|| MODEL_NOT_PINNED.to_string())?;
    let dir = provision::install_dir().ok_or_else(|| NOWHERE_TO_PUT_IT.to_string())?;

    if state.in_flight.swap(true, Ordering::SeqCst) {
        return Ok(serde_json::json!({ "status": "already-running" }));
    }
    state.cancel.store(false, Ordering::SeqCst);
    let _guard = InFlight(state.clone());

    let mut last: Option<Finding> = None;
    for attempt in 1..=provision::MAX_ATTEMPTS {
        if state.cancel.load(Ordering::SeqCst) {
            break;
        }
        match attempt_once(obs, &state, &pin, &model_id, &dir).await {
            Ok(value) => return Ok(value),
            Err(finding) => {
                let retryable = finding.retryable();
                last = Some(finding);
                if !retryable || attempt == provision::MAX_ATTEMPTS {
                    break;
                }
            }
        }
    }

    let received = state.received.load(Ordering::SeqCst);
    if state.cancel.load(Ordering::SeqCst) {
        // A stop the CEO asked for is not a failure, and it must not wear a failure's face. The
        // partial is left where it is, so asking again resumes.
        emit(
            obs,
            ModelPhase::Failed,
            &model_id,
            received,
            pin.bytes,
            Some(STOPPED_BY_REQUEST),
            true,
        );
        return Ok(serde_json::json!({ "status": "canceled" }));
    }

    let finding = last.unwrap_or_else(|| provision::disk_preflight(Some(0), pin.bytes).expect("a finding"));
    let mut sentence = finding.ceo_message();
    if finding.retryable() {
        // SAID OUT LOUD THAT IT STOPPED. A bounded retry that goes quiet after its last attempt is
        // indistinguishable from one still running.
        sentence.push_str(&format!(
            " I tried {} times and stopped rather than keep going.",
            provision::MAX_ATTEMPTS
        ));
    }
    report(obs, &model_id, &pin, received, &finding);
    Err(sentence)
}

/// Clears the in-flight flag however this function leaves — including a panic.
struct InFlight(Arc<ModelFetchState>);
impl Drop for InFlight {
    fn drop(&mut self) {
        self.0.in_flight.store(false, Ordering::SeqCst);
    }
}

/// ONE attempt. Every judgment in here is `provision`'s; this function supplies bytes and a socket.
async fn attempt_once(
    obs: &dyn ModelObserver,
    state: &ModelFetchState,
    pin: &Pin,
    model_id: &str,
    dir: &std::path::Path,
) -> Result<serde_json::Value, Finding> {
    let free = provision::free_bytes_for(dir);
    let (url, dest, part, from) = match provision::plan_fetch(pin, dir, free) {
        FetchPlan::AlreadyPresent { path } => {
            emit(obs, ModelPhase::Installed, model_id, pin.bytes, pin.bytes, None, false);
            eprintln!("[richos] voice model: {} is already installed and verifies", pin.file);
            return Ok(serde_json::json!({ "status": "already-present", "path": path }));
        }
        FetchPlan::Refused { finding } => return Err(finding),
        FetchPlan::Fetch { url, dest, part, from, resume_reason, .. } => {
            eprintln!("[richos] voice model: {} — {resume_reason}", pin.file);
            (url, dest, part, from)
        }
    };

    state.received.store(from, Ordering::SeqCst);
    emit(obs, ModelPhase::Started, model_id, from, pin.bytes, None, false);

    ensure_crypto_provider();
    let client = reqwest::Client::builder().build().map_err(|e| Finding {
        detail: Some(e.to_string()),
        ..failure_of(provision::Failure::Network)
    })?;
    let mut req = client.get(&url);
    if from > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={from}-"));
    }
    let res = match until_cancelled(&state.cancel, req.send()).await {
        None => {
            return Err(Finding {
                detail: Some("stopped on request".to_string()),
                ..failure_of(provision::Failure::Network)
            })
        }
        Some(Ok(r)) => r,
        Some(Err(e)) => {
            return Err(Finding {
                detail: Some(e.to_string()),
                ..failure_of(provision::Failure::Network)
            })
        }
    };

    let status = res.status().as_u16();
    if !res.status().is_success() {
        return Err(Finding {
            detail: Some(format!("{status} {}", res.status().canonical_reason().unwrap_or(""))),
            ..failure_of(provision::Failure::Http)
        });
    }

    // A server that ignores `Range` answers 200 with the WHOLE file. Appending that onto a partial
    // would produce a longer-than-pinned file that hashes to nothing, so the partial is dropped and
    // the body is written from zero. Decided by reading the status, never by trusting the request.
    let mut from = from;
    if from > 0 && status != 206 {
        let _ = std::fs::remove_file(&part);
        eprintln!("[richos] voice model: the server ignored the resume request — starting over");
        from = 0;
        state.received.store(0, Ordering::SeqCst);
    }

    let content_length = res.content_length();
    let content_range = res
        .headers()
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let declared = provision::declared_total(status, content_length, content_range.as_deref(), from);

    let mut stream = res.bytes_stream();

    // THE CHECK THAT CAN SAVE A WHOLE DOWNLOAD. The server has said how long the file is before
    // sending it; a bounded peek at the body turns "3,104 bytes where 487,614,201 were pinned"
    // into "you are behind a sign-in page" whenever there is a better sentence to be had.
    let mut peek: Vec<u8> = Vec::new();
    if declared.map(|d| d != pin.bytes).unwrap_or(false) {
        while peek.len() < provision::SNIFF_BYTES {
            match until_cancelled(&state.cancel, stream.next()).await {
                Some(Some(Ok(chunk))) => peek.extend_from_slice(&chunk),
                None => {
                    return Err(Finding {
                        detail: Some("stopped on request".to_string()),
                        ..failure_of(provision::Failure::Network)
                    })
                }
                _ => break,
            }
        }
        if let Some(finding) = provision::check_declared(pin, declared, &peek) {
            return Err(finding);
        }
    }

    let mut file = PartFile::open(pin, &part, &dest, from).map_err(|e| Finding {
        detail: Some(e.to_string()),
        ..failure_of(provision::Failure::Network)
    })?;

    let step = (pin.bytes / PROGRESS_STEPS).max(1);
    let mut next_at = from + step;
    if !peek.is_empty() {
        if let Err(e) = file.write(&peek) {
            return Err(Finding { detail: Some(e.to_string()), ..failure_of(provision::Failure::Network) });
        }
    }

    loop {
        // The stop is honored while WAITING for a chunk too, not only when one arrives.
        let next = until_cancelled(&state.cancel, stream.next()).await;
        if state.cancel.load(Ordering::SeqCst) || next.is_none() {
            // The string is the ENGINEER's detail, not the CEO's: it reaches `describe` on
            // stderr and never `ceo_message`. What he reads is the sentence composed below,
            // after the loop, which says his stop was honored and what it cost him.
            return Err(file.interrupted("stopped on request"));
        }
        let Some(chunk) = next.flatten() else { break };
        let chunk = match chunk {
            Ok(c) => c,
            Err(e) => return Err(file.interrupted(&e.to_string())),
        };
        if let Err(e) = file.write(&chunk) {
            return Err(Finding { detail: Some(e.to_string()), ..failure_of(provision::Failure::Network) });
        }
        if file.is_overfull() {
            // More than the pinned model: stop reading now instead of filling the disk. `finish`
            // below classifies and deletes it.
            break;
        }
        let got = file.received();
        if got >= next_at {
            next_at = got + step;
            state.received.store(got, Ordering::SeqCst);
            emit(obs, ModelPhase::Progress, model_id, got, pin.bytes, None, false);
        }
    }

    // ITS OWN PHASE, because it is its own wait: hashing this file takes about a second, and a bar
    // sitting at 100% with no explanation is where a person decides the app has hung.
    let got = file.received();
    state.received.store(got, Ordering::SeqCst);
    emit(obs, ModelPhase::Verifying, model_id, got, pin.bytes, None, false);

    match file.finish() {
        Outcome::Installed { path, bytes, sha256, resumed_from } => {
            eprintln!(
                "[richos] voice model: {} installed and verified against its pinned sha256 ({bytes} bytes, resumed from {resumed_from})",
                pin.file
            );
            emit(obs, ModelPhase::Installed, model_id, bytes, pin.bytes, None, false);
            Ok(serde_json::json!({
                "status": "installed",
                "path": path,
                "bytes": bytes,
                "sha256": sha256,
                "modelId": model_id,
            }))
        }
        Outcome::Failed { finding } => Err(finding),
    }
}

/// **INSTALL THE TLS CIPHER SUITE OURSELVES RATHER THAN HOPE SOMEBODY ELSE ALREADY HAS.**
///
/// `reqwest` is built here with `rustls-no-provider` — the feature `tauri-plugin-updater`'s own
/// default set turns on — which means reqwest chooses no cipher implementation and a process-level
/// provider must exist before a client is built, or the build fails outright.
///
/// The only code in this binary that installs one today is
/// `tauri-plugin-updater-2.11.0/src/updater.rs:492-494`, and it does it LAZILY, inside the update
/// check itself. So whether a speech-model download works would depend on whether the app had
/// already checked for an update — two unrelated features, one ordering, and the first run is
/// exactly the case where the update check has not happened yet.
///
/// `install_default` returns `Err` only when a provider is already installed — here, another
/// thread installing one between the check and the install. That is not a failure: the
/// post-condition this function promises is "a provider exists", and the `Err` branch asserts
/// exactly that rather than discarding it. Idempotent, so calling it per attempt costs nothing.
///
/// The phone channel's TLS listener and its Connect client need the same provider for the same
/// reason, so they call this one function rather than repeating the install.
pub(crate) fn ensure_crypto_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_none()
        && rustls::crypto::ring::default_provider().install_default().is_err()
    {
        debug_assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }
}

// ---------------------------------------------------------------------------------------------
// Rich's voice: Pocket TTS + the Peter Yearsley recording (CEO, 2026-10-14).
// ---------------------------------------------------------------------------------------------

/// One background fetch of the voice files at a time.
static RICH_VOICE_FETCH: AtomicBool = AtomicBool::new(false);

/// A transfer that delivers no byte for this long has stalled; the attempt ends and is retried.
const RICH_VOICE_STALL: std::time::Duration = std::time::Duration::from_secs(60);

/// **Make Rich's voice ready, in the background: install it if any pinned file is missing, then
/// load it, and load it at once when it is already here.** Returns at once and never blocks a
/// screen; a sentence Rich says meanwhile waits for it (no other voice). Loaded when the app is
/// up, never first at a Talk press (CEO, 2026-10-15). Called when voice is found ready on this
/// Mac (the window asks at launch), after first-run setup has installed the speech models, when
/// voice mode starts and when a phone reply is spoken; the last two find it loaded already.
pub fn fetch_rich_voice_in_background() {
    let Some(dir) = richos_voice::pocket::bundle_dir() else { return };
    if richos_voice::pocket::missing_files(&dir).is_empty() {
        richos_voice::tts::warm_rich_voice();
        return;
    }
    if RICH_VOICE_FETCH.swap(true, Ordering::SeqCst) {
        return;
    }
    // Before the thread, so a voice session started right after this call already sees the
    // install and waits for Rich's voice instead of finding it missing.
    richos_voice::tts::rich_voice_installing();
    std::thread::spawn(move || {
        let done = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())
            .and_then(|rt| rt.block_on(fetch_rich_voice(&dir)));
        RICH_VOICE_FETCH.store(false, Ordering::SeqCst);
        // Installed in silence (CEO, 2026-10-15: no log lines about the voice); only a fault is
        // written, because a failed install with nothing on record cannot be diagnosed.
        if let Err(e) = done {
            eprintln!("[richos] speech synthesis install failed this time: {e}");
        }
        richos_voice::tts::rich_voice_install_ended();
        // Installed: load it now, so it is ready before anyone presses Talk.
        richos_voice::tts::warm_rich_voice();
    });
}

/// Every missing pinned file, each with at most [`provision::MAX_ATTEMPTS`] attempts.
async fn fetch_rich_voice(dir: &std::path::Path) -> Result<(), String> {
    let missing = richos_voice::pocket::missing_files(dir);
    let need: u64 = missing.iter().map(|f| f.bytes).sum();
    if let Some(finding) = provision::disk_preflight(provision::free_bytes_for(dir), need.saturating_mul(11).div_ceil(10)) {
        return Err(finding.describe("speech synthesis", false));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("create the speech synthesis folder: {e}"))?;
    // A fault is logged by the file's place in the list, never its name or download address:
    // no log line names the voice.
    let total = missing.len();
    for (n, pin) in missing.into_iter().enumerate() {
        let n = n + 1;
        let mut last = String::new();
        for attempt in 1..=provision::MAX_ATTEMPTS {
            match fetch_rich_voice_file(dir, &pin).await {
                Ok(()) => {
                    last.clear();
                    break;
                }
                Err(e) => {
                    let e = richos_voice::pocket::fault_without_names(&e);
                    eprintln!("[richos] speech synthesis install: file {n} of {total} attempt {attempt} failed: {e}");
                    last = e;
                }
            }
        }
        if !last.is_empty() {
            return Err(format!("file {n} of {total}: {last}"));
        }
    }
    Ok(())
}

/// One file: stream it to `<name>.part`, check its size and sha256 against the pin, and only then
/// give it its real name, so the loader can never see a half-written or substituted file.
async fn fetch_rich_voice_file(dir: &std::path::Path, pin: &richos_voice::pocket::PinnedFile) -> Result<(), String> {
    use std::io::Write;
    let part = dir.join(format!("{}.part", pin.name));
    let dest = dir.join(pin.name);
    ensure_crypto_provider();
    let client = reqwest::Client::builder().build().map_err(|e| e.to_string())?;
    let res = tokio::time::timeout(RICH_VOICE_STALL, client.get(pin.url).send())
        .await
        .map_err(|_| "no answer from the server".to_string())?
        .map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("the server answered {}", res.status()));
    }
    let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut stream = res.bytes_stream();
    let mut got: u64 = 0;
    loop {
        let next = tokio::time::timeout(RICH_VOICE_STALL, stream.next()).await.map_err(|_| "the transfer stalled".to_string())?;
        let Some(chunk) = next else { break };
        let chunk = chunk.map_err(|e| e.to_string())?;
        got += chunk.len() as u64;
        if got > pin.bytes {
            std::fs::remove_file(&part).ok();
            return Err(format!("more than the pinned {} bytes arrived", pin.bytes));
        }
        file.write_all(&chunk).map_err(|e| e.to_string())?;
    }
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    if let Err(e) = richos_voice::pocket::verify_file(&part, pin) {
        std::fs::remove_file(&part).ok();
        return Err(e);
    }
    std::fs::rename(&part, &dest).map_err(|e| e.to_string())
}

/// How often a network wait looks at the stop flag.
const CANCEL_POLL: std::time::Duration = std::time::Duration::from_millis(200);

/// Await `fut`, but give up as soon as `cancel` is set. `None` means the stop won.
///
/// The request and every body read can wait on a peer that says nothing, and reqwest sets no
/// deadline by default. Testing the flag only after a chunk arrives left Stop unable to release a
/// stalled download. The flag is polled between short timeouts on the SAME pinned future, so no
/// progress of the wait is lost when it is not set. Dropping the future on stop closes the socket;
/// the partial file is untouched and stays resumable.
pub(crate) async fn until_cancelled<F: std::future::Future>(cancel: &AtomicBool, fut: F) -> Option<F::Output> {
    let mut fut = std::pin::pin!(fut);
    loop {
        if cancel.load(Ordering::SeqCst) {
            return None;
        }
        if let Ok(v) = tokio::time::timeout(CANCEL_POLL, fut.as_mut()).await {
            return Some(v);
        }
    }
}

/// A finding with no numbers — for the two kinds the transport itself produces.
fn failure_of(kind: provision::Failure) -> Finding {
    Finding { kind, have: None, want: None, detail: None, resumable: false }
}

#[cfg(test)]
mod cancel_tests {
    use super::*;
    use std::time::Duration;

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread().enable_time().build().expect("runtime")
    }

    /// A wait on a peer that never answers is released by the stop flag, not by the peer.
    #[test]
    fn a_stalled_wait_is_released_by_the_stop_flag() {
        // The stop is pressed only once the wait has actually started: the first poll of the peer
        // sets the flag and then stays pending forever, exactly as a silent socket does. A wait
        // that ignored the flag would never return (a hang, not a clock verdict).
        let cancel = AtomicBool::new(false);
        let silent_peer = std::future::poll_fn(|_cx| {
            cancel.store(true, Ordering::SeqCst);
            std::task::Poll::<u8>::Pending
        });
        let out = rt().block_on(until_cancelled(&cancel, silent_peer));
        assert!(out.is_none(), "the stop must win over a wait that never ends");
    }

    /// POSITIVE CONTROL: with the flag clear, a wait that completes returns its value.
    #[test]
    fn a_wait_that_completes_returns_its_value() {
        let cancel = AtomicBool::new(false);
        let out = rt().block_on(until_cancelled(&cancel, async {
            tokio::time::sleep(Duration::from_millis(450)).await;
            7u8
        }));
        assert_eq!(out, Some(7), "slower than one poll interval still completes");
    }
}

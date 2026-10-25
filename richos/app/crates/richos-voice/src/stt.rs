//! Speech to text — local whisper.cpp, reusing the model path the repo already ships.
//!
//! **No cloud STT, ever.** The repo already runs whisper.cpp locally in two places — the
//! open-wispr dictation path (the local-dictation notes) and the call-transcription service
//! (`tools/richos-service/lib/{config,transcribe}.js`) — so this is a Rust port of that
//! resolution logic, not a new dependency. Same binary (`whisper-cli`), same model
//! directories, same env-override names, so one machine feeds all three.
//!
//! ## Utterance-endpointed, NOT token-streaming — stated plainly
//!
//! `whisper-cli` transcribes a finished file. There is no partial-hypothesis stream and
//! therefore **no live transcript building word-by-word while the CEO talks** (the UX direction §4.1
//! sketch shows one; this v1 does not deliver it). What the CEO gets instead is the finished
//! transcript landing in the thread within ~0.5 s of him stopping. The upgrade path is a warm
//! whisper daemon or `whisper-stream`, both flagged in the brief.
//!
//! ## Model choice for the CONVERSATIONAL loop: ASKED, not compiled in
//!
//! Until 2026-09-10 this section defended a constant — `DEFAULT_MODEL_ID = "small.en"`,
//! identical on every machine in the world. The CEO's question was the right one:
//! *"WHY IS THE SHITTIEST POSSIBLE HARDWARE HARDCODED INTO THE APP???"* It now resolves at
//! run time from what the machine can actually do — [`crate::hardware`] holds the rule, the
//! measured ladder and the reasoning; this module supplies the measurement.
//!
//! **The budget stated here is the one the resolver gates on**, and it is unchanged: a
//! conversational utterance should land in **~0.5 s**, and **a second of dead air after every
//! sentence is the difference between talking to Rich and operating him.** That sentence is
//! now load-bearing rather than commentary — `model-costs.json` carries 1.000 s as the live
//! ceiling and cites this file for it.
//!
//! **What measuring it changed.** On this M4, three runs of a 3.095 s utterance cost
//! **0.539 / 0.578 / 0.593 s** for `small.en` — reproducing the 0.47–0.74 s this comment used to
//! claim, so the old figure was right and is now checked rather than trusted. The turbo-class
//! cost it quoted as "+0.63–0.79 s absolute" reproduces too, at **+0.749 s / +0.810 s** across
//! two load conditions. What was NOT right was the reason `small.en` was kept: the tier table
//! calls it the fallback for *"low-RAM hosts"*, and `large-v3-turbo-q5_0` measures
//! **8,945,664 B CHEAPER** in peak RSS. Memory never chose this model; latency did, and latency
//! is a property of the machine rather than of the product.
//!
//! So `q5_0` sits at the TOP of the live ladder and is rejected on every machine measured so far
//! (1.549 s and 1.331 s against the 1.000 s ceiling). The door opens by itself on the first
//! machine that decodes it in time. The honest lever is still a smaller/quantized model rather
//! than a warm daemon — turbo's fixed per-invocation overhead is 0.48–0.59 s against
//! `small.en`'s 0.20–0.23 s, so a daemon returns only ~0.3 s of the gap, and the earlier claim
//! of a ~1.4 s load tax curable by a daemon is measured FALSE (the dictation-daemon + q5 brief,
//! 2026-08-26).
//!
//! `RICHOS_VOICE_WHISPER_MODEL_ID` still names a model outright and
//! `RICHOS_VOICE_WHISPER_MODEL`/`RICHOS_WHISPER_MODEL` still pin the weights by path. Both win
//! over the resolver — an engineer reproducing a measurement needs them to. They are no longer
//! the ONLY control, which is the whole change.

use crate::vad::SAMPLE_RATE;
use crate::wav;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

/// The id assumed when NO resolution happened — and only then.
///
/// THIS IS NOT A DEFAULT AND THE RENAME IS THE POINT. It used to be `DEFAULT_MODEL_ID`, the
/// model every machine got. It is now reached in exactly two situations, neither of which is
/// "an ordinary launch":
///
/// 1. `RICHOS_VOICE_WHISPER_MODEL` or `RICHOS_WHISPER_MODEL` pinned the WEIGHTS BY PATH. Walking
///    a ladder then would be theatre — `resolve_model` returns that one file for every id, so
///    every rung would "measure" the same weights. The id is still needed, because
///    `toolchain.rs` looks the pin up by it, and this preserves exactly the behavior that
///    override has always had: point it at something else and the pin check refuses, loudly.
/// 2. The compiled-in registry is unusable, which is a build-time mistake in this repository.
///
/// The ORDINARY unmeasurable case does not come here: it goes to the registry's `safeRung`,
/// which the CEO is told about. See [`crate::hardware`].
pub const FALLBACK_MODEL_ID: &str = "small.en";

#[derive(Debug)]
pub enum SttError {
    BinaryNotFound(String),
    ModelNotFound(String),
    /// The model file is not the model RichOS pinned, or strict mode refused a changed binary.
    /// A SEPARATE variant from `ModelNotFound` on purpose: "not installed" and "installed but not
    /// what it claims to be" need different words to the CEO and different actions from whoever
    /// set the machine up.
    ToolchainRefused(String),
    Io(String),
    Failed { status: String, stderr: String },
}

impl SttError {
    /// The CEO-facing line. No paths, no exit codes, no model filenames.
    pub fn ceo_message(&self) -> String {
        match self {
            SttError::BinaryNotFound(_) => {
                // NAMES THE PARTY. "Aren't installed yet" implies somebody will install them
                // and never said who, leaving a reader who cannot install anything holding a
                // job with no owner. RichOS cannot fetch a DECODER for itself — see the table
                // on `SpeechReadiness` — so this one still names whoever set the machine up.
                "My ears aren't installed on this machine yet. Whoever set RichOS up adds \
                 those. I can still read what you type."
                    .into()
            }
            SttError::ModelNotFound(_) => {
                // ITS OWN SENTENCE, split from `BinaryNotFound` on 2026-09-17. Until this split
                // it shared "whoever set RichOS up adds those" with the decoder gap — true for
                // a missing decoder, false for a missing MODEL once `crate::provision` can fetch
                // one on its own (landed 516975db). Nobody sets this one up but RichOS itself,
                // so it stops naming a party and states what it can do about it instead.
                // Phrased like `hardware.rs`'s `Basis::Unmeasured` line: what happens NEXT, tied
                // to the control already on screen (the ◉ toggle), never a command aimed at the
                // reader — so this reads as INFORMATIONAL rather than an instruction with no
                // control beside it.
                "My speech model isn't on this machine yet. I can fetch it myself now, so I'll \
                 offer to download it the next time you turn voice on. I can still read what \
                 you type."
                    .into()
            }
            SttError::ToolchainRefused(_) => {
                // DIFFERENT WORDS, because it is a different situation and the difference matters
                // to him. "Not installed yet" would be false and would send whoever helps him to
                // install something that is already there. Still no paths, no hashes, no
                // filenames — the detail is on stderr and in the record, where it is useful.
                "Something about my hearing changed on this machine and I'd rather not guess at \
                 what you said than get it wrong. Whoever set RichOS up can put it right. I can \
                 still read what you type."
                    .into()
            }
            SttError::Io(_) | SttError::Failed { .. } => {
                "I didn't catch that. Say it again?".into()
            }
        }
    }
}

impl std::fmt::Display for SttError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SttError::BinaryNotFound(s) => write!(f, "whisper binary not found: {s}"),
            SttError::ModelNotFound(s) => write!(f, "whisper model not found: {s}"),
            SttError::ToolchainRefused(s) => write!(f, "whisper toolchain refused: {s}"),
            SttError::Io(s) => write!(f, "stt io: {s}"),
            SttError::Failed { status, stderr } => write!(f, "whisper failed ({status}): {stderr}"),
        }
    }
}

/// The `bin` directory of the engine runtime this app process verified, if it has one.
///
/// Set by the desktop shell (`main.rs` at boot, `setup_view.rs` after a first-run engine install)
/// from `richos_core::runtime::verify_engine`, which has just hashed every runtime file against
/// `delivery.json`, `bin/whisper-cli` included. This crate does not depend on richos-core, so the
/// shell hands the directory over rather than this crate finding the engine itself.
static DELIVERED_RUNTIME_BIN: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

/// Tell the resolver where the verified engine runtime's executables are (`<runtime>/bin`), or
/// that this launch has none.
pub fn set_delivered_runtime_bin(dir: Option<PathBuf>) {
    if let Ok(mut held) = DELIVERED_RUNTIME_BIN.write() {
        *held = dir;
    }
}

fn delivered_runtime_bin() -> Option<PathBuf> {
    DELIVERED_RUNTIME_BIN.read().ok().and_then(|held| held.clone())
}

/// The directory [`set_delivered_runtime_bin`] last recorded, for a caller that hands it on to a
/// process of its own: the dictation tool is started by the app and hears with the same decoder.
pub fn delivered_runtime_bin_dir() -> Option<PathBuf> {
    delivered_runtime_bin()
}

/// Where a DEVELOPER's own `whisper-cli` may be (Homebrew on Apple silicon, then Intel, then the
/// system). Reached only when this launch has no delivered runtime, which a user's install always
/// has once first-run setup has installed the engine.
pub const DEVELOPER_WHISPER_DIRS: [&str; 3] = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"];

/// Resolve `whisper-cli`. **The order, and why:**
///
/// 1. `RICHOS_WHISPER_BIN` — an engineer who names a binary is not asking to be second-guessed.
///    Deliberately the SAME env var the Node service uses so one machine configures both.
/// 2. **The engine runtime's own copy**, `<engine>/runtime/bin/whisper-cli`. Since 2026-10-07 it
///    is built from pinned whisper.cpp 1.9.1 source (`scripts/build-runtimes.py`), the version the
///    decode settings were measured on, installed onto every user's Mac by first-run setup and
///    hashed at every load. It comes before PATH because PATH holds whatever version someone else
///    installed, and a user's voice should not depend on that.
/// 3. PATH, then [`DEVELOPER_WHISPER_DIRS`] (Homebrew) — only for a launch without a delivered
///    runtime: a development build, or a fresh Mac before setup has installed the engine.
pub fn resolve_whisper_bin() -> Result<PathBuf, SttError> {
    let developer_dirs = DEVELOPER_WHISPER_DIRS.map(Path::new);
    whisper_bin_from(
        std::env::var("RICHOS_WHISPER_BIN").ok().as_deref(),
        delivered_runtime_bin().as_deref(),
        whisper_on_path,
        &developer_dirs,
    )
}

fn whisper_on_path() -> Option<PathBuf> {
    let out = Command::new("/usr/bin/env").args(["sh", "-c", "command -v whisper-cli"]).output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty() && Path::new(&s).exists()).then(|| PathBuf::from(s))
}

/// [`resolve_whisper_bin`]'s order as a function of its inputs, so the order is provable without
/// touching this machine's PATH or its Homebrew. `on_path` runs only if nothing before it answered.
pub fn whisper_bin_from(
    env_override: Option<&str>,
    delivered_bin: Option<&Path>,
    on_path: impl FnOnce() -> Option<PathBuf>,
    developer_dirs: &[&Path],
) -> Result<PathBuf, SttError> {
    if let Some(v) = env_override {
        let p = PathBuf::from(expand_tilde(v));
        if p.exists() {
            return Ok(p);
        }
        return Err(SttError::BinaryNotFound(format!("RICHOS_WHISPER_BIN={v} does not exist")));
    }
    if let Some(p) = delivered_bin.map(|dir| dir.join("whisper-cli")).filter(|p| p.is_file()) {
        return Ok(p);
    }
    if let Some(p) = on_path() {
        return Ok(p);
    }
    for dir in developer_dirs {
        let p = dir.join("whisper-cli");
        if p.exists() {
            return Ok(p);
        }
    }
    Err(SttError::BinaryNotFound(match delivered_bin {
        Some(dir) => format!("whisper-cli is not in the engine runtime ({}), on PATH or in a developer prefix", dir.display()),
        None => "this launch has no engine runtime, and whisper-cli is not on PATH or in a developer prefix".into(),
    }))
}

/// Resolve a GGML model file. Mirrors `tools/richos-service/lib/config.js::resolveModel`,
/// plus a voice-specific override so the conversational model can differ from the
/// transcription service's without fighting over one variable.
///
/// **`~/.config/richos/models` joined this list on 2026-09-17** and is the directory
/// [`crate::provision`] installs into. Everything else about the order is unchanged.
pub fn resolve_model(model_id: &str) -> Result<PathBuf, SttError> {
    for var in ["RICHOS_VOICE_WHISPER_MODEL", "RICHOS_WHISPER_MODEL"] {
        if let Ok(v) = std::env::var(var) {
            let p = PathBuf::from(expand_tilde(&v));
            if p.exists() {
                return Ok(p);
            }
            return Err(SttError::ModelNotFound(format!("{var}={v} does not exist")));
        }
    }
    let file = format!("ggml-{model_id}.bin");
    let home = std::env::var("HOME").unwrap_or_default();
    let dirs = model_search_dirs(&home, std::env::var("RICHOS_MODEL_DIR").ok().as_deref());
    for d in &dirs {
        let p = d.join(&file);
        if p.exists() {
            return Ok(p);
        }
    }
    Err(SttError::ModelNotFound(format!(
        "{file} not found in {}",
        dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>().join(", ")
    )))
}

/// The directory RichOS installs the models IT downloaded into, relative to `$HOME`.
///
/// ONE CONSTANT, TWO USERS, and that is the whole reason it is not simply spelled twice.
/// [`model_search_dirs`] looks here and [`crate::provision::install_dir`] writes here; a product
/// whose downloader and whose resolver each carry their own copy of a path is a product that
/// downloads half a gigabyte into a directory nothing reads, and nothing about either file would
/// look wrong on the day it happened.
pub const RICHOS_MODELS_SUBDIR: &str = ".config/richos/models";

/// Every directory a model may be resolved from, best first — a PURE function of the two inputs
/// that decide it.
///
/// PURE SO THE INSTALL LOCATION IS PROVABLE. `provision.rs` writes the model it downloads into one
/// specific directory, and the only thing that makes that download worth anything is that this
/// walk looks there. Asserting it by setting `HOME` to a temporary directory would prove it for
/// one run of one machine and would race every other test in this crate that reads the
/// environment; asserting it over this function proves it for every `HOME` there will ever be.
///
/// **THE ORDER, AND WHY.** `RICHOS_MODEL_DIR` first, because an engineer who names a directory is
/// not asking to be second-guessed. Then `~/.config/richos/models` — RichOS's OWN, added
/// 2026-09-17, and the only entry on this list whose contents this product downloaded and verified
/// against `model-pins.json` itself. Then the three that belong to other software: the dictation
/// app's, the CEO's own, and whisper.cpp's cache. RichOS reads those and never writes to them —
/// a product that puts half a gigabyte into another application's directory has made that
/// application's cleanup our outage.
///
/// A machine that already carries weights in one of the older three resolves exactly as it always
/// did, because the new directory will not exist there and the walk falls through.
pub fn model_search_dirs(home: &str, override_dir: Option<&str>) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = override_dir.filter(|d| !d.trim().is_empty()) {
        dirs.push(PathBuf::from(expand_tilde(d)));
    }
    dirs.push(Path::new(home).join(RICHOS_MODELS_SUBDIR));
    dirs.push(Path::new(home).join(".config/open-wispr/models"));
    dirs.push(Path::new(home).join("Models/Whisper"));
    dirs.push(Path::new(home).join(".cache/whisper.cpp"));
    dirs
}

fn expand_tilde(p: &str) -> String {
    if let Some(rest) = p.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}/{rest}");
        }
    }
    p.to_string()
}

/// Text-context budget when NO decoding prompt is configured: none, the pipeline-wide invariant.
///
/// This path used to pass no `-mc` at all, which meant whisper.cpp's own `-1` — carry every
/// previously decoded token — the identical unexamined default that filled 7.8% of a 92-minute
/// channel with one fabricated sentence in the call-transcription service
/// (`docs/measurements/whisper-settings-2026-09-10/measurements/longform.txt`). It was fixed there
/// on 2026-08-29 and left live here, in a second consumer nobody had audited.
const MAX_CONTEXT_NO_PROMPT: &str = "0";

/// Text-context budget when a decoding prompt IS configured.
///
/// NOT a compromise and not a guess. At `0` the prompt is INERT — whisper has no room to keep the
/// prompt tokens, so the flag is accepted and does nothing. Measured on one 6-second utterance
/// through `small.en`, three runs each
/// (`docs/measurements/whisper-settings-2026-09-10/measurements/dictation-probe.txt`):
///
/// ```text
///   -mc -1  (what this path shipped)   "Haldan Freight"    587-661 ms
///   -mc 0                              "Haldan Freight"    612-641 ms
///   -mc 0  + prompt                    "Haldan Freight"    609-614 ms   <- prompt does NOTHING
///   -mc 64 + prompt                    "Halden Freight"    607-654 ms   <- correct
/// ```
///
/// 64 is the smallest budget measured to recover the names: on the 6-call reference corpus a
/// carried entity prompt at 64 takes proper-noun exact hits from 46 to 55 of 66 and gives the best
/// WER measured (2.73%), and going to 224 adds no further names while wrecking the transcript
/// (9.92%, insertions 8 -> 156).
///
/// THE COST, STATED. Carried context is what produces long-form fabrication: 64 tokens fills 3.3%
/// of a 92-minute timeline with repeated phrases where 0 fills none. That is why it is taken ONLY
/// when a prompt is set — dictation utterances are seconds long, so there is nothing to accumulate
/// — and why anyone routing a long recording through this path should read the settings table
/// first. The alternative was to accept a prompt and silently ignore it, and a seam that reports on
/// while doing nothing is the defect class this codebase refuses everywhere else.
const MAX_CONTEXT_WITH_PROMPT: &str = "64";

/// The decode flags this path hands to `whisper-cli`, everything except `-m` and `-f`.
///
/// A FUNCTION, SO THE SETTINGS CAN BE ASSERTED WITHOUT A DECODER. These values are decided in
/// `docs/measurements/whisper-settings-2026-09-10/whisper-settings-decisions.md` §7; a test that
/// cannot see them cannot stop the next one being added by accident.
pub fn decode_args(prompt: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = vec![
        // Pinned, not inherited: `-l auto` is byte-identical on English audio and 20% slower.
        "-l".into(),
        "en".into(),
        // The decode is Metal-bound; 8 threads measured byte-identical at the same wall clock.
        "-t".into(),
        "4".into(),
        // Flash attention. whisper.cpp 1.9.1 defaults it ON and it is worth 1.57 WER points, which
        // is far too much to hold by inheritance from a default a formula bump can flip.
        "-fa".into(),
        "-np".into(), // no progress prints — this path reads the words off stdout
        "-nt".into(), // no timestamps — we want the words, nothing else
        "-mc".into(),
        if prompt.is_some() { MAX_CONTEXT_WITH_PROMPT.into() } else { MAX_CONTEXT_NO_PROMPT.into() },
    ];
    if let Some(p) = prompt {
        args.push("--prompt".into());
        args.push(p.to_string());
    }
    args
}

// -------------------------------------------------------------------------------------------
// Choosing the model — the measurement half. The RULE lives in `hardware.rs`.
// -------------------------------------------------------------------------------------------

/// Pick the model for THIS machine, measuring it if nobody has yet.
///
/// The split with [`crate::hardware`] is deliberate and it is what makes any of this testable:
/// that module holds the ladder, the gates and the reasoning as a pure function; this one owns
/// the two impure things it needs — synthesizing a probe utterance and timing a decode.
///
/// COST, STATED. On a machine that has been measured before this is one small file read. On a
/// machine that has not, it is one `say` render plus one decode per rung tried, stopping at the
/// first rung that fits — two decodes on the reference host, about 2 s, ONCE for that machine and
/// that whisper binary. It happens in `Recognizer::resolve()`, which already hashes the binary
/// and sometimes a 487 MB model, and which already exists so that nothing surprising happens
/// mid-sentence.
fn choose_model(bin: &Path) -> crate::hardware::Resolution {
    use crate::hardware::{self, Basis, Machine};

    let machine = Machine::read();

    // AN EXPLICIT ID WINS OUTRIGHT. An engineer who names a model is not asking to be second
    // guessed, and reproducing a measurement depends on it.
    if let Ok(id) = std::env::var("RICHOS_VOICE_WHISPER_MODEL_ID") {
        let id = id.trim().to_string();
        if !id.is_empty() {
            return overridden(id, machine);
        }
    }
    // SO DOES A PATH OVERRIDE, and for a sharper reason: `resolve_model` returns that one file
    // for EVERY id, so a ladder walk would time the same weights four times and report a
    // "resolution" that resolved nothing.
    for var in ["RICHOS_VOICE_WHISPER_MODEL", "RICHOS_WHISPER_MODEL"] {
        if std::env::var(var).map(|v| !v.trim().is_empty()).unwrap_or(false) {
            return overridden(FALLBACK_MODEL_ID.to_string(), machine);
        }
    }

    let mut costs = hardware::Costs::load();
    // A rung is only a rung if its weights are on this machine.
    costs.retain_installed(|id| resolve_model(id).is_ok());

    // The cache is keyed by binary AND machine: a `brew upgrade` can change decode cost, and a
    // speed attributed to the wrong build is the hole `toolchain.rs` exists to close.
    let bin_sha = crate::toolchain::hash_file(bin).unwrap_or_default();
    let key = hardware::cache_key(&bin_sha, &machine);
    // ONE CALLER MEASURES, the others wait for it and then read what it saved (Ray, test55: a
    // launch's callers used to run two to five measurements side by side). Taken BEFORE the
    // cache is read, so a caller that waited sees the finished measurement.
    let _turn = hardware::measuring_turn();
    let cached = hardware::load_speeds(&key);

    // The probe utterance, synthesized once for this whole walk and only if something is actually
    // unmeasured. `say -o` writes a file and plays nothing, so this is silent even mid-call.
    let probe_dir = std::env::temp_dir().join("richos-voice-calibration");
    // The probe and whether this walk rendered it.
    let mut probe_wav: Option<Option<(PathBuf, bool)>> = None;

    let resolution = hardware::resolve_live(&costs, &machine, |id| {
        if let Some(secs) = cached.get(id) {
            return Some(*secs);
        }
        let (wav, _) = probe_wav
            .get_or_insert_with(|| synthesize_probe(&costs.probe_text, &probe_dir))
            .clone()?;
        let model = resolve_model(id).ok()?;
        let earlier = hardware::load_unfinished(&key, id);
        let secs = time_decode(bin, &model, &wav, costs.live_ceiling_secs, earlier, |best, reps| {
            hardware::record_unfinished(&key, id, best, reps)
        })?;
        hardware::record_speed(&key, id, secs);
        Some(secs)
    });

    // The probe this walk rendered is garbage once the walk is done (CEO §54 addendum 2); still
    // under the measuring turn, so no other walk is reading it. One that was already there is
    // left as it was found.
    if let Some(Some((wav, true))) = &probe_wav {
        std::fs::remove_file(wav).ok();
    }

    // Said out loud on stderr as well as carried in provenance — the same channel the toolchain
    // warnings use, and the one a headless voice loop shares with whoever started it.
    if resolution.basis != Basis::TopRung {
        eprintln!("richos-voice: {}", resolution.provenance());
    }
    resolution
}

/// The shape an override takes: a resolution that records what the machine looked like without
/// pretending a rule chose anything.
fn overridden(model_id: String, machine: crate::hardware::Machine) -> crate::hardware::Resolution {
    crate::hardware::Resolution {
        model_id,
        basis: crate::hardware::Basis::Override,
        rejected: None,
        measured_secs: None,
        ceiling_secs: crate::hardware::Costs::load().live_ceiling_secs,
        machine,
    }
}

/// The shape a person's own choice takes ([`Recognizer::for_model`]): like [`overridden`], a
/// resolution that records the machine without pretending a rule chose anything.
fn chosen(model_id: String, machine: crate::hardware::Machine) -> crate::hardware::Resolution {
    crate::hardware::Resolution { basis: crate::hardware::Basis::Chosen, ..overridden(model_id, machine) }
}

/// Render the probe sentence to a 16 kHz mono WAV with macOS `say`. `None` if it cannot be done.
///
/// `-o` WRITES A FILE AND PLAYS NOTHING, which is the only reason calibrating at voice-mode start
/// is acceptable at all — the alternative would put a sentence through the speakers on first
/// launch. Same synthesizer `tts.rs` already depends on, so this adds no new requirement.
///
/// The render is NOT expected to be byte-identical to the one the reference figures came from —
/// `say` output drifts between renders and voices differ per machine. It does not need to be: the
/// gate is "under 1.000 s", and the ladder's rungs differ by 2.5x, so a few percent of drift in
/// the probe cannot move a verdict.
/// The probe utterance, and `true` when this call rendered it (`false`: it was already there).
fn synthesize_probe(text: &str, dir: &Path) -> Option<(PathBuf, bool)> {
    if text.trim().is_empty() {
        return None;
    }
    std::fs::create_dir_all(dir).ok()?;
    let out = dir.join("probe.wav");
    if out.exists() {
        return Some((out, false));
    }
    let status = Command::new("say")
        .arg("-o")
        .arg(&out)
        .arg("--data-format=LEI16@16000")
        .arg(text)
        .status()
        .ok()?;
    if status.success() && out.exists() {
        Some((out, true))
    } else {
        None
    }
}

/// How many reps a calibration may take before it is allowed to REJECT a rung.
///
/// THREE, AND ONLY WHEN REJECTING — see [`time_decode`] for why the asymmetry is sound rather
/// than a compromise. Three is what `utterance-sweep.sh` takes, and the spread it measured is
/// the reason more would be waste and fewer would be wrong.
const CALIBRATION_REJECT_REPS: usize = 3;

/// Wall-clock seconds for a decode of `wav` by `model`, at the argv this path really uses —
/// the BEST of up to [`CALIBRATION_REJECT_REPS`] reps, and usually just one.
///
/// # Why a single sample is not enough, measured rather than supposed
///
/// The first version of this took one sample, and running it caught its own defect on the CEO's
/// machine: `small.en` timed **1.005 s** against the 1.000 s ceiling — five milliseconds over —
/// and the machine was demoted to `base.en` and told so. The reference figures for that same
/// model are 0.512 s quiet and 0.800 s busy. Nothing was wrong with the machine; the sample was
/// taken while it was doing something else.
///
/// `docs/measurements/hardware-model-resolution-2026-09-10/` §3 predicted exactly this — the same
/// M4 measured ~50% slower under load, and that is why the cache keeps a MINIMUM. A single
/// sample is trivially its own minimum, so the cache's protection did nothing until the
/// measurement itself took more than one.
///
/// # Why the extra reps are only spent on a REJECTION
///
/// The minimum over reps only ever goes DOWN. So a rep that already clears the ceiling settles
/// the question — no number of further reps could overturn it — and the walk takes it and stops.
/// Only a rep that MISSES the ceiling is inconclusive, because it might be this one sample rather
/// than the machine, and only then are more reps bought.
///
/// That asymmetry is the right way round on cost as well as on logic: a rejection is the
/// expensive, sticky verdict — it takes a model away from the CEO and writes a sentence explaining
/// it — so a rejection is what deserves corroboration. On a quiet reference machine this is four
/// decodes in total (three to reject `q5_0`, one to accept `small.en`), about 4.4 s, ONCE for that
/// machine and that whisper binary.
///
/// [`decode_args`]`(None)` is passed VERBATIM, not an approximation. A calibration at different
/// settings from the shipping run measures a decode this product never performs — `-fa` alone is
/// worth 1.57 WER points and a measurable slice of the wall clock.
///
/// # Why the reps of one rung can span launches
///
/// `earlier` is what a launch that was quit in the middle of this rung had already taken (best,
/// reps), and `keep` saves each inconclusive rep as it is taken, so the reps of a rung add up
/// across launches to the same at most three rather than starting again (Ray, test55: a guest
/// quit half a minute into its measurement measured again on the next launch).
fn time_decode(
    bin: &Path,
    model: &Path,
    wav: &Path,
    ceiling_secs: f64,
    earlier: Option<(f64, usize)>,
    mut keep: impl FnMut(f64, usize),
) -> Option<f64> {
    let (mut best, taken) = match earlier {
        Some((secs, reps)) => (Some(secs), reps),
        None => (None, 0),
    };
    for rep in taken..CALIBRATION_REJECT_REPS {
        let started = Instant::now();
        let out = tethered_output(decoder_command(bin).arg("-m").arg(model).arg("-f").arg(wav).args(decode_args(None)))
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let secs = started.elapsed().as_secs_f64();
        let min = best.map_or(secs, |b: f64| b.min(secs));
        best = Some(min);
        // Settled: the minimum cannot rise, so no further rep could turn this into a rejection.
        if secs <= ceiling_secs {
            break;
        }
        if rep + 1 < CALIBRATION_REJECT_REPS {
            keep(min, rep + 1);
        }
    }
    best
}

/// **WHAT IS STANDING BETWEEN THIS MACHINE AND TALK TO RICH**, as a kind rather than a sentence.
///
/// THE REASON THIS TYPE EXISTS is that the three gaps below need three different actions.
/// `SttError::ceo_message` used to give `BinaryNotFound` and `ModelNotFound` the SAME words —
/// *"My ears aren't installed on this machine yet"* — because to the CEO they used to be the
/// same event: somebody else had to fix it either way. Now that RichOS can fetch its own model
/// (`crate::provision`, landed 516975db) that has stopped being true, so `ModelNotFound` carries
/// its own sentence and the two no longer collapse into one — this table is what the split is
/// for:
///
/// | gap | what RichOS can do about it |
/// |---|---|
/// | [`SpeechReadiness::ToolchainMissing`] | **Nothing.** The decoder is not a model and is not pinned. |
/// | [`SpeechReadiness::ModelMissing`] | Download the pinned weights this machine resolved to. |
/// | [`SpeechReadiness::Refused`] | **Nothing automatic.** Weights that are not the pinned weights are a fact to report, never one to paper over with a fresh download of something else. |
pub enum SpeechReadiness {
    /// Everything resolved and checked. Boxed because a `Recognizer` is far larger than the other
    /// variants and this enum is returned by value on every readiness probe.
    Ready(Box<Recognizer>),
    /// No `whisper-cli`. Not provisionable by this app — see the table above.
    ToolchainMissing { detail: String },
    /// The decoder is here and the weights this machine resolved to are not. `model_id` is what
    /// [`choose_model`] asked for, so an offer names the file the resolver will actually look for.
    ModelMissing { model_id: String, detail: String },
    /// Installed, and not what it claims to be.
    Refused { detail: String },
}

/// HAND-WRITTEN, not derived, and the difference is a leak. Deriving would require `Debug` on
/// `Recognizer`, whose fields are two absolute filesystem paths — so every `{:?}` of a readiness
/// answer anywhere in this product would print the CEO's home directory into whatever log caught
/// it. The model id is the part that identifies anything worth identifying.
impl std::fmt::Debug for SpeechReadiness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpeechReadiness::Ready(r) => write!(f, "Ready({})", r.model_id),
            SpeechReadiness::ToolchainMissing { detail } => write!(f, "ToolchainMissing({detail})"),
            SpeechReadiness::ModelMissing { model_id, detail } => write!(f, "ModelMissing({model_id}: {detail})"),
            SpeechReadiness::Refused { detail } => write!(f, "Refused({detail})"),
        }
    }
}

impl SpeechReadiness {
    /// A stable tag for the UI and the record. Never a sentence — sentences move.
    pub fn tag(&self) -> &'static str {
        match self {
            SpeechReadiness::Ready(_) => "ready",
            SpeechReadiness::ToolchainMissing { .. } => "toolchain-missing",
            SpeechReadiness::ModelMissing { .. } => "model-missing",
            SpeechReadiness::Refused { .. } => "refused",
        }
    }

    /// Can RichOS itself close this gap by downloading something it has pinned?
    ///
    /// TRUE FOR EXACTLY ONE VARIANT, and every caller that offers a download asks this rather than
    /// matching on the tag, so a fourth gap added later cannot become offerable by omission.
    pub fn provisionable(&self) -> bool {
        matches!(self, SpeechReadiness::ModelMissing { .. })
    }

    /// The CEO-facing line for a gap, or `None` when there is no gap. Reuses
    /// [`SttError::ceo_message`] so a machine that cannot be helped says exactly what it has
    /// always said.
    pub fn ceo_message(&self) -> Option<String> {
        match self {
            SpeechReadiness::Ready(_) => None,
            SpeechReadiness::ToolchainMissing { detail } => Some(SttError::BinaryNotFound(detail.clone()).ceo_message()),
            SpeechReadiness::ModelMissing { detail, .. } => Some(SttError::ModelNotFound(detail.clone()).ceo_message()),
            SpeechReadiness::Refused { detail } => Some(SttError::ToolchainRefused(detail.clone()).ceo_message()),
        }
    }
}

/// Resolve the binary, choose the model, check the toolchain — ONCE, and report the typed answer.
///
/// **THIS IS THE ONE RESOLUTION.** [`Recognizer::resolve`] is now a thin mapping of this function
/// onto [`SttError`], so the readiness probe the window reads and the check `start_voice_capture`
/// runs before it opens the microphone cannot disagree about the same machine — they are the same
/// code, not two functions written to the same description. The ordering below is unchanged from
/// what `resolve()` always did: binary first, then the model the machine resolved to, then the
/// toolchain check over both.
/// **The second model setup installs: for file and call transcription, not for live voice.**
///
/// The CEO, 2026-10-07, asked whether setup should download both speech models: *"Both, in this
/// nightly, yes."* Live voice keeps choosing its model by speed ([`choose_model`]); this one is
/// installed beside it whatever voice chose, because transcribing a file or a call is not bound
/// by the one-second live ceiling. Pinned in `engine/voice/models/model-pins.json`
/// (574,041,195 B).
pub const TRANSCRIPTION_MODEL_ID: &str = "large-v3-turbo-q5_0";

/// **Is the pinned model `model_id` on this machine and verified?** `Ok` with its path, or `Err`
/// with why not.
///
/// Resolved where voice resolves every model ([`resolve_model`]), so a copy the machine already
/// has (the CEO's `~/Models/Whisper`) counts, and verified by [`crate::toolchain::check`]: the
/// same identity check voice runs, refusing weights that are not the pinned weights, and caching
/// the hash by file identity in the toolchain lock, so a launch pays a stat rather than hashing
/// 574 MB once the file has been verified once.
pub fn model_verified(model_id: &str) -> Result<PathBuf, String> {
    let bin = resolve_whisper_bin().map_err(|e| e.to_string())?;
    let model = resolve_model(model_id).map_err(|e| e.to_string())?;
    let report = crate::toolchain::check(&bin, &model, model_id);
    if report.verdict() == crate::toolchain::Severity::Refuse {
        return Err(report.refusals().join(" "));
    }
    let pinned = crate::toolchain::pinned_sha256(model_id);
    match (pinned, report.model_sha256.as_deref()) {
        (Some(want), Some(got)) if want == got.to_ascii_lowercase() => Ok(model),
        (None, _) => Err(format!("{model_id} is not a pinned model")),
        _ => Err(format!("{} could not be read to verify it", model.display())),
    }
}

/// **Does voice have a model to hear with — answered WITHOUT calibrating.** `Ok` with the first
/// live-ladder model that is installed and verified ([`model_verified`]), best first, or `Err`.
///
/// Why first-run setup asks this rather than [`readiness`]: `readiness` CHOOSES the model by
/// timing real decodes on this machine ([`choose_model`]) whenever a rung's speed is not cached,
/// and setup's presence check runs at every launch, on the boot path. Measured 2026-10-07 in a
/// test-VM guest with both pinned models installed: large-v3-turbo-q5_0 4.929 s per utterance
/// (three tries before it is rejected) and small.en 2.310 s, and the boot missed gui-boot's 60 s
/// mark. The two answers agree: the ladder keeps only installed rungs and always accepts its
/// last, so `readiness` is `Ready` exactly when some live-ladder model is installed and its
/// weights verify, which is what this checks, at the cost of a cached hash.
pub fn voice_model_installed() -> Result<String, String> {
    let mut reasons = Vec::new();
    for id in crate::hardware::Costs::load().live_ladder {
        match model_verified(&id) {
            Ok(_) => return Ok(id),
            Err(why) => reasons.push(why),
        }
    }
    Err(if reasons.is_empty() { "the live ladder names no model".to_string() } else { reasons.join("; ") })
}

pub fn readiness() -> SpeechReadiness {
    let bin = match resolve_whisper_bin() {
        Ok(b) => b,
        Err(e) => return SpeechReadiness::ToolchainMissing { detail: e.to_string() },
    };
    let resolution = choose_model(&bin);
    let model_id = resolution.model_id.clone();
    let model = match resolve_model(&model_id) {
        Ok(m) => m,
        Err(e) => return SpeechReadiness::ModelMissing { model_id, detail: e.to_string() },
    };
    let toolchain = crate::toolchain::check(&bin, &model, &model_id);
    // Said out loud on stderr, not only stored. A warning nobody meets is not a warning, and
    // this is the one channel a headless voice loop shares with whoever started it.
    for w in toolchain.warnings() {
        eprintln!("richos-voice: {w}");
    }
    if toolchain.verdict() == crate::toolchain::Severity::Refuse {
        return SpeechReadiness::Refused { detail: toolchain.refusals().join(" ") };
    }
    SpeechReadiness::Ready(Box::new(Recognizer {
        bin,
        model,
        model_id,
        prompt: std::env::var("RICHOS_WHISPER_PROMPT").ok().filter(|s| !s.trim().is_empty()),
        toolchain,
        resolution,
    }))
}

/// A resolved, ready-to-use recognizer. Resolution happens ONCE at voice-mode start so a
/// missing model is a calm message at the toggle, not a failure in the middle of a sentence.
#[derive(Clone)]
pub struct Recognizer {
    bin: PathBuf,
    model: PathBuf,
    model_id: String,
    /// Optional decoding hint. The loro entity-biasing lever from the local-dictation notes
    /// plugs in HERE — feed `loro/entities.json` terms and whisper biases toward the
    /// company's names and jargon. Not wired to loro in v1; the seam is one string.
    prompt: Option<String>,
    /// Which binary, which ggml backends and which weights — established ONCE, here, and carried
    /// so every utterance is attributed to what actually heard it.
    toolchain: crate::toolchain::Report,
    /// WHY those weights and not better ones. `toolchain` answers "which model heard this?";
    /// this answers "and why was that the model on this machine?", which is the question a
    /// transcript could not answer at all while the id was a compile-time constant.
    resolution: crate::hardware::Resolution,
}

impl Recognizer {
    /// Resolve the binary and the model, and CHECK THEM, before the mic is ever opened.
    ///
    /// Resolution has always happened once here rather than per utterance, so that a missing model
    /// is a calm message at the toggle instead of a failure in the middle of a sentence. A
    /// SUBSTITUTED one now gets the same treatment, which is the point of doing the check in this
    /// function and not in `transcribe`: at 0.47–0.74 s per utterance there is no room to hash a
    /// 487 MB model on each one, and there is no need to — the binary cannot change between two
    /// sentences of one conversation, and the lock's cache means even this once is usually free.
    ///
    /// Weights that are not the pinned weights REFUSE. A binary or backend that is not the one
    /// this machine locked WARNS, loudly, naming both identities — see `toolchain.rs` for why the
    /// two differ. `RICHOS_WHISPER_STRICT_TOOLCHAIN=1` makes every warning a refusal.
    pub fn resolve() -> Result<Recognizer, SttError> {
        match readiness() {
            SpeechReadiness::Ready(r) => Ok(*r),
            SpeechReadiness::ToolchainMissing { detail } => Err(SttError::BinaryNotFound(detail)),
            SpeechReadiness::ModelMissing { detail, .. } => Err(SttError::ModelNotFound(detail)),
            SpeechReadiness::Refused { detail } => Err(SttError::ToolchainRefused(detail)),
        }
    }

    /// **The recognizer for ONE NAMED model**, for dictation, where the person picks More
    /// accurate or Faster himself (dictation plan section 2, "Which model runs").
    ///
    /// The same binary resolution and the same toolchain check as [`Recognizer::resolve`], for
    /// `model_id`, WITHOUT [`choose_model`]: voice mode times decodes on this machine to stay
    /// under its one-second live ceiling, and dictation must not, because the speed he trades
    /// for accuracy is his choice and not this machine's. Weights that are not the pinned weights
    /// refuse exactly as they do for voice. No prompt is taken from the environment: the decode
    /// flags are `decode_args(None)`, the measured settings.
    pub fn for_model(model_id: &str) -> Result<Recognizer, SttError> {
        let bin = resolve_whisper_bin()?;
        let model = resolve_model(model_id)?;
        let toolchain = crate::toolchain::check(&bin, &model, model_id);
        for w in toolchain.warnings() {
            eprintln!("richos-voice: {w}");
        }
        if toolchain.verdict() == crate::toolchain::Severity::Refuse {
            return Err(SttError::ToolchainRefused(toolchain.refusals().join(" ")));
        }
        Ok(Recognizer {
            bin,
            model,
            model_id: model_id.to_string(),
            prompt: None,
            toolchain,
            resolution: chosen(model_id.to_string(), crate::hardware::Machine::read()),
        })
    }

    /// One line naming the binary and the weights that heard this conversation. The answer to
    /// "which binary and which weights produced this?" for every turn this recognizer serves.
    ///
    /// UNCHANGED, AND STILL A `&str`. The resolution rides in [`Recognizer::provenance_full`]
    /// beside it rather than being spliced in here: this string is compared verbatim by
    /// `toolchain.rs`'s own test and is what existing readers already parse, and widening a
    /// stable identity line to carry a second, machine-dependent fact is how a record stops
    /// being comparable between two runs.
    pub fn provenance(&self) -> &str {
        &self.toolchain.provenance
    }

    /// The identity line PLUS why this model and not a better one — the whole answer, for a
    /// turn record or a support question.
    ///
    /// Example from the CEO's own M4, where the better rung exists and does not fit the budget:
    ///
    /// ```text
    /// whisper.cpp 1.9.1 bin:7dc20e3106d7 [BLAS/MTL/CPU] model:small.en@c6138d6d58ec
    ///   | model:small.en (hw-resolved, large-v3-turbo-q5_0 too slow here at 1.302s/utt > 1.000s;
    ///     this 0.512s/utt) mem:5873516544B/25769803776B pressure:warn
    /// ```
    pub fn provenance_full(&self) -> String {
        format!("{} | {}", self.toolchain.provenance, self.resolution.provenance())
    }

    /// Why this machine got this model. Carries the CEO-facing sentence when there is one.
    pub fn resolution(&self) -> &crate::hardware::Resolution {
        &self.resolution
    }

    /// The full identity report, for a caller that wants to record more than the line.
    pub fn toolchain(&self) -> &crate::toolchain::Report {
        &self.toolchain
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }
    pub fn model_path(&self) -> &Path {
        &self.model
    }
    pub fn binary_path(&self) -> &Path {
        &self.bin
    }

    /// Initialize the decoder before the first user utterance. The first decoder invocation
    /// measured 9.8 seconds on a fresh packaged decoder; subsequent recognition was under one
    /// second. Cache only successful warmups by the verified binary and model identities.
    /// Concurrent readiness and voice-start callers join the same initialization.
    pub fn warm(&self, scratch_root: &Path) -> Result<(), SttError> {
        use std::collections::HashSet;
        use std::sync::{Mutex, OnceLock};
        static WARMED: OnceLock<Mutex<HashSet<(String, String)>>> = OnceLock::new();
        let key = (self.toolchain.bin_sha256.clone(), self.toolchain.model_sha256.clone().unwrap_or_default());
        let mut warmed = WARMED.get_or_init(|| Mutex::new(HashSet::new())).lock()
            .map_err(|_| SttError::Io("speech initialization lock poisoned".into()))?;
        if warmed.contains(&key) { return Ok(()); }
        std::fs::create_dir_all(scratch_root).map_err(|e| SttError::Io(e.to_string()))?;
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| SttError::Io(e.to_string()))?.as_nanos();
        let dir = scratch_root.join(format!("richos-speech-warm-{}-{stamp}", std::process::id()));
        std::fs::create_dir(&dir).map_err(|e| SttError::Io(e.to_string()))?;
        // A silent, app-owned file, never microphone audio and never a conversation prompt.
        // Use the regular pinned decoder and its normal flags; do not change models or threads.
        let result = self.transcribe_bounded(&vec![0.0; SAMPLE_RATE as usize], &dir, std::time::Duration::from_secs(30));
        if let Err(error) = std::fs::remove_dir_all(&dir) {
            eprintln!("[richos-voice] speech initialization scratch cleanup failed: {error}");
        }
        let (_, ms) = result?;
        warmed.insert(key);
        eprintln!("[richos-voice] speech decoder initialized before capture in {ms} ms");
        Ok(())
    }

    /// Transcribe one utterance (16 kHz mono f32). Returns the text and the MEASURED
    /// wall-clock recognition latency.
    pub fn transcribe(&self, samples: &[f32], scratch_dir: &Path) -> Result<(String, u64), SttError> {
        self.transcribe_inner(samples,scratch_dir,None,None)
    }
    /// HTTP callers can bound their owned decoder without changing the desktop loop.
    pub fn transcribe_bounded(&self,samples:&[f32],scratch_dir:&Path,timeout:std::time::Duration)->Result<(String,u64),SttError> {
        self.transcribe_inner(samples,scratch_dir,Some(timeout),None)
    }
    /// [`Self::transcribe_bounded`] that also ends early when `stop` is set: the decoder is
    /// killed and reaped, and the error is `Interrupted`. The dictation tool's writing worker
    /// uses it so an ordinary quit of the tool joins the worker instead of orphaning whisper-cli
    /// and leaving the recording on disk (review finding 1, 2026-10-08).
    pub fn transcribe_bounded_until(&self,samples:&[f32],scratch_dir:&Path,timeout:std::time::Duration,stop:&std::sync::atomic::AtomicBool)->Result<(String,u64),SttError> {
        self.transcribe_inner(samples,scratch_dir,Some(timeout),Some(stop))
    }
    fn transcribe_inner(&self,samples:&[f32],scratch_dir:&Path,timeout:Option<std::time::Duration>,stop:Option<&std::sync::atomic::AtomicBool>)->Result<(String,u64),SttError> {
        std::fs::create_dir_all(scratch_dir).map_err(|e| SttError::Io(e.to_string()))?;
        let wav_path = scratch_dir.join(format!("utt-{}.wav", std::process::id()));
        wav::write_pcm16_mono(&wav_path, samples, SAMPLE_RATE).map_err(|e| SttError::Io(e.to_string()))?;

        let started = Instant::now();
        let mut cmd = decoder_command(&self.bin);
        cmd.arg("-m").arg(&self.model).arg("-f").arg(&wav_path);
        cmd.args(decode_args(self.prompt.as_deref()));
        let out = match timeout {
            Some(timeout)=>bounded_decoder_until(&mut cmd,scratch_dir,timeout,stop),
            None=>tethered_output(&mut cmd),
        };
        // The recording is removed whatever the decoder did: a timeout, a stop or a failure
        // leaves no voice on disk here (the caller's scratch folder removes the rest).
        let _ = std::fs::remove_file(&wav_path);
        let out = out.map_err(|e| SttError::Io(e.to_string()))?;
        let elapsed_ms = started.elapsed().as_millis() as u64;

        if !out.status.success() {
            return Err(SttError::Failed {
                status: out.status.to_string(),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            });
        }
        Ok((clean_transcript(&String::from_utf8_lossy(&out.stdout)), elapsed_ms))
    }
}

/// **`whisper-cli` TIED TO THIS PROCESS'S LIFE**: `bin` and the arguments added after this, run
/// so that it ends when this process ends, however it ends.
///
/// THE DEFECT (Ray, test55, nightly 54 and 55): a measurement decode kept running at full CPU
/// after the app that started it had been quit. The app is quit by a signal more often than by
/// its own exit path (the test VM's relaunch sends SIGTERM, a crash or a Force Quit sends none
/// of it), no code of the app's runs then, and macOS has no "end my children when I end".
///
/// So the tie is made by the kernel. A small `/bin/sh` starts the decoder and a watcher; the
/// watcher reads a pipe whose only writing end this process holds and never writes, so the read
/// returns exactly when this process closes it: when the decode is over, or when this process
/// is gone, because the kernel closes every descriptor of a process that ends. The watcher then
/// ends the decoder. When the decoder ends first, the shell ends the watcher and exits with the
/// decoder's own status. No polling and no clock.
///
/// The writing end is this command's stdin: a caller runs it through [`tethered_output`] or
/// [`bounded_decoder_until`], which hold it until the decode is over.
pub fn decoder_command(bin: &Path) -> Command {
    let mut command = Command::new("/bin/sh");
    command.arg("-c").arg(DECODER_TETHER).arg("richos-decoder").arg(bin);
    command.stdin(std::process::Stdio::piped());
    command
}

/// The shell of [`decoder_command`]. The decoder gets an empty stdin and not the tether, and the
/// watcher keeps the tether as descriptor 3 (a background job's own stdin is `/dev/null`).
const DECODER_TETHER: &str = "exec 3<&0 0</dev/null
\"$@\" 3<&- &
decoder=$!
(read -r _ <&3; kill -KILL \"$decoder\" 2>/dev/null) &
watcher=$!
exec 3<&-
wait \"$decoder\"
status=$?
kill -KILL \"$watcher\" 2>/dev/null
exit \"$status\"";

/// Run a [`decoder_command`] to the end and collect what it printed, holding its tether until
/// then. `Command::output` would close the tether at once (it drops a piped stdin before
/// waiting), which ends the decoder before it starts.
pub fn tethered_output(command: &mut Command) -> std::io::Result<std::process::Output> {
    use std::process::Stdio;
    let mut child = command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let tether = child.stdin.take();
    let out = child.wait_with_output();
    drop(tether);
    out
}

// Spool bounded decoder output to owned scratch rather than blocking on full pipes.
// Drop always kills/reaps only this child. No process-name or USB resets are involved. A
// [`decoder_command`]'s tether is closed first, so its decoder ends with it.
// `pub` since dictation: its scratch-folder tests run the real decoder handling twice in a row and
// past its deadline (`src-tauri/src/dictation/scratch.rs`).
pub fn bounded_decoder(command:&mut Command,dir:&Path,timeout:std::time::Duration)->std::io::Result<std::process::Output> {
    bounded_decoder_until(command,dir,timeout,None)
}

/// [`bounded_decoder`] that also returns `Interrupted`, with the child killed and reaped, as
/// soon as `stop` is set. Polled every 20 ms, so a stop costs at most that plus the kill.
pub fn bounded_decoder_until(command:&mut Command,dir:&Path,timeout:std::time::Duration,stop:Option<&std::sync::atomic::AtomicBool>)->std::io::Result<std::process::Output> {
    use std::{io,process::{Child,Stdio},os::unix::fs::OpenOptionsExt};
    struct Owned(Child);
    impl Drop for Owned {
        fn drop(&mut self) {
            drop(self.0.stdin.take());
            match self.0.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) => {},
                Err(error) => eprintln!("[richos] could not check owned decoder before cleanup: {error}"),
            }
            if let Err(error) = self.0.kill() {
                eprintln!("[richos] could not stop owned decoder: {error}");
            }
            if let Err(error) = self.0.wait() {
                eprintln!("[richos] could not reap owned decoder: {error}");
            }
        }
    }
    let output=dir.join("decoder.stdout");let errors=dir.join("decoder.stderr");
    let file=|path:&Path|std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path);
    // A pipe never written rather than `/dev/null`: it is a [`decoder_command`]'s tether, held
    // here until the guard drops, and an empty input to any other command.
    let mut child=Owned(command.stdin(Stdio::piped()).stdout(file(&output)?).stderr(file(&errors)?).spawn()?);
    let started=Instant::now();
    loop {
        if stop.is_some_and(|s|s.load(std::sync::atomic::Ordering::SeqCst)) {return Err(io::Error::new(io::ErrorKind::Interrupted,"decoder stopped by its owner"))}
        if started.elapsed()>=timeout {return Err(io::Error::new(io::ErrorKind::TimedOut,"decoder deadline"))}
        if [&output,&errors].iter().any(|p|std::fs::metadata(p).map(|m|m.len()>128000).unwrap_or(true)) {
            return Err(io::Error::new(io::ErrorKind::InvalidData,"decoder output limit"));
        }
        if let Some(status)=child.0.try_wait()? {
            return Ok(std::process::Output{status,stdout:std::fs::read(&output)?,stderr:std::fs::read(&errors)?});
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Collapse whisper's stdout into one line of plain text.
pub fn clean_transcript(raw: &str) -> String {
    raw.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whisper's documented silence/noise hallucinations. This is a NARROW, evidence-based list,
/// not a general stopword filter: "yes", "no", "yeah" and "okay" are real CEO decisions and
/// must survive. "you", "thank you" and "goodbye" are NOT here (hunt part 1, finding 37): the
/// silence that makes `small.en` say "you" never reaches it, because the controller refuses
/// audio with no measured voice before any transcript exists (`VoiceEvidence`), so a short
/// phrase that arrives here was said by someone.
const NOISE_TRANSCRIPTS: &[&str] = &[
    "thanks for watching",
    "thank you for watching",
    "please subscribe",
    "subscribe",
];

/// Is this transcript worth sending to Rich as a turn?
///
/// An open mic in a quiet room WILL produce spurious transcripts — see [`NOISE_TRANSCRIPTS`].
/// Sending one costs a real Claude turn and puts words in the CEO's mouth in the durable
/// ledger, which is the worse failure. Dropping a genuine "yes" is the failure this list is
/// kept narrow to avoid.
pub fn is_meaningful(transcript: &str) -> bool {
    let stripped = strip_annotations(transcript);
    let norm: String = stripped
        .trim()
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    if norm.is_empty() {
        return false;
    }
    if !norm.chars().any(|c| c.is_alphanumeric()) {
        return false;
    }
    !NOISE_TRANSCRIPTS.contains(&norm.as_str())
}

/// Remove `[BLANK_AUDIO]`, `(upbeat music)` and friends — whisper's non-speech annotations.
///
/// `pub(crate)` since dictation: words it pastes into another app pass through this first
/// (`dictation.rs`), because [`clean_transcript`] leaves the annotations in the text.
pub(crate) fn strip_annotations(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth_sq = 0i32;
    let mut depth_par = 0i32;
    for c in s.chars() {
        match c {
            '[' => depth_sq += 1,
            ']' => depth_sq = (depth_sq - 1).max(0),
            '(' => depth_par += 1,
            ')' => depth_par = (depth_par - 1).max(0),
            _ if depth_sq == 0 && depth_par == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoder_warmup_is_silent_once_per_identity_and_a_failure_is_not_cached() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("richos-warm-test-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let bin = root.join("decoder");
        let calls = root.join("calls");
        let fail = root.join("fail");
        std::fs::write(&bin, format!("#!/bin/sh\nprintf 'call\\n' >> '{}'\n[ ! -f '{}' ] || exit 7\nprintf 'discard these warmup words\\n'\n", calls.display(), fail.display())).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut recognizer = Recognizer {
            bin, model: root.join("model"), model_id: "small.en".into(), prompt: None,
            toolchain: crate::toolchain::Report { findings: Vec::new(), strict: false,
                provenance: "warmup test".into(), bin_sha256: format!("test-{}", root.display()),
                bin_version: None, model_sha256: Some("first".into()), model_pinned: true,
                model_hash_cached: false, first_run: true },
            resolution: chosen("small.en".into(), crate::hardware::Machine::read()),
        };
        std::fs::write(&fail, "fail").unwrap();
        assert!(recognizer.warm(&root).is_err());
        std::fs::remove_file(&fail).unwrap();
        recognizer.warm(&root).unwrap();
        recognizer.clone().warm(&root).unwrap();
        assert_eq!(std::fs::read_to_string(&calls).unwrap().lines().count(), 2, "failed initialization must retry; successful initialization must be joined");
        recognizer.toolchain.model_sha256 = Some("replacement-model".into());
        recognizer.warm(&root).unwrap();
        assert_eq!(std::fs::read_to_string(&calls).unwrap().lines().count(), 3, "different verified model bytes require initialization");
        assert!(!std::fs::read_dir(&root).unwrap().any(|entry| entry.unwrap().file_name().to_string_lossy().starts_with("richos-speech-warm-")), "warmup output must be removed on success and failure");
        std::fs::remove_dir_all(root).unwrap();
    }


    /// INVARIANT: this path never decodes at whisper.cpp's own text-context default.
    ///
    /// It did until 2026-09-10 — `-mc` was simply absent, so every dictation ran at `-1`, the
    /// setting that filled 7.8% of a 92-minute channel with one fabricated sentence in the sibling
    /// service. The whole point of the settings table is that a value nobody chose is not a value.
    #[test]
    fn decode_context_is_never_the_vendor_default() {
        for prompt in [None, Some("Halden Freight, Priya Sandoval")] {
            let args = decode_args(prompt);
            let i = args.iter().rposition(|a| a == "-mc").expect("-mc must be emitted");
            assert_ne!(args[i + 1], "-1", "the vendor default must never be what this path decodes at");
            assert!(args[i + 1].parse::<i32>().unwrap() >= 0);
        }
    }

    /// INVARIANT: a configured prompt is a prompt that WORKS.
    ///
    /// At `-mc 0` whisper accepts `--prompt` and ignores it — measured byte-identical to passing no
    /// prompt at all. So the two must never be shipped together: entity biasing that silently does
    /// nothing is worse than no entity biasing, because it reports success.
    #[test]
    fn a_configured_prompt_gets_a_context_budget_to_live_in() {
        let with = decode_args(Some("Halden Freight"));
        let i = with.iter().rposition(|a| a == "-mc").unwrap();
        assert!(
            with[i + 1].parse::<i32>().unwrap() >= 64,
            "a prompt needs room in the text context or it is inert; got -mc {}",
            with[i + 1]
        );
        assert!(with.iter().any(|a| a == "--prompt"));
        assert!(with.iter().any(|a| a == "Halden Freight"));

        // And without a prompt there is nothing to make room for, so the invariant is 0.
        let without = decode_args(None);
        let j = without.iter().rposition(|a| a == "-mc").unwrap();
        assert_eq!(without[j + 1], "0");
        assert!(!without.iter().any(|a| a == "--prompt"));
    }

    /// INVARIANT: flash attention is PASSED here too, for the same measured reason as the service —
    /// `-nfa` costs 1.57 WER points, and a default that valuable is not left to the vendor.
    #[test]
    fn flash_attention_is_passed_not_inherited() {
        assert!(decode_args(None).iter().any(|a| a == "-fa"));
    }

    /// INVARIANT: every flag this path passes is one the settings table decided. Adding a flag
    /// without deciding it fails here, which is the point.
    #[test]
    fn no_flag_ships_without_a_row_in_the_settings_table() {
        const DECIDED: &[&str] = &["-l", "-t", "-fa", "-np", "-nt", "-mc", "--prompt"];
        const TAKES_VALUE: &[&str] = &["-l", "-t", "-mc", "--prompt"];
        let args = decode_args(Some("Halden Freight"));
        let mut i = 0;
        while i < args.len() {
            assert!(
                DECIDED.contains(&args[i].as_str()),
                "{} reaches whisper-cli but is not in the settings decision table",
                args[i]
            );
            if TAKES_VALUE.contains(&args[i].as_str()) {
                i += 1;
            }
            i += 1;
        }
    }

    /// INVARIANT: whisper's stdout becomes one clean line, whatever leading newline or
    /// padding the CLI adds. (Observed: it prefixes "\n " before the text.)
    #[test]
    fn whisper_stdout_becomes_one_clean_line() {
        assert_eq!(
            clean_transcript("\n Rich, what is the status of the voice pipeline today?"),
            "Rich, what is the status of the voice pipeline today?"
        );
        assert_eq!(clean_transcript("\n one\n two \n\n three\n"), "one two three");
        assert_eq!(clean_transcript(""), "");
    }

    /// INVARIANT: the video-outro hallucinations are rejected.
    #[test]
    fn the_video_outro_hallucinations_are_rejected() {
        assert!(!is_meaningful("Thanks for watching!"));
        assert!(!is_meaningful("Please subscribe."));
    }

    /// Finding 37: a real thank-you or goodbye that passed the audio-evidence gate is a turn.
    #[test]
    fn a_real_thank_you_or_goodbye_is_a_turn() {
        for said in ["Thank you.", "Thanks", "Goodbye", "Bye.", "You."] {
            assert!(is_meaningful(said), "{said} was dropped as noise");
        }
    }

    /// INVARIANT: non-speech annotations alone are not a turn.
    #[test]
    fn annotation_only_transcripts_are_not_turns() {
        assert!(!is_meaningful("[BLANK_AUDIO]"));
        assert!(!is_meaningful("(upbeat music)"));
        assert!(!is_meaningful("[Music]"));
        assert!(!is_meaningful("   "));
        assert!(!is_meaningful("..."));
        assert!(!is_meaningful("."));
    }

    /// INVARIANT: the noise filter is NARROW. A one-word decision from the CEO is the whole
    /// point of voice mode and must never be swallowed.
    #[test]
    fn one_word_ceo_decisions_are_never_swallowed_by_the_noise_filter() {
        for word in ["Yes.", "No.", "Yeah", "Okay", "Approved.", "Stop.", "Ship it."] {
            assert!(is_meaningful(word), "{word} was dropped as noise");
        }
    }

    /// INVARIANT: real speech passes, including speech that merely CONTAINS a noise phrase.
    #[test]
    fn real_speech_passes_even_when_it_contains_a_noise_phrase() {
        assert!(is_meaningful("Renegotiate Acme and get me the number by Thursday."));
        assert!(is_meaningful("Thank you, that's exactly right."));
        assert!(is_meaningful("[cough] renegotiate Acme"));
    }

    /// INVARIANT: **the conversational model is whatever THIS MACHINE can decode inside the
    /// conversational budget** — the RULE, not a string.
    ///
    /// # What this replaced, and why it had to go
    ///
    /// It used to be `the_conversational_default_model_is_small_en_for_latency`, and it read:
    ///
    /// ```text
    /// assert_eq!(DEFAULT_MODEL_ID, "small.en");
    /// assert_ne!(DEFAULT_MODEL_ID, "large-v3-turbo");
    /// assert_ne!(DEFAULT_MODEL_ID, "large-v3-turbo-q5_0");
    /// ```
    ///
    /// That test was careful about the wrong thing. Its own comment worried that pinning one
    /// turbo id would let the assertion pass while no longer testing what it was written to test —
    /// a real hazard, correctly spotted — and it answered by naming BOTH turbo ids. But the defect
    /// was a level up: **it asserted a constant, so it passed identically on every machine in the
    /// world and would have gone on passing if `small.en` had been the wrong choice for all of
    /// them.** A test that cannot fail on a bad machine is not testing the choice; it is
    /// testifying that somebody typed a string.
    ///
    /// The CEO named the same defect from the outside: *"WHY IS THE SHITTIEST POSSIBLE HARDWARE
    /// HARDCODED INTO THE APP???"*
    ///
    /// # What is pinned now
    ///
    /// The property that actually matters, over machines the CI runner will never be:
    ///
    /// - a machine fast enough for the better rung is GIVEN the better rung, and is told nothing,
    ///   because that is the product working;
    /// - a machine that is not is stepped DOWN and is TOLD, in words with no path and no model
    ///   filename in them;
    /// - the ceiling those verdicts turn on is the one this module's own docs state.
    ///
    /// Not one assertion below names `small.en`. Change the ladder, add a rung, re-measure a
    /// model, and this test goes on being the thing it was written to be — which is exactly what
    /// the old one could not do.
    #[test]
    fn the_conversational_model_is_the_one_this_machine_can_decode_in_time() {
        use crate::hardware::{resolve_live, Basis, Costs, Machine, Pressure};
        let costs = Costs::load();
        let machine = Machine {
            total_bytes: 25_769_803_776,
            available_bytes: 5_873_516_544,
            pressure: Pressure::Warn,
            cores: 10,
        };

        // The ceiling is this module's stated failure point, carried in the registry rather than
        // restated. If that sentence in the module docs ever changes, this is where it bites.
        assert_eq!(costs.live_ceiling_secs, 1.0, "a second of dead air is the stated failure point");

        // A machine that clears the ceiling on the top rung gets the top rung — whatever it is
        // called — and is told nothing.
        let fast = resolve_live(&costs, &machine, |_| Some(costs.live_ceiling_secs - 0.5));
        assert_eq!(fast.model_id, costs.live_ladder[0], "the best rung this machine can afford");
        assert_eq!(fast.basis, Basis::TopRung);
        assert_eq!(fast.ceo_message(), None, "the product working is not a notification");

        // A machine that does not clear it on the top rung is stepped down silently.
        let slow = resolve_live(&costs, &machine, |id| {
            if id == costs.live_ladder[0] {
                Some(costs.live_ceiling_secs + 0.302)
            } else {
                Some(costs.live_ceiling_secs - 0.488)
            }
        });
        assert_ne!(slow.model_id, costs.live_ladder[0], "the rung it could not afford");
        assert_eq!(slow.basis, Basis::TooSlow);
        assert_eq!(slow.ceo_message(), None, "automatic model selection stays out of the conversation");
    }

    /// INVARIANT: the two env overrides still win outright, and they win in the two different
    /// ways they always have — an ID names a model, a PATH pins the weights.
    ///
    /// The path form is the sharper case: `resolve_model` returns that one file for EVERY id, so
    /// a ladder walk under it would time the same weights on every rung and report a resolution
    /// that resolved nothing. It short-circuits to `Override` instead, and `FALLBACK_MODEL_ID`
    /// supplies the id `toolchain.rs` looks the pin up by — preserving exactly the behavior that
    /// override has always had, including refusing when the file is not what the pin says.
    #[test]
    fn an_engineer_who_names_a_model_or_a_path_is_not_second_guessed() {
        assert_eq!(FALLBACK_MODEL_ID, "small.en", "the id a path override is attributed to");
        // The resolver is never consulted for an override, so the basis is the whole assertion:
        // it records that no rule ran, rather than a rung that was never chosen.
        let m = crate::hardware::Machine::read();
        let r = overridden("large-v3-turbo".into(), m);
        assert_eq!(r.basis, crate::hardware::Basis::Override);
        assert_eq!(r.model_id, "large-v3-turbo");
        assert_eq!(r.ceo_message(), None, "an engineer's own choice is not explained back to him");
        assert!(r.provenance().contains("env-override"), "{}", r.provenance());
    }

    /// INVARIANT: a model he chose is recorded as his choice, not as an engineer's override and
    /// not as a rung the resolver took, and it is never explained back to him.
    #[test]
    fn a_model_he_chose_is_recorded_as_his_choice() {
        let r = chosen("small.en".into(), crate::hardware::Machine::read());
        assert_eq!(r.basis, crate::hardware::Basis::Chosen);
        assert_eq!(r.model_id, "small.en");
        assert_eq!(r.ceo_message(), None);
        assert!(r.provenance().contains("chosen in Dictation settings"), "{}", r.provenance());
        assert!(!r.provenance().contains("env-override"), "{}", r.provenance());
    }

    /// INVARIANT: `for_model` resolves the NAMED weights and no others. Asked for a model that
    /// is not on this machine it says which file it looked for, rather than falling back to a
    /// rung of its own choosing (the fallback between More accurate and Faster is dictation's
    /// decision, in `dictation.rs`, where it is logged).
    #[test]
    fn for_model_refuses_a_model_that_is_not_here_by_name() {
        let id = "no-such-model-for-dictation";
        match Recognizer::for_model(id) {
            Ok(_) => panic!("a model that does not exist resolved"),
            Err(SttError::ModelNotFound(detail)) => assert!(detail.contains(&format!("ggml-{id}.bin")), "{detail}"),
            // A machine with no whisper-cli at all refuses one step earlier, which is the same
            // property: nothing was substituted.
            Err(SttError::BinaryNotFound(_)) => {}
            Err(other) => panic!("unexpected refusal: {other}"),
        }
    }

    /// INVARIANT: a missing recognizer reaches the CEO as a calm line with no path in it,
    /// while the developer-facing Display keeps the path.
    #[test]
    fn a_missing_recognizer_reaches_the_ceo_without_a_path_in_it() {
        let e = SttError::ModelNotFound("/Users/x/.config/open-wispr/models/ggml-small.en.bin".into());
        assert!(!e.ceo_message().contains('/'));
        assert!(e.to_string().contains("ggml-small.en.bin"));
    }

    /// INVARIANT: `ModelNotFound` stopped sharing words with `BinaryNotFound` on 2026-09-17,
    /// the day RichOS gained the ability to fetch its own model (`crate::provision`, landed
    /// 516975db). Before that split the two variants' `ceo_message()`s were identical BY
    /// DESIGN — this test pins that they no longer are, and would have failed on its own
    /// premise had it been written a day earlier.
    ///
    /// POSITIVE CONTROL: `SpeechReadiness::ToolchainMissing` — the gap RichOS genuinely cannot
    /// close on its own — still routes through `BinaryNotFound` and still gets the untouched
    /// sentence. Without this half, a refactor that deleted the split entirely (making every
    /// assertion above vacuously true by making `model_missing` equal to nothing in
    /// particular) would not be caught.
    #[test]
    fn model_missing_stopped_sharing_words_with_a_missing_decoder() {
        let model_missing = SttError::ModelNotFound("ggml-small.en.bin not found in /x".into()).ceo_message();
        let binary_missing = SttError::BinaryNotFound("whisper-cli is not on PATH".into()).ceo_message();
        assert_ne!(model_missing, binary_missing, "the two gaps need different words now that one of them is fixable");
        assert!(
            model_missing.contains("I can fetch it myself"),
            "ModelNotFound must say RichOS can get this one on its own: {model_missing}"
        );
        assert!(!model_missing.contains('/'), "no paths reach him: {model_missing}");
        assert!(
            !model_missing.contains("whoever set RichOS up"),
            "that phrase names a party for a gap RichOS can now close itself: {model_missing}"
        );

        // Positive control: the gap RichOS genuinely cannot fix itself is untouched — same
        // words as always — so the pin above is not vacuously true because nothing routes
        // through the shared sentence any more.
        let toolchain_missing = SpeechReadiness::ToolchainMissing { detail: "whisper-cli is not on PATH".into() }
            .ceo_message()
            .expect("a missing decoder is still told about");
        assert_eq!(
            toolchain_missing, binary_missing,
            "ToolchainMissing must still route through BinaryNotFound's unchanged sentence"
        );

        // The readiness-level `ModelMissing` variant carries the SAME new sentence as the
        // `SttError` it wraps, so `voice_readiness`'s boot-log line and `Recognizer::resolve`'s
        // `Err` path can never disagree about the same gap.
        let model_missing_readiness = SpeechReadiness::ModelMissing {
            model_id: "small.en".into(),
            detail: "ggml-small.en.bin not found in /x".into(),
        }
        .ceo_message()
        .expect("a missing model is still told about");
        assert_eq!(model_missing_readiness, model_missing);
    }

    /// Resolution is environment-dependent, so this asserts the CONTRACT (a result either
    /// way, never a panic) rather than a machine-specific path.
    #[test]
    fn resolution_returns_a_result_and_never_panics() {
        let _ = resolve_whisper_bin();
        let _ = resolve_model("small.en");
        let _ = resolve_model("definitely-not-a-real-model");
        assert!(resolve_model("definitely-not-a-real-model").is_err());
    }
}

#[cfg(test)]
mod delivered_decoder_tests {
    use super::*;

    /// A temporary directory holding `<runtime>/bin/whisper-cli` and an empty developer prefix.
    fn machine(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("richos-delivered-whisper-{tag}-{}", std::process::id()));
        if root.exists() {
            std::fs::remove_dir_all(&root).unwrap();
        }
        let bin = root.join("engine/runtime/bin");
        let homebrew = root.join("opt/homebrew/bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&homebrew).unwrap();
        std::fs::write(bin.join("whisper-cli"), b"#!/bin/sh\n").unwrap();
        (root, bin, homebrew)
    }

    /// THE CEO'S CASE (2026-10-07): a user's Mac with no Homebrew and nothing on PATH. Before the
    /// runtime carried a decoder this was `BinaryNotFound` and voice was not offered at all.
    #[test]
    fn a_mac_without_homebrew_or_path_resolves_the_runtimes_copy() {
        let (root, bin, homebrew) = machine("bare");
        let resolved = whisper_bin_from(None, Some(&bin), || None, &[homebrew.as_path()]);
        assert_eq!(resolved.unwrap(), bin.join("whisper-cli"));
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The runtime's pinned copy wins over a PATH copy and a Homebrew copy; PATH is not even asked.
    #[test]
    fn the_runtimes_copy_comes_before_path_and_homebrew() {
        let (root, bin, homebrew) = machine("both");
        std::fs::write(homebrew.join("whisper-cli"), b"#!/bin/sh\n").unwrap();
        let resolved = whisper_bin_from(None, Some(&bin), || panic!("PATH was asked"), &[homebrew.as_path()]);
        assert_eq!(resolved.unwrap(), bin.join("whisper-cli"));
        // Without a delivered runtime (a development build), Homebrew's is still found.
        let developer = whisper_bin_from(None, None, || None, &[homebrew.as_path()]);
        assert_eq!(developer.unwrap(), homebrew.join("whisper-cli"));
        // RICHOS_WHISPER_BIN is the explicit developer override and wins over everything.
        let named = homebrew.join("whisper-cli");
        let overridden = whisper_bin_from(Some(named.to_str().unwrap()), Some(&bin), || None, &[]);
        assert_eq!(overridden.unwrap(), named);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A runtime directory without the file falls through, and nothing anywhere is a typed miss.
    #[test]
    fn nothing_anywhere_is_binary_not_found() {
        let (root, bin, homebrew) = machine("none");
        std::fs::remove_file(bin.join("whisper-cli")).unwrap();
        let missing = whisper_bin_from(None, Some(&bin), || None, &[homebrew.as_path()]);
        assert!(matches!(missing, Err(SttError::BinaryNotFound(_))));
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod bounded_tests {
    use super::*;
    #[test] fn decoder_deadline_reaps_its_child() {
        let directory=std::env::temp_dir().join(format!("bounded-decoder-{}",std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let started=Instant::now();let result=bounded_decoder(Command::new("/bin/sleep").arg("10"),&directory,std::time::Duration::from_millis(100));
        assert_eq!(result.unwrap_err().kind(),std::io::ErrorKind::TimedOut);assert!(started.elapsed().as_secs()<2);
        std::fs::remove_dir_all(directory).unwrap();
    }

    /// INVARIANT (review finding 1): a decoder told to stop ends at once, `Interrupted`, with its
    /// child gone, long before its deadline; a stop never set changes nothing.
    #[test] fn decoder_stop_reaps_its_child_before_the_deadline() {
        use std::sync::{atomic::{AtomicBool,Ordering},Arc};
        let directory=std::env::temp_dir().join(format!("bounded-decoder-stop-{}",std::process::id()));
        std::fs::remove_dir_all(&directory).ok();
        std::fs::create_dir_all(&directory).unwrap();
        let stop=Arc::new(AtomicBool::new(false));
        let flag=stop.clone();
        std::thread::spawn(move||{std::thread::sleep(std::time::Duration::from_millis(80));flag.store(true,Ordering::SeqCst);});
        let pid_file=directory.join("child.pid");
        // The verdict is the error kind: Interrupted comes only from the stop, never from the
        // 10 s deadline (TimedOut) or the child ending (Ok), so no clock is read.
        let result=bounded_decoder_until(Command::new("/bin/sh").args(["-c","echo $$ > \"$1\"; exec /bin/sleep 10","x"]).arg(&pid_file),&directory,std::time::Duration::from_secs(10),Some(&stop));
        assert_eq!(result.unwrap_err().kind(),std::io::ErrorKind::Interrupted);
        let pid:i32=std::fs::read_to_string(&pid_file).unwrap().trim().parse().unwrap();
        // SAFETY: signal 0 checks existence of the child this test started, and nothing else.
        assert_ne!(unsafe{libc::kill(pid,0)},0,"the stopped decoder (pid {pid}) is gone");
        std::fs::remove_dir_all(directory).unwrap();
    }

    /// INVARIANT: a [`decoder_command`] passed its deadline ends its DECODER, not only the shell
    /// that holds it, and a finished one hands back the decoder's own words and status.
    #[test] fn a_tethered_decoder_ends_at_its_deadline_and_reports_like_the_bare_one() {
        let directory=std::env::temp_dir().join(format!("bounded-decoder-tether-{}",std::process::id()));
        std::fs::remove_dir_all(&directory).ok();
        std::fs::create_dir_all(&directory).unwrap();
        let pid_file=directory.join("decoder.pid");
        let result=bounded_decoder(decoder_command(Path::new("/bin/sh")).args(["-c","echo $$ > \"$1\"; exec /bin/sleep 10","x"]).arg(&pid_file),&directory,std::time::Duration::from_millis(300));
        assert_eq!(result.unwrap_err().kind(),std::io::ErrorKind::TimedOut);
        let pid:i32=std::fs::read_to_string(&pid_file).unwrap().trim().parse().unwrap();
        // SAFETY: signal 0 only checks that the decoder this test started still exists.
        let alive=||unsafe{libc::kill(pid,0)}==0;
        let by=Instant::now()+std::time::Duration::from_secs(2);
        while alive()&&Instant::now()<by {std::thread::sleep(std::time::Duration::from_millis(10));}
        assert!(!alive(),"the decoder (pid {pid}) outlived its deadline");
        let out=tethered_output(decoder_command(Path::new("/bin/sh")).args(["-c","echo words; exit 3"])).unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(),"words");
        assert_eq!(out.status.code(),Some(3));
        std::fs::remove_dir_all(directory).unwrap();
    }
}

/// **A LAUNCH IS A PROCESS**, so these tests launch processes. Each "launch" re-runs this test
/// binary on one test with its own `HOME`, temporary folder, model folder, speed cache and a
/// stand-in `whisper-cli`, which keeps every variable out of the other tests' environment and
/// lets a quit be a real signal to a real process. The stand-in decodes nothing: it appends its
/// pid to a log and sleeps, so the log is the count of measurement runs and their pids.
#[cfg(test)]
mod launch_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const ROLE: &str = "RICHOS_TEST_LAUNCH_ROLE";

    struct Machine {
        root: PathBuf,
    }

    impl Machine {
        fn new(name: &str) -> Machine {
            let root = std::env::temp_dir().join(format!("richos-launch-{name}-{}", std::process::id()));
            std::fs::remove_dir_all(&root).ok();
            for dir in ["home", "tmp/richos-voice-calibration", "models"] {
                std::fs::create_dir_all(root.join(dir)).unwrap();
            }
            // The probe utterance is already rendered, so no launch runs `say`.
            std::fs::write(root.join("tmp/richos-voice-calibration/probe.wav"), b"RIFF").unwrap();
            // One installed rung, so one walk is exactly one measurement run.
            std::fs::write(root.join("models/ggml-large-v3-turbo-q5_0.bin"), b"weights").unwrap();
            let fake = root.join("whisper-cli");
            std::fs::write(
                &fake,
                "#!/bin/sh\ncase \" $* \" in *\" -m \"*) ;; *) exit 0;; esac\n\
                 echo $$ >> \"$RICHOS_TEST_DECODE_LOG\"\nexec /bin/sleep \"$RICHOS_TEST_DECODE_SECS\"\n",
            )
            .unwrap();
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700)).unwrap();
            Machine { root }
        }

        /// This test binary, re-run on `test` as one launch of the app on this machine.
        fn launch(&self, test: &str, role: &str, decode_secs: &str) -> Command {
            let mut c = Command::new(std::env::current_exe().unwrap());
            c.args([test, "--exact", "--test-threads=1", "--nocapture"])
                .env(ROLE, role)
                .env("HOME", self.root.join("home"))
                .env("TMPDIR", self.root.join("tmp"))
                .env("RICHOS_MODEL_DIR", self.root.join("models"))
                .env("RICHOS_WHISPER_BIN", self.root.join("whisper-cli"))
                .env("RICHOS_WHISPER_SPEED_CACHE", self.root.join("whisper-speed.json"))
                .env("RICHOS_TEST_DECODE_LOG", self.root.join("decodes.log"))
                .env("RICHOS_TEST_DECODE_SECS", decode_secs)
                .env_remove("RICHOS_VOICE_WHISPER_MODEL")
                .env_remove("RICHOS_WHISPER_MODEL")
                .env_remove("RICHOS_VOICE_WHISPER_MODEL_ID")
                .env_remove("RICHOS_HW_TOTAL_MEMORY_BYTES")
                .env_remove("RICHOS_HW_AVAILABLE_MEMORY_BYTES");
            c
        }

        /// The pid of every measurement run started on this machine so far, in order.
        fn decodes(&self) -> Vec<i32> {
            std::fs::read_to_string(self.root.join("decodes.log"))
                .unwrap_or_default()
                .lines()
                .filter_map(|l| l.trim().parse().ok())
                .collect()
        }
    }

    impl Drop for Machine {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.root).ok();
        }
    }

    /// What one launch of the app does: every caller that asks which model this machine uses
    /// (the window's readiness, the phone's voice desk, a lease's video model) asks at once.
    fn act_as_a_launch(callers: usize) {
        let bin = resolve_whisper_bin().expect("the stand-in decoder");
        let start = std::sync::Arc::new(std::sync::Barrier::new(callers));
        let handles: Vec<_> = (0..callers)
            .map(|_| {
                let (bin, start) = (bin.clone(), start.clone());
                std::thread::spawn(move || {
                    start.wait();
                    choose_model(&bin).model_id
                })
            })
            .collect();
        for h in handles {
            assert_eq!(h.join().unwrap(), "large-v3-turbo-q5_0");
        }
    }

    /// INVARIANT (Ray, test55): the speed measurement runs ONCE PER MACHINE. The first launch's
    /// callers ask together and one measurement run answers all of them; the second launch reuses
    /// the saved measurement and starts no measurement run at all.
    #[test]
    fn the_speed_measurement_runs_once_per_machine_and_a_second_launch_reuses_it() {
        if std::env::var(ROLE).as_deref() == Ok("launch") {
            return act_as_a_launch(4);
        }
        let name = "stt::launch_tests::the_speed_measurement_runs_once_per_machine_and_a_second_launch_reuses_it";
        let machine = Machine::new("once");
        let first = machine.launch(name, "launch", "0.3").output().unwrap();
        assert!(first.status.success(), "first launch: {}", String::from_utf8_lossy(&first.stdout));
        let runs = machine.decodes().len();
        assert_eq!(runs, 1, "the first launch's four callers started {runs} measurement runs");
        let second = machine.launch(name, "launch", "0.3").output().unwrap();
        assert!(second.status.success(), "second launch: {}", String::from_utf8_lossy(&second.stdout));
        let runs = machine.decodes().len();
        assert_eq!(runs, 1, "the second launch measured again ({} runs in all) instead of reusing the saved measurement", runs);
    }

    /// CEO §54 addendum 2: the probe utterance a measurement renders is garbage once the
    /// measurement is done, and is removed; one that was already there is left as it was found.
    /// `say` here is a stand-in that writes its `-o` file and plays nothing.
    #[test]
    fn the_probe_a_measurement_renders_is_removed_and_one_already_there_is_kept() {
        if std::env::var(ROLE).as_deref() == Ok("launch") {
            return act_as_a_launch(1);
        }
        let name = "stt::launch_tests::the_probe_a_measurement_renders_is_removed_and_one_already_there_is_kept";
        let probe = |m: &Machine| m.root.join("tmp/richos-voice-calibration/probe.wav");
        let rendering = Machine::new("probe-rendered");
        std::fs::remove_file(probe(&rendering)).unwrap();
        let bin = rendering.root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("say"), "#!/bin/sh\n[ \"$1\" = -o ] && printf RIFF > \"$2\"\nexit 0\n").unwrap();
        std::fs::set_permissions(bin.join("say"), std::fs::Permissions::from_mode(0o700)).unwrap();
        let launch = rendering.launch(name, "launch", "0.1")
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display())).output().unwrap();
        assert!(launch.status.success(), "launch: {}", String::from_utf8_lossy(&launch.stdout));
        assert_eq!(rendering.decodes().len(), 1, "the measurement ran on the rendered probe");
        assert!(!probe(&rendering).exists(), "the probe this measurement rendered was left behind");

        let seeded = Machine::new("probe-seeded");
        let launch = seeded.launch(name, "launch", "0.1").output().unwrap();
        assert!(launch.status.success(), "launch: {}", String::from_utf8_lossy(&launch.stdout));
        assert_eq!(seeded.decodes().len(), 1, "the measurement ran on the probe already there");
        assert!(probe(&seeded).exists(), "a probe this measurement did not render was removed");
    }

    /// INVARIANT: a launch quit in the middle of a measurement does not cost the next launch the
    /// whole measurement again. The stand-in takes 1.05 s, over the 1.000 s ceiling, so the rung
    /// wants three reps; the first launch is quit during its second, and the next launch takes
    /// the two still owed, not three.
    #[test]
    fn a_measurement_cut_short_by_a_quit_is_resumed_not_restarted() {
        if std::env::var(ROLE).as_deref() == Ok("launch") {
            return act_as_a_launch(1);
        }
        let name = "stt::launch_tests::a_measurement_cut_short_by_a_quit_is_resumed_not_restarted";
        let machine = Machine::new("resume");
        let mut app = machine
            .launch(name, "launch", "1.05")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + std::time::Duration::from_secs(20);
        while machine.decodes().len() < 2 && Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // SAFETY: SIGTERM to the child this test spawned and has not yet reaped.
        unsafe { libc::kill(app.id() as i32, libc::SIGTERM) };
        app.wait().unwrap();
        let interrupted = machine.decodes();
        assert_eq!(interrupted.len(), 2, "the first launch was quit during its second rep");
        // Each stand-in logs once, when it starts, so the interrupted rep cannot be counted below.
        let second = machine.launch(name, "launch", "1.05").output().unwrap();
        assert!(second.status.success(), "second launch: {}", String::from_utf8_lossy(&second.stdout));
        let resumed = machine.decodes().len() - interrupted.len();
        assert_eq!(resumed, 2, "the second launch took {resumed} reps; one was kept, so two were owed");
    }

    /// INVARIANT (Ray, test55): no `whisper-cli` outlives the app that started it. The app is
    /// quit the way the test VM's relaunch quits it (SIGTERM, which runs none of the app's own
    /// exit code) in the middle of a measurement run, and the run must end with it.
    #[test]
    fn a_quit_leaves_no_whisper_cli() {
        if std::env::var(ROLE).as_deref() == Ok("quit") {
            return act_as_a_launch(1);
        }
        let name = "stt::launch_tests::a_quit_leaves_no_whisper_cli";
        let machine = Machine::new("quit");
        let mut app = machine
            .launch(name, "quit", "30")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + std::time::Duration::from_secs(20);
        while machine.decodes().is_empty() && Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let decoder = *machine.decodes().first().expect("the app started its measurement run");
        // SAFETY: SIGTERM to the child this test spawned and has not yet reaped.
        unsafe { libc::kill(app.id() as i32, libc::SIGTERM) };
        app.wait().unwrap();
        // SAFETY: signal 0 only checks that the stand-in this test's app started still exists.
        let alive = |pid: i32| unsafe { libc::kill(pid, 0) } == 0;
        let gone_by = Instant::now() + std::time::Duration::from_secs(3);
        while alive(decoder) && Instant::now() < gone_by {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let survived = alive(decoder);
        if survived {
            // Never leave the stand-in running past the test, whatever the verdict.
            // SAFETY: the pid this test's own stand-in wrote, checked alive just above.
            unsafe { libc::kill(decoder, libc::SIGKILL) };
        }
        assert!(!survived, "whisper-cli (pid {decoder}) kept running after the app that started it quit");
    }
}

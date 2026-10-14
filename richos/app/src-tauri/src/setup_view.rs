//! FIRST RUN, THE SURFACE — one consent step, then progress, then done.
//!
//! `richos_core::setup` decides and does; this file is what the window talks to. It holds no
//! decisions of its own, for the reason `memory.rs` holds none: two places that both know
//! where an engine may live is how an install succeeds and a boot then fails to find it.
//!
//! # What the CEO sees, and what he is spared
//!
//! **No terminal. No path. No version number.** One sheet naming what is about to be
//! installed and why, a button, progress while it runs, and a sentence at the end. The
//! consent copy is [`richos_core::setup::Component::why`], and a test asserts it carries no
//! slash, no dollar sign, no digit and no mention of Terminal.
//!
//! **Three sentences are his, and they and the counter are exempt from that floor.** The account
//! sentence ([`SETUP_ACCOUNT_NOTE`], "Max 20x", "Max/Max 20x") is his as of 2026-10-08. Once
//! dictation is there (`richos_core::dictation_ready`), the video tools' line is the CEO's own
//! setup sentence ([`richos_core::setup::MEDIA_TOOLS_WHY`], "private/local", "$140+/year") and
//! the download carries his line ([`SETUP_DOWNLOAD_LINE`], "$140+/year"), with the counter ("490
//! MB of 1.06 GB", [`counter_line`]) beside it. `ui/tests/setup.js` case 2 removes exactly those
//! three before it checks, compared against these constants so a changed word is not exempt, and
//! holds every other word on the sheet to the full floor.
//!
//! **And it does not imply zero-touch.** RichOS is BYO-Anthropic: `open-items.md` row 3.14
//! lists it as the second of the three things to settle — *"D removes one setup step of two,
//! not all of them — RichOS is BYO-Anthropic, so the customer still needs an account and a
//! login, and D must not be sold to him as zero-touch."* [`SETUP_ACCOUNT_NOTE`] is that
//! sentence in his language, on the same sheet, before he presses anything.
//!
//! # Progress, and why it is an event rather than a return value
//!
//! Anthropic's installer downloads the `claude` binary — 197,220,928 B on a Mac with no
//! `zstd`, which macOS 15.6 does not ship (§19, finding 3). A command that returns only when
//! that finishes is a window that looks hung for minutes. Each step emits [`EVENT_SETUP`] as
//! it starts and as it ends, so the surface can say what is happening while it happens.
//!
//! # The failure contract
//!
//! Every failure is a named `SetupError`, its `Display` is written for the CEO, and the
//! `finished` event carries `machine_unchanged` so the surface can say whether anything on his
//! Mac was touched. **A step that fails stops the run**: installing an engine for a Claude that
//! is not there produces a half-set-up machine that reports two successes and works for
//! nothing, which is the failure mode this whole module exists to avoid.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use richos_core::setup::{
    self, BashRunner, Component, CurlFetcher, SetupError, SetupPaths, SetupStatus, TarExtractor,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// The channel the window listens on while setup runs.
pub const EVENT_SETUP: &str = "richos://setup";

/// **The sentence that keeps this honest.** He still needs his own Anthropic account and a
/// completed login through the provider-owned browser flow. Shown on the consent sheet,
/// before the button, not in a footnote afterwards.
pub const SETUP_ACCOUNT_NOTE: &str =
    "You need your own Anthropic account and a Max subscription there. You can sign in through your browser after setup; I never see your password. If you don't already have that subscription, sign up there first and pick the Max/Max 20x tier.";

/// **His line while the speech models download**, as Rich says it on the sheet (round 19, state
/// 2; the CEO wrote it on 2026-10-07 and approved round 19 on 2026-10-08). Word for word as round
/// 19 draws it, "about 1 GB" and the thumbs-up included: Iris changed his "a few hundred MB" to
/// "about 1 GB" because setup fetches both speech models, 1,061,655,396 B together
/// (`model-pins.json`), and he approved that round.
///
/// **Shown only while BOTH speech models are being fetched** (dictation plan revision 2, minor
/// 10): with one missing the download is 487,614,201 B or 574,041,195 B and "about 1 GB" is
/// untrue, so the rows and the counter show without it ([`DownloadTally::both_models`]). And
/// only once dictation is there (`richos_core::dictation_ready`), because it promises Wispr Flow
/// is no longer needed.
pub const SETUP_DOWNLOAD_LINE: &str = "Sit tight, we need to download about 1 GB of local voice AI so that you can just talk to Rich instead of typing. This will save you $140+/year👍 because you won't need Wispr Flow with this setup.";

/// The voice row while the downloaded files are hashed against their pins (round 19, state 2).
pub const COUNTER_CHECKING: &str = "Checking\u{2026}";

/// **What the voice and video tools row counts**: everything this Mac is actually fetching for
/// them, summed (dictation plan revision 2, section 2 row 2), so the row says "490 MB of 1.06 GB"
/// for the whole download rather than a percent of one model.
///
/// The parts are the speech models still missing, in the order they are fetched (the one voice
/// hears with, then the transcription model), each at its pinned size, and yt-dlp's bytes when
/// it was fetched in this run (about 3 MB; already received by the time a model starts, because
/// it comes first). A model's own event says how much of IT is on disk (`received`, a resumed
/// partial included), so the bytes in hand are yt-dlp, every model before it in the order, and
/// its own.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DownloadTally {
    models: Vec<(String, u64)>,
    tools_bytes: u64,
}

impl DownloadTally {
    /// `models`: the missing models in fetch order with their pinned bytes; a model named twice
    /// is counted once. `tools_bytes`: yt-dlp's size when it was fetched now, else 0.
    pub fn new(models: Vec<(String, u64)>, tools_bytes: u64) -> Self {
        let mut unique: Vec<(String, u64)> = Vec::new();
        for (id, bytes) in models {
            if !unique.iter().any(|(seen, _)| *seen == id) {
                unique.push((id, bytes));
            }
        }
        DownloadTally { models: unique, tools_bytes }
    }

    /// Every byte this download brings.
    pub fn total(&self) -> u64 {
        self.tools_bytes + self.models.iter().map(|(_, b)| b).sum::<u64>()
    }

    /// The bytes in hand when `model_id` has `received` of its own. A model this tally does not
    /// name (the voice panel's own download) counts nothing of its own.
    pub fn received(&self, model_id: &str, received: u64) -> u64 {
        let mut before = self.tools_bytes;
        for (id, bytes) in &self.models {
            if id == model_id {
                return before + received.min(*bytes);
            }
            before += bytes;
        }
        self.tools_bytes
    }

    /// Whether this download fetches both speech models: the one case [`SETUP_DOWNLOAD_LINE`]'s
    /// "about 1 GB" is true of.
    pub fn both_models(&self) -> bool {
        self.models.len() >= 2
    }
}

/// The voice row's counter, as round 19 draws it: "490 MB of 1.06 GB". Decimal units (a
/// megabyte is 1,000,000 B, a gigabyte 1,000,000,000 B), the bytes in hand in whole megabytes,
/// the whole in gigabytes to two places from 1 GB up and in whole megabytes below it (one model
/// alone is "of 574 MB").
pub fn counter_line(received: u64, total: u64) -> String {
    let megabytes = |b: u64| format!("{} MB", (b + 500_000) / 1_000_000);
    let whole = if total >= 1_000_000_000 {
        format!("{:.2} GB", total as f64 / 1_000_000_000.0)
    } else {
        megabytes(total)
    };
    format!("{} of {whole}", megabytes(received.min(total)))
}

/// The sentence a build with no engine pin shows INSTEAD of a button.
///
/// It exists as a `const` rather than only as `SetupError::EngineUnpinned`'s `Display`
/// because `affordances.js` scrapes the product's CEO-facing sentences out of the source and
/// classifies every one of them, and a `#[error(...)]` attribute is not a place it can see.
/// A sentence the state registry cannot see is a sentence nobody has said whether the CEO can
/// act on. The test below requires the two to be the same string, so there is one wording and
/// not two.
pub const SETUP_UNPINNED_NOTE: &str =
    "This copy of RichOS wasn't built with an engine to install, so I can't fetch one. \
     It needs whoever set RichOS up to publish one and pin it.";

/// **What the CEO is told when he tries to send and the setting up was never done.**
///
/// THE DEFECT THIS CLOSES (ray-opus-a2, published v1.0.1, 2026-09-04). Four sends, four
/// refusals, no restart in between, and every one of them said: *"Quit RichOS and open it
/// again — that clears it most of the time."* On that machine
/// `~/Library/Application Support/RichOS/engine` held zero files, so quitting and reopening
/// was advice that could not work: the boot would look for an engine that was still not
/// there, fail to attach exactly as before, and hand him the same sentence. He would have
/// done it, watched it not work, and concluded the product was broken — which, in that
/// state, it was.
///
/// [`crate::LEASE_UNAVAILABLE_MESSAGE`] is not wrong; it is written for the OTHER no-lease
/// cause, the one where everything is installed and the account is not signed in. Quitting
/// and reopening genuinely does clear that one. The two causes were sharing a sentence, and
/// the sentence was true of only one of them.
///
/// So this is the arm where the cause is on disk in front of us. It says which piece is
/// missing, in the same words the consent sheet uses ([`Component::display_name`]), it
/// offers the one thing that fixes it — the sheet is reopened behind this notice, and
/// `run_setup` refreshes the lease factory with no relaunch — and it closes the door on the advice that
/// cannot help: *there's nothing to quit and nothing to reopen*.
///
/// THREE ARMS AND THREE WHOLE SENTENCES, not one template with a hole in it. `affordances.js`
/// scrapes CEO-facing literals out of this source and classifies every one; a `{}` is a
/// sentence nobody can classify, and the state registry would be carrying a fragment instead
/// of the thing he reads.
pub const SETUP_INCOMPLETE_ENGINE: &str =
    "I can't take that on yet: the RichOS engine isn't on this Mac, and that's the part \
     of me that knows how I work. I've put the setting up back on your screen: press Set \
     it up and I'll fetch it. There's nothing to quit and nothing to reopen.";

/// The same state, when what is missing is the program rather than the instructions.
pub const SETUP_INCOMPLETE_CLAUDE: &str =
    "I can't take that on yet: Claude Code isn't on this Mac, and that's the program I \
     think with. I've put the setting up back on your screen: press Set it up and I'll \
     fetch it. There's nothing to quit and nothing to reopen.";

/// The same state on a Mac that has neither — the customer's, on the day he installs.
pub const SETUP_INCOMPLETE_BOTH: &str =
    "I can't take that on yet: this Mac doesn't have Claude Code or the RichOS engine, \
     and those are what I think with. I've put the setting up back on your screen: press \
     Set it up and I'll fetch them. There's nothing to quit and nothing to reopen.";

/// Which sentence he is shown, from what the DISK says — not from what a boot once thought.
///
/// An empty `needs` has no sentence here BY CONSTRUCTION: nothing is missing, so the no-lease
/// cause is the other one and [`crate::LEASE_UNAVAILABLE_MESSAGE`] is the honest answer. The
/// `Option` is what makes that a decision the caller has to make rather than a fallthrough.
///
/// A BLOCKED BUILD GETS THE SENTENCE THAT IS TRUE OF IT. Three of these four arms promise
/// "press Set it up", and on a build with no engine pin there is no such button to press —
/// `ask_for_with` hides it and shows [`SETUP_UNPINNED_NOTE`] instead. Promising a control that is
/// not drawn is the same class of defect as promising a restart that cannot work, so that
/// arm gets the note the sheet itself is showing, which names the party who can act.
pub fn incomplete_message(status: &SetupStatus) -> Option<&'static str> {
    let needs = status.needs();
    if needs.is_empty() {
        return None;
    }
    if status.blocked() {
        return Some(SETUP_UNPINNED_NOTE);
    }
    // THE VIDEO TOOLS ARE NOT A CAUSE OF "NO LEASE", SO THEY GET NO SENTENCE HERE. This function
    // answers why a send found no lease, and a lease needs Claude Code and the engine only
    // (`EngineLeaseFactory`); the sheet itself lists the video tools whenever they are missing
    // (`ask_for_with`). With only they missing, the no-lease cause is the other one — the account —
    // and naming the video tools would send him to the wrong fix.
    match (
        needs.contains(&Component::ClaudeCode),
        needs.contains(&Component::Engine),
    ) {
        (true, true) => Some(SETUP_INCOMPLETE_BOTH),
        (true, false) => Some(SETUP_INCOMPLETE_CLAUDE),
        (false, true) => Some(SETUP_INCOMPLETE_ENGINE),
        (false, false) => None,
    }
}

/// One step of the run, as it happens.
#[derive(Debug, Clone, Serialize)]
pub struct SetupProgress {
    /// `started`, `done`, `failed`, or `finished` for the whole run.
    pub state: &'static str,
    /// Which component this is about, when it is about one.
    pub component: Option<&'static str>,
    /// What the CEO reads for this line — his language, no path.
    pub what: String,
    /// 1-based position, and how many steps in total.
    pub index: usize,
    pub total: usize,
    /// The failure's own sentence, when `state` is `failed`.
    pub detail: Option<String>,
    /// The machine tag, for the operator's log and the tests. Never rendered.
    pub kind: Option<&'static str>,
    /// Whether his Mac is unchanged. Only meaningful with `state == "failed"`.
    pub machine_unchanged: Option<bool>,
    /// **The voice and video tools' download, summed** ([`DownloadTally`]): the bytes in hand
    /// and the whole, on the model download's own progress lines only. `None` on every other
    /// line.
    pub received: Option<u64>,
    pub of: Option<u64>,
    /// The voice row's words for it: [`counter_line`], or [`COUNTER_CHECKING`] while the file is
    /// hashed against its pin.
    pub counter: Option<String>,
    /// Whether this download fetches both speech models, the one case his download line is
    /// shown in ([`SETUP_DOWNLOAD_LINE`]).
    pub both_models: Option<bool>,
}

impl SetupProgress {
    /// A line with no download counter, which is every line but the model download's progress.
    fn line(
        state: &'static str,
        component: Option<&'static str>,
        what: String,
        index: usize,
        total: usize,
    ) -> Self {
        SetupProgress {
            state,
            component,
            what,
            index,
            total,
            detail: None,
            kind: None,
            machine_unchanged: None,
            received: None,
            of: None,
            counter: None,
            both_models: None,
        }
    }
}

fn emit(app: &AppHandle, p: SetupProgress) {
    emit_from(app, p, false)
}

/// `background` heads the operator's line with "video tools in the background" rather than
/// "setup", so the launch download's lines are never read as a step of a press.
fn emit_from(app: &AppHandle, p: SetupProgress, background: bool) {
    let from = if background { "video tools in the background" } else { "setup" };
    // The operator's line first, so a run is legible in a log with no window attached.
    match (p.state, &p.detail) {
        ("failed", Some(d)) => eprintln!("[richos] {from} {}/{} FAILED — {d}", p.index, p.total),
        (state, _) => eprintln!("[richos] {from} {}/{} {state}: {}", p.index, p.total, p.what),
    }
    let _ = app.emit(EVENT_SETUP, p);
}

// =============================================================================================
// THE VIDEO TOOLS DOWNLOAD BY THEMSELVES, FROM LAUNCH (the CEO, 2026-10-07)
// =============================================================================================
//
// *"the download of the essential files starts right away and independently of the other stuff.
// So that by the time the user is done with their intial setup all those essential files will
// already have downloaded or nearly finished downloading in the background."* And: *"The speech
// models and video tools begin downloading in the background at first launch, while the user does
// the rest of setup."*
//
// Until this change they were the LAST step of a press of "Set it up": the sheet hid both of its
// buttons for the whole run, so the memory question, the company question and the first
// conversation all waited for 1,061,655,396 B of models (small.en 487,614,201 + large-v3-turbo-q5_0
// 574,041,195, `model-pins.json`).
//
// Now one background thread fetches them, started by the boot the moment it finds them missing
// (`main.rs`, the first-run setup block), and by a press only when it is not already running.
// While it runs the video tools are not on the sheet and not part of any press: nothing in first
// setup waits for them. Its lines are the step's own (`started_line`, the done sentence), on the
// same `richos://setup` channel, and every model event also goes to `rich://voice-model`, so the
// voice panel shows the same download and voice switches on when a model arrives.

/// Whether the background download is running now.
static VIDEO_TOOLS_DOWNLOADING: AtomicBool = AtomicBool::new(false);

/// Is the video tools' background download running right now?
pub fn video_tools_downloading() -> bool {
    VIDEO_TOOLS_DOWNLOADING.load(Ordering::SeqCst)
}

/// **What the sheet asks for, and what a press installs**: everything missing, except the video
/// tools while their background download runs. A pure function of the status and that one fact,
/// so the rule is a unit test.
pub fn sheet_needs(status: &SetupStatus, downloading: bool) -> Vec<Component> {
    status
        .needs()
        .into_iter()
        .filter(|c| !(downloading && *c == Component::MediaTools))
        .collect()
}

/// The steps a press of "Set it up" runs and waits for, in order. **Never the video tools**: a
/// press that finds them missing starts their background download instead ([`run`]).
pub fn foreground_steps(needs: &[Component]) -> Vec<Component> {
    needs.iter().copied().filter(|c| *c != Component::MediaTools).collect()
}

/// Clears the running flag however the download thread ends, a panic included.
struct Downloading;
impl Drop for Downloading {
    fn drop(&mut self) {
        VIDEO_TOOLS_DOWNLOADING.store(false, Ordering::SeqCst);
    }
}

/// **Start the video tools' download in the background, now.** `false` when it was already
/// running (one at a time) or the thread could not start. Returns at once: nothing waits for it.
pub fn start_video_tools_download(app: &AppHandle) -> bool {
    if VIDEO_TOOLS_DOWNLOADING.swap(true, Ordering::SeqCst) {
        return false;
    }
    let app = app.clone();
    let spawned = std::thread::Builder::new().name("video-tools-download".into()).spawn(move || {
        let running = Downloading;
        let step = |state, what: String| {
            SetupProgress::line(state, Some(Component::MediaTools.as_str()), what, 1, 1)
        };
        emit_from(&app, step("started", started_line(Component::MediaTools)), true);
        let home = SetupPaths::from_process().home;
        let outcome = install_media_tools(&app, home.as_deref(), 1, 1);
        // The flag clears BEFORE the last line, so a window that re-reads the status on it reads
        // the finished state rather than "still downloading".
        drop(running);
        match outcome {
            Ok(what) => emit_from(&app, step("done", what), true),
            Err(e) => emit_from(&app, failure(Component::MediaTools, 1, 1, &e), true),
        }
    });
    match spawned {
        Ok(_) => {
            eprintln!("[richos] video tools: downloading in the background");
            true
        }
        Err(e) => {
            VIDEO_TOOLS_DOWNLOADING.store(false, Ordering::SeqCst);
            eprintln!("[richos] video tools: the background download could not start: {e}");
            false
        }
    }
}

/// Read this machine, using the SAME engine answer the boot used.
///
/// `boot_engine` is `main.rs::resolve_engine`'s result when it found a real one. Passing it in
/// rather than re-deriving it is the whole point: the repo-ancestor candidates (`engine.rs` 4
/// and 5) are a dogfood layout that `setup.rs` deliberately does not reimplement, and a
/// developer running from the checkout must never be asked to install something already three
/// directories away.
///
/// # TWO THINGS THIS FUNCTION USED TO DO, AND WHY IT STOPPED — the nightly's D1
///
/// **It promoted the boot's engine to `engine_override`.** That field is documented as
/// "`$RICHOS_ENGINE_DIR` / `$RICHOS_ENGINE_ROOT` — either explicit engine statement", and
/// `find_engine` treats an explicit statement as EXCLUSIVE: first candidate, no fall-through.
/// So a boot that resolved a broken engine made the walk stop on it, and the candidate list
/// reported one entry tagged `($RICHOS_ENGINE_DIR)` for a variable the operator had never
/// set. The boot engine is a PREFERENCE, not a statement, and `extra` — which already carried
/// it, two lines above — is exactly the right strength: first among the searched candidates,
/// still able to be passed over when it cannot be used.
///
/// **It re-verified afterwards and flipped `present` to `false` on failure.** That is a veto
/// applied after selection, and it produced the audit's boot line: `ComponentStatus::found`
/// clears `looked_in`, so a component vetoed here reported NOWHERE looked. Verification is now
/// part of selection inside `setup::find_engine` — one decision, one place, and a candidate
/// that is rejected is recorded with the reason it was rejected.
pub fn detect(boot_engine: Option<&std::path::Path>) -> SetupStatus {
    let extra: Vec<PathBuf> = boot_engine
        .filter(|d| setup::engine_looks_valid(d))
        .map(|d| vec![d.to_path_buf()])
        .unwrap_or_default();
    let paths = SetupPaths::from_process();
    setup::detect(&paths, &extra, &setup::engine_is_usable, &speech_model)
}

/// **The speech-model half of the video tools: BOTH models** (media-tools plan §2; the CEO,
/// 2026-10-07, "Both, in this nightly, yes"). Present exactly when
///
/// 1. voice has a model: a live-ladder model is installed and verified
///    (`stt::voice_model_installed`), which is exactly when `stt::readiness()` is `Ready`,
///    answered without the decode timing `readiness` runs to choose among them; AND
/// 2. the transcription model (`stt::TRANSCRIPTION_MODEL_ID`) is installed and verified
///    (`stt::model_verified`).
///
/// The `Err` names the half that is missing, for the operator's boot line.
fn speech_model() -> Result<String, String> {
    use richos_voice::stt;
    // NOT `readiness()`: that calibrates by timing decodes, and this runs on the boot path (see
    // `stt::voice_model_installed` for the measurement). The operator's boot-line wording, never
    // shown to the CEO (`looked_in` is the boot log's).
    let voice = stt::voice_model_installed().map_err(|why| {
        let operator_line = format!("for voice, {why}");
        operator_line
    });
    let transcription = stt::model_verified(stt::TRANSCRIPTION_MODEL_ID)
        .map(|_| stt::TRANSCRIPTION_MODEL_ID.to_string())
        .map_err(|why| format!("for transcription, {why}"));
    match (voice, transcription) {
        (Ok(v), Ok(t)) => Ok(format!("{v} for voice and {t} for transcription")),
        (Err(a), Err(b)) => Err(format!("{a}; {b}")),
        (Err(a), _) | (_, Err(a)) => Err(a),
    }
}

/// What the consent sheet says, assembled from the status. Returned to the window so the copy
/// lives in one place and the window renders rather than composes.
#[derive(Debug, Clone, Serialize)]
pub struct SetupAsk {
    /// One line per missing component: what it is, and what it is for.
    pub items: Vec<SetupAskItem>,
    /// [`SETUP_ACCOUNT_NOTE`].
    pub account_note: &'static str,
    /// `true` when everything missing can actually be installed by this build. `false` puts
    /// the sheet in its explain-rather-than-offer state.
    pub can_install: bool,
    /// Why not, when `can_install` is false — the CEO-facing sentence, not a code.
    pub cannot_install_reason: Option<String>,
    /// **Dictation is there** (`richos_core::dictation_ready`): the window draws round 19's
    /// sheet (states 1 to 3: the setup, the progress rows with the counter, "You're all set."
    /// with Start) and Rich's offer after it (state 4). `false` draws the sheet as it was.
    pub dictation: bool,
    /// [`SETUP_DOWNLOAD_LINE`] when `dictation`, else `None`. The window shows it only while a
    /// download line says both speech models are being fetched (`both_models`).
    pub download_line: Option<&'static str>,
}

/// One row of the consent sheet.
#[derive(Debug, Clone, Serialize)]
pub struct SetupAskItem {
    pub component: &'static str,
    pub name: &'static str,
    pub why: &'static str,
}

/// **The one shape both commands return**, so the window never has to know which command it
/// came from. `complete` is here rather than on `SetupStatus` because it is a derived answer
/// (`SetupStatus::needs().is_empty()`), and a serialized field that duplicates a method is a
/// second place for the answer to be wrong.
pub fn view(status: &SetupStatus) -> serde_json::Value {
    view_with(status, video_tools_downloading())
}

/// [`view`] for a stated download state. **Complete when nothing the sheet would ask for is
/// missing**: video tools still downloading in the background hold nothing up (their own
/// progress and done lines say how far they are).
pub fn view_with(status: &SetupStatus, downloading: bool) -> serde_json::Value {
    serde_json::json!({
        "status": status,
        "ask": ask_for_with(status, downloading),
        "complete": sheet_needs(status, downloading).is_empty(),
        // So a sheet that opens mid-download draws the voice row as counting at once.
        "downloading": downloading,
    })
}

/// Turn a status into the sheet's contents, for a stated download state: the video tools are not
/// on the sheet while their background download runs, until dictation is there.
pub fn ask_for_with(status: &SetupStatus, downloading: bool) -> SetupAsk {
    ask_for_gated(status, downloading, richos_core::dictation_ready())
}

/// [`ask_for_with`] with the dictation gate stated.
///
/// **ONCE DICTATION IS THERE, THE SHEET LISTS THE VIDEO TOOLS EVEN WHILE THEY DOWNLOAD**
/// (dictation plan revision 2, section 2 row 1): the sheet is consent copy, and he is told what is
/// being installed. Only on a sheet that asks for something else: with Claude Code and the engine
/// in, nothing opens a sheet for a download already running, exactly as before. Listing them
/// changes nothing a press waits for: `can_install` and `complete` still come from
/// [`sheet_needs`], and a press never waits for them ([`foreground_steps`]).
pub fn ask_for_gated(status: &SetupStatus, downloading: bool, dictation: bool) -> SetupAsk {
    let needs = sheet_needs(status, downloading);
    let listed = if dictation && !needs.is_empty() { status.needs() } else { needs.clone() };
    let items = listed
        .iter()
        .map(|c| SetupAskItem {
            component: c.as_str(),
            name: c.display_name_for(dictation),
            why: c.why_for(dictation),
        })
        .collect();
    let blocked = status.blocked();
    SetupAsk {
        items,
        account_note: SETUP_ACCOUNT_NOTE,
        can_install: !needs.is_empty() && !blocked,
        cannot_install_reason: blocked.then(|| SETUP_UNPINNED_NOTE.to_string()),
        dictation,
        download_line: dictation.then_some(SETUP_DOWNLOAD_LINE),
    }
}

/// **HE PRESSES THE BUTTON.** Install everything missing, in order, reporting each step.
///
/// `engine_dir` is the shared cell the lease factory reads (`main.rs::EngineLeaseFactory`).
/// A successful engine install rewrites it, so the next lease — a rotation, a recovery, or the
/// first accepted request — starts `claude` in the engine that was just installed,
/// **without a relaunch**. That property is not a nicety: `provision_memory` already set it as
/// the standard, after a customer's first five minutes were spent with a corpus he had just
/// created and a desk that would not open until he quit.
pub fn run(
    app: &AppHandle,
    boot_engine: Option<&std::path::Path>,
    engine_dir: &Arc<Mutex<PathBuf>>,
) -> Result<SetupStatus, String> {
    let status = detect(boot_engine);
    let wanted = sheet_needs(&status, video_tools_downloading());
    // THE VIDEO TOOLS ARE NEVER A STEP THIS PRESS WAITS FOR. Missing and not downloading (the
    // launch's download failed, or never started): their background download starts now,
    // FIRST, so it runs beside Claude Code's installer rather than after it.
    if wanted.contains(&Component::MediaTools) {
        start_video_tools_download(app);
    }
    let needs = foreground_steps(&wanted);
    if needs.is_empty() {
        // Nothing to do is a success with no steps, not an error and not a no-op that looks
        // like one: the window asked because the status said to, and the status can have
        // changed under it (he installed Claude Code himself in another window).
        let mut done = status;
        done.installed_now = true;
        return Ok(done);
    }
    let total = needs.len();
    let paths = SetupPaths::from_process();

    for (i, component) in needs.iter().enumerate() {
        let index = i + 1;
        emit(
            app,
            SetupProgress::line("started", Some(component.as_str()), started_line(*component), index, total),
        );

        let outcome: Result<String, SetupError> = match component {
            Component::ClaudeCode => setup::install_claude_code(&CurlFetcher, &BashRunner, &paths)
                .map(|r| {
                    // The operator's line names the binary and its signature verdict, and
                    // NOTHING about the account. RichOS may never collect, store or
                    // intermediate Claude credentials, and a log line is storage.
                    eprintln!(
                        "[richos] setup: Claude Code at {} — signature {} ({})",
                        r.installed_at,
                        if r.signature.trusted { "VERIFIED" } else { "REJECTED" },
                        r.signature.checked
                    );
                    format!("Claude Code is installed. ({} bytes of installer)", r.installer_bytes)
                }),
            Component::Engine => match setup::engine_pin() {
                None => Err(SetupError::EngineUnpinned),
                Some(pin) => {
                    let home = match paths.home.as_deref() {
                        Some(h) => h.to_path_buf(),
                        None => {
                            let e = SetupError::NoHome;
                            emit_failure(app, *component, index, total, &e);
                            return Err(e.to_string());
                        }
                    };
                    let dest = setup::engine_install_dir(&home);
                    setup::install_engine(&CurlFetcher, &TarExtractor, &pin, &dest).map(|r| {
                        eprintln!(
                            "[richos] setup: engine {} at {} — sha256 {}, {} bytes",
                            r.version, r.installed_at, r.sha256, r.bytes
                        );
                        // THE LEASE FACTORY NOW POINTS AT IT. Without this line the app would
                        // have an engine on disk and keep starting `claude` in the directory
                        // it failed to find at boot.
                        if let Ok(mut cell) = engine_dir.lock() {
                            *cell = dest.clone();
                        }
                        // AND VOICE FINDS ITS DECODER IN IT: the engine's runtime carries
                        // `whisper-cli` (build-runtimes.py), verified here like every runtime file.
                        if let Ok(runtime) = richos_core::runtime::verify_engine(&dest) {
                            richos_voice::stt::set_delivered_runtime_bin(Some(runtime.root.join("bin")));
                        }
                        "Your engine is installed.".to_string()
                    })
                }
            },
            // Filtered out by `foreground_steps`: started in the background above, never waited for.
            Component::MediaTools => continue,
        };

        match outcome {
            Ok(what) => emit(app, SetupProgress::line("done", Some(component.as_str()), what, index, total)),
            Err(e) => {
                emit_failure(app, *component, index, total, &e);
                // STOP. A half-set-up machine that reports two successes and works for
                // nothing is exactly what this module exists to prevent.
                return Err(e.to_string());
            }
        }
    }

    // RE-READ FROM DISK. Not "every step returned Ok, therefore it is all there" — the same
    // rule `install_claude_code` applies to Anthropic's own exit code.
    let selected = engine_dir.lock().map_err(|_| "The selected engine could not be read.")?.clone();
    let mut after = detect(Some(&selected));
    after.installed_now = true;
    emit(
        app,
        SetupProgress {
            machine_unchanged: Some(true),
            ..SetupProgress::line(
                "finished",
                None,
                if sheet_needs(&after, video_tools_downloading()).is_empty() {
                    "The software is installed. Connect your account to start working.".to_string()
                } else {
                    // Reached only if something removed a component between the install and this
                    // line. It is still not reported as a success.
                    "Something is still missing.".to_string()
                },
                total,
                total,
            )
        },
    );
    Ok(after)
}

fn emit_failure(
    app: &AppHandle,
    component: Component,
    index: usize,
    total: usize,
    e: &SetupError,
) {
    emit(app, failure(component, index, total, e));
}

fn failure(component: Component, index: usize, total: usize, e: &SetupError) -> SetupProgress {
    SetupProgress {
        detail: Some(e.to_string()),
        kind: Some(e.kind()),
        machine_unchanged: Some(e.machine_unchanged()),
        ..SetupProgress::line(
            "failed",
            Some(component.as_str()),
            format!("{} could not be installed.", component.display_name()),
            index,
            total,
        )
    }
}

/// The progress line, in his language. Present tense, no path, no byte count — the byte count
/// belongs on the operator's line, where it is useful.
fn started_line(c: Component) -> String {
    match c {
        Component::ClaudeCode => {
            "Getting Claude Code from Anthropic. This is the big one: a few minutes."
                .to_string()
        }
        Component::Engine => "Getting my instructions.".to_string(),
        Component::MediaTools => format!("Getting {}.", c.display_name()),
    }
}

/// **THE VIDEO TOOLS, INSTALLED** (media-tools plan §2, slice 3), by the background download
/// ([`start_video_tools_download`]) and nothing else: yt-dlp through slice 2's verified install,
/// then BOTH speech models through the voice panel's own verified, resumable fetch — the model
/// voice will hear with, then the transcription model (`stt::TRANSCRIPTION_MODEL_ID`; the CEO,
/// 2026-10-07: "Both, in this nightly, yes"). Each part is skipped when it is already there, so the
/// CEO's Mac — which has both models in `~/Models/Whisper` — fetches only the 3 MB yt-dlp, and a
/// download that failed resumes rather than starting over (`provision.rs` resume rules).
///
/// **NO DECODER NEEDED, because it starts at launch.** On a fresh Mac `whisper-cli` arrives with
/// the engine, which is installed only once he presses "Set it up". So this never asks
/// `stt::readiness` which model to fetch (that needs the decoder and times decodes); it fetches the
/// pinned files by name, and `provision` checks every one against its pinned sha256 before it
/// becomes a model. Which model voice gets without a decoder is [`voice_model_to_fetch`].
fn install_media_tools(
    app: &AppHandle,
    home: Option<&std::path::Path>,
    index: usize,
    total: usize,
) -> Result<String, SetupError> {
    use richos_core::media_tools;
    use richos_voice::stt;

    let home = home.ok_or(SetupError::NoHome)?;
    let tools = media_tools::tools_dir(home);
    // yt-dlp's bytes when it is fetched in this run, for the voice row's counter.
    let mut tools_bytes = 0;
    match media_tools::installed(&tools) {
        Some(i) => eprintln!("[richos] setup: yt-dlp nightly {} already installed", i.tag),
        None => match media_tools::refresh_with_curl(&tools)? {
            media_tools::Refreshed::Installed { tag, sha256, .. } => {
                eprintln!("[richos] setup: yt-dlp nightly {tag} installed — sha256 {sha256}");
                tools_bytes = media_tools::installed(&tools)
                    .and_then(|i| std::fs::metadata(&i.file).ok())
                    .map(|m| m.len())
                    .unwrap_or(0);
            }
            media_tools::Refreshed::Unchanged { tag } => {
                eprintln!("[richos] setup: yt-dlp nightly {tag} already the newest")
            }
        },
    }

    // WHAT THIS MAC FETCHES, decided before the first byte, so the counter's whole is right from
    // its first line: the models missing now, in the order they are fetched below.
    let voice_missing = stt::voice_model_installed().map_err(|why| (voice_model_to_fetch(), why));
    let transcription_missing = stt::model_verified(stt::TRANSCRIPTION_MODEL_ID).err();
    let mut fetching = Vec::new();
    if let Err((id, _)) = &voice_missing {
        fetching.push(id.clone());
    }
    if transcription_missing.is_some() {
        fetching.push(stt::TRANSCRIPTION_MODEL_ID.to_string());
    }
    let tally = DownloadTally::new(
        fetching
            .into_iter()
            .map(|id| {
                let bytes = richos_voice::provision::pin_for(&id).map(|p| p.bytes).unwrap_or(0);
                (id, bytes)
            })
            .collect(),
        tools_bytes,
    );
    eprintln!(
        "[richos] setup: the voice and video tools download brings {} bytes ({})",
        tally.total(),
        if tally.both_models() { "both speech models" } else { "not both speech models" }
    );

    let observer = SetupModelObserver { app: app.clone(), index, total, tally };
    let state = crate::ensure_model_fetch_state(app);

    // 1. THE MODEL VOICE HEARS WITH, first: with only the large model installed, voice would take
    //    it whatever its speed (the ladder always accepts its last installed rung).
    match voice_missing {
        Ok(id) => eprintln!("[richos] setup: voice speech model {id} already installed and verified"),
        Err((id, why)) => {
            eprintln!("[richos] setup: voice speech model missing ({why}); fetching {id}");
            fetch_verified(&observer, &state, &id)?;
        }
    }

    // 2. THE TRANSCRIPTION MODEL, whatever voice chose (the CEO, 2026-10-07: "Both"). Asked again
    //    rather than taken from above: voice's fetch may have been this very model.
    let id = stt::TRANSCRIPTION_MODEL_ID;
    match stt::model_verified(id) {
        Ok(path) => eprintln!("[richos] setup: transcription model {id} already installed and verified at {}", path.display()),
        Err(why) => {
            eprintln!("[richos] setup: transcription model {id} missing ({why}); fetching it");
            fetch_verified(&observer, &state, id)?;
        }
    }
    // 3. RICH'S VOICE (Pocket TTS + Peter Yearsley), in the background: it never holds this sheet,
    //    and until it is in Rich speaks with `say`.
    crate::voice_provision::fetch_rich_voice_in_background();
    Ok(format!("{} are installed.", capitalized(Component::MediaTools.display_name())))
}

/// **The model voice will hear with, named without a decoder.** `RICHOS_VOICE_WHISPER_MODEL_ID`
/// when an engineer set it (voice honors it first, `stt::choose_model`); otherwise the cost
/// table's safe rung (small.en today). That is the model `stt::readiness` asks for on any Mac with
/// no live-ladder model installed: the ladder keeps only installed rungs, and with none it answers
/// the safe rung (`hardware::resolve_live`). With this and the large model in, voice still
/// chooses between them by speed.
fn voice_model_to_fetch() -> String {
    std::env::var("RICHOS_VOICE_WHISPER_MODEL_ID")
        .ok()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| richos_voice::hardware::Costs::load().safe_rung)
}

/// **Fetch the pinned model `id` until it is in place and verified, or say why not.** The fetch's
/// own answer is the check: `installed` and `already-present` mean `provision` hashed the file
/// against its pin. `already-running` is the voice panel's own download: this waits for it to end
/// and asks again, which then finds the file or resumes it. `canceled` (Stop in the voice panel)
/// and anything else is no model, with voice's own sentence for that.
fn fetch_verified(
    observer: &SetupModelObserver,
    state: &Arc<crate::voice_provision::ModelFetchState>,
    id: &str,
) -> Result<(), SetupError> {
    loop {
        let fetched = run_fetch(|| crate::voice_provision::fetch_pinned(observer, state.clone(), id))?;
        let status = fetched.as_ref().and_then(|v| v.get("status")).and_then(|s| s.as_str()).unwrap_or("");
        match status {
            "installed" | "already-present" => {
                eprintln!("[richos] setup: speech model {id} in place and verified against its pin");
                return Ok(());
            }
            "already-running" => {
                eprintln!("[richos] setup: speech model {id}: another download is running; waiting for it");
                while state.in_flight.load(Ordering::SeqCst) {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
            }
            other => {
                eprintln!("[richos] setup: speech model {id} not in place after the fetch ({other:?})");
                return Err(SetupError::SpeechModelFailed {
                    sentence: richos_voice::stt::SttError::ModelNotFound(id.to_string())
                        .ceo_message(),
                });
            }
        }
    }
}

/// Run one model download to its end. ITS OWN THREAD AND ITS OWN RUNTIME: `block_on` inside a
/// runtime panics ("Cannot start a runtime from within a runtime"); a scoped thread owns a
/// current-thread runtime for exactly this download and ends with it. A download's own failure is
/// its sentence (written by `provision`); a panic is `None`, which the caller reports as a model
/// that is not in place.
fn run_fetch<F, Fut>(fetch: F) -> Result<Option<serde_json::Value>, SetupError>
where
    F: FnOnce() -> Fut + Send,
    Fut: std::future::Future<Output = Result<serde_json::Value, String>>,
{
    let fetched = std::thread::scope(|scope| {
        scope
            .spawn(|| -> Result<serde_json::Value, String> {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| format!("the download could not start: {e}"))?;
                runtime.block_on(fetch())
            })
            .join()
    });
    match fetched {
        Ok(Ok(v)) => {
            eprintln!("[richos] setup: speech model fetch answered {v}");
            Ok(Some(v))
        }
        Ok(Err(sentence)) => Err(SetupError::SpeechModelFailed { sentence }),
        Err(_) => {
            eprintln!("[richos] setup: the speech model download stopped unexpectedly");
            Ok(None)
        }
    }
}

fn capitalized(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// **The model download's progress, on this sheet AND in the voice panel** (plan §2). Each
/// `rich://voice-model` event goes to the voice panel verbatim and becomes a `started` line for
/// this step, with how far it has got, so a 574 MB download is a moving line rather than a sheet
/// that looks hung. Failure is not relayed as a line here: `install_media_tools` returns it, and
/// the background download emits it once.
struct SetupModelObserver {
    app: AppHandle,
    index: usize,
    total: usize,
    /// What this download brings in all, for the voice row's counter.
    tally: DownloadTally,
}

impl crate::voice_provision::ModelObserver for SetupModelObserver {
    fn on_model_event(&self, name: &str, payload: serde_json::Value) {
        // THE VOICE PANEL SEES THE SAME DOWNLOAD, on its own channel: its progress row, and on
        // `installed` the window asks voice again, so voice switches on when a model arrives
        // (`ui/main.js`, the `rich://voice-model` listener).
        if let Err(e) = self.app.emit(name, payload.clone()) {
            eprintln!("[richos] setup: the voice panel missed a model event ({e})");
        }
        if let Some(line) = model_progress(&self.tally, &payload, self.index, self.total) {
            emit_from(&self.app, line, true);
        }
    }
}

/// **One model event as the setup step's progress line**, or `None` for an event that is not
/// progress. The line keeps its words ("Getting my video tools. 43%", that model's own share) and
/// carries the whole download summed for the voice row ([`DownloadTally`]): `received` and `of`,
/// the counter's words, and whether both speech models are being fetched. While the file is
/// hashed against its pin the counter says [`COUNTER_CHECKING`].
fn model_progress(
    tally: &DownloadTally,
    payload: &serde_json::Value,
    index: usize,
    total: usize,
) -> Option<SetupProgress> {
    let phase = payload.get("phase").and_then(|p| p.as_str()).unwrap_or("");
    let model_id = payload.get("modelId").and_then(|v| v.as_str()).unwrap_or("");
    let received = payload.get("received").and_then(|v| v.as_u64()).unwrap_or(0);
    let whole = payload.get("total").and_then(|v| v.as_u64()).unwrap_or(0);
    let (what, counter) = match phase {
        "started" | "progress" if whole > 0 => (
            format!("{} {}%", started_line(Component::MediaTools), received.saturating_mul(100) / whole),
            None,
        ),
        "verifying" => (format!("{} 100%", started_line(Component::MediaTools)), Some(COUNTER_CHECKING.to_string())),
        _ => return None,
    };
    let (sum, of) = (tally.received(model_id, received), tally.total());
    Some(SetupProgress {
        received: Some(sum),
        of: Some(of),
        counter: Some(counter.unwrap_or_else(|| counter_line(sum, of))),
        both_models: Some(tally.both_models()),
        ..SetupProgress::line("started", Some(Component::MediaTools.as_str()), what, index, total)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **THE CONSENT SHEET IS ONE STEP AND CARRIES THE ACCOUNT SENTENCE.** The BYO-Anthropic
    /// caveat is row 3.14's second condition and it must reach him before he presses anything,
    /// not after.
    #[test]
    fn the_consent_sheet_says_he_still_needs_his_own_account() {
        assert!(SETUP_ACCOUNT_NOTE.contains("Anthropic account"));
        assert!(SETUP_ACCOUNT_NOTE.contains("sign in"));
        assert!(SETUP_ACCOUNT_NOTE.contains("never see your password"));
        // His sentence (CEO, 2026-10-08), word for word. It carries digits and a slash, so it is
        // exempt from the component copy's floor, compared against this constant.
        assert_eq!(
            SETUP_ACCOUNT_NOTE,
            "You need your own Anthropic account and a Max subscription there. You can sign in through your browser after setup; I never see your password. If you don't already have that subscription, sign up there first and pick the Max/Max 20x tier."
        );
        // The words the sheet turns into the pricing link must be in it.
        assert!(SETUP_ACCOUNT_NOTE.contains("sign up there first and pick the Max/Max 20x tier"));
    }

    /// **ONE WORDING, NOT TWO.** The const the state registry can see and the error the
    /// backend raises must be the same sentence, or the classified copy and the shipped copy
    /// drift and nobody finds out.
    #[test]
    fn the_unpinned_sentence_is_the_error_it_stands_for() {
        assert_eq!(SETUP_UNPINNED_NOTE, SetupError::EngineUnpinned.to_string());
        // And it names a party from `affordances.js`'s closed set, so a state he cannot fix
        // never leaves him with a fault and no owner.
        assert!(SETUP_UNPINNED_NOTE.contains("whoever set RichOS up"));
    }

    /// A status shaped by hand, so each arm of [`incomplete_message`] can be asked for
    /// directly rather than depending on what this machine happens to have installed.
    fn status(claude: bool, engine: bool, installable: bool) -> SetupStatus {
        status_with_video_tools(claude, engine, installable, true)
    }

    /// Built from an empty `SetupPaths` and stub answers rather than `detect(None)`, which reads
    /// this machine and asks `stt::readiness()` — and that can run whisper decodes on the host
    /// to calibrate (`stt.rs` `choose_model`).
    fn status_with_video_tools(claude: bool, engine: bool, installable: bool, video: bool) -> SetupStatus {
        let mut st = setup::detect_with_pin(&SetupPaths::default(), &[], &|_| Ok(()), None, &|| {
            Err("not asked in this test".to_string())
        });
        st.claude.present = claude;
        st.engine.present = engine;
        st.media_tools.present = video;
        st.engine_installable = installable;
        st
    }

    /// **A MACHINE SET UP BEFORE THE VIDEO TOOLS SEES THE SHEET WITH THEM ALONE** (media-tools
    /// plan §2), and can install them: they need no pin.
    #[test]
    fn only_the_video_tools_missing_puts_them_alone_on_the_sheet() {
        let st = status_with_video_tools(true, true, true, false);
        let ask = ask_for_with(&st, false);
        let items: Vec<&str> = ask.items.iter().map(|i| i.component).collect();
        assert_eq!(items, vec!["media-tools"]);
        assert_eq!(ask.items[0].name, Component::MediaTools.display_name());
        assert!(ask.can_install, "the video tools are always installable");
        assert!(!view_with(&st, false)["complete"].as_bool().unwrap(), "setup is not complete without them");
        // An unpinned build still offers them when the engine is already there.
        assert!(ask_for_with(&status_with_video_tools(true, true, false, false), false).can_install);
    }

    /// **NOTHING IN FIRST SETUP WAITS FOR THE VIDEO TOOLS** (the CEO, 2026-10-07: "The speech
    /// models and video tools begin downloading in the background at first launch, while the
    /// user does the rest of setup."). While their background download runs they are not on the
    /// sheet and not a step of any press; a press never waits for them even when they are missing
    /// and not downloading (it starts their download instead); and a Mac whose Claude Code and
    /// engine are in is complete for the window while the models are still arriving.
    #[test]
    fn the_video_tools_download_in_the_background_and_hold_nothing_up() {
        use Component::*;
        // The customer's Mac at first launch: nothing installed, the download already started.
        let fresh = status_with_video_tools(false, false, true, false);
        assert_eq!(sheet_needs(&fresh, true), vec![ClaudeCode, Engine]);
        // The gate stated, not read: `ask_for_with` reads `richos_core::DICTATION_READY`, a build
        // constant, and once dictation shipped (slice 5) the sheet LISTS the tools while they
        // download (plan rev 2 section 2 row 1; `once_dictation_is_there_the_sheet_lists_the_video_tools_while_they_download`).
        // This test is about the press and completeness, which the gate never changes.
        let names: Vec<&str> = ask_for_gated(&fresh, true, false).items.iter().map(|i| i.component).collect();
        assert_eq!(names, vec!["claude-code", "engine"], "the sheet does not ask for what is already downloading");
        // A press waits for Claude Code and the engine, never for the video tools.
        assert_eq!(foreground_steps(&[ClaudeCode, Engine, MediaTools]), vec![ClaudeCode, Engine]);
        assert!(foreground_steps(&sheet_needs(&fresh, false)).iter().all(|c| *c != MediaTools));
        // Claude Code and the engine in, the models still downloading: nothing to ask, setup done
        // for the window, so the memory and company questions come next.
        let lease_ready = status_with_video_tools(true, true, true, false);
        assert!(ask_for_with(&lease_ready, true).items.is_empty());
        assert!(view_with(&lease_ready, true)["complete"].as_bool().unwrap());
        // Not downloading (the launch's download failed): the sheet offers them again.
        assert_eq!(sheet_needs(&lease_ready, false), vec![MediaTools]);
        assert!(!view_with(&lease_ready, false)["complete"].as_bool().unwrap());
    }

    /// **THE NO-LEASE SENTENCE NEVER BLAMES THE VIDEO TOOLS.** A lease needs Claude Code and the
    /// engine; with only the video tools missing, a send that found no lease has the other cause
    /// (the account), and that sentence is `LEASE_UNAVAILABLE_MESSAGE`'s, not one of these.
    #[test]
    fn the_video_tools_alone_are_not_a_no_lease_cause() {
        assert_eq!(incomplete_message(&status_with_video_tools(true, true, true, false)), None);
        assert_eq!(
            incomplete_message(&status_with_video_tools(true, false, true, false)),
            Some(SETUP_INCOMPLETE_ENGINE)
        );
    }

    /// **THE SENTENCE NEVER TELLS HIM TO QUIT AND REOPEN WHEN THAT CANNOT HELP.**
    ///
    /// ray-opus-a2, published v1.0.1, 2026-09-04: an engine directory holding zero files, and
    /// four sends refused with "Quit RichOS and open it again". The next boot would look for
    /// the same absent engine and fail the same attach, so the only instruction he was given
    /// was one that could not work.
    #[test]
    fn a_machine_missing_something_is_never_told_to_restart() {
        for (c, e) in [(true, false), (false, true), (false, false)] {
            let msg = incomplete_message(&status(c, e, true)).expect("something is missing");
            // The INSTRUCTION, not the word. These sentences end by ruling a restart out —
            // "there's nothing to quit and nothing to reopen" — so a bare search for "quit"
            // would flag the fix as the defect.
            assert!(
                !msg.contains("Quit RichOS"),
                "a state a restart cannot clear must not ask for one: {msg}"
            );
            assert!(
                !msg.contains("open it again"),
                "a state a restart cannot clear must not ask for one: {msg}"
            );
            // And it says so out loud, because he has already been told the opposite.
            assert!(
                msg.contains("nothing to quit and nothing to reopen"),
                "the sentence must close the door on the advice that cannot work: {msg}"
            );
        }
        // The sentence LEASE_UNAVAILABLE_MESSAGE carries is the one this arm must not
        // duplicate, and the two must stay different strings.
        assert_ne!(
            incomplete_message(&status(true, false, true)),
            Some(crate::LEASE_UNAVAILABLE_MESSAGE)
        );
    }

    /// **IT NAMES THE MISSING PIECE, AND OFFERS THE THING THAT FIXES IT.** Naming is the
    /// difference between a status and a diagnosis; the offer is the difference between a
    /// diagnosis and a way out.
    #[test]
    fn it_names_what_is_missing_and_offers_the_setting_up() {
        let engine = incomplete_message(&status(true, false, true)).unwrap();
        assert!(engine.contains("the RichOS engine"), "{engine}");
        let claude = incomplete_message(&status(false, true, true)).unwrap();
        assert!(claude.contains("Claude Code"), "{claude}");
        let both = incomplete_message(&status(false, false, true)).unwrap();
        assert!(both.contains("Claude Code") && both.contains("the RichOS engine"), "{both}");
        for msg in [engine, claude, both] {
            assert!(msg.contains("Set it up"), "the offer is missing: {msg}");
            // The same floor the rest of this surface meets.
            assert!(!msg.contains('/'), "{msg}");
            assert!(!msg.chars().any(|c| c.is_ascii_digit()), "{msg}");
        }
    }

    /// **A COMPLETE MACHINE FALLS THROUGH TO THE OTHER SENTENCE.** No lease with nothing
    /// missing is the signed-out case, and for that one quitting and reopening genuinely does
    /// clear it — which is why the two states must not share a sentence in either direction.
    #[test]
    fn a_complete_machine_has_no_setup_sentence() {
        assert!(incomplete_message(&status(true, true, true)).is_none());
    }

    /// **A BUILD THAT CANNOT INSTALL DOES NOT PROMISE A BUTTON.** `ask_for_with` hides "Set it up"
    /// when the engine is unpinned, so a sentence saying to press it would name a control the
    /// sheet is not drawing.
    #[test]
    fn a_blocked_build_says_what_the_sheet_says() {
        let msg = incomplete_message(&status(true, false, false)).unwrap();
        assert_eq!(msg, SETUP_UNPINNED_NOTE);
        assert!(!msg.contains("Set it up"), "{msg}");
    }

    /// A progress line never puts a path or a version in front of him.
    #[test]
    fn no_progress_line_carries_a_path_or_a_version() {
        for c in [Component::ClaudeCode, Component::Engine, Component::MediaTools] {
            let line = started_line(c);
            assert!(!line.contains('/'), "{line}");
            assert!(!line.contains('~'), "{line}");
            assert!(!line.chars().any(|ch| ch.is_ascii_digit()), "{line}");
        }
    }

    // --- the dictation plan's slice 4: round 19, states 1 to 3 ---------------------------------

    const SMALL: (&str, u64) = ("small.en", 487_614_201);
    const LARGE: (&str, u64) = ("large-v3-turbo-q5_0", 574_041_195);

    fn tally(models: &[(&str, u64)], tools: u64) -> DownloadTally {
        DownloadTally::new(models.iter().map(|(id, b)| (id.to_string(), *b)).collect(), tools)
    }

    /// **HIS DOWNLOAD LINE, WORD FOR WORD** as round 19 draws it ("about 1 GB" and the thumbs-up
    /// included), Rich's line while the speech models download.
    #[test]
    fn his_download_line_is_word_for_word() {
        assert_eq!(
            SETUP_DOWNLOAD_LINE,
            "Sit tight, we need to download about 1 GB of local voice AI so that you can just talk to Rich instead of typing. This will save you $140+/year👍 because you won't need Wispr Flow with this setup."
        );
        assert_eq!(COUNTER_CHECKING, "Checking…");
    }

    /// **THE VOICE ROW COUNTS THE WHOLE DOWNLOAD** (dictation plan revision 2, section 2 row 2):
    /// yt-dlp and every missing speech model, summed, in round 19's words.
    #[test]
    fn the_counter_sums_what_this_mac_fetches_in_megabytes_and_gigabytes() {
        let both = tally(&[SMALL, LARGE], 3_000_000);
        assert_eq!(both.total(), 3_000_000 + 487_614_201 + 574_041_195);
        // The first model's own bytes, after yt-dlp.
        assert_eq!(both.received("small.en", 100), 3_000_100);
        // The second model's, after yt-dlp and the whole first model.
        assert_eq!(both.received("large-v3-turbo-q5_0", 1_000), 3_000_000 + 487_614_201 + 1_000);
        // A model's received never counts past its pinned size, and a model the tally does not
        // name adds nothing of its own.
        assert_eq!(both.received("small.en", u64::MAX), 3_000_000 + 487_614_201);
        assert_eq!(both.received("tiny.en", 5), 3_000_000);
        // A model named twice is fetched, and counted, once.
        assert_eq!(tally(&[SMALL, SMALL], 0).total(), 487_614_201);

        // Round 19's own reading, and the ends of the range.
        assert_eq!(counter_line(490_000_000, 1_061_655_396), "490 MB of 1.06 GB");
        assert_eq!(counter_line(0, 1_064_655_396), "0 MB of 1.06 GB");
        assert_eq!(counter_line(1_064_655_396, 1_064_655_396), "1065 MB of 1.06 GB");
        assert_eq!(counter_line(489_500_000, 1_061_655_396), "490 MB of 1.06 GB", "rounded, not truncated");
        // Below 1 GB the whole is in megabytes: one model alone.
        assert_eq!(counter_line(100_000_000, 574_041_195), "100 MB of 574 MB");
        assert_eq!(counter_line(200_000_000, 487_614_201), "200 MB of 488 MB");
    }

    /// **HIS LINE ONLY WHILE BOTH SPEECH MODELS ARE FETCHED** (minor 10): with one missing,
    /// "about 1 GB" is untrue, so the progress lines say so and the window leaves it out. The
    /// counter rides on every progress line, and says "Checking…" while the file is hashed.
    #[test]
    fn the_progress_line_carries_the_sum_and_whether_both_models_are_fetched() {
        let event = |phase: &str, id: &str, received: u64, total: u64| {
            serde_json::json!({ "phase": phase, "modelId": id, "received": received, "total": total })
        };
        let both = tally(&[SMALL, LARGE], 0);
        assert!(both.both_models());
        let line = model_progress(&both, &event("progress", "large-v3-turbo-q5_0", 2_385_195, LARGE.1), 1, 1).unwrap();
        assert_eq!(line.both_models, Some(true));
        assert_eq!(line.received, Some(487_614_201 + 2_385_195));
        assert_eq!(line.of, Some(1_061_655_396));
        assert_eq!(line.counter.as_deref(), Some("490 MB of 1.06 GB"));
        assert_eq!(line.component, Some("media-tools"));
        assert_eq!(line.state, "started");
        // The words stay that model's own share, as before.
        assert!(line.what.ends_with(" 0%"), "{}", line.what);

        for one in [tally(&[SMALL], 0), tally(&[LARGE], 3_000_000), tally(&[], 3_000_000)] {
            assert!(!one.both_models());
        }
        let one = tally(&[LARGE], 0);
        let line = model_progress(&one, &event("started", "large-v3-turbo-q5_0", 0, LARGE.1), 1, 1).unwrap();
        assert_eq!(line.both_models, Some(false));
        assert_eq!(line.counter.as_deref(), Some("0 MB of 574 MB"));

        let checking = model_progress(&both, &event("verifying", "large-v3-turbo-q5_0", LARGE.1, LARGE.1), 1, 1).unwrap();
        assert_eq!(checking.counter.as_deref(), Some("Checking…"));
        assert_eq!(checking.received, Some(1_061_655_396));
        // Not progress: nothing to relay (installed, failed, a started with no size).
        assert!(model_progress(&both, &event("installed", "small.en", SMALL.1, SMALL.1), 1, 1).is_none());
        assert!(model_progress(&both, &event("failed", "small.en", 0, SMALL.1), 1, 1).is_none());
        assert!(model_progress(&both, &event("started", "small.en", 0, 0), 1, 1).is_none());
        // Every other line carries no counter.
        let plain = SetupProgress::line("done", Some("engine"), "Your engine is installed.".into(), 2, 2);
        assert_eq!((plain.received, plain.of, plain.counter, plain.both_models), (None, None, None, None));
    }

    /// **ONCE DICTATION IS THERE, THE SHEET IS ROUND 19'S** (section 2 row 1): his name and line
    /// for the video tools, and the video tools listed even while they download, because the
    /// sheet is consent copy; nothing a press waits for changes. Until then, the sheet as it was.
    #[test]
    fn once_dictation_is_there_the_sheet_lists_the_video_tools_while_they_download() {
        let fresh = status_with_video_tools(false, false, true, false);
        let on = ask_for_gated(&fresh, true, true);
        let names: Vec<&str> = on.items.iter().map(|i| i.component).collect();
        assert_eq!(names, vec!["claude-code", "engine", "media-tools"]);
        assert_eq!(on.items[2].name, richos_core::setup::MEDIA_TOOLS_NAME);
        assert_eq!(on.items[2].why, richos_core::setup::MEDIA_TOOLS_WHY);
        assert!(on.can_install);
        assert!(on.dictation);
        assert_eq!(on.download_line, Some(SETUP_DOWNLOAD_LINE));
        // A press still waits for Claude Code and the engine only.
        assert_eq!(foreground_steps(&sheet_needs(&fresh, true)), vec![Component::ClaudeCode, Component::Engine]);

        // Off: exactly the sheet as it was.
        let off = ask_for_gated(&fresh, true, false);
        let names: Vec<&str> = off.items.iter().map(|i| i.component).collect();
        assert_eq!(names, vec!["claude-code", "engine"]);
        assert!(!off.dictation);
        assert_eq!(off.download_line, None);
        let not_downloading = ask_for_gated(&fresh, false, false);
        assert_eq!(not_downloading.items[2].name, "my video tools");

        // With Claude Code and the engine in, a running download opens no sheet, on or off.
        let lease_ready = status_with_video_tools(true, true, true, false);
        assert!(ask_for_gated(&lease_ready, true, true).items.is_empty());
        assert!(!ask_for_gated(&lease_ready, true, true).can_install);
        // Not downloading (the launch's download failed): offered alone, under his name.
        let alone = ask_for_gated(&lease_ready, false, true);
        assert_eq!(alone.items.len(), 1);
        assert_eq!(alone.items[0].name, "my voice and video tools");
    }
}

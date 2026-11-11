//! **The dictation tool**: the same RichOS executable started with `--richos-dictation` (plan
//! section 1 and section 6).
//!
//! Checked in `main` after `startup_alert::install` and before `update_startup::prepare`, so the
//! tool builds no main window, opens no launch record, holds no update lease, never calls
//! `begin_run` and never writes `launches.json`. Its own small Tauri builder carries no plugin;
//! its only commands are its own pages' (`bar.rs`). The tool sets the Accessory activation
//! policy before its event loop runs, so it has no Dock icon and never takes the front.
//!
//! It owns the key tap, the dictation microphone, whisper-cli and the paste (slice 1), and the
//! bar, the words' flight and the menu bar item (slice 3: `bar.rs`, `menubar.rs`). It is the
//! app's child (`--parent <pid>`) in any copy that cannot register the login start, started by
//! the app when `dictation.json` says on, and it exits within a second of the app ending; in an
//! installed copy it is launchd's (slice 5, `login.rs`): started at login with no arguments but
//! its own, on the installed data folder it computes for the account, resolving the engine's
//! whisper-cli itself, outliving the app, and restarting for an app of another version.
//!
//!   richos-tauri --richos-dictation [--data-dir <dir>] [--parent <pid>]

use super::appkit::{self, Kind};
use super::bar::{self, Bar, Menu, MenuMessage, UiEvent, UiTx, MENU};
use super::ipc::{self, AppMessage, Hub, ToolMessage};
use super::keytap::{self, KeyTap, TapEvent};
use super::{insert, log, menubar, scratch::Scratch, store};
use richos_voice::capture::{self, AudioSource, Capture};
use richos_voice::dictation::{
    decode_bound, judge_recording, judge_transcript, pick_model, Draft, Drafts, Insert, ModelPick, Phase, Problem, Recording, Session,
    Stopped, TapAction,
};
use richos_voice::dictation_bar::{choice_for_model, hide_after, meter_level, model_for_choice, view_for, BarView, MeterGate, Rect};
use richos_voice::stt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::Manager;
mod writing;

/// How long the tool waits for every connected app to answer `will-listen` with
/// `voice-yielded` before it opens the microphone (review finding 2). An app answers from its
/// socket reader the moment it has closed voice mode, which is milliseconds; this bound only
/// matters for an app that is wedged, and it is logged when it is reached.
pub const HANDOVER_WAIT: Duration = Duration::from_secs(2);

/// A quit that reaches the control loop (the parent gone, SIGTERM) ends the tool there, where
/// the writing worker can be stopped and joined; this is the backstop if the loop never answers.
const QUIT_BACKSTOP: Duration = Duration::from_secs(5);

/// SIGTERM was delivered (launchd's `bootout`, a `kill`): read by a thread, never acted on in
/// the handler.
static TERMINATED: AtomicBool = AtomicBool::new(false);

/// Unlike Instant on macOS, this clock also advances while the Mac sleeps.
/// Wall-clock changes must never extend a draft's retention.
fn draft_now() -> Duration {
    unsafe extern "C" { fn clock_gettime_nsec_np(clock: libc::clockid_t) -> u64; }
    // SAFETY: this documented clock takes no pointers, includes sleep and is
    // available since macOS 10.12. Unlike CLOCK_UPTIME_RAW it counts sleep.
    Duration::from_nanos(unsafe { clock_gettime_nsec_np(libc::CLOCK_MONOTONIC_RAW) })
}

extern "C" fn on_sigterm(_: libc::c_int) {
    TERMINATED.store(true, Ordering::SeqCst);
}

/// **The microphone handover, counted by copy** (review finding 2; third review, finding 1):
/// `will-listen` went to the copies in `told`; the microphone opens once every one of them has
/// answered this handover, and never at the bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handover {
    pub seq: u64,
    /// The connections told `will-listen`, by the ids the Hub gave them (fourth review, finding
    /// 1: two processes from one bundle are two connections).
    pub told: std::collections::BTreeSet<u64>,
    /// The connections that have answered this handover.
    pub heard: std::collections::BTreeSet<u64>,
}

/// What one `voice-yielded` did to the handover (recheck, third review, finding 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Accept {
    /// Counted; others are still to answer.
    Counted,
    /// Counted, and every copy told has now answered.
    Complete,
    /// An answer to an earlier handover: dropped.
    Stale,
    /// A second answer from a copy already counted: dropped.
    Duplicate,
    /// An answer from a copy that was not told: dropped.
    Unknown,
}

impl Handover {
    pub fn new(seq: u64, told: Vec<u64>) -> Handover {
        Handover { seq, told: told.into_iter().collect(), heard: Default::default() }
    }

    /// Every app that was told has answered.
    pub fn complete(&self) -> bool {
        self.told.iter().all(|b| self.heard.contains(b))
    }

    /// **One answer, matched to this handover and its connection.** The third review's
    /// counterexample: a late answer from A to a timed-out handover plus A's current answer
    /// counted as two apps while B had not yielded; here A's late answer is Stale and A's second
    /// is Duplicate, and B's silence keeps the microphone closed. The fourth review's: two
    /// processes from one bundle are two connections, so one answer never completes for both.
    pub fn accept(&mut self, seq: u64, client: u64) -> Accept {
        if seq != self.seq {
            return Accept::Stale;
        }
        if !self.told.contains(&client) {
            return Accept::Unknown;
        }
        if !self.heard.insert(client) {
            return Accept::Duplicate;
        }
        if self.complete() {
            Accept::Complete
        } else {
            Accept::Counted
        }
    }

    /// **May the microphone open now?** Only when every app has yielded it: at the bound, an
    /// unanswered will-listen keeps the microphone closed (the handover fails closed; recheck
    /// finding 1), because an app that has not answered may still be listening in voice mode.
    pub fn microphone_may_open(&self) -> bool {
        self.complete()
    }
}

pub const ARG: &str = "--richos-dictation";

/// Where `dictation.log` writes the hourly key-tap line.
pub const HOUR: Duration = Duration::from_secs(3600);

/// How often a child tool checks that its app is still there: within a second of the app ending
/// the tool is gone (plan section 6).
const PARENT_POLL: Duration = Duration::from_millis(200);

/// How often Secure Event Input is read while dictation is on (Frank's minor 5: one call every
/// 2 seconds).
const SECURE_POLL: Duration = Duration::from_secs(2);

/// A click on the menu bar item this soon after the menu lost the keyboard is the click that
/// took it: the menu stays closed rather than opening again.
const REOPEN_GUARD: Duration = Duration::from_millis(300);

/// **The window check's preview** (slice 3's first step): a problem bar held on screen from the
/// start, so the check can put it over a full-screen app and click its Fix it in the guest,
/// where no real microphone or Accessibility refusal can be arranged. Its Fix it only logs. Never
/// set in a shipping run.
pub const PREVIEW_ENV: &str = "RICHOS_DICTATION_PREVIEW";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// The data folder; `None` is a launchd start, which computes the installed one.
    pub data_dir: Option<PathBuf>,
    pub parent: Option<i32>,
}

/// The arguments after `--richos-dictation`.
pub fn parse(args: &[String]) -> Result<Args, String> {
    let mut data_dir = None;
    let mut parent = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--data-dir" => data_dir = Some(it.next().map(PathBuf::from).ok_or("--data-dir needs a folder")?),
            "--parent" => parent = Some(it.next().and_then(|p| p.parse::<i32>().ok()).ok_or("--parent needs a process id")?),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if data_dir.as_ref().is_some_and(|d| !d.is_absolute()) {
        return Err("--data-dir must be absolute".into());
    }
    Ok(Args { data_dir, parent })
}

/// **The engine's whisper-cli, for a tool the app did not start.** The app hands its child the
/// verified runtime's `bin` (`RICHOS_DICTATION_RUNTIME_BIN`); a launchd start has no app to ask,
/// so it resolves the engine exactly as the app does at boot (`engine.rs`, the same seven
/// candidates) and verifies its runtime (`runtime::verify_engine`, the same hashes). The line
/// this returns goes in the log, so "which whisper-cli" is never a guess.
fn resolve_runtime_bin_alone() -> String {
    let resolution = crate::engine::resolve_engine_dir(&crate::engine::LaunchPaths::from_process());
    let Some(dir) = resolution.dir.clone() else {
        return format!("no engine found for whisper-cli ({}); PATH and Homebrew are tried at the first dictation", resolution.describe());
    };
    match richos_core::runtime::verify_engine(&dir) {
        Ok(runtime) => {
            let bin = runtime.root.join("bin");
            stt::set_delivered_runtime_bin(Some(bin.clone()));
            format!("whisper-cli from the engine runtime at {} ({})", bin.display(), resolution.describe())
        }
        Err(e) => format!("the engine at {} has no verified runtime ({e}); PATH and Homebrew are tried at the first dictation", dir.display()),
    }
}

/// The preview's problem, by its tag.
pub fn preview_problem(tag: Option<&str>) -> Option<Problem> {
    let tag = tag?.trim();
    [
        Problem::DidNotCatch,
        Problem::NoSound,
        Problem::NoTextBox,
        Problem::NoMicrophone,
        Problem::NoAccessibility,
        Problem::ModelMissing,
        Problem::CouldNotWrite,
    ]
    .into_iter()
    .find(|p| p.tag() == tag)
}

/// Everything the control loop hears.
enum Control {
    WritingReply { client:u64,id:String,action:String,payload:serde_json::Value },
    LivePreview { seq: u64, update: super::live_preview::Update },
    Vocabulary { seq: u64, client: u64, publication: super::vocabulary::Publication },
    Tap(TapEvent),
    CleanerLookup {id:String,result:Result<serde_json::Value,String>},
    CleanerReady {id:String,result:Result<serde_json::Value,String>},
    App(AppMessage),
    InputDied(u64),
    Written { seq: u64, result: WriteResult },
    DraftExpiry,
    Hour,
    Ui(UiEvent),
    BarExpired(u64),
    FlightDone(u64),
    Secure(Option<Option<String>>),
    /// The app that started this child tool has ended, or SIGTERM arrived: end here, where the
    /// writing worker is stopped and joined (review finding 1).
    Quit(&'static str),
    /// The handover's bound passed (review finding 2).
    HandoverTimeout(u64),
    /// A `voice-yielded` from connection `client` for handover `seq` (fourth review, finding 1).
    Yielded { seq: u64, client: u64, bundle: String },
}

/// What the state message reports, shared with the socket thread.
struct Shared {
    data_dir: String,
    owner: String,
    on: bool,
    phase: Phase,
    key_tap: bool,
    secure: Option<Option<String>>,
    key: u8,
    model: String,
}

impl Shared {
    fn message(&self) -> ToolMessage {
        ToolMessage::State {
            data_dir: self.data_dir.clone(),
            owner: self.owner.clone(),
            on: self.on,
            listening: matches!(self.phase, Phase::Listening { .. }),
            writing: self.phase == Phase::Writing,
            problem: match self.phase {
                Phase::Problem(p) => Some(p.tag().to_string()),
                _ => None,
            },
            key_tap: self.key_tap,
            secure_input: self.secure.is_some(),
            secure_app: self.secure.clone().flatten(),
            key: self.key,
            model: self.model.clone(),
        }
    }
}

/// The writing worker: one dictation from the recording to the words in place, on a thread of
/// its own, owned so a quit can stop its decoder and join it (review finding 1).
struct Writing {
    stop: Arc<AtomicBool>,
    join: std::thread::JoinHandle<()>,
}

impl Writing {
    /// Stop the decoder (killed and reaped inside `bounded_decoder_until`) and wait for the
    /// worker, whose scratch folder then drops its files.
    fn stop_and_join(self) {
        self.stop.store(true, Ordering::SeqCst);
        self.join.thread().unpark();
        if self.join.join().is_err() {
            log::line("the writing worker ended with a panic while being stopped");
        }
    }
}

/// The bundle this executable belongs to: `<x>.app` three levels above `Contents/MacOS/<exe>`.
fn own_bundle() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.ancestors().nth(3).map(|p| p.display().to_string()))
        .unwrap_or_default()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Send `control` after `after`, on a thread of its own.
fn later(tx: &Sender<Control>, after: Duration, control: Control) {
    let tx = tx.clone();
    std::thread::Builder::new()
        .name("dictation-timer".into())
        .spawn(move || {
            std::thread::sleep(after);
            tx.send(control).ok();
        })
        .ok();
}

/// The tool's `main`. Returns the process exit code.
pub fn main(context: tauri::Context<tauri::Wry>, args: &[String]) -> i32 {
    // The tool never puts a modal on his screen: a failure here is a log line. The panic hook
    // `startup_alert::install` set stays, so a panic is still recorded.
    crate::startup_alert::disarm();
    let args = match parse(args) {
        Ok(a) => a,
        Err(why) => {
            eprintln!("[richos-dictation] {why}");
            return 2;
        }
    };
    let own_version = context.package_info().version.to_string();
    // The tool runs only while RichOS runs (the CEO, 2026-10-08: no start at login in this
    // release): its data folder is always named by the app that started it.
    let Some(data_dir) = args.data_dir.clone() else {
        eprintln!("[richos-dictation] no --data-dir: the dictation tool is started by RichOS, never on its own");
        return 2;
    };
    let launched_by = if args.parent.is_some() { "as the app's child" } else { "on its own" };
    log::init(&data_dir);
    let runtime_note = match std::env::var_os("RICHOS_DICTATION_RUNTIME_BIN").filter(|d| !d.is_empty()) {
        Some(dir) => {
            stt::set_delivered_runtime_bin(Some(PathBuf::from(&dir)));
            format!("whisper-cli from the app's runtime at {}", PathBuf::from(dir).display())
        }
        None => resolve_runtime_bin_alone(),
    };
    let settings = match store::read(&data_dir) {
        Ok(s) => s,
        Err(why) => {
            log::line(&why);
            return 2;
        }
    };
    if !settings.on {
        log::line("dictation is off in dictation.json; the tool has nothing to do");
        return 0;
    }
    let owner = match ipc::claim(&ipc::runtime_dir()) {
        Ok(Some(owner)) => owner,
        Ok(None) => {
            log::line("another copy of RichOS holds the dictation key in this login session; this tool exits with no key tap");
            return 3;
        }
        Err(e) => {
            log::line(&format!("the dictation key could not be claimed: {e}"));
            return 2;
        }
    };
    let (tx, rx) = channel::<Control>();
    if let Some(parent) = args.parent {
        let tx = tx.clone();
        std::thread::Builder::new()
            .name("dictation-parent".into())
            .spawn(move || loop {
                // SAFETY: getppid has no failure mode.
                if unsafe { libc::getppid() } != parent {
                    // Through the control loop, which stops and joins the writing worker
                    // (finding 1); the backstop is for a loop that never answers.
                    tx.send(Control::Quit("the app that started this tool has ended")).ok();
                    std::thread::sleep(QUIT_BACKSTOP);
                    insert::settle();
                    std::process::exit(0);
                }
                std::thread::sleep(PARENT_POLL);
            })
            .ok();
    }
    {
        // SIGTERM (launchd's bootout, a kill): the same path as the parent ending.
        // SAFETY: installing a handler that only stores a flag.
        unsafe {
            libc::signal(libc::SIGTERM, on_sigterm as extern "C" fn(libc::c_int) as libc::sighandler_t);
        }
        let tx = tx.clone();
        std::thread::Builder::new()
            .name("dictation-sigterm".into())
            .spawn(move || loop {
                if TERMINATED.load(Ordering::SeqCst) {
                    tx.send(Control::Quit("SIGTERM")).ok();
                    std::thread::sleep(QUIT_BACKSTOP);
                    insert::settle();
                    std::process::exit(0);
                }
                std::thread::sleep(Duration::from_millis(100));
            })
            .ok();
    }
    let shared = Arc::new(Mutex::new(Shared {
        data_dir: data_dir.display().to_string(),
        owner: own_bundle(),
        on: true,
        phase: Phase::Idle,
        key_tap: false,
        secure: None,
        key: settings.key(),
        model: settings.model.clone(),
    }));
    let hub = Arc::new(Hub::default());
    {
        let shared = shared.clone();
        let tx = tx.clone();
        if let Err(e) = ipc::serve(
            &owner,
            hub.clone(),
            move || shared.lock().unwrap_or_else(|p| p.into_inner()).message(),
            move |client, m| {
                // A voice-yielded is tied to the connection that sent it, by the id the Hub
                // gave that connection (fourth review, finding 1); its bundle is for the log.
                let control = match m {
                    AppMessage::Writing {id,action,payload}=>Control::WritingReply {client,id,action,payload},
                    AppMessage::Vocabulary { seq, publication } => Control::Vocabulary { seq, client, publication },
                    AppMessage::VoiceYielded { seq, bundle } => Control::Yielded { seq, client, bundle },
                    m => Control::App(m),
                };
                tx.send(control).ok();
            },
        ) {
            log::line(&format!("the dictation socket could not be served: {e}"));
            return 2;
        }
    }
    {
        let tx = tx.clone();
        std::thread::Builder::new()
            .name("dictation-hour".into())
            .spawn(move || loop {
                std::thread::sleep(HOUR);
                if tx.send(Control::Hour).is_err() {
                    return;
                }
            })
            .ok();
    }
    {
        // Secure Event Input, read every 2 s; only a change is reported.
        let tx = tx.clone();
        std::thread::Builder::new()
            .name("dictation-secure".into())
            .spawn(move || {
                let mut last = None;
                loop {
                    let now = appkit::secure_input();
                    if now != last {
                        last = now.clone();
                        if tx.send(Control::Secure(now)).is_err() {
                            return;
                        }
                    }
                    std::thread::sleep(SECURE_POLL);
                }
            })
            .ok();
    }
    let (ui_tx, ui_rx) = channel::<UiEvent>();
    {
        let tx = tx.clone();
        std::thread::Builder::new()
            .name("dictation-ui-relay".into())
            .spawn(move || {
                while let Ok(e) = ui_rx.recv() {
                    if tx.send(Control::Ui(e)).is_err() {
                        return;
                    }
                }
            })
            .ok();
    }
    let kind = appkit::kind_from(std::env::var(appkit::KIND_ENV).ok().as_deref());
    let preview = preview_problem(std::env::var(PREVIEW_ENV).ok().as_deref());
    log::line(&format!(
        "dictation tool started: version {own_version}, key F{}, accuracy {}, {launched_by}, the key held at {}{}",
        settings.key(),
        settings.model,
        owner.socket().display(),
        preview.map(|p| format!("; PREVIEW of the {} bar (window check only)", p.tag())).unwrap_or_default(),
    ));
    log::line(&runtime_note);

    let builder = tauri::Builder::default()
        .manage(UiTx(Mutex::new(ui_tx.clone())))
        .invoke_handler(tauri::generate_handler![
            bar::dictation_page_ready,
            bar::dictation_bar_laid_out,
            bar::dictation_fix_it,
            bar::dictation_bar_act,
            bar::dictation_menu_laid_out,
            bar::dictation_menu_act,
        ])
        .setup(move |app| {
            // No Dock icon, never the front: before the event loop runs (activation.rs's lever).
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            if let Err(why) = appkit::watch_activation() {
                log::line(&why);
            }
            let handle = app.handle().clone();
            if let Err(why) = bar::build(&handle, kind) {
                log::line(&why);
            }
            if let Some(menu) = app.get_webview_window(MENU) {
                // The menu closes when it loses the keyboard: a click anywhere else.
                let ui = ui_tx.clone();
                menu.on_window_event(move |e| {
                    if let tauri::WindowEvent::Focused(false) = e {
                        ui.send(UiEvent::Menu { act: "close".into(), value: None }).ok();
                    }
                });
            }
            let tray = match menubar::build(&handle) {
                Ok(t) => Some(t),
                Err(why) => {
                    log::line(&why);
                    None
                }
            };
            let data_dir = args.data_dir.clone().unwrap_or_default();
            std::thread::Builder::new().name("dictation-control".into()).spawn(move || {
                let mut tool = Tool::new(settings, data_dir, shared, hub, tx, handle, kind, tray, preview);
                tool.version = own_version;
                let code = tool.run(rx);
                drop(owner);
                std::process::exit(code);
            })?;
            Ok(())
        });
    match builder.build(context) {
        Ok(app) => {
            app.run(|_, _| {});
            0
        }
        Err(e) => {
            log::line(&format!("the dictation tool could not start its event loop: {e}"));
            2
        }
    }
}

struct Tool {
    assistance: Option<writing::Assistance>,
    cleanup_anchor: Option<insert::Anchor>,
    writing_client: Option<u64>,
    timings: Option<super::timings::Timings>,
    live_preview: Option<Writing>,
    live_words: Option<String>,
    live_preview_paused: bool,
    destination: Option<insert::Destination>,
    destination_name: String,
    navigated: Arc<AtomicBool>,
    pointer_observer: Option<keytap::PointerObserver>,
    drafts: Drafts,
    vocabulary_expanded: bool,
    notice: Option<bar::DeliveryMessage>,
    recovery_open: bool,
    recovery_status: Option<&'static str>,
    vocabulary: Option<super::vocabulary::Publication>,
    recording_vocabulary: Option<super::vocabulary::Snapshot>,
    vocabulary_revoked: Arc<AtomicBool>,
    settings: store::Settings,
    data_dir: PathBuf,
    shared: Arc<Mutex<Shared>>,
    hub: Arc<Hub>,
    tx: Sender<Control>,
    app: tauri::AppHandle,
    session: Session,
    tap: Option<KeyTap>,
    capture: Option<Capture>,
    recording: Arc<Mutex<Option<Recording>>>,
    disables: u64,
    kind: Kind,
    bar: Bar,
    menu: Menu,
    menu_closed_at: Option<Instant>,
    tray: Option<tauri::tray::TrayIcon>,
    tray_listening: bool,
    secure: Option<Option<String>>,
    preview: Option<Problem>,
    /// The front app's window when his key went down, for the bar's screen.
    focused: Option<Rect>,
    flights: u64,
    /// **Turn dictation off** was chosen: the tool ends once the bar has said so.
    ending: bool,
    /// This tool's compiled version, logged with each app's `hello`.
    version: String,
    /// The dictation being written down, owned (finding 1).
    writing: Option<Writing>,
    /// `will-listen` is out and the microphone waits for the answers (finding 2).
    handover: Option<Handover>,
    handovers: u64,
    /// **The accuracy pinned when listening began** (finding 6): Faster chosen mid-dictation
    /// takes effect on the next one, as the menu says.
    recording_model: String,
    capture_seq:u64,
    cleaner_enabled:bool,
    cleaner_pending:Option<String>,
    cleaner_lookup:Option<Writing>,
    cleaner_target:Option<(String,serde_json::Value)>,
}

impl Tool {
    #[allow(clippy::too_many_arguments)]
    fn new(
        settings: store::Settings,
        data_dir: PathBuf,
        shared: Arc<Mutex<Shared>>,
        hub: Arc<Hub>,
        tx: Sender<Control>,
        app: tauri::AppHandle,
        kind: Kind,
        tray: Option<tauri::tray::TrayIcon>,
        preview: Option<Problem>,
    ) -> Tool {
        Tool {
            assistance: None,
            cleanup_anchor: None,
            writing_client: None,
            timings: None,
            live_preview: None,
            live_words: None,
            live_preview_paused: false,
            destination: None,
            destination_name: String::new(),
            navigated: Arc::new(AtomicBool::new(false)),
            pointer_observer: None,
            drafts: Drafts::default(),
            vocabulary_expanded: false,
            notice: None,
            recovery_open: false,
            recovery_status: None,
            vocabulary: None,
            recording_vocabulary: None,
            vocabulary_revoked: Arc::new(AtomicBool::new(false)),
            settings,
            data_dir,
            shared,
            hub,
            tx,
            app,
            session: Session::default(),
            tap: None,
            capture: None,
            recording: Arc::new(Mutex::new(None)),
            disables: 0,
            kind,
            bar: Bar::default(),
            menu: Menu::default(),
            menu_closed_at: None,
            tray,
            tray_listening: false,
            secure: None,
            preview,
            focused: None,
            flights: 0,
            ending: false,
            version: String::new(),
            writing: None,
            handover: None,
            handovers: 0,
            recording_model: String::new(),
            capture_seq:0,cleaner_enabled:false,cleaner_pending:None,cleaner_lookup:None,cleaner_target:None,
        }
    }

    /// **Open RichOS**, **Dictation settings…** and **Fix it**: the connected app comes forward,
    /// on the Dictation sheet when `sheet`. The tool is a running app's child, so an app is
    /// connected except in the moment it is starting or ending; with none, nothing is started.
    fn bring_app(&mut self, sheet: bool) {
        let heard = self.hub.broadcast(&ToolMessage::ComeForward { sheet });
        if heard > 0 {
            log::line(&format!("RichOS asked for: {heard} connected app(s) told to come forward"));
        } else {
            log::line("RichOS asked for, but no app is connected to this tool");
        }
    }

    fn publish(&mut self) {
        if self.instruction_recording() {
            if let Phase::Problem(problem)=self.session.phase() {self.instruction_done(None,Err(problem));}
        }
        if !matches!(self.session.phase(), Phase::Listening { .. } | Phase::Writing) { self.end_navigation(); }
        if !matches!(self.session.phase(), Phase::Listening { .. } | Phase::Writing) { self.end_navigation(); }
        if let Phase::Problem(problem)=self.session.phase(){if let Some((id,_))=self.cleaner_target.take(){self.session.reset();self.hub.broadcast(&ToolMessage::Cleaner{id,action:"error".into(),payload:serde_json::json!({"message":problem.tag()})});}}

        let message = {
            let Ok(mut s) = self.shared.lock() else { return };
            s.phase = self.session.phase();
            s.on = self.settings.on;
            s.key_tap = self.tap.is_some();
            s.secure = self.secure.clone();
            s.key = self.settings.key();
            s.model = self.settings.model.clone();
            s.message()
        };
        self.hub.broadcast(&message);
        self.sync_bar();
        self.sync_tray();
    }

    /// The bar follows the session: each new view is drawn, and the ones that go by themselves
    /// get their timer (`dictation_bar::hide_after`).
    fn sync_bar(&mut self) {
        if self.cleaner_pending.is_some() || self.cleaner_target.is_some(){self.bar.show(&self.app,BarView::Hidden,self.settings.key(),None,bar::look(&self.data_dir),bar::BarDetails::default());return;}
        if self.ending {
            return;
        }
        let view = if self.notice.as_ref().is_some_and(|n| n.state != "added")
            && !matches!(self.session.phase(), Phase::Listening { .. } | Phase::Writing | Phase::Idle) {
                BarView::Delivery
            } else { view_for(self.session.phase()) };
        if self.bar.view() == Some(view) || (self.bar.view().is_none() && view == BarView::Hidden) {
            return;
        }
        self.show_bar(view);
    }

    fn show_bar(&mut self, view: BarView) {
        let started = match self.session.phase() {
            Phase::Listening { started_ms } => Some(started_ms),
            _ => None,
        };
        let details = bar::BarDetails {
            instruction: self.instruction_recording(),
            blocked_keys: self.secure.is_some(),
            blocked_by: self.secure.clone().flatten(),
            destination: (!self.destination_name.is_empty()).then(|| self.destination_name.clone()),
            vocabulary: self.recording_vocabulary.as_ref().map(|v| v.company.name.clone()),
            delivery: self.notice.clone(),
            preview: self.live_words.clone(),
            preview_paused: self.live_preview_paused,
        };
        let seq = self.bar.show(&self.app, view, self.settings.key(), started, bar::look(&self.data_dir), details);
        if let Some(after) = hide_after(view) {
            later(&self.tx, after, Control::BarExpired(seq));
        }
    }

    /// The menu bar item is gold while listening.
    fn sync_tray(&mut self) {
        let listening = matches!(self.session.phase(), Phase::Listening { .. });
        if listening == self.tray_listening {
            return;
        }
        self.tray_listening = listening;
        if let Some(tray) = &self.tray {
            let dark = bar::ask_main(&self.app, appkit::dark_appearance).unwrap_or(true);
            menubar::set_listening(tray, listening, dark);
        }
    }

    fn menu_message(&mut self, seq: u64) -> MenuMessage {
        let draft = self.drafts.recovery(draft_now()).cloned();
        let recording = matches!(self.session.phase(), Phase::Listening { .. } | Phase::Writing);
        MenuMessage {
            cleanup_available: !recording && self.drafts.cleanup_subject(draft_now()).is_some(),
            writing: self.writing_view(),
            seq,
            on: self.settings.on,
            key: format!("F{}", self.settings.key()),
            choice: choice_for_model(&self.settings.model),
            paused: self.secure.is_some(),
            secure_app: self.secure.clone().flatten(),
            companies: self.vocabulary.as_ref().map(|v| v.companies.clone()).unwrap_or_default(),
            vocabulary_company: self.vocabulary.as_ref().and_then(|v| v.active.as_ref()).map(|v| v.company.id.clone()),
            vocabulary_error: self.vocabulary.as_ref().and_then(|v| v.error.clone()),
            vocabulary_expanded: self.vocabulary_expanded,
            recording,
            recover_available: draft.is_some() && !recording,
            recovery: self.recovery_open.then(|| bar::RecoveryMessage {
                text: draft.as_ref().map(|d| d.text.clone()),
                destination: draft.as_ref().map(|d| d.destination.clone()),
                completed_ms: draft.as_ref().map(|d| d.completed_ms),
                expires_ms: self.drafts.recovery_lifetime(draft_now()).map(|remaining|
                    richos_core::util::now_millis().saturating_add(remaining.as_millis() as u64)),
                status: self.recovery_status,
            }),
            look: bar::look(&self.data_dir),
        }
    }

    /// The tap exists only while dictation is on and Accessibility is allowed.
    fn ensure_tap(&mut self) {
        if self.tap.is_some() {
            return;
        }
        if !keytap::accessibility_allowed() {
            log::line("Accessibility is not allowed for RichOS yet; no key tap until it is");
            return;
        }
        let tx = self.tx.clone();
        let (tap_tx, tap_rx) = channel::<TapEvent>();
        std::thread::Builder::new()
            .name("dictation-tap-relay".into())
            .spawn(move || {
                while let Ok(e) = tap_rx.recv() {
                    if tx.send(Control::Tap(e)).is_err() {
                        return;
                    }
                }
            })
            .ok();
        match KeyTap::start(self.settings.key(), tap_tx) {
            Ok(tap) => {
                log::line(&format!("key tap created for F{}", self.settings.key()));
                self.tap = Some(tap);
            }
            Err(why) => log::line(&why),
        }
    }

    fn run(mut self, rx: Receiver<Control>) -> i32 {
        self.ensure_tap();
        self.publish();
        // Logged once the main run loop has placed the item, never the unplaced frame.
        if let Some(tray) = self.tray.clone() {
            menubar::log_item_when_placed(tray);
        }
        loop {
            self.drafts.expire(draft_now());
            self.writing_expire();
            if self.recovery_open && matches!(self.recovery_status, None | Some("copy-failed"))
                && self.drafts.recovery(draft_now()).is_none() {
                self.recovery_status = Some("expired");
                let message = self.menu_message(0);
                self.menu.redraw(&self.app, |seq| MenuMessage { seq, ..message });
            }
            let delay=self.drafts.until_expiry(draft_now()).into_iter().chain(self.writing_timeout()).min();
            let control = if let Some(delay) = delay {
                match rx.recv_timeout(delay.min(SECURE_POLL)) {
                    Ok(control) => control,
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Control::DraftExpiry,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            } else {
                match rx.recv() { Ok(control) => control, Err(_) => break }
            };
            match control {
                Control::WritingReply {client,id,action,payload}=>self.writing_reply(client,id,action,payload),
                Control::App(AppMessage::Writing {..})=>{}, // connection identity is retained above
                Control::LivePreview { seq, update } => {
                    if seq != self.capture_seq || !matches!(self.session.phase(), Phase::Listening { .. }) { continue; }
                    match update {
                        super::live_preview::Update::Words { text, sampled_at, decode_ms } => {
                            if sampled_at.elapsed() > Duration::from_secs(3) {
                                if let Some(t) = &self.timings { t.event("preview_paused", serde_json::json!({"reason":"control_delivery_stale"})); }
                                self.stop_live_preview();
                                self.live_preview_paused = true;
                            } else {
                                if let Some(t) = &mut self.timings { t.preview(decode_ms, sampled_at.elapsed().as_millis() as u64); }
                                self.live_words = Some(text);
                                log::line(&format!("preview decoded in {decode_ms} ms; recent window age {} ms", sampled_at.elapsed().as_millis()));
                            }
                        }
                        super::live_preview::Update::Paused { reason, input_ms } => {
                            if let Some(t) = &self.timings { t.preview_paused(reason, input_ms); }
                            self.stop_live_preview();
                            self.live_preview_paused = true;
                            log::line("preview paused for this recording; capture and final recognition continue");
                        }
                        super::live_preview::Update::Unavailable => {
                            if let Some(t) = &self.timings { t.event("preview_unavailable", serde_json::json!({"reason":"model_unavailable"})); }
                            self.stop_live_preview();
                            self.live_words = None;
                            self.live_preview_paused = false;
                        }
                        super::live_preview::Update::Quiet => {
                            if let Some(t) = &mut self.timings { t.preview_quiet(); }
                            self.live_words = None;
                        }
                    }
                    bar::preview(&self.app, self.bar.sequence(), self.live_words.as_deref(), self.live_preview_paused);
                }
                Control::DraftExpiry => {} // next loop expires and clears any open panel
                Control::Vocabulary { seq, client, publication } => {
                    // Only the owning data profile supplies company identities.
                    if std::path::Path::new(&publication.data_dir) != self.data_dir { continue; }
                    if self.writing_client.is_none_or(|id|!self.hub.connected(id)) {self.writing_client=Some(client);}
                    if seq != 0 && !self.handover.as_ref().is_some_and(|h| h.seq == seq && h.told.contains(&client)) { continue; }
                    let company_deleted = self.recording_vocabulary.as_ref().is_some_and(|recorded|
                        !publication.companies.iter().any(|c| c.id == recorded.company.id));
                    if company_deleted { self.cancel_recording(); self.publish(); }
                    if let Some(recorded) = &self.recording_vocabulary {
                        if publication.active.is_none() || !publication.companies.iter().any(|c| c.id == recorded.company.id) {
                            self.vocabulary_revoked.store(true, Ordering::Release);
                        }
                    }
                    if seq != 0 && !self.instruction_recording() {
                        self.recording_vocabulary = publication.active.clone();
                        self.vocabulary_revoked.store(false, Ordering::Release);
                        if matches!(self.session.phase(), Phase::Listening { .. }) { self.show_bar(BarView::Listening); }
                    }
                    if let Some(old) = &self.vocabulary {
                        for company in &old.companies {
                            if !publication.companies.iter().any(|c| c.id == company.id) { self.drafts.remove_company(&company.id); }
                        }
                    }
                    self.vocabulary = Some(publication);
                    let message = self.menu_message(0);
                    self.menu.redraw(&self.app, |seq| MenuMessage { seq, ..message });
                }
                Control::App(AppMessage::Vocabulary { .. }) => {} // carries its connection id above
                Control::App(AppMessage::ClearDrafts { data_dir }) => {
                    if std::path::Path::new(&data_dir) == self.data_dir {
                        self.cancel_assistance();self.cleanup_anchor=None;
                        self.cancel_recording();
                        self.drafts.clear();
                        self.close_menu();
                        self.publish();
                    }
                }
                Control::Tap(TapEvent::Toggle) | Control::App(AppMessage::Finish) => self.toggle(matches!(control, Control::App(_))),
                Control::Tap(TapEvent::WritingEscape)=>{
                    if self.instruction_recording() {self.cancel_recording();}
                    self.cancel_assistance();self.close_menu();self.publish();
                }
                Control::CleanerReady{id,result}=>{
                    if let Some(lookup)=self.cleaner_lookup.take(){lookup.stop_and_join();}
                    let Some((active,expected))=self.cleaner_target.as_ref() else{continue};if active!=&id{continue}
                    let good=result.as_ref().is_ok_and(|context|context["route"]=="file" && context["path"]==expected["path"] && context["identity"]["object"]==expected["identity"]["object"] && context["identity"]["changed"]==expected["identity"]["changed"]);
                    if good{if self.session.tap(now_ms())==TapAction::StartListening{self.listen();}}else{self.cancel_cleaner();self.hub.broadcast(&ToolMessage::Cleaner{id,action:"error".into(),payload:serde_json::json!({"message":"Focus or selection changed. Try again"})});}
                }
                Control::CleanerLookup{id,result}=>{
                    if let Some(lookup)=self.cleaner_lookup.take(){lookup.stop_and_join();}
                    if self.cleaner_pending.as_deref()!=Some(id.as_str()){continue;}
                    self.cleaner_pending=None;
                    match result {
                        Ok(context) if context["route"]=="text"=>{self.cleaner_target=None;self.hub.broadcast(&ToolMessage::Cleaner{id,action:"passthrough".into(),payload:serde_json::json!({})});if self.session.tap(now_ms())==TapAction::StartListening{self.listen();}},
                        Ok(context)=>{self.cleaner_target=Some((id.clone(),context.clone()));self.hub.broadcast(&ToolMessage::Cleaner{id,action:"target".into(),payload:context});},
                        Err(reason)=>{self.hub.broadcast(&ToolMessage::Cleaner{id,action:"error".into(),payload:serde_json::json!({"message":reason})});}
                    }
                }
                Control::App(AppMessage::Cleaner{id,action,payload})=>{
                    match action.as_str(){
                        "mode"=>self.cleaner_enabled=payload["enabled"]==true,
                        "release" if self.cleaner_target.as_ref().is_some_and(|(active,_)|active==&id)=>{self.cleaner_target=None;},
                        "service-target" if self.settings.on && !matches!(self.session.phase(),Phase::Listening{..}|Phase::Writing) && self.cleaner_target.is_none()=>{self.cleaner_target=Some((id.clone(),payload.clone()));self.hub.broadcast(&ToolMessage::Cleaner{id,action:"target".into(),payload});},
                        "record" if self.cleaner_target.as_ref().is_some_and(|(active,_)|active==&id)=>{
                            if self.cleaner_target.as_ref().is_some_and(|(_,context)|context["service"]==true){if self.session.tap(now_ms())==TapAction::StartListening{self.listen();}}
                            else{let tx=self.tx.clone();let stop=Arc::new(AtomicBool::new(false));let flag=stop.clone();let join=std::thread::spawn(move||{let result=crate::cleaner_native::Native::bundled().and_then(|native|native.context(&flag));if tx.send(Control::CleanerReady{id,result}).is_err(){log::line("RichCleaner target verification receiver ended");}});self.cleaner_lookup=Some(Writing{stop,join});}
                        },
                        "finish" if self.cleaner_target.as_ref().is_some_and(|(active,_)|active==&id) && matches!(self.session.phase(),Phase::Listening{..})=>self.toggle(true),
                        "cancel" if self.cleaner_pending.as_deref()==Some(id.as_str()) || self.cleaner_target.as_ref().is_some_and(|(active,_)|active==&id)=>{self.cancel_cleaner();if let Some(message)=payload["message"].as_str(){log::line(message);}},
                        _=>{}
                    }
                }

                Control::Tap(TapEvent::Disabled { by_timeout }) => {
                    self.disables += 1;
                    log::line(&format!(
                        "key tap disabled by {}; re-enabled",
                        if by_timeout { "timeout" } else { "user input" }
                    ));
                }
                Control::InputDied(seq) => {
                    if seq!=self.capture_seq {continue;}
                    if let Some(duration_ms) = self.session.input_died(now_ms()) {
                        log::line("the microphone delivered nothing for 3.008 s; listening ended");
                        self.stop(duration_ms);
                    }
                }
                Control::Written { seq, mut result } => {
                    if seq!=self.capture_seq {continue;}
                    if let Some(w) = self.writing.take() {
                        // Done: joined at once, so the thread is reaped and nothing is owned.
                        w.stop_and_join();
                    }
                    self.end_navigation();
                    if self.instruction_recording() {
                        self.instruction_done(result.instruction.take(),result.outcome);continue;
                    }
                    self.cleanup_anchor=result.anchor.take();
                    if let (Some(draft), Some(delivery)) = (result.draft.take(), result.delivery) {
                        let copied = if delivery == insert::Delivery::Check
                            && self.drafts.protects_unresolved(draft_now()) {
                            insert::copy_only(&draft.text).is_ok()
                        } else { matches!(delivery, insert::Delivery::NotAdded { copied: true, .. }) };
                        let retention = self.drafts.complete(draft, delivery.unresolved(), copied, draft_now());
                        self.notice = Some(bar::DeliveryMessage {
                            state: delivery.tag(), destination: self.destination_name.clone(),
                            copied: if delivery.unresolved() { Some(copied) } else { None },
                            earlier_draft: retention.earlier_unresolved,
                            replaced_copied_draft: retention.replaced_clipboard_only,
                            recoverable: self.drafts.recovery(draft_now()).is_some(),
                            destination_changed: matches!(delivery, insert::Delivery::NotAdded { destination_changed: true, .. }),
                        });
                    } else if result.delivery == Some(insert::Delivery::DiscardedSecure) {
                        self.secure_notice(true);
                    }
                    let outcome = result.outcome;
                    if let Some((id,_))=self.cleaner_target.take(){if let Err(problem)=outcome{self.hub.broadcast(&ToolMessage::Cleaner{id,action:"error".into(),payload:serde_json::json!({"message":problem.tag()})});}self.session.reset();}else{self.session.finish(outcome);}
                    self.publish();
                    if let (Ok(()), Some(words)) = (outcome, result.words) {
                        self.fly(words);
                    }
                }
                Control::Quit(why) => {
                    log::line(&format!("{why}; the tool ends"));
                    return self.end();
                }
                Control::Yielded { seq, client, bundle } => {
                    if let Some(h) = self.handover.as_mut() {
                        match h.accept(seq, client) {
                            Accept::Complete => {
                                let told = h.told.len();
                                self.handover = None;
                                log::line(&format!("every app ({told}) yielded the microphone for will-listen {seq}"));
                                self.open_microphone();
                            }
                            Accept::Counted => {}
                            verdict => log::line(&format!(
                                "a voice-yielded for will-listen {seq} from connection {client} ({bundle}) is dropped: {verdict:?}"
                            )),
                        }
                    } else {
                        log::line(&format!(
                            "a voice-yielded for will-listen {seq} from connection {client} ({bundle}) arrived with no handover open"
                        ));
                    }
                }
                Control::App(AppMessage::VoiceYielded { .. }) => {} // always arrives as Control::Yielded
                Control::HandoverTimeout(seq) => {
                    let due = self.handover.as_ref().is_some_and(|h| h.seq == seq);
                    if let Some(h) = due.then(|| self.handover.take()).flatten() {
                        if h.microphone_may_open() {
                            self.open_microphone();
                        } else {
                            // Fails closed (recheck finding 1): voice mode may still be
                            // listening in the app that did not answer, so dictation does not.
                            log::line(&format!(
                                "{} of {} app(s) answered will-listen {seq} within {} ms; the microphone stays closed, voice-still-listening",
                                h.heard.len(),
                                h.told.len(),
                                HANDOVER_WAIT.as_millis()
                            ));
                            self.session.finish(Err(Problem::VoiceStillListening));
                            self.publish();
                        }
                    }
                }
                Control::App(AppMessage::Change { on, key, model }) => {
                    // A copy that does not own the key changes the owner's file, here (finding 4).
                    match store::update(&self.data_dir, |s| {
                        if let Some(on) = on {
                            s.on = on;
                        }
                        if let Some(key) = key.filter(|k| (1..=19).contains(k)) {
                            s.key = key;
                        }
                        if let Some(model) = model.as_deref().and_then(model_for_choice_or_id) {
                            s.model = model.to_string();
                        }
                    }) {
                        Ok(s) => {
                            self.settings = s;
                            log::line("another copy of RichOS changed the settings; applied");
                            // on=false reaches every copy before the tool ends (recheck finding 2).
                            self.publish();
                            self.hub.broadcast(&ToolMessage::SettingsChanged);
                        }
                        Err(why) => log::line(&format!("another copy's change could not be saved: {why}")),
                    }
                    if !self.settings.on {
                        log::line("dictation was turned off by another copy of RichOS; the tool stops");
                        return self.end();
                    }
                    if let Some(tap) = &self.tap {
                        tap.set_key(self.settings.key());
                    }
                    self.publish();
                }
                Control::App(AppMessage::Hello { version, bundle, data_dir }) => {
                    log::line(&format!(
                        "an app connected: version {version}, {bundle}, data folder {data_dir} (this tool is {})",
                        self.version
                    ));
                }
                Control::App(AppMessage::SettingsChanged) | Control::App(AppMessage::PermissionsChanged) => {
                    match store::read(&self.data_dir) {
                        Ok(s) => self.settings = s,
                        Err(why) => log::line(&why),
                    }
                    if !self.settings.live_preview {
                        self.stop_live_preview();
                        self.live_words = None;
                        self.live_preview_paused = false;
                        bar::preview(&self.app, self.bar.sequence(), None, false);
                    }
                    if !self.settings.on {
                        log::line("dictation was turned off; the tool stops");
                        // on=false reaches every connected copy before the tool ends (recheck
                        // finding 2): a second copy's recovery never reverses this Off.
                        self.publish();
                        return self.end();
                    }
                    if let Some(tap) = &self.tap {
                        tap.set_key(self.settings.key());
                    }
                    self.ensure_tap();
                    self.publish();
                }
                // Key capture (slice 2): the window asked for the next F-key; the tap answers
                // with it instead of acting on it. Which key is never logged.
                Control::App(AppMessage::CaptureNextKey) => match &self.tap {
                    Some(tap) => tap.capture_next(true),
                    None => log::line("key capture was asked for with no key tap; the window's own keys answer"),
                },
                Control::App(AppMessage::CancelCapture) => {
                    if let Some(tap) = &self.tap {
                        tap.capture_next(false);
                    }
                }
                Control::Tap(TapEvent::Captured(key)) => {
                    log::line("key capture answered");
                    self.hub.broadcast(&ToolMessage::Key { key });
                }
                Control::Hour => {
                    let longest = self.tap.as_ref().map(|t| t.take_longest_micros()).unwrap_or(0);
                    log::line(&format!("key tap: longest callback {longest} us in the last hour; {} disable(s) so far", self.disables));
                }
                Control::Ui(event) => {
                    if let Some(code) = self.ui(event) {
                        return code;
                    }
                }
                Control::BarExpired(seq) => {
                    if self.bar.expire(&self.app, seq) {
                        if self.ending {
                            return self.end();
                        }
                        if matches!(self.session.phase(), Phase::Done | Phase::Problem(_)) {
                            self.session.reset();
                            self.publish();
                        }
                    }
                }
                Control::FlightDone(n) => {
                    if n == self.flights {
                        bar::land(&self.app);
                    }
                }
                Control::Secure(now) => {
                    match &now {
                        Some(Some(app)) => log::line(&format!("Secure Event Input is on, held by {app}: the key cannot reach the tap")),
                        Some(None) => log::line("Secure Event Input is on (macOS does not say which app): the key cannot reach the tap"),
                        None => log::line("Secure Event Input is off"),
                    }
                    self.secure = now;
                    self.publish();
                    if matches!(self.session.phase(), Phase::Listening { .. }) { self.show_bar(BarView::Listening); }
                    let m = self.menu_message(0);
                    self.menu.redraw(&self.app, |seq| MenuMessage { seq, ..m });
                }
            }
        }
        0
    }

    /// Stop everything and say the exit code. The writing worker, if one runs, is stopped (its
    /// decoder killed and reaped) and joined, so its scratch folder drops its files before the
    /// process ends (finding 1).
    fn end(&mut self) -> i32 {
        self.cancel_assistance();self.cleanup_anchor=None;
        if let Some(lookup)=self.cleaner_lookup.take(){lookup.stop_and_join();}

        self.capture = None;
        self.stop_live_preview();
        self.end_navigation();
        self.drafts.clear();
        self.tap = None;
        self.handover = None;
        if let Some(w) = self.writing.take() {
            log::line("a dictation was being written down: stopping its decoder and removing its recording");
            w.stop_and_join();
        }
        insert::settle();
        0
    }

    /// One event from a page or the menu bar item. `Some(code)` ends the tool.
    fn ui(&mut self, event: UiEvent) -> Option<i32> {
        match event {
            UiEvent::BarAction { seq, act } => {
                if self.bar.sequence() != seq { return None; }
                match act.as_str() {
                    "finish" if matches!(self.session.phase(), Phase::Listening { .. }) => self.toggle(true),
                    "cancel" if matches!(self.session.phase(), Phase::Listening { .. }) => {
                        self.cancel_recording();
                        self.cancel_assistance();
                        self.publish();
                    }
                    "recover" => { self.menu_act("recover", None); }
                    "dismiss" if self.bar.view() == Some(BarView::Delivery) => {
                        self.bar.expire(&self.app, seq);
                        self.session.reset();
                        self.notice = None;
                        self.publish();
                    }
                    _ => {}
                }
            }
            UiEvent::Ready { role } => {
                log::line(&format!("the {role} page is ready"));
                if role == "bar" {
                    if let Some(p) = self.preview {
                        // Held, with no timer: the check reads it, clicks it and ends the tool.
                        self.bar.show(&self.app, BarView::Problem(p), self.settings.key(), None, bar::look(&self.data_dir), bar::BarDetails::default());
                    }
                }
            }
            UiEvent::BarLaidOut { seq, w, h, orb, fix } => {
                let view = self.bar.view();
                if let Some(placed) = self.bar.laid_out(&self.app, seq, (w, h), orb, fix, self.focused) {
                    let f = placed.frame;
                    log::line(&format!(
                        "bar shown: {} at {:.0},{:.0} {:.0}x{:.0}{}",
                        view.map(|v| match v {
                            BarView::Problem(p) => format!("problem {}", p.tag()),
                            other => other.tag().to_string(),
                        })
                        .unwrap_or_default(),
                        f.x,
                        f.y,
                        f.w,
                        f.h,
                        placed.fix.map(|r| format!("; Fix it at {:.0},{:.0} {:.0}x{:.0}", r.x, r.y, r.w, r.h)).unwrap_or_default(),
                    ));
                }
            }
            UiEvent::FixIt => {
                if self.preview.is_some() {
                    log::line(&format!("Fix it pressed (preview: nothing opens); the front app is pid {:?}", appkit::frontmost_pid()));
                    return None;
                }
                log::line("Fix it pressed: RichOS opens on the Dictation sheet");
                self.bring_app(true);
                self.session.reset();
                self.publish();
            }
            UiEvent::ItemClicked { item } => {
                log::line("menu bar item pressed");
                if self.menu.is_open() {
                    self.cancel_assistance();
                    self.close_menu();
                } else if self.menu_closed_at.is_some_and(|t| t.elapsed() < REOPEN_GUARD) {
                    // The click that took the keyboard from the menu: it stays closed.
                } else {
                    let m = self.menu_message(0);
                    self.menu.open(&self.app, item, |seq| MenuMessage { seq, ..m });
                    log::line(&format!("menu opened from the menu bar item; the front app is pid {:?}", appkit::frontmost_pid()));
                }
            }
            UiEvent::MenuLaidOut { seq, w, h } => {
                if let Some(f) = self.menu.laid_out(&self.app, seq, (w, h), self.kind) {
                    log::line(&format!("menu shown at {:.0},{:.0} {:.0}x{:.0}", f.x, f.y, f.w, f.h));
                    self.writing_laid_out();
                }
            }
            UiEvent::Menu { act, value } => return self.menu_act(&act, value.as_deref()),
        }
        None
    }

    fn close_menu(&mut self) {
        self.vocabulary_expanded = false;
        self.recovery_open = false;
        self.recovery_status = None;
        if self.menu.close(&self.app, self.kind).is_some() || !self.menu.is_open() {
            self.menu_closed_at = Some(Instant::now());
            log::line(&format!("menu closed; the front app is pid {:?}", appkit::frontmost_pid()));
        }
    }

    fn menu_act(&mut self, act: &str, value: Option<&str>) -> Option<i32> {
        match act {
            "clean-up"=>self.begin_assistance(richos_core::writing::Mode::Cleanup),
            "rewrite"=>self.begin_assistance(richos_core::writing::Mode::Rewrite),
            action if action.starts_with("writing-")=>self.writing_action(action,value),
            "vocabulary-expand" | "vocabulary-collapse" => {
                self.vocabulary_expanded = act == "vocabulary-expand";
                let message = self.menu_message(0);
                self.menu.redraw(&self.app, |seq| MenuMessage { seq, ..message });
            }
            "recover" if !matches!(self.session.phase(), Phase::Listening { .. } | Phase::Writing) => {
                self.drafts.recovery(draft_now())?;
                self.recovery_open = true;
                self.recovery_status = None;
                let message = self.menu_message(0);
                if self.menu.is_open() {
                    self.menu.redraw(&self.app, |seq| MenuMessage { seq, ..message });
                } else if let Some(tray) = self.tray.clone() {
                    if let Some(item) = bar::ask_main(&self.app, move || menubar::item_rect(&tray)).flatten() {
                        self.menu.open(&self.app, item, |seq| MenuMessage { seq, ..message });
                    }
                }
            }
            "recovery-copy" if self.recovery_open => {
                if let Some(draft) = self.drafts.recovery(draft_now()) {
                    if insert::copy_only(&draft.text).is_ok() {
                        self.drafts.copied_recovery();
                        self.recovery_status = Some("copied");
                        self.notice = None;
                        self.session.reset();
                        self.publish();
                    } else { self.recovery_status = Some("copy-failed"); }
                } else { self.recovery_status = Some("expired"); }
                let message = self.menu_message(0);
                self.menu.redraw(&self.app, |seq| MenuMessage { seq, ..message });
            }
            "recovery-clear" if self.recovery_open => {
                self.cancel_assistance();self.cleanup_anchor=None;
                self.drafts.clear();
                self.close_menu();
                self.notice = None;
                self.session.reset();
                self.publish();
            }
            "vocabulary" => {
                let company = value.filter(|v| !v.is_empty()).map(str::to_string);
                if company.as_ref().is_none_or(|id| self.vocabulary.as_ref().is_some_and(|v| v.companies.iter().any(|c| &c.id == id))) {
                    self.hub.broadcast(&ToolMessage::SelectVocabulary { data_dir: self.data_dir.display().to_string(), company });
                }
            }
            "close" => {
                // A proposal is an explicit operation, ended by its identified
                // Cancel/Escape action. Native blur can belong to our own hide
                // before Replace, or the user checking/editing the destination.
                if self.assistance.is_none() && self.menu.is_open() {self.close_menu();}
            }
            "mode" => {
                let model = value.and_then(model_for_choice)?;
                if model != self.settings.model {
                    match store::update(&self.data_dir, |s| s.model = model.to_string()) {
                        Ok(s) => {
                            self.settings = s;
                            log::line(&format!("accuracy set to {model} from the menu bar; the next dictation uses it"));
                            self.hub.broadcast(&ToolMessage::SettingsChanged);
                        }
                        Err(why) => log::line(&format!("the accuracy could not be saved: {why}")),
                    }
                }
                let m = self.menu_message(0);
                self.menu.redraw(&self.app, |seq| MenuMessage { seq, ..m });
            }
            "settings" | "open" => {
                self.close_menu();
                let sheet = act == "settings";
                log::line(if sheet { "Dictation settings… chosen: RichOS opens on the Dictation sheet" } else { "Open RichOS chosen" });
                self.bring_app(sheet);
            }
            "onoff" => {
                self.close_menu();
                if !self.settings.on {
                    return None;
                }
                match store::update(&self.data_dir, |s| s.on = false) {
                    Ok(s) => {
                        self.settings = s;
                        log::line("Turn dictation off chosen in the menu bar; the tool ends once the bar has said so");
                        // Every connected copy hears on=false before the tool ends, so none
                        // starts a tool again that would undo this Off (recheck finding 2).
                        self.publish();
                        self.hub.broadcast(&ToolMessage::SettingsChanged);
                        self.cancel_recording();
                        self.tap = None;
                        self.show_bar(BarView::Off);
                        self.ending = true;
                    }
                    Err(why) => log::line(&format!("dictation could not be turned off: {why}")),
                }
            }
            other => log::line(&format!("the menu sent an unknown row: {other}")),
        }
        None
    }

    fn fly(&mut self, words: Rect) {
        let Some(placed) = self.bar.placed() else { return };
        self.flights += 1;
        if let Some(screen) = bar::fly(&self.app, placed.orb, words, bar::look(&self.data_dir)) {
            log::line(&format!(
                "the words flew to {:.0},{:.0} {:.0}x{:.0} on the screen at {:.0},{:.0}",
                words.x, words.y, words.w, words.h, screen.x, screen.y
            ));
            later(&self.tx, bar::FLIGHT_FOR, Control::FlightDone(self.flights));
        }
    }

    fn toggle(&mut self, from_app: bool) {
        if !from_app && self.assistance.is_some() && !matches!(self.session.phase(),Phase::Listening {..}) {
            if self.instruction_recording() {self.cancel_recording();}
            self.cancel_assistance();self.close_menu();self.session.reset();
        }
        // `finish` from the app writes a dictation that is listening and does nothing otherwise.
        if from_app && !matches!(self.session.phase(), Phase::Listening { .. }) {
            self.publish();
            return;
        }
        if self.ending {
            return;
        }
        if !from_app && !matches!(self.session.phase(),Phase::Listening{..}|Phase::Writing) && self.cleaner_enabled && insert::front_is_finder() {
            if self.cleaner_pending.is_some() || self.cleaner_target.is_some(){self.cancel_cleaner();return;}
            let id=richos_core::cleaner::capture_id();self.cleaner_pending=Some(id.clone());
            self.hub.broadcast(&ToolMessage::Cleaner{id:id.clone(),action:"checking".into(),payload:serde_json::json!({"key":self.settings.key()})});
            let tx=self.tx.clone();let stop=Arc::new(AtomicBool::new(false));let flag=stop.clone();
            let join=std::thread::spawn(move||{let result=crate::cleaner_native::Native::bundled().and_then(|native|native.context(&flag));if tx.send(Control::CleanerLookup{id,result}).is_err(){log::line("RichCleaner lookup receiver ended");}});
            self.cleaner_lookup=Some(Writing{stop,join});return;
        }
        match self.session.tap(now_ms()) {
            TapAction::StartListening => self.listen(),
            TapAction::StopListening { duration_ms } => self.stop(duration_ms),
            TapAction::Ignore => {}
        }
    }

    fn cancel_cleaner(&mut self){
        let id=self.cleaner_pending.take().or_else(||self.cleaner_target.as_ref().map(|(id,_)|id.clone()));
        if let Some(lookup)=self.cleaner_lookup.take(){lookup.stop_and_join();}
        self.cancel_recording();
        self.cleaner_target=None;self.publish();
        if let Some(id)=id{self.hub.broadcast(&ToolMessage::Cleaner{id,action:"canceled".into(),payload:serde_json::json!({})});}
    }
    fn listen(&mut self) {
        if !self.instruction_recording() {self.cancel_assistance();self.cleanup_anchor=None;}
        self.capture_seq+=1;
        self.close_menu();
        self.show_bar(BarView::Hidden);
        self.notice = None;
        self.live_words = None;
        self.live_preview_paused = false;
        self.destination = self.cleaner_target.is_none().then(insert::Destination::capture);
        self.destination_name = self.destination.as_ref().map(|d| d.name.clone()).unwrap_or_default();
        if self.destination.as_ref().is_some_and(|d| d.secure) {
            self.secure_notice(false);
            self.session.finish(Err(Problem::NoTextBox));
            self.publish();
            return;
        }
        self.navigated = self.tap.as_ref().map(|t| t.track_destination(true)).unwrap_or_else(|| Arc::new(AtomicBool::new(false)));
        if let Some(destination) = &self.destination {
            self.pointer_observer = keytap::PointerObserver::start(&self.app, destination.pid, self.navigated.clone());
            if self.pointer_observer.is_none() { log::line("destination pointer observer unavailable; unreadable-field navigation protection is unverified"); }
        }
        self.vocabulary_revoked = Arc::new(AtomicBool::new(false));
        self.recording_vocabulary = if self.instruction_recording() {None} else {self.vocabulary.as_ref().and_then(|v| v.active.clone())};
        self.focused = insert::focused_window_rect();
        // The accuracy for THIS dictation is the one chosen when it begins (finding 6).
        self.recording_model = self.settings.model.clone();
        // Iris's line 5a shows at the tap, before any talking: with neither speech model on this
        // Mac, nobody talks for a minute into nothing (dictation-more-lines.html, NOTES).
        if pick_model(&self.recording_model, |id| stt::resolve_model(id).is_ok()).id().is_none() {
            log::line("no speech model is on this Mac yet; nothing heard, model-missing");
            self.session.finish(Err(Problem::ModelMissing));
            self.publish();
            return;
        }
        // Voice mode ends first, so the two never listen at once (plan section 2): every
        // connected app is told, and the microphone opens once each has answered that it closed
        // voice mode (finding 2), or at the bound.
        self.publish();
        self.handovers += 1;
        let seq = self.handovers;
        let told = self.hub.broadcast_to(&ToolMessage::WillListen { seq });
        let handover = Handover::new(seq, told);
        if handover.complete() {
            self.open_microphone();
            return;
        }
        log::line(&format!(
            "will-listen {seq} sent to {} app(s); the microphone waits for each one's answer",
            handover.told.len()
        ));
        self.handover = Some(handover);
        later(&self.tx, HANDOVER_WAIT, Control::HandoverTimeout(seq));
    }

    /// The microphone, once every app has yielded it.
    fn open_microphone(&mut self) {
        if !matches!(self.session.phase(), Phase::Listening { .. }) {
            // The key was tapped again, or the input died, while the handover waited.
            return;
        }
        *self.recording.lock().unwrap_or_else(|p| p.into_inner()) = Some(Recording::new());
        let recording = self.recording.clone();
        let tx = self.tx.clone();
        let app = self.app.clone();
        let mut meter = MeterGate::default();
        let source = AudioSource::from_env();
        let capture_seq=self.capture_seq;
        let cleaner_id=self.cleaner_target.as_ref().map(|(id,_)|id.clone());let cleaner_hub=self.hub.clone();
        match capture::start(&source, move |frame| {
            let died = recording.lock().ok().and_then(|mut r| r.as_mut().map(|r| r.push(frame))).unwrap_or(false);
            if died {
                tx.send(Control::InputDied(capture_seq)).ok();
            }
            if meter.due(now_ms()) {
                if let Some(id)=&cleaner_id{cleaner_hub.broadcast(&ToolMessage::Cleaner{id:id.clone(),action:"level".into(),payload:serde_json::json!({"level":(meter_level(frame)*100.) as u32})});}else{bar::level(&app, meter_level(frame));}
            }
        }) {
            Ok(c) => {
                self.timings = Some(super::timings::Timings::new(self.capture_seq, &self.recording_model,
                    self.settings.live_preview && self.cleaner_target.is_none(), &self.version));
                if !self.instruction_recording() {self.drafts.recording_started(draft_now());}
                log::line(&format!("listening ({})", c.source_label));
                self.capture = Some(c);
                if self.settings.live_preview && self.cleaner_target.is_none() {
                    let recording = self.recording.clone();
                    let data_dir = self.data_dir.clone();
                    let sender = self.tx.clone();
                    let stop = Arc::new(AtomicBool::new(false));
                    let flag = stop.clone();
                    match std::thread::Builder::new().name("dictation-live-preview".into()).spawn(move || {
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            super::live_preview::run(recording, &data_dir, &flag,
                                |update| { sender.send(Control::LivePreview { seq: capture_seq, update }).ok(); });
                        }));
                        if result.is_err() && !flag.load(Ordering::Acquire) {
                            sender.send(Control::LivePreview { seq: capture_seq, update: super::live_preview::Update::Paused { reason: "worker_panicked", input_ms: None } }).ok();
                        }
                    }) {
                        Ok(join) => self.live_preview = Some(Writing { stop, join }),
                        Err(_) => {
                            self.live_preview_paused = true;
                            if let Some(t) = &self.timings { t.event("preview_paused", serde_json::json!({"reason":"worker_unavailable"})); }
                        },
                    }
                }
                if let Some((id,_))=&self.cleaner_target{self.hub.broadcast(&ToolMessage::Cleaner{id:id.clone(),action:"listening".into(),payload:serde_json::json!({"key":self.settings.key()})});}
            }
            Err(e) => {
                log::line(&format!("the microphone could not be opened: {e}"));
                self.session.finish(Err(Problem::NoMicrophone));
            }
        }
        self.publish();
    }

    fn stop(&mut self, duration_ms: u64) {
        let instruction=self.instruction_recording();
        let mut timings = self.timings.take();
        if let Some(t) = &mut timings { t.stop(); }
        // Dropping the capture closes the microphone.
        self.capture = None;
        self.stop_live_preview();
        if let Some(t) = &timings { t.event("preview_stopped", serde_json::json!({})); }
        // A key menu panel may still own the keyboard after a vocabulary or
        // accuracy choice. Hide it before final decode and the single paste.
        self.close_menu();
        self.handover = None;
        let recording = self.recording.lock().unwrap_or_else(|p| p.into_inner()).take().unwrap_or_default();
        self.publish();
        let data_dir = self.data_dir.clone();
        let model = self.recording_model.clone();
        let tx = self.tx.clone();
        let app = self.app.clone();
        let capture_seq=self.capture_seq;
        let target=self.cleaner_target.as_ref().map(|(id,_)|id.clone());let hub=self.hub.clone();
        let vocabulary = self.recording_vocabulary.clone();
        let vocabulary_revoked = self.vocabulary_revoked.clone();
        let destination = self.destination.take();
        let navigated = self.navigated.clone();
        if let Some(id)=&target{self.hub.broadcast(&ToolMessage::Cleaner{id:id.clone(),action:"writing".into(),payload:serde_json::json!({})});}
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        match std::thread::Builder::new().name("dictation-write".into()).spawn(move || {
            let mut timings = timings;
            let text = TextContext { instruction, timings: timings.as_ref(), cleaner: target.as_deref().map(|id|(id,hub.as_ref())),
                                     vocabulary: vocabulary.as_ref(), vocabulary_revoked: &vocabulary_revoked,
                                     destination: destination.as_ref(), navigated: &navigated };
            let result = write(recording, duration_ms, &model, &data_dir, &app, &flag, text);
            if let Some(t) = &mut timings {
                t.finished(result.delivery.as_ref().map(|d| d.tag()).unwrap_or_else(|| match result.outcome { Ok(()) => "completed", Err(p) => p.tag() }));
            }
            tx.send(Control::Written { seq:capture_seq, result }).ok();
        }) {
            Ok(join) => self.writing = Some(Writing { stop, join }),
            Err(e) => {
                log::line(&format!("the writing worker could not start: {e}"));
                self.session.finish(Err(Problem::CouldNotWrite));
                self.publish();
            }
        }
    }

    fn end_navigation(&mut self) {
        if let Some(tap) = &self.tap { tap.track_destination(false); }
        self.pointer_observer = None;
    }

    fn stop_live_preview(&mut self) {
        if let Some(preview) = self.live_preview.take() { preview.stop_and_join(); }
    }

    fn cancel_recording(&mut self) {
        self.timings.take();
        self.capture_seq += 1;
        self.capture = None;
        self.stop_live_preview();
        self.handover = None;
        if let Some(writing) = self.writing.take() { writing.stop_and_join(); }
        self.recording.lock().unwrap_or_else(|p| p.into_inner()).take();
        self.end_navigation();
        self.destination = None;
        self.live_words = None;
        self.live_preview_paused = false;
        self.notice = None;
        self.session.reset();
    }

    fn secure_notice(&mut self, captured: bool) {
        self.notice = Some(bar::DeliveryMessage { state: if captured { "discarded-secure" } else { "secure-field" }, destination: self.destination_name.clone(),
            copied: None, earlier_draft: false, replaced_copied_draft: false, recoverable: false, destination_changed: false });
    }
}

/// A model for `change`: an accuracy label (`accurate`, `fast`) or a model id the tool knows.
fn model_for_choice_or_id(value: &str) -> Option<&'static str> {
    model_for_choice(value).or_else(|| {
        [richos_voice::dictation::MORE_ACCURATE, richos_voice::dictation::FASTER].into_iter().find(|id| *id == value)
    })
}

/// V's key code, read on the main thread (Text Input Sources require it).
fn v_code_on_main(app: &tauri::AppHandle) -> u16 {
    let (tx, rx) = channel();
    if app.run_on_main_thread(move || { tx.send(insert::v_key_code()).ok(); }).is_err() {
        return 9;
    }
    rx.recv_timeout(Duration::from_secs(2)).unwrap_or(9)
}

/// One dictation, from the recording to the words in place, and where they landed on screen
/// when Accessibility says. Never logs the words.
struct TextContext<'a> {
    instruction: bool,
    timings: Option<&'a super::timings::Timings>,
    cleaner: Option<(&'a str, &'a Hub)>,
    vocabulary: Option<&'a super::vocabulary::Snapshot>,
    vocabulary_revoked: &'a AtomicBool,
    destination: Option<&'a insert::Destination>,
    navigated: &'a AtomicBool,
}

struct WriteResult {
    instruction: Option<String>,
    anchor: Option<insert::Anchor>,
    outcome: Result<(), Problem>,
    words: Option<Rect>,
    draft: Option<Draft>,
    delivery: Option<insert::Delivery>,
}

fn write(recording: Recording, duration_ms: u64, chosen: &str, data_dir: &std::path::Path, app: &tauri::AppHandle, stop: &AtomicBool, text: TextContext<'_>) -> WriteResult {
    write_words(recording, duration_ms, chosen, data_dir, app, stop, text).unwrap_or_else(|problem|
        WriteResult { instruction: None, anchor: None, outcome: Err(problem), words: None, draft: None, delivery: None })
}

/// Looks for the engine's decoder again when none is delivered, THEN picks the model: the pick's
/// own check needs the decoder, so the order is the fix.
fn pick_after_engine_lookup(
    chosen: &str,
    has_engine: impl Fn() -> bool,
    look: impl FnOnce(),
    verified: impl Fn(&str) -> bool,
) -> ModelPick {
    if !has_engine() {
        look();
    }
    pick_model(chosen, verified)
}

/// The words in place: `Ok` with the problem to say afterwards (if any) and what was pasted where.
fn write_words(recording: Recording, duration_ms: u64, chosen: &str, data_dir: &std::path::Path, app: &tauri::AppHandle, stop: &AtomicBool, text: TextContext<'_>) -> Result<WriteResult, Problem> {
    let timings = text.timings;
    let secs = recording.secs();
    let then = match judge_recording(duration_ms, &recording) {
        Stopped::Refuse(p) => {
            log::line(&format!("dictation of {secs:.2} s ({duration_ms} ms tapped): nothing written, {}", p.tag()));
            return Err(p);
        }
        Stopped::Transcribe { then } => then,
    };
    // **The engine is looked for again BEFORE the models are checked.** A tool whose single look
    // at its own start found no verified runtime (the engine was being replaced as the app
    // started, 2026-11-01 16:43Z) kept looking at nothing for good, because
    // `DELIVERED_RUNTIME_BIN` is only set at start; and `model_verified` resolves whisper-cli
    // first, so with no engine both models failed and this returned ModelMissing before any
    // later retry could run.
    let pick = pick_after_engine_lookup(
        chosen,
        || stt::delivered_runtime_bin_dir().is_some(),
        || log::line(&resolve_runtime_bin_alone()),
        |id| stt::model_verified(id).is_ok(),
    );
    if let ModelPick::Fallback { wanted, using } = &pick {
        log::line(&format!("{wanted} is not verified on this Mac yet; using {using} for this dictation"));
    }
    let Some(id) = pick.id() else {
        log::line("no speech model is verified on this Mac; nothing written, model-missing");
        return Err(Problem::ModelMissing);
    };
    let recognizer = stt::Recognizer::for_model(id).map_err(|e| {
        log::line(&format!("the recognizer for {id} could not be resolved: {e}"));
        Problem::ModelMissing
    })?;
    let samples = recording.into_samples();
    let bound = decode_bound(samples.len());
    if let Some(t) = timings { t.event("recognition_started", serde_json::json!({"model":id})); }
    let started = Instant::now();
    let transcript = {
        let scratch = Scratch::new(data_dir).map_err(|e| {
            log::line(&format!("a dictation's scratch folder could not be made: {e}"));
            Problem::CouldNotWrite
        })?;
        recognizer.transcribe_bounded_until(&samples, scratch.path(), bound, stop).map_err(|e| {
            let elapsed = started.elapsed();
            let reason = super::timings::decoder_failure_reason(&e);
            log::line(&format!("whisper-cli did not write the words (bound {} s): {reason}", bound.as_secs()));
            if let Some(t) = timings {
                t.event("recognition_failed", super::timings::recognition_failure_detail(id, elapsed, bound, &e));
            }
            Problem::CouldNotWrite
        })?
        // `scratch` is dropped here: the recording and the words on disk are removed, on a
        // stop too (`transcribe_bounded_until` returns at once with the decoder reaped).
    };
    if stop.load(Ordering::SeqCst) {
        return Err(Problem::CouldNotWrite);
    }
    let latency = started.elapsed().as_millis();
    if let Some(t) = timings { t.event("recognition_completed", serde_json::json!({"model":id,"recognition_ms":latency})); }
    let words = match judge_transcript(&transcript.0) {
        Ok(w) => w,
        Err(p) => {
            log::line(&format!("dictation: model {id}, {secs:.2} s of audio, decoded in {latency} ms: nothing meaningful, {}", p.tag()));
            return Err(then.unwrap_or(p));
        }
    };
    if text.instruction {
        if text.destination.is_some_and(|d|d.permission(false)==richos_voice::dictation::DeliveryPermission::DiscardSecure) {
            return Err(Problem::NoTextBox);
        }
        return Ok(WriteResult {instruction:Some(words),anchor:None,outcome:then.map_or(Ok(()),Err),words:None,draft:None,delivery:None});
    }
    if let Some((id,hub))=text.cleaner {
        if stop.load(Ordering::Acquire){return Err(Problem::CouldNotWrite)}
        if hub.broadcast(&ToolMessage::Cleaner{id:id.into(),action:"transcript".into(),payload:serde_json::json!({"note":words})})==0{return Err(Problem::CouldNotWrite)}
        return Ok(WriteResult { instruction: None, anchor: None, outcome: then.map_or(Ok(()), Err), words: None, draft: None, delivery: None });
    }
    let original = words;
    let words = if !text.vocabulary_revoked.load(Ordering::Acquire) {
        text.vocabulary.filter(|snapshot| {
            store::read(data_dir).ok().and_then(|s| super::vocabulary::Saved::read(&s.vocabulary).ok())
                .is_some_and(|s| s.selected_company.is_some() && s.by_company.contains_key(&snapshot.company.id))
        }).and_then(|snapshot| {
            // The owning app already checked the local English dictionary before
            // publishing. Re-check structure and protected common-word fixtures.
            richos_voice::dictation_vocabulary::Rules::checked(&snapshot.rules, |_| Some(false)).ok()
        }).map(|rules| rules.apply(&original)).unwrap_or_else(|| original.clone())
    } else { original.clone() };
    let destination_name = text.destination.map(|d| d.name.clone()).unwrap_or_else(|| "the destination app".into());
    let mut draft = Draft { original, text: words.clone(), destination: destination_name,
        company: text.vocabulary.map(|v| v.company.id.clone()), completed_ms: now_ms(), range: None };
    if stop.load(Ordering::Acquire) { return Err(Problem::CouldNotWrite); }
    if !keytap::accessibility_allowed() || text.destination.is_none() {
        let copied = insert::copy_only(&words).is_ok();
        log::line(&format!("dictation: model {id}, {secs:.2} s, {latency} ms: automatic delivery unavailable, copied={copied}"));
        return Ok(WriteResult { instruction: None, anchor: None, outcome: Err(Problem::NoAccessibility), words: None,
            draft: Some(draft), delivery: Some(insert::Delivery::NotAdded { copied, destination_changed: false }) });
    }
    let inserted = match insert::insert(&words, v_code_on_main(app), text.destination.expect("checked destination"), text.navigated, stop) {
        Ok(inserted) => inserted,
        Err(_) if stop.load(Ordering::Acquire) => return Err(Problem::CouldNotWrite),
        Err(_) => {
            log::line("the words could not be put in place; the recognized draft is held for recovery");
            return Ok(WriteResult { instruction: None, anchor: None, outcome: Err(Problem::CouldNotWrite), words: None, draft: Some(draft),
                delivery: Some(insert::Delivery::NotAdded { copied: false, destination_changed: false }) });
        }
    };
    if let Some(t) = timings { t.event("delivery_completed", serde_json::json!({"outcome":inserted.delivery.tag()})); }
    if inserted.delivery == insert::Delivery::DiscardedSecure {
        log::line("destination is secure; no paste, copy or retained draft");
        return Ok(WriteResult { instruction: None, anchor: None, outcome: Err(Problem::NoTextBox), words: None, draft: None, delivery: Some(inserted.delivery) });
    }
    log::line(&format!(
        "dictation: model {id}, {secs:.2} s of audio, written in {latency} ms, {}{}; {}; delivery={}",
        if inserted.how == Insert::Paste { "pasted" } else { "copied (no text box)" },
        if inserted.spaced { ", spacing read" } else { "" }, inserted.seen, inserted.delivery.tag(),
    ));
    let anchor=matches!(inserted.delivery,insert::Delivery::Added|insert::Delivery::Sent)
        .then(||inserted.landed.as_ref().and_then(|(start,actual)|text.destination
            .and_then(|destination|insert::Anchor::inserted(destination,*start,actual.clone())))).flatten();
    if let Some((start, actual)) = inserted.landed {
        draft.range = Some((start, actual.encode_utf16().count() as isize));
        draft.text = actual;
    }
    draft.completed_ms = now_ms();
    Ok(WriteResult { instruction: None, anchor, outcome: then.map_or(Ok(()), Err), words: inserted.rect,
                    draft: Some(draft), delivery: Some(inserted.delivery) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    /// A start that found no engine recovers: the lookup runs BEFORE the models are checked, so a
    /// model check that needs the decoder (as `stt::model_verified` does) passes after it.
    #[test]
    fn the_engine_is_looked_for_before_the_models_are_checked() {
        use std::cell::Cell;
        let found = Cell::new(false);
        let verified = |_: &str| found.get();
        let none = pick_after_engine_lookup("large-v3-turbo-q5_0", || found.get(), || {}, verified);
        assert!(none.id().is_none(), "control: without the lookup, no model verifies");
        let recovered = pick_after_engine_lookup("large-v3-turbo-q5_0", || found.get(), || found.set(true), verified);
        assert!(recovered.id().is_some(), "the lookup must run before the models are checked");
        let calls = Cell::new(0);
        let has = pick_after_engine_lookup("large-v3-turbo-q5_0", || true, || calls.set(1), |_| true);
        assert!(has.id().is_some() && calls.get() == 0, "an engine already delivered is not looked for again");
    }

    /// INVARIANT: the tool takes exactly its two arguments. A data folder, when given, is
    /// absolute, so a child can never resolve a different folder from its app's; none at all is
    /// a launchd start (the plist names none), which computes the installed one.
    #[test]
    fn the_arguments() {
        assert_eq!(
            parse(&s(&["--data-dir", "/d", "--parent", "42"])).unwrap(),
            Args { data_dir: Some("/d".into()), parent: Some(42) }
        );
        assert_eq!(parse(&s(&["--data-dir", "/d"])).unwrap().parent, None);
        assert_eq!(parse(&s(&[])).unwrap(), Args { data_dir: None, parent: None }, "a launchd start");
        assert!(parse(&s(&["--data-dir"])).is_err());
        assert!(parse(&s(&["--data-dir", "relative"])).is_err());
        assert!(parse(&s(&["--data-dir", "/d", "--parent", "x"])).is_err());
        assert!(parse(&s(&["--data-dir", "/d", "--other"])).is_err());
    }

    /// INVARIANT: the window check's preview names a problem by its own tag, and nothing else
    /// turns it on.
    #[test]
    fn the_preview_is_a_problem_tag_or_nothing() {
        assert_eq!(preview_problem(Some("no-microphone")), Some(Problem::NoMicrophone));
        assert_eq!(preview_problem(Some(" model-missing ")), Some(Problem::ModelMissing));
        assert_eq!(preview_problem(Some("listening")), None);
        assert_eq!(preview_problem(Some("")), None);
        assert_eq!(preview_problem(None), None);
    }

    /// INVARIANT: the state on the wire says Secure Event Input and its app, as the menu shows it,
    /// and the key and model the tool actually uses (finding 4).
    #[test]
    fn the_state_carries_secure_event_input() {
        let shared = Shared { data_dir: String::new(), owner: "o".into(), on: true, phase: Phase::Idle, key_tap: true, secure: Some(Some("1Password".into())), key: 5, model: "small.en".into() };
        match shared.message() {
            ToolMessage::State { secure_input, secure_app, key, model, .. } => {
                assert!(secure_input);
                assert_eq!(secure_app.as_deref(), Some("1Password"));
                assert_eq!((key, model.as_str()), (5, "small.en"));
            }
            other => panic!("{other:?}"),
        }
        let unnamed = Shared { secure: Some(None), ..shared };
        assert!(matches!(unnamed.message(), ToolMessage::State { secure_input: true, secure_app: None, .. }));
    }

    /// INVARIANT (finding 2): the microphone opens only once every app told has answered; with
    /// nobody to tell it opens at once.
    #[test]
    fn the_handover_waits_for_every_answer() {
        let mut h = Handover::new(1, vec![1, 2]);
        assert!(!h.complete());
        h.accept(1, 1);
        assert!(!h.complete());
        h.accept(1, 2);
        assert!(h.complete());
        assert!(Handover::new(2, Vec::new()).complete(), "no app connected: nothing to wait for");
    }

    /// INVARIANT (recheck finding 1): at the bound, an unanswered will-listen keeps the
    /// microphone closed; it opens only once every app has yielded, or with no app connected.
    #[test]
    fn the_handover_fails_closed_at_the_bound() {
        let two = || Handover::new(1, vec![1, 2]);
        let mut one_silent = two();
        assert_eq!(one_silent.accept(1, 1), Accept::Counted);
        assert!(!one_silent.microphone_may_open(), "one app silent: closed");
        assert!(!Handover::new(1, vec![1]).microphone_may_open(), "the only app silent: closed");
        let mut both = two();
        both.accept(1, 1);
        assert_eq!(both.accept(1, 2), Accept::Complete);
        assert!(both.microphone_may_open(), "every app yielded");
        assert!(Handover::new(1, Vec::new()).microphone_may_open(), "no app connected: nobody to wait for");
    }

    /// INVARIANT (third and fourth reviews, finding 1): an answer counts only for its own
    /// handover and its own connection. The third review's counterexample: A's late answer to
    /// a timed-out handover plus A's current answer counted as two apps while B had not
    /// yielded. The fourth's: two processes from one bundle were one name; here they are
    /// connections 1 and 2, and one answer never completes for both.
    #[test]
    fn an_answer_counts_only_for_its_handover_and_its_connection() {
        let mut h = Handover::new(2, vec![1, 2]);
        assert_eq!(h.accept(1, 1), Accept::Stale, "A's late answer to handover 1");
        assert_eq!(h.accept(2, 1), Accept::Counted, "A's answer to this one");
        assert_eq!(h.accept(2, 1), Accept::Duplicate, "A again stands for nobody else");
        assert_eq!(h.accept(2, 3), Accept::Unknown, "a connection that was not told");
        assert!(!h.microphone_may_open(), "B has not yielded: closed");
        assert_eq!(h.accept(2, 2), Accept::Complete);
        assert!(h.microphone_may_open());
    }

    /// INVARIANT (fourth review, finding 1): two processes started from one bundle are two
    /// connections, and one answer never completes the handover for both. RED on 31f787bae
    /// (the handover named the copies told by bundle path, so the two were one name: told 1,
    /// not 2); GREEN here, where the Hub names each connection it accepted by its own id.
    #[test]
    fn two_processes_from_one_bundle_are_two_connections() {
        let hub = ipc::Hub::default();
        let (a, _a_peer) = std::os::unix::net::UnixStream::pair().unwrap();
        let (b, _b_peer) = std::os::unix::net::UnixStream::pair().unwrap();
        let a = hub.add(a);
        let b = hub.add(b);
        assert_ne!(a, b, "two connections, two ids, whatever bundle each process came from");
        let told = hub.broadcast_to(&ToolMessage::WillListen { seq: 3 });
        let mut h = Handover::new(3, told);
        assert_eq!(h.told.len(), 2, "two connections told, two to wait for");
        assert_eq!(h.accept(3, a), Accept::Counted, "the first process answers");
        assert!(!h.microphone_may_open(), "the other process from the same bundle has not yielded");
        assert_eq!(h.accept(3, a), Accept::Duplicate, "the first again stands for nobody else");
        assert_eq!(h.accept(3, b), Accept::Complete);
        assert!(h.microphone_may_open());
    }

    /// INVARIANT (finding 1): a writing worker told to stop ends with its child gone and is
    /// joined; `end` owns that, so an ordinary quit leaves no decoder and no recording.
    #[test]
    fn a_stopped_writing_worker_is_joined_with_its_decoder_gone() {
        let dir = std::env::temp_dir().join(format!("richos-dictation-writing-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let scratch = Scratch::new(&dir).unwrap();
        let folder = scratch.path().to_path_buf();
        let pid_file = dir.join("decoder.pid");
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let join = std::thread::spawn(move || {
            let mut cmd = std::process::Command::new("/bin/sh");
            cmd.args(["-c", "echo $$ > \"$1\"; exec /bin/sleep 30", "x"]).arg(&pid_file);
            let r = stt::bounded_decoder_until(&mut cmd, scratch.path(), Duration::from_secs(30), Some(&flag));
            assert_eq!(r.unwrap_err().kind(), std::io::ErrorKind::Interrupted);
            // `scratch` drops here: the folder goes with the decoder's files.
        });
        // load-bound: a hang guard only, never the verdict; the child's own pid file is the fact waited for.
        let deadline = Instant::now() + Duration::from_secs(30);
        let pid = loop {
            // Shell redirection creates the file before echo writes its PID. Existence
            // alone can observe an empty file while the decoder is still starting.
            if let Some(pid) = std::fs::read_to_string(dir.join("decoder.pid")).ok()
                .and_then(|text| text.trim().parse::<i32>().ok()).filter(|pid| *pid > 0) {
                break pid;
            }
            assert!(Instant::now() < deadline, "the decoder did not publish a valid PID");
            // load-bound: only a hang guard; the published PID and joined child decide correctness.
            std::thread::sleep(Duration::from_millis(10));
        };
        // The verdict is the worker's own error (Interrupted, asserted inside it: only the stop
        // produces it, never the 30 s deadline) and the child being gone; no clock is read.
        Writing { stop, join }.stop_and_join();
        // SAFETY: signal 0 asks whether the child this test started still exists.
        assert_ne!(unsafe { libc::kill(pid, 0) }, 0, "the decoder (pid {pid}) is gone");
        assert!(!folder.exists(), "the scratch folder {} is gone", folder.display());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// INVARIANT (finding 4): a second copy's change names a model by label or by id, and
    /// nothing else; an unknown value changes nothing.
    #[test]
    fn a_change_names_a_model_by_label_or_id() {
        assert_eq!(model_for_choice_or_id("fast"), Some("small.en"));
        assert_eq!(model_for_choice_or_id("accurate"), Some("large-v3-turbo-q5_0"));
        assert_eq!(model_for_choice_or_id("small.en"), Some("small.en"));
        assert_eq!(model_for_choice_or_id("tiny.en"), None);
    }
}

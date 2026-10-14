//! **Meeting notes in the app** (Granola PRD V4, richos-hq
//! `docs/prds/2026-10-12-granola-replacement.md`; the CEO's chosen design, round 25.1).
//!
//! The decision of what to do lives in `richos_core::meeting::MeetingMonitor` (pure, unit-tested).
//! This module is its edges, and only those:
//!
//! - **The watcher**: `richos-companion watch`, started at launch, excluding this app's own
//!   microphone use (voice, dictation). Its lines go to the monitor. It exits when the pipe this
//!   module holds closes, so it never outlives the app; if it ends by itself (EOF, a positive
//!   signal) it is started again, never assumed alive from silence.
//! - **The clock**: one tick a second, which is how a call's end is noticed after the grace interval.
//! - **The panel**: the floating window (`ui/meeting-panel.html`) at the top right of the display,
//!   over the meeting app, never taking the keyboard: the prompt, the voice bar, the recording
//!   indicator, a problem beside the call.
//! - **The capture**: `richos-companion capture --json`, started on the one click, stopped with
//!   SIGINT when the call ends (or on Stop). Its JSON lines say when it is really recording, and
//!   any failure, so the app can say so instead of leaving it on stderr. Its own pid is excluded
//!   from the monitor: RichOS holding the microphone is never "the call". Like the watcher, it
//!   holds a pipe from this app (`--end-with-stdin`): when RichOS quits, is relaunched for an
//!   update or crashes, the pipe closes and the recorder finishes its files itself. Its
//!   `meeting.json` is written the moment it starts, so the next launch finds a recording cut
//!   short and still makes its notes ([`quit`], [`meeting_notes::cut_short`]).
//!
//! What happens to a finished recording (transcript, notes, the brief through Rich) is
//! [`Meetings::recording_closed`]'s job.

use crate::dictation::appkit::{self, Win};
use richos_core::ledger::AttentionTier;
use richos_core::meeting::{parse_watch_line, Action, EndReason, Input, MeetingMonitor, PromptKind, WatchLine};
use richos_core::meeting_calendar::{self, CalendarMeeting};
use richos_core::meeting_notes::{self, meeting_zone, MeetingFacts, NextStep};
use richos_core::util::now_millis;
use richos_voice::dictation_bar::Rect;
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

/// The floating panel's window label, and the events its page and the main window listen to.
pub const PANEL: &str = "meeting-panel";
pub const EVENT_PANEL: &str = "meeting-panel";
pub const EVENT_STATE: &str = "meeting-state";
/// The voice bar's level, about 20 a second while the bar is up: `{"rms": .., "peak": ..}`.
pub const EVENT_LEVEL: &str = "meeting-level";

/// One calendar sync pass is bounded, so a stuck network never holds the next one back.
const CALENDAR_SYNC_DEADLINE: Duration = Duration::from_secs(3 * 60);
/// A meeting's transcription is bounded (whisper on Apple silicon measured 249 s for a 92-minute
/// channel, `config.js`; two channels of an hour stay well inside this).
const TRANSCRIBE_DEADLINE: Duration = Duration::from_secs(60 * 60);
/// One brief attempt (a bug report's is 90 s; an hour of transcript needs longer).
const BRIEF_DEADLINE: Duration = Duration::from_secs(5 * 60);
/// Between failed brief attempts (PRD V4: up to three attempts per session).
const RETRY_AFTER: [Duration; 2] = [Duration::from_secs(30), Duration::from_secs(120)];
/// How long a quit waits for a running recorder to finish its files (its SIGINT finish drains,
/// flushes and rewrites `session.json`: one pump and one fsync).
const QUIT_CLOSE_WAIT: Duration = Duration::from_secs(3);
/// How long a launch waits for a previous launch's recorder to close before making its notes.
const RECORDER_CLOSE_WAIT: Duration = Duration::from_secs(10);

/// The panel's place on the display (round 25.1: `top:35px;right:14px` under a 25px menu bar):
/// its right edge 14 points in from the display's visible right, its top 10 below the menu bar.
/// The page reports its box with the room for its shadow (`meeting-panel.js` `PAD`), so these are
/// the WINDOW's offsets: the panel sits 10 in from the window's right and 6 down from its top.
const PANEL_RIGHT_INSET: f64 = 14.0 - 10.0;
const PANEL_TOP_INSET: f64 = 10.0 - 6.0;

/// What the panel shows. Serialized for `meeting-panel.js` `render`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "view", rename_all = "kebab-case")]
pub enum PanelView {
    Hidden,
    Prompt { title: String, sub: Option<String> },
    Starting,
    Recording {
        #[serde(rename = "startedAt")]
        started_at: u64,
        note: Option<String>,
    },
    Problem { title: String, sub: Option<String>, actions: Vec<PanelAction> },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PanelAction {
    pub label: String,
    pub act: String,
    pub primary: bool,
}

/// What the main window shows about meeting notes (Settings rows, the paused chip, the quiet
/// status in the conversation).
#[derive(Debug, Clone, Serialize)]
pub struct MeetingState {
    pub available: bool,
    pub paused: bool,
    pub recording: bool,
    #[serde(rename = "startedAt")]
    pub started_at: Option<u64>,
    /// "preparing", "transcript-failed", "notes-failed", or none.
    pub processing: Option<String>,
}

/// One recording, from the click to its closed session.
#[derive(Debug, Clone)]
struct CaptureRun {
    pid: u32,
    started_at: u64,
    title: Option<String>,
    event_id: Option<String>,
    manual: bool,
    zone: PathBuf,
    entity: Option<String>,
    thread: Option<String>,
    /// The session directory, once the capture says where it is writing.
    dir: Option<PathBuf>,
    /// The capture reported `started`: audio is really being written.
    recording: bool,
    stop: Option<EndReason>,
    /// A side that stalled during the recording, in the capture's own words: the notes must say
    /// the capture is partial instead of reading as complete (PRD V4).
    warnings: Vec<String>,
    /// The recorder's stdin. Held for as long as the recorder runs: it closes with this app,
    /// however the app ends, and the recorder then finishes its files by itself.
    _stdin: Option<Arc<ChildStdin>>,
}

impl CaptureRun {
    /// What the app knows about this recording, as `meeting.json` holds it. `ended_at` 0: the
    /// recording has not ended yet (written at its start, so a quit or crash leaves it findable).
    fn facts(&self, ended_at: u64) -> MeetingFacts {
        MeetingFacts {
            title: self.title.clone(),
            event_id: self.event_id.clone(),
            date_line: date_line(self.started_at),
            started_at: self.started_at,
            ended_at,
            stopped_early: self.stop == Some(EndReason::StoppedEarly),
            manual: self.manual,
            entity: self.entity.clone(),
            thread: self.thread.clone(),
            warnings: self.warnings.clone(),
        }
    }
}

/// The recorder with this pid, out of the current slot or the ones still finishing after a
/// stop, and whether it was the current one. A stopped recorder drains for seconds after its
/// SIGINT, and the next call's prompt can be answered inside them: its lines and its exit must
/// still find it, and must not touch the recording that started meanwhile.
fn take_run(capture: &mut Option<CaptureRun>, draining: &mut Vec<CaptureRun>, pid: u32) -> Option<(CaptureRun, bool)> {
    if capture.as_ref().is_some_and(|c| c.pid == pid) {
        return capture.take().map(|run| (run, true));
    }
    let i = draining.iter().position(|r| r.pid == pid)?;
    Some((draining.remove(i), false))
}

struct Inner {
    monitor: MeetingMonitor,
    view: PanelView,
    /// The recording now running (Settings' Stop, the voice bar), if any.
    capture: Option<CaptureRun>,
    /// Recorders told to stop that are still finishing their files ([`take_run`]).
    draining: Vec<CaptureRun>,
    processing: Option<String>,
    /// Holding this keeps the watcher's stdin open; dropping it ends the watcher.
    watcher_stdin: Option<ChildStdin>,
    watcher_pid: Option<u32>,
    /// A capture has shown the System Audio Recording grant exists (sound came through its tap).
    /// Only then may the voice bar read the call's level: starting a tap before he has decided
    /// would put the permission dialog up from a prompt he declined.
    system_audio_seen: bool,
    /// The voice bar's level reader (`richos-companion level`) while the bar is up.
    level: Option<LevelRun>,
    /// The synced calendar's meetings that may prompt, as of the last refresh.
    calendar: Vec<CalendarMeeting>,
    /// The panel's last reported size, so the log says a size once, not every frame of a fold.
    last_layout: Option<(i64, i64, bool)>,
}

/// The level reader: its pid (to end it) and its stdin (dropping it also ends it).
struct LevelRun {
    pid: u32,
    _stdin: Option<ChildStdin>,
}

pub struct Meetings {
    app: AppHandle,
    companion: PathBuf,
    /// `<app data>/meeting-notes.json`: what this Mac has shown about meeting notes (the grant).
    marker: PathBuf,
    inner: Mutex<Inner>,
}

/// The capture companion this build carries: `RICHOS_COMPANION_BIN` for a developer, else the
/// executable beside the app's own (`Contents/MacOS/richos-companion`, the bundled sidecar).
pub fn companion_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("RICHOS_COMPANION_BIN").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(p);
    }
    let exe = std::env::current_exe().ok()?;
    let beside = exe.parent()?.join("richos-companion");
    beside.is_file().then_some(beside)
}

/// Start meeting notes for this launch: the managed state, the watcher and the clock. A build
/// without the companion says so once and runs without meeting notes.
pub fn start(app: &AppHandle) {
    let Some(companion) = companion_path() else {
        eprintln!("[richos] meeting notes: no capture companion beside this app; meeting prompts are off");
        return;
    };
    eprintln!("[richos] meeting notes: companion {}", companion.display());
    let marker = app.state::<crate::AppState>().data_dir.join("meeting-notes.json");
    let remembered = std::fs::read(&marker)
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .unwrap_or_default();
    let system_audio_seen = remembered["systemAudioSeen"].as_bool().unwrap_or(false);
    let mut monitor = MeetingMonitor::new();
    // Paused stays paused across a relaunch, "until Resume or the switch" (round 25).
    let _nothing_to_hide = monitor.set_paused(remembered["paused"].as_bool().unwrap_or(false));
    let meetings = Arc::new(Meetings {
        app: app.clone(),
        companion,
        marker,
        inner: Mutex::new(Inner {
            monitor,
            view: PanelView::Hidden,
            capture: None,
            draining: Vec::new(),
            processing: None,
            watcher_stdin: None,
            watcher_pid: None,
            system_audio_seen,
            level: None,
            calendar: Vec::new(),
            last_layout: None,
        }),
    });
    app.manage(meetings.clone());
    // Meetings recorded before a quit, an update or a crash whose notes were never said: found
    // NOW, before the watcher can start this launch's first recording (whose meeting.json is
    // written at its start and must never be taken for a previous launch's), and finished one at
    // a time on a thread of their own (handover review rv-20261013T141401Z-390d6bea-2e23 #2).
    let unfinished = meeting_notes::unfinished_sessions(&meetings.corpus_root());
    let resume = meetings.clone();
    std::thread::Builder::new().name("meeting-resume".into()).spawn(move || resume.resume_unfinished(unfinished)).ok();
    let watcher = meetings.clone();
    std::thread::Builder::new().name("meeting-watcher".into()).spawn(move || watcher.run_watcher()).ok();
    let clock = meetings.clone();
    std::thread::Builder::new()
        .name("meeting-clock".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(1));
            let now = now_millis();
            let actions = {
                let mut guard = clock.inner.lock().unwrap();
                let inner = &mut *guard;
                inner.monitor.clock(now, &inner.calendar)
            };
            clock.apply(actions);
        })
        .ok();
    // The calendar: one pass at launch and every five minutes, one at a time (this one thread).
    let calendar = meetings.clone();
    std::thread::Builder::new()
        .name("meeting-calendar".into())
        .spawn(move || {
            let mut known = BTreeMap::new();
            loop {
                calendar.refresh_calendar(&mut known);
                std::thread::sleep(Duration::from_millis(meeting_calendar::REFRESH_MS));
            }
        })
        .ok();
}

impl Meetings {
    /// What survives a relaunch (`meeting-notes.json` in the data folder): whether the
    /// system-audio grant has been seen, and whether prompts are paused.
    fn remember(&self, inner: &Inner) -> std::io::Result<()> {
        let body = serde_json::json!({ "systemAudioSeen": inner.system_audio_seen, "paused": inner.monitor.paused() });
        std::fs::write(&self.marker, format!("{body}\n"))
    }

    // ---- the calendar ------------------------------------------------------------------------

    /// One calendar pass: `workspace sync <vendor> --once --source calendar` for each connected
    /// vendor (the existing sync and its Keychain grant), then the latest revisions read back.
    /// A failed or unconnected sync is logged and never blocks microphone prompts or capture; the
    /// meetings already synced still prompt. A canceled or moved meeting's prompt is withdrawn.
    fn refresh_calendar(&self, known: &mut BTreeMap<String, u64>) {
        let zone = meeting_calendar::workspace_zone(&self.corpus_root());
        let vendors = meeting_calendar::connected_vendors(&zone);
        if vendors.is_empty() {
            return;
        }
        match service_cli() {
            Some(service) => {
                for vendor in &vendors {
                    if let Err(why) = self.sync_calendar(&service, vendor) {
                        eprintln!("[richos] meeting notes: calendar sync ({vendor}) did not complete: {why}");
                    }
                }
            }
            None => eprintln!("[richos] meeting notes: no calendar sync in this build; using what was synced before"),
        }
        let current = meeting_calendar::meetings(&zone);
        let gone = meeting_calendar::withdrawn(known, &current);
        *known = current.iter().map(|m| (m.event_id.clone(), m.starts_at)).collect();
        let actions = {
            let mut inner = self.inner.lock().unwrap();
            inner.calendar = current;
            gone.iter().flat_map(|id| inner.monitor.withdraw(id)).collect::<Vec<_>>()
        };
        self.apply(actions);
    }

    /// One bounded sync pass for one vendor, with the installed runtime and the CEO's corpus.
    fn sync_calendar(&self, service: &Path, vendor: &str) -> Result<(), String> {
        let mut command = Command::new(node_bin());
        command
            .arg(service)
            .args(["workspace", "sync", vendor, "--once", "--source", "calendar"])
            .env("LORO_CORPUS", self.corpus_root())
            .env_remove("RICHOS_ACTIVE_COMPANY")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        installed_paths(&mut command);
        let mut child = command.spawn().map_err(|e| e.to_string())?;
        let started = std::time::Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if started.elapsed() < CALENDAR_SYNC_DEADLINE => std::thread::sleep(Duration::from_millis(250)),
                _ => {
                    logged("stopping the calendar sync", child.kill());
                    logged("reaping the calendar sync", child.wait());
                    return Err("it took longer than three minutes".into());
                }
            }
        }
        let out = child.wait_with_output().map_err(|e| e.to_string())?;
        let said = String::from_utf8_lossy(&out.stdout);
        let last = said.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").to_string();
        if out.status.success() {
            Ok(())
        } else {
            Err(format!("{} ({last})", out.status))
        }
    }

    // ---- the watcher -----------------------------------------------------------------------

    fn run_watcher(self: Arc<Self>) {
        let mut backoff = Duration::from_secs(2);
        loop {
            let started = std::time::Instant::now();
            match self.watch_once() {
                Ok(()) => eprintln!("[richos] meeting notes: the watcher ended; starting it again"),
                Err(e) => eprintln!("[richos] meeting notes: the watcher could not run: {e}"),
            }
            if started.elapsed() > Duration::from_secs(60) {
                backoff = Duration::from_secs(2);
            }
            std::thread::sleep(backoff);
            backoff = (backoff * 2).min(Duration::from_secs(60));
        }
    }

    fn watch_once(&self) -> Result<(), String> {
        let mut child = Command::new(&self.companion)
            .args(["watch", "--exclude-pid", &std::process::id().to_string(), "--exclude-bundle", "com.richos.app"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        {
            let mut inner = self.inner.lock().unwrap();
            inner.watcher_stdin = child.stdin.take();
            inner.watcher_pid = Some(child.id());
        }
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            let Some(parsed) = parse_watch_line(&line) else { continue };
            if let WatchLine::Error { message } = &parsed {
                eprintln!("[richos] meeting notes: watcher: {message}");
                continue;
            }
            match &parsed {
                WatchLine::Started { pid, family, .. } => eprintln!("[richos] meeting notes: microphone in use: {family} (pid {pid})"),
                WatchLine::Stopped { pid, family, .. } => eprintln!("[richos] meeting notes: microphone released: {family} (pid {pid})"),
                _ => {}
            }
            let actions = self.inner.lock().unwrap().monitor.watch(&parsed, now_millis());
            self.apply(actions);
        }
        // EOF: the watcher ended (a positive signal). Reap it, forget its pipe.
        let status = child.wait().map_err(|e| e.to_string())?;
        let mut inner = self.inner.lock().unwrap();
        inner.watcher_stdin = None;
        inner.watcher_pid = None;
        if status.success() { Ok(()) } else { Err(format!("it exited with {status}")) }
    }

    // ---- what the monitor decided ------------------------------------------------------------

    fn apply(&self, actions: Vec<Action>) {
        for action in actions {
            // One line per decision, so a walk's app log says what the monitor did and when.
            eprintln!("[richos] meeting notes: {action:?}");
            match action {
                Action::ShowPrompt(kind) => {
                    let view = match kind {
                        PromptKind::Unscheduled { app } => {
                            PanelView::Prompt { title: format!("Call detected in {}.", friendly_app(&app)), sub: None }
                        }
                        PromptKind::Scheduled { title, starts_at, .. } => {
                            PanelView::Prompt { title, sub: Some(starts_line(starts_at, now_millis())) }
                        }
                    };
                    self.show(view);
                }
                Action::HidePrompt => {
                    let prompting = matches!(self.inner.lock().unwrap().view, PanelView::Prompt { .. } | PanelView::Problem { .. });
                    if prompting {
                        self.show(PanelView::Hidden);
                    }
                }
                Action::StartCapture { title, event_id, manual } => self.start_capture(title, event_id, manual),
                Action::StopCapture(reason) => self.stop_capture(reason),
            }
        }
        self.publish_state();
    }

    fn show(&self, view: PanelView) {
        if !matches!(view, PanelView::Prompt { .. } | PanelView::Problem { .. }) {
            self.set_level(false);
        }
        self.inner.lock().unwrap().view = view.clone();
        self.ensure_panel();
        // Hidden too: the page fades the panel out and then reports 0 x 0, which hides the window.
        logged("the panel event", self.app.emit_to(PANEL, EVENT_PANEL, &view));
    }

    pub fn state(&self) -> MeetingState {
        let inner = self.inner.lock().unwrap();
        MeetingState {
            available: true,
            paused: inner.monitor.paused(),
            recording: inner.capture.is_some(),
            started_at: inner.capture.as_ref().filter(|c| c.recording).map(|c| c.started_at),
            processing: inner.processing.clone(),
        }
    }

    fn publish_state(&self) {
        logged("the meeting-state event", self.app.emit(EVENT_STATE, self.state()));
    }

    // ---- the panel window --------------------------------------------------------------------

    /// Build the panel window once: transparent, no decorations, above every app on every Space,
    /// a nonactivating panel that takes clicks without taking the keyboard from the meeting app.
    fn ensure_panel(&self) {
        if self.app.get_webview_window(PANEL).is_some() {
            return;
        }
        let built = WebviewWindowBuilder::new(&self.app, PANEL, WebviewUrl::App("meeting-panel.html".into()))
            .title("RichOS meeting notes")
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .resizable(false)
            .focusable(false)
            .focused(false)
            .always_on_top(true)
            .visible_on_all_workspaces(true)
            .skip_taskbar(true)
            .accept_first_mouse(true)
            .visible(false)
            .inner_size(420.0, 200.0)
            .build();
        match built {
            Ok(w) => {
                eprintln!("[richos] meeting notes: the panel window is built");
                if let Ok(ns) = w.ns_window() {
                    let win = Win(ns as *mut _);
                    logged("the panel's setup", self.app.run_on_main_thread(move || {
                        if let Err(why) = win.make_panel(false) {
                            eprintln!("[richos] meeting notes: the panel stays a plain window: {why}");
                        }
                        win.float_everywhere();
                    }));
                }
            }
            Err(e) => eprintln!("[richos] meeting notes: the panel window could not be built: {e}"),
        }
    }

    /// The page laid out a view: size and place the window. `w` and `h` include the shadow room.
    /// A first show goes to the top right of the display under the pointer; after that the panel's
    /// right edge and top stay where they are (the voice bar may have been dragged), except that a
    /// card coming back from the bar near the display's left edge opens to the right instead.
    pub fn laid_out(&self, w: f64, h: f64) {
        let Some(window) = self.app.get_webview_window(PANEL) else { return };
        let Ok(ns) = window.ns_window() else { return };
        let win = Win(ns as *mut _);
        let (hidden, changed) = {
            let mut inner = self.inner.lock().unwrap();
            let hidden = w <= 0.0 || h <= 0.0 || inner.view == PanelView::Hidden;
            let now = Some((w.round() as i64, h.round() as i64, hidden));
            let changed = inner.last_layout != now;
            inner.last_layout = now;
            (hidden, changed)
        };
        // Said when the size changes, not for every frame of a fold.
        if changed {
            eprintln!("[richos] meeting notes: the panel laid out {w:.0} x {h:.0}{}", if hidden { " (hidden)" } else { "" });
        }
        logged("placing the panel", self.app.run_on_main_thread(move || {
            if hidden {
                win.hide();
                return;
            }
            let screens = appkit::screen_rects();
            let pointer = appkit::pointer();
            let screen = screens
                .iter()
                .find(|(frame, _)| pointer.is_some_and(|p| frame.contains(p)))
                .or(screens.first())
                .map(|(_, visible)| *visible);
            let Some(visible) = screen else { return };
            let now = win.frame();
            let shown = now.w > 1.0 && now.h > 1.0 && visible.contains((now.x + now.w / 2.0, now.y + 1.0));
            let (right, top) = if shown && win.is_visible() {
                (now.x + now.w, now.y)
            } else {
                (visible.x + visible.w - PANEL_RIGHT_INSET, visible.y + PANEL_TOP_INSET)
            };
            let mut x = right - w;
            if x < visible.x + 8.0 {
                x = now.x.max(visible.x + 8.0); // no room on the left: open to the right
            }
            win.set_frame(Rect { x, y: top, w, h });
            win.show();
        }));
    }

    // ---- the voice bar's level ---------------------------------------------------------------

    /// The bar is up (`on`) or gone. While it is up, and only once the grant has been seen, the
    /// call's sound level is read live through the system tap and sent to the panel; nothing is
    /// recorded or kept (CEO, 2026-10-13: "move with the call's sound as designed"). Before the
    /// grant, or without a reader, the bar rests at the round's resting height.
    fn set_level(&self, on: bool) {
        let mut inner = self.inner.lock().unwrap();
        if !on {
            if let Some(run) = inner.level.take() {
                // SAFETY: our own child, not yet reaped (its reader thread reaps it).
                unsafe {
                    libc::kill(run.pid as i32, libc::SIGTERM);
                }
            }
            return;
        }
        if inner.level.is_some() || !inner.system_audio_seen {
            return;
        }
        let spawned = Command::new(&self.companion)
            .args(["level", "--exclude-pid", &std::process::id().to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match spawned {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[richos] meeting notes: the voice bar's level did not start: {e}");
                return;
            }
        };
        let pid = child.id();
        inner.level = Some(LevelRun { pid, _stdin: child.stdin.take() });
        drop(inner);
        let stdout = child.stdout.take();
        let app = self.app.clone();
        let me = self.app.state::<Arc<Meetings>>().inner().clone();
        std::thread::Builder::new()
            .name("meeting-level".into())
            .spawn(move || {
                if let Some(out) = stdout {
                    for line in BufReader::new(out).lines() {
                        let Ok(line) = line else { break };
                        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
                        if v["type"] == "level" {
                            logged("the level event", app.emit_to(PANEL, EVENT_LEVEL, serde_json::json!({"rms": v["rms"], "peak": v["peak"]})));
                        }
                    }
                }
                logged("reaping the level reader", child.wait());
                let mut inner = me.inner.lock().unwrap();
                if inner.level.as_ref().map(|r| r.pid) == Some(pid) {
                    inner.level = None;
                }
            })
            .ok();
    }

    // ---- the capture -------------------------------------------------------------------------

    fn start_capture(&self, title: Option<String>, event_id: Option<String>, manual: bool) {
        let (entity, thread) = self.active_scope();
        let zone = meeting_zone(&self.corpus_root(), entity.as_deref());
        if let Err(e) = std::fs::create_dir_all(&zone) {
            self.capture_failed(&format!("the meeting folder could not be created: {e}"), false);
            return;
        }
        self.show(PanelView::Starting);
        let mut command = Command::new(&self.companion);
        command
            .args(["capture", "--json", "--zone"])
            .arg(&zone)
            .args(["--exclude-pid", &std::process::id().to_string()])
            .arg("--end-with-stdin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(e) = &entity {
            command.env("RICHOS_ACTIVE_COMPANY", e);
        }
        installed_paths(&mut command);
        let mut child = match command.spawn() {
            Ok(c) => c,
            Err(e) => {
                self.capture_failed(&format!("the recorder did not start: {e}"), false);
                return;
            }
        };
        let pid = child.id();
        {
            let mut inner = self.inner.lock().unwrap();
            inner.monitor.exclude_pid(pid as i32);
            inner.capture = Some(CaptureRun {
                pid,
                started_at: now_millis(),
                title,
                event_id,
                manual,
                zone,
                entity,
                thread,
                dir: None,
                recording: false,
                stop: None,
                warnings: Vec::new(),
                _stdin: child.stdin.take().map(Arc::new),
            });
        }
        let stdout = child.stdout.take();
        let me = self.app.state::<Arc<Meetings>>().inner().clone();
        std::thread::Builder::new()
            .name("meeting-capture".into())
            .spawn(move || {
                if let Some(out) = stdout {
                    for line in BufReader::new(out).lines() {
                        let Ok(line) = line else { break };
                        me.capture_line(pid, &line);
                    }
                }
                let status = child.wait();
                me.capture_exited(pid, status.ok().and_then(|s| s.code()));
            })
            .ok();
    }

    fn capture_line(&self, pid: u32, line: &str) {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { return };
        let kind = v["type"].as_str().unwrap_or("");
        let mut inner = self.inner.lock().unwrap();
        let grant_known = inner.system_audio_seen;
        let current = inner.capture.as_ref().is_some_and(|c| c.pid == pid);
        let run = if current { inner.capture.as_mut() } else { inner.draining.iter_mut().find(|r| r.pid == pid) };
        let Some(run) = run else { return };
        match kind {
            // A stopped recorder finishing its files: what it reports is logged; the panel and the
            // monitor now belong to whatever came next.
            "started" | "failed" | "stand-down" if !current => {
                eprintln!("[richos] meeting notes: a stopped recorder ({pid}) reported {kind} while finishing: {}", v["message"].as_str().unwrap_or(""));
            }
            "started" => {
                run.recording = true;
                run.started_at = v["at"].as_u64().unwrap_or_else(now_millis);
                run.dir = v["dir"].as_str().map(PathBuf::from);
                // Written now, not only when the recording closes: a quit, an update or a crash
                // in the middle of the call still leaves a session the next launch finds.
                if let Some(dir) = &run.dir {
                    logged("writing meeting.json at the start", meeting_notes::write_facts(dir, &run.facts(0)));
                }
                let view = PanelView::Recording { started_at: run.started_at, note: None };
                drop(inner);
                self.show(view);
                self.publish_state();
            }
            "closed" => {
                run.dir = v["dir"].as_str().map(PathBuf::from).or(run.dir.take());
            }
            "channel" => {
                let message = v["message"].as_str().unwrap_or("").to_string();
                eprintln!("[richos] meeting notes: capture: {message}");
                if v["stalled"].as_bool() == Some(true) && !message.is_empty() {
                    run.warnings.push(message);
                    // Kept on disk at once, so a recording cut short still says it.
                    if let Some(dir) = &run.dir {
                        logged("writing meeting.json", meeting_notes::write_facts(dir, &run.facts(0)));
                    }
                }
            }
            // Sound came through the system tap: the grant exists. Remembered, so the voice bar may
            // follow the call's sound from now on (CEO, 2026-10-13).
            "system-audio" if !grant_known => {
                inner.system_audio_seen = true;
                logged("remembering the system-audio grant", self.remember(&inner));
            }
            "alarm" => eprintln!("[richos] meeting notes: capture alarm: {}", v["message"].as_str().unwrap_or("")),
            // What the recorder says about itself (the microphone following a device change, with
            // the gap it filled): the log is the only place this is visible.
            "info" => eprintln!("[richos] meeting notes: capture: {}", v["message"].as_str().unwrap_or("")),
            "failed" | "stand-down" => {
                let message = v["message"].as_str().or(v["reason"].as_str()).unwrap_or("").to_string();
                let recorded = run.recording;
                eprintln!("[richos] meeting notes: capture {kind}: {message}");
                drop(inner);
                if kind == "stand-down" {
                    self.capture_failed("Another RichOS recorder already has this call.", recorded);
                } else {
                    self.capture_failed(&message, recorded);
                }
            }
            _ => {}
        }
    }

    /// The capture process ended. After a stop this is the normal end; otherwise the recording
    /// ended by itself, and the monitor is told so the call does not prompt again.
    fn capture_exited(&self, pid: u32, code: Option<i32>) {
        let (finished, current) = {
            let mut guard = self.inner.lock().unwrap();
            let inner = &mut *guard;
            let Some((run, current)) = take_run(&mut inner.capture, &mut inner.draining, pid) else { return };
            if run.stop.is_none() {
                inner.monitor.input(Input::CaptureEnded);
            }
            (run, current)
        };
        eprintln!(
            "[richos] meeting notes: capture {} ended (exit {:?}, {}), session {}",
            pid,
            code,
            match finished.stop {
                Some(EndReason::CallEnded) => "the call ended",
                Some(EndReason::StoppedEarly) => "stopped early",
                None => "by itself",
            },
            finished.dir.as_deref().map(Path::display).map(|d| d.to_string()).unwrap_or_else(|| "none".into())
        );
        // Only the current recorder's indicator is its to take down; a stopped one's went at the stop.
        if current && matches!(self.inner.lock().unwrap().view, PanelView::Recording { .. } | PanelView::Starting) {
            self.show(PanelView::Hidden);
        }
        if code == Some(0) && finished.recording {
            self.recording_closed(finished);
        } else if let Some(dir) = finished.dir.as_deref().filter(|_| finished.recording) {
            // A recorder that failed (said beside the call): its session stays as it always has,
            // without notes, and the meeting.json written at its start must not make the next
            // launch present it as a meeting RichOS was closed during.
            logged("removing the failed recording's meeting.json", std::fs::remove_file(dir.join(meeting_notes::MEETING_FILE)));
        }
        self.publish_state();
    }

    fn stop_capture(&self, reason: EndReason) {
        let pid = {
            let mut inner = self.inner.lock().unwrap();
            let Some(mut run) = inner.capture.take() else { return };
            run.stop = Some(reason);
            let pid = run.pid;
            // Finishing from here on; the slot is free for the next call's recording.
            inner.draining.push(run);
            pid
        };
        // SIGINT is the recorder's own clean finish: drain, flush, close the session.
        // SAFETY: `pid` is the child this module spawned and has not reaped (the capture thread
        // reaps it after this signal makes it exit).
        unsafe {
            libc::kill(pid as i32, libc::SIGINT);
        }
        self.show(PanelView::Hidden);
    }

    fn capture_failed(&self, why: &str, partly_recorded: bool) {
        eprintln!("[richos] meeting notes: {why}");
        {
            // A start that failed before a recorder ran has no process whose exit would tell the
            // monitor; without this it stays "recording" and Try again does nothing.
            let mut inner = self.inner.lock().unwrap();
            if inner.capture.is_none() {
                inner.monitor.input(Input::CaptureEnded);
            }
        }
        let title = if partly_recorded { "Only part of this meeting was recorded." } else { "Nothing was recorded." };
        self.show(PanelView::Problem {
            title: title.into(),
            sub: Some(why.to_string()),
            actions: vec![
                PanelAction { label: "Try again".into(), act: "retry".into(), primary: true },
                PanelAction { label: "Not this one".into(), act: "aside".into(), primary: false },
            ],
        });
    }

    /// A recording is closed on disk: write down what the app knows about it (`meeting.json`),
    /// say "Preparing your meeting notes…", and turn it into notes on a thread of its own.
    fn recording_closed(&self, run: CaptureRun) {
        let Some(dir) = run.dir.clone() else {
            eprintln!("[richos] meeting notes: the recorder closed without naming its session");
            return;
        };
        let facts = run.facts(now_millis());
        eprintln!(
            "[richos] meeting notes: recorded {} ({}, company {}, partial: {}) into {}",
            facts.title.as_deref().unwrap_or("a call"),
            if facts.manual { "manual" } else { "prompted" },
            facts.entity.as_deref().unwrap_or("none"),
            if facts.warnings.is_empty() { "no".to_string() } else { facts.warnings.join(" / ") },
            dir.display()
        );
        logged("writing meeting.json", meeting_notes::write_facts(&dir, &facts));
        let zone = run.zone.clone();
        let me = self.app.state::<Arc<Meetings>>().inner().clone();
        std::thread::Builder::new()
            .name("meeting-notes".into())
            .spawn(move || me.prepare_notes(&dir, &zone, facts))
            .ok();
    }

    /// Every meeting a previous launch recorded and never finished ([`meeting_notes::unfinished_sessions`]),
    /// oldest first: its notes are prepared from where they stopped, exactly as at the call's end.
    /// A recording the previous launch never saw end (quit, update or crash mid-call) is first given
    /// the moment its recorder finishes its files, then its end and the line saying it was cut short.
    fn resume_unfinished(&self, sessions: Vec<PathBuf>) {
        for dir in sessions {
            let Some(mut facts) = meeting_notes::read_facts(&dir) else {
                eprintln!("[richos] meeting notes: {} has an unreadable meeting.json; left as it is", dir.display());
                continue;
            };
            let Some(zone) = dir.parent().map(Path::to_path_buf) else { continue };
            eprintln!("[richos] meeting notes: finishing the notes a previous launch left unfinished ({})", dir.display());
            if facts.ended_at == 0 {
                let closed = wait_for_recorder(&dir, RECORDER_CLOSE_WAIT);
                facts = meeting_notes::cut_short(&dir, facts, now_millis());
                eprintln!(
                    "[richos] meeting notes: its recording was cut short by the previous launch's end; the recorder {}",
                    if closed { "had closed its files" } else { "never closed its files (it ends where it stopped)" }
                );
                logged("writing meeting.json", meeting_notes::write_facts(&dir, &facts));
            }
            wait_for_orphaned_transcription(&dir);
            self.prepare_notes(&dir, &zone, facts);
        }
    }

    /// RichOS is quitting (Quit, or the relaunch an update asks for): every recording still running
    /// is told to finish its files now, and its `meeting.json` says where and why it ends, so the
    /// next launch makes its notes ([`Self::resume_unfinished`]). Bounded: a recorder that has not
    /// closed by then still does, by itself, when this app's pipe to it closes.
    fn finish_for_quit(&self, bound: Duration) {
        let runs: Vec<CaptureRun> = {
            let mut inner = self.inner.lock().unwrap();
            let mut runs: Vec<CaptureRun> = inner.draining.drain(..).collect();
            runs.extend(inner.capture.take());
            runs
        };
        if runs.is_empty() {
            return;
        }
        let now = now_millis();
        for run in &runs {
            if let (Some(dir), Some(facts)) = (run.dir.as_deref(), quit_facts(run, now)) {
                logged("writing meeting.json at the quit", meeting_notes::write_facts(dir, &facts));
            }
            if run.stop.is_none() {
                // SAFETY: `pid` is a recorder this module spawned; its capture thread has not
                // reaped it (it was still in the slot this lock just emptied).
                unsafe {
                    libc::kill(run.pid as i32, libc::SIGINT);
                }
            }
        }
        let dirs: Vec<&Path> = runs.iter().filter(|r| r.recording).filter_map(|r| r.dir.as_deref()).collect();
        let deadline = std::time::Instant::now() + bound;
        while dirs.iter().any(|d| !meeting_notes::recorder_closed(d)) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        let closed = dirs.iter().filter(|d| meeting_notes::recorder_closed(d)).count();
        eprintln!(
            "[richos] meeting notes: quitting with {} recorder(s) running; {closed} of {} recording(s) closed within {} ms; their notes come at the next launch",
            runs.len(),
            dirs.len(),
            bound.as_millis()
        );
    }

    /// A failure said through Rich finishes the session: the next launch does not say it again.
    fn failure_said(&self, dir: &Path, said: Result<(), String>) {
        match said {
            Ok(()) => {
                let mut state = meeting_notes::read_state(dir);
                state.failure_said = true;
                logged("recording that the failure was said", meeting_notes::write_state(dir, &state));
            }
            Err(e) => eprintln!("[richos] meeting notes: saying the failure failed: {e}"),
        }
    }

    fn set_processing(&self, what: Option<&str>) {
        self.inner.lock().unwrap().processing = what.map(str::to_string);
        self.publish_state();
    }

    /// Transcript, then the one brief, then Rich's message. Each failure is said through Rich in
    /// the design's own words; the recording and the transcript are never lost.
    fn prepare_notes(&self, dir: &Path, zone: &Path, facts: MeetingFacts) {
        self.set_processing(Some("preparing"));
        if !dir.join(meeting_notes::TRANSCRIPT_FILE).is_file() {
            if let Err(why) = self.transcribe(dir, zone, &facts) {
                eprintln!("[richos] meeting notes: transcription failed: {why}");
                self.failure_said(dir, self.say(&facts, &meeting_notes::transcript_failed_text(&why), &[]));
                self.set_processing(Some("transcript-failed"));
                return;
            }
        }
        // The transcription's own warnings join the capture's, so a span it found missing is said
        // in the notes rather than written up as complete; kept in meeting.json beside them.
        let mut facts = facts;
        for w in meeting_notes::transcription_warnings(dir) {
            if !facts.warnings.contains(&w) {
                facts.warnings.push(w);
            }
        }
        logged("writing meeting.json", meeting_notes::write_facts(dir, &facts));
        let transcript = match std::fs::read_to_string(dir.join(meeting_notes::TRANSCRIPT_FILE)) {
            Ok(t) => t,
            Err(e) => {
                self.failure_said(dir, self.say(&facts, &meeting_notes::transcript_failed_text(&format!("the transcript could not be read: {e}")), &[]));
                self.set_processing(Some("transcript-failed"));
                return;
            }
        };
        loop {
            let mut state = meeting_notes::read_state(dir);
            match meeting_notes::next_step(dir, &state) {
                NextStep::Write(n) => {
                    match self.write_brief_once(dir, &facts, &transcript) {
                        Ok(()) => eprintln!("[richos] meeting notes: brief written on attempt {n}"),
                        Err(why) => {
                            eprintln!("[richos] meeting notes: brief attempt {n} failed: {why}");
                            state.attempts = n;
                            state.last_error = Some(why);
                            logged("recording the attempt", meeting_notes::write_state(dir, &state));
                            if n < meeting_notes::MAX_ATTEMPTS {
                                std::thread::sleep(RETRY_AFTER[(n as usize - 1).min(RETRY_AFTER.len() - 1)]);
                            }
                        }
                    }
                }
                NextStep::Deliver => {
                    let brief = std::fs::read_to_string(dir.join(meeting_notes::BRIEF_FILE)).unwrap_or_default();
                    let files = [dir.join(meeting_notes::BRIEF_FILE), dir.join(meeting_notes::TRANSCRIPT_FILE)];
                    match self.say(&facts, &meeting_notes::message_text(&brief, &facts), &files) {
                        Ok(()) => {
                            state.delivered = true;
                            eprintln!("[richos] meeting notes: Rich said the notes in the conversation ({})", dir.display());
                            logged("recording the delivery", meeting_notes::write_state(dir, &state));
                            self.set_processing(None);
                        }
                        Err(why) => {
                            eprintln!("[richos] meeting notes: the brief is written but Rich could not say it: {why}");
                            self.set_processing(Some("notes-failed"));
                        }
                    }
                    return;
                }
                NextStep::Done => {
                    self.set_processing(None);
                    return;
                }
                NextStep::GiveUp => {
                    let why = state.last_error.clone().unwrap_or_default();
                    self.failure_said(dir, self.say(&facts, &meeting_notes::notes_failed_text(&why), &[]));
                    self.set_processing(Some("notes-failed"));
                    return;
                }
            }
        }
    }

    /// The pipeline (`richos-service run <session>`): the runtime's own node, ffmpeg and
    /// whisper-cli, the corpus and company of the meeting, bounded. Its output goes to
    /// `pipeline.log` in the session.
    fn transcribe(&self, dir: &Path, zone: &Path, facts: &MeetingFacts) -> Result<(), String> {
        let service = service_cli().ok_or("this build has no transcription service installed")?;
        let log = std::fs::File::create(dir.join("pipeline.log")).map_err(|e| e.to_string())?;
        let mut command = Command::new(node_bin());
        command
            .arg(&service)
            .arg("run")
            .arg(dir)
            .env("RICHOS_DROP_ZONE", zone)
            .env("LORO_CORPUS", self.corpus_root())
            .stdin(Stdio::null())
            .stdout(log.try_clone().map_err(|e| e.to_string())?)
            .stderr(log);
        installed_paths(&mut command);
        if let Ok(whisper) = richos_voice::stt::resolve_whisper_bin() {
            command.env("RICHOS_WHISPER_BIN", whisper);
        }
        // The selected call model, named outright: setup fetches it into RichOS's own model
        // folder (`setup_view.rs`, both models), which the service's own search does not visit.
        let model = richos_voice::stt::resolve_model(CALL_MODEL)
            .map_err(|e| format!("the call transcription model is not installed yet ({e})"))?;
        command.env("RICHOS_WHISPER_MODEL", model);
        match &facts.entity {
            Some(e) => command.env("RICHOS_ACTIVE_COMPANY", e),
            None => command.env_remove("RICHOS_ACTIVE_COMPANY"),
        };
        let mut child = command.spawn().map_err(|e| format!("the transcription service did not start: {e}"))?;
        // Its pid beside the session: a quit or crash of the app does not end it, and the next
        // launch must not start a second one on the same files ([`wait_for_orphaned_transcription`]).
        let marker = dir.join(TRANSCRIBING_FILE);
        logged("noting the transcription's pid", std::fs::write(&marker, child.id().to_string()));
        let started = std::time::Instant::now();
        let result = loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => break Ok(()),
                Ok(Some(status)) => {
                    let tail = std::fs::read_to_string(dir.join("pipeline.log")).unwrap_or_default();
                    let last = tail.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").to_string();
                    break Err(format!("the transcription ended with {status}: {last}"));
                }
                Ok(None) if started.elapsed() < TRANSCRIBE_DEADLINE => std::thread::sleep(Duration::from_millis(500)),
                _ => {
                    logged("stopping the transcription", child.kill());
                    logged("reaping the transcription", child.wait());
                    break Err("the transcription took longer than an hour".into());
                }
            }
        };
        logged("removing the transcription's pid", std::fs::remove_file(&marker));
        result
    }

    /// One attempt at the brief: one tool-free call under the conversation's account (Bust a
    /// bug's execution), the answer checked, `brief.md` written atomically.
    fn write_brief_once(&self, dir: &Path, facts: &MeetingFacts, transcript: &str) -> Result<(), String> {
        let state = self.app.state::<crate::AppState>();
        let (bin, folder) = crate::bug_report::claude_for(&state);
        let input = richos_core::bug_report::writer_input(&meeting_notes::writer_prompt(facts, transcript), None);
        let answer = crate::bug_report::ask_rich_within(&bin, folder.as_deref(), dir, &input, BRIEF_DEADLINE)?;
        let brief = meeting_notes::accept_brief(&answer)?;
        meeting_notes::write_brief(dir, &brief).map(|_| ()).map_err(|e| format!("brief.md could not be written: {e}"))
    }

    /// Rich says it, in the conversation and company captured at meeting start, as a visible
    /// (Digest) proactive message; `files` become the Output record's entries of that turn.
    fn say(&self, facts: &MeetingFacts, text: &str, files: &[PathBuf]) -> Result<(), String> {
        let state = self.app.state::<crate::AppState>();
        let mut spine = state.spine.lock().map_err(|_| "the conversation is unavailable".to_string())?;
        let turn = spine.raise_proactive(facts.thread.as_deref(), AttentionTier::Digest, text).map_err(|e| e.to_string())?;
        if !files.is_empty() {
            let thread = facts.thread.clone().or_else(|| spine.active_thread().map(str::to_string)).ok_or("no conversation")?;
            spine.witness_app_files(&thread, &turn, files).map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    // ---- scope: which company this meeting belongs to ----------------------------------------

    /// The conversation's company at meeting start, without waiting on a turn: the active turn's
    /// own binding first (lock-free), then the spine only if it is free right now.
    fn active_scope(&self) -> (Option<String>, Option<String>) {
        let state = self.app.state::<crate::AppState>();
        if let Some(turn) = state.control.active_turn() {
            return (turn.entity_id.map(|e| e.as_str().to_string()), Some(turn.thread_id));
        }
        if let Ok(spine) = state.spine.try_lock() {
            if let Some(b) = spine.active_binding() {
                return (Some(b.entity_id().as_str().to_string()), Some(b.thread_id().to_string()));
            }
        }
        (None, None)
    }

    /// The corpus the service writes meeting evidence into: the app's resolved corpus, else the
    /// service's own default (`tools/richos-service/lib/config.js` `DEFAULT_CORPUS_ROOT`).
    fn corpus_root(&self) -> PathBuf {
        let state = self.app.state::<crate::AppState>();
        let install = state.loro_install.lock().unwrap();
        crate::meetings_corpus_root(install.as_ref())
    }
}

/// The transcription service this launch carries: `RICHOS_SERVICE_BIN` (the pointer the app
/// already honors for the vocabulary service), else the copy the engine archive carries in the
/// voice delivery contract's product layout, `<engine>/service/tools/richos-service/...`, whose
/// relative imports reach `<engine>/service/engine/voice` and the extension module beside it
/// (`make-engine-asset.sh`). Never a checkout found by walking up.
pub fn service_cli() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("RICHOS_SERVICE_BIN").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(p);
    }
    let engine = richos_voice::stt::delivered_runtime_bin_dir()?.parent()?.parent()?.to_path_buf();
    let p = engine.join(SERVICE_ENTRY);
    p.is_file().then_some(p)
}

/// The running transcription's pid, beside its session while it runs.
const TRANSCRIBING_FILE: &str = "transcribing.pid";

/// RichOS is quitting: finish every running recording's files and keep what the next launch
/// needs to make its notes (handover review rv-20261013T145354Z-da4ac5cf-c86a #1). Called from the
/// app's exit path; a launch without meeting notes has nothing to do.
pub fn quit(app: &AppHandle) {
    if let Some(meetings) = app.try_state::<Arc<Meetings>>() {
        meetings.finish_for_quit(QUIT_CLOSE_WAIT);
    }
}

/// What a quit writes as a recording's `meeting.json`: one already told to stop (the call ended,
/// or Stop) ended where it was told to; one still running is cut short by the quit, and its notes
/// say so. Nothing for a recorder that never started writing.
fn quit_facts(run: &CaptureRun, now: u64) -> Option<MeetingFacts> {
    let dir = run.dir.as_deref().filter(|_| run.recording)?;
    Some(if run.stop.is_some() { run.facts(now) } else { meeting_notes::cut_short(dir, run.facts(0), now) })
}

/// Wait, at most `bound`, for a recorder to close its session's files (a previous launch's
/// recorder finishes them when that launch's pipe to it closes). True when it did.
fn wait_for_recorder(dir: &Path, bound: Duration) -> bool {
    let deadline = std::time::Instant::now() + bound;
    while !meeting_notes::recorder_closed(dir) {
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    true
}

/// A transcription a previous launch started may still be running: quitting or a crash of the app
/// does not end its child, and two runs on one session would write the same files. The next launch
/// waits for it (at most [`TRANSCRIBE_DEADLINE`]), then goes on from what it left. It is recognized
/// by the pid it left AND its own command line naming this session, never by a process name.
fn wait_for_orphaned_transcription(dir: &Path) {
    let Some(pid) = std::fs::read_to_string(dir.join(TRANSCRIBING_FILE)).ok().and_then(|s| s.trim().parse::<u32>().ok()) else {
        return;
    };
    let started = std::time::Instant::now();
    let mut waited = false;
    while transcription_running(pid, dir) && started.elapsed() < TRANSCRIBE_DEADLINE {
        if !waited {
            eprintln!("[richos] meeting notes: the previous launch's transcription (pid {pid}) is still running; waiting for it");
            waited = true;
        }
        std::thread::sleep(Duration::from_secs(2));
    }
    if waited {
        eprintln!("[richos] meeting notes: the previous launch's transcription (pid {pid}) ended after {} s", started.elapsed().as_secs());
    }
}

/// Process `pid` is running and is the transcription of this session (`richos-service run <dir>`).
fn transcription_running(pid: u32, dir: &Path) -> bool {
    let out = Command::new("/bin/ps").args(["-ww", "-p", &pid.to_string(), "-o", "command="]).stdin(Stdio::null()).stderr(Stdio::null()).output();
    out.is_ok_and(|o| o.status.success() && String::from_utf8_lossy(&o.stdout).contains(&*dir.to_string_lossy()))
}

/// The call-transcription model: the service's default tier ("quantized"), the batch ladder's
/// top (`engine/voice/models`), and one of the two models setup already fetches.
pub const CALL_MODEL: &str = "large-v3-turbo-q5_0";

/// Where the engine archive puts the service's entry point (`make-engine-asset.sh`).
pub const SERVICE_ENTRY: &str = "service/tools/richos-service/bin/richos-service.js";

/// The runtime's Node, else `RICHOS_NODE_BIN`, else the first on PATH (a development launch).
fn node_bin() -> PathBuf {
    richos_voice::stt::delivered_runtime_bin_dir()
        .map(|b| b.join("node"))
        .filter(|n| n.is_file())
        .or_else(|| std::env::var_os("RICHOS_NODE_BIN").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("node"))
}

/// Explicit installed paths for a child: the runtime's `bin` first on PATH (so the companion's
/// coordinator, which runs `/usr/bin/env node`, gets the runtime's Node), its ffmpeg named, and
/// the service CLI for the coordinator (`RICHOS_SERVICE_CLI`). Nothing is found by walking up a
/// checkout; a launch with no delivered runtime keeps its own PATH.
fn installed_paths(command: &mut Command) {
    if let Some(bin) = richos_voice::stt::delivered_runtime_bin_dir() {
        command.env("PATH", format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", bin.display()));
        if bin.join("ffmpeg").is_file() {
            command.env("RICHOS_FFMPEG_BIN", bin.join("ffmpeg"));
        }
    }
    if let Some(cli) = service_cli() {
        command.env("RICHOS_SERVICE_CLI", cli);
    }
}

/// "12 October 2026" for the meeting's local day (the design's example brief).
pub fn date_line(at_ms: u64) -> String {
    const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let secs = (at_ms / 1000) as libc::time_t;
    // SAFETY: localtime_r writes into the tm we own; a zeroed tm is a valid initial value.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let ok = unsafe { !libc::localtime_r(&secs, &mut tm).is_null() };
    if !ok {
        return String::new();
    }
    format!("{} {} {}", tm.tm_mday, MONTHS[(tm.tm_mon.clamp(0, 11)) as usize], tm.tm_year + 1900)
}

/// The name a person calls the app. macOS names Zoom's process "zoom.us"; the design says "Call
/// detected in Zoom." Anything else is its own localized name.
pub fn friendly_app(name: &str) -> String {
    match name {
        "zoom.us" | "us.zoom.xos" => "Zoom".into(),
        n if n.starts_with("Microsoft Teams") || n == "com.microsoft.teams2" => "Microsoft Teams".into(),
        "com.google.Chrome" => "Google Chrome".into(),
        n => n.to_string(),
    }
}

/// "Starts in 1 minute." / "Starting now." for a calendar prompt.
pub fn starts_line(starts_at: u64, now: u64) -> String {
    let secs = starts_at.saturating_sub(now) / 1000;
    match (secs + 30) / 60 {
        0 => "Starting now.".into(),
        1 => "Starts in 1 minute.".into(),
        n => format!("Starts in {n} minutes."),
    }
}

// ---- window commands -------------------------------------------------------------------------

/// A result nothing can act on is still said: one log line, never a silent discard.
fn logged<T, E: std::fmt::Display>(what: &str, result: Result<T, E>) {
    if let Err(e) = result {
        eprintln!("[richos] meeting notes: {what} failed: {e}");
    }
}

fn meetings(app: &AppHandle) -> Result<Arc<Meetings>, String> {
    app.try_state::<Arc<Meetings>>().map(|s| s.inner().clone()).ok_or_else(|| "Meeting notes are not available in this build.".into())
}

/// What the panel should show now (its page asks once when it loads).
#[richos_command::command]
pub fn meeting_panel_state(app: AppHandle) -> Option<PanelView> {
    meetings(&app).ok().map(|m| m.inner.lock().unwrap().view.clone())
}

/// The panel's page laid out a view: the window it needs, shadow room included.
#[richos_command::command]
pub fn meeting_panel_laid_out(app: AppHandle, w: f64, h: f64, aside: bool) {
    if let Ok(m) = meetings(&app) {
        m.laid_out(w, h);
        m.set_level(aside && w > 0.0 && h > 0.0);
    }
}

/// The voice bar's grip was pressed: the window follows the pointer (AppKit's own window drag).
/// From Rust, so the page needs no window permission of its own.
#[richos_command::command]
pub fn meeting_panel_drag(app: AppHandle) {
    if let Some(w) = app.get_webview_window(PANEL) {
        logged("dragging the panel", w.start_dragging());
    }
}

/// A press on the panel: `take` (Take Notes), `not-this-one` (the voice bar's ×), `stop`, `retry`.
#[richos_command::command]
pub fn meeting_answer(app: AppHandle, answer: String) -> Result<(), String> {
    let m = meetings(&app)?;
    let input = answer_input(&answer)?;
    let (actions, view) = {
        let mut inner = m.inner.lock().unwrap();
        (inner.monitor.input(input), inner.view.clone())
    };
    // The panel is closed by `show`, the one place that emits the page's transition, and never
    // by writing the view behind its back (second review rv-20261013T100322Z-8bfcf4fe-37ac #2).
    if hides_panel(&answer, &view, &actions) {
        m.show(PanelView::Hidden);
    }
    m.apply(actions);
    Ok(())
}

/// The panel's buttons, as the monitor's inputs. "Try again" is its own input: the monitor
/// repeats the failed recording, a detected call still armed (second review #1).
fn answer_input(answer: &str) -> Result<Input, String> {
    match answer {
        "take" => Ok(Input::TakeNotes),
        "retry" => Ok(Input::Retry),
        "not-this-one" => Ok(Input::NotThisOne),
        "stop" => Ok(Input::Stop),
        other => Err(format!("unknown answer {other}")),
    }
}

/// Does this answer close the panel here, rather than through the monitor's `HidePrompt`? The
/// x / "Not this one" always closes it: on a problem card (the monitor may hold nothing to hide)
/// and when the monitor answered nothing at all. A prompt the monitor hides is left to `apply`.
fn hides_panel(answer: &str, view: &PanelView, actions: &[Action]) -> bool {
    answer == "not-this-one" && (actions.is_empty() || matches!(view, PanelView::Problem { .. }))
}

/// Settings: "Start meeting notes" (or, while recording, "Stop meeting notes").
#[richos_command::command]
pub fn meeting_start_or_stop(app: AppHandle) -> Result<MeetingState, String> {
    let m = meetings(&app)?;
    let actions = {
        let mut inner = m.inner.lock().unwrap();
        if inner.capture.is_some() { inner.monitor.input(Input::Stop) } else { inner.monitor.input(Input::ManualStart) }
    };
    m.apply(actions);
    Ok(m.state())
}

/// Settings: "Pause meeting prompts" and the chip's Resume.
#[richos_command::command]
pub fn meeting_set_paused(app: AppHandle, paused: bool) -> Result<MeetingState, String> {
    let m = meetings(&app)?;
    let actions = {
        let mut inner = m.inner.lock().unwrap();
        let actions = inner.monitor.set_paused(paused);
        logged("remembering the pause", m.remember(&inner));
        actions
    };
    m.apply(actions);
    Ok(m.state())
}

/// The main window's view of meeting notes.
#[richos_command::command]
pub fn meeting_state(app: AppHandle) -> MeetingState {
    match meetings(&app) {
        Ok(m) => m.state(),
        Err(_) => MeetingState { available: false, paused: false, recording: false, started_at: None, processing: None },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(pid: u32) -> CaptureRun {
        CaptureRun {
            pid,
            started_at: 0,
            title: None,
            event_id: None,
            manual: false,
            zone: PathBuf::new(),
            entity: None,
            thread: None,
            dir: None,
            recording: true,
            stop: None,
            warnings: Vec::new(),
            _stdin: None,
        }
    }

    #[test]
    fn a_stopped_recorder_finishing_late_never_takes_the_next_recording_with_it() {
        // The next call's prompt shows the moment a call ends (rv-20261013T130528Z #3), so Take
        // Notes can come while the stopped recorder is still draining: its exit is its own.
        let mut capture = Some(run(20));
        let mut draining = vec![run(10)];
        let (old, current) = take_run(&mut capture, &mut draining, 10).unwrap();
        assert_eq!((old.pid, current), (10, false));
        assert_eq!(capture.as_ref().map(|c| c.pid), Some(20), "the new recording keeps running");
        assert!(draining.is_empty());
        let (new, current) = take_run(&mut capture, &mut draining, 20).unwrap();
        assert_eq!((new.pid, current), (20, true));
        assert!(take_run(&mut capture, &mut draining, 20).is_none());
    }

    #[test]
    fn a_quit_during_a_recording_keeps_it_and_says_it_was_cut_short() {
        // Handover review rv-20261013T145354Z-da4ac5cf-c86a #1: Quit during a recording left it
        // running and unrecoverable. The quit now writes each recording's meeting.json.
        let dir = std::env::temp_dir().join(format!("richos-quit-facts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut running = run(30);
        running.dir = Some(dir.clone());
        running.started_at = 1_000;
        let facts = quit_facts(&running, 7_000).expect("a running recording is kept");
        assert_eq!(facts.ended_at, 7_000, "it ends at the quit (the recorder had not closed yet)");
        assert_eq!(facts.warnings, vec![meeting_notes::CUT_SHORT_WARNING.to_string()]);
        assert!(!facts.stopped_early);
        // One the call's end had already stopped ended where it was told to: no cut-short line.
        let mut stopped = running.clone();
        stopped.stop = Some(EndReason::CallEnded);
        let facts = quit_facts(&stopped, 7_000).unwrap();
        assert_eq!((facts.ended_at, facts.warnings.len()), (7_000, 0));
        // A recorder that never started writing has nothing to keep.
        let mut starting = running.clone();
        starting.recording = false;
        assert!(quit_facts(&starting, 7_000).is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_previous_launchs_transcription_is_known_by_its_pid_and_its_session() {
        // Handover review rv-20261013T141401Z-390d6bea-2e23 #2: the next launch resumes the notes,
        // and must wait for a transcription the killed launch left running on the same session.
        let dir = std::env::temp_dir().join(format!("richos-orphan-{}", std::process::id()));
        let other = std::env::temp_dir().join(format!("richos-orphan-other-{}", std::process::id()));
        // `sh -c CMD NAME`: NAME is on its command line, as `richos-service run <session>` is.
        let mut child = Command::new("/bin/sh").args(["-c", "sleep 30"]).arg(&dir).spawn().unwrap();
        let pid = child.id();
        assert!(transcription_running(pid, &dir));
        assert!(!transcription_running(pid, &other), "another session's run is not this one");
        child.kill().unwrap();
        child.wait().unwrap();
        assert!(!transcription_running(pid, &dir), "an ended run is not waited for");
        // No pid beside the session, or the pid of a run that has ended: it returns (a wait here
        // would hang the test, not pass it).
        wait_for_orphaned_transcription(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(TRANSCRIBING_FILE), pid.to_string()).unwrap();
        wait_for_orphaned_transcription(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_brief_dates_the_meeting_by_its_local_day() {
        // 2026-10-12T12:00:00Z: the same calendar day in every zone within eleven hours of UTC.
        assert_eq!(date_line(1_791_806_400_000), "12 October 2026");
    }

    #[test]
    fn the_prompt_names_the_app_as_a_person_does() {
        assert_eq!(friendly_app("zoom.us"), "Zoom");
        assert_eq!(friendly_app("Microsoft Teams (work or school)"), "Microsoft Teams");
        assert_eq!(friendly_app("Google Chrome"), "Google Chrome");
        assert_eq!(friendly_app("com.google.Chrome"), "Google Chrome");
        assert_eq!(friendly_app("Webex"), "Webex");
    }

    #[test]
    fn the_calendar_prompt_counts_minutes() {
        assert_eq!(starts_line(60_000, 0), "Starts in 1 minute.");
        assert_eq!(starts_line(70_000, 0), "Starts in 1 minute.");
        assert_eq!(starts_line(10_000, 0), "Starting now.");
        assert_eq!(starts_line(0, 5_000), "Starting now.");
        assert_eq!(starts_line(180_000, 0), "Starts in 3 minutes.");
    }

    #[test]
    fn the_panel_views_serialize_for_the_page() {
        let v = serde_json::to_value(PanelView::Recording { started_at: 5, note: None }).unwrap();
        assert_eq!(v, serde_json::json!({"view": "recording", "startedAt": 5, "note": null}));
        let p = serde_json::to_value(PanelView::Prompt { title: "Call detected in zoom.us.".into(), sub: None }).unwrap();
        assert_eq!(p["view"], "prompt");
    }

    #[test]
    fn closing_a_failed_captures_bar_goes_through_show_and_try_again_is_a_retry() {
        // Second review rv-20261013T100322Z-8bfcf4fe-37ac #2: the x on a put-aside problem card
        // set the view to Hidden without emitting, so the page kept the bar on screen. The rule
        // now: on a problem card the x closes through `show` (which emits) whatever the monitor
        // answers; `apply`'s HidePrompt then finds it hidden and emits nothing twice.
        let problem = PanelView::Problem { title: "Nothing was recorded.".into(), sub: None, actions: vec![] };
        let prompt = PanelView::Prompt { title: "Call detected in Zoom.".into(), sub: None };
        assert!(hides_panel("not-this-one", &problem, &[Action::HidePrompt]));
        assert!(hides_panel("not-this-one", &problem, &[]));
        assert!(hides_panel("not-this-one", &PanelView::Hidden, &[]));
        assert!(!hides_panel("not-this-one", &prompt, &[Action::HidePrompt]), "apply hides a prompt the monitor hides");
        assert!(!hides_panel("retry", &problem, &[]));
        assert_eq!(answer_input("retry"), Ok(Input::Retry));
        assert_eq!(answer_input("take"), Ok(Input::TakeNotes));
        assert!(answer_input("aside").is_err());
    }
}

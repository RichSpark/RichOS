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
//!   from the monitor: RichOS holding the microphone is never "the call".
//!
//! What happens to a finished recording (transcript, notes, the brief through Rich) is
//! [`Meetings::recording_closed`]'s job.

use crate::dictation::appkit::{self, Win};
use richos_core::meeting::{parse_watch_line, Action, EndReason, Input, MeetingMonitor, PromptKind, WatchLine};
use richos_core::util::now_millis;
use richos_voice::dictation_bar::Rect;
use serde::Serialize;
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
}

struct Inner {
    monitor: MeetingMonitor,
    view: PanelView,
    capture: Option<CaptureRun>,
    processing: Option<String>,
    /// Holding this keeps the watcher's stdin open; dropping it ends the watcher.
    watcher_stdin: Option<ChildStdin>,
    watcher_pid: Option<u32>,
}

pub struct Meetings {
    app: AppHandle,
    companion: PathBuf,
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
    let meetings = Arc::new(Meetings {
        app: app.clone(),
        companion,
        inner: Mutex::new(Inner {
            monitor: MeetingMonitor::new(),
            view: PanelView::Hidden,
            capture: None,
            processing: None,
            watcher_stdin: None,
            watcher_pid: None,
        }),
    });
    app.manage(meetings.clone());
    let watcher = meetings.clone();
    std::thread::Builder::new().name("meeting-watcher".into()).spawn(move || watcher.run_watcher()).ok();
    let clock = meetings.clone();
    std::thread::Builder::new()
        .name("meeting-clock".into())
        .spawn(move || loop {
            std::thread::sleep(Duration::from_secs(1));
            let actions = clock.inner.lock().unwrap().monitor.tick(now_millis());
            clock.apply(actions);
        })
        .ok();
}

impl Meetings {
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
            match action {
                Action::ShowPrompt(kind) => {
                    let view = match kind {
                        PromptKind::Unscheduled { app } => PanelView::Prompt { title: format!("Call detected in {app}."), sub: None },
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
        let hidden = w <= 0.0 || h <= 0.0 || self.inner.lock().unwrap().view == PanelView::Hidden;
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
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if let Some(e) = &entity {
            command.env("RICHOS_ACTIVE_COMPANY", e);
        }
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
        let Some(run) = inner.capture.as_mut().filter(|c| c.pid == pid) else { return };
        match kind {
            "started" => {
                run.recording = true;
                run.started_at = v["at"].as_u64().unwrap_or_else(now_millis);
                run.dir = v["dir"].as_str().map(PathBuf::from);
                let view = PanelView::Recording { started_at: run.started_at, note: None };
                drop(inner);
                self.show(view);
                self.publish_state();
            }
            "closed" => {
                run.dir = v["dir"].as_str().map(PathBuf::from).or(run.dir.take());
            }
            "alarm" => eprintln!("[richos] meeting notes: capture alarm: {}", v["message"].as_str().unwrap_or("")),
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
        let finished = {
            let mut inner = self.inner.lock().unwrap();
            if inner.capture.as_ref().map(|c| c.pid) != Some(pid) {
                return;
            }
            let run = inner.capture.take().unwrap();
            if run.stop.is_none() {
                inner.monitor.input(Input::CaptureEnded);
            }
            run
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
        if matches!(self.inner.lock().unwrap().view, PanelView::Recording { .. } | PanelView::Starting) {
            self.show(PanelView::Hidden);
        }
        if code == Some(0) && finished.recording {
            self.recording_closed(finished);
        }
        self.publish_state();
    }

    fn stop_capture(&self, reason: EndReason) {
        let pid = {
            let mut inner = self.inner.lock().unwrap();
            let Some(run) = inner.capture.as_mut() else { return };
            run.stop = Some(reason);
            run.pid
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

    /// A recording is closed on disk. Turning it into notes is the next step's (transcript, the
    /// one brief, Rich's message); for now the session is where the pipeline expects it.
    fn recording_closed(&self, run: CaptureRun) {
        eprintln!(
            "[richos] meeting notes: recorded {} ({}{}, company {}, conversation {}) into {}",
            run.title.as_deref().unwrap_or("a call"),
            if run.manual { "manual" } else { "prompted" },
            if run.event_id.is_some() { ", calendar" } else { "" },
            run.entity.as_deref().unwrap_or("none"),
            run.thread.as_deref().unwrap_or("none"),
            run.dir.as_deref().unwrap_or(run.zone.as_path()).display()
        );
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
        if let Some(install) = state.loro_install.lock().unwrap().as_ref() {
            return install.root().path().to_path_buf();
        }
        std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join("RichOS/corpus")
    }
}

/// The capture destination, by the service's own rule (`config.js` `evidenceRoot` + `dropZone`):
/// a company's `companies/<id>/evidence/meetings`, or `ceo/evidence/unfiled/meetings` when the
/// conversation has no company.
pub fn meeting_zone(corpus: &Path, entity: Option<&str>) -> PathBuf {
    match entity {
        Some(e) => corpus.join("companies").join(e).join("evidence").join("meetings"),
        None => corpus.join("ceo").join("evidence").join("unfiled").join("meetings"),
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
pub fn meeting_panel_laid_out(app: AppHandle, w: f64, h: f64, #[allow(unused_variables)] aside: bool) {
    if let Ok(m) = meetings(&app) {
        m.laid_out(w, h);
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
    let input = match answer.as_str() {
        "take" | "retry" => Input::TakeNotes,
        "not-this-one" => Input::NotThisOne,
        "stop" => Input::Stop,
        other => return Err(format!("unknown answer {other}")),
    };
    // Try again on a failed start: the monitor is no longer prompting, so it is a manual start
    // of the same recording.
    let actions = {
        let mut inner = m.inner.lock().unwrap();
        let mut out = inner.monitor.input(input.clone());
        if out.is_empty() && answer == "retry" {
            out = inner.monitor.input(Input::ManualStart);
        }
        if out.is_empty() && answer == "not-this-one" && matches!(inner.view, PanelView::Problem { .. }) {
            inner.view = PanelView::Hidden;
            out.push(Action::HidePrompt);
        }
        out
    };
    if actions.is_empty() && answer == "not-this-one" {
        m.show(PanelView::Hidden);
    }
    m.apply(actions);
    Ok(())
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
    let actions = m.inner.lock().unwrap().monitor.set_paused(paused);
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

    #[test]
    fn the_zone_is_the_services_rule() {
        let corpus = Path::new("/c");
        assert_eq!(meeting_zone(corpus, Some("acme")), PathBuf::from("/c/companies/acme/evidence/meetings"));
        assert_eq!(meeting_zone(corpus, None), PathBuf::from("/c/ceo/evidence/unfiled/meetings"));
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
}

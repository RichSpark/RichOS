//! **The app's side of dictation** (plan section 1 and section 6): starting the dictation tool
//! as this app's child, the socket client, the voice-mode yield and `finish`.
//!
//! The app is RichOS exactly as it is today. It starts, counts its starts and shows its splash
//! as before; nothing here touches its lifecycle, its launch record, its activation rule, its
//! window counting or its updater. In slices 1 to 4 the tool is always this app's child: it
//! inherits this app's environment and data folder and ends within a second of this app ending.
//!
//! **Dictation and voice mode never listen at once** (plan section 2). When the tool starts
//! listening it says `will-listen`, and an app in voice mode ends it, discarding the utterance in
//! progress: never sent as a turn. When the talk button is pressed while a dictation listens, the
//! app says `finish` first, and voice mode listens once the dictation is being written.
//!
//! **The Dictation sheet and the Settings row** (slice 2, plan section 2 rows 5 to 12, section
//! 4): the [`Host`] writes `dictation.json` (the app is its only writer), starts and stops the
//! tool with the switch, asks macOS for the two permissions in the drawn order (the microphone,
//! then Accessibility), tells the tool to make its key tap the moment Accessibility is allowed,
//! and asks the tool's tap for the next key ("Press a different key"). The words are the
//! window's (`ui/dictation.js`, round 19); [`view_of`] decides the facts they are drawn from.
//!
//! **The login start** (slice 5, `dictation/login.rs`): in an installed copy on the account's
//! real home the tool is launchd's, registered as a LaunchAgent when the switch goes on (and
//! again at each app start while it is on), unregistered when it goes off; it outlives the app.
//! Anywhere else (a folder copy, an older macOS) the tool stays this app's child.
//!
//! **A copy that does not own the key** (review finding 4, 2026-10-08) changes the owner's
//! settings through the tool (`change`), never its own file, and shows the key and accuracy the
//! tool reports. **A tool that goes away** (finding 5) is started again, or reconnected to, with
//! a bound on how often. **The microphone handover is answered** (finding 2): `will-listen` is
//! acknowledged with `voice-yielded` once voice mode is closed, and `finish` refuses to open
//! voice mode while a dictation still listens.
//!
//! **The test switch stays** for the walks: `RICHOS_DICTATION_TEST_ON` set to `1` or `0` in the
//! app's environment writes `on` into `dictation.json` at start.

use crate::dictation::ipc::{self, AppMessage, ToolMessage};
use crate::dictation::store;
use serde::Serialize;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::time::{Duration, Instant};

/// The test switch's environment variable (slice 1 only).
pub const TEST_SWITCH: &str = "RICHOS_DICTATION_TEST_ON";

/// How long the app waits for its child tool's socket after starting it.
const CONNECT_WITHIN: Duration = Duration::from_secs(5);
/// How long `finish` waits for the dictation to stop listening. Past it, voice mode does NOT
/// open (finding 2): speech meant for dictation never reaches Rich.
const FINISH_WITHIN: Duration = Duration::from_secs(3);
/// A tool that went away is started again after this, so a crash loop is never a tight one.
const RESTART_AFTER: Duration = Duration::from_secs(1);
/// At most this many restarts in [`RESTART_WINDOW`]; past it the app stops trying and says so.
pub const RESTART_LIMIT: usize = 3;
pub const RESTART_WINDOW: Duration = Duration::from_secs(60);

// =============================================================================================
// THE YIELD: pure enough to test without a microphone
// =============================================================================================

/// One voice-mode session's gate on its turns. Closed when dictation takes the microphone, so
/// whatever that session was still holding is discarded rather than sent.
#[derive(Clone, Default)]
pub struct TurnGate(Arc<AtomicBool>);

impl TurnGate {
    pub fn new() -> Self {
        TurnGate::default()
    }
    pub fn close(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    pub fn closed(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// `submit`, behind `gate`: once the gate is closed nothing reaches the conversation.
pub fn gated_submit(gate: TurnGate, submit: Arc<dyn Fn(String, bool) + Send + Sync>) -> Arc<dyn Fn(String, bool) + Send + Sync> {
    Arc::new(move |text: String, rich_audible: bool| {
        if gate.closed() {
            eprintln!("[richos] voice: an utterance was discarded because dictation took the microphone");
            return;
        }
        submit(text, rich_audible);
    })
}

/// What the yield needs from voice mode, and what the tool's menu bar menu and its bar's Fix it
/// ask of the app's window (slice 3).
pub trait VoiceMode: Send + Sync {
    /// End voice mode for dictation: close its gate FIRST, then close the microphone and tell
    /// the window. `true` when voice mode was on.
    fn end_for_dictation(&self) -> bool;
    /// Bring the app's window forward, on the Dictation sheet when `sheet` (plan section 6:
    /// **Open RichOS**, **Dictation settings…**, **Fix it**).
    fn come_forward(&self, _sheet: bool) {}
    /// The tool changed `dictation.json` from its menu: whatever the window shows of it is
    /// re-read.
    fn settings_changed(&self) {}
}

/// What a message from the tool does in the app.
pub fn on_tool_message(message: &ToolMessage, voice: &dyn VoiceMode, link: &Link) {
    match message {
        ToolMessage::WillListen { seq } => {
            if voice.end_for_dictation() {
                eprintln!("[richos] voice: ended because dictation started listening");
            }
            // Answered only now, with voice mode's microphone and turn gate closed, so the
            // tool opens its microphone after and never beside it (finding 2); the answer names
            // the handover and this copy, so it stands for no other (third review, finding 1).
            let bundle = std::env::current_exe().map(|e| bundle_of(&e).display().to_string()).unwrap_or_default();
            link.send(&AppMessage::VoiceYielded { seq: *seq, bundle });
        }
        ToolMessage::State { .. } => link.observe(message.clone()),
        ToolMessage::Key { .. } => {}
        ToolMessage::ComeForward { sheet } => voice.come_forward(*sheet),
        ToolMessage::SettingsChanged => voice.settings_changed(),
    }
    link.tell_listener(Some(message));
}

// =============================================================================================
// THE LINK: the socket client
// =============================================================================================

/// What the window is told when the tool says something: `Some` with the message, `None` when
/// the tool's socket closed.
pub type Listener = Box<dyn Fn(Option<&ToolMessage>) + Send + Sync>;

/// Told when the tool's socket closed (the tool ended, or crashed): the host decides what then.
pub type OnClosed = Box<dyn Fn() + Send + Sync>;

/// The app's connection to whichever tool holds the key in this login session.
#[derive(Default)]
pub struct Link {
    writer: Mutex<Option<UnixStream>>,
    last: Mutex<Option<ToolMessage>>,
    changed: Condvar,
    listener: Mutex<Option<Listener>>,
    on_closed: Mutex<Option<OnClosed>>,
}

impl Link {
    fn observe(&self, state: ToolMessage) {
        *self.last.lock().unwrap_or_else(|p| p.into_inner()) = Some(state);
        self.changed.notify_all();
    }

    /// Who hears what the tool says (the window, through `rich://dictation`).
    pub fn set_listener(&self, listener: Listener) {
        *self.listener.lock().unwrap_or_else(|p| p.into_inner()) = Some(listener);
    }

    /// Who hears that the tool's socket closed (the host, which starts it again).
    pub fn set_on_closed(&self, on_closed: OnClosed) {
        *self.on_closed.lock().unwrap_or_else(|p| p.into_inner()) = Some(on_closed);
    }

    /// The key and the model the tool reports it uses (`None` before any state).
    pub fn tool_settings(&self) -> Option<(u8, String)> {
        match &*self.last.lock().unwrap_or_else(|p| p.into_inner()) {
            Some(ToolMessage::State { key, model, .. }) if *key > 0 => Some((*key, model.clone())),
            _ => None,
        }
    }

    fn tell_listener(&self, message: Option<&ToolMessage>) {
        if let Some(listener) = self.listener.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            listener(message);
        }
    }

    /// A tool is connected now.
    pub fn connected(&self) -> bool {
        self.writer.lock().unwrap_or_else(|p| p.into_inner()).is_some()
    }

    /// The last state the tool reported: the bundle of the copy that holds the key, and whether
    /// its key tap exists. `None` when no tool has said anything.
    pub fn owner_and_tap(&self) -> Option<(String, bool)> {
        match &*self.last.lock().unwrap_or_else(|p| p.into_inner()) {
            Some(ToolMessage::State { owner, key_tap, .. }) => Some((owner.clone(), *key_tap)),
            _ => None,
        }
    }

    /// The last thing the tool said was that dictation is off: it ended because a switch or the
    /// menu bar turned dictation off, in whichever copy (recheck finding 2).
    pub fn tool_said_off(&self) -> bool {
        matches!(*self.last.lock().unwrap_or_else(|p| p.into_inner()), Some(ToolMessage::State { on: false, .. }))
    }

    /// Say `message` to the tool. `false` when no tool is connected.
    pub fn tell(&self, message: &AppMessage) -> bool {
        self.send(message)
    }

    fn listening(&self) -> bool {
        matches!(*self.last.lock().unwrap_or_else(|p| p.into_inner()), Some(ToolMessage::State { listening: true, .. }))
    }

    fn send(&self, message: &AppMessage) -> bool {
        let mut writer = self.writer.lock().unwrap_or_else(|p| p.into_inner());
        match writer.as_mut() {
            Some(stream) => {
                let ok = ipc::send(stream, message).is_ok();
                if !ok {
                    *writer = None;
                }
                ok
            }
            None => false,
        }
    }

    /// **The talk button was pressed.** If a dictation is listening, it is written as if the key
    /// had been tapped; this waits (at most 3 s) until it has stopped listening, so voice mode and
    /// dictation never hold the microphone at once. `true` when voice mode may open: no
    /// dictation was listening, or it has stopped. `false` when it still listens, or the tool
    /// could not be told: voice mode must NOT open then (finding 2).
    pub fn finish(&self) -> bool {
        if !self.listening() {
            return true;
        }
        if !self.send(&AppMessage::Finish) {
            // A tool that cannot be told is a tool whose socket closed; its state is cleared
            // with it, so re-read before refusing.
            return !self.listening();
        }
        let deadline = Instant::now() + FINISH_WITHIN;
        let mut last = self.last.lock().unwrap_or_else(|p| p.into_inner());
        while matches!(*last, Some(ToolMessage::State { listening: true, .. })) {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                eprintln!("[richos] voice: the dictation did not stop listening within 3 s; voice mode does not open");
                return false;
            }
            last = self.changed.wait_timeout(last, left).unwrap_or_else(|p| p.into_inner()).0;
        }
        true
    }

    /// Hear the tool until its socket closes. At the close, the host is told FIRST, while the
    /// tool's last word is still here to read (`tool_said_off`: recheck, third review, finding
    /// 2: clearing it first made every ended tool look as if it had not said off), and only then
    /// is the state cleared and the window told.
    fn hear(self: &Arc<Self>, reader: UnixStream, voice: Arc<dyn VoiceMode>) {
        ipc::read_lines::<ToolMessage>(reader, |m| on_tool_message(&m, voice.as_ref(), self));
        *self.writer.lock().unwrap_or_else(|p| p.into_inner()) = None;
        eprintln!("[richos] dictation: the tool's socket closed");
        if let Some(on_closed) = self.on_closed.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            on_closed();
        }
        *self.last.lock().unwrap_or_else(|p| p.into_inner()) = None;
        self.changed.notify_all();
        self.tell_listener(None);
    }

    /// Connect to the tool's socket and start hearing it.
    fn connect(self: &Arc<Self>, hello: AppMessage, voice: Arc<dyn VoiceMode>) -> std::io::Result<()> {
        let socket = ipc::runtime_dir().join(ipc::SOCKET_NAME);
        let stream = UnixStream::connect(socket)?;
        let reader = stream.try_clone()?;
        *self.writer.lock().unwrap_or_else(|p| p.into_inner()) = Some(stream);
        let link = self.clone();
        std::thread::Builder::new().name("dictation-link".into()).spawn(move || link.hear(reader, voice))?;
        self.send(&hello);
        Ok(())
    }
}

/// **Whether a tool that went away is started again** (finding 5), pure: on, and fewer than
/// [`RESTART_LIMIT`] restarts inside [`RESTART_WINDOW`].
pub fn restart_allowed(on: bool, recent_restarts: &[Instant], now: Instant) -> bool {
    on && recent_restarts.iter().filter(|t| now.duration_since(**t) < RESTART_WINDOW).count() < RESTART_LIMIT
}

// =============================================================================================
// STARTING THE TOOL
// =============================================================================================

/// The test switch's value, if it is set to `1` or `0`.
pub fn test_switch(value: Option<&str>) -> Option<bool> {
    match value.map(str::trim) {
        Some("1") => Some(true),
        Some("0") => Some(false),
        _ => None,
    }
}

/// The child's command line: this executable, the tool's argument, this app's data folder and
/// this process as its parent.
pub fn child_args(data_dir: &Path, parent: u32) -> Vec<std::ffi::OsString> {
    vec![
        crate::dictation::tool::ARG.into(),
        "--data-dir".into(),
        data_dir.as_os_str().to_owned(),
        "--parent".into(),
        parent.to_string().into(),
    ]
}

/// **At app start**, on a thread of its own so first paint never waits for it: apply the test
/// switch, and when `dictation.json` says on, start the tool as this app's child and connect the
/// host's link to whichever tool holds the key.
pub fn boot(host: Arc<Host>) {
    let spawned = std::thread::Builder::new().name("dictation-boot".into()).spawn(move || {
        if let Some(on) = test_switch(std::env::var(TEST_SWITCH).ok().as_deref()) {
            match host.update(|s| s.on = on) {
                Ok(_) => eprintln!("[richos] dictation: turned {} by the test switch ({TEST_SWITCH})", if on { "on" } else { "off" }),
                Err(why) => eprintln!("[richos] dictation: the test switch could not write dictation.json: {why}"),
            }
        }
        host.ensure_tool();
    });
    if let Err(e) = spawned {
        eprintln!("[richos] dictation: could not start: {e}");
    }
}

// =============================================================================================
// THE HOST: this app's dictation, for the Dictation sheet and the Settings row (slice 2)
// =============================================================================================

/// **This app's dictation**: the settings file (the app is its only writer), the tool it
/// starts, the link to whichever tool holds the key, and the two macOS permissions it asks for
/// (plan section 4: asked by the app, whose identity the tool shares).
pub struct Host {
    pub link: Arc<Link>,
    data_dir: PathBuf,
    version: String,
    voice: Arc<dyn VoiceMode>,
    /// macOS's microphone prompt is up: `requestAccess` has not answered yet.
    mic_asking: Arc<AtomicBool>,
    /// One read-modify-write of `dictation.json` at a time in this process (the file lock in
    /// `store::update` is what excludes the other processes).
    writing: Mutex<()>,
    /// One tool start at a time (the boot, the switch and the sheet can each ask).
    starting: Mutex<()>,
    /// When a tool that went away was started again (finding 5's bound).
    restarts: Mutex<Vec<Instant>>,
    /// This host, for the link's closed callback.
    me: Weak<Host>,
}

impl Host {
    /// The tool is this app's child and runs only while RichOS runs (the CEO, 2026-10-08: no
    /// start at login in this release).
    pub fn new(link: Arc<Link>, data_dir: PathBuf, version: String, voice: Arc<dyn VoiceMode>) -> Arc<Host> {
        let host = Arc::new_cyclic(|me| Host {
            link: link.clone(),
            data_dir,
            version,
            voice,
            mic_asking: Arc::new(AtomicBool::new(false)),
            writing: Mutex::new(()),
            starting: Mutex::new(()),
            restarts: Mutex::new(Vec::new()),
            me: me.clone(),
        });
        let weak = host.me.clone();
        link.set_on_closed(Box::new(move || {
            if let Some(host) = weak.upgrade() {
                host.tool_gone();
            }
        }));
        host
    }

    /// The settings on disk.
    pub fn settings(&self) -> Result<store::Settings, String> {
        store::read(&self.data_dir)
    }

    /// Change the settings: read, change, write, under the file lock (and one at a time in this
    /// process). A file that cannot be read is never overwritten.
    pub fn update(&self, change: impl FnOnce(&mut store::Settings)) -> Result<store::Settings, String> {
        let one_writer = self.writing.lock().unwrap_or_else(|p| p.into_inner());
        let settings = store::update(&self.data_dir, change);
        drop(one_writer);
        settings
    }

    /// **The tool runs while dictation is on.** When `dictation.json` says on and no tool is
    /// connected: in an installed copy, register the LaunchAgent (idempotent; launchd starts
    /// the tool at once) and connect; anywhere else, start one as this app's child. Either way
    /// the link ends up on whichever tool holds the key (its own, or another copy's: a child
    /// that finds the key taken exits with 3).
    pub fn ensure_tool(&self) {
        let one_start = self.starting.lock().unwrap_or_else(|p| p.into_inner());
        self.start_tool_now();
        drop(one_start);
    }

    /// **The tool's socket closed** (finding 5): the tool ended by itself (off, exit 3, a
    /// version restart) or crashed. While `dictation.json` still says on, it is started again
    /// after a moment, at most [`RESTART_LIMIT`] times in [`RESTART_WINDOW`]; in an installed
    /// copy launchd restarts it and this only reconnects.
    pub fn tool_gone(&self) {
        // Off is never undone (recheck finding 2): a tool that said "off" before it ended was
        // turned off by a switch or the menu bar, in this copy or another; this copy's own
        // switch follows, so its recovery never starts a tool that reverses that Off.
        if self.link.tool_said_off() {
            match self.update(|s| s.on = false) {
                Ok(_) => eprintln!("[richos] dictation: the tool ended because dictation was turned off; this copy's switch follows it"),
                Err(why) => eprintln!("[richos] dictation: the tool ended because dictation was turned off, and this copy's switch could not follow: {why}"),
            }
            return;
        }
        let on = self.settings().map(|s| s.on).unwrap_or(false);
        let allowed = self.restart_permitted(on);
        if !on {
            return;
        }
        if !allowed {
            eprintln!("[richos] dictation: the tool went away {RESTART_LIMIT} times in a minute; not started again until the switch is used");
            return;
        }
        let Some(host) = self.me.upgrade() else { return };
        let spawned = std::thread::Builder::new().name("dictation-restart".into()).spawn(move || {
            std::thread::sleep(RESTART_AFTER);
            eprintln!("[richos] dictation: the tool went away while dictation is on; starting it again");
            host.ensure_tool();
        });
        if let Err(e) = spawned {
            eprintln!("[richos] dictation: the tool could not be started again: {e}");
        }
    }

    /// `ensure_tool`, off this thread, when dictation is on and no tool is connected: what a
    /// read of the sheet or the row does, so a missing tool is never only reported (finding 5).
    /// `true` when a tool may be started again now: dictation is on, and fewer than
    /// [`RESTART_LIMIT`] starts were granted in the last [`RESTART_WINDOW`]. A grant is counted
    /// here, so the sheet's reads (`reconnect_in_background`) and the socket's close
    /// (`tool_gone`) share one bound (recheck, third review, finding 3).
    fn restart_permitted(&self, on: bool) -> bool {
        let now = Instant::now();
        let mut recent = self.restarts.lock().unwrap_or_else(|p| p.into_inner());
        recent.retain(|t| now.duration_since(*t) < RESTART_WINDOW);
        let allowed = restart_allowed(on, &recent, now);
        if allowed {
            recent.push(now);
        }
        allowed
    }

    /// A tool that should be running and is not is started again off this thread, within the
    /// restart bound. `true` when a start was set going.
    fn reconnect_in_background(&self) -> bool {
        if self.link.connected() || !self.settings().map(|s| s.on).unwrap_or(false) {
            return false;
        }
        if self.starting.try_lock().is_err() {
            return false; // a start is under way
        }
        if !self.restart_permitted(true) {
            return false; // the bound: not started again until the switch is used
        }
        let Some(host) = self.me.upgrade() else { return false };
        std::thread::Builder::new().name("dictation-reconnect".into()).spawn(move || host.ensure_tool()).is_ok()
    }

    fn start_tool_now(&self) {
        let settings = match self.settings() {
            Ok(s) => s,
            Err(why) => {
                eprintln!("[richos] dictation: {why}");
                return;
            }
        };
        if !settings.on || self.link.connected() {
            return;
        }
        let exe = match std::env::current_exe() {
            Ok(e) => e,
            Err(e) => {
                eprintln!("[richos] dictation: this executable's path is unknown, so the tool cannot start: {e}");
                return;
            }
        };
        {
            let mut command = std::process::Command::new(&exe);
            command.args(child_args(&self.data_dir, std::process::id())).stdin(std::process::Stdio::null());
            if let Some(bin) = richos_voice::stt::delivered_runtime_bin_dir() {
                command.env("RICHOS_DICTATION_RUNTIME_BIN", bin);
            }
            match command.spawn() {
                Ok(mut child) => {
                    let pid = child.id();
                    eprintln!("[richos] dictation: the tool started as this app's child (pid {pid})");
                    // Reaped here, and its end said: exit 3 is another copy holding the key.
                    let reaper = std::thread::Builder::new().name("dictation-child".into()).spawn(move || {
                        if let Ok(status) = child.wait() {
                            eprintln!("[richos] dictation: the tool (pid {pid}) ended: {status}");
                        }
                    });
                    if let Err(e) = reaper {
                        eprintln!("[richos] dictation: the tool's end will not be reported: {e}");
                    }
                }
                Err(e) => eprintln!("[richos] dictation: the tool could not be started: {e}"),
            }
        }
        let hello = AppMessage::Hello {
            version: self.version.clone(),
            bundle: bundle_of(&exe).display().to_string(),
            data_dir: self.data_dir.display().to_string(),
        };
        let started = Instant::now();
        loop {
            match self.link.connect(hello.clone(), self.voice.clone()) {
                Ok(()) => {
                    eprintln!("[richos] dictation: connected to the tool");
                    break;
                }
                Err(_) if started.elapsed() < CONNECT_WITHIN => std::thread::sleep(Duration::from_millis(100)),
                Err(e) => {
                    eprintln!("[richos] dictation: no tool answered within 5 s: {e}");
                    break;
                }
            }
        }
    }

    /// **What the sheet and the row show**, read now. A tool that should be running and is not
    /// is started again off this thread (finding 5).
    pub fn view(&self) -> Result<View, String> {
        self.reconnect_in_background();
        let settings = self.settings()?;
        let own = std::env::current_exe().map(|e| bundle_of(&e).display().to_string()).unwrap_or_default();
        let tool = self.link.owner_and_tap();
        let tool_settings = self.link.tool_settings();
        Ok(view_of(&Facts {
            ready: richos_core::dictation_ready(),
            settings: &settings,
            mic_status: mac::mic_status(),
            mic_asking: self.mic_asking.load(Ordering::SeqCst),
            ax_trusted: crate::dictation::keytap::accessibility_allowed(),
            owner: tool.as_ref().map(|(owner, _)| owner.as_str()),
            own_bundle: &own,
            key_tap: tool.as_ref().map(|(_, tap)| *tap).unwrap_or(false),
            secure: mac::secure_input(),
            tool_key: tool_settings.as_ref().map(|(k, _)| *k),
            tool_model: tool_settings.as_ref().map(|(_, m)| m.as_str()),
        }))
    }

    /// Another copy's tool holds the key: its settings are the ones that count (finding 4).
    fn owned_elsewhere(&self) -> bool {
        let own = std::env::current_exe().map(|e| bundle_of(&e).display().to_string()).unwrap_or_default();
        matches!(self.link.owner_and_tap(), Some((owner, _)) if owner != own)
    }

    /// **The switch.** On: the setting is written and the tool started (it asks for nothing; the
    /// window asks for the two permissions next, in the drawn order). Off: the tool is told, and
    /// it stops (whichever copy's tool it is: one switch, one dictation on this Mac).
    pub fn set_on(&self, on: bool) -> Result<View, String> {
        self.update(|s| s.on = on)?;
        if on {
            self.ensure_tool();
        } else if self.owned_elsewhere() {
            self.link.tell(&AppMessage::Change { on: Some(false), key: None, model: None });
        } else {
            self.link.tell(&AppMessage::SettingsChanged);
        }
        self.view()
    }

    /// **Your key**: F1 to F19, nothing else. With another copy's tool holding the key, the
    /// change goes to that tool's file, through it, so it takes effect (finding 4).
    pub fn set_key(&self, key: u8) -> Result<View, String> {
        if !(1..=19).contains(&key) {
            return Err(format!("F{key} is not a key dictation can use"));
        }
        if self.owned_elsewhere() {
            self.link.tell(&AppMessage::Change { on: None, key: Some(key), model: None });
            return self.view();
        }
        self.update(|s| s.key = key)?;
        self.link.tell(&AppMessage::SettingsChanged);
        self.view()
    }

    /// **Accuracy**: More accurate or Faster. The model id is stored; the label is derived from
    /// it, never stored beside it. Through the owner's tool when another copy holds the key.
    pub fn set_accuracy(&self, accuracy: &str) -> Result<View, String> {
        let model = model_for(accuracy).ok_or_else(|| format!("{accuracy} is not an accuracy dictation offers"))?;
        if self.owned_elsewhere() {
            self.link.tell(&AppMessage::Change { on: None, key: None, model: Some(model.to_string()) });
            return self.view();
        }
        self.update(|s| s.model = model.to_string())?;
        self.link.tell(&AppMessage::SettingsChanged);
        self.view()
    }

    /// **macOS's microphone prompt**, once: only when macOS has not been asked yet. The prompt
    /// carries `NSMicrophoneUsageDescription`; the microphone is never opened by asking. The
    /// answer arrives on its own; the window reads it from `view` once a second.
    pub fn ask_microphone(&self) -> Result<View, String> {
        if mac::mic_status() == mac::MIC_NOT_DETERMINED && !self.mic_asking.swap(true, Ordering::SeqCst) {
            if let Err(why) = self.update(|s| s.mic_asked = true) {
                self.mic_asking.store(false, Ordering::SeqCst);
                return Err(why);
            }
            let asking = self.mic_asking.clone();
            if !mac::request_mic(Box::new(move |granted| {
                asking.store(false, Ordering::SeqCst);
                eprintln!("[richos] dictation: the microphone was {}", if granted { "allowed" } else { "not allowed" });
            })) {
                self.mic_asking.store(false, Ordering::SeqCst);
                return Err("macOS could not be asked for the microphone".into());
            }
        }
        self.view()
    }

    /// **macOS's Accessibility prompt**, once (`axAsked`): after that, "denied" is known because
    /// RichOS asked and is still not trusted, and the sheet offers Open System Settings instead.
    /// `true` when the prompt was put up now.
    pub fn ask_accessibility(&self) -> Result<bool, String> {
        let settings = self.settings()?;
        if crate::dictation::keytap::accessibility_allowed() || settings.ax_asked {
            return Ok(false);
        }
        self.update(|s| s.ax_asked = true)?;
        Ok(mac::prompt_accessibility())
    }

    /// **A permission changed** (Accessibility allowed in System Settings): the tool creates its
    /// key tap now, with no relaunch; a tool that is not running is started.
    pub fn permissions_changed(&self) {
        if !self.link.tell(&AppMessage::PermissionsChanged) {
            self.ensure_tool();
        }
    }

    /// **Key capture over the socket.** `true` when the tool's key tap will answer with the next
    /// F-key; `false` when there is no tap (the window's own keys answer then).
    pub fn capture(&self, on: bool) -> bool {
        let tap = self.link.owner_and_tap().map(|(_, tap)| tap).unwrap_or(false);
        let sent = self.link.tell(if on { &AppMessage::CaptureNextKey } else { &AppMessage::CancelCapture });
        on && tap && sent
    }
}

// =============================================================================================
// THE VIEW: what the sheet and the row are drawn from, decided here so each rule is a test
// =============================================================================================

/// Another app has Secure Event Input on: every key is hidden from every tap.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Secure {
    /// The app macOS names for it, when it names one.
    pub app: Option<String>,
}

/// What the sheet and the row show. Words are the window's (round 19); these are the facts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    /// The ways in are shown (`richos_core::dictation_ready`).
    pub ready: bool,
    pub on: bool,
    /// The chosen key as an F-number.
    pub key: u8,
    /// `accurate` or `fast`, derived from the model id.
    pub accuracy: &'static str,
    /// `unknown` (never asked), `asking` (macOS's prompt is up), `allowed` or `denied`.
    pub mic: &'static str,
    /// `unknown` (never asked), `allowed` or `denied` (asked once and still not trusted). The
    /// window adds `asking` while it waits for System Settings.
    pub ax: &'static str,
    /// Whose tool holds the key: `self`, `other` (another copy of RichOS) or `none`.
    pub owner: &'static str,
    /// The tool's key tap exists (so key capture goes through it).
    pub key_tap: bool,
    /// Another app is hiding keys, or `None`.
    pub secure: Option<Secure>,
}

/// Everything [`view_of`] reads.
pub struct Facts<'a> {
    pub ready: bool,
    pub settings: &'a store::Settings,
    pub mic_status: i64,
    pub mic_asking: bool,
    pub ax_trusted: bool,
    pub owner: Option<&'a str>,
    pub own_bundle: &'a str,
    pub key_tap: bool,
    pub secure: Option<Option<String>>,
    /// The key and model the tool reports (finding 4): shown instead of this copy's own file
    /// when another copy's tool holds the key.
    pub tool_key: Option<u8>,
    pub tool_model: Option<&'a str>,
}

pub fn view_of(f: &Facts) -> View {
    let owner = owner_word(f.owner, f.own_bundle);
    let elsewhere = owner == "other";
    View {
        ready: f.ready,
        on: f.settings.on,
        key: match (elsewhere, f.tool_key) {
            (true, Some(k)) if (1..=19).contains(&k) => k,
            _ => f.settings.key(),
        },
        accuracy: match (elsewhere, f.tool_model) {
            (true, Some(m)) => accuracy_of(m),
            _ => accuracy_of(&f.settings.model),
        },
        mic: mic_word(f.mic_status, f.mic_asking),
        ax: ax_word(f.ax_trusted, f.settings.ax_asked),
        owner,
        key_tap: f.key_tap,
        secure: f.secure.clone().map(|app| Secure { app }),
    }
}

/// `AVAuthorizationStatus`: 0 not determined, 1 restricted, 2 denied, 3 authorized.
pub fn mic_word(status: i64, asking: bool) -> &'static str {
    match status {
        3 => "allowed",
        1 | 2 => "denied",
        _ if asking => "asking",
        _ => "unknown",
    }
}

/// Accessibility: trusted is allowed; asked once and not trusted is denied (macOS says nothing
/// finer, plan section 2 row 12); never asked is unknown.
pub fn ax_word(trusted: bool, asked: bool) -> &'static str {
    match (trusted, asked) {
        (true, _) => "allowed",
        (false, true) => "denied",
        (false, false) => "unknown",
    }
}

/// The tool that answered is this copy's, another copy's, or there is none.
pub fn owner_word(owner: Option<&str>, own_bundle: &str) -> &'static str {
    match owner {
        None => "none",
        Some(o) if o == own_bundle => "self",
        Some(_) => "other",
    }
}

/// The accuracy label, derived from the model id (never stored beside it).
pub fn accuracy_of(model: &str) -> &'static str {
    if model == richos_voice::dictation::FASTER { "fast" } else { "accurate" }
}

/// The model id for an accuracy label.
pub fn model_for(accuracy: &str) -> Option<&'static str> {
    match accuracy {
        "accurate" => Some(richos_voice::dictation::MORE_ACCURATE),
        "fast" => Some(richos_voice::dictation::FASTER),
        _ => None,
    }
}

/// System Settings, opened on the pane a refusal names. Two fixed addresses, never one built
/// from what the window sent.
pub fn privacy_pane(pane: &str) -> Option<&'static str> {
    match pane {
        "microphone" => Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"),
        "accessibility" => Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"),
        _ => None,
    }
}

// =============================================================================================
// THE MACOS EDGES: AVFoundation, Accessibility, Secure Event Input, the system version
// =============================================================================================

pub mod mac {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::string::{CFString, CFStringRef};
    use objc2::msg_send;
    use objc2::runtime::{AnyClass, AnyObject, Bool};
    use std::sync::Mutex;

    /// `AVAuthorizationStatusNotDetermined`.
    pub const MIC_NOT_DETERMINED: i64 = 0;

    #[link(name = "AVFoundation", kind = "framework")]
    extern "C" {
        static AVMediaTypeAudio: *mut AnyObject;
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        static kAXTrustedCheckOptionPrompt: CFStringRef;
        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
    }

    /// `AVCaptureDevice authorizationStatusForMediaType:AVMediaTypeAudio`. Reading it opens
    /// nothing and asks nothing.
    pub fn mic_status() -> i64 {
        objc2::rc::autoreleasepool(|_| {
            let Some(class) = AnyClass::get(c"AVCaptureDevice") else { return MIC_NOT_DETERMINED };
            // SAFETY: a documented class method taking AVFoundation's own media-type constant.
            let status: isize = unsafe { msg_send![class, authorizationStatusForMediaType: AVMediaTypeAudio] };
            status as i64
        })
    }

    /// `AVCaptureDevice requestAccessForMediaType:completionHandler:`. macOS shows its prompt
    /// with RichOS's usage sentence; `done` hears the answer, on a queue of macOS's choosing.
    /// `false` when the request could not be made.
    pub fn request_mic(done: Box<dyn FnOnce(bool) + Send>) -> bool {
        let Some(class) = AnyClass::get(c"AVCaptureDevice") else { return false };
        let once = Mutex::new(Some(done));
        let block = block2::RcBlock::new(move |granted: Bool| {
            if let Some(done) = once.lock().unwrap_or_else(|p| p.into_inner()).take() {
                done(granted.as_bool());
            }
        });
        // SAFETY: the documented class method; AVFoundation copies the block it is given.
        unsafe {
            let () = msg_send![class, requestAccessForMediaType: AVMediaTypeAudio, completionHandler: &*block];
        }
        true
    }

    /// `AXIsProcessTrustedWithOptions` with the prompt option: macOS puts up its own
    /// Accessibility prompt (its wording; an app cannot change it). `true` when it was asked.
    pub fn prompt_accessibility() -> bool {
        // SAFETY: a constant ApplicationServices exports, read under the get rule.
        let key = unsafe { CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt) };
        let options = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), CFBoolean::true_value().as_CFType())]);
        // SAFETY: a valid dictionary for the duration of the call.
        let trusted = unsafe { AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef()) };
        !trusted
    }

    /// **Secure Event Input** (plan minor 5): `None` when it is off; `Some(app)` when another app
    /// has it on, naming that app when macOS reports its process. Slice 3's reader
    /// (`dictation::appkit::secure_input`), so the app and the tool read it one way and each
    /// macOS symbol has one declaration (two clashed at the merge with main).
    pub fn secure_input() -> Option<Option<String>> {
        crate::dictation::appkit::secure_input()
    }
}

fn bundle_of(exe: &Path) -> PathBuf {
    exe.ancestors().nth(3).map(Path::to_path_buf).unwrap_or_default()
}

// =============================================================================================
// THE COMMANDS: what the Dictation sheet and the Settings row ask (slice 2)
// =============================================================================================

/// The window's event: `{"changed": true}` when the tool's state moved (or its socket closed),
/// `{"key": n}` when key capture answered.
pub const EVENT: &str = "rich://dictation";

/// What the window is told for one thing the tool said.
pub fn event_payload(message: Option<&ToolMessage>) -> serde_json::Value {
    match message {
        Some(ToolMessage::Key { key }) => serde_json::json!({ "key": key }),
        _ => serde_json::json!({ "changed": true }),
    }
}

fn host(app: &tauri::AppHandle) -> Result<Arc<Host>, String> {
    use tauri::Manager;
    app.try_state::<Arc<Host>>().map(|h| h.inner().clone()).ok_or_else(|| "dictation has not started in this window yet".to_string())
}

/// What the sheet and the row show, read now.
#[richos_command::command]
pub fn dictation_status(app: tauri::AppHandle) -> Result<View, String> {
    host(&app)?.view()
}

/// The switch.
#[richos_command::command]
pub fn dictation_set_on(app: tauri::AppHandle, on: bool) -> Result<View, String> {
    host(&app)?.set_on(on)
}

/// Your key, F1 to F19.
#[richos_command::command]
pub fn dictation_set_key(app: tauri::AppHandle, key: u8) -> Result<View, String> {
    host(&app)?.set_key(key)
}

/// Accuracy: `accurate` or `fast`.
#[richos_command::command]
pub fn dictation_set_accuracy(app: tauri::AppHandle, accuracy: String) -> Result<View, String> {
    host(&app)?.set_accuracy(&accuracy)
}

/// macOS's microphone prompt, once.
#[richos_command::command]
pub fn dictation_ask_microphone(app: tauri::AppHandle) -> Result<View, String> {
    host(&app)?.ask_microphone()
}

/// macOS's Accessibility prompt, once. `true` when it was put up now.
#[richos_command::command]
pub fn dictation_ask_accessibility(app: tauri::AppHandle) -> Result<bool, String> {
    host(&app)?.ask_accessibility()
}

/// **Accessibility was allowed while the sheet waited** (plan section 2 row 9): the tool makes
/// its key tap now, and RichOS comes back to the front on the sheet, as drawn. `forward: false`
/// when the window found it allowed with nothing waiting (it is in front already).
#[richos_command::command]
pub fn dictation_permissions_changed(app: tauri::AppHandle, forward: Option<bool>) -> Result<(), String> {
    use tauri::Manager;
    host(&app)?.permissions_changed();
    if !forward.unwrap_or(true) {
        return Ok(());
    }
    if let Some(window) = app.webview_windows().values().next() {
        for (what, result) in [("shown", window.show()), ("unminimized", window.unminimize()), ("brought to the front", window.set_focus())] {
            if let Err(e) = result {
                eprintln!("[richos] dictation: the window could not be {what}: {e}");
            }
        }
    }
    Ok(())
}

/// **Open System Settings** on the pane a refusal names: `microphone` or `accessibility`.
#[richos_command::command]
pub fn dictation_open_settings(pane: String) -> Result<(), String> {
    let url = privacy_pane(&pane).ok_or_else(|| format!("{pane} is not a pane dictation opens"))?;
    let status = std::process::Command::new("/usr/bin/open").arg(url).status().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("System Settings did not open ({status})"))
    }
}

/// **Press a different key**: `on` asks the tool's key tap for the next F-key (answered on
/// [`EVENT`] as `{"key": n}`); `false` stops waiting. `true` when the tap will answer.
#[richos_command::command]
pub fn dictation_capture_key(app: tauri::AppHandle, on: bool) -> Result<bool, String> {
    Ok(host(&app)?.capture(on))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    /// A voice mode with one session's gate and a counter of turns sent.
    struct FakeVoice {
        on: AtomicBool,
        gate: TurnGate,
    }
    impl VoiceMode for FakeVoice {
        fn end_for_dictation(&self) -> bool {
            self.gate.close();
            self.on.swap(false, Ordering::SeqCst)
        }
    }

    /// INVARIANT (plan section 2): `will-listen` during voice mode ends it, and the utterance it
    /// was still holding is never sent as a turn.
    #[test]
    fn will_listen_ends_voice_mode_and_sends_no_turn() {
        let sent = Arc::new(AtomicUsize::new(0));
        let counter = sent.clone();
        let voice = FakeVoice { on: AtomicBool::new(true), gate: TurnGate::new() };
        let submit = gated_submit(voice.gate.clone(), Arc::new(move |_t: String, _a: bool| {
            counter.fetch_add(1, Ordering::SeqCst);
        }));
        submit("a turn before dictation".into(), false);
        assert_eq!(sent.load(Ordering::SeqCst), 1);
        let link = Link::default();
        on_tool_message(&ToolMessage::WillListen { seq: 9 }, &voice, &link);
        assert!(!voice.on.load(Ordering::SeqCst), "voice mode ended");
        // The recognizer drains what it had: it reaches the gate, and stops there.
        submit("the utterance in progress".into(), false);
        assert_eq!(sent.load(Ordering::SeqCst), 1, "no turn sent after dictation took the microphone");
    }

    /// INVARIANT (slice 3): the tool's **Open RichOS**, **Dictation settings…** and **Fix it**
    /// reach the window with the sheet flag they carry, and the menu's change to dictation.json
    /// is passed on; neither ends voice mode.
    #[test]
    fn come_forward_and_settings_changed_reach_the_window() {
        #[derive(Default)]
        struct Window {
            forward: Mutex<Vec<bool>>,
            changed: AtomicUsize,
            ended: AtomicBool,
        }
        impl VoiceMode for Window {
            fn end_for_dictation(&self) -> bool {
                self.ended.store(true, Ordering::SeqCst);
                false
            }
            fn come_forward(&self, sheet: bool) {
                self.forward.lock().unwrap().push(sheet);
            }
            fn settings_changed(&self) {
                self.changed.fetch_add(1, Ordering::SeqCst);
            }
        }
        let window = Window::default();
        let link = Link::default();
        on_tool_message(&ToolMessage::ComeForward { sheet: true }, &window, &link);
        on_tool_message(&ToolMessage::ComeForward { sheet: false }, &window, &link);
        on_tool_message(&ToolMessage::SettingsChanged, &window, &link);
        assert_eq!(*window.forward.lock().unwrap(), vec![true, false]);
        assert_eq!(window.changed.load(Ordering::SeqCst), 1);
        assert!(!window.ended.load(Ordering::SeqCst));
    }

    fn state(listening: bool) -> ToolMessage {
        ToolMessage::State { owner: "x".into(), on: true, listening, writing: false, problem: None, key_tap: true, secure_input: false, secure_app: None, key: 1, model: "large-v3-turbo-q5_0".into() }
    }

    /// INVARIANT: a state message is only recorded; it never ends voice mode.
    #[test]
    fn a_state_message_never_ends_voice_mode() {
        let voice = FakeVoice { on: AtomicBool::new(true), gate: TurnGate::new() };
        let link = Link::default();
        on_tool_message(&state(true), &voice, &link);
        assert!(voice.on.load(Ordering::SeqCst));
        assert!(!voice.gate.closed());
        assert!(link.listening());
    }

    /// INVARIANT (finding 2): `will-listen` is answered with `voice-yielded` only after voice
    /// mode was ended, so the tool's microphone opens after this app's closed, never beside it.
    #[test]
    fn will_listen_is_answered_after_voice_mode_ended() {
        let (app_end, tool_end) = UnixStream::pair().unwrap();
        let link = Link::default();
        *link.writer.lock().unwrap() = Some(app_end);
        let voice = FakeVoice { on: AtomicBool::new(true), gate: TurnGate::new() };
        on_tool_message(&ToolMessage::WillListen { seq: 9 }, &voice, &link);
        assert!(!voice.on.load(Ordering::SeqCst), "voice mode ended before the answer");
        *link.writer.lock().unwrap() = None;
        let mut heard = Vec::new();
        ipc::read_lines::<AppMessage>(tool_end, |m| heard.push(m));
        let own = std::env::current_exe().map(|e| bundle_of(&e).display().to_string()).unwrap();
        assert_eq!(heard, vec![AppMessage::VoiceYielded { seq: 9, bundle: own }], "the answer names the handover and this copy");
    }

    /// INVARIANT (third review, finding 2): the tool's last word survives its socket closing
    /// long enough for the host to read it. The review's counterexample: off observed, then at
    /// EOF `tool_said_off` was false because the reader had cleared the state before telling
    /// the host; through the real reader, the host now hears the close with the off still there.
    #[test]
    fn the_tools_off_is_still_there_when_the_socket_closes() {
        let (app_end, mut tool_end) = UnixStream::pair().unwrap();
        let link = Arc::new(Link::default());
        *link.writer.lock().unwrap() = Some(app_end.try_clone().unwrap());
        let said_off_at_close = Arc::new(AtomicBool::new(false));
        let seen = said_off_at_close.clone();
        let weak = Arc::downgrade(&link);
        link.set_on_closed(Box::new(move || {
            if let Some(link) = weak.upgrade() {
                seen.store(link.tool_said_off(), Ordering::SeqCst);
            }
        }));
        let off = ToolMessage::State {
            owner: "/other/RichOS.app".into(),
            on: false,
            listening: false,
            writing: false,
            problem: None,
            key_tap: false,
            secure_input: false,
            secure_app: None,
            key: 1,
            model: "small.en".into(),
        };
        ipc::send(&mut tool_end, &off).unwrap();
        drop(tool_end);
        let voice: Arc<dyn VoiceMode> = Arc::new(FakeVoice { on: AtomicBool::new(false), gate: TurnGate::new() });
        link.hear(app_end, voice);
        assert!(said_off_at_close.load(Ordering::SeqCst), "the host read the off at the close");
        assert!(!link.tool_said_off(), "and the state is cleared after the host was told");
        assert!(!link.connected());
    }

    /// INVARIANT (third review, finding 3): the sheet's reads share the restart bound with the
    /// socket's close: after RESTART_LIMIT grants in RESTART_WINDOW, a read starts no tool.
    #[test]
    fn a_read_of_the_sheet_keeps_the_restart_bound() {
        let r = rig("read-bound");
        r.host.update(|s| s.on = true).unwrap();
        *r.host.link.writer.lock().unwrap() = None;
        for _ in 0..RESTART_LIMIT {
            assert!(r.host.restart_permitted(true));
        }
        assert!(!r.host.restart_permitted(true), "the bound");
        assert!(!r.host.reconnect_in_background(), "a read past the bound starts nothing");
        std::fs::remove_dir_all(&r.dir).ok();
    }

    /// INVARIANT: `finish` with no tool, or with a tool that is not listening, says nothing to
    /// the tool and lets voice mode open.
    #[test]
    fn finish_without_a_listening_dictation_says_nothing() {
        assert!(Link::default().finish());
        let (app_end, tool_end) = UnixStream::pair().unwrap();
        let link = Link::default();
        *link.writer.lock().unwrap() = Some(app_end);
        assert!(link.finish());
        link.observe(state(false));
        assert!(link.finish());
        *link.writer.lock().unwrap() = None; // closes the app's end
        let mut heard = Vec::new();
        ipc::read_lines::<AppMessage>(tool_end, |m| heard.push(m));
        assert!(heard.is_empty(), "{heard:?}");
    }

    /// INVARIANT: with a dictation listening, `finish` says so to the tool and returns `true`
    /// only once the tool reports it has stopped listening.
    #[test]
    fn finish_returns_once_the_dictation_has_stopped_listening() {
        let (app_end, tool_end) = UnixStream::pair().unwrap();
        let link = Arc::new(Link::default());
        *link.writer.lock().unwrap() = Some(app_end);
        link.observe(state(true));
        let heard_finish = Arc::new(AtomicBool::new(false));
        let tool_link = link.clone();
        let tool_heard = heard_finish.clone();
        let tool = std::thread::spawn(move || {
            ipc::read_lines::<AppMessage>(tool_end, |m| {
                if m == AppMessage::Finish && !tool_heard.swap(true, Ordering::SeqCst) {
                    tool_link.observe(state(false));
                }
            });
        });
        assert!(link.finish(), "voice mode may open once the dictation stopped listening");
        assert!(heard_finish.load(Ordering::SeqCst), "finish returned before the tool heard it");
        assert!(!link.listening(), "finish returned while the dictation was still listening");
        *link.writer.lock().unwrap() = None; // closes the app's end, so the tool's reader ends
        tool.join().unwrap();
    }

    /// INVARIANT (finding 2): a dictation that does not stop listening within the bound keeps
    /// the microphone: `finish` answers `false`, and voice mode does not open.
    #[test]
    fn finish_refuses_voice_mode_while_the_dictation_still_listens() {
        let (app_end, tool_end) = UnixStream::pair().unwrap();
        let link = Link::default();
        *link.writer.lock().unwrap() = Some(app_end);
        link.observe(state(true));
        let started = Instant::now();
        assert!(!link.finish(), "the tool never said it stopped: voice mode must not open");
        assert!(started.elapsed() >= FINISH_WITHIN - Duration::from_millis(50), "{:?}", started.elapsed());
        *link.writer.lock().unwrap() = None;
        let mut heard = Vec::new();
        ipc::read_lines::<AppMessage>(tool_end, |m| heard.push(m));
        assert_eq!(heard, vec![AppMessage::Finish]);
    }

    /// INVARIANT (recheck finding 2): a tool that said "off" before it ended was turned off by a
    /// switch or the menu bar, in whichever copy; a copy whose own file still says on follows
    /// that Off and starts no tool, so Off is never undone by another copy's recovery.
    #[test]
    fn off_is_never_undone_by_another_copys_recovery() {
        let r = rig("off-follows");
        r.host.update(|s| s.on = true).unwrap();
        let off = ToolMessage::State {
            owner: "/other/RichOS.app".into(),
            on: false,
            listening: false,
            writing: false,
            problem: None,
            key_tap: false,
            secure_input: false,
            secure_app: None,
            key: 1,
            model: "small.en".into(),
        };
        r.host.link.observe(off);
        *r.host.link.writer.lock().unwrap() = None;
        r.host.tool_gone();
        assert!(!r.host.settings().unwrap().on, "this copy's switch follows the Off");
        assert!(r.host.restarts.lock().unwrap().is_empty(), "no restart was counted");
        std::fs::remove_dir_all(&r.dir).ok();
    }

    /// INVARIANT (finding 5): a tool that went away is started again while dictation is on, at
    /// most RESTART_LIMIT times in RESTART_WINDOW, and never while it is off.
    #[test]
    fn a_tool_that_went_away_is_started_again_with_a_bound() {
        let now = Instant::now();
        assert!(restart_allowed(true, &[], now));
        assert!(!restart_allowed(false, &[], now));
        let recent: Vec<Instant> = (0..RESTART_LIMIT).map(|i| now - Duration::from_secs(i as u64)).collect();
        assert!(!restart_allowed(true, &recent, now), "the bound");
        let old: Vec<Instant> = (0..RESTART_LIMIT).map(|_| now - RESTART_WINDOW - Duration::from_secs(1)).collect();
        assert!(restart_allowed(true, &old, now), "restarts outside the window do not count");
    }

    /// INVARIANT: the test switch is exactly `1` or `0`; the child gets this app's data folder
    /// and this process as its parent.
    #[test]
    fn the_switch_and_the_child_line() {
        assert_eq!(test_switch(Some("1")), Some(true));
        assert_eq!(test_switch(Some("0")), Some(false));
        assert_eq!(test_switch(Some("yes")), None);
        assert_eq!(test_switch(None), None);
        let args: Vec<String> = child_args(Path::new("/data"), 77).into_iter().map(|a| a.into_string().unwrap()).collect();
        assert_eq!(args, ["--richos-dictation", "--data-dir", "/data", "--parent", "77"]);
        assert_eq!(bundle_of(Path::new("/A/RichOS.app/Contents/MacOS/richos-tauri")), PathBuf::from("/A/RichOS.app"));
    }

    // ---- slice 2: the sheet's facts --------------------------------------------------------

    /// INVARIANT (plan section 4): the microphone's state is macOS's own answer; "asking" only
    /// while macOS has not answered and its prompt is up.
    #[test]
    fn the_microphone_is_what_macos_says() {
        assert_eq!(mic_word(3, false), "allowed");
        assert_eq!(mic_word(2, false), "denied");
        assert_eq!(mic_word(1, false), "denied", "restricted by a profile is refused too");
        assert_eq!(mic_word(0, true), "asking");
        assert_eq!(mic_word(0, false), "unknown");
        assert_eq!(mic_word(3, true), "allowed", "an answer ends the asking");
    }

    /// INVARIANT (section 2 row 12): Accessibility is "denied" only because RichOS asked once
    /// and is still not trusted; never asked is not a refusal.
    #[test]
    fn accessibility_is_denied_only_after_asking() {
        assert_eq!(ax_word(true, false), "allowed");
        assert_eq!(ax_word(true, true), "allowed");
        assert_eq!(ax_word(false, true), "denied");
        assert_eq!(ax_word(false, false), "unknown");
    }

    /// INVARIANT (M3, line 1): the tool that holds the key is this copy's, another copy's, or
    /// none at all.
    #[test]
    fn whose_tool_holds_the_key() {
        assert_eq!(owner_word(None, "/Applications/RichOS.app"), "none");
        assert_eq!(owner_word(Some("/Applications/RichOS.app"), "/Applications/RichOS.app"), "self");
        assert_eq!(owner_word(Some("/Users/a/myrichos-nightly-a/RichOS.app"), "/Applications/RichOS.app"), "other");
    }

    /// INVARIANT: the accuracy label is derived from the model id, and each label stores the
    /// model the next dictation uses.
    #[test]
    fn accuracy_is_derived_from_the_model() {
        assert_eq!(accuracy_of("large-v3-turbo-q5_0"), "accurate");
        assert_eq!(accuracy_of("small.en"), "fast");
        assert_eq!(model_for("accurate"), Some("large-v3-turbo-q5_0"));
        assert_eq!(model_for("fast"), Some("small.en"));
        assert_eq!(model_for("small.en"), None, "the window names a label, never an id");
        for label in ["accurate", "fast"] {
            assert_eq!(accuracy_of(model_for(label).unwrap()), label);
        }
    }

    /// INVARIANT: Open System Settings opens one of two fixed panes, and nothing else.
    #[test]
    fn open_system_settings_is_two_fixed_panes() {
        assert_eq!(privacy_pane("accessibility"), Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"));
        assert_eq!(privacy_pane("microphone"), Some("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"));
        assert_eq!(privacy_pane("x-apple.systempreferences:anything"), None);
        assert_eq!(privacy_pane(""), None);
    }

    /// INVARIANT: the view is composed of exactly those rules; with another copy's tool holding
    /// the key, the key and accuracy shown are the TOOL's, not this copy's file's (finding 4).
    #[test]
    fn the_view_of_a_fresh_install() {
        let settings = store::Settings::default();
        let v = view_of(&Facts {
            ready: false,
            settings: &settings,
            mic_status: 0,
            mic_asking: false,
            ax_trusted: false,
            owner: None,
            own_bundle: "/Applications/RichOS.app",
            key_tap: false,
            secure: None,
            tool_key: None,
            tool_model: None,
        });
        assert_eq!(
            serde_json::to_value(&v).unwrap(),
            serde_json::json!({"ready": false, "on": false, "key": 1, "accuracy": "accurate", "mic": "unknown", "ax": "unknown",
                "owner": "none", "keyTap": false, "secure": null})
        );
        let on = store::Settings { on: true, key: 5, model: "small.en".into(), mic_asked: true, ax_asked: true, offered: false, feature_thread: None };
        let v = view_of(&Facts {
            ready: true,
            settings: &on,
            mic_status: 3,
            mic_asking: false,
            ax_trusted: false,
            owner: Some("/other/RichOS.app"),
            own_bundle: "/Applications/RichOS.app",
            key_tap: true,
            secure: Some(Some("1Password".into())),
            tool_key: Some(9),
            tool_model: Some("large-v3-turbo-q5_0"),
        });
        assert_eq!(
            serde_json::to_value(&v).unwrap(),
            serde_json::json!({"ready": true, "on": true, "key": 9, "accuracy": "accurate", "mic": "allowed", "ax": "denied",
                "owner": "other", "keyTap": true, "secure": {"app": "1Password"}})
        );
        let own = view_of(&Facts {
            ready: true,
            settings: &on,
            mic_status: 3,
            mic_asking: false,
            ax_trusted: true,
            owner: Some("/Applications/RichOS.app"),
            own_bundle: "/Applications/RichOS.app",
            key_tap: true,
            secure: None,
            tool_key: Some(9),
            tool_model: Some("large-v3-turbo-q5_0"),
        });
        assert_eq!((own.key, own.accuracy), (5, "fast"), "its own file when it owns the key");
    }

    /// INVARIANT: the window hears a captured key as `{"key": n}` and anything else as a change.
    #[test]
    fn the_windows_event() {
        assert_eq!(event_payload(Some(&ToolMessage::Key { key: 9 })), serde_json::json!({"key": 9}));
        assert_eq!(event_payload(Some(&ToolMessage::WillListen { seq: 0 })), serde_json::json!({"changed": true}));
        assert_eq!(event_payload(None), serde_json::json!({"changed": true}));
        let heard = Arc::new(Mutex::new(Vec::new()));
        let into = heard.clone();
        let link = Link::default();
        link.set_listener(Box::new(move |m| into.lock().unwrap().push(event_payload(m))));
        let voice = FakeVoice { on: AtomicBool::new(false), gate: TurnGate::new() };
        on_tool_message(&ToolMessage::Key { key: 4 }, &voice, &link);
        assert_eq!(*heard.lock().unwrap(), vec![serde_json::json!({"key": 4})]);
    }

    // ---- slice 2: the host's writes and what the tool is told -------------------------------

    struct Rig {
        host: Arc<Host>,
        dir: PathBuf,
        tool: UnixStream,
    }

    /// A host on a fresh data folder whose link is already connected (to one end of a socket
    /// pair), so nothing here ever starts a tool: `ensure_tool` returns at once.
    fn rig(name: &str) -> Rig {
        let dir = std::env::temp_dir().join(format!("richos-dictation-host-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let link = Arc::new(Link::default());
        let (app_end, tool_end) = UnixStream::pair().unwrap();
        *link.writer.lock().unwrap() = Some(app_end);
        let voice: Arc<dyn VoiceMode> = Arc::new(FakeVoice { on: AtomicBool::new(false), gate: TurnGate::new() });
        let host = Host::new(link, dir.clone(), "1.2.0".into(), voice);
        Rig { host, dir, tool: tool_end }
    }

    impl Rig {
        /// Everything the tool heard, once the app's end is closed.
        fn heard(self) -> Vec<AppMessage> {
            *self.host.link.writer.lock().unwrap() = None;
            let mut heard = Vec::new();
            ipc::read_lines::<AppMessage>(self.tool, |m| heard.push(m));
            std::fs::remove_dir_all(&self.dir).ok();
            heard
        }
    }

    /// INVARIANT: the switch, the key and the accuracy are each written to `dictation.json`, and
    /// the tool is told to re-read it; a key outside F1..F19 and an unknown accuracy are refused
    /// and write nothing.
    #[test]
    fn each_setting_is_written_and_the_tool_told() {
        let r = rig("writes");
        assert_eq!(r.host.set_key(5).unwrap().key, 5);
        assert!(r.host.set_key(0).is_err());
        assert!(r.host.set_key(20).is_err());
        assert_eq!(r.host.set_accuracy("fast").unwrap().accuracy, "fast");
        assert!(r.host.set_accuracy("small.en").is_err());
        assert!(r.host.set_on(true).unwrap().on, "on, with the tool already connected: nothing started");
        let on = r.host.settings().unwrap();
        assert_eq!((on.on, on.key, on.model.as_str()), (true, 5, "small.en"));
        assert!(!r.host.set_on(false).unwrap().on);
        assert_eq!(r.heard(), vec![AppMessage::SettingsChanged, AppMessage::SettingsChanged, AppMessage::SettingsChanged]);
    }

    /// INVARIANT: a broken `dictation.json` is never overwritten by a change.
    #[test]
    fn a_broken_file_is_never_overwritten() {
        let r = rig("broken");
        std::fs::create_dir_all(&r.dir).unwrap();
        std::fs::write(store::path(&r.dir), b"{not json").unwrap();
        assert!(r.host.set_key(3).is_err());
        assert!(r.host.set_on(true).is_err());
        assert_eq!(std::fs::read(store::path(&r.dir)).unwrap(), b"{not json");
        assert!(r.heard().is_empty(), "the tool is told nothing about a change that was not made");
    }

    /// INVARIANT (section 2 row 8): Accessibility is asked once. Once `axAsked` is written, it is
    /// never asked again (the sheet offers Open System Settings instead).
    #[test]
    fn accessibility_is_asked_once() {
        let r = rig("ax");
        r.host.update(|s| s.ax_asked = true).unwrap();
        assert!(!r.host.ask_accessibility().unwrap(), "asked before: no second prompt");
        r.heard();
    }

    /// INVARIANT (finding 4): with another copy's tool holding the key, the key and the
    /// accuracy are changed through that tool (`change`), this copy's own file is untouched,
    /// and the view shows what the tool reports; Off goes to the tool too, and is written here.
    #[test]
    fn a_copy_that_does_not_own_the_key_changes_the_owners_settings() {
        let r = rig("elsewhere");
        r.host.update(|s| s.on = true).unwrap();
        let other = |key: u8, model: &str| ToolMessage::State { owner: "/other/RichOS.app".into(), on: true, listening: false, writing: false, problem: None, key_tap: true, secure_input: false, secure_app: None, key, model: model.into() };
        r.host.link.observe(other(1, "large-v3-turbo-q5_0"));
        let v = r.host.set_key(7).unwrap();
        assert_eq!((v.owner, v.key), ("other", 1), "the tool has not applied it yet: the view shows the tool's key");
        r.host.link.observe(other(7, "large-v3-turbo-q5_0"));
        assert_eq!(r.host.view().unwrap().key, 7, "once the tool reports it, the view shows it");
        r.host.set_accuracy("fast").unwrap();
        let own = r.host.settings().unwrap();
        assert_eq!((own.key, own.model.as_str()), (1, "large-v3-turbo-q5_0"), "this copy's own file is untouched");
        assert!(!r.host.set_on(false).unwrap().on);
        assert!(!r.host.settings().unwrap().on, "Off is written here too, so this copy starts no tool of its own");
        assert_eq!(
            r.heard(),
            vec![
                AppMessage::Change { on: None, key: Some(7), model: None },
                AppMessage::Change { on: None, key: None, model: Some("small.en".into()) },
                AppMessage::Change { on: Some(false), key: None, model: None },
            ]
        );
    }

    /// INVARIANT: "Press a different key" goes through the tool's tap when it has one, and is
    /// stopped the same way; with no tap the window's own keys answer.
    #[test]
    fn key_capture_goes_through_the_tap() {
        let r = rig("capture");
        let state = |tap: bool| ToolMessage::State { owner: "x".into(), on: true, listening: false, writing: false, problem: None, key_tap: tap, secure_input: false, secure_app: None, key: 1, model: String::new() };
        r.host.link.observe(state(false));
        assert!(!r.host.capture(true), "no tap: the window's keys answer");
        r.host.link.observe(state(true));
        assert!(r.host.capture(true), "the tap answers");
        assert!(!r.host.capture(false));
        assert_eq!(r.heard(), vec![AppMessage::CaptureNextKey, AppMessage::CaptureNextKey, AppMessage::CancelCapture]);
    }
}

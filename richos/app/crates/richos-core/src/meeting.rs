//! MEETING NOTES — the one prompt/capture state (Granola PRD V4, richos-hq
//! `docs/prds/2026-10-12-granola-replacement.md`, "Meeting prompts and automatic completion").
//!
//! **Match Granola: detect and prompt automatically, start capture only after the CEO clicks
//! "Take Notes", then detect the call's end and process it automatically.** This module is the
//! whole of that decision, and nothing else: it takes what the companion's `watch` reports
//! (another process started or stopped using the microphone), the CEO's clicks and the clock,
//! and says what the app should do: show or hide the prompt, start or stop the capture. It runs
//! no process and draws nothing, so every rule below is a unit test.
//!
//! # The call is the app's microphone use
//!
//! A call is identified by the app family holding the microphone (`us.zoom.xos`,
//! `com.google.Chrome`; the companion folds helper processes into their app). It is LIVE while
//! any process of that family runs input, and it is over once none does: its recording stops
//! after [`END_GRACE_MS`] of that, or at once when that app takes the microphone again, which is
//! the next call (a call's input never pauses inside the call, measured through mute and silence).
//! RichOS's own processes never count: the watcher is started excluding the app, and the capture
//! process the app starts is excluded here ([`MeetingMonitor::exclude_pid`]), because a
//! device-wide "microphone in use" flag cannot see a call end while RichOS itself holds the
//! microphone.
//!
//! # What does NOT end a call
//!
//! The scheduled end time, silence, muting, and a recording started before the call began.
//! A recording started from a calendar prompt before anyone joined is UNARMED: it has no call
//! yet, so nothing can end it until a call's microphone use starts (it then arms on that app)
//! and stops again. A manual start (an in-person conversation) never arms: only Stop ends it.
//!
//! # One prompt, one recording, per call
//!
//! A calendar prompt and the microphone signal for the same meeting are one call: whichever
//! comes first is the prompt, the other attaches to it (the calendar title replaces "Call
//! detected in …"), when the call is in the app the meeting's link opens; a call in any other app
//! is a different call and waits for its own prompt. Not this one, and Stop, hold for that call: it does not prompt again until
//! it has ended. The next call prompts normally. A calendar meeting he answered (Take Notes, Not
//! this one, Stop) asks once: every call that begins during the meeting's own scheduled time, in
//! the meeting link's app when the event names one ([`Scheduled::covers`]), is that meeting,
//! however often he leaves and rejoins; any other call prompts. A rejoin of a meeting he is taking
//! notes on (Take Notes, not stopped or declined since) records that meeting again without asking;
//! after Stop or Not this one it records nothing. A browser call is a meeting only
//! when its link is a browser meeting (Google Meet). While prompts are paused nothing prompts, and
//! a running recording is left alone.
//!
//! # Nothing that waits is forgotten
//!
//! Another app's call that starts while one call is followed (or a manual recording runs) is
//! kept, and prompts the moment that call ends; so does a calendar meeting that comes due while
//! another call is followed, if it is still on. A meeting that comes due during a call that began
//! close enough to its start ([`SAME_MEETING_MS`]) and is still on when it starts IS that call,
//! and never prompts afterwards; a call over before the meeting starts was not it, and the
//! meeting prompts. A call already known to be one calendar meeting (it joined that meeting's
//! prompt or recording, or is the meeting he declined) is never another: a different meeting due
//! while it runs over, declined or stopped, prompts when it ends.
//! Every sequence is one row of `every_call_sequence_gives_the_expected_prompt_and_recording`.

use crate::meeting_calendar::{CalendarMeeting, PROMPT_LEAD_MS};
use std::collections::{BTreeMap, BTreeSet};

/// How long every process of the call's app must have been off the microphone before the call's
/// recording stops, when that app has not taken the microphone again (if it has, that is the next
/// call, and the recording stops at once).
///
/// MEASURED (2026-10-13, real Zoom calls from a test VM guest against the Windows call lab;
/// richos-hq `docs/verification/2026-10-13-granola-end-detection/`): the call app's input held
/// through every mute (55 s native Zoom, 47 s Zoom in Chrome) and every minute of silence on both
/// routes, with no gap at all inside a call, and stopped 2.6 s after the guest left, 3.2 to 4.0 s
/// after the host ended the meeting and 1.8 s after the tab closed. An idle Zoom, an idle Chrome
/// and played sounds never started input; every start after a stop was a new join. 8 s ends a
/// recording within about 12 s of hang-up. NOT measured: a change of microphone inside a call; if
/// it stops the app's input, the call is split there and the rest is prompted as the next call.
pub const END_GRACE_MS: u64 = 8_000;

/// A calendar meeting that comes due while a call is followed is THAT call when the call began no
/// earlier than this before the meeting's start (joining a few minutes early); otherwise it is
/// the next meeting, and it prompts once the call ends. A chosen value, not a measured one: longer
/// than any early join, shorter than the shortest meeting of a back-to-back day.
pub const SAME_MEETING_MS: u64 = 10 * 60_000;

/// One line of `richos-companion watch` (JSON lines on its stdout).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchLine {
    Hello { pid: i32 },
    Started { pid: i32, family: String, app: Option<String>, at: u64 },
    Stopped { pid: i32, family: String, exited: bool, at: u64 },
    Error { message: String },
    /// `--trace` lines and anything this build does not know; ignored.
    Other,
}

/// Parse one watcher line. `None` for a line that is not JSON at all.
pub fn parse_watch_line(line: &str) -> Option<WatchLine> {
    let v: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
    let n = |k: &str| v.get(k).and_then(|x| x.as_i64());
    Some(match (v.get("type").and_then(|t| t.as_str()), v.get("state").and_then(|t| t.as_str())) {
        (Some("hello"), _) => WatchLine::Hello { pid: n("pid").unwrap_or(0) as i32 },
        (Some("input"), Some(state)) => {
            let pid = n("pid").unwrap_or(0) as i32;
            // A process with no bundle id is still a process: its pid is its family.
            let family = s("family").or_else(|| s("bundleId")).unwrap_or_else(|| format!("pid:{pid}"));
            let at = n("at").unwrap_or(0).max(0) as u64;
            if state == "started" {
                WatchLine::Started { pid, family, app: s("app"), at }
            } else {
                let exited = v.get("exited").and_then(|x| x.as_bool()).unwrap_or(false);
                WatchLine::Stopped { pid, family, exited, at }
            }
        }
        (Some("error"), _) => WatchLine::Error { message: s("message").unwrap_or_default() },
        _ => WatchLine::Other,
    })
}

/// The call app a calendar meeting's link opens, read from the event's location or invite text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeetingApp {
    Zoom,
    Teams,
    Meet,
    Webex,
}

impl MeetingApp {
    /// Each app's link hosts, as they appear in an invite.
    const HOSTS: [(&'static str, MeetingApp); 6] = [
        ("zoom.us/", MeetingApp::Zoom),
        ("zoomgov.com/", MeetingApp::Zoom),
        ("teams.microsoft.com/", MeetingApp::Teams),
        ("teams.live.com/", MeetingApp::Teams),
        ("meet.google.com/", MeetingApp::Meet),
        ("webex.com/", MeetingApp::Webex),
    ];

    /// Browsers a Google Meet link opens in (Meet has no desktop app). Which tab holds the
    /// microphone is not visible, so a browser call can be any browser meeting.
    const BROWSERS: [&'static str; 6] =
        ["com.google.Chrome", "com.apple.Safari", "com.microsoft.edgemac", "org.mozilla.firefox", "company.thebrowser.Browser", "com.brave.Browser"];

    /// The app of the first meeting link in this text, if there is one.
    pub fn in_text(text: &str) -> Option<MeetingApp> {
        let text = text.to_ascii_lowercase();
        Self::HOSTS.iter().filter_map(|(host, app)| text.find(host).map(|at| (at, *app))).min_by_key(|(at, _)| *at).map(|(_, app)| app)
    }

    /// A call in this app family can be a meeting with this link: the link's own desktop app, or,
    /// for a browser meeting (Google Meet), a browser. A browser call during a Zoom, Teams or Webex
    /// meeting is a different call and gets its own prompt (rv-20261014T105556Z-71dc7488-f9ab #2).
    fn takes(self, family: &str) -> bool {
        let native: &[&str] = match self {
            MeetingApp::Zoom => &["us.zoom.xos"],
            MeetingApp::Teams => &["com.microsoft.teams2", "com.microsoft.teams"],
            MeetingApp::Meet => &Self::BROWSERS,
            MeetingApp::Webex => &["com.cisco.webexmeetingsapp", "Cisco-Systems.Spark"],
        };
        native.contains(&family)
    }
}

/// A synced calendar meeting that is due to prompt (the calendar side decides "one minute
/// before a timed meeting with other attendees"; this module only places it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scheduled {
    pub event_id: String,
    pub title: String,
    pub starts_at: u64,
    /// Its scheduled end.
    pub ends_at: u64,
    /// The app its link opens, when the event names one.
    pub app: Option<MeetingApp>,
}

impl Scheduled {
    /// This call can be this answered meeting, joined or rejoined: it began during the meeting's
    /// own time (from its prompt, [`PROMPT_LEAD_MS`] before the start, to its end), in the link's
    /// app when the event names a link ([`MeetingApp::takes`]). Any other call is a different call
    /// and prompts (rv-20261014T102622Z-ea367a6c-5c3e: a fixed window around the start asked again
    /// when A was joined 11 minutes late, and took an unrelated Teams call 5 minutes in for A).
    fn covers(&self, call: &Call) -> bool {
        let during = call.since + PROMPT_LEAD_MS >= self.starts_at && call.since < self.ends_at;
        during && self.takes(&call.family)
    }

    /// A call in this app family can be this meeting: any app when the event names no link, else
    /// the link's app ([`MeetingApp::takes`]). A calendar prompt or recording for a Zoom meeting
    /// never takes a Teams call as its own (rv-20261014T110606Z-1e6b535c-f031 #1).
    fn takes(&self, family: &str) -> bool {
        self.app.is_none_or(|app| app.takes(family))
    }
}

/// What the prompt says. `Scheduled` carries the calendar title; `Unscheduled` the app's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptKind {
    Scheduled { event_id: String, title: String, starts_at: u64 },
    Unscheduled { app: String },
}

/// Why a recording ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndReason {
    /// The call's app stopped using the microphone and stayed off for [`END_GRACE_MS`].
    CallEnded,
    /// The CEO pressed Stop.
    StoppedEarly,
}

/// What the app should do now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    ShowPrompt(PromptKind),
    HidePrompt,
    /// Start the capture. `title` is the calendar title when the call matched one.
    StartCapture { title: Option<String>, event_id: Option<String>, manual: bool },
    StopCapture(EndReason),
}

/// The CEO's input and the capture's own news.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    TakeNotes,
    /// "Not this one" (and the voice bar's ×): neither records, and the call does not prompt again.
    NotThisOne,
    Stop,
    /// "Start meeting notes" in Settings (a meeting nothing detected, an in-person conversation).
    ManualStart,
    /// The capture process ended by itself (failed, or its permission was revoked). The
    /// recording is over; the app reports what was saved.
    CaptureEnded,
    /// "Try again" on the problem card after a capture ended by itself: the SAME recording again,
    /// still ending when its call does. With no failed recording to repeat, a manual start.
    Retry,
}

/// What a recording was started as, kept so "Try again" repeats it exactly.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Start {
    title: Option<String>,
    event_id: Option<String>,
    /// The calendar meeting it records (answered: [`MeetingMonitor::answered`]).
    scheduled: Option<Scheduled>,
    manual: bool,
}

impl Start {
    fn action(&self) -> Action {
        Action::StartCapture { title: self.title.clone(), event_id: self.event_id.clone(), manual: self.manual }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Call {
    family: String,
    app: Option<String>,
    /// Processes of this family running input right now.
    live: BTreeSet<i32>,
    /// When the last of them stopped, while none is live.
    quiet_since: Option<u64>,
    /// When its microphone use was first seen.
    since: u64,
    /// Calendar meetings, by event id, that came due while this call was followed and that it
    /// began close enough to be ([`Call::is_meeting`]). Only the call's end says whether it WAS
    /// one: see [`MeetingMonitor::end_call`].
    meetings: BTreeMap<String, Scheduled>,
    /// The calendar meeting this call is KNOWN to be: the calendar prompt it joined, the calendar
    /// recording it armed, the declined meeting whose call it is, or the title its prompt took.
    /// Kept when the call is declined or stopped, so a different meeting due while it runs over
    /// waits for its end instead of being taken for it (handover review
    /// rv-20261013T141401Z-390d6bea-2e23 #1).
    event: Option<String>,
}

impl Call {
    fn new(family: &str, app: Option<String>, now: u64) -> Call {
        Call { family: family.to_string(), app, live: BTreeSet::new(), quiet_since: None, since: now, meetings: BTreeMap::new(), event: None }
    }
    fn ended(&self, now: u64) -> bool {
        self.live.is_empty() && self.quiet_since.is_some_and(|q| now.saturating_sub(q) >= END_GRACE_MS)
    }
    /// This call is that calendar meeting: it is known to be it, or, known to be none, it began no
    /// earlier than [`SAME_MEETING_MS`] before it in an app the meeting's link opens. A call known
    /// to be another meeting never is.
    fn is_meeting(&self, meeting: &Scheduled) -> bool {
        match &self.event {
            Some(known) => *known == meeting.event_id,
            None => self.since + SAME_MEETING_MS >= meeting.starts_at && meeting.takes(&self.family),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum Phase {
    #[default]
    Idle,
    /// The prompt is up (as the card, or put aside as the voice bar: that difference is the
    /// window's, not this state's). `call` is None for a calendar prompt nobody has joined yet.
    Prompting { call: Option<Call>, scheduled: Option<Scheduled> },
    /// Recording. `call` None = unarmed (calendar start before the call, or manual).
    Recording { call: Option<Call>, start: Start },
    /// The recording ended by itself and the problem card is up. The call is still followed, so
    /// "Try again" records it as before and still stops when it ends.
    Failed { call: Option<Call>, start: Start },
    /// Declined or stopped early: this call does not prompt again until it ends. Always holds a
    /// call; with none to wait for, the state is Idle.
    Done { call: Call },
}

impl Phase {
    fn call_mut(&mut self) -> Option<&mut Call> {
        match self {
            Phase::Prompting { call, .. } | Phase::Recording { call, .. } | Phase::Failed { call, .. } => call.as_mut(),
            Phase::Done { call } => Some(call),
            Phase::Idle => None,
        }
    }

    fn call(&self) -> Option<&Call> {
        match self {
            Phase::Prompting { call, .. } | Phase::Recording { call, .. } | Phase::Failed { call, .. } => call.as_ref(),
            Phase::Done { call } => Some(call),
            Phase::Idle => None,
        }
    }

    /// The state can follow a call it does not have yet: nothing is up, a calendar prompt nobody
    /// joined, or a calendar recording started before its call.
    fn can_take_a_call(&self) -> bool {
        match self {
            Phase::Idle | Phase::Prompting { call: None, .. } => true,
            Phase::Recording { call: None, start } | Phase::Failed { call: None, start } => !start.manual,
            _ => false,
        }
    }

    /// The calendar meeting the prompt or recording up is for.
    fn meeting(&self) -> Option<Scheduled> {
        match self {
            Phase::Prompting { scheduled, .. } => scheduled.clone(),
            Phase::Recording { start, .. } | Phase::Failed { start, .. } => start.scheduled.clone(),
            _ => None,
        }
    }

    /// Declined or stopped: wait for this call to end, or Idle when there is none.
    fn done(call: &Option<Call>) -> Phase {
        match call {
            Some(c) => Phase::Done { call: c.clone() },
            None => Phase::Idle,
        }
    }
}

/// The one prompt/capture state. Feed it watcher lines ([`MeetingMonitor::watch`]), the CEO's
/// input ([`MeetingMonitor::input`]), calendar prompts ([`MeetingMonitor::scheduled`]) and a
/// clock tick at least once a second ([`MeetingMonitor::tick`]); act on what each returns.
#[derive(Debug, Clone, Default)]
pub struct MeetingMonitor {
    phase: Phase,
    excluded: BTreeSet<i32>,
    paused: bool,
    /// Microphone use by other apps while the state follows one call (or a manual recording):
    /// kept, so the next call prompts when this one ends (second review
    /// rv-20261013T130528Z-361c4090-dbf1 #3: a call started inside the end grace was lost).
    others: BTreeMap<String, Call>,
    /// Calendar events shown, attached to a call, or that WERE the call followed (decided when
    /// that call ends), so the clock's offer every second does not prompt twice. A meeting that had
    /// to wait is not in here, so it prompts when the call ends (rv-20261013T130528Z-361c4090-dbf1
    /// #1).
    prompted_events: BTreeSet<String>,
    /// Every calendar meeting he answered (Take Notes, Not this one, Stop), by event id, kept for
    /// its whole scheduled time: a call that starts during it in its link's app
    /// ([`Scheduled::covers`]) is that meeting, joined or rejoined, and does not prompt (second
    /// reviews rv-20261013T120455Z-516bb135-67f2 #1, rv-20261014T095959Z-f7ee40c9-86b6 #1,
    /// rv-20261014T105556Z-71dc7488-f9ab #1: matching a call used the answer up, and another
    /// prompt erased it). Any other call prompts. Dropped when the meeting is over, canceled or
    /// moved.
    answered: BTreeMap<String, Scheduled>,
    /// The answered meetings he is taking notes on: Take Notes, or a recording that ran until its
    /// call ended, and never Stop or Not this one since. A rejoin inside the meeting's time records
    /// into that meeting again without asking (meet7b's report: a rejoin after notes started
    /// recorded nothing). Dropped with the answer.
    taking_notes: BTreeSet<String>,
}

impl MeetingMonitor {
    pub fn new() -> Self {
        Self::default()
    }

    /// A process whose microphone use is RichOS's own (the capture process it starts).
    ///
    /// Its own start may already have been reported (the watcher's poll and the spawn race): a
    /// "call" that was only this process was never a call, and a recording it armed is unarmed.
    pub fn exclude_pid(&mut self, pid: i32) {
        self.excluded.insert(pid);
        self.others.retain(|_, c| {
            c.live.remove(&pid);
            !c.live.is_empty() || c.quiet_since.is_some()
        });
        let only_ours = self.phase.call_mut().is_some_and(|c| c.live.remove(&pid) && c.live.is_empty() && c.quiet_since.is_none());
        if only_ours {
            if let Phase::Recording { call, .. } | Phase::Failed { call, .. } | Phase::Prompting { call, scheduled: Some(_) } = &mut self.phase {
                *call = None;
            }
        }
    }

    pub fn is_recording(&self) -> bool {
        matches!(self.phase, Phase::Recording { .. })
    }
    pub fn is_prompting(&self) -> bool {
        matches!(self.phase, Phase::Prompting { .. })
    }
    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Pause or resume meeting prompts. Pausing hides a prompt that is up and never touches a
    /// running recording.
    pub fn set_paused(&mut self, paused: bool) -> Vec<Action> {
        self.paused = paused;
        if paused {
            if let Phase::Prompting { call, .. } = &self.phase {
                self.phase = Phase::done(call);
                return vec![Action::HidePrompt];
            }
        }
        vec![]
    }

    /// One watcher line, at `now`.
    pub fn watch(&mut self, line: &WatchLine, now: u64) -> Vec<Action> {
        match line {
            WatchLine::Started { pid, family, app, .. } if !self.excluded.contains(pid) => self.mic_started(*pid, family, app.clone(), now),
            WatchLine::Stopped { pid, family, .. } if !self.excluded.contains(pid) => self.mic_stopped(*pid, family, now),
            _ => vec![],
        }
    }

    fn mic_started(&mut self, pid: i32, family: &str, app: Option<String>, now: u64) -> Vec<Action> {
        // Another process of the followed call's app joining while the call holds the microphone:
        // the same call.
        if let Some(c) = self.phase.call_mut().filter(|c| c.family == family && !c.live.is_empty()) {
            c.live.insert(pid);
            return vec![];
        }
        // The followed call's app back after every process of it left the microphone: that call
        // is over and this is the next one, with its own prompt and recording (handover review
        // rv-20261013T135051Z-1b6ba88a-4002 #1: leaving one Zoom call and joining the next inside
        // the end grace recorded the second without its own click). Measured, a call's input
        // never pauses inside a call, through mute or silence (END_GRACE_MS).
        let mut out = vec![];
        if self.phase.call().is_some_and(|c| c.family == family) {
            out = self.end_call(now);
        }
        // Any other app is kept; `settle` gives it to the state when the state can take a call.
        // An app waiting there that left the microphone and is back is a new call too.
        let c = self.others.entry(family.to_string()).or_insert_with(|| Call::new(family, app.clone(), now));
        if c.live.is_empty() {
            *c = Call::new(family, app, now);
        }
        c.live.insert(pid);
        out.extend(self.settle());
        out
    }

    fn mic_stopped(&mut self, pid: i32, family: &str, now: u64) -> Vec<Action> {
        let call = match self.phase.call_mut().filter(|c| c.family == family) {
            Some(c) => Some(c),
            None => self.others.get_mut(family),
        };
        if let Some(c) = call {
            if c.live.remove(&pid) && c.live.is_empty() {
                c.quiet_since = Some(now);
            }
        }
        vec![]
    }

    /// When the state can follow a call, the earliest other app still on the microphone becomes
    /// it: a prompt, the call of a declined calendar meeting, or the call a calendar recording
    /// arms on. One prompt and one recording at a time; whatever waits, waits here.
    fn settle(&mut self) -> Vec<Action> {
        if !self.phase.can_take_a_call() {
            return vec![];
        }
        // A calendar prompt or pre-call recording takes only a call in its meeting's app, and,
        // when its event names no link, not a call a different answered meeting's link names: that
        // call is the answered meeting rejoined. Any other call waits, and is settled once that
        // meeting's prompt or recording is over (rv-20261014T110606Z-1e6b535c-f031 #1,
        // rv-20261014T114658Z-04417671-85c4 #1).
        let pending = self.phase.meeting();
        let answered = &self.answered;
        let fits = |c: &Call| match &pending {
            None => true,
            Some(m) => {
                let other = || answered.values().any(|a| a.event_id != m.event_id && a.app.is_some() && a.covers(c));
                m.takes(&c.family) && (m.app.is_some() || !other())
            }
        };
        let next = self
            .others
            .iter()
            .filter(|(_, c)| !c.live.is_empty() && fits(c))
            .min_by_key(|(_, c)| c.since)
            .map(|(f, _)| f.clone());
        let Some(mut call) = next.and_then(|family| self.others.remove(&family)) else {
            return vec![];
        };
        // A calendar meeting he already answered whose time and app this call fits (the latest
        // started, when meetings overlap). The answer stays for every later rejoin.
        let answered = self.answered.values().filter(|meeting| meeting.covers(&call)).max_by_key(|meeting| meeting.starts_at).map(|meeting| meeting.event_id.clone());
        match &mut self.phase {
            Phase::Idle if answered.is_some() => {
                // That meeting, joined or rejoined, known to be that meeting. He is taking notes on
                // it: they go on, into the same meeting, without asking (not while prompts are
                // paused). Otherwise it is held as answered until this call ends, like any
                // declined call.
                let meeting = answered.as_ref().filter(|id| self.taking_notes.contains(*id) && !self.paused).and_then(|id| self.answered.get(id)).cloned();
                call.event = answered;
                match meeting {
                    Some(meeting) => {
                        let start = Start { title: Some(meeting.title.clone()), event_id: Some(meeting.event_id.clone()), scheduled: Some(meeting), manual: false };
                        let action = start.action();
                        self.phase = Phase::Recording { call: Some(call), start };
                        vec![action]
                    }
                    None => {
                        self.phase = Phase::Done { call };
                        vec![]
                    }
                }
            }
            Phase::Idle if self.paused => {
                // Nothing prompts while paused; the call is remembered so it does not prompt the
                // moment prompts resume halfway through it.
                self.phase = Phase::Done { call };
                vec![]
            }
            Phase::Idle => {
                let kind = PromptKind::Unscheduled { app: call.app.clone().unwrap_or_else(|| call.family.clone()) };
                self.phase = Phase::Prompting { call: Some(call), scheduled: None };
                vec![Action::ShowPrompt(kind)]
            }
            // A calendar prompt nobody had joined: this is its call (no second prompt). A calendar
            // recording, or a failed one waiting on its card, arms on it, so it ends with the call
            // (second review rv-20261013T120455Z-516bb135-67f2 #2). Either way the call is known to
            // be that calendar meeting.
            Phase::Prompting { call: slot, scheduled } => {
                call.event = scheduled.as_ref().map(|s| s.event_id.clone());
                *slot = Some(call);
                vec![]
            }
            Phase::Recording { call: slot, start } | Phase::Failed { call: slot, start } => {
                call.event = start.event_id.clone();
                *slot = Some(call);
                vec![]
            }
            Phase::Done { .. } => {
                self.others.insert(call.family.clone(), call);
                vec![]
            }
        }
    }

    /// The followed call is over: its grace ran out ([`MeetingMonitor::tick`]), or its app came
    /// back as the next call ([`MeetingMonitor::watch`]).
    fn end_call(&mut self, now: u64) -> Vec<Action> {
        if let Some(c) = self.phase.call() {
            // A meeting this call began close enough to be WAS this call if the call was still on
            // when the meeting started, and never prompts. A call over before the meeting started
            // was not it: the meeting stays eligible and prompts on the clock's next offer
            // (handover review rv-20261013T135051Z-1b6ba88a-4002 #2). A call he answered (anything
            // but a prompt left up) answered that meeting: a rejoin does not ask again.
            let ended_at = c.quiet_since.unwrap_or(now);
            let answered = !matches!(self.phase, Phase::Prompting { .. });
            // A recording that ran until its call ended: he is taking notes on that meeting.
            let recorded = matches!(&self.phase, Phase::Recording { start, .. } if !start.manual);
            for meeting in c.meetings.values().filter(|meeting| ended_at >= meeting.starts_at) {
                self.prompted_events.insert(meeting.event_id.clone());
                if answered {
                    self.answered.insert(meeting.event_id.clone(), meeting.clone());
                }
                if recorded {
                    self.taking_notes.insert(meeting.event_id.clone());
                }
            }
        }
        let (said, idle) = match &mut self.phase {
            // The call went before anybody answered: the prompt goes with it.
            Phase::Prompting { .. } => (Some(Action::HidePrompt), true),
            // A manual recording only followed a declined call; it ends with Stop alone.
            Phase::Recording { call, start } if start.manual => {
                *call = None;
                (None, false)
            }
            Phase::Recording { .. } => (Some(Action::StopCapture(EndReason::CallEnded)), true),
            // A failed recording's call ended: nothing is left to retry against. The problem
            // card stays until answered; "Try again" is then a manual start.
            _ => (None, true),
        };
        if idle {
            self.phase = Phase::Idle;
        }
        said.into_iter().collect()
    }

    /// The clock. Ends a call whose app has stayed off the microphone for [`END_GRACE_MS`].
    pub fn tick(&mut self, now: u64) -> Vec<Action> {
        let mut out = vec![];
        if self.phase.call().is_some_and(|c| c.ended(now)) {
            out.extend(self.end_call(now));
        }
        // Other apps whose calls ended while they waited are forgotten; one still on is next.
        self.others.retain(|_, c| !c.ended(now));
        out.extend(self.settle());
        out
    }

    /// The app's clock, once a second: [`MeetingMonitor::tick`], then every synced meeting due now
    /// ([`CalendarMeeting::due`]) offered to [`MeetingMonitor::scheduled`], then a calendar prompt
    /// nobody joined whose meeting is over retired, and answered meetings that are over forgotten.
    /// One function, so the sequences the tests walk are the ones the app runs.
    pub fn clock(&mut self, now: u64, calendar: &[CalendarMeeting]) -> Vec<Action> {
        let mut actions = self.tick(now);
        for meeting in calendar.iter().filter(|m| m.due(now)) {
            actions.extend(self.scheduled(meeting.scheduled()));
        }
        let over = self
            .unjoined_calendar_prompt()
            .map(str::to_string)
            .filter(|id| calendar.iter().find(|m| &m.event_id == id).is_none_or(|m| !m.due(now)));
        if let Some(id) = over {
            actions.extend(self.retire(&id));
        }
        // An answer stays; the meeting's time and link are the calendar's latest, so an end the
        // calendar moved later still covers a rejoin, and one moved earlier stops covering calls
        // (rv-20261014T114658Z-04417671-85c4 #2).
        for meeting in calendar {
            if let Some(kept) = self.answered.get_mut(&meeting.event_id) {
                *kept = meeting.scheduled();
            }
        }
        self.answered.retain(|_, meeting| now < meeting.ends_at);
        let answered = &self.answered;
        self.taking_notes.retain(|id| answered.contains_key(id));
        actions
    }

    /// A calendar meeting is due to prompt. The app's clock offers every due meeting every second
    /// ([`MeetingMonitor::clock`]): one that cannot prompt now is simply not taken, and prompts on
    /// a later offer, once the call in the way has ended, if the meeting is still on.
    pub fn scheduled(&mut self, meeting: Scheduled) -> Vec<Action> {
        if self.prompted_events.contains(&meeting.event_id) {
            return vec![];
        }
        enum Offer {
            /// Prompt for it (replacing a calendar prompt nobody joined, if one is up).
            Show,
            /// The unscheduled prompt up is this meeting: it gets the title.
            Title,
            /// Nothing to show: paused, or a manual recording is most likely it.
            Taken,
            /// The followed call began close enough to be it: nothing to show while that call is
            /// on, and the call's end settles whether it was ([`MeetingMonitor::end_call`]).
            Held,
            /// Another call is followed: offered again once that call ends.
            Wait,
        }
        let offer = match &self.phase {
            Phase::Idle if self.paused => Offer::Taken,
            Phase::Idle => Offer::Show,
            // A calendar prompt nobody joined, and a later meeting is due: the later one replaces
            // it (second review rv-20261013T110409Z-a46fc74d-c9b9 #1).
            Phase::Prompting { call: None, scheduled: Some(old) } if old.starts_at < meeting.starts_at => Offer::Show,
            Phase::Prompting { call: Some(c), scheduled: None } if c.is_meeting(&meeting) => Offer::Title,
            // Following another calendar meeting: this one is a different meeting.
            Phase::Prompting { scheduled: Some(_), .. } => Offer::Wait,
            Phase::Recording { start, .. } | Phase::Failed { start, .. } if start.event_id.is_some() => Offer::Wait,
            Phase::Prompting { call: Some(c), .. } | Phase::Recording { call: Some(c), .. } | Phase::Failed { call: Some(c), .. } | Phase::Done { call: c } => {
                if c.is_meeting(&meeting) {
                    Offer::Held
                } else {
                    Offer::Wait
                }
            }
            // A manual recording with no call to compare: an in-person meeting is most likely
            // this one, so it is taken, as before.
            _ => Offer::Taken,
        };
        let kind = PromptKind::Scheduled { event_id: meeting.event_id.clone(), title: meeting.title.clone(), starts_at: meeting.starts_at };
        match offer {
            Offer::Wait => vec![],
            Offer::Taken => {
                self.prompted_events.insert(meeting.event_id);
                vec![]
            }
            Offer::Held => {
                if let Some(c) = self.phase.call_mut() {
                    c.meetings.insert(meeting.event_id.clone(), meeting);
                }
                vec![]
            }
            Offer::Title => {
                self.prompted_events.insert(meeting.event_id.clone());
                if let Phase::Prompting { call: Some(c), scheduled } = &mut self.phase {
                    c.event = Some(meeting.event_id.clone());
                    *scheduled = Some(meeting);
                }
                vec![Action::ShowPrompt(kind)]
            }
            Offer::Show => {
                self.prompted_events.insert(meeting.event_id.clone());
                self.phase = Phase::Prompting { call: None, scheduled: Some(meeting) };
                vec![Action::ShowPrompt(kind)]
            }
        }
    }

    /// The calendar meeting a prompt nobody has joined is for, if that is what is up.
    pub fn unjoined_calendar_prompt(&self) -> Option<&str> {
        match &self.phase {
            Phase::Prompting { call: None, scheduled: Some(s) } => Some(&s.event_id),
            _ => None,
        }
    }

    /// The meeting a calendar prompt was for has ended and nobody joined: the prompt goes, and the
    /// event stays prompted (it never comes back); an answered one is forgotten, so the next call
    /// prompts. A prompt with a live call is untouched.
    pub fn retire(&mut self, event_id: &str) -> Vec<Action> {
        self.answered.remove(event_id);
        self.taking_notes.remove(event_id);
        if self.unjoined_calendar_prompt() == Some(event_id) {
            self.phase = Phase::Idle;
            let mut out = vec![Action::HidePrompt];
            out.extend(self.settle());
            return out;
        }
        vec![]
    }

    /// The calendar no longer has this event (canceled), or moved it: a prompt for it that
    /// nobody has joined is withdrawn, and a moved meeting may prompt again at its new time.
    pub fn withdraw(&mut self, event_id: &str) -> Vec<Action> {
        self.prompted_events.remove(event_id);
        self.answered.remove(event_id);
        self.taking_notes.remove(event_id);
        if let Phase::Prompting { call: None, scheduled: Some(s) } = &self.phase {
            if s.event_id == event_id {
                self.phase = Phase::Idle;
                let mut out = vec![Action::HidePrompt];
                out.extend(self.settle());
                return out;
            }
        }
        vec![]
    }

    pub fn input(&mut self, input: Input) -> Vec<Action> {
        let mut out = self.answer(input);
        // Declining or stopping may leave the state free: a call that waited prompts now.
        out.extend(self.settle());
        out
    }

    fn answer(&mut self, input: Input) -> Vec<Action> {
        match (input, &self.phase) {
            (Input::TakeNotes | Input::ManualStart, Phase::Prompting { call, scheduled }) => {
                // Starting from Settings while a prompt (or its bar) is up starts THAT recording
                // (round 25.1: "removes the bar and starts the recording"). A calendar meeting is
                // answered: no call in its time and app asks again.
                if let Some(meeting) = scheduled {
                    self.answered.insert(meeting.event_id.clone(), meeting.clone());
                    self.taking_notes.insert(meeting.event_id.clone());
                }
                let start = Start {
                    title: scheduled.as_ref().map(|s| s.title.clone()),
                    event_id: scheduled.as_ref().map(|s| s.event_id.clone()),
                    scheduled: scheduled.clone(),
                    manual: false,
                };
                let action = start.action();
                self.phase = Phase::Recording { call: call.clone(), start };
                vec![Action::HidePrompt, action]
            }
            (Input::NotThisOne, Phase::Prompting { call, .. } | Phase::Failed { call, .. }) => {
                // Declined (the calendar prompt, or a failed calendar recording's card): the
                // meeting is answered, so neither its call nor a rejoin prompts.
                if let Some(meeting) = self.phase.meeting() {
                    self.taking_notes.remove(&meeting.event_id);
                    self.answered.insert(meeting.event_id.clone(), meeting);
                }
                self.phase = Phase::done(call);
                vec![Action::HidePrompt]
            }
            (Input::Retry | Input::ManualStart, Phase::Failed { call, start }) => {
                // The same recording again: a detected call stays armed and still ends it. Settings'
                // Start while a failed capture's card (or its bar) is up is that same retry
                // (second review rv-20261013T110409Z-a46fc74d-c9b9 #3: it did nothing).
                let action = start.action();
                self.phase = Phase::Recording { call: call.clone(), start: start.clone() };
                vec![Action::HidePrompt, action]
            }
            (Input::ManualStart | Input::Retry, Phase::Idle | Phase::Done { .. }) => {
                // A manual start never arms on a call: an in-person conversation has no app. A
                // declined call is still followed (it never ends this recording, `tick`), so it
                // does not prompt again after this recording either.
                let declined = match &self.phase {
                    Phase::Done { call } => Some(call.clone()),
                    _ => None,
                };
                let start = Start { manual: true, ..Start::default() };
                let action = start.action();
                self.phase = Phase::Recording { call: declined, start };
                vec![action]
            }
            (Input::Stop, Phase::Recording { call, start }) => {
                // Stopped early: this call does not prompt again until it ends. A calendar
                // recording's meeting was answered by its Take Notes and stays answered, and a
                // rejoin does not record it again.
                for id in start.event_id.iter().chain(call.as_ref().and_then(|c| c.event.as_ref())) {
                    self.taking_notes.remove(id);
                }
                self.phase = Phase::done(call);
                vec![Action::StopCapture(EndReason::StoppedEarly)]
            }
            (Input::CaptureEnded, Phase::Recording { call, start }) => {
                self.phase = Phase::Failed { call: call.clone(), start: start.clone() };
                vec![]
            }
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn started(pid: i32, family: &str, app: &str) -> WatchLine {
        WatchLine::Started { pid, family: family.into(), app: Some(app.into()), at: 0 }
    }
    fn stopped(pid: i32, family: &str) -> WatchLine {
        WatchLine::Stopped { pid, family: family.into(), exited: false, at: 0 }
    }
    const ZOOM: &str = "us.zoom.xos";
    fn zoom_prompt() -> Vec<Action> {
        vec![Action::ShowPrompt(PromptKind::Unscheduled { app: "zoom.us".into() })]
    }

    #[test]
    fn parses_the_watcher_lines() {
        assert_eq!(parse_watch_line(r#"{"type":"hello","version":"0.1.0-p2","at":1,"pid":77}"#), Some(WatchLine::Hello { pid: 77 }));
        assert_eq!(
            parse_watch_line(r#"{"type":"input","state":"started","at":1000,"pid":45,"bundleId":"com.google.Chrome.helper","family":"com.google.Chrome","app":"Google Chrome"}"#),
            Some(WatchLine::Started { pid: 45, family: "com.google.Chrome".into(), app: Some("Google Chrome".into()), at: 1000 })
        );
        assert_eq!(
            parse_watch_line(r#"{"type":"input","state":"stopped","exited":true,"at":2000,"pid":10,"bundleId":null,"family":null,"app":null}"#),
            Some(WatchLine::Stopped { pid: 10, family: "pid:10".into(), exited: true, at: 2000 })
        );
        assert_eq!(parse_watch_line(r#"{"type":"trace","at":1,"processes":[]}"#), Some(WatchLine::Other));
        assert_eq!(parse_watch_line("not json"), None);
    }

    #[test]
    fn an_unscheduled_call_prompts_once_and_one_click_records_until_it_ends() {
        let mut m = MeetingMonitor::new();
        assert_eq!(m.watch(&started(10, ZOOM, "zoom.us"), 0), zoom_prompt());
        // A second Zoom process joining the input is the same call: no second prompt.
        assert_eq!(m.watch(&started(11, ZOOM, "zoom.us"), 500), vec![]);
        assert_eq!(m.input(Input::TakeNotes), vec![Action::HidePrompt, Action::StartCapture { title: None, event_id: None, manual: false }]);
        m.watch(&stopped(11, ZOOM), 60_000);
        m.watch(&stopped(10, ZOOM), 61_000);
        assert_eq!(m.tick(61_000 + END_GRACE_MS - 1), vec![], "inside the grace interval the call is not over");
        assert_eq!(m.tick(61_000 + END_GRACE_MS), vec![Action::StopCapture(EndReason::CallEnded)]);
        assert!(!m.is_recording());
    }

    #[test]
    fn ignoring_the_prompt_records_nothing_and_the_prompt_goes_with_the_call() {
        let mut m = MeetingMonitor::new();
        m.watch(&started(10, ZOOM, "zoom.us"), 0);
        m.watch(&stopped(10, ZOOM), 30_000);
        assert_eq!(m.tick(30_000 + END_GRACE_MS), vec![Action::HidePrompt]);
        assert!(!m.is_recording());
    }

    #[test]
    fn not_this_one_holds_for_the_call_and_the_next_call_prompts() {
        let mut m = MeetingMonitor::new();
        m.watch(&started(10, ZOOM, "zoom.us"), 0);
        assert_eq!(m.input(Input::NotThisOne), vec![Action::HidePrompt]);
        // Another process of the same app joining the call does not prompt again.
        assert_eq!(m.watch(&started(11, ZOOM, "zoom.us"), 3_000), vec![]);
        m.watch(&stopped(11, ZOOM), 9_000);
        m.watch(&stopped(10, ZOOM), 10_000);
        assert_eq!(m.tick(10_000 + END_GRACE_MS), vec![]);
        // The next meeting prompts normally.
        assert_eq!(m.watch(&started(20, ZOOM, "zoom.us"), 60_000), zoom_prompt());
    }

    #[test]
    fn stop_early_does_not_reprompt_the_same_call_and_the_next_prompts() {
        let mut m = MeetingMonitor::new();
        m.watch(&started(10, ZOOM, "zoom.us"), 0);
        m.input(Input::TakeNotes);
        assert_eq!(m.input(Input::Stop), vec![Action::StopCapture(EndReason::StoppedEarly)]);
        assert_eq!(m.watch(&started(11, ZOOM, "zoom.us"), 6_000), vec![]);
        m.watch(&stopped(10, ZOOM), 9_000);
        m.watch(&stopped(11, ZOOM), 9_000);
        assert_eq!(m.tick(9_000 + END_GRACE_MS), vec![]);
        assert_eq!(m.watch(&started(30, ZOOM, "zoom.us"), 99_000), zoom_prompt());
    }

    #[test]
    fn a_calendar_prompt_and_its_call_are_one_prompt_and_one_recording() {
        let mut m = MeetingMonitor::new();
        let s = Scheduled { event_id: "e1".into(), title: "Acme pricing review".into(), starts_at: 60_000, ends_at: 60_000 + 3_600_000, app: None };
        let shown = m.scheduled(s.clone());
        assert!(matches!(&shown[..], [Action::ShowPrompt(PromptKind::Scheduled { title, .. })] if title == "Acme pricing review"));
        // A refresh delivering the same event again does not prompt again.
        assert_eq!(m.scheduled(s), vec![]);
        // Joining the call: the same prompt, no second one.
        assert_eq!(m.watch(&started(10, ZOOM, "zoom.us"), 59_000), vec![]);
        assert_eq!(
            m.input(Input::TakeNotes),
            vec![Action::HidePrompt, Action::StartCapture { title: Some("Acme pricing review".into()), event_id: Some("e1".into()), manual: false }]
        );
    }

    #[test]
    fn a_calendar_prompt_during_an_unscheduled_prompt_gives_it_the_title() {
        let mut m = MeetingMonitor::new();
        m.watch(&started(10, ZOOM, "zoom.us"), 0);
        let out = m.scheduled(Scheduled { event_id: "e1".into(), title: "Board".into(), starts_at: 60_000, ends_at: 60_000 + 3_600_000, app: None });
        assert!(matches!(&out[..], [Action::ShowPrompt(PromptKind::Scheduled { title, .. })] if title == "Board"));
        assert!(matches!(&m.input(Input::TakeNotes)[1], Action::StartCapture { title: Some(t), .. } if t == "Board"));
    }

    #[test]
    fn starting_before_the_scheduled_call_does_not_finalize_before_it_begins() {
        let mut m = MeetingMonitor::new();
        m.scheduled(Scheduled { event_id: "e1".into(), title: "Board".into(), starts_at: 60_000, ends_at: 60_000 + 3_600_000, app: None });
        m.input(Input::TakeNotes);
        // Nobody has joined: no tick, however late, ends an unarmed recording.
        assert_eq!(m.tick(10 * 60_000), vec![]);
        assert!(m.is_recording());
        // The call starts (arms), then ends.
        m.watch(&started(10, ZOOM, "zoom.us"), 11 * 60_000);
        m.watch(&stopped(10, ZOOM), 40 * 60_000);
        assert_eq!(m.tick(40 * 60_000 + END_GRACE_MS), vec![Action::StopCapture(EndReason::CallEnded)]);
    }

    #[test]
    fn a_manual_start_ends_only_with_stop() {
        let mut m = MeetingMonitor::new();
        assert_eq!(m.input(Input::ManualStart), vec![Action::StartCapture { title: None, event_id: None, manual: true }]);
        m.watch(&started(10, ZOOM, "zoom.us"), 1_000);
        m.watch(&stopped(10, ZOOM), 2_000);
        assert_eq!(m.tick(2_000 + 10 * END_GRACE_MS), vec![]);
        assert_eq!(m.input(Input::Stop), vec![Action::StopCapture(EndReason::StoppedEarly)]);
    }

    #[test]
    fn manual_start_while_a_prompt_is_up_records_that_call() {
        let mut m = MeetingMonitor::new();
        m.watch(&started(10, ZOOM, "zoom.us"), 0);
        m.input(Input::NotThisOne);
        // Declined, then started from Settings: a manual recording; the call still never re-prompts.
        assert_eq!(m.input(Input::ManualStart), vec![Action::StartCapture { title: None, event_id: None, manual: true }]);
        let mut p = MeetingMonitor::new();
        p.watch(&started(10, ZOOM, "zoom.us"), 0);
        assert_eq!(p.input(Input::ManualStart), vec![Action::HidePrompt, Action::StartCapture { title: None, event_id: None, manual: false }]);
        p.watch(&stopped(10, ZOOM), 1_000);
        assert_eq!(p.tick(1_000 + END_GRACE_MS), vec![Action::StopCapture(EndReason::CallEnded)]);
    }

    #[test]
    fn try_again_after_a_failed_capture_records_the_same_call_and_hang_up_still_ends_it() {
        // Second review rv-20261013T100322Z-8bfcf4fe-37ac #1: Try again used to become an unarmed
        // manual recording that hang-up could not stop.
        let mut m = MeetingMonitor::new();
        m.watch(&started(10, ZOOM, "zoom.us"), 0);
        m.input(Input::TakeNotes);
        assert_eq!(m.input(Input::CaptureEnded), vec![]);
        assert!(!m.is_recording());
        assert_eq!(m.input(Input::Retry), vec![Action::HidePrompt, Action::StartCapture { title: None, event_id: None, manual: false }]);
        assert!(m.is_recording());
        m.watch(&stopped(10, ZOOM), 60_000);
        assert_eq!(m.tick(60_000 + END_GRACE_MS), vec![Action::StopCapture(EndReason::CallEnded)]);
        // A calendar recording keeps its title and event on the retry.
        let mut c = MeetingMonitor::new();
        c.scheduled(Scheduled { event_id: "e1".into(), title: "Acme pricing review".into(), starts_at: 0, ends_at: 3_600_000, app: None });
        c.input(Input::TakeNotes);
        c.input(Input::CaptureEnded);
        assert_eq!(
            c.input(Input::Retry),
            vec![Action::HidePrompt, Action::StartCapture { title: Some("Acme pricing review".into()), event_id: Some("e1".into()), manual: false }]
        );
    }

    #[test]
    fn nothing_without_a_call_ever_blocks_the_next_prompt() {
        // Second review #1, its second half: a stopped recording with no call left a state that
        // never cleared, and no later call prompted.
        let mut m = MeetingMonitor::new();
        m.input(Input::ManualStart);
        m.input(Input::CaptureEnded);
        assert_eq!(m.input(Input::Retry), vec![Action::HidePrompt, Action::StartCapture { title: None, event_id: None, manual: true }]);
        assert_eq!(m.input(Input::Stop), vec![Action::StopCapture(EndReason::StoppedEarly)]);
        assert_eq!(m.watch(&started(10, ZOOM, "zoom.us"), 1_000), zoom_prompt());
        // A failed capture declined from its card, with no call: the next call prompts.
        let mut f = MeetingMonitor::new();
        f.input(Input::ManualStart);
        f.input(Input::CaptureEnded);
        assert_eq!(f.input(Input::NotThisOne), vec![Action::HidePrompt]);
        assert_eq!(f.watch(&started(10, ZOOM, "zoom.us"), 1_000), zoom_prompt());
        // A calendar prompt that came while paused holds nothing after resuming.
        let mut p = MeetingMonitor::new();
        p.set_paused(true);
        assert_eq!(p.scheduled(Scheduled { event_id: "e1".into(), title: "Standup".into(), starts_at: 0, ends_at: 3_600_000, app: None }), vec![]);
        p.set_paused(false);
        assert_eq!(p.watch(&started(10, ZOOM, "zoom.us"), 1_000), zoom_prompt());
        // Declining a failed capture's call: it does not prompt again until it has ended.
        let mut d = MeetingMonitor::new();
        d.watch(&started(10, ZOOM, "zoom.us"), 0);
        d.input(Input::TakeNotes);
        d.input(Input::CaptureEnded);
        assert_eq!(d.input(Input::NotThisOne), vec![Action::HidePrompt]);
        assert_eq!(d.watch(&started(11, ZOOM, "zoom.us"), 1_000), vec![]);
        d.watch(&stopped(10, ZOOM), 2_000);
        d.watch(&stopped(11, ZOOM), 2_000);
        d.tick(2_000 + END_GRACE_MS);
        assert_eq!(d.watch(&started(12, ZOOM, "zoom.us"), 20_000), zoom_prompt());
    }

    #[test]
    fn an_unjoined_calendar_prompt_gives_way_to_the_next_meeting_and_retires_when_over() {
        // Second review rv-20261013T110409Z-a46fc74d-c9b9 #1: an invitation nobody joined stayed
        // up for ever, the next meeting never prompted, and its call took the old title.
        let missed = Scheduled { event_id: "old".into(), title: "Missed meeting".into(), starts_at: 1_000, ends_at: 1_000 + 3_600_000, app: None };
        let next = Scheduled { event_id: "new".into(), title: "Pricing review".into(), starts_at: 3_600_000, ends_at: 3_600_000 + 3_600_000, app: None };
        let mut m = MeetingMonitor::new();
        m.scheduled(missed.clone());
        assert_eq!(m.unjoined_calendar_prompt(), Some("old"));
        assert_eq!(
            m.scheduled(next.clone()),
            vec![Action::ShowPrompt(PromptKind::Scheduled { event_id: "new".into(), title: "Pricing review".into(), starts_at: 3_600_000 })]
        );
        // The old one, still listed as due by a calendar that keeps history, never comes back.
        assert_eq!(m.scheduled(missed), vec![]);
        m.watch(&started(10, ZOOM, "zoom.us"), 3_600_000);
        assert_eq!(
            m.input(Input::TakeNotes),
            vec![Action::HidePrompt, Action::StartCapture { title: Some("Pricing review".into()), event_id: Some("new".into()), manual: false }]
        );
        // Retiring: only an unjoined calendar prompt, and only its own event.
        let mut r = MeetingMonitor::new();
        r.scheduled(next.clone());
        assert_eq!(r.retire("other"), vec![]);
        assert_eq!(r.retire("new"), vec![Action::HidePrompt]);
        assert_eq!(r.scheduled(next), vec![], "a retired meeting does not prompt again");
        // A prompt with a live call is never replaced or retired.
        let mut c = MeetingMonitor::new();
        c.scheduled(Scheduled { event_id: "a".into(), title: "A".into(), starts_at: 1_000, ends_at: 1_000 + 3_600_000, app: None });
        c.watch(&started(10, ZOOM, "zoom.us"), 2_000);
        assert_eq!(c.scheduled(Scheduled { event_id: "b".into(), title: "B".into(), starts_at: 9_000, ends_at: 9_000 + 3_600_000, app: None }), vec![]);
        assert_eq!(c.retire("a"), vec![]);
        assert!(c.is_prompting());
    }

    #[test]
    fn a_calendar_meeting_declined_before_joining_does_not_prompt_when_its_call_starts() {
        // Second review rv-20261013T120455Z-516bb135-67f2 #1: closing the pre-call prompt, then
        // joining, prompted again for the same meeting.
        let meeting = Scheduled { event_id: "e1".into(), title: "Standup".into(), starts_at: 60_000, ends_at: 60_000 + 3_600_000, app: None };
        let mut m = MeetingMonitor::new();
        m.scheduled(meeting);
        assert_eq!(m.input(Input::NotThisOne), vec![Action::HidePrompt]);
        assert!(m.answered.contains_key("e1"));
        assert_eq!(m.watch(&started(10, ZOOM, "zoom.us"), 70_000), vec![], "its call does not prompt");
        // Held as declined until it ends; a rejoin during the meeting is still it (no link named,
        // so any call in its time: rv-20261014T105556Z #1), and a call after it ends prompts.
        m.watch(&stopped(10, ZOOM), 90_000);
        m.tick(90_000 + END_GRACE_MS);
        assert_eq!(m.watch(&started(11, ZOOM, "zoom.us"), 200_000), vec![], "a rejoin does not prompt");
        m.watch(&stopped(11, ZOOM), 300_000);
        m.tick(300_000 + END_GRACE_MS);
        m.clock(3_660_000, &[]);
        assert_eq!(m.watch(&started(12, ZOOM, "zoom.us"), 3_700_000), zoom_prompt());
        // Never joined: once the meeting is over the app retires it, and a later call prompts.
        let mut r = MeetingMonitor::new();
        r.scheduled(Scheduled { event_id: "e2".into(), title: "Sync".into(), starts_at: 60_000, ends_at: 60_000 + 3_600_000, app: None });
        r.input(Input::NotThisOne);
        assert_eq!(r.retire("e2"), vec![]);
        assert!(r.answered.is_empty());
        assert_eq!(r.watch(&started(10, ZOOM, "zoom.us"), 9_000_000), zoom_prompt());
    }

    #[test]
    fn a_call_that_starts_while_a_pre_call_capture_is_failed_is_followed_by_the_retry() {
        // Second review rv-20261013T120455Z-516bb135-67f2 #2: the retry stayed unarmed, so
        // hang-up never stopped it.
        let mut m = MeetingMonitor::new();
        m.scheduled(Scheduled { event_id: "e1".into(), title: "Standup".into(), starts_at: 60_000, ends_at: 60_000 + 3_600_000, app: None });
        m.input(Input::TakeNotes);
        m.input(Input::CaptureEnded);
        assert_eq!(m.watch(&started(10, ZOOM, "zoom.us"), 70_000), vec![], "no second prompt");
        m.input(Input::Retry);
        assert!(m.is_recording());
        m.watch(&stopped(10, ZOOM), 100_000);
        assert_eq!(m.tick(100_000 + END_GRACE_MS), vec![Action::StopCapture(EndReason::CallEnded)]);
    }

    #[test]
    fn settings_start_on_a_failed_capture_retries_that_recording() {
        // Second review rv-20261013T110409Z-a46fc74d-c9b9 #3: it did nothing and left the bar up.
        let mut m = MeetingMonitor::new();
        m.watch(&started(10, ZOOM, "zoom.us"), 0);
        m.input(Input::TakeNotes);
        m.input(Input::CaptureEnded);
        assert_eq!(m.input(Input::ManualStart), vec![Action::HidePrompt, Action::StartCapture { title: None, event_id: None, manual: false }]);
        m.watch(&stopped(10, ZOOM), 10_000);
        assert_eq!(m.tick(10_000 + END_GRACE_MS), vec![Action::StopCapture(EndReason::CallEnded)]);
    }

    #[test]
    fn richos_own_capture_process_never_counts() {
        let mut m = MeetingMonitor::new();
        m.watch(&started(10, ZOOM, "zoom.us"), 0);
        m.input(Input::TakeNotes);
        m.exclude_pid(99);
        assert_eq!(m.watch(&started(99, "pid:99", "richos-companion"), 100), vec![]);
        m.watch(&stopped(10, ZOOM), 5_000);
        assert_eq!(m.tick(5_000 + END_GRACE_MS), vec![Action::StopCapture(EndReason::CallEnded)]);
    }

    #[test]
    fn another_app_during_a_recording_is_not_a_second_call() {
        let mut m = MeetingMonitor::new();
        m.watch(&started(10, ZOOM, "zoom.us"), 0);
        m.input(Input::TakeNotes);
        assert_eq!(m.watch(&started(20, "com.apple.VoiceMemos", "Voice Memos"), 1_000), vec![]);
        m.watch(&stopped(10, ZOOM), 5_000);
        // Still on the microphone when the call ends, it is the next one (rv-20261013T130528Z #3).
        assert_eq!(
            m.tick(5_000 + END_GRACE_MS),
            vec![Action::StopCapture(EndReason::CallEnded), Action::ShowPrompt(PromptKind::Unscheduled { app: "Voice Memos".into() })]
        );
    }

    #[test]
    fn paused_prompts_show_nothing_and_leave_a_recording_alone() {
        let mut m = MeetingMonitor::new();
        m.set_paused(true);
        assert_eq!(m.watch(&started(10, ZOOM, "zoom.us"), 0), vec![]);
        assert_eq!(m.scheduled(Scheduled { event_id: "e1".into(), title: "x".into(), starts_at: 0, ends_at: 3_600_000, app: None }), vec![]);
        // Resuming halfway through that call does not prompt for it.
        m.set_paused(false);
        assert_eq!(m.watch(&started(11, ZOOM, "zoom.us"), 1_000), vec![]);

        let mut r = MeetingMonitor::new();
        r.watch(&started(10, ZOOM, "zoom.us"), 0);
        r.input(Input::TakeNotes);
        assert_eq!(r.set_paused(true), vec![]);
        assert!(r.is_recording());
    }

    #[test]
    fn a_canceled_meeting_withdraws_its_prompt_and_a_moved_one_prompts_again() {
        let mut m = MeetingMonitor::new();
        let s = Scheduled { event_id: "e1".into(), title: "x".into(), starts_at: 60_000, ends_at: 60_000 + 3_600_000, app: None };
        m.scheduled(s.clone());
        assert_eq!(m.withdraw("e1"), vec![Action::HidePrompt]);
        assert!(!m.is_prompting());
        assert_eq!(m.scheduled(s).len(), 1, "moved to a later time: it prompts at the new time");
    }

    // ---- Call sequences, as a table ---------------------------------------------------------
    //
    // Four second reviews in a row (rv-20261013T090233Z, rv-20261013T100322Z-8bfcf4fe-37ac,
    // rv-20261013T110409Z-a46fc74d-c9b9, rv-20261013T120455Z-516bb135-67f2, then
    // rv-20261013T130528Z-361c4090-dbf1) each found one more sequence this state got wrong. This
    // is every sequence the monitor must handle, written as what the CEO would see: each row is a
    // moment (seconds), what happens then, what the app is told to do since the row before, and
    // the state after. The clock runs once a second through MeetingMonitor::clock, with the
    // calendar the rows have added, exactly as the app's clock thread runs it.

    const TEAMS: &str = "com.microsoft.teams2";
    const CAPTURE: &str = "pid:99";
    const CHROME: &str = "com.google.Chrome";

    #[derive(Debug, Clone)]
    enum Ev {
        /// A process of this app family starts using the microphone.
        On(i32, &'static str),
        /// It stops.
        Off(i32, &'static str),
        /// The synced calendar gains a meeting: title (also its id), start and end, in seconds.
        Cal(&'static str, u64, u64),
        /// The same, with a meeting link in this app.
        CalIn(&'static str, u64, u64, MeetingApp),
        /// The CEO's input or the capture's news.
        Do(Input),
        /// The app excludes its own capture process.
        Exclude(i32),
        Pause(bool),
        /// Only the clock runs.
        Wait,
    }
    use Ev::*;

    fn app_of(family: &str) -> &'static str {
        match family {
            ZOOM => "Zoom",
            TEAMS => "Teams",
            CHROME => "Chrome",
            _ => "RichOS capture",
        }
    }

    fn say(a: &Action) -> String {
        match a {
            Action::ShowPrompt(PromptKind::Unscheduled { app }) => format!("prompt {app}"),
            Action::ShowPrompt(PromptKind::Scheduled { title, .. }) => format!("prompt '{title}'"),
            Action::HidePrompt => "hide".into(),
            Action::StartCapture { manual: true, .. } => "start manual".into(),
            Action::StartCapture { title: Some(t), .. } => format!("start '{t}'"),
            Action::StartCapture { title: None, .. } => "start".into(),
            Action::StopCapture(EndReason::CallEnded) => "stop: call ended".into(),
            Action::StopCapture(EndReason::StoppedEarly) => "stop: early".into(),
        }
    }

    fn state(m: &MeetingMonitor) -> String {
        let on = |call: &Option<Call>| match call {
            Some(c) => format!(" on {}", app_of(&c.family)),
            None => String::new(),
        };
        let what = |start: &Start, call: &Option<Call>| {
            let title = start.title.as_ref().map(|t| format!(" '{t}'")).unwrap_or_default();
            let tail = match (start.manual, call) {
                (true, _) => " manual".to_string(),
                (false, None) => " unarmed".to_string(),
                (false, c) => on(c),
            };
            format!("{title}{tail}")
        };
        match &m.phase {
            Phase::Idle => "idle".into(),
            Phase::Prompting { call, scheduled } => {
                let title = scheduled.as_ref().map(|s| format!(" '{}'", s.title)).unwrap_or_default();
                format!("prompting{title}{}", on(call))
            }
            Phase::Recording { call, start } => format!("recording{}", what(start, call)),
            Phase::Failed { call, start } => format!("failed{}", what(start, call)),
            Phase::Done { call } => format!("holding {}", app_of(&call.family)),
        }
    }

    type Row = (u64, Ev, &'static [&'static str], &'static str);

    /// Walk one table; every mismatch is returned, not just the first.
    fn walk(name: &str, rows: &[Row]) -> Vec<String> {
        let mut m = MeetingMonitor::new();
        let mut calendar: Vec<CalendarMeeting> = Vec::new();
        let mut clock = 0u64;
        let mut wrong = Vec::new();
        for (i, (t, ev, want, want_state)) in rows.iter().enumerate() {
            let mut got = Vec::new();
            while clock < *t {
                clock += 1;
                got.extend(m.clock(clock * 1000, &calendar));
            }
            let now = t * 1000;
            got.extend(match ev.clone() {
                On(pid, family) => m.watch(&started(pid, family, app_of(family)), now),
                Off(pid, family) => m.watch(&stopped(pid, family), now),
                Cal(title, start, end) => {
                    calendar.push(CalendarMeeting { event_id: title.into(), title: title.into(), starts_at: start * 1000, ends_at: Some(end * 1000), app: None });
                    m.clock(now, &calendar)
                }
                CalIn(title, start, end, app) => {
                    calendar.push(CalendarMeeting { event_id: title.into(), title: title.into(), starts_at: start * 1000, ends_at: Some(end * 1000), app: Some(app) });
                    m.clock(now, &calendar)
                }
                Do(input) => m.input(input),
                Exclude(pid) => {
                    m.exclude_pid(pid);
                    vec![]
                }
                Pause(p) => m.set_paused(p),
                Wait => vec![],
            });
            let got: Vec<String> = got.iter().map(say).collect();
            if got != *want || state(&m) != *want_state {
                wrong.push(format!(
                    "{name}, row {} (t={t} s, {ev:?}): told {got:?}, now {:?}; expected {want:?}, {want_state:?}",
                    i + 1,
                    state(&m)
                ));
                break;
            }
        }
        wrong
    }

    #[test]
    fn every_call_sequence_gives_the_expected_prompt_and_recording() {
        use Input::*;
        let tables: &[(&str, &[Row])] = &[
            ("back-to-back calls, in two apps and then the same app", &[
                (10, On(1, ZOOM), &["prompt Zoom"], "prompting on Zoom"),
                (12, Do(TakeNotes), &["hide", "start"], "recording on Zoom"),
                (100, Off(1, ZOOM), &[], "recording on Zoom"),
                (107, Wait, &[], "recording on Zoom"),
                (108, Wait, &["stop: call ended"], "idle"),
                (130, On(2, TEAMS), &["prompt Teams"], "prompting on Teams"),
                (131, Do(TakeNotes), &["hide", "start"], "recording on Teams"),
                (200, Off(2, TEAMS), &[], "recording on Teams"),
                (208, Wait, &["stop: call ended"], "idle"),
                (220, On(3, ZOOM), &["prompt Zoom"], "prompting on Zoom"),
            ]),
            ("the next call starts inside the end grace (rv-20261013T130528Z #3)", &[
                (10, On(1, ZOOM), &["prompt Zoom"], "prompting on Zoom"),
                (12, Do(TakeNotes), &["hide", "start"], "recording on Zoom"),
                (100, Off(1, ZOOM), &[], "recording on Zoom"),
                (102, On(2, TEAMS), &[], "recording on Zoom"),
                (108, Wait, &["stop: call ended", "prompt Teams"], "prompting on Teams"),
                (110, Do(TakeNotes), &["hide", "start"], "recording on Teams"),
                (300, Off(2, TEAMS), &[], "recording on Teams"),
                (308, Wait, &["stop: call ended"], "idle"),
            ]),
            ("the next call in the same app inside the end grace is its own call (rv-20261013T135051Z #1)", &[
                (10, On(1, ZOOM), &["prompt Zoom"], "prompting on Zoom"),
                (12, Do(TakeNotes), &["hide", "start"], "recording on Zoom"),
                (100, Off(1, ZOOM), &[], "recording on Zoom"),
                (102, On(1, ZOOM), &["stop: call ended", "prompt Zoom"], "prompting on Zoom"),
                (110, Wait, &[], "prompting on Zoom"),
                (111, Do(TakeNotes), &["hide", "start"], "recording on Zoom"),
                (200, Off(1, ZOOM), &[], "recording on Zoom"),
                (208, Wait, &["stop: call ended"], "idle"),
            ]),
            // A mute does not stop the app's input at all (END_GRACE_MS, measured): what can come
            // and go inside a call is a second process of the same app.
            ("a second process of the call's app is the same call", &[
                (10, On(1, ZOOM), &["prompt Zoom"], "prompting on Zoom"),
                (12, Do(TakeNotes), &["hide", "start"], "recording on Zoom"),
                (50, On(2, ZOOM), &[], "recording on Zoom"),
                (100, Off(1, ZOOM), &[], "recording on Zoom"),
                (130, Wait, &[], "recording on Zoom"),
                (200, Off(2, ZOOM), &[], "recording on Zoom"),
                (208, Wait, &["stop: call ended"], "idle"),
            ]),
            ("a second app while a prompt is up prompts when the first call goes", &[
                (10, On(1, ZOOM), &["prompt Zoom"], "prompting on Zoom"),
                (20, On(2, TEAMS), &[], "prompting on Zoom"),
                (30, Off(1, ZOOM), &[], "prompting on Zoom"),
                (38, Wait, &["hide", "prompt Teams"], "prompting on Teams"),
            ]),
            ("calendar prompt, then its call", &[
                (0, Cal("Weekly sync", 60, 1860), &["prompt 'Weekly sync'"], "prompting 'Weekly sync'"),
                (65, On(1, ZOOM), &[], "prompting 'Weekly sync' on Zoom"),
                (70, Do(TakeNotes), &["hide", "start 'Weekly sync'"], "recording 'Weekly sync' on Zoom"),
                (1800, Off(1, ZOOM), &[], "recording 'Weekly sync' on Zoom"),
                (1808, Wait, &["stop: call ended"], "idle"),
                (1840, Wait, &[], "idle"),
            ]),
            ("Take Notes before joining: the call arms it and ends it", &[
                (0, Cal("Board", 60, 1860), &["prompt 'Board'"], "prompting 'Board'"),
                (10, Do(TakeNotes), &["hide", "start 'Board'"], "recording 'Board' unarmed"),
                (600, Wait, &[], "recording 'Board' unarmed"),
                (610, On(1, ZOOM), &[], "recording 'Board' on Zoom"),
                (1500, Off(1, ZOOM), &[], "recording 'Board' on Zoom"),
                (1508, Wait, &["stop: call ended"], "idle"),
            ]),
            ("the capture's own microphone use reported before the app excludes it", &[
                (0, Cal("Board", 60, 1860), &["prompt 'Board'"], "prompting 'Board'"),
                (10, Do(TakeNotes), &["hide", "start 'Board'"], "recording 'Board' unarmed"),
                (11, On(99, CAPTURE), &[], "recording 'Board' on RichOS capture"),
                (11, Exclude(99), &[], "recording 'Board' unarmed"),
                (70, On(1, ZOOM), &[], "recording 'Board' on Zoom"),
                (900, Off(1, ZOOM), &[], "recording 'Board' on Zoom"),
                (908, Wait, &["stop: call ended"], "idle"),
            ]),
            // Rejoining inside the meeting's time is still that meeting (rv-20261014T105556Z #1);
            // a call after it has ended is a different call and asks.
            ("a calendar prompt dismissed, then joined, left and rejoined (rv-20261013T120455Z #1)", &[
                (0, Cal("Standup", 60, 960), &["prompt 'Standup'"], "prompting 'Standup'"),
                (5, Do(NotThisOne), &["hide"], "idle"),
                (60, On(1, ZOOM), &[], "holding Zoom"),
                (900, Off(1, ZOOM), &[], "holding Zoom"),
                (908, Wait, &[], "idle"),
                (930, On(2, ZOOM), &[], "holding Zoom"),
                (940, Off(2, ZOOM), &[], "holding Zoom"),
                (948, Wait, &[], "idle"),
                (1000, On(3, ZOOM), &["prompt Zoom"], "prompting on Zoom"),
            ]),
            ("a call dismissed; a second app waits for it to end", &[
                (10, On(1, ZOOM), &["prompt Zoom"], "prompting on Zoom"),
                (12, Do(NotThisOne), &["hide"], "holding Zoom"),
                (20, On(2, TEAMS), &[], "holding Zoom"),
                (100, Off(1, ZOOM), &[], "holding Zoom"),
                (108, Wait, &["prompt Teams"], "prompting on Teams"),
            ]),
            ("a failed capture, Try again, a second failure, then a new call", &[
                (10, On(1, ZOOM), &["prompt Zoom"], "prompting on Zoom"),
                (12, Do(TakeNotes), &["hide", "start"], "recording on Zoom"),
                (40, Do(CaptureEnded), &[], "failed on Zoom"),
                (50, On(2, TEAMS), &[], "failed on Zoom"),
                (60, Do(Retry), &["hide", "start"], "recording on Zoom"),
                (70, Do(CaptureEnded), &[], "failed on Zoom"),
                (100, Off(1, ZOOM), &[], "failed on Zoom"),
                (108, Wait, &["prompt Teams"], "prompting on Teams"),
            ]),
            ("a pre-call capture fails, then the call starts (rv-20261013T120455Z #2)", &[
                (0, Cal("Standup", 60, 960), &["prompt 'Standup'"], "prompting 'Standup'"),
                (5, Do(TakeNotes), &["hide", "start 'Standup'"], "recording 'Standup' unarmed"),
                (6, Do(CaptureEnded), &[], "failed 'Standup' unarmed"),
                (60, On(1, ZOOM), &[], "failed 'Standup' on Zoom"),
                (61, Do(Retry), &["hide", "start 'Standup'"], "recording 'Standup' on Zoom"),
                (900, Off(1, ZOOM), &[], "recording 'Standup' on Zoom"),
                (908, Wait, &["stop: call ended"], "idle"),
            ]),
            ("two calendar meetings due: the later replaces a prompt nobody joined", &[
                (0, Cal("Missed", 0, 1800), &["prompt 'Missed'"], "prompting 'Missed'"),
                (0, Cal("Pricing", 120, 1920), &[], "prompting 'Missed'"),
                (60, Wait, &["prompt 'Pricing'"], "prompting 'Pricing'"),
                (125, On(1, ZOOM), &[], "prompting 'Pricing' on Zoom"),
                (126, Do(TakeNotes), &["hide", "start 'Pricing'"], "recording 'Pricing' on Zoom"),
            ]),
            ("two calendar meetings back to back, the first recorded (rv-20261013T130528Z #1)", &[
                (0, Cal("A", 60, 1860), &["prompt 'A'"], "prompting 'A'"),
                (0, Cal("B", 1860, 3660), &[], "prompting 'A'"),
                (60, On(1, ZOOM), &[], "prompting 'A' on Zoom"),
                (61, Do(TakeNotes), &["hide", "start 'A'"], "recording 'A' on Zoom"),
                (1810, Wait, &[], "recording 'A' on Zoom"),
                (1850, Off(1, ZOOM), &[], "recording 'A' on Zoom"),
                (1858, Wait, &["stop: call ended", "prompt 'B'"], "prompting 'B'"),
                (1865, On(2, ZOOM), &[], "prompting 'B' on Zoom"),
                (1866, Do(TakeNotes), &["hide", "start 'B'"], "recording 'B' on Zoom"),
            ]),
            ("a meeting due during a long declined call prompts when that call ends", &[
                (10, On(1, ZOOM), &["prompt Zoom"], "prompting on Zoom"),
                (12, Do(NotThisOne), &["hide"], "holding Zoom"),
                (20, Cal("Later", 3000, 4800), &[], "holding Zoom"),
                (2960, Wait, &[], "holding Zoom"),
                (3100, Off(1, ZOOM), &[], "holding Zoom"),
                (3108, Wait, &["prompt 'Later'"], "prompting 'Later'"),
            ]),
            ("joining a little early: the meeting due during that call IS that call", &[
                (0, Cal("Early", 300, 2100), &[], "idle"),
                (200, On(1, ZOOM), &["prompt Zoom"], "prompting on Zoom"),
                (240, Wait, &["prompt 'Early'"], "prompting 'Early' on Zoom"),
                (245, Do(TakeNotes), &["hide", "start 'Early'"], "recording 'Early' on Zoom"),
                (1500, Off(1, ZOOM), &[], "recording 'Early' on Zoom"),
                (1508, Wait, &["stop: call ended"], "idle"),
                (1530, Wait, &[], "idle"),
            ]),
            ("a short call declined and over before a different meeting starts: that meeting prompts (rv-20261013T135051Z #2)", &[
                (0, On(1, ZOOM), &["prompt Zoom"], "prompting on Zoom"),
                (1, Cal("B", 300, 2100), &[], "prompting on Zoom"),
                (2, Do(NotThisOne), &["hide"], "holding Zoom"),
                (241, Wait, &[], "holding Zoom"),
                (250, Off(1, ZOOM), &[], "holding Zoom"),
                (258, Wait, &["prompt 'B'"], "prompting 'B'"),
                (300, On(2, TEAMS), &[], "prompting 'B' on Teams"),
                (301, Do(TakeNotes), &["hide", "start 'B'"], "recording 'B' on Teams"),
            ]),
            ("a known meeting declined in its call, which runs into the next meeting: the next prompts when it ends (rv-20261013T141401Z #1)", &[
                (0, Cal("A", 60, 660), &["prompt 'A'"], "prompting 'A'"),
                (0, Cal("B", 660, 1260), &[], "prompting 'A'"),
                (60, On(1, ZOOM), &[], "prompting 'A' on Zoom"),
                (61, Do(NotThisOne), &["hide"], "holding Zoom"),
                (600, Wait, &[], "holding Zoom"),
                (670, Off(1, ZOOM), &[], "holding Zoom"),
                (678, Wait, &["prompt 'B'"], "prompting 'B'"),
            ]),
            ("a known meeting stopped early, its call running into the next meeting: the next prompts when it ends", &[
                (0, Cal("A", 60, 660), &["prompt 'A'"], "prompting 'A'"),
                (0, Cal("B", 660, 1260), &[], "prompting 'A'"),
                (60, On(1, ZOOM), &[], "prompting 'A' on Zoom"),
                (61, Do(TakeNotes), &["hide", "start 'A'"], "recording 'A' on Zoom"),
                (120, Do(Stop), &["stop: early"], "holding Zoom"),
                (600, Wait, &[], "holding Zoom"),
                (670, Off(1, ZOOM), &[], "holding Zoom"),
                (678, Wait, &["prompt 'B'"], "prompting 'B'"),
            ]),
            ("a known meeting declined before joining, its call running into the next meeting: the next prompts when it ends", &[
                (0, Cal("A", 60, 660), &["prompt 'A'"], "prompting 'A'"),
                (0, Cal("B", 660, 1260), &[], "prompting 'A'"),
                (5, Do(NotThisOne), &["hide"], "idle"),
                (60, On(1, ZOOM), &[], "holding Zoom"),
                (600, Wait, &[], "holding Zoom"),
                (670, Off(1, ZOOM), &[], "holding Zoom"),
                (678, Wait, &["prompt 'B'"], "prompting 'B'"),
            ]),
            ("joining early and recording before the meeting is due: no second prompt after", &[
                (0, Cal("Early", 300, 2100), &[], "idle"),
                (200, On(1, ZOOM), &["prompt Zoom"], "prompting on Zoom"),
                (205, Do(TakeNotes), &["hide", "start"], "recording on Zoom"),
                (260, Wait, &[], "recording on Zoom"),
                (1500, Off(1, ZOOM), &[], "recording on Zoom"),
                (1508, Wait, &["stop: call ended"], "idle"),
                (1530, Wait, &[], "idle"),
            ]),
            ("paused: nothing prompts, the call is not prompted on resume, the next one is", &[
                (0, Pause(true), &[], "idle"),
                (10, On(1, ZOOM), &[], "holding Zoom"),
                (20, Pause(false), &[], "holding Zoom"),
                (100, Off(1, ZOOM), &[], "holding Zoom"),
                (108, Wait, &[], "idle"),
                (120, On(2, ZOOM), &["prompt Zoom"], "prompting on Zoom"),
            ]),
            ("manual notes, then a call: offered when the notes stop", &[
                (0, Do(ManualStart), &["start manual"], "recording manual"),
                (10, On(1, ZOOM), &[], "recording manual"),
                (60, Do(Stop), &["stop: early", "prompt Zoom"], "prompting on Zoom"),
            ]),
            ("Take Notes before joining, Stop, then joined: that meeting does not ask again (rv-20261014T095959Z #1)", &[
                (0, Cal("A", 60, 3660), &["prompt 'A'"], "prompting 'A'"),
                (5, Do(TakeNotes), &["hide", "start 'A'"], "recording 'A' unarmed"),
                (10, Do(Stop), &["stop: early"], "idle"),
                (60, On(1, ZOOM), &[], "holding Zoom"),
                (900, Off(1, ZOOM), &[], "holding Zoom"),
                (908, Wait, &[], "idle"),
                (930, On(2, ZOOM), &[], "holding Zoom"),
            ]),
            ("a pre-call capture fails and its card is closed, then joined: that meeting does not ask again (rv-20261014T095959Z #1)", &[
                (0, Cal("A", 60, 3660), &["prompt 'A'"], "prompting 'A'"),
                (5, Do(TakeNotes), &["hide", "start 'A'"], "recording 'A' unarmed"),
                (6, Do(CaptureEnded), &[], "failed 'A' unarmed"),
                (7, Do(NotThisOne), &["hide"], "idle"),
                (60, On(1, ZOOM), &[], "holding Zoom"),
            ]),
            // A declined meeting is matched by what it is: its own scheduled time and, when the
            // event names a link, the link's app (rv-20261014T102622Z-ea367a6c-5c3e). A call in
            // another app, or after the meeting ends, is a different call and asks. With no link
            // named, any call during the meeting's time is taken to be it.
            ("a Zoom meeting dismissed and never joined: an unrelated Teams call 20 minutes in asks (rv-20261014T095959Z #2)", &[
                (0, CalIn("A", 60, 3660, MeetingApp::Zoom), &["prompt 'A'"], "prompting 'A'"),
                (5, Do(NotThisOne), &["hide"], "idle"),
                (1260, On(1, TEAMS), &["prompt Teams"], "prompting on Teams"),
                (1262, Do(TakeNotes), &["hide", "start"], "recording on Teams"),
            ]),
            ("a meeting dismissed and never joined: a call after it has ended asks (rv-20261014T095959Z #2)", &[
                (0, Cal("A", 60, 3660), &["prompt 'A'"], "prompting 'A'"),
                (5, Do(NotThisOne), &["hide"], "idle"),
                (3700, On(1, ZOOM), &["prompt Zoom"], "prompting on Zoom"),
            ]),
            ("a Zoom meeting dismissed: an unrelated Teams call 5 minutes after its start asks (rv-20261014T102622Z #2)", &[
                (0, CalIn("A", 60, 3660, MeetingApp::Zoom), &["prompt 'A'"], "prompting 'A'"),
                (0, Do(NotThisOne), &["hide"], "idle"),
                (360, On(1, TEAMS), &["prompt Teams"], "prompting on Teams"),
            ]),
            ("a Zoom meeting stopped before joining, then joined in Zoom 11 minutes late: no prompt", &[
                (0, CalIn("A", 60, 3660, MeetingApp::Zoom), &["prompt 'A'"], "prompting 'A'"),
                (0, Do(TakeNotes), &["hide", "start 'A'"], "recording 'A' unarmed"),
                (0, Do(Stop), &["stop: early"], "idle"),
                (720, On(1, ZOOM), &[], "holding Zoom"),
            ]),
            // A browser call is a meeting only when the meeting's link is a browser meeting
            // (rv-20261014T105556Z #2): a Zoom or Teams meeting is joined in its app, and a
            // browser call during it is a different call with its own prompt.
            ("a Zoom meeting dismissed: a Google Meet call in Chrome 5 minutes in asks (rv-20261014T105556Z #2)", &[
                (0, CalIn("A", 60, 3660, MeetingApp::Zoom), &["prompt 'A'"], "prompting 'A'"),
                (0, Do(NotThisOne), &["hide"], "idle"),
                (360, On(1, CHROME), &["prompt Chrome"], "prompting on Chrome"),
            ]),
            ("a Teams meeting dismissed: a browser call during it asks", &[
                (0, CalIn("A", 60, 3660, MeetingApp::Teams), &["prompt 'A'"], "prompting 'A'"),
                (0, Do(NotThisOne), &["hide"], "idle"),
                (360, On(1, CHROME), &["prompt Chrome"], "prompting on Chrome"),
            ]),
            ("a Google Meet meeting dismissed, then joined in Chrome: that meeting, no prompt", &[
                (0, CalIn("A", 60, 3660, MeetingApp::Meet), &["prompt 'A'"], "prompting 'A'"),
                (0, Do(NotThisOne), &["hide"], "idle"),
                (120, On(1, CHROME), &[], "holding Chrome"),
                (900, Off(1, CHROME), &[], "holding Chrome"),
                (908, Wait, &[], "idle"),
                (930, On(2, CHROME), &[], "holding Chrome"),
            ]),
            // rv-20261014T105556Z-71dc7488-f9ab #1: Zoom-linked A (60 s to 3660 s) answered three
            // ways before anyone joined, joined at 720, left at 900, rejoined at 930. Every
            // rejoin is still A: no prompt, however often.
            ("Stop before joining, joined, left and rejoined (rv-20261014T105556Z #1)", &[
                (0, CalIn("A", 60, 3660, MeetingApp::Zoom), &["prompt 'A'"], "prompting 'A'"),
                (0, Do(TakeNotes), &["hide", "start 'A'"], "recording 'A' unarmed"),
                (0, Do(Stop), &["stop: early"], "idle"),
                (720, On(1, ZOOM), &[], "holding Zoom"),
                (900, Off(1, ZOOM), &[], "holding Zoom"),
                (908, Wait, &[], "idle"),
                (930, On(2, ZOOM), &[], "holding Zoom"),
                (1200, Off(2, ZOOM), &[], "holding Zoom"),
                (1208, Wait, &[], "idle"),
                (1500, On(3, ZOOM), &[], "holding Zoom"),
            ]),
            ("a failed pre-call capture closed, joined, left and rejoined (rv-20261014T105556Z #1)", &[
                (0, CalIn("A", 60, 3660, MeetingApp::Zoom), &["prompt 'A'"], "prompting 'A'"),
                (0, Do(TakeNotes), &["hide", "start 'A'"], "recording 'A' unarmed"),
                (0, Do(CaptureEnded), &[], "failed 'A' unarmed"),
                (0, Do(NotThisOne), &["hide"], "idle"),
                (720, On(1, ZOOM), &[], "holding Zoom"),
                (900, Off(1, ZOOM), &[], "holding Zoom"),
                (908, Wait, &[], "idle"),
                (930, On(2, ZOOM), &[], "holding Zoom"),
            ]),
            ("dismissed before joining, joined, left and rejoined; after it ends a call asks (rv-20261014T105556Z #1)", &[
                (0, CalIn("A", 60, 3660, MeetingApp::Zoom), &["prompt 'A'"], "prompting 'A'"),
                (0, Do(NotThisOne), &["hide"], "idle"),
                (720, On(1, ZOOM), &[], "holding Zoom"),
                (900, Off(1, ZOOM), &[], "holding Zoom"),
                (908, Wait, &[], "idle"),
                (930, On(2, ZOOM), &[], "holding Zoom"),
                (3600, Off(2, ZOOM), &[], "holding Zoom"),
                (3608, Wait, &[], "idle"),
                (3700, On(3, ZOOM), &["prompt Zoom"], "prompting on Zoom"),
            ]),
            ("A dismissed, an overlapping B dismissed, then A joined: still A (rv-20261014T105556Z #1)", &[
                (0, CalIn("A", 60, 3660, MeetingApp::Zoom), &["prompt 'A'"], "prompting 'A'"),
                (0, Do(NotThisOne), &["hide"], "idle"),
                (0, CalIn("B", 600, 1200, MeetingApp::Teams), &[], "idle"),
                (540, Wait, &["prompt 'B'"], "prompting 'B'"),
                (541, Do(NotThisOne), &["hide"], "idle"),
                (720, On(1, ZOOM), &[], "holding Zoom"),
                (900, Off(1, ZOOM), &[], "holding Zoom"),
                (908, Wait, &[], "idle"),
                (930, On(2, ZOOM), &[], "holding Zoom"),
            ]),
            // meet7b's own report: a rejoin after notes started recorded nothing. It records into
            // that meeting again, without asking.
            ("A's calendar prompt answered in its call with Take Notes, then left and rejoined: no prompt, it records A again", &[
                (0, CalIn("A", 60, 3660, MeetingApp::Zoom), &["prompt 'A'"], "prompting 'A'"),
                (60, On(1, ZOOM), &[], "prompting 'A' on Zoom"),
                (61, Do(TakeNotes), &["hide", "start 'A'"], "recording 'A' on Zoom"),
                (900, Off(1, ZOOM), &[], "recording 'A' on Zoom"),
                (908, Wait, &["stop: call ended"], "idle"),
                (930, On(2, ZOOM), &["start 'A'"], "recording 'A' on Zoom"),
                (1000, Off(2, ZOOM), &[], "recording 'A' on Zoom"),
                (1008, Wait, &["stop: call ended"], "idle"),
            ]),
            ("notes on A stopped early, left and rejoined: no prompt and nothing records", &[
                (0, CalIn("A", 60, 3660, MeetingApp::Zoom), &["prompt 'A'"], "prompting 'A'"),
                (60, On(1, ZOOM), &[], "prompting 'A' on Zoom"),
                (61, Do(TakeNotes), &["hide", "start 'A'"], "recording 'A' on Zoom"),
                (300, Do(Stop), &["stop: early"], "holding Zoom"),
                (900, Off(1, ZOOM), &[], "holding Zoom"),
                (908, Wait, &[], "idle"),
                (930, On(2, ZOOM), &[], "holding Zoom"),
            ]),
            ("notes on A, then a Teams call during A's time: its own prompt, A is not recorded on it", &[
                (0, CalIn("A", 60, 3660, MeetingApp::Zoom), &["prompt 'A'"], "prompting 'A'"),
                (60, On(1, ZOOM), &[], "prompting 'A' on Zoom"),
                (61, Do(TakeNotes), &["hide", "start 'A'"], "recording 'A' on Zoom"),
                (900, Off(1, ZOOM), &[], "recording 'A' on Zoom"),
                (908, Wait, &["stop: call ended"], "idle"),
                (930, On(2, TEAMS), &["prompt Teams"], "prompting on Teams"),
            ]),
            ("A's calendar prompt dismissed in its call, then left and rejoined: no prompt", &[
                (0, CalIn("A", 60, 3660, MeetingApp::Zoom), &["prompt 'A'"], "prompting 'A'"),
                (60, On(1, ZOOM), &[], "prompting 'A' on Zoom"),
                (61, Do(NotThisOne), &["hide"], "holding Zoom"),
                (900, Off(1, ZOOM), &[], "holding Zoom"),
                (908, Wait, &[], "idle"),
                (930, On(2, ZOOM), &[], "holding Zoom"),
            ]),
            ("joined early, notes taken before the meeting was due, left and rejoined: no prompt, it records that meeting", &[
                (0, Cal("Early", 300, 2100), &[], "idle"),
                (200, On(1, ZOOM), &["prompt Zoom"], "prompting on Zoom"),
                (205, Do(TakeNotes), &["hide", "start"], "recording on Zoom"),
                (900, Off(1, ZOOM), &[], "recording on Zoom"),
                (908, Wait, &["stop: call ended"], "idle"),
                (930, On(2, ZOOM), &["start 'Early'"], "recording 'Early' on Zoom"),
            ]),
            ("a calendar prompt ignored until its call ended: a rejoin asks", &[
                (0, CalIn("A", 60, 3660, MeetingApp::Zoom), &["prompt 'A'"], "prompting 'A'"),
                (60, On(1, ZOOM), &[], "prompting 'A' on Zoom"),
                (900, Off(1, ZOOM), &[], "prompting 'A' on Zoom"),
                (908, Wait, &["hide"], "idle"),
                (930, On(2, ZOOM), &["prompt Zoom"], "prompting on Zoom"),
            ]),
            ("a meeting dismissed, then joined nine minutes late: still that meeting, no prompt", &[
                (0, Cal("A", 60, 3660), &["prompt 'A'"], "prompting 'A'"),
                (5, Do(NotThisOne), &["hide"], "idle"),
                (600, On(1, ZOOM), &[], "holding Zoom"),
            ]),
            // rv-20261014T102622Z-ea367a6c-5c3e: a one-hour meeting A (60 s to 3660 s) answered
            // three ways before anyone joined, then joined on time and 11 minutes late. All six
            // are that meeting: no prompt.
            ("Stop before joining, joined on time (rv-20261014T102622Z)", &[
                (0, Cal("A", 60, 3660), &["prompt 'A'"], "prompting 'A'"),
                (0, Do(TakeNotes), &["hide", "start 'A'"], "recording 'A' unarmed"),
                (0, Do(Stop), &["stop: early"], "idle"),
                (60, On(1, ZOOM), &[], "holding Zoom"),
            ]),
            ("Stop before joining, joined 11 minutes late (rv-20261014T102622Z #1)", &[
                (0, Cal("A", 60, 3660), &["prompt 'A'"], "prompting 'A'"),
                (0, Do(TakeNotes), &["hide", "start 'A'"], "recording 'A' unarmed"),
                (0, Do(Stop), &["stop: early"], "idle"),
                (720, On(1, ZOOM), &[], "holding Zoom"),
            ]),
            ("a failed pre-call capture closed, joined on time (rv-20261014T102622Z)", &[
                (0, Cal("A", 60, 3660), &["prompt 'A'"], "prompting 'A'"),
                (0, Do(TakeNotes), &["hide", "start 'A'"], "recording 'A' unarmed"),
                (0, Do(CaptureEnded), &[], "failed 'A' unarmed"),
                (0, Do(NotThisOne), &["hide"], "idle"),
                (60, On(1, ZOOM), &[], "holding Zoom"),
            ]),
            ("a failed pre-call capture closed, joined 11 minutes late (rv-20261014T102622Z #1)", &[
                (0, Cal("A", 60, 3660), &["prompt 'A'"], "prompting 'A'"),
                (0, Do(TakeNotes), &["hide", "start 'A'"], "recording 'A' unarmed"),
                (0, Do(CaptureEnded), &[], "failed 'A' unarmed"),
                (0, Do(NotThisOne), &["hide"], "idle"),
                (720, On(1, ZOOM), &[], "holding Zoom"),
            ]),
            ("dismissed before joining, joined on time (rv-20261014T102622Z)", &[
                (0, Cal("A", 60, 3660), &["prompt 'A'"], "prompting 'A'"),
                (0, Do(NotThisOne), &["hide"], "idle"),
                (60, On(1, ZOOM), &[], "holding Zoom"),
            ]),
            ("dismissed before joining, joined 11 minutes late (rv-20261014T102622Z #1)", &[
                (0, Cal("A", 60, 3660), &["prompt 'A'"], "prompting 'A'"),
                (0, Do(NotThisOne), &["hide"], "idle"),
                (720, On(1, ZOOM), &[], "holding Zoom"),
            ]),
        ];
        let wrong: Vec<String> = tables.iter().flat_map(|(name, rows)| walk(name, rows)).collect();
        assert!(wrong.is_empty(), "{} of {} sequences went wrong:\n{}", wrong.len(), tables.len(), wrong.join("\n"));
    }

    /// The handover review's own witnesses (rv-20261014T105556Z-71dc7488-f9ab,
    /// `fixtures/meeting_identity_review.py`), driven through the monitor's API as it wrote them.
    mod review_rv_20261014t105556z {
        use super::super::*;
        const ZOOM: &str = "us.zoom.xos";
        const TEAMS: &str = "com.microsoft.teams2";
        fn on(m: &mut MeetingMonitor, family: &str, now: u64) -> Vec<Action> {
            m.watch(&WatchLine::Started { pid: 1, family: family.into(), app: Some(family.into()), at: now }, now)
        }
        fn off(m: &mut MeetingMonitor, family: &str, now: u64) {
            m.watch(&WatchLine::Stopped { pid: 1, family: family.into(), exited: false, at: now }, now);
            m.tick(now + END_GRACE_MS);
        }
        fn meeting(id: &str, app: Option<MeetingApp>, start: u64, end: u64) -> Scheduled {
            Scheduled { event_id: id.into(), title: id.into(), starts_at: start, ends_at: end, app }
        }
        fn answered(how: &str, app: Option<MeetingApp>) -> MeetingMonitor {
            let mut m = MeetingMonitor::new();
            m.scheduled(meeting("A", app, 60_000, 3_660_000));
            if how != "dismissed" {
                m.input(Input::TakeNotes);
            }
            if how == "failed capture closed" {
                m.input(Input::CaptureEnded);
            }
            m.input(if how == "Stop" { Input::Stop } else { Input::NotThisOne });
            m
        }

        #[test]
        fn earlier_findings_controls() {
            for how in ["Stop", "failed capture closed", "dismissed"] {
                for now in [60_000, 720_000] {
                    let mut m = answered(how, None);
                    assert_eq!(on(&mut m, ZOOM, now), vec![], "{how}, join at {now}");
                }
            }
            let mut m = answered("dismissed", Some(MeetingApp::Zoom));
            on(&mut m, TEAMS, 360_000);
            assert!(m.is_prompting(), "linked Zoom A, unrelated native Teams at 5 min");
        }

        #[test]
        fn same_answered_meeting_rejoined_inside_its_time() {
            for how in ["Stop", "failed capture closed", "dismissed"] {
                let mut m = answered(how, Some(MeetingApp::Zoom));
                assert_eq!(on(&mut m, ZOOM, 720_000), vec![], "{how}: first join");
                off(&mut m, ZOOM, 900_000);
                assert_eq!(on(&mut m, ZOOM, 930_000), vec![], "{how}: answered A asks again on rejoin");
            }
        }

        #[test]
        fn different_app_browser_call_gets_its_own_prompt() {
            let mut m = answered("dismissed", Some(MeetingApp::Zoom));
            on(&mut m, "com.google.Chrome", 360_000);
            assert!(m.is_prompting(), "different-app call inherited A's dismissal");
        }

        #[test]
        fn later_calendar_prompt_keeps_answer_for_earlier_meeting() {
            let mut m = answered("dismissed", Some(MeetingApp::Zoom));
            m.scheduled(meeting("B", Some(MeetingApp::Teams), 600_000, 1_200_000));
            m.input(Input::NotThisOne);
            assert_eq!(on(&mut m, ZOOM, 720_000), vec![], "B's prompt erased A's remembered answer");
        }
    }

    /// The handover review's witness (rv-20261014T110606Z-1e6b535c-f031 #1,
    /// `fixtures/behavior_probes.py`): a calendar prompt or pre-call recording for a Zoom-linked
    /// meeting took an unrelated Teams call as its own, in both orders.
    mod review_rv_20261014t110606z {
        use super::super::*;
        fn on(pid: i32, family: &str, app: &str) -> WatchLine {
            WatchLine::Started { pid, family: family.into(), app: Some(app.into()), at: 0 }
        }
        fn off(pid: i32, family: &str) -> WatchLine {
            WatchLine::Stopped { pid, family: family.into(), exited: false, at: 0 }
        }
        fn calendar() -> Vec<CalendarMeeting> {
            vec![CalendarMeeting {
                event_id: "zoom-A".into(),
                title: "Zoom customer review".into(),
                starts_at: 60_000,
                ends_at: Some(3_660_000),
                app: Some(MeetingApp::Zoom),
            }]
        }

        #[test]
        fn a_pre_call_recording_is_not_ended_by_another_apps_call() {
            let cal = calendar();
            let mut m = MeetingMonitor::new();
            assert_eq!(m.clock(0, &cal).len(), 1, "the calendar prompt");
            m.input(Input::TakeNotes);
            assert_eq!(m.watch(&on(7, "com.microsoft.teams2", "Teams"), 10_000), vec![]);
            m.watch(&off(7, "com.microsoft.teams2"), 30_000);
            assert_eq!(m.clock(38_000, &cal), vec![], "Teams ending stopped the Zoom meeting's recording");
            assert!(m.is_recording());
            assert_eq!(m.watch(&on(8, "us.zoom.xos", "Zoom"), 60_000), vec![], "the real Zoom join asked again");
            m.watch(&off(8, "us.zoom.xos"), 900_000);
            assert_eq!(m.clock(908_000, &cal), vec![Action::StopCapture(EndReason::CallEnded)], "the recording ends with Zoom");
        }

        #[test]
        fn a_zoom_meeting_due_during_a_teams_prompt_does_not_take_it_and_prompts_after() {
            let cal = calendar();
            let mut m = MeetingMonitor::new();
            assert_eq!(m.watch(&on(7, "com.microsoft.teams2", "Teams"), 0), vec![Action::ShowPrompt(PromptKind::Unscheduled { app: "Teams".into() })]);
            assert_eq!(m.clock(1_000, &cal), vec![], "the Zoom reminder replaced the Teams prompt");
            assert_eq!(m.input(Input::TakeNotes), vec![Action::HidePrompt, Action::StartCapture { title: None, event_id: None, manual: false }]);
            m.watch(&off(7, "com.microsoft.teams2"), 30_000);
            assert_eq!(
                m.clock(38_000, &cal),
                vec![
                    Action::StopCapture(EndReason::CallEnded),
                    Action::ShowPrompt(PromptKind::Scheduled { event_id: "zoom-A".into(), title: "Zoom customer review".into(), starts_at: 60_000 })
                ],
                "the Zoom meeting prompts once the Teams call is over"
            );
        }

        /// rv-20261014T114658Z-04417671-85c4 #1 (`fixtures/meeting_review.rs`): dismiss Zoom A, let
        /// the overlapping Teams B prompt, join A: Zoom was attached to B, B's notes were recorded
        /// from A's call and ended with it, and the real Teams B join was never recorded.
        #[test]
        fn an_answered_meeting_rejoined_while_another_meetings_prompt_is_up_stays_that_meeting() {
            let a = CalendarMeeting { event_id: "A".into(), title: "A".into(), starts_at: 60_000, ends_at: Some(3_660_000), app: Some(MeetingApp::Zoom) };
            let b = CalendarMeeting { event_id: "B".into(), title: "B".into(), starts_at: 600_000, ends_at: Some(1_200_000), app: Some(MeetingApp::Teams) };
            let mut m = MeetingMonitor::new();
            m.clock(0, std::slice::from_ref(&a));
            m.input(Input::NotThisOne);
            let cal = vec![a, b];
            assert_eq!(m.clock(540_000, &cal), vec![Action::ShowPrompt(PromptKind::Scheduled { event_id: "B".into(), title: "B".into(), starts_at: 600_000 })]);
            assert_eq!(m.watch(&on(1, "us.zoom.xos", "Zoom"), 720_000), vec![]);
            assert_eq!(m.input(Input::TakeNotes), vec![Action::HidePrompt, Action::StartCapture { title: Some("B".into()), event_id: Some("B".into()), manual: false }]);
            m.watch(&off(1, "us.zoom.xos"), 900_000);
            assert_eq!(m.clock(908_000, &cal), vec![], "A's Zoom call ending stopped B's recording");
            assert!(m.is_recording());
            assert_eq!(m.watch(&on(2, "com.microsoft.teams2", "Teams"), 930_000), vec![], "B's own call joins its recording");
            m.watch(&off(2, "com.microsoft.teams2"), 1_100_000);
            assert_eq!(m.clock(1_108_000, &cal), vec![Action::StopCapture(EndReason::CallEnded)], "B's recording ends with B's Teams call");

            // The same with B naming no link: a call A's Zoom link names is still A rejoined.
            let a = CalendarMeeting { event_id: "A".into(), title: "A".into(), starts_at: 60_000, ends_at: Some(3_660_000), app: Some(MeetingApp::Zoom) };
            let b = CalendarMeeting { event_id: "B".into(), title: "B".into(), starts_at: 600_000, ends_at: Some(1_200_000), app: None };
            let mut m = MeetingMonitor::new();
            m.clock(0, std::slice::from_ref(&a));
            m.input(Input::NotThisOne);
            let cal = vec![a, b];
            assert_eq!(m.clock(540_000, &cal).len(), 1, "B prompts");
            assert_eq!(m.watch(&on(1, "us.zoom.xos", "Zoom"), 720_000), vec![]);
            assert_eq!(m.input(Input::TakeNotes).len(), 2);
            m.watch(&off(1, "us.zoom.xos"), 900_000);
            assert_eq!(m.clock(908_000, &cal), vec![], "A's Zoom call was taken for no-link B");
            assert!(m.is_recording());
        }
    }

    /// rv-20261014T114658Z-04417671-85c4 #2 (`fixtures/meeting_review.rs`): an answer kept the
    /// meeting's end as it was when he answered, though the calendar later moved the end.
    mod review_rv_20261014t114658z {
        use super::super::*;
        fn zoom_on(m: &mut MeetingMonitor, seconds: u64) -> Vec<Action> {
            m.watch(&WatchLine::Started { pid: 1, family: "us.zoom.xos".into(), app: Some("Zoom".into()), at: seconds * 1000 }, seconds * 1000)
        }
        fn a(end: u64) -> CalendarMeeting {
            CalendarMeeting { event_id: "A".into(), title: "A".into(), starts_at: 60_000, ends_at: Some(end * 1000), app: Some(MeetingApp::Zoom) }
        }
        fn dismissed() -> MeetingMonitor {
            let mut m = MeetingMonitor::new();
            m.clock(0, &[a(3660)]);
            m.input(Input::NotThisOne);
            m
        }

        #[test]
        fn an_answered_meeting_extended_by_the_calendar_still_covers_a_rejoin() {
            let mut m = dismissed();
            m.clock(3_000_000, &[a(7260)]);
            m.clock(3_660_000, &[a(7260)]);
            assert_eq!(zoom_on(&mut m, 4000), vec![], "extended to 7260 s: a join at 4000 s asked again");
        }

        #[test]
        fn an_answered_meeting_shortened_by_the_calendar_ends_at_its_new_end() {
            let mut m = dismissed();
            m.clock(600_000, &[a(1200)]);
            m.clock(1_300_000, &[a(1200)]);
            assert_eq!(
                zoom_on(&mut m, 1300),
                vec![Action::ShowPrompt(PromptKind::Unscheduled { app: "Zoom".into() })],
                "shortened to 1200 s: a different Zoom call at 1300 s was taken for it"
            );
        }
    }
}

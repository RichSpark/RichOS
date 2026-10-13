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
//! any process of that family runs input, and it has ENDED once none has for [`END_GRACE_MS`].
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
//! detected in …"). Not this one, and Stop, hold for that call: it does not prompt again until
//! it has ended. The next call prompts normally. While prompts are paused nothing prompts, and
//! a running recording is left alone.

use std::collections::BTreeSet;

/// How long every process of the call's app must have been off the microphone before the call
/// counts as ended; a mute, a silence, or a device switch inside a call must not reach it.
///
/// MEASURED (2026-10-13, real Zoom calls from a test VM guest against the Windows call lab;
/// richos-hq `docs/verification/2026-10-13-granola-end-detection/`): the call app's input held
/// through every mute (55 s native Zoom, 47 s Zoom in Chrome) and every minute of silence on both
/// routes, with no gap at all inside a call, and stopped 2.6 s after the guest left, 3.2 to 4.0 s
/// after the host ended the meeting and 1.8 s after the tab closed. An idle Zoom, an idle Chrome
/// and played sounds never started input. 8 s is therefore longer than any in-call gap measured
/// (none) and ends a recording within about 12 s of hang-up.
pub const END_GRACE_MS: u64 = 8_000;

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

/// A synced calendar meeting that is due to prompt (the calendar side decides "one minute
/// before a timed meeting with other attendees"; this module only places it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scheduled {
    pub event_id: String,
    pub title: String,
    pub starts_at: u64,
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
}

impl Call {
    fn new(family: &str, app: Option<String>, pid: i32) -> Call {
        Call { family: family.to_string(), app, live: BTreeSet::from([pid]), quiet_since: None }
    }
    fn ended(&self, now: u64) -> bool {
        self.live.is_empty() && self.quiet_since.is_some_and(|q| now.saturating_sub(q) >= END_GRACE_MS)
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
    /// Calendar events already prompted for (or declined), so a refresh does not prompt twice.
    prompted_events: BTreeSet<String>,
    /// A calendar meeting declined before anyone joined it: its call, when it starts, is that
    /// declined meeting and does not prompt (second review rv-20261013T120455Z-516bb135-67f2 #1).
    /// Cleared when that call starts, when a later meeting prompts, or when the meeting is over.
    declined_event: Option<String>,
}

impl MeetingMonitor {
    pub fn new() -> Self {
        Self::default()
    }

    /// A process whose microphone use is RichOS's own (the capture process it starts).
    pub fn exclude_pid(&mut self, pid: i32) {
        self.excluded.insert(pid);
        if let Some(c) = self.phase.call_mut() {
            c.live.remove(&pid);
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
            WatchLine::Started { pid, family, app, .. } if !self.excluded.contains(pid) => self.mic_started(*pid, family, app.clone()),
            WatchLine::Stopped { pid, family, .. } if !self.excluded.contains(pid) => self.mic_stopped(*pid, family, now),
            _ => vec![],
        }
    }

    fn mic_started(&mut self, pid: i32, family: &str, app: Option<String>) -> Vec<Action> {
        if let Some(c) = self.phase.call_mut() {
            if c.family == family {
                c.live.insert(pid);
                c.quiet_since = None;
            }
            // Another app while one call is prompting, recording or declined: one prompt and one
            // recording at a time, never a second.
            return vec![];
        }
        match &mut self.phase {
            Phase::Idle if self.declined_event.is_some() => {
                // The calendar meeting he declined before joining: this is its call. Held as
                // declined until it ends, like any declined call.
                self.declined_event = None;
                self.phase = Phase::Done { call: Call::new(family, app, pid) };
                vec![]
            }
            Phase::Idle => {
                let call = Call::new(family, app, pid);
                if self.paused {
                    // Nothing prompts while paused; remember the call so it does not prompt the
                    // moment prompts resume halfway through it.
                    self.phase = Phase::Done { call };
                    return vec![];
                }
                let kind = PromptKind::Unscheduled { app: call.app.clone().unwrap_or_else(|| call.family.clone()) };
                self.phase = Phase::Prompting { call: Some(call), scheduled: None };
                vec![Action::ShowPrompt(kind)]
            }
            // A calendar prompt nobody had joined: this is its call (no second prompt).
            Phase::Prompting { call, .. } => {
                *call = Some(Call::new(family, app, pid));
                vec![]
            }
            // An unarmed calendar recording arms on the first call to start; a manual one never does.
            // So does a failed one waiting on its card, so Try again still ends with the call
            // (second review rv-20261013T120455Z-516bb135-67f2 #2).
            Phase::Recording { call, start } | Phase::Failed { call, start } if !start.manual => {
                *call = Some(Call::new(family, app, pid));
                vec![]
            }
            _ => vec![],
        }
    }

    fn mic_stopped(&mut self, pid: i32, family: &str, now: u64) -> Vec<Action> {
        if let Some(c) = self.phase.call_mut() {
            if c.family == family && c.live.remove(&pid) && c.live.is_empty() {
                c.quiet_since = Some(now);
            }
        }
        vec![]
    }

    /// The clock. Ends a call whose app has stayed off the microphone for [`END_GRACE_MS`].
    pub fn tick(&mut self, now: u64) -> Vec<Action> {
        let ended = self.phase.call_mut().is_some_and(|c| c.ended(now));
        if !ended {
            return vec![];
        }
        let out = match &mut self.phase {
            // The call went before anybody answered: the prompt goes with it.
            Phase::Prompting { .. } => vec![Action::HidePrompt],
            // A manual recording only followed a declined call; it ends with Stop alone.
            Phase::Recording { call, start } if start.manual => {
                *call = None;
                return vec![];
            }
            Phase::Recording { .. } => vec![Action::StopCapture(EndReason::CallEnded)],
            // A failed recording's call ended: nothing is left to retry against. The problem card
            // stays until answered; "Try again" is then a manual start.
            _ => vec![],
        };
        self.phase = Phase::Idle;
        out
    }

    /// A calendar meeting is due to prompt.
    pub fn scheduled(&mut self, meeting: Scheduled) -> Vec<Action> {
        if !self.prompted_events.insert(meeting.event_id.clone()) {
            return vec![];
        }
        let kind = PromptKind::Scheduled { event_id: meeting.event_id.clone(), title: meeting.title.clone(), starts_at: meeting.starts_at };
        match &mut self.phase {
            // Paused: no prompt. The event is remembered above, so it does not prompt on resume,
            // and nothing else is held: a call that starts later is free to prompt.
            Phase::Idle if self.paused => vec![],
            Phase::Idle => {
                self.declined_event = None;
                self.phase = Phase::Prompting { call: None, scheduled: Some(meeting) };
                vec![Action::ShowPrompt(kind)]
            }
            // An unscheduled prompt already up for a live call: the same meeting; give it the title.
            Phase::Prompting { scheduled: s @ None, .. } => {
                *s = Some(meeting);
                vec![Action::ShowPrompt(kind)]
            }
            // A calendar prompt nobody joined, and a later meeting is due: the later one replaces it
            // (second review rv-20261013T110409Z-a46fc74d-c9b9 #1: the old invitation stayed up for
            // ever and took the next call). A prompt with a live call is never replaced.
            Phase::Prompting { call: None, scheduled: Some(old) } if old.starts_at < meeting.starts_at => {
                self.phase = Phase::Prompting { call: None, scheduled: Some(meeting) };
                vec![Action::ShowPrompt(kind)]
            }
            // A recording, a declined call or another calendar prompt: no second prompt.
            _ => vec![],
        }
    }

    /// The calendar meeting a prompt nobody has joined is for, if that is what is up.
    pub fn unjoined_calendar_prompt(&self) -> Option<&str> {
        match &self.phase {
            Phase::Prompting { call: None, scheduled: Some(s) } => Some(&s.event_id),
            _ => None,
        }
    }

    /// The calendar meeting held without a call: an unjoined prompt, or one declined before
    /// anyone joined. The app retires it once the meeting is over.
    pub fn pending_calendar_event(&self) -> Option<&str> {
        self.unjoined_calendar_prompt().or(self.declined_event.as_deref())
    }

    /// The meeting a calendar prompt was for has ended and nobody joined: the prompt goes, and the
    /// event stays prompted (it never comes back); a declined one is forgotten, so the next call
    /// prompts. A prompt with a live call is untouched.
    pub fn retire(&mut self, event_id: &str) -> Vec<Action> {
        if self.declined_event.as_deref() == Some(event_id) {
            self.declined_event = None;
        }
        if self.unjoined_calendar_prompt() == Some(event_id) {
            self.phase = Phase::Idle;
            return vec![Action::HidePrompt];
        }
        vec![]
    }

    /// The calendar no longer has this event (canceled), or moved it: a prompt for it that
    /// nobody has joined is withdrawn, and a moved meeting may prompt again at its new time.
    pub fn withdraw(&mut self, event_id: &str) -> Vec<Action> {
        self.prompted_events.remove(event_id);
        if let Phase::Prompting { call: None, scheduled: Some(s) } = &self.phase {
            if s.event_id == event_id {
                self.phase = Phase::Idle;
                return vec![Action::HidePrompt];
            }
        }
        vec![]
    }

    pub fn input(&mut self, input: Input) -> Vec<Action> {
        match (input, &self.phase) {
            (Input::TakeNotes | Input::ManualStart, Phase::Prompting { call, scheduled }) => {
                // Starting from Settings while a prompt (or its bar) is up starts THAT recording
                // (round 25.1: "removes the bar and starts the recording").
                let (title, event_id) = match scheduled {
                    Some(s) => (Some(s.title.clone()), Some(s.event_id.clone())),
                    None => (None, None),
                };
                let start = Start { title, event_id, manual: false };
                let action = start.action();
                self.phase = Phase::Recording { call: call.clone(), start };
                vec![Action::HidePrompt, action]
            }
            (Input::NotThisOne, Phase::Prompting { call, .. } | Phase::Failed { call, .. }) => {
                // Declined before anyone joined: remember the meeting, so its call does not prompt.
                if let Phase::Prompting { call: None, scheduled: Some(s) } = &self.phase {
                    self.declined_event = Some(s.event_id.clone());
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
            (Input::Stop, Phase::Recording { call, .. }) => {
                // Stopped early: this call does not prompt again until it ends.
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
    fn input_returning_inside_the_grace_interval_does_not_split_the_call() {
        let mut m = MeetingMonitor::new();
        m.watch(&started(10, ZOOM, "zoom.us"), 0);
        m.input(Input::TakeNotes);
        m.watch(&stopped(10, ZOOM), 5_000);
        m.watch(&started(10, ZOOM, "zoom.us"), 5_000 + END_GRACE_MS - 100);
        assert_eq!(m.tick(5_000 + 3 * END_GRACE_MS), vec![]);
        assert!(m.is_recording());
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
        // The same call's input flickering does not prompt again.
        m.watch(&stopped(10, ZOOM), 2_000);
        assert_eq!(m.watch(&started(10, ZOOM, "zoom.us"), 3_000), vec![]);
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
        let s = Scheduled { event_id: "e1".into(), title: "Acme pricing review".into(), starts_at: 60_000 };
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
        let out = m.scheduled(Scheduled { event_id: "e1".into(), title: "Board".into(), starts_at: 60_000 });
        assert!(matches!(&out[..], [Action::ShowPrompt(PromptKind::Scheduled { title, .. })] if title == "Board"));
        assert!(matches!(&m.input(Input::TakeNotes)[1], Action::StartCapture { title: Some(t), .. } if t == "Board"));
    }

    #[test]
    fn starting_before_the_scheduled_call_does_not_finalize_before_it_begins() {
        let mut m = MeetingMonitor::new();
        m.scheduled(Scheduled { event_id: "e1".into(), title: "Board".into(), starts_at: 60_000 });
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
        c.scheduled(Scheduled { event_id: "e1".into(), title: "Acme pricing review".into(), starts_at: 0 });
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
        assert_eq!(p.scheduled(Scheduled { event_id: "e1".into(), title: "Standup".into(), starts_at: 0 }), vec![]);
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
        let missed = Scheduled { event_id: "old".into(), title: "Missed meeting".into(), starts_at: 1_000 };
        let next = Scheduled { event_id: "new".into(), title: "Pricing review".into(), starts_at: 3_600_000 };
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
        c.scheduled(Scheduled { event_id: "a".into(), title: "A".into(), starts_at: 1_000 });
        c.watch(&started(10, ZOOM, "zoom.us"), 2_000);
        assert_eq!(c.scheduled(Scheduled { event_id: "b".into(), title: "B".into(), starts_at: 9_000 }), vec![]);
        assert_eq!(c.retire("a"), vec![]);
        assert!(c.is_prompting());
    }

    #[test]
    fn a_calendar_meeting_declined_before_joining_does_not_prompt_when_its_call_starts() {
        // Second review rv-20261013T120455Z-516bb135-67f2 #1: closing the pre-call prompt, then
        // joining, prompted again for the same meeting.
        let meeting = Scheduled { event_id: "e1".into(), title: "Standup".into(), starts_at: 60_000 };
        let mut m = MeetingMonitor::new();
        m.scheduled(meeting);
        assert_eq!(m.input(Input::NotThisOne), vec![Action::HidePrompt]);
        assert_eq!(m.pending_calendar_event(), Some("e1"));
        assert_eq!(m.watch(&started(10, ZOOM, "zoom.us"), 70_000), vec![], "its call does not prompt");
        // Held as declined until it ends; the next call prompts.
        m.watch(&stopped(10, ZOOM), 90_000);
        m.tick(90_000 + END_GRACE_MS);
        assert_eq!(m.watch(&started(11, ZOOM, "zoom.us"), 200_000), zoom_prompt());
        // Never joined: once the meeting is over the app retires it, and a later call prompts.
        let mut r = MeetingMonitor::new();
        r.scheduled(Scheduled { event_id: "e2".into(), title: "Sync".into(), starts_at: 60_000 });
        r.input(Input::NotThisOne);
        assert_eq!(r.retire("e2"), vec![]);
        assert_eq!(r.pending_calendar_event(), None);
        assert_eq!(r.watch(&started(10, ZOOM, "zoom.us"), 9_000_000), zoom_prompt());
    }

    #[test]
    fn a_call_that_starts_while_a_pre_call_capture_is_failed_is_followed_by_the_retry() {
        // Second review rv-20261013T120455Z-516bb135-67f2 #2: the retry stayed unarmed, so
        // hang-up never stopped it.
        let mut m = MeetingMonitor::new();
        m.scheduled(Scheduled { event_id: "e1".into(), title: "Standup".into(), starts_at: 60_000 });
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
        assert_eq!(m.tick(5_000 + END_GRACE_MS), vec![Action::StopCapture(EndReason::CallEnded)]);
    }

    #[test]
    fn paused_prompts_show_nothing_and_leave_a_recording_alone() {
        let mut m = MeetingMonitor::new();
        m.set_paused(true);
        assert_eq!(m.watch(&started(10, ZOOM, "zoom.us"), 0), vec![]);
        assert_eq!(m.scheduled(Scheduled { event_id: "e1".into(), title: "x".into(), starts_at: 0 }), vec![]);
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
        let s = Scheduled { event_id: "e1".into(), title: "x".into(), starts_at: 60_000 };
        m.scheduled(s.clone());
        assert_eq!(m.withdraw("e1"), vec![Action::HidePrompt]);
        assert!(!m.is_prompting());
        assert_eq!(m.scheduled(s).len(), 1, "moved to a later time: it prompts at the new time");
    }
}

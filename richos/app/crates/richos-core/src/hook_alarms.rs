//! THE ENGINE'S ALARMS, READ OFF A LEASE'S STREAM (written for the operator back-end spec r2
//! (r), unchanged in r3 §11 item 6, Frank's B4; moved here from `operator_frames.rs` when the
//! operator path was retired, because every user's app runs the engine's full checks and their
//! alarms must reach the user as work notices: ruling §138).
//!
//! The engine speaks to the PERSON through a hook's `{"systemMessage": …}`: the escalation
//! ledger, the in-flight acknowledgements, the CPU guard, the engine's own ACTIVE/BROKEN banner
//! (`engine-status.sh`'s header measured which channel reaches a person, on 2.1.270). In a
//! terminal Claude Code renders those lines on screen. Behind the app nobody sees that screen,
//! so a lease started with `--include-hook-events` carries them on its stream, and this file
//! reads the two frames that carry them:
//!
//! - `{"type":"system","subtype":"hook_response", "hook_event", "hook_name", "stdout",
//!   "stderr", "output", "outcome", …}` — the hook's own stdout, in which a `systemMessage` is
//!   JSON. Shape from the 2.1.282 binary's stream schema; P10 confirms it on the wire.
//! - `{"type":"system","subtype":"informational","content","level"}` — the channel
//!   `scripts/lib/stop-hook-notice.sh:20-35` measured a Stop hook's message arriving on
//!   (2.1.251), as `"Stop says: <message>"`.
//!
//! **Only `systemMessage` is an alarm.** Plain stdout and stderr go to the model or to nobody
//! in a terminal (engine-status.sh's table), so treating them as alarms here would put in
//! front of the user what a terminal never shows. That is P10's control, as a test.
//!
//! **Verbatim.** The text is delivered as the engine wrote it. Nothing is summarized, because
//! a summarized alarm is a second author's opinion of an alarm.
//!
//! **Once, across every lease** ([`AlarmDeduper`]). Five leases each run the `Stop` hooks, so
//! one escalation would otherwise reach the user five times. The key is the message's own text,
//! with the platform's `"<Hook> says: "` framing removed so the same message arriving on both
//! frames counts once. A LOUDER repeat is different text (the ledger escalates at 1 h, 24 h
//! and 72 h), so it is a new alarm by construction. An identical text is delivered again
//! only after [`ALARM_PERIOD`].
use serde_json::Value;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How long an identical alarm is held as already delivered. The spec says "within one alarm
/// period" and names none; one hour is the escalation ledger's first step, so an alarm the user
/// was told about is repeated no more often than the ledger itself grows louder.
pub const ALARM_PERIOD: Duration = Duration::from_secs(60 * 60);

/// Which frame an alarm came from. Recorded so P10's result can be read against it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlarmFrame {
    /// `system/hook_response`, the hook's JSON stdout.
    HookResponse,
    /// `system/informational`.
    Informational,
}

/// One message the engine addressed to the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alarm {
    /// Exactly as the engine wrote it (or as the platform framed it, for `informational`).
    pub text: String,
    pub frame: AlarmFrame,
    /// `SessionStart`, `Stop`, … when the frame says.
    pub hook_event: Option<String>,
    pub hook_name: Option<String>,
    /// `info`, `notice`, `suggestion`, `warning` on an `informational` frame.
    pub level: Option<String>,
}

/// Every alarm one stream frame carries. Most frames carry none.
pub fn alarms_in(frame: &Value) -> Vec<Alarm> {
    if frame.get("type").and_then(Value::as_str) != Some("system") {
        return Vec::new();
    }
    let field = |name: &str| frame.get(name).and_then(Value::as_str).map(str::to_string);
    match frame.get("subtype").and_then(Value::as_str) {
        Some("hook_response") => {
            let stdout = frame.get("stdout").and_then(Value::as_str).unwrap_or("");
            let mut texts: Vec<String> = Vec::new();
            for text in system_messages(stdout) {
                if !texts.contains(&text) {
                    texts.push(text);
                }
            }
            texts.into_iter().map(|text| Alarm {
                text,
                frame: AlarmFrame::HookResponse,
                hook_event: field("hook_event"),
                hook_name: field("hook_name"),
                level: None,
            }).collect()
        }
        Some("informational") => match field("content") {
            Some(text) if !text.trim().is_empty() => vec![Alarm {
                text,
                frame: AlarmFrame::Informational,
                hook_event: None,
                hook_name: None,
                level: field("level"),
            }],
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// The non-empty `systemMessage` strings in one hook's stdout: the whole of it as one JSON
/// object (which is how Claude Code reads it, pretty-printed included), else each line that is
/// one. Plain text is never a message.
fn system_messages(stdout: &str) -> Vec<String> {
    let message = |value: Value| value.get("systemMessage").and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty()).map(str::to_string);
    if let Ok(value) = serde_json::from_str::<Value>(stdout.trim()) {
        return message(value).into_iter().collect();
    }
    stdout.lines().map(str::trim).filter(|line| line.starts_with('{'))
        .filter_map(|line| serde_json::from_str::<Value>(line).ok()).filter_map(message).collect()
}

/// Hook events whose name the platform puts in front of a message (`"Stop says: …"`).
const HOOK_EVENTS: [&str; 15] = [
    "SessionStart", "SessionEnd", "UserPromptSubmit", "PreToolUse", "PostToolUse", "PostToolUseFailure",
    "Stop", "StopFailure", "SubagentStart", "SubagentStop", "Notification", "PreCompact", "PostCompact",
    "TeammateIdle", "TaskCompleted",
];

/// The key two deliveries of one message share. The platform's `"<Hook> says: "` framing is
/// removed, and surrounding whitespace, and nothing else.
pub fn dedupe_key(text: &str) -> String {
    let text = text.trim();
    if let Some((speaker, rest)) = text.split_once(" says: ") {
        let event = speaker.split(':').next().unwrap_or(speaker);
        if !speaker.contains(char::is_whitespace) && HOOK_EVENTS.contains(&event) {
            return rest.trim().to_string();
        }
    }
    text.to_string()
}

/// Delivered-once bookkeeping, shared by every lease of the app.
#[derive(Debug)]
pub struct AlarmDeduper {
    period: Duration,
    delivered: HashMap<String, Instant>,
}

impl Default for AlarmDeduper {
    fn default() -> Self {
        AlarmDeduper::new(ALARM_PERIOD)
    }
}

impl AlarmDeduper {
    pub fn new(period: Duration) -> Self {
        AlarmDeduper { period, delivered: HashMap::new() }
    }

    /// Should this alarm be delivered now? `true` records it as delivered at `now`.
    pub fn admit(&mut self, alarm: &Alarm, now: Instant) -> bool {
        let period = self.period;
        self.delivered.retain(|_, at| now.saturating_duration_since(*at) < period);
        let key = dedupe_key(&alarm.text);
        if self.delivered.contains_key(&key) {
            return false;
        }
        self.delivered.insert(key, now);
        true
    }

    /// How many distinct alarms are being remembered. Bounded by what arrived in one period.
    pub fn remembered(&self) -> usize {
        self.delivered.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn hook_response(event: &str, stdout: &str, stderr: &str) -> Value {
        json!({"type":"system","subtype":"hook_response","hook_id":"h-1","hook_name":event,
               "hook_event":event,"output":format!("{stdout}{stderr}"),"stdout":stdout,"stderr":stderr,
               "exit_code":0,"outcome":"success","uuid":"u-1","session_id":"s-1"})
    }

    fn informational(content: &str, level: &str) -> Value {
        json!({"type":"system","subtype":"informational","content":content,"level":level,
               "uuid":"u-2","session_id":"s-1"})
    }

    #[test]
    fn a_system_message_in_a_hook_s_stdout_is_an_alarm_verbatim() {
        let text = "ESCALATION esc-1 is 24 h old and nobody has acknowledged it.";
        for event in ["SessionStart", "PreToolUse", "Stop"] {
            let frame = hook_response(event, &format!("{{\"systemMessage\":{}}}\n", json!(text)), "");
            let alarms = alarms_in(&frame);
            assert_eq!(alarms.len(), 1, "{event}: {alarms:?}");
            assert_eq!(alarms[0].text, text);
            assert_eq!(alarms[0].frame, AlarmFrame::HookResponse);
            assert_eq!(alarms[0].hook_event.as_deref(), Some(event));
        }
    }

    #[test]
    fn a_message_beside_other_keys_or_after_other_lines_is_still_read() {
        let stdout = "{\"hookSpecificOutput\":{\"hookEventName\":\"SessionStart\",\"additionalContext\":\"for the model\"},\"systemMessage\":\"for him\"}";
        assert_eq!(alarms_in(&hook_response("SessionStart", stdout, ""))[0].text, "for him");
        let stdout = "some plain line\n{\"systemMessage\":\"second line\"}\n";
        assert_eq!(alarms_in(&hook_response("Stop", stdout, ""))[0].text, "second line");
        let pretty = "{\n  \"systemMessage\": \"pretty\"\n}\n";
        assert_eq!(alarms_in(&hook_response("Stop", pretty, ""))[0].text, "pretty");
    }

    /// P10's control, as a unit: what a terminal never shows is not an alarm here.
    #[test]
    fn plain_stdout_stderr_and_additional_context_are_not_alarms() {
        assert!(alarms_in(&hook_response("Stop", "just text for the model\n", "")).is_empty());
        assert!(alarms_in(&hook_response("Stop", "", "{\"systemMessage\":\"on stderr\"}")).is_empty());
        assert!(alarms_in(&hook_response("SessionStart",
            "{\"hookSpecificOutput\":{\"hookEventName\":\"SessionStart\",\"additionalContext\":\"model only\"}}", "")).is_empty());
        assert!(alarms_in(&hook_response("Stop", "{\"systemMessage\":\"\"}", "")).is_empty());
        assert!(alarms_in(&hook_response("Stop", "{\"systemMessage\":42}", "")).is_empty());
    }

    #[test]
    fn an_informational_frame_is_an_alarm_with_its_level() {
        let alarms = alarms_in(&informational("Stop says: CPU GUARD ALERT", "notice"));
        assert_eq!(alarms.len(), 1);
        assert_eq!(alarms[0].text, "Stop says: CPU GUARD ALERT", "verbatim, framing included");
        assert_eq!(alarms[0].frame, AlarmFrame::Informational);
        assert_eq!(alarms[0].level.as_deref(), Some("notice"));
        assert!(alarms_in(&informational("", "info")).is_empty());
    }

    #[test]
    fn other_frames_carry_no_alarm() {
        for frame in [json!({"type":"system","subtype":"init","tools":[]}),
                      json!({"type":"system","subtype":"hook_started","hook_event":"Stop","hook_name":"Stop"}),
                      json!({"type":"assistant","message":{"content":[{"type":"text","text":"{\"systemMessage\":\"no\"}"}]}}),
                      json!({"type":"result","result":"done"}),
                      json!("not an object")] {
            assert!(alarms_in(&frame).is_empty(), "{frame}");
        }
    }

    #[test]
    fn the_same_message_on_both_frames_and_from_five_leads_is_delivered_once() {
        let mut deduper = AlarmDeduper::default();
        let now = Instant::now();
        let from_hook = &alarms_in(&hook_response("Stop", "{\"systemMessage\":\"esc-1 is waiting\"}", ""))[0];
        let from_info = &alarms_in(&informational("Stop says: esc-1 is waiting", "notice"))[0];
        assert!(deduper.admit(from_hook, now));
        assert!(!deduper.admit(from_info, now), "the platform's framing must not make it new");
        for lead in 0..5 {
            assert!(!deduper.admit(from_hook, now + Duration::from_secs(lead)), "lead {lead} repeated it");
        }
        assert_eq!(deduper.remembered(), 1);
    }

    #[test]
    fn a_louder_repeat_is_new_and_an_identical_one_returns_after_the_period() {
        let mut deduper = AlarmDeduper::new(Duration::from_secs(60));
        let t0 = Instant::now();
        let alarm = |text: &str| alarms_in(&informational(text, "warning")).remove(0);
        assert!(deduper.admit(&alarm("esc-1 unacknowledged for 1 h"), t0));
        assert!(deduper.admit(&alarm("esc-1 unacknowledged for 24 h"), t0 + Duration::from_secs(1)));
        assert!(!deduper.admit(&alarm("esc-1 unacknowledged for 1 h"), t0 + Duration::from_secs(59)));
        assert!(deduper.admit(&alarm("esc-1 unacknowledged for 1 h"), t0 + Duration::from_secs(61)));
        // What has aged out is forgotten, so the memory is bounded by one period's alarms.
        assert!(deduper.admit(&alarm("something new"), t0 + Duration::from_secs(200)));
        assert_eq!(deduper.remembered(), 1);
    }

    #[test]
    fn the_key_removes_only_the_platform_s_framing() {
        assert_eq!(dedupe_key("Stop says: esc-1"), "esc-1");
        assert_eq!(dedupe_key("  SessionStart says: banner  "), "banner");
        assert_eq!(dedupe_key("Rich says: keep this"), "Rich says: keep this",
            "only a hook event's framing is removed, never the message's own words");
        assert_eq!(dedupe_key("plain"), "plain");
    }
}

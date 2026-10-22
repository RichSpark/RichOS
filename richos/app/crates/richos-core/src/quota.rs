//! Claude subscription windows and an opt-in background-turn admission policy.
//! Only normalized quota data leaves the reader. No account identity or billing data is retained.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::Mutex,
};

pub mod gate;
pub mod holds;
pub mod probe;
pub mod resets;
pub mod terminal;
pub mod tokens;
pub mod reset_transport;
pub mod reset_tools;

pub const REFRESH_INTERVAL_MS: u64 = 5 * 60_000;

/// **When usage speeds up a lot, or a big rise is expected, the quota is checked every MINUTE
/// instead of 5** (ruling §108: "every 2 minutes"; plan §15 answer 10: "use every minute
/// instead if that causes no problems", after measuring what one reading costs).
///
/// **The measurement, 2026-10-04, this Mac, Claude Code 2.1.288**, with
/// `cargo run -p richos-core --example claude_quota -- --measure <claude> N GAP`: a cold read
/// (spawn, initialize, `get_usage`) took 2.49-2.92 s; a warm read on the kept connection
/// 2.02-2.22 s; the child's CPU time grew 2.03-2.04 s per read with reads 6 s apart and
/// 2.29 s per read with reads 30 s apart — so the cost is the READ, not an idle child: about
/// 2.2 s of one core per read. The resident child is 390-430 MB. At one read a minute that is
/// about 3.7% of one core per account being read fast (2.2 / 60), only while usage is fast or
/// a rise is expected, and only for the account in use; at 2 minutes it would be 1.8%. Every
/// minute lets the act point sit closer to the normal one at the same speed (4 points a minute
/// projects 4 points ahead instead of 8), so it is the choice. The cost no measurement here can
/// show is Anthropic's side (a `get_usage` a minute per account); a failed read backs off
/// `BACKOFF_MS` as before.
pub const FAST_REFRESH_INTERVAL_MS: u64 = 60_000;

/// **An EXPECTED rise** (plan §15 answer 10): this many leases started, or agents dispatched,
/// within `EXPECTED_RISE_WINDOW_MS` switches checking to the fast interval before any jump is
/// measured, for `EXPECTED_RISE_MS`. Then it ends by itself unless a measured speed keeps it.
pub const EXPECTED_RISE_STARTS: usize = 3;
pub const EXPECTED_RISE_WINDOW_MS: u64 = 60_000;
pub const EXPECTED_RISE_MS: u64 = 10 * 60_000;

/// **"Speeds up a lot", as a number:** a window gaining 5 percentage points or more per
/// five-minute check, i.e. 1 point a minute. That is 3x the even pace of a five-hour window
/// (100 points / 300 minutes = 0.33 a minute) and it is the speed at which one ordinary check
/// interval eats most of the 7 points between the 93% rule and the wall. The episode §108
/// names went 93 -> 100 inside one check: at least 7 points in 5 minutes = 1.4 a minute.
pub const FAST_POINTS_PER_CHECK: f64 = 5.0;

/// The shortest gap two readings of one window must have before a speed is taken from them.
/// Turn-by-turn streamed readings can be seconds apart, and a speed from a 2-second gap is noise.
const SPEED_MIN_GAP_MS: u64 = 30_000;

/// One account's freshest reading: its windows and each window's measured speed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reading {
    pub windows: Vec<Window>,
    /// Window id -> percentage points per millisecond, from the last two readings of that
    /// same window (same reset time). Absent until two readings far enough apart exist.
    pub speeds: BTreeMap<String, f64>,
    /// A big rise is expected (many leases or agents started at once): check fast already.
    pub expected: bool,
    /// Window id -> the rise each speed above was measured over (round 16's "from 40% to 71%
    /// in 12 minutes", said with the app's own two readings).
    pub rises: BTreeMap<String, Rise>,
    /// **The test cut-off** for this account (`RICHOS_TEST_WEEKLY_CUTOFF`, weekly-switch plan
    /// §4): it replaces 99 as the weekly point at normal speed. `None` everywhere but the one
    /// account the variable names.
    pub weekly_cutoff: Option<f64>,
    /// **This account's last measured handoff** (`Service::track_tokens`, from the gate's
    /// markers): the longest time from the order to a helper's handoff commit, and the weekly
    /// points the account spent in it. `None` until one is measured: then
    /// [`HANDOFF_DEFAULT_MS`] and no points.
    pub handoff: Option<(u64, f64)>,
}

/// **H until a handoff is measured: 5 minutes** (the CEO, 2026-10-07: *"I would hope that when
/// the teammates get their handoff message, they'd get to a full finish within a few minutes.
/// Hopefully no more than 3-5 minutes or so."*). The 10-minute guess it replaces was never
/// measured. Once an account has a measured handoff, that is its H (`Reading::handoff`).
pub const HANDOFF_DEFAULT_MS: u64 = 5 * 60_000;

/// The orders of one handoff reach each helper at its next tool call, so within minutes of
/// each other; an account's next leaving is days later. The markers ordered within this span
/// of an account's newest one are its last handoff.
pub const HANDOFF_BATCH_MS: u64 = 3_600_000;

/// `RICHOS_TEST_WEEKLY_CUTOFF=<account id>:<percent>` (weekly-switch plan §4): the weekly
/// point of that one account is `<percent>` instead of 99. No UI; read once at launch. Only
/// one account, because a cut-off on every account would leave nowhere to switch to.
///
/// It acts only inside the app's own quota service (its published `leaving` and its switch).
/// It never reaches the separate-process gate's own recomputation: `configure` removes every
/// inherited `RICHOS_*` from the provider and so from its hooks (`engine_profile.rs`), and the
/// gate reads the published `leaving` instead (Frank's review of the plan, minor 4).
pub const TEST_WEEKLY_CUTOFF_ENV: &str = "RICHOS_TEST_WEEKLY_CUTOFF";

/// `<account id>:<percent>`, the percent from 0 to 100. Anything else is no cut-off.
pub fn parse_weekly_cutoff(text: &str) -> Option<(String, f64)> {
    let (id, percent) = text.trim().rsplit_once(':')?;
    let (id, percent) = (id.trim(), percent.trim().parse::<f64>().ok()?);
    (!id.is_empty() && percent.is_finite() && (0. ..=100.).contains(&percent)).then(|| (id.to_string(), percent))
}

/// The test cut-off this process was launched with, read once.
pub fn test_weekly_cutoff() -> Option<(String, f64)> {
    static CUTOFF: std::sync::OnceLock<Option<(String, f64)>> = std::sync::OnceLock::new();
    CUTOFF.get_or_init(|| std::env::var(TEST_WEEKLY_CUTOFF_ENV).ok().and_then(|t| parse_weekly_cutoff(&t))).clone()
}
/// The two readings a window's speed was measured from: used percent then and now, and the
/// time between them.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rise { pub from: f64, pub to: f64, pub ms: u64 }
impl Reading {
    /// Is usage measured fast enough to check every minute (§108)?
    pub fn fast(&self) -> bool {
        self.speeds.values().any(|s| s * REFRESH_INTERVAL_MS as f64 >= FAST_POINTS_PER_CHECK)
    }
    /// The gap to the next check: one minute when fast or a rise is expected, 5 otherwise.
    /// When the speed comes back down (and no rise is expected) it is 5 minutes again.
    pub fn interval(&self) -> u64 {
        if self.fast() || self.expected { FAST_REFRESH_INTERVAL_MS } else { REFRESH_INTERVAL_MS }
    }
    /// What this window will read at the next check, at its measured speed.
    pub fn projected(&self, window: &Window) -> f64 {
        window.used_percent + self.speeds.get(&window.id).copied().unwrap_or(0.) * self.interval() as f64
    }
    /// **The check point, recalculated from the measured speed** (plan §15 answer 10): the
    /// normal threshold (93% five-hour, 99% weekly), or lower when the speed would carry the
    /// window to 100% before the next check — `100 - speed x interval`. At normal speed it is
    /// the normal threshold again (answer 11).
    pub fn act_point(&self, window: &Window, threshold: f64) -> f64 {
        let reach = self.speeds.get(&window.id).copied().unwrap_or(0.) * self.interval() as f64;
        // Never below 0%: a jump faster than the whole window (20% -> 95% in 30 s) would
        // otherwise print "-36%". At 0 the window is already at the point, so it still acts at once.
        threshold.min(100. - reach).clamp(0., 100.)
    }
    /// **Act now?** At the check point (§108: never at 100%).
    pub fn reaches(&self, window: &Window, threshold: f64) -> bool {
        window.used_percent >= self.act_point(window, threshold)
    }
    /// **The weekly handoff point** (weekly-switch plan §1; his words, 2026-10-07: *"the
    /// percentage for the switch needs to be adjusted dynamically so that this doesn't happen.
    /// My "1-2% sooner" was just guesswork."*, "this" being 100% before the switch):
    ///
    /// `point = 99 - max(0, ceil(s x I + max(s x H, P)) - 1)`
    ///
    /// with `s` the window's measured speed (the larger of the percentage speed and the token
    /// speed), `I` the time to the next check (`interval`), `H` the time a handoff takes and `P`
    /// the points the last handoff on this account spent (`handoff`: measured from the order to
    /// the handoff commit, 5 minutes and no points until one is measured). If the wait for the
    /// next check and the handoff would use 1 point or less, it is 99; up to 2 points, 98; up to
    /// 5, 95; and so on, as early as the speed requires, with no floor but 0. So the point plus
    /// what the wait and the handoff spend is at or under 100 at any speed. Every weekly
    /// decision uses it, so the account switch and the order to every teammate on the account
    /// fire at the same moment. 99 is the test cut-off instead, on the one account it names
    /// (`weekly_cutoff`).
    pub fn weekly_point(&self, window: &Window) -> f64 {
        let base = self.weekly_cutoff.unwrap_or(resets::WEEKLY_THRESHOLD);
        let speed = self.speeds.get(&window.id).copied().unwrap_or(0.);
        let (handoff_ms, handoff_points) = self.handoff.unwrap_or((HANDOFF_DEFAULT_MS, 0.));
        let points = speed * self.interval() as f64 + (speed * handoff_ms as f64).max(handoff_points);        let earlier = (points.ceil() - 1.).max(0.);
        (base - earlier).clamp(0., 100.)
    }
    /// Has this weekly window reached its handoff point?
    pub fn reaches_weekly(&self, window: &Window) -> bool {
        window.used_percent >= self.weekly_point(window)
    }
    /// **Is this account being left for its week?** (`View::leaving`.) Its weekly window has
    /// reached its point, and that window has not reset since: a reset that has passed is a
    /// fresh week, exactly as `claude_accounts::gone` reads it.
    pub fn leaving(&self, now: u64) -> bool {
        self.windows.iter().any(|w| w.id == "seven_day" && self.reaches_weekly(w)
            && w.resets_at.is_none_or(|t| t > now))
    }
}
pub const RESET_EXEMPTION_MS: u64 = 20 * 60_000;

/// A span as Rich says it: "41 min", "1 h 41 min", "2 d 3 h" (round 16's way of writing one).
pub fn words(ms: u64) -> String {
    let minutes = ms.div_ceil(60_000).max(1);
    let (hours, rest) = (minutes / 60, minutes % 60);
    match (hours, rest) {
        (0, m) => format!("{m} min"),
        (h, 0) if h < 24 => format!("{h} h"),
        (h, m) if h < 24 => format!("{h} h {m} min"),
        (h, _) if h % 24 == 0 => format!("{} d", h / 24),
        (h, _) => format!("{} d {} h", h / 24, h % 24),
    }
}
/// A moment as Rich says it, at the webview's offset: "2:17 AM" today, "Wed 12:20 AM" on any
/// other day (round 16's `fmtWhen`). Rounded to the nearest minute, as the panel rounds it.
pub fn clock(at_ms: u64, utc_offset_minutes: i32, now_ms: u64) -> String {
    let local = |ms: u64| {
        let minute = (ms + 30_000) / 60_000 * 60;
        let offset = time::UtcOffset::from_whole_seconds(utc_offset_minutes.clamp(-1439, 1439) * 60)
            .unwrap_or(time::UtcOffset::UTC);
        time::OffsetDateTime::from_unix_timestamp(i64::try_from(minute).unwrap_or(i64::MAX))
            .unwrap_or(time::OffsetDateTime::UNIX_EPOCH).to_offset(offset)
    };
    let (at, now) = (local(at_ms), local(now_ms));
    let hour = match at.hour() % 12 { 0 => 12, h => h };
    let time = format!("{hour}:{:02} {}", at.minute(), if at.hour() < 12 { "AM" } else { "PM" });
    if at.date() == now.date() { time } else {
        let day = at.weekday().to_string();
        format!("{} {time}", &day[..3])
    }
}
/// A moment as round 18's lines say it: "today at 2:10 PM", "tomorrow at 9:00 AM", "on
/// Thursday at 9:00 AM" (the mockup's `whenLong`), at the webview's offset. Before the webview
/// has said its offset, the span: "in 2 d 3 h".
pub fn when_long(at_ms: u64, utc_offset_minutes: Option<i32>, now_ms: u64) -> String {
    let Some(offset) = utc_offset_minutes else { return format!("in {}", words(at_ms.saturating_sub(now_ms))) };
    let offset = time::UtcOffset::from_whole_seconds(offset.clamp(-1439, 1439) * 60).unwrap_or(time::UtcOffset::UTC);
    let local = |ms: u64| time::OffsetDateTime::from_unix_timestamp(i64::try_from((ms + 30_000) / 60_000 * 60).unwrap_or(i64::MAX))
        .unwrap_or(time::OffsetDateTime::UNIX_EPOCH).to_offset(offset);
    let (at, now) = (local(at_ms), local(now_ms));
    let hour = match at.hour() % 12 { 0 => 12, h => h };
    let time = format!("{hour}:{:02} {}", at.minute(), if at.hour() < 12 { "AM" } else { "PM" });
    if at.date() == now.date() { format!("today at {time}") }
    else if now.date().next_day() == Some(at.date()) { format!("tomorrow at {time}") }
    else { format!("on {} at {time}", at.weekday()) }
}
pub const BACKOFF_MS: u64 = 600_000;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    pub id: String,
    pub label: String,
    pub used_percent: f64,
    pub resets_at: Option<u64>,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Policy {
    pub enabled: bool,
    pub pause_percent: u8,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            enabled: false,
            pause_percent: 93,
        }
    }
}
impl Policy {
    /// The five-hour line while the automatic pause-or-switch is on; `None` while it is off
    /// (`claude_accounts::gone`: off, nothing happens at the line).
    pub fn line(&self) -> Option<u8> {
        self.enabled.then_some(self.pause_percent)
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if !(1..=99).contains(&self.pause_percent) {
            return Err("Choose a pause threshold from 1% to 99% used.");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum State {
    Fresh,
    Stale,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "state"
)]
pub enum Admission {
    Disabled,
    Ready,
    Held { resets_at: u64 },
    Unknown,
    /// **The last check came back with no figures** (`View::empty_at`): Claude Code answered
    /// `rate_limits: null`, or the read failed, or its answer was unreadable or unsupported,
    /// and no reading has arrived since. The pause acts on a number, and there is none, so it
    /// does not hold work on it (walk of nightly 36, D1: Claude Code 2.1.289 answers null some
    /// of the time, and the old `Unknown` held every new background step until a reading came,
    /// for as long as Claude Code kept answering null). A reading that proves a hold still
    /// holds: `Held` is decided before this, from the last figures, until their window resets.
    NoReading,
}
impl Admission {
    pub fn allows_work(&self) -> bool {
        // Missing telemetry is not evidence that an account reached its limit.
        // The provider still enforces actual limits; only an observed hold waits.
        !matches!(self, Self::Held { .. })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    #[serde(default)]
    pub resets: resets::View,
    pub state: State,
    pub windows: Vec<Window>,
    pub checked_at: Option<u64>,
    pub retry_at: Option<u64>,
    pub next_check_at: Option<u64>,
    pub refresh_interval_ms: u64,
    pub message: Option<String>,
    /// **Is this account signed in on this Mac?** `Some(false)` when Claude Code's own
    /// `auth status` says nobody is ([`ReadError::SignedOut`]), `Some(true)` once a reading has
    /// arrived (only a signed-in account reports one) or `auth status` says it is signed in
    /// ([`ReadError::SignedInNoLimits`]), `None` while nothing says either. The
    /// account card draws "Not signed in" with the sign-in from `Some(false)`, never "No
    /// reading yet" (`ui/accounts.js`).
    #[serde(default)]
    pub signed_in: Option<bool>,
    pub policy: Policy,
    pub admission: Admission,
    /// **Fill-first** (`claude_accounts.rs`). Every field below is additive and defaulted, so
    /// this file stays readable by its existing readers (`quota/gate.rs`, the desktop shell,
    /// `scripts/claude-quota.test.sh`), and every field above describes the account IN USE.
    /// One row per account; empty while there is only Account 1.
    #[serde(default)]
    pub accounts: Vec<crate::claude_accounts::AccountView>,
    /// Every account is exhausted: work is held until this moment, the soonest reset among
    /// them. The gate honors it (`gate.rs`). `None` with one account.
    #[serde(default)]
    pub held_until: Option<u64>,
    /// The setting for the five-hour threshold: Pause (default) or Switch.
    #[serde(default)]
    pub at_threshold: crate::claude_accounts::AtThreshold,
    /// Each window's measured speed, percentage points per millisecond (§108). Published so
    /// the separate-process gate projects exactly as this process does; `refreshIntervalMs`
    /// above reads 120000 while it is fast.
    #[serde(default)]
    pub speeds: BTreeMap<String, f64>,
    /// The check point of each window, percent used: 93 (five-hour) and 99 (weekly) at
    /// normal speed, lower while a measured speed would otherwise reach 100% between checks.
    #[serde(default)]
    pub act_at: BTreeMap<String, f64>,
    /// The rise each speed above was measured over, so the sheet's fast card says round 16's
    /// "took Home's five-hour window from 40% to 71% in 12 minutes" with the same two readings
    /// the account settings view shows.
    #[serde(default)]
    pub rises: BTreeMap<String, Rise>,
    /// The agents working at the shell's last count (`set_agents_working`), for the same
    /// card's "15 agents reading at once". 0 when none is counted; the card then says the
    /// rise alone.
    #[serde(default)]
    pub agents_working: usize,
    /// The last switch between accounts (round 16's card after a switch). `None` with one
    /// account.
    #[serde(default)]
    pub last_switch: Option<crate::claude_accounts::LastSwitch>,
    /// When the last check came back with no figures (`Admission::NoReading`); `None` once a
    /// reading has arrived since. The sheet says "no reading this time" from it, and the next
    /// check is counted from it (`next_check_at`), so an empty answer is asked again on the
    /// normal schedule, never in a loop and never after the failure backoff.
    #[serde(default)]
    pub empty_at: Option<u64>,
    /// **Round 18, for everyone** (`ui/accounts.js`): Recent changes, oldest first.
    #[serde(default)]
    pub changes: Vec<crate::claude_accounts::Change>,
    /// Rich's one-time suggestion of a second account, once said, with its answer.
    #[serde(default)]
    pub nudge: Option<crate::claude_accounts::Said>,
    /// Rich's lines in the conversation that are about accounts, by turn.
    #[serde(default)]
    pub notes: Vec<crate::claude_accounts::NoteTurn>,
    /// The one account's name, when there is only one and it has been named (round 18 shows
    /// "Your Claude account" until it is). `None` with two or more: `accounts` names them.
    #[serde(default)]
    pub first_label: Option<String>,
    /// **Every account being left for its week** (weekly-switch plan §1): its weekly window
    /// has reached its handoff point (`Reading::weekly_point`). Published with one account
    /// too, unless that one account has automatic pause off. The gate orders every teammate
    /// running on one of these accounts to commit, hand off and end (`gate.rs`).
    #[serde(default)]
    pub leaving: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReadError {
    Unsupported,
    Failed,
    Malformed,
    /// Claude Code answered, with `rate_limits_available: true` and `rate_limits: null`: no
    /// figures this time (Claude Code 2.1.289, walk of nightly 36, `qa/usage-shape.sh`). Not a
    /// failure and not unreadable data: nothing is reported as broken, no backoff starts, and
    /// the next check comes on the normal schedule.
    NoReading,
    /// Claude Code reported no limits AND its own `claude auth status` says nobody is signed in
    /// to this account's folder (`probe.rs`). Permanent until somebody signs in, so the card
    /// says so and offers the sign-in, never "No reading yet".
    SignedOut,
    /// Claude Code reported no limits AND its own `claude auth status` says this account IS
    /// signed in (`probe.rs`): an account without subscription limits. The sign-in verdict
    /// stands on its own, so the card never offers "Sign in" to it.
    SignedInNoLimits,
}
impl ReadError {
    fn message(self) -> &'static str {
        match self {
            Self::Unsupported => "Claude Code did not report subscription limits for this account.",
            Self::SignedOut => "This Claude account is not signed in on this Mac.",
            Self::SignedInNoLimits => "This account is signed in on this Mac. Claude Code does not report usage limits for it, so there are none to show.",
            // Round 16's words for a refresh that got no answer (quota.html, `refresh-failed`).
            Self::Failed => "Claude Code did not answer just now.",
            Self::Malformed => "Claude Code returned quota data this version of RichOS could not read.",
            Self::NoReading => "Claude Code answered without its usage figures just now.",
        }
    }
}

fn reset(value: &Value) -> Option<u64> {
    let text = value.as_str()?;
    let parsed =
        time::OffsetDateTime::parse(text, &time::format_description::well_known::Rfc3339).ok()?;
    u64::try_from(parsed.unix_timestamp_nanos() / 1_000_000).ok()
}

/// Recognized account-wide windows plus provider-named model weeklies. Unknown
/// billing/experimental buckets are deliberately not guessed into quota rows.
pub fn normalize(response: &Value) -> Result<Vec<Window>, ReadError> {
    match response
        .get("rate_limits_available")
        .and_then(Value::as_bool)
    {
        Some(false) => return Err(ReadError::Unsupported),
        Some(true) => {}
        None => return Err(ReadError::Malformed),
    }
    // An explicit null is Claude Code saying it has no figures this time; anything else that
    // is not an object (absent, a string, a number) is still data this reader cannot read.
    if response.get("rate_limits").is_some_and(Value::is_null) {
        return Err(ReadError::NoReading);
    }
    let limits = response
        .get("rate_limits")
        .and_then(Value::as_object)
        .ok_or(ReadError::Malformed)?;
    let mut windows = Vec::new();
    let mut append = |id: String, label: String, duration_ms: u64, value: &Value| {
        if let Some(used) = value
            .get("utilization")
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite() && *v >= 0.0 && *v <= 100.0)
        {
            windows.push(Window {
                id,
                label,
                used_percent: used,
                resets_at: value.get("resets_at").and_then(reset),
                duration_ms,
            });
        }
    };
    for (id, label, hours) in [("five_hour", "Five-hour", 5), ("seven_day", "Weekly", 168)] {
        if let Some(value) = limits.get(id) {
            append(id.into(), label.into(), hours * 3_600_000, value);
        }
    }
    if let Some(models) = limits.get("model_scoped").and_then(Value::as_array) {
        for model in models.iter().take(32) {
            if let Some(name) = model
                .get("display_name")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
            {
                let name: String = name.chars().filter(|c| !c.is_control()).take(80).collect();
                append(
                    format!("model:{name}"),
                    format!("Weekly · {name}"),
                    168 * 3_600_000,
                    model,
                );
            }
        }
    }
    // Older Claude versions name model windows directly.
    if !windows.iter().any(|w| w.id.starts_with("model:")) {
        for (key, label) in [
            ("seven_day_opus", "Weekly · Opus"),
            ("seven_day_sonnet", "Weekly · Sonnet"),
        ] {
            if let Some(value) = limits.get(key) {
                if let Some(used) = value
                    .get("utilization")
                    .and_then(Value::as_f64)
                    .filter(|v| v.is_finite() && (0.0..=100.0).contains(v))
                {
                    windows.push(Window {
                        id: key.into(),
                        label: label.into(),
                        used_percent: used,
                        resets_at: value.get("resets_at").and_then(reset),
                        duration_ms: 168 * 3_600_000,
                    });
                }
            }
        }
    }
    if windows.is_empty() {
        Err(ReadError::Malformed)
    } else {
        Ok(windows)
    }
}

pub trait Source: Send {
    fn read(&mut self, bin: &Path, cwd: &Path) -> Result<Vec<Window>, ReadError>;
    fn disconnect(&mut self) {}
}

#[derive(Default)]
struct Snapshot {
    windows: Vec<Window>,
    checked_at: Option<u64>,
    retry_at: Option<u64>,
    error: Option<ReadError>,
    /// Each window's measured speed (§108), kept across readings of the same window.
    speeds: BTreeMap<String, f64>,
    /// Each window's speed base: the reading the next speed is measured from.
    bases: BTreeMap<String, SpeedBase>,
    /// The rise each speed was measured over (`Reading::rises`).
    rises: BTreeMap<String, Rise>,
    /// A big rise is expected until this moment (plan §15 answer 10). It ends by itself.
    rise_until: Option<u64>,
    /// The last check came back with no figures at this moment (`View::empty_at`). A reading
    /// (`accept`) clears it.
    empty_at: Option<u64>,
    /// The weekly speed from the last minute's token use, points per millisecond
    /// (`tokens::Track::speed`), set at every token scan (`Service::track_tokens`).
    token_speed: Option<f64>,
    /// Weekly points the counted tokens spent since `checked_at` (`tokens::Track::points_since`),
    /// set at every token scan. Used only while the reading is older than the normal check
    /// (`reading_with`).
    since_reading: f64,
    /// This account's last measured handoff (`Reading::handoff`), set at every token scan.
    handoff: Option<(u64, f64)>,
    /// Null answers in a row before this account's first reading (`FIRST_READING_RETRY_MS`).
    empties: u32,
    /// **What the last answer ABOUT the sign-in said** ([`View::signed_in`]), kept apart from
    /// the last check's error (mid-job review rv-20261021T233844Z-10df0b1f-1846, finding 1):
    /// while it lived only in that error, one check where Claude Code did not answer replaced
    /// "signed out" or "signed in without limits" with `Failed`, and the card offered Sign in
    /// to a signed-in account. Set by a reading (`Some(true)`), `SignedOut` (`Some(false)`)
    /// and `SignedInNoLimits` (`Some(true)`); every other outcome leaves it as it was. A
    /// changed account connection starts a new `Snapshot` and so forgets it (`disconnect`).
    signed_in: Option<bool>,
}

/// **An account with no reading yet is asked again soon after a null answer** (handoff round
/// 2, runs 2-4 and 6: null answers, the usage endpoint's 429 reported as `rate_limits: null`,
/// starved Work's first reading for the whole run, and an account never read is never switched
/// to). 30 seconds after the first null, doubling to `FIRST_READING_RETRY_MAX_MS` and staying
/// there until the first reading arrives: well under the 5-minute schedule, never more than
/// one ask in 30 seconds. With a reading in hand a null waits for the normal schedule.
pub const FIRST_READING_RETRY_MS: u64 = 30_000;
pub const FIRST_READING_RETRY_MAX_MS: u64 = 120_000;

/// One window's reading as a speed base: used percent, its reset time, when it was first read
/// (`at`) and last read (`last`) at that value, and whether its rise into that value was seen
/// (`rose`: a lower reading came just before it).
#[derive(Clone, Copy, Debug, PartialEq)]
struct SpeedBase { used: f64, resets_at: Option<u64>, at: u64, last: u64, rose: bool }

/// A value Claude reports in whole points.
fn whole(used: f64) -> bool { used.fract() == 0. }

impl Snapshot {
    /// Measure each window's speed against its base, a reading of the SAME window (same reset
    /// time) at least `SPEED_MIN_GAP_MS` earlier. A new window starts again with no speed.
    ///
    /// **The base is the FIRST reading of its value** (nightly 65, 2026-10-18): a reading that
    /// shows the same value again leaves it where it is. Claude reports the week in whole
    /// points, so measuring from the last reading of 95% to the first of 96% timed one point by
    /// the gap between two readings, a minute or two, and an ordinary pace read as abnormal
    /// speed (Work was left at 96% instead of his 99%). And when the rise into the base value
    /// was not seen (the first reading of a window), a whole-point value may already have been
    /// nearly a point higher, so that one point is not counted as speed.
    ///
    /// **A burst after a long unchanged stretch is still a burst** (review
    /// rv-20261019T005929Z-e3d9ad03-3e7b, finding 1): timed from the first reading alone, two
    /// hours at 90% and then 98% five minutes later averaged 8 points over 125 minutes, a normal
    /// speed, and the week reached 100% before the switch. So the rise is also timed from the
    /// LAST reading of the base value, counting only what it certainly gained since then: with
    /// whole points, a value read as 90 was under 91, so 90 -> 98 is at least 7 points in those
    /// 5 minutes. The larger of the two speeds is the speed. One whole point after a steady
    /// reading certainly gained nothing, so it is still no speed.
    fn measure(&mut self, windows: &[Window], observed: u64) {
        for window in windows {
            let fresh = SpeedBase { used: window.used_percent, resets_at: window.resets_at, at: observed, last: observed, rose: false };
            // The SAME window when the reset times agree within a minute: the probe reports
            // an ISO time and the stream whole seconds, so exact equality would treat one
            // window read two ways as two windows and never measure a speed across them.
            let same = |a: Option<u64>, b: Option<u64>| match (a, b) {
                (Some(a), Some(b)) => a.abs_diff(b) < 60_000,
                (a, b) => a == b,
            };
            match self.bases.get(&window.id).copied() {
                Some(base) if same(base.resets_at, window.resets_at) => {
                    if observed >= base.at + SPEED_MIN_GAP_MS {
                        let gained = (window.used_percent - base.used).max(0.);
                        let points = whole(base.used) && whole(window.used_percent);
                        let unseen = if !base.rose && points { 1. } else { 0. };
                        let mut speed = (gained - unseen).max(0.) / (observed - base.at) as f64;
                        let mut rise = Rise { from: base.used, to: window.used_percent, ms: observed - base.at };
                        // Since the last reading of the base value, at least `certain` points.
                        let certain = (gained - if points { 1. } else { 0. }).max(0.);
                        let since_last = observed.saturating_sub(base.last).max(SPEED_MIN_GAP_MS);
                        if certain / since_last as f64 > speed {
                            speed = certain / since_last as f64;
                            rise.ms = since_last;
                        }
                        self.speeds.insert(window.id.clone(), speed);
                        self.rises.insert(window.id.clone(), rise);
                        if window.used_percent != base.used {
                            self.bases.insert(window.id.clone(), SpeedBase { rose: gained > 0., ..fresh });
                        }
                    }
                    if window.used_percent == base.used {
                        if let Some(base) = self.bases.get_mut(&window.id) { base.last = base.last.max(observed); }
                    }
                }
                _ => {
                    self.speeds.remove(&window.id);
                    self.rises.remove(&window.id);
                    self.bases.insert(window.id.clone(), fresh);
                }
            }
        }
    }
    /// The reading with no test cut-off, for the tests; the service reads each account with
    /// its own (`Service::readings`).
    #[cfg(test)]
    fn reading(&self) -> Reading {
        self.reading_with(None, crate::util::now_millis())
    }
    /// This account's reading, with its test cut-off (`Reading::weekly_cutoff`).
    ///
    /// **The weekly speed is the larger of the percentage speed and the token speed** (the CEO,
    /// 2026-10-07): live tokens move it before the percentage does, and the percentage speed
    /// stays its floor, for a model with no tokens-per-point figure yet and for use this Mac's
    /// transcripts cannot see.
    ///
    /// **An old figure is never treated as current** (handoff round 2, run 10: a null answer
    /// kept Work's 0.0% for 27 minutes while its use rose). While the last good reading is
    /// older than the normal check, the weekly figure the decisions read is moved forward by
    /// the points the counted tokens spent since it (`since_reading`). The figure shown stays
    /// the one read, with its age (`View::checked_at`, `State::Stale`).
    fn reading_with(&self, weekly_cutoff: Option<f64>, now: u64) -> Reading {
        let mut speeds = self.speeds.clone();
        if let Some(tokens) = self.token_speed.filter(|s| *s > 0.) {
            let speed = speeds.entry("seven_day".to_string()).or_insert(0.);
            *speed = speed.max(tokens);
        }
        // Expired windows remain in the published telemetry, but cannot drive
        // admission, an account switch or a worker handoff in the next window.
        let mut windows: Vec<_> = self.windows.iter().filter(|w|w.resets_at.is_none_or(|t|t>now)).cloned().collect();
        if self.checked_at.is_some_and(|t| now.saturating_sub(t) >= REFRESH_INTERVAL_MS) && self.since_reading > 0. {
            for window in windows.iter_mut().filter(|w| w.id == "seven_day") {
                window.used_percent = (window.used_percent + self.since_reading).min(100.);
            }
        }
        Reading { windows, speeds,
            expected: self.rise_until.is_some_and(|t| t > now), rises: self.rises.clone(), weekly_cutoff, handoff: self.handoff }
    }
    fn accept(&mut self, mut windows: Vec<Window>, observed: u64) {
        self.measure(&windows, observed);
        // Missing weekly data is not evidence that a known weekly hold ended.
        let missing_held_weekly = (!windows.iter().any(|w| w.id == "seven_day"))
            .then(|| self.windows.iter().find(|w| w.id == "seven_day"
                && w.used_percent >= resets::WEEKLY_THRESHOLD).cloned()).flatten();
        let error = missing_held_weekly.as_ref().map(|_| ReadError::Malformed);
        if let Some(previous) = missing_held_weekly { windows.push(previous); }
        let speeds = std::mem::take(&mut self.speeds);
        let bases = std::mem::take(&mut self.bases);
        let rises = std::mem::take(&mut self.rises);
        let (rise_until, token_speed, handoff) = (self.rise_until, self.token_speed, self.handoff);
        // Only a signed-in account reports a reading.
        *self = Self { windows, checked_at: Some(observed), retry_at: None, error, speeds, bases, rises, rise_until, empty_at: None, token_speed,
            since_reading: 0., handoff, empties: 0, signed_in: Some(true) };
    }
    /// **A check that came back with no figures.** Every kind records when, so the pause stops
    /// waiting on it (`Admission::NoReading`). A null answer (`ReadError::NoReading`) is not a
    /// failure: it clears any earlier failure and its backoff, keeps the last figures as they
    /// were, and leaves the next check to the normal schedule, counted from now
    /// (`next_check_at`). A failure, an unreadable answer or an unsupported account is reported
    /// as before, with its `BACKOFF_MS` wait for the automatic check.
    fn empty(&mut self, error: ReadError, observed: u64) {
        self.empty_at = Some(observed);
        if error == ReadError::NoReading {
            self.error = None;
            self.retry_at = None;
            if self.checked_at.is_none() { self.empties = self.empties.saturating_add(1); }
            return;
        }
        match error {
            ReadError::SignedOut => self.signed_in = Some(false),
            ReadError::SignedInNoLimits => self.signed_in = Some(true),
            _ => {}
        }
        if matches!(error, ReadError::Unsupported | ReadError::SignedOut | ReadError::SignedInNoLimits) {
            self.windows.clear();
            self.checked_at = None;
        }
        self.error = Some(error);
        self.retry_at = Some(observed + BACKOFF_MS);
    }
    /// **A reading the lease itself streamed** (`rate_limit_event`), merged window by window
    /// over the probe's. It never clears a probe error or its backoff; it only adds what it saw.
    fn observe(&mut self, windows: &[Window], observed: u64) {
        if self.checked_at.is_some_and(|t| observed <= t) { return; }
        self.measure(windows, observed);
        for window in windows {
            match self.windows.iter_mut().find(|w| w.id == window.id) {
                Some(existing) => *existing = window.clone(),
                None => self.windows.push(window.clone()),
            }
        }
        self.checked_at = Some(observed);
        self.signed_in = Some(true);
    }
    fn view(&self, policy: Policy, now: u64) -> View {
        self.view_with(policy, None, now)
    }
    /// [`View::signed_in`]: what the last answer about the sign-in established, and nothing
    /// more; a check that said nothing about it leaves it standing (`Snapshot::signed_in`).
    fn signed_in(&self) -> Option<bool> {
        self.signed_in
    }
    /// The view of this account, with its test cut-off (`Reading::weekly_cutoff`).
    fn view_with(&self, policy: Policy, weekly_cutoff: Option<f64>, now: u64) -> View {
        let reading = self.reading_with(weekly_cutoff, now);
        let interval = reading.interval();
        // The two check points, recalculated from the measured speed (answer 10), shown in
        // the panel; the normal 93% and 99% again once the speed is back down (answer 11).
        // The weekly one is the handoff point (weekly-switch plan §1).
        let act_at: BTreeMap<String, f64> = self.windows.iter().filter_map(|w| match w.id.as_str() {
            "five_hour" => Some((w.id.clone(), reading.act_point(w, f64::from(policy.pause_percent)))),
            "seven_day" => Some((w.id.clone(), reading.weekly_point(w))),
            _ => None,
        }).collect();
        let expired = self
            .windows
            .iter()
            .any(|w| w.resets_at.is_some_and(|t| t <= now));
        let fresh = self
            .checked_at
            .is_some_and(|t| now >= t && now - t < interval)
            && self.error.is_none()
            && !expired;
        let state = if self.checked_at.is_none() {
            State::Unavailable
        } else if fresh {
            State::Fresh
        } else {
            State::Stale
        };
        // Unknown and missing readings remain visible as such, but do not hold work.
        // An observed above-threshold window still holds until its actual reset.
        let answered_empty = self.empty_at.is_some_and(|e| self.checked_at.is_none_or(|c| e >= c));
        let admission = if !policy.enabled {
            Admission::Disabled
        } else if self.checked_at.is_some_and(|t| t > now) {
            Admission::Unknown
        } else if self.checked_at.is_none() {
            if answered_empty { Admission::NoReading } else { Admission::Unknown }
        } else if let Some(until) = reading.windows.iter() // moved forward while old (`reading_with`)
            .filter(|w| w.id == "seven_day" && reading.reaches_weekly(w))
            .filter_map(|w| w.resets_at.filter(|t| *t > now)).max() {
            // Weekly exhaustion never inherits the five-hour 20-minute exception.
            // An approval is not allowance: hold until a fresh post-reset reading.
            Admission::Held { resets_at: until }
        } else {
            // §108: at normal speed the 93% rule exactly as before; at a measured fast speed
            // the pause comes EARLIER, when the next check would already find 100%. The
            // 20-minute exception still lets work continue near a reset, but only when the
            // measured speed would not carry the window to 100% before that reset.
            let speed = |w: &Window| reading.speeds.get(&w.id).copied().unwrap_or(0.);
            match self.windows.iter().find(|w| w.id == "five_hour") {
                Some(w)
                    if reading.reaches(w, f64::from(policy.pause_percent))
                        && w.resets_at
                            .is_some_and(|t| t > now && t - now < RESET_EXEMPTION_MS
                                && (speed(w) == 0.
                                    || w.used_percent + speed(w) * ((t - now) as f64) < 100.)) =>
                {
                    Admission::Ready
                }
                // Within the same window, a stale above-threshold value still proves a hold.
                Some(w)
                    if reading.reaches(w, f64::from(policy.pause_percent))
                        && w.resets_at.is_some_and(|t| t > now) =>
                {
                    Admission::Held {
                        resets_at: w.resets_at.unwrap(),
                    }
                }
                Some(w) if fresh && w.resets_at.is_some_and(|t| t > now) => Admission::Ready,
                _ if answered_empty => Admission::NoReading,
                _ => Admission::Unknown,
            }
        };
        View {
            resets: resets::View::default(),
            state,
            windows: self.windows.clone(),
            checked_at: self.checked_at,
            // Counted from the last check of any outcome, so a check that came back empty is
            // asked again one interval later (5 minutes, or 1 while usage is fast), not on the
            // monitor's next tick.
            // An account never read yet is asked again soon after a null answer
            // (`FIRST_READING_RETRY_MS`).
            next_check_at: [self.checked_at, self.empty_at].into_iter().flatten().max().map(|t| {
                let wait = if self.checked_at.is_none() && self.error.is_none() && self.empties > 0 {
                    (FIRST_READING_RETRY_MS << (self.empties - 1).min(8)).min(FIRST_READING_RETRY_MAX_MS)
                } else { interval };
                self.windows
                    .iter()
                    .filter_map(|w| w.resets_at)
                    .filter(|r| *r > t)
                    .fold(t.saturating_add(wait), u64::min)
            }),
            refresh_interval_ms: interval,
            retry_at: self.retry_at.filter(|t| *t > now),
            message: self.error.map(|e| e.message().into()),
            signed_in: self.signed_in(),
            policy,
            admission,
            accounts: Vec::new(),
            held_until: None,
            at_threshold: Default::default(),
            // The speeds the check points were computed from, token speed included, so the
            // separate-process gate decides from the same figures.
            speeds: reading.speeds.clone(),
            act_at,
            rises: self.rises.clone(),
            agents_working: 0,
            last_switch: None,
            empty_at: self.empty_at.filter(|_| answered_empty),
            changes: Vec::new(),
            nudge: None,
            notes: Vec::new(),
            first_label: None,
            leaving: Vec::new(),
        }
    }
}

/// Probe I/O and published state have separate locks: a slow control reply cannot
/// freeze a scheduler, a settings paint or shutdown. Concurrent refreshes coalesce.
pub struct Service {
    pub resets: resets::Service,
    last_reset_marker: Mutex<Option<String>>,
    source: Mutex<Box<dyn Source>>,
    snapshot: Mutex<Snapshot>,
    policy: Mutex<Policy>,
    policy_path: PathBuf,
    cwd: PathBuf,
    control: std::sync::Arc<probe::Control>,
    publication: Mutex<()>,
    connecting: std::sync::atomic::AtomicBool,
    refresh_requested: Mutex<bool>,
    refresh_wake: std::sync::Condvar,
    /// The account list and the switch decision (fill-first). `source` and `snapshot` above
    /// are Account 1's, exactly as before; every added account has its own pair here.
    pub accounts: std::sync::Arc<crate::claude_accounts::Accounts>,
    extra: Mutex<BTreeMap<String, AccountReader>>,
    /// Notices about work actually being held, delivered once in the conversation.
    notes: Mutex<std::collections::VecDeque<String>>,
    /// Rich's lines about ACCOUNTS (round 18): the one-time suggestion of a second account
    /// (`nudge`) and every account close to its week (`bothNear`), each with its kind, so the
    /// conversation can draw the buttons round 18 puts under them (`note_accounts`).
    account_notes: Mutex<std::collections::VecDeque<(String, String)>>,
    was_held: std::sync::atomic::AtomicBool,
    /// When leases and agent dispatches started, for the EXPECTED rise (answer 10).
    starts: Mutex<std::collections::VecDeque<u64>>,
    /// How many of Rich's agents were working at the last count (`set_agents_working`, the
    /// desktop shell's count of the provider's own launch rows): round 16's "15 agents
    /// reading at once" and "3 agents are holding their place".
    agents_working: std::sync::atomic::AtomicUsize,
    /// The webview's offset from UTC in minutes (`set_utc_offset`), so a line can say a clock
    /// time ("until 2:17 AM", round 16). The webview is the only layer that knows what local
    /// means; until it has said, a line says the span instead.
    utc_offset_minutes: Mutex<Option<i32>>,
    /// The test cut-off this app was launched with (`test_weekly_cutoff`, weekly-switch plan
    /// §4): one account and its weekly point. `None` in every ordinary launch.
    weekly_cutoff: Option<(String, f64)>,
    /// Each account's token reader and its learned episodes (`tokens::Track`).
    tokens: Mutex<BTreeMap<String, tokens::Track>>,
    /// Account 1's Claude Code folder, whose transcripts are its token use: the app's own
    /// (`CLAUDE_CONFIG_DIR`, else `~/.claude`), set only by the desktop entrypoint so tests and
    /// simulations never read the developer's transcripts. Added accounts use their folder.
    transcripts_one: Option<PathBuf>,
    /// When the transcripts were last read (`track_tokens`).
    scanned_at: std::sync::atomic::AtomicU64,
    /// The token use every lease of the app streamed, by account (`tokens::Sink`), taken into
    /// each account's track at every token scan. The shell gives it to each lease it starts.
    sink: std::sync::Arc<tokens::Sink>,
    /// **The API-equivalent token spend** (§118, `crate::spend`): every request on every
    /// account's transcripts and every lease's stream, counted once and priced. The leases'
    /// requests reach it from `sink` at every token scan; the transcripts are read when the
    /// screen asks (`spend_view`, `spend_live`) and once after launch.
    spend: std::sync::Arc<crate::spend::ledger::Ledger>,
}

/// The shortest gap between two transcript reads from the monitor's one-second tick.
pub const TOKEN_SCAN_MS: u64 = 10_000;

/// Every account's learned weekly episodes (`tokens::Episode`), kept across launches so the
/// token speed does not start from nothing at every launch.
pub const TOKEN_POINTS_FILE: &str = "claude-token-points.json";

/// The gate appends one line per agent dispatch here (`gate.rs`); the service counts them.
pub const DISPATCH_LOG: &str = "agent-dispatches.log";

/// An added account's own probe and its last reading. The probe has its own lock, as
/// Account 1's `source` does, so a read holds it and never the map (hunt part 1 v3,
/// finding 48: it used to be swapped out of the map for the read, and an overlapping
/// refresh read that account with the placeholder, which is Account 1's reader).
type AccountReader = (std::sync::Arc<Mutex<Box<dyn Source>>>, Snapshot);
/// Windows a lease streamed, and when they were observed (epoch ms).
pub type StreamedReading = (Vec<Window>, u64);

/// Is a read due? The same rule Account 1's reader has always used: a manual refresh waits
/// only for the five-second double-click cooldown; the monitor's tick waits for the backoff
/// and the five-minute cache.
fn due(current: &View, force: bool, now: u64) -> bool {
    let last_attempt = [current.checked_at, current.empty_at, current.retry_at.map(|t| t.saturating_sub(BACKOFF_MS))]
        .into_iter()
        .flatten()
        .filter(|t| *t <= now + 5_000)
        .max();
    let cooling = last_attempt.is_some_and(|t| now.saturating_sub(t) < 5_000);
    !if force {
        cooling
    } else {
        current.retry_at.is_some() || current.next_check_at.is_some_and(|t| t > now)
    }
}

impl Service {
    /// Production desktop entrypoint. Tests and simulations keep `open` isolated.
    pub fn open_account_wide(data_dir: &Path) -> io::Result<Self> {
        let reset_dir = terminal::data_dir().map_err(io::Error::other)?;
        let mut service = Self::open_with_reset_dir(data_dir, &reset_dir)?;
        let nonempty = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
        service.transcripts_one = nonempty("CLAUDE_CONFIG_DIR").or_else(|| nonempty("HOME").map(|h| h.join(".claude")));
        Ok(service)
    }
    fn open_with_reset_dir(data_dir: &Path, reset_dir: &Path) -> io::Result<Self> {
        let mut service = Self::open(data_dir)?;
        service.resets = resets::Service::new(reset_dir);
        service.resets.import_legacy(data_dir).map_err(io::Error::other)?;
        service.publish()?;
        Ok(service)
    }
    pub fn open(data_dir: &Path) -> io::Result<Self> {
        let policy_path = data_dir.join("claude-quota-policy.json");
        let policy = match fs::read(&policy_path) {
            Ok(bytes) => {
                let p: Policy = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
                p.validate().map_err(io::Error::other)?;
                p
            }
            // A fresh install: round 18 calls Pause "The default. Never longer than 5 hours.",
            // so the pause is on until the person releases it (walk of nightly 41, D13).
            Err(e) if e.kind() == io::ErrorKind::NotFound => Policy { enabled: true, ..Policy::default() },
            Err(e) => return Err(e),
        };
        let control = std::sync::Arc::new(probe::Control::default());
        // A missing or unreadable file is no episodes yet: the figures are learned again.
        let learned: BTreeMap<String, Vec<tokens::Episode>> = fs::read(data_dir.join(TOKEN_POINTS_FILE)).ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok()).unwrap_or_default();
        let service = Self {
            resets: resets::Service::new(data_dir),
            last_reset_marker: Mutex::new(None),
            source: Mutex::new(Box::new(probe::ClaudeSource::controlled(control.clone()))),
            snapshot: Mutex::new(Snapshot::default()),
            policy: Mutex::new(policy),
            policy_path,
            cwd: data_dir.to_path_buf(),
            control,
            publication: Mutex::new(()),
            connecting: std::sync::atomic::AtomicBool::new(false),
            refresh_requested: Mutex::new(true),
            refresh_wake: std::sync::Condvar::new(),
            accounts: std::sync::Arc::new(crate::claude_accounts::Accounts::open(data_dir)?),
            extra: Mutex::new(BTreeMap::new()),
            notes: Mutex::new(std::collections::VecDeque::new()),
            account_notes: Mutex::new(std::collections::VecDeque::new()),
            was_held: std::sync::atomic::AtomicBool::new(false),
            starts: Mutex::new(std::collections::VecDeque::new()),
            agents_working: std::sync::atomic::AtomicUsize::new(0),
            utc_offset_minutes: Mutex::new(None),
            weekly_cutoff: test_weekly_cutoff(),
            tokens: Mutex::new(learned.into_iter().map(|(id, e)| (id, tokens::Track::with_episodes(e))).collect()),
            transcripts_one: None,
            scanned_at: std::sync::atomic::AtomicU64::new(0),
            sink: std::sync::Arc::new(tokens::Sink::default()),
            spend: std::sync::Arc::new(crate::spend::ledger::Ledger::open(&data_dir.join("spend"))),
        };
        service.publish()?;
        Ok(service)
    }
    fn publish(&self) -> io::Result<()> {
        let _serial = self.publication.lock().unwrap();
        atomic_write(
            &self.cwd.join("engine-state/claude-quota.json"),
            &self.view(),
        )
    }
    pub fn view(&self) -> View {
        self.view_at(crate::util::now_millis())
    }
    /// Wake the desktop reader at a new-session boundary without blocking the
    /// session's launch or Stop controls on provider I/O. Bursts coalesce.
    pub fn request_refresh(&self) {
        *self.refresh_requested.lock().unwrap() = true;
        self.refresh_wake.notify_one();
    }
    /// The desktop monitor consumes this signal before applying the normal TTL.
    /// Startup begins signaled, even when automatic pausing is disabled.
    pub fn wait_for_refresh(&self, timeout: std::time::Duration) -> bool {
        let (mut requested, _) = self.refresh_wake.wait_timeout_while(
            self.refresh_requested.lock().unwrap(), timeout, |requested| !*requested,
        ).unwrap();
        std::mem::take(&mut *requested)
    }
    /// The test cut-off of `account`, if the app was launched with one for it.
    fn cutoff(&self, account: &str) -> Option<f64> {
        self.weekly_cutoff.as_ref().filter(|(id, _)| id == account).map(|(_, percent)| *percent)
    }
    fn view_at(&self, now: u64) -> View {
        let policy = self.policy.lock().unwrap().clone();
        let in_use = self.accounts.in_use();
        let cutoff = self.cutoff(&in_use.id);
        // The top-level reading is the account IN USE, so the pause gate and every existing
        // reader keep describing the subscription the work is actually running on.
        let mut view = if in_use.id == crate::claude_accounts::ACCOUNT_ONE {
            self.snapshot.lock().unwrap().view_with(policy.clone(), cutoff, now)
        } else {
            match self.extra.lock().unwrap().get(&in_use.id) {
                Some((_, snapshot)) => snapshot.view_with(policy.clone(), cutoff, now),
                None => Snapshot::default().view(policy.clone(), now),
            }
        };
        let readings = self.readings();
        // Every account being left for its week (plan §1), with one account too, unless that
        // one account has automatic pause off: then nothing acts before 100%, as the fast-use
        // alert says, so no handoff order goes out. With two or more the weekly switch acts
        // whatever the pause setting says.
        view.leaving = if self.accounts.count() <= 1 && !policy.enabled { Vec::new() } else {
            self.accounts.list().into_iter()
                .filter(|account| readings.get(&account.id).is_some_and(|r| r.leaving(now)))
                .map(|account| account.id).collect()
        };
        view.resets = self.resets.view();
        view.at_threshold = self.accounts.at_threshold();
        if self.accounts.count() > 1 {
            view.accounts = self.accounts.list().into_iter().map(|account| {
                let account_view = if account.id == crate::claude_accounts::ACCOUNT_ONE {
                    self.snapshot.lock().unwrap().view(policy.clone(), now)
                } else {
                    self.extra.lock().unwrap().get(&account.id).map(|(_, s)| s.view(policy.clone(), now))
                        .unwrap_or_else(|| Snapshot::default().view(policy.clone(), now))
                };
                let exhausted = crate::claude_accounts::gone(
                    readings.get(&account.id).unwrap_or(&Reading::default()), view.at_threshold,
                    policy.line(), self.accounts.limited_until(&account.id), now);
                crate::claude_accounts::AccountView {
                    in_use: account.id == in_use.id,
                    working: self.accounts.working_on(&account.id),
                    id: account.id,
                    label: account.label,
                    windows: account_view.windows,
                    checked_at: account_view.checked_at,
                    exhausted_until: exhausted.map(|(_, until)| until),
                    message: account_view.message,
                    signed_in: account_view.signed_in,
                }
            }).collect();
            view.held_until = self.accounts.held_until(&readings, policy.line(), now);
            // The card after a switch, only once the switch has happened for every session
            // (`Accounts::switch_pending`): nightly 65 showed "Rich switched to Home ... the
            // team carried on" while Stu's job still ran on Work.
            let pending = self.accounts.switch_pending();
            view.last_switch = self.accounts.last_switch().filter(|_| !pending);
            if let Some(until) = view.held_until {
                view.admission = Admission::Held { resets_at: until };
            }
        }
        view.agents_working = self.agents_working.load(std::sync::atomic::Ordering::SeqCst);
        view.changes = self.accounts.changes();
        // Recent changes' "Rich switched to ..." row waits for the same moment.
        if self.accounts.switch_pending() {
            if let Some(at) = view.changes.iter().rposition(|change| change.kind == "switched") {
                view.changes.remove(at);
            }
        }
        view.nudge = self.accounts.nudge();
        view.notes = self.accounts.notes();
        if self.accounts.count() == 1 && in_use.label != "Account 1" {
            view.first_label = Some(in_use.label);
        }
        view
    }

    /// Every account's freshest reading (probe and streamed, merged in its snapshot), with
    /// each window's measured speed. Only accounts that have been read appear.
    fn readings(&self) -> BTreeMap<String, Reading> {
        let mut readings = BTreeMap::new();
        {
            let one = self.snapshot.lock().unwrap();
            if one.checked_at.is_some() {
                readings.insert(crate::claude_accounts::ACCOUNT_ONE.to_string(),
                    one.reading_with(self.cutoff(crate::claude_accounts::ACCOUNT_ONE), crate::util::now_millis()));
            }
        }
        for (id, (_, snapshot)) in self.extra.lock().unwrap().iter() {
            if snapshot.checked_at.is_some() {
                readings.insert(id.clone(), snapshot.reading_with(self.cutoff(id), crate::util::now_millis()));
            }
        }
        readings
    }

    /// **Before EVERY turn** (the CEO's correction, 2026-10-04): merge the reading the lease
    /// on `account` streamed, if any (it is fresher than the five-minute probe), decide on the
    /// freshest readings whether the account in use must be left, and return the account that
    /// is in use now. The caller rotates its lease when that is not `account`; this is a turn
    /// boundary, so rotating is legal.
    pub fn before_turn(&self, account: &str, streamed: Option<StreamedReading>) -> String {
        if let Some((windows, observed)) = streamed {
            if account == crate::claude_accounts::ACCOUNT_ONE {
                self.snapshot.lock().unwrap().observe(&windows, observed);
            } else if let Some((_, snapshot)) = self.extra.lock().unwrap().get_mut(account) {
                snapshot.observe(&windows, observed);
            }
        }
        self.track_tokens();
        self.decide();
        self.note_held();
        self.note_accounts();
        let _best_effort = self.publish();
        self.accounts.in_use().id
    }

    /// **Token use, before every decision** (`tokens`): take what the app's own leases streamed
    /// on each account (`tokens::Sink`) and what its transcripts gained (Claude Code used
    /// outside the app), give its freshest weekly reading to its track (an episode may close:
    /// tokens per point are learned from it), and set its token speed from the last minute.
    /// True when a token speed changed.
    fn track_tokens(&self) -> bool {
        let now = crate::util::now_millis();
        self.scanned_at.store(now, std::sync::atomic::Ordering::SeqCst);
        let streamed = self.sink.take();
        // The leases' requests into the spend ledger too: nothing else holds them (§118).
        self.spend.add(self.sink.take_for_spend(), now);
        self.spend.save(now, false);
        // The gate's handoff markers: when each helper was ordered and when it committed.
        let markers = gate::handoffs(&self.cwd.join("engine-state"));
        let (mut learned, mut moved) = (false, false);
        for account in self.accounts.list() {
            let mine: Vec<&(PathBuf, gate::Handoff)> = markers.iter().filter(|(_, m)| m.account == account.id).collect();
            let one = account.id == crate::claude_accounts::ACCOUNT_ONE;
            let folder = if one { self.transcripts_one.clone() } else { account.folder.clone() };
            let week = |s: &Snapshot| s.windows.iter().find(|w| w.id == "seven_day").cloned().zip(s.checked_at);
            let reading = if one { week(&self.snapshot.lock().unwrap()) }
                else { self.extra.lock().unwrap().get(&account.id).and_then(|(_, s)| week(s)) };
            let (speed, since, counted) = {
                let mut tracks = self.tokens.lock().unwrap();
                let track = tracks.entry(account.id.clone()).or_default();
                if let Some(folder) = &folder { track.scan(folder, now); }
                for (_, found) in streamed.iter().filter(|(id, _)| *id == account.id) { track.add_use(found); }
                let since = reading.as_ref().map_or(0., |(_, at)| track.points_since(*at, now));
                // A handoff under way keeps its tokens until its commit; a committed one has its
                // points counted once, from the order to the commit.
                track.hold_from = mine.iter().filter(|(_, m)| m.committed_at.is_none() && m.at + HANDOFF_BATCH_MS > now)
                    .map(|(_, m)| m.at).min();
                let counted: Vec<(PathBuf, f64)> = mine.iter()
                    .filter_map(|(path, m)| match (m.committed_at, m.points) {
                        (Some(committed), None) => Some((path.clone(), track.points_since(m.at, committed))),
                        _ => None,
                    }).collect();
                if let Some((window, at)) = reading { learned |= track.reading(&window, at, now); }
                (track.speed(now), since, counted)
            };
            for (path, points) in &counted {
                if let Err(error) = gate::record_points(path, *points) {
                    eprintln!("[richos] quota: a handoff's points could not be kept ({error})");
                }
            }
            // **What a handoff really takes on this account** (the CEO, 2026-10-07): its last
            // handoff's longest order-to-commit time and the points it spent.
            let measured: Vec<(u64, u64, f64)> = mine.iter().filter_map(|(path, m)| {
                let committed = m.committed_at?;
                let points = m.points.or_else(|| counted.iter().find(|(p, _)| p == path).map(|(_, p)| *p)).unwrap_or(0.);
                Some((m.at, committed.saturating_sub(m.at), points))
            }).collect();
            let handoff = measured.iter().map(|m| m.0).max().map(|newest| {
                let last = measured.iter().filter(|m| m.0 + HANDOFF_BATCH_MS >= newest);
                (last.clone().map(|m| m.1).max().unwrap_or(HANDOFF_DEFAULT_MS), last.map(|m| m.2).fold(0., f64::max))
            });
            let mut set = |s: &mut Snapshot| {
                moved |= s.token_speed != Some(speed) || s.since_reading != since || s.handoff != handoff;
                s.token_speed = Some(speed);
                s.since_reading = since;
                s.handoff = handoff;
            };
            if one { set(&mut self.snapshot.lock().unwrap()); }
            else if let Some((_, snapshot)) = self.extra.lock().unwrap().get_mut(&account.id) { set(snapshot); }
        }
        if learned {
            let episodes: BTreeMap<String, Vec<tokens::Episode>> = self.tokens.lock().unwrap().iter()
                .map(|(id, t)| (id.clone(), t.episodes.clone())).collect();
            if let Err(error) = atomic_write(&self.cwd.join(TOKEN_POINTS_FILE), &episodes) {
                eprintln!("[richos] quota: the tokens per point could not be kept ({error})");
            }
        }
        moved
    }

    /// **Every account used up** (round 16): said once when the hold begins, naming the account
    /// that comes back first, why, and in how long. Said again only after the hold has ended.
    fn note_held(&self) {
        let pause = self.policy.lock().unwrap().line();
        let now = crate::util::now_millis();
        let held = self.accounts.held(&self.readings(), pause, now);
        let was = self.was_held.swap(held.is_some(), std::sync::atomic::Ordering::SeqCst);
        let Some((label, why, until)) = held else { return };
        if was { return; }
        // Round 16: "Every account is used up. 3 agents are holding their place until 2:17 AM,
        // when Home's window resets, the soonest." The clock time needs the webview's offset;
        // before it has said one, the span is said instead.
        let back = match why {
            crate::claude_accounts::Gone::Limit => "usage limit lifts",
            crate::claude_accounts::Gone::Weekly(_) | crate::claude_accounts::Gone::FiveHour(_) => "window resets",
        };
        let who = match self.agents_working.load(std::sync::atomic::Ordering::SeqCst) {
            0 => "Agents hold their place".to_string(),
            1 => "1 agent is holding its place".to_string(),
            n => format!("{n} agents are holding their place"),
        };
        let offset = *self.utc_offset_minutes.lock().unwrap();
        self.notes.lock().unwrap().push_back(match offset {
            Some(offset) => format!("Every account is used up. {who} until {}, when {label}'s {back}, the soonest.",
                clock(until, offset, now)),
            None => format!("Every account is used up. {who} until {label}'s {back} in {}, the soonest.",
                words(until.saturating_sub(now))),
        });
    }

    /// **How many of Rich's agents are working**, as the desktop shell last counted them (the
    /// provider's own native task frames). Said in the
    /// settings view and the every-account-used-up line; 0 leaves the count out.
    pub fn set_agents_working(&self, count: usize) {
        self.agents_working.store(count, std::sync::atomic::Ordering::SeqCst);
    }

    /// **The webview's offset from UTC**, minutes east positive (`-getTimezoneOffset()`), so
    /// Rich's lines can say a clock time. The caller's, never read from a timezone here.
    pub fn set_utc_offset(&self, minutes: i32) {
        *self.utc_offset_minutes.lock().unwrap() = Some(minutes);
        self.accounts.set_utc_offset(minutes);
    }

    /// **Rich's lines about accounts, for someone who is not technical** (round 18, richos-hq
    /// `design/mockups/rounds/round-18/`; the CEO 2026-10-06, feedback item 9: "Non-technical
    /// users should also be able to take advantage of the multi-account setup").
    /// - **The suggestion, once ever** (`one-nudge`): one account, at
    ///   [`crate::claude_accounts::NUDGE_WEEKLY_PERCENT`] of its week. Most people will
    ///   discover a second account here; the buttons are drawn under it by the conversation.
    /// - **Every account close to its week** (`both-near-chat`): each at
    ///   [`crate::claude_accounts::NEAR_WEEKLY_PERCENT`] or more. Said once while that lasts.
    ///
    /// Round 18's "At today's pace, both run out in about 7 hours" is a fixture in the mockup
    /// (its NOTES.md, "Limits"); nothing here measures a pace across accounts, so the line says
    /// what happens when they run out and not when.
    fn note_accounts(&self) {
        use crate::claude_accounts::{NEAR_WEEKLY_PERCENT, NUDGE_WEEKLY_PERCENT};
        let now = crate::util::now_millis();
        let readings = self.readings();
        let offset = *self.utc_offset_minutes.lock().unwrap();
        let week = |id: &str| readings.get(id)
            .and_then(|r| r.windows.iter().find(|w| w.id == "seven_day").cloned());
        let list = self.accounts.list();
        if list.len() == 1 {
            let Some(w) = week(&list[0].id) else { return };
            if w.used_percent < NUDGE_WEEKLY_PERCENT { return; }
            match self.accounts.say_once("nudge", now) {
                Ok(true) => {}
                Ok(false) => return,
                Err(error) => { eprintln!("[richos] claude accounts: the suggestion could not be recorded ({error})"); return; }
            }
            let fresh = w.resets_at.filter(|t| *t > now)
                .map(|t| format!(" It is fresh again {}. If it runs out before then, the team pauses until it is.", when_long(t, offset, now)))
                .unwrap_or_default();
            self.account_notes.lock().unwrap().push_back(("nudge".into(), format!(
                "Your Claude account has used **{}%** of this week's limit.{fresh}\n\n\
                 If you have a second Claude account, for example one for work, I can switch to it \
                 when this one is nearly full, so the team keeps working.", w.used_percent.floor())));
            return;
        }
        // Every account has a reading and is close to its week. Ordered as the sheet orders
        // them: the one in use first, then by when its week is fresh again.
        let in_use = self.accounts.in_use().id;
        let mut near: Vec<(String, Window)> = Vec::new();
        for account in &list {
            match week(&account.id) {
                Some(w) if w.used_percent >= NEAR_WEEKLY_PERCENT => near.push((account.label.clone(), w)),
                _ => {
                    if let Err(error) = self.accounts.clear_both_near() { eprintln!("[richos] claude accounts: {error}"); }
                    return;
                }
            }
        }
        match self.accounts.say_once("bothNear", now) {
            Ok(true) => {}
            Ok(false) => return,
            Err(error) => { eprintln!("[richos] claude accounts: the line could not be recorded ({error})"); return; }
        }
        let first = list.iter().position(|a| a.id == in_use).unwrap_or(0);
        let lead = near.remove(first);
        near.sort_by_key(|(_, w)| w.resets_at.unwrap_or(u64::MAX));
        near.insert(0, lead);
        let all = if near.len() == 2 { "both of your Claude accounts are".to_string() } else { format!("all {} of your Claude accounts are", near.len()) };
        let figures: Vec<String> = near.iter().enumerate()
            .map(|(i, (label, w))| if i == 0 { format!("{label} has used {}%", w.used_percent.floor()) } else { format!("{label} {}%", w.used_percent.floor()) })
            .collect();
        let figures = match figures.split_last() {
            Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
            _ => figures.join(""),
        };
        let back = near.iter().filter_map(|(label, w)| w.resets_at.filter(|t| *t > now).map(|t| (label, t))).min_by_key(|(_, t)| *t);
        let then = match back {
            Some((label, t)) => format!("When they run out, the team pauses until {label} is fresh again {}. Nothing is lost.", when_long(t, offset, now)),
            None => "When they run out, the team pauses until one of them is fresh again. Nothing is lost.".to_string(),
        };
        self.account_notes.lock().unwrap().push_back(("bothNear".into(), format!(
            "Heads up: {all} close to their weekly limit. {figures}.\n\n{then} \
             If you have another Claude account, add it and the team keeps going.")));
    }

    /// Rich's lines about accounts (`note_accounts`), each `(kind, text)`, taken once by whoever
    /// says them in the conversation; the caller records the turn with
    /// [`crate::claude_accounts::Accounts::noted`].
    pub fn take_account_notes(&self) -> Vec<(String, String)> {
        self.account_notes.lock().unwrap().drain(..).collect()
    }

    /// Rich's other quota lines, each taken once by whoever says it in the conversation.
    pub fn take_notes(&self) -> Vec<String> {
        self.notes.lock().unwrap().drain(..).collect()
    }

    /// **A lease was started** (the shell's lease factory, conversation or work). Several
    /// within a minute is an EXPECTED rise (plan §15 answer 10): checking goes to the fast
    /// interval now, before any jump is measured.
    pub fn lease_started(&self) {
        self.started(&[crate::util::now_millis()]);
    }

    fn started(&self, at: &[u64]) {
        let now = crate::util::now_millis();
        let expected = {
            let mut starts = self.starts.lock().unwrap();
            starts.extend(at.iter().copied());
            while starts.front().is_some_and(|t| *t + EXPECTED_RISE_WINDOW_MS < now) { starts.pop_front(); }
            starts.len() >= EXPECTED_RISE_STARTS
        };
        if expected {
            self.starts.lock().unwrap().clear();
            let until = Some(now + EXPECTED_RISE_MS);
            let in_use = self.accounts.in_use().id;
            if in_use == crate::claude_accounts::ACCOUNT_ONE {
                self.snapshot.lock().unwrap().rise_until = until;
            } else if let Some((_, snapshot)) = self.extra.lock().unwrap().get_mut(&in_use) {
                snapshot.rise_until = until;
            }
            self.request_refresh();
            let _best_effort = self.publish();
        }
    }

    /// The agent dispatches the gate wrote since the last look (a large parallel job is an
    /// expected rise too). The file is emptied after each look, so it stays small.
    fn take_dispatches(&self) {
        let path = self.cwd.join("engine-state").join(DISPATCH_LOG);
        let Ok(text) = fs::read_to_string(&path) else { return };
        if text.is_empty() { return; }
        let _best_effort = fs::write(&path, b"");
        let at: Vec<u64> = text.lines().filter_map(|l| l.trim().parse().ok()).collect();
        self.started(&at);
    }

    fn decide(&self) {
        let pause = self.policy.lock().unwrap().line();
        match self.accounts.evaluate(&self.readings(), pause, crate::util::now_millis()) {
            Ok(true) => {
                if let Err(error) = self.publish() { eprintln!("[richos] quota snapshot: {error}"); }
            }
            Ok(false) => {}
            Err(error) => eprintln!("[richos] claude accounts: the switch could not be saved ({error})"),
        }
    }

    /// **The backstop**: a turn on `account` was refused by a usage limit anyway.
    pub fn limit_reached(&self, account: &str, resets_at: Option<u64>) -> crate::claude_accounts::AfterLimit {
        let pause = self.policy.lock().unwrap().line();
        let outcome = self.accounts.limit_reached(account, resets_at, &self.readings(), pause, crate::util::now_millis())
            .unwrap_or(crate::claude_accounts::AfterLimit::NoRoom);
        if let Err(error) = self.publish() { eprintln!("[richos] quota snapshot: {error}"); }
        outcome
    }

    /// The folder a lease on the account in use runs under, and that account's id.
    pub fn lease_account(&self) -> crate::claude_accounts::Account {
        self.accounts.in_use()
    }

    /// Where a lease puts the tokens it streams (`EngineProfile::token_sink`), counted under
    /// the account it runs on.
    pub fn token_sink(&self) -> std::sync::Arc<tokens::Sink> {
        self.sink.clone()
    }

    /// Every account folder whose transcripts are spend: Account 1's only when the desktop
    /// entrypoint set it (`transcripts_one`), as for the token speed, so tests and simulations
    /// never read the developer's history.
    fn spend_folders(&self) -> Vec<PathBuf> {
        self.accounts.list().into_iter().filter_map(|account| {
            if account.id == crate::claude_accounts::ACCOUNT_ONE { self.transcripts_one.clone() } else { account.folder }
        }).collect()
    }

    /// **Bring the spend up to date** (§118): the leases' requests, then what every account's
    /// transcripts gained (the whole history on the first read). `false` without reading while
    /// another read is under way: the first one reads gigabytes once.
    pub fn spend_update(&self) -> bool {
        let now = crate::util::now_millis();
        self.spend.add(self.sink.take_for_spend(), now);
        self.spend.update(&self.spend_folders(), now)
    }

    /// **The first read never holds a screen** (9.28 GB took 15.97 s on the CEO's Mac): until the
    /// whole history has been read once, a call starts that read on its own thread and answers at
    /// once with `ready: false`, so the screen says it is counting instead of waiting blank. A read
    /// already under way makes this one return at once (`Ledger::update`'s own lock).
    fn spend_catch_up(&self) {
        if self.spend.ready() {
            self.spend_update();
            return;
        }
        let (ledger, sink, folders) = (self.spend.clone(), self.sink.clone(), self.spend_folders());
        std::thread::spawn(move || {
            let now = crate::util::now_millis();
            ledger.add(sink.take_for_spend(), now);
            ledger.update(&folders, now);
        });
    }

    /// The spend screen's snapshot of everything at or after `from` (Unix ms: the webview's local
    /// midnight, Monday or first of the month; 0 for all time).
    pub fn spend_view(&self, from: u64) -> crate::spend::ledger::SpendView {
        // Ready as it was when asked: a first read started now is never reported as done.
        let ready = self.spend.ready();
        self.spend_catch_up();
        crate::spend::ledger::SpendView {
            ready,
            as_of: crate::util::now_millis(),
            price_date: crate::spend::PRICE_DATE,
            totals: self.spend.totals(from),
        }
    }

    /// The spend screen's live line: the agents working at the shell's last count, the latest
    /// request, and how many have been counted ever (more than the snapshot's: Refresh's dot).
    pub fn spend_live(&self) -> crate::spend::ledger::SpendLive {
        let ready = self.spend.ready();
        self.spend_catch_up();
        crate::spend::ledger::SpendLive {
            ready,
            counted: self.spend.counted(),
            agents_working: self.agents_working.load(std::sync::atomic::Ordering::SeqCst),
            last: self.spend.last().as_ref().map(crate::spend::ledger::LastRequest::of),
        }
    }

    /// Add an account. `current` names the account already signed in when this is the second
    /// (round 16: going from one account to two names both); empty keeps its label.
    pub fn add_account(&self, label: &str, current: Option<&str>) -> io::Result<View> {
        if self.accounts.count() == 1 {
            if let Some(current) = current { self.accounts.rename(crate::claude_accounts::ACCOUNT_ONE, current)?; }
        }
        self.accounts.add(label)?;
        self.request_refresh();
        let _best_effort = self.publish();
        Ok(self.view())
    }

    /// **A sign-in that just completed is recorded where the card reads it** (review
    /// rv-20261022T035101Z-dd8cd796-0c4b, finding 1): Claude Code's browser sign-in came back
    /// connected for `id`, so its quota record says signed in (`Snapshot::signed_in`) from now,
    /// not from the next usage read, and a read that then fails cannot take it back. Account 1's
    /// record has just been cleared by the changed connection (`set_connecting`); an added
    /// account's is made here if no read has made it yet.
    pub fn signed_in(&self, id: &str) {
        if id == crate::claude_accounts::ACCOUNT_ONE {
            self.snapshot.lock().unwrap().signed_in = Some(true);
        } else if let Some(folder) = self.accounts.folder(id) {
            let mut extra = self.extra.lock().unwrap();
            let (_, snapshot) = extra.entry(id.to_string()).or_insert_with(|| self.reader(folder));
            snapshot.signed_in = Some(true);
        }
        if let Err(error) = self.publish() {
            eprintln!("[richos] quota snapshot: {error}");
        }
    }
    /// An added account's own probe and an empty record, made on first use.
    fn reader(&self, folder: std::path::PathBuf) -> AccountReader {
        (std::sync::Arc::new(Mutex::new(Box::new(probe::ClaudeSource::for_folder(self.control.clone(), folder)) as Box<dyn Source>)),
            Snapshot::default())
    }
    pub fn remove_account(&self, id: &str) -> io::Result<View> {
        self.accounts.remove(id)?;
        self.extra.lock().unwrap().remove(id);
        let _best_effort = self.publish();
        Ok(self.view())
    }

    /// **He picks the account that drains first** (the CEO 2026-10-06, feedback item 8). Only an
    /// account with a reading and room can be picked: one never read may not be signed in, and
    /// one with no room would be left again before its first turn. The automatic switch at 99%
    /// of the week then works from the account he picked, as from any other (`decide`).
    pub fn use_account_first(&self, id: &str) -> io::Result<View> {
        let account = self.accounts.list().into_iter().find(|a| a.id == id)
            .ok_or_else(|| io::Error::other("That account is no longer on this Mac."))?;
        let readings = self.readings();
        let Some(reading) = readings.get(id) else {
            return Err(io::Error::other(format!("{} has no reading yet. Sign it in, then refresh.", account.label)));
        };
        let pause = self.policy.lock().unwrap().line();
        let now = crate::util::now_millis();
        if crate::claude_accounts::gone(reading, self.accounts.at_threshold(), pause, self.accounts.limited_until(id), now).is_some() {
            return Err(io::Error::other(format!("{} has no room right now, so Rich would switch away from it at once.", account.label)));
        }
        self.accounts.use_first(id)?;
        self.decide();
        let _best_effort = self.publish();
        Ok(self.view())
    }

    pub fn set_at_threshold(&self, value: crate::claude_accounts::AtThreshold) -> io::Result<View> {
        self.accounts.set_at_threshold(value)?;
        self.decide();
        let _best_effort = self.publish();
        Ok(self.view())
    }

    /// Read every added account on the same schedule as Account 1, one control-only child
    /// each, under its own folder.
    fn refresh_accounts(&self, bin: &Path, force: bool) {
        let policy = self.policy.lock().unwrap().clone();
        for account in self.accounts.list() {
            if account.id == crate::claude_accounts::ACCOUNT_ONE || self.is_shutdown() { continue; }
            let Some(folder) = account.folder.clone() else { continue };
            let now = crate::util::now_millis();
            let (reader, current) = {
                let mut extra = self.extra.lock().unwrap();
                let (reader, snapshot) = extra.entry(account.id.clone()).or_insert_with(|| self.reader(folder));
                (reader.clone(), snapshot.view(policy.clone(), now))
            };
            if !due(&current, force, now) { continue; }
            // The read holds this account's own probe lock, never the map's, so a slow
            // provider never holds the lock a settings paint needs. A read already in flight
            // for this account is shared, exactly as Account 1's is (`refresh_windows`).
            let Ok(mut source) = reader.try_lock() else { continue };
            let result = source.read(bin, &self.cwd);
            drop(source);
            let observed = crate::util::now_millis();
            let mut extra = self.extra.lock().unwrap();
            let Some((_, snapshot)) = extra.get_mut(&account.id) else { continue };
            match result {
                Ok(windows) => snapshot.accept(windows, observed),
                Err(error) => snapshot.empty(error, observed),
            }
        }
    }
    pub fn set_policy(&self, policy: Policy) -> io::Result<View> {
        policy.validate().map_err(io::Error::other)?;
        let mut current = self.policy.lock().unwrap();
        atomic_write(&self.policy_path, &policy)?;
        *current = policy;
        drop(current);
        // Turning the automatic switch on with Switch chosen past the line moves now, as the
        // setting itself does (`set_at_threshold`).
        self.decide();
        if let Err(error) = self.publish() {
            eprintln!("[richos] quota snapshot: {error}");
        }
        Ok(self.view())
    }
    pub fn refresh(&self, bin: &Path, force: bool) -> View {
        if self.is_shutdown() || self.connecting.load(std::sync::atomic::Ordering::SeqCst) { return self.view(); }
        self.take_dispatches();
        let marker = self.resets.marker();
        let changed = {
            let mut last = self.last_reset_marker.lock().unwrap();
            if marker != *last { *last = marker; true } else { false }
        };
        if changed {
            self.source.lock().unwrap().disconnect();
            *self.snapshot.lock().unwrap() = Snapshot::default();
            // The reset was used on Account 1's sign-in (the reset reader's), so its
            // usage-limit hold goes with its old reading (hunt part 1 v3, finding 49).
            if let Err(error) = self.accounts.reset_used(crate::claude_accounts::ACCOUNT_ONE) {
                eprintln!("[richos] claude accounts: the reset could not be recorded ({error})");
            }
        }
        self.refresh_windows(bin, force || changed);
        if self.accounts.count() > 1 { self.refresh_accounts(bin, force); }
        // The monitor ticks every second; transcripts are read at most every `TOKEN_SCAN_MS`
        // here (every turn reads them anyway, `before_turn`).
        let now = crate::util::now_millis();
        let moved = now >= self.scanned_at.load(std::sync::atomic::Ordering::SeqCst) + TOKEN_SCAN_MS
            && self.track_tokens();
        if self.accounts.count() > 1 {
            self.decide();
            if let Err(error) = self.publish() { eprintln!("[richos] quota snapshot: {error}"); }
        } else if moved {
            if let Err(error) = self.publish() { eprintln!("[richos] quota snapshot: {error}"); }
        }
        self.note_held();
        self.note_accounts();
        self.resets.refresh(bin, force || changed);
        self.view()
    }
    /// Run the user's prepared weekly action without depending on another model turn.
    /// Only the desktop monitor calls this; settings reads and approval never redeem.
    pub fn run_approved_weekly_reset(&self, bin: &Path) {
        if self.is_shutdown() || self.connecting.load(std::sync::atomic::Ordering::SeqCst) { return; }
        #[cfg(not(test))]
        self.resets.tick(bin, || !self.is_shutdown()
            && !self.connecting.load(std::sync::atomic::Ordering::SeqCst));
        #[cfg(test)] let _unused = bin;
    }
    fn refresh_windows(&self, bin: &Path, force: bool) -> View {
        if self.is_shutdown() {
            return self.view();
        }
        let Ok(mut source) = self.source.try_lock() else {
            return self.view();
        };
        if self.connecting.load(std::sync::atomic::Ordering::SeqCst) {
            return self.view();
        }
        let now = crate::util::now_millis();
        // Account 1's own reading (the account in use may be another one).
        let current = self.snapshot.lock().unwrap().view(self.policy.lock().unwrap().clone(), now);
        // **A manual refresh does not wait out the automatic failure backoff.** The backoff
        // protects the provider from the monitor's ticks; a person who fixed the problem and
        // asked again is owed a real check. What still protects the provider from him is the
        // short cooldown against double-clicks, measured from the last attempt of any kind —
        // a failure's `retry_at` is set `BACKOFF_MS` past the attempt that caused it.
        if !due(&current, force, now) {
            return self.view();
        }
        let result = source.read(bin, &self.cwd);
        let observed = crate::util::now_millis();
        let mut snapshot = self.snapshot.lock().unwrap();
        match result {
            Ok(windows) => {
                snapshot.accept(windows, observed);
            }
            Err(error) => snapshot.empty(error, observed),
        }
        drop(snapshot);
        if let Err(error) = self.publish() {
            eprintln!("[richos] quota snapshot: {error}");
        }
        self.view()
    }
    pub fn set_connecting(&self, connecting: bool) {
        if self
            .connecting
            .swap(connecting, std::sync::atomic::Ordering::SeqCst)
            != connecting
        {
            self.disconnect();
        }
    }
    /// Called when the account connection changes. Old account readings cannot survive it.
    pub fn disconnect(&self) {
        self.source.lock().unwrap().disconnect();
        self.resets.clear_connection();
        *self.snapshot.lock().unwrap() = Snapshot::default();
        let _best_effort = self.publish();
        self.request_refresh();
    }
    pub fn is_shutdown(&self) -> bool {
        self.control.stopped()
    }
    pub fn shutdown(&self) {
        self.control.stop();
        self.source.lock().unwrap().disconnect();
        // The spend's last requests, written now (§118; the review of 2dd1375d7): a lease's
        // requests are in no transcript, and a token scan saves the ledger at most every 30 s, so
        // whatever came in since the last save is drained into it and saved before the app goes.
        let now = crate::util::now_millis();
        self.spend.add(self.sink.take_for_spend(), now);
        self.spend.save(now, true);
        // Approval survives app restarts; a fresh authenticated account check fences use.
        self.request_refresh();
    }
}

pub(crate) fn atomic_write(path: &Path, value: &impl Serialize) -> io::Result<()> {
    use io::Write;
    fs::create_dir_all(
        path.parent()
            .ok_or_else(|| io::Error::other("missing parent"))?,
    )?;
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&serde_json::to_vec(value).map_err(io::Error::other)?)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _best_effort = fs::remove_file(temporary);
    }
    result
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    #[test]
    fn desktop_boot_keeps_app_data_isolated_but_reads_the_shared_reset_store() {
        let root = Scratch::new();
        let shared = root.path().join("account");
        let nightly = root.path().join("nightly");
        fs::create_dir_all(&shared).unwrap();
        atomic_write(&shared.join("claude-reset-offers.json"), &serde_json::json!({
            "account":"fixture", "view":resets::View::default(), "attempts":[]
        })).unwrap();
        atomic_write(&shared.join("claude-reset-refresh.json"), &"shared-marker").unwrap();
        let app = Service::open_with_reset_dir(&nightly, &shared).unwrap();
        assert_eq!(app.resets.marker(), resets::Service::new(&shared).marker());
        assert!(nightly.join("engine-state/claude-quota.json").exists());
        assert!(!shared.join("engine-state").exists());
        assert!(!nightly.join("claude-reset-offers.json").exists());
    }

    use serde_json::json;
    pub(crate) struct Scratch(pub PathBuf);
    impl Scratch {
        pub fn new() -> Self {
            let path = std::env::temp_dir().join(format!("richos-quota-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        pub fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _best_effort = fs::remove_dir_all(&self.0);
        }
    }
    const NOW: u64 = 1_000_000_000;
    fn snapshot(used: f64, left: u64) -> Snapshot {
        Snapshot {
            windows: vec![Window {
                id: "five_hour".into(),
                label: "Five-hour".into(),
                used_percent: used,
                resets_at: Some(NOW + left),
                duration_ms: 18_000_000,
            }],
            checked_at: Some(NOW),
            ..Default::default()
        }
    }
    fn policy() -> Policy {
        Policy {
            enabled: true,
            ..Default::default()
        }
    }
    /// **The CEO, 2026-10-07**: the weekly switch point reacts to live token use before the
    /// weekly percentage moves. The app learns Opus's tokens per point from its own readings
    /// (51% to 52% took 1,000,000 Opus tokens), then 2,000,000 Opus tokens in the last minute
    /// are 2 points a minute while the window still reads 52%.
    #[test]
    fn a_burst_of_opus_tokens_moves_the_weekly_switch_point_before_the_percentage_moves() {
        let root = Scratch::new();
        let mut service = Service::open(&root.path().join("app")).unwrap();
        let folder = root.path().join("claude");
        service.transcripts_one = Some(folder.clone());
        let transcript = folder.join("projects").join("p").join("session.jsonl");
        fs::create_dir_all(transcript.parent().unwrap()).unwrap();
        let now = crate::util::now_millis();
        let (minute, hour) = (60_000, 3_600_000);
        let append = |id: &str, at: u64, tokens: u64| {
            use io::Write;
            let t = time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(at) * 1_000_000).unwrap();
            let stamp = format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z", t.year(), u8::from(t.month()), t.day(),
                t.hour(), t.minute(), t.second(), t.millisecond());
            let line = json!({"type": "assistant", "timestamp": stamp, "message": {"id": id, "model": "claude-opus-5-5",
                "usage": {"input_tokens": 0, "output_tokens": 1_000, "cache_creation_input_tokens": 0,
                    "cache_read_input_tokens": tokens - 1_000}}});
            // Claude Code writes one message several times; it counts once.
            let mut file = fs::OpenOptions::new().create(true).append(true).open(&transcript).unwrap();
            writeln!(file, "{line}\n{line}").unwrap();
        };
        let week = |used: f64, at: u64| (vec![Window { id: "seven_day".into(), label: "Weekly".into(), used_percent: used,
            resets_at: Some(now + 72 * hour), duration_ms: 168 * hour }], at);
        let one = crate::claude_accounts::ACCOUNT_ONE;
        service.before_turn(one, Some(week(50., now - 3 * hour)));
        service.before_turn(one, Some(week(51., now - 2 * hour)));
        append("msg_1", now - 90 * minute, 1_000_000);
        service.before_turn(one, Some(week(52., now - hour)));
        let before = service.view();
        assert_eq!(before.act_at["seven_day"], 99., "a point an hour switches at 99%");
        assert!(root.path().join("app").join(TOKEN_POINTS_FILE).exists(), "the tokens per point are kept");
        // The burst: still 52%.
        append("msg_2", now - 20_000, 2_000_000);
        service.before_turn(one, Some(week(52., now - 1_000)));
        let after = service.view();
        assert_eq!(after.windows[0].used_percent, 52.);
        let per_minute = after.speeds["seven_day"] * minute as f64;
        assert!((per_minute - 2.).abs() < 1e-9, "2,000,000 / 1,000,000 tokens a point = 2 points a minute, got {per_minute}");
        assert!(after.act_at["seven_day"] < 99., "the switch point moved: {}", after.act_at["seven_day"]);
    }

    /// **The spend counts both sources once** (§118): a request in Account 1's transcripts and a
    /// request a lease streamed (no transcript holds it), each priced at its own model's rates;
    /// the quota's own reader still gets the lease's tokens, from its own queue.
    #[test]
    fn the_spend_counts_the_transcripts_and_the_leases_requests() {
        let root = Scratch::new();
        let mut service = Service::open(&root.path().join("app")).unwrap();
        let folder = root.path().join("claude");
        service.transcripts_one = Some(folder.clone());
        let transcript = folder.join("projects").join("p").join("session.jsonl");
        fs::create_dir_all(transcript.parent().unwrap()).unwrap();
        // report.md's worked example: Opus 5.5, $0.0944854.
        let row = json!({"type": "assistant", "timestamp": "2026-10-10T04:46:13.800Z", "effort": "high",
            "message": {"id": "msg_t", "model": "claude-opus-5-5", "usage": {"input_tokens": 2, "output_tokens": 51,
                "cache_read_input_tokens": 447_967, "cache_creation_input_tokens": 483,
                "cache_creation": {"ephemeral_5m_input_tokens": 0, "ephemeral_1h_input_tokens": 483}}}});
        fs::write(&transcript, format!("{row}\n{row}\n")).unwrap();
        // A lease's Sonnet 5.5 frame: 1,000 x $2 + 2,000 x $10 + 50,000 x $0.10 + 4,000 x $2.50 = $0.037.
        service.token_sink().frame("1", &json!({"type": "assistant", "message": {"id": "msg_l", "model": "claude-sonnet-5-5",
            "usage": {"input_tokens": 1_000, "output_tokens": 2_000, "cache_read_input_tokens": 50_000,
                "cache_creation_input_tokens": 4_000}}}), crate::util::now_millis());
        // The first call answers at once, counting, and reads on its own thread.
        let first = service.spend_view(0);
        let waited = std::time::Instant::now();
        while !service.spend.ready() {
            assert!(waited.elapsed() < std::time::Duration::from_secs(30), "the first read never finished");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!first.ready, "the first call answers at once, counting, while the history is read");
        let view = service.spend_view(0);
        assert!(view.ready);
        assert_eq!(view.totals.requests, 2);
        assert!((view.totals.cost - (0.0944854 + 0.037)).abs() < 1e-12, "{}", view.totals.cost);
        assert_eq!(service.sink.take().len(), 1, "the quota's reader keeps its own copy of the lease's tokens");
        let live = service.spend_live();
        assert_eq!(live.counted, 2);
        let last = live.last.unwrap();
        assert_eq!((last.name.as_str(), last.tokens), ("Sonnet 5.5", 57_000));
        assert!((last.cost.unwrap() - 0.037).abs() < 1e-12);
    }

    /// **A normal quit keeps every request** (the review of 2dd1375d7, rv-20261011T085659Z: "CLEAN
    /// QUIT: before=$0.1889708/2 requests, reopen=$0.0944854/1"). A lease's requests are in no
    /// transcript, so the ledger is the only place they live, and a token scan saves it at most
    /// every 30 seconds. Two Opus 5.5 lease requests (report.md's worked example, $0.0944854 each),
    /// a scan after each, the second within 30 seconds of the first save; then the app quits
    /// (the service's own quit path, as main.rs calls it on exit) and opens again. By hand:
    /// 2 requests, 2 x $0.0944854 = $0.1889708.
    #[test]
    fn a_normal_quit_keeps_the_leases_requests_the_last_scan_did_not_save() {
        let root = Scratch::new();
        let app = root.path().join("app");
        let frame = |id: &str| json!({"type": "assistant", "message": {"id": id, "model": "claude-opus-5-5",
            "usage": {"input_tokens": 2, "output_tokens": 51, "cache_read_input_tokens": 447_967, "cache_creation_input_tokens": 483,
                "cache_creation": {"ephemeral_5m_input_tokens": 0, "ephemeral_1h_input_tokens": 483}}}});
        {
            let service = Service::open(&app).unwrap();
            service.token_sink().frame("1", &frame("msg_a"), crate::util::now_millis());
            service.track_tokens();
            service.token_sink().frame("1", &frame("msg_b"), crate::util::now_millis());
            service.track_tokens(); // within 30 seconds of the first save: not written
            assert_eq!(service.spend.totals(0).requests, 2, "both are counted while the app runs");
            service.shutdown();
        }
        let again = Service::open(&app).unwrap();
        again.spend_view(0);
        let waited = std::time::Instant::now();
        while !again.spend.ready() {
            assert!(waited.elapsed() < std::time::Duration::from_secs(30), "the first read never finished");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let t = again.spend_view(0).totals;
        assert_eq!(t.requests, 2, "reopened with {} request(s), ${:.7}; by hand 2, $0.1889708", t.requests, t.cost);
        assert!((t.cost - 0.1889708).abs() < 1e-12, "reopened at ${:.7}, by hand $0.1889708", t.cost);
    }

    /// **An old figure is never current** (handoff round 2, run 10: Work read 0.0% weekly for
    /// 27 minutes while its use rose, every check since answering null). Opus at 1,000,000
    /// tokens a point is learned from the app's own readings, the last good reading says 52%
    /// 20 minutes ago, and 3,000,000 Opus tokens were spent 10 minutes ago (outside the live
    /// minute, so no speed sees them). While the reading is older than the normal check, the
    /// decision reads 52 + 3 = 55%: past the test cut-off 54, so the account is being left. The
    /// figure shown stays 52%, with its age (`checked_at`) and as stale. A fresh reading of 53%
    /// is current again and moves nowhere: 53 is under 54.
    #[test]
    fn an_old_weekly_figure_is_moved_forward_by_the_tokens_since_it_and_shown_with_its_age() {
        let root = Scratch::new();
        let mut service = Service::open(&root.path().join("app")).unwrap();
        service.weekly_cutoff = parse_weekly_cutoff("1:54");
        let folder = root.path().join("claude");
        service.transcripts_one = Some(folder.clone());
        let transcript = folder.join("projects").join("p").join("session.jsonl");
        fs::create_dir_all(transcript.parent().unwrap()).unwrap();
        let now = crate::util::now_millis();
        let (minute, hour) = (60_000, 3_600_000);
        let append = |id: &str, at: u64, tokens: u64| {
            use io::Write;
            let t = time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(at) * 1_000_000).unwrap();
            let stamp = format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z", t.year(), u8::from(t.month()), t.day(),
                t.hour(), t.minute(), t.second(), t.millisecond());
            let line = json!({"type": "assistant", "timestamp": stamp, "message": {"id": id, "model": "claude-opus-5-5",
                "usage": {"input_tokens": 0, "output_tokens": 1_000, "cache_creation_input_tokens": 0,
                    "cache_read_input_tokens": tokens - 1_000}}});
            let mut file = fs::OpenOptions::new().create(true).append(true).open(&transcript).unwrap();
            writeln!(file, "{line}").unwrap();
        };
        let week = |used: f64, at: u64| (vec![Window { id: "seven_day".into(), label: "Weekly".into(), used_percent: used,
            resets_at: Some(now + 72 * hour), duration_ms: 168 * hour }], at);
        let one = crate::claude_accounts::ACCOUNT_ONE;
        service.before_turn(one, Some(week(50., now - 3 * hour)));
        service.before_turn(one, Some(week(51., now - 2 * hour)));
        append("msg_1", now - 90 * minute, 1_000_000);
        service.before_turn(one, Some(week(52., now - hour)));
        service.before_turn(one, Some(week(52., now - 20 * minute)));
        append("msg_2", now - 10 * minute, 3_000_000);
        // Every check since came back null: no reading reaches the service.
        service.before_turn(one, None);
        let view = service.view();
        assert_eq!(view.windows[0].used_percent, 52., "the figure shown is the one read");
        assert_eq!(view.checked_at, Some(now - 20 * minute), "and it carries its age");
        assert_eq!(view.state, State::Stale);
        assert_eq!(view.leaving, vec![one.to_string()], "52% read 20 minutes ago + 3 points spent since = 55%, past 54");
        service.before_turn(one, Some(week(53., now)));
        assert!(service.view().leaving.is_empty(), "a fresh 53% is current and is not moved");
    }

    /// **An account the app has never read is asked again soon** (handoff round 2, runs 2-4
    /// and 6: null answers starved Work's first reading for the whole run, and an account with
    /// no reading is never switched to). While there is no reading yet, a null answer is asked
    /// again 30 seconds later, then 1, 2 minutes, and never sooner than 2 minutes after that:
    /// well under the 5-minute schedule and at most one ask in 30 seconds. Once a reading has
    /// arrived, a null answer waits for the normal schedule again, as before.
    #[test]
    fn a_null_answer_before_the_first_reading_is_asked_again_soon_and_bounded() {
        let mut s = Snapshot::default();
        let mut at = NOW;
        for wait in [30_000, 60_000, 120_000, 120_000, 120_000] {
            s.empty(ReadError::NoReading, at);
            let next = s.view(policy(), at).next_check_at.expect("a next check");
            assert_eq!(next - at, wait, "no reading yet: asked again after {wait} ms");
            at = next;
        }
        s.accept(snapshot(20., 3_600_000).windows, at);
        s.empty(ReadError::NoReading, at + 1_000);
        assert_eq!(s.view(policy(), at + 1_000).next_check_at, Some(at + 1_000 + REFRESH_INTERVAL_MS),
            "with a reading in hand, the normal schedule");
    }

    /// **The view says whether the account is signed in, and only what was established**: not
    /// known before the first check, signed out from Claude Code's own `auth status`
    /// (`probe.rs`), signed in once a reading has arrived. The account card draws its verdict
    /// from this, never "No reading yet" (`ui/accounts.js`).
    #[test]
    fn the_view_carries_the_sign_in_the_last_check_established() {
        let mut s = Snapshot::default();
        assert_eq!(s.view(policy(), NOW).signed_in, None, "nothing checked yet");
        s.empty(ReadError::SignedOut, NOW);
        let view = s.view(policy(), NOW);
        assert_eq!(view.signed_in, Some(false));
        assert_eq!(view.message.as_deref(), Some("This Claude account is not signed in on this Mac."));
        s.empty(ReadError::Unsupported, NOW + 1);
        assert_eq!(s.view(policy(), NOW + 1).signed_in, Some(false),
            "no limits alone says nothing new about the sign-in, so the last verdict stands");
        s.empty(ReadError::SignedInNoLimits, NOW + 1);
        let view = s.view(policy(), NOW + 1);
        assert_eq!(view.signed_in, Some(true), "a signed-in verdict stands without a usage reading");
        assert!(view.windows.is_empty());
        s.accept(snapshot(20., 3_600_000).windows, NOW + 2);
        assert_eq!(s.view(policy(), NOW + 2).signed_in, Some(true));
    }

    /// **A sign-in verdict is never lost because one later check failed** (mid-job review
    /// rv-20261021T233844Z-10df0b1f-1846, finding 1): when the verdict lived only in the last
    /// check's error, one check where Claude Code did not answer replaced it with `Failed`, the
    /// verdict dropped to unknown and the card offered Sign in to a signed-in account. Only an
    /// answer about the sign-in (signed out, signed in without limits, a reading) changes it.
    #[test]
    fn a_failed_check_never_erases_what_the_last_answer_said_about_the_sign_in() {
        for (verdict, want) in [(ReadError::SignedOut, Some(false)), (ReadError::SignedInNoLimits, Some(true))] {
            let mut s = Snapshot::default();
            s.empty(verdict, NOW);
            for (i, later) in [ReadError::Failed, ReadError::Malformed, ReadError::NoReading, ReadError::Unsupported].into_iter().enumerate() {
                let at = NOW + 1 + i as u64;
                s.empty(later, at);
                assert_eq!(s.view(policy(), at).signed_in, want, "{verdict:?} then {later:?}");
            }
        }
        let mut s = Snapshot::default();
        s.accept(snapshot(20., 3_600_000).windows, NOW);
        s.empty(ReadError::Failed, NOW + 1);
        assert_eq!(s.view(policy(), NOW + 1).signed_in, Some(true), "a reading, then a failed check");
        s.empty(ReadError::SignedOut, NOW + 2);
        assert_eq!(s.view(policy(), NOW + 2).signed_in, Some(false), "a signed-out answer still changes it");
    }

    #[test]
    fn weekly_hold_ends_with_its_window_and_does_not_mask_the_five_hour_decision() {
        let mut state = snapshot(100., RESET_EXEMPTION_MS - 1);
        state.windows.push(Window { id: "seven_day".into(), label: "Weekly".into(),
            used_percent: 99., resets_at: Some(NOW + 600_000), duration_ms: 604_800_000 });
        assert_eq!(state.view(policy(), NOW).admission, Admission::Held { resets_at: NOW + 600_000 });
        // An old high reading also holds; time passing alone cannot invent allowance.
        assert!(matches!(state.view(policy(), NOW + 300_000).admission, Admission::Held { .. }));
        assert!(state.view(policy(), NOW + 600_000).admission.allows_work());
        state.windows[1].resets_at = None;
        assert!(state.view(policy(), NOW).admission.allows_work());
        let fresh_five = snapshot(10., 3_600_000).windows;
        state.accept(fresh_five, NOW);
        assert!(state.view(policy(), NOW).admission.allows_work());
        assert_eq!(state.view(policy(), NOW).state, State::Stale);
        // A current weekly reading still applies independently of the five-hour rule.
        state.windows[1].resets_at = Some(NOW + 600_000);
        state.windows[0] = snapshot(100., RESET_EXEMPTION_MS - 1).windows.remove(0);
        state.error = None;
        state.windows[1].used_percent = 98.99;
        assert_eq!(state.view(policy(), NOW).admission, Admission::Ready);
        state.windows[0].resets_at = Some(NOW + 3_600_000);
        assert!(matches!(state.view(policy(), NOW).admission, Admission::Held { .. }));
    }

    #[test]
    fn threshold_and_twenty_minute_boundaries() {
        assert_eq!(
            snapshot(92.99, RESET_EXEMPTION_MS)
                .view(policy(), NOW)
                .admission,
            Admission::Ready
        );
        assert!(matches!(
            snapshot(93., RESET_EXEMPTION_MS)
                .view(policy(), NOW)
                .admission,
            Admission::Held { .. }
        ));
        assert_eq!(
            snapshot(100., RESET_EXEMPTION_MS - 1)
                .view(policy(), NOW)
                .admission,
            Admission::Ready
        );
        assert_eq!(
            snapshot(93., 0).view(policy(), NOW).admission,
            Admission::Unknown
        );
        assert_eq!(
            snapshot(100., 0).view(Policy::default(), NOW).admission,
            Admission::Disabled
        );
        assert_eq!(
            snapshot(100., 1).view(policy(), NOW - 1).admission,
            Admission::Unknown
        );
    }
    #[test]
    fn five_minute_polling_at_every_usage_level_and_reset_deadline() {
        for used in [0., 69.99, 70., 93., 100.] {
            let interval = 300_000;
            let s = snapshot(used, 18_000_000);
            assert_eq!(s.view(policy(), NOW).next_check_at, Some(NOW + interval));
            assert_eq!(s.view(policy(), NOW + interval - 1).state, State::Fresh);
            assert_eq!(
                s.view(policy(), NOW + interval).state,
                State::Stale
            );
        }
        assert_eq!(
            snapshot(20., 60_000).view(policy(), NOW).next_check_at,
            Some(NOW + 60_000)
        );
    }
    #[test]
    fn stale_high_reading_holds_then_exempts_without_inventing_a_reset() {
        let mut s = snapshot(94., 3_600_000);
        s.error = Some(ReadError::Failed);
        assert!(matches!(
            s.view(policy(), NOW).admission,
            Admission::Held { .. }
        ));
        assert_eq!(
            s.view(policy(), NOW + 2_400_001).admission,
            Admission::Ready
        );
        assert_eq!(
            s.view(policy(), NOW + 3_600_000).admission,
            Admission::Unknown
        );
        s.windows[0].resets_at = None;
        assert_eq!(s.view(policy(), NOW).admission, Admission::Unknown);
    }
    #[test]
    fn normalization_ignores_billing_and_rejects_invalid_percentages() {
        let response = json!({"rate_limits_available":true,"session":{"secret":"never retained"},"rate_limits":{
            "five_hour":{"utilization":93,"resets_at":"2026-09-25T12:00:00Z"},
            "seven_day":{"utilization":101}, "extra_usage":{"utilization":45,"spend":100},
            "model_scoped":[{"display_name":"Fable","utilization":25,"resets_at":"bad"}]}});
        let windows = normalize(&response).unwrap();
        assert_eq!(windows.len(), 2);
        assert!(windows[0].resets_at.is_some());
        assert_eq!(windows[1].resets_at, None);
        assert_eq!(windows[1].label, "Weekly · Fable");
        assert!(!serde_json::to_string(&windows).unwrap().contains("secret"));
        assert_eq!(
            normalize(&json!({"rate_limits_available":false})),
            Err(ReadError::Unsupported)
        );
        assert_eq!(
            normalize(&json!({"rate_limits_available":true,"rate_limits":{}})),
            Err(ReadError::Malformed)
        );
    }
    // ---- §108 and plan §15 answers 10-11: the speed of use --------------------------------

    fn five_hour_at(used: f64) -> Vec<Window> {
        snapshot(used, 3 * 3_600_000).windows
    }
    /// Both windows: five-hour at `five`%, weekly at `weekly`% (reset in two days).
    fn both_at(five: f64, weekly: f64) -> Vec<Window> {
        let mut windows = five_hour_at(five);
        windows.push(Window { id: "seven_day".into(), label: "Weekly".into(), used_percent: weekly,
            resets_at: Some(NOW + 48 * 3_600_000), duration_ms: 168 * 3_600_000 });
        windows
    }

    /// **§108: when usage speeds up a lot, the quota is checked every minute instead of 5**
    /// (answer 10 allows a minute; `FAST_REFRESH_INTERVAL_MS` carries the read-cost
    /// measurement). "A lot" is 5 points or more per five-minute check (1 point a minute, 3x a
    /// five-hour window's even pace). The 2026-09-29 run measured 3.7 to 4 a minute: 7% -> 11%
    /// in one minute here. Ordinary use measured about 0.33 a minute: 50% -> 50.33% is not fast.
    #[test]
    fn fast_usage_is_checked_every_minute_and_normal_usage_every_five() {
        let mut fast = Snapshot::default();
        fast.accept(five_hour_at(7.), NOW);
        fast.accept(five_hour_at(11.), NOW + 60_000);
        let view = fast.view(policy(), NOW + 60_000);
        assert_eq!(view.refresh_interval_ms, FAST_REFRESH_INTERVAL_MS);
        assert_eq!(view.next_check_at, Some(NOW + 60_000 + 60_000));
        assert_eq!(view.state, State::Fresh);

        let mut normal = Snapshot::default();
        normal.accept(five_hour_at(50.), NOW);
        normal.accept(five_hour_at(50.33), NOW + 60_000);
        let view = normal.view(policy(), NOW + 60_000);
        assert_eq!(view.refresh_interval_ms, REFRESH_INTERVAL_MS);
        assert_eq!(view.next_check_at, Some(NOW + 60_000 + 5 * 60_000));
        // A reading of a NEW window (another reset time) starts again with no speed.
        normal.accept(snapshot(1., 5 * 3_600_000).windows, NOW + 120_000);
        assert!(normal.speeds.is_empty());
    }

    /// **§108's case, at its MEASURED speed: 15 parallel Fable workers.** Reed's reading of the
    /// 2026-09-29 run (richos-hq `docs/research/2026-10-04-fifteen-fable-workers-quota-burn.md`):
    /// the five-hour window went 7% -> 100% in about 25 minutes, 3.7 to 4 points a minute,
    /// about 11x ordinary use (0.33 a minute); 93% was crossed between two five-minute checks
    /// and the first alarm came at 98%. At 4 a minute, checked every minute, the 93% point
    /// already lands before the wall: the last check under it (92.9%) is followed one minute
    /// later by one that reads at most 96.9% and holds. At twice that speed (8 a minute) the
    /// point moves to 100 - 8 = 92%, so the hold comes at 92.5%, before 93% — and the
    /// separate-process gate, reading the published file, agrees.
    #[test]
    fn a_fast_five_hour_burn_holds_before_100_and_before_93_when_the_speed_demands_it() {
        let mut s = Snapshot::default();
        s.accept(five_hour_at(88.9), NOW);
        s.accept(five_hour_at(92.9), NOW + 60_000);
        assert!(s.reading().fast());
        assert_eq!(s.view(policy(), NOW + 60_000).act_at["five_hour"], 93.);
        assert_eq!(s.view(policy(), NOW + 60_000).admission, Admission::Ready);
        assert!(s.reading().projected(&s.windows[0]) < 100., "the next check comes before the wall");
        s.accept(five_hour_at(96.9), NOW + 120_000);
        assert!(matches!(s.view(policy(), NOW + 120_000).admission, Admission::Held { .. }), "held at 96.9%, never at 100%");

        let mut double = Snapshot::default();
        double.accept(five_hour_at(84.5), NOW);
        double.accept(five_hour_at(92.5), NOW + 60_000);
        let view = double.view(policy(), NOW + 60_000);
        assert!((view.act_at["five_hour"] - 92.).abs() < 1e-9, "{:?}", view.act_at);
        assert!(matches!(view.admission, Admission::Held { .. }), "held at 92.5%, before 93%");
        let dir = Scratch::new();
        let state = dir.path().join("engine-state");
        atomic_write(&state.join("claude-quota.json"), &view).unwrap();
        atomic_write(&dir.path().join("claude-quota-policy.json"), &policy()).unwrap();
        assert!(matches!(gate::admission(&state, NOW + 60_000), Admission::Held { .. }));
    }

    /// D14: a jump from 20% to 95% between two quick readings must not print a negative line.
    #[test]
    fn a_huge_jump_between_two_readings_never_puts_the_line_below_zero() {
        let mut s = Snapshot::default();
        s.accept(five_hour_at(20.), NOW);
        s.accept(five_hour_at(95.), NOW + 30_000);
        let view = s.view(policy(), NOW + 30_000);
        let at = view.act_at["five_hour"];
        assert!((0. ..=100.).contains(&at), "line was {at}");
        assert!(matches!(view.admission, Admission::Held { .. }), "it still holds at once");
    }

    /// **One window read two ways is still one window.** The probe's reset time is an ISO
    /// string and the stream's whole seconds, so the same window can differ by a fraction of
    /// a second; a speed must still be measured across them (it was never measured across a
    /// probe and a streamed reading before this tolerance, found on the guest walk).
    #[test]
    fn a_probe_reading_and_a_streamed_reading_of_one_window_measure_a_speed() {
        let mut s = Snapshot::default();
        s.accept(five_hour_at(50.), NOW);
        let mut streamed = five_hour_at(54.);
        streamed[0].resets_at = streamed[0].resets_at.map(|t| t + 400);
        s.observe(&streamed, NOW + 60_000);
        assert!(s.reading().fast(), "4 points in a minute across probe and stream: {:?}", s.speeds);
        // A genuinely new window (an hour later) is not the same one.
        let mut next = five_hour_at(1.);
        next[0].resets_at = next[0].resets_at.map(|t| t + 3_600_000);
        s.observe(&next, NOW + 120_000);
        assert!(s.speeds.is_empty());
    }

    /// **Plan §15 answer 10, point 3: a measured jump moves BOTH check points at once**, and the
    /// published view (the panel's) carries them. Five-hour gaining 8 a minute: 100 - 8 = 92
    /// (from 93). Weekly gaining 2 a minute, checked every minute: the next check and a
    /// 5-minute handoff (the default until one is measured) use 2 x (1 + 5) = 12 points, so the
    /// weekly handoff point is 99 - (12 - 1) = 88, and 88 + 12 = 100 (weekly-switch plan §1;
    /// 78 while the guessed handoff was 10 minutes). Both windows' rise into 40% and 60% was
    /// seen a minute before, so their whole points are measured as they are.
    #[test]
    fn a_measured_jump_moves_both_check_points_and_the_view_shows_them() {
        let mut s = Snapshot::default();
        s.accept(both_at(39., 59.), NOW - 60_000);
        s.accept(both_at(40., 60.), NOW);
        assert_eq!(s.view(policy(), NOW).act_at, [("five_hour".to_string(), 93.), ("seven_day".to_string(), 99.)].into());
        s.accept(both_at(48., 62.), NOW + 60_000);
        let act = s.view(policy(), NOW + 60_000).act_at;
        assert!((act["five_hour"] - 92.).abs() < 1e-9 && (act["seven_day"] - 88.).abs() < 1e-9, "{act:?}");
    }

    // ---- the weekly switch handoff (richos-hq docs/plans/2026-10-07-weekly-switch-handoff.md) --

    /// **Plan §1, the weekly handoff point moves as early as the measured speed requires**
    /// (his words, 2026-10-07: *"the percentage for the switch needs to be adjusted dynamically
    /// so that this doesn't happen"*, "this" being 100% before the switch). With the 5-minute
    /// handoff used until one is measured, `s x (I + H)` is the speed times 10 minutes when
    /// checked every 5, and times 6 when the speed is fast enough to check every minute:
    /// - 0.05 a minute: 0.05 x 10 = 0.5 points, 1 or less, so 99;
    /// - 0.15 a minute: 0.15 x 10 = 1.5 points, so 98;
    /// - 0.25 a minute: 0.25 x 10 = 2.5 points, so 97;
    /// - 1 point in 3 minutes, its rise into 60% seen a minute earlier: (1 / 3) x 10 = 3.3
    ///   points, so 96 (`one_whole_point_after_a_steady_reading_is_not_an_abnormal_speed` is
    ///   the same point timed from the last of many equal readings, which is no speed);
    /// - 4 a minute (fast, checked every minute): 4 x 6 = 24 points, so 76.
    ///
    /// At every speed the point plus `s x (I + H)` is at or under 100: the next check and the
    /// handoff end by 100%. The published point and the weekly hold agree: at the point the
    /// view holds, a tenth under it the view admits.
    #[test]
    fn the_weekly_handoff_point_moves_as_early_as_the_measured_speed_requires() {
        for (gain, minutes, interval, point) in [
            (0.05, 1, REFRESH_INTERVAL_MS, 99.),
            (0.15, 1, REFRESH_INTERVAL_MS, 98.),
            (0.25, 1, REFRESH_INTERVAL_MS, 97.),
            (1., 3, REFRESH_INTERVAL_MS, 96.),
            (4., 1, FAST_REFRESH_INTERVAL_MS, 76.),
        ] {
            let mut s = Snapshot::default();
            s.accept(both_at(39., 59.), NOW - 60_000);
            s.accept(both_at(40., 60.), NOW);
            let at = NOW + minutes * 60_000;
            s.accept(both_at(40., 60. + gain), at);
            let view = s.view(policy(), at);
            assert_eq!(view.refresh_interval_ms, interval, "{gain} in {minutes} min");
            assert!((view.act_at["seven_day"] - point).abs() < 1e-9, "{gain} in {minutes} min: {:?}", view.act_at);
            assert_eq!(view.act_at["five_hour"], 93., "the five-hour point is untouched");
            let reading = s.reading();
            let wait_and_handoff = reading.speeds["seven_day"] * (reading.interval() + HANDOFF_DEFAULT_MS) as f64;
            assert!(point + wait_and_handoff <= 100. + 1e-9, "{gain} in {minutes} min: {point} + {wait_and_handoff} passes 100");
            for (used, held) in [(point - 0.1, false), (point, true)] {
                s.windows[1].used_percent = used;
                let admission = s.view(policy(), at).admission;
                assert_eq!(matches!(admission, Admission::Held { .. }), held, "{gain} in {minutes} min at {used}%: {admission:?}");
            }
        }
    }

    /// **One whole point is not a speed** (nightly 65, 2026-10-18: Work was left at 96% of its
    /// week, not at his 99%). Claude reports the week in whole points. Work read 95% for many
    /// minutes and 96% at 22:48:39Z; the speed was taken from the last 95% reading to the first
    /// 96% one, a minute or two apart, so one point looked like abnormal speed and the point
    /// moved to 96. The real pace was 96% -> 97% in about 74 minutes. A speed is measured from
    /// the first reading of a value; a value whose rise was not seen (the first reading after
    /// launch) may already have been nearly a point higher, so that point is not counted.
    #[test]
    fn one_whole_point_after_a_steady_reading_is_not_an_abnormal_speed() {
        let mut s = Snapshot::default();
        s.accept(both_at(6., 95.), NOW);
        for minute in [4, 8, 12, 16] {
            s.observe(&both_at(6., 95.), NOW + minute * 60_000);
        }
        let at = NOW + 17 * 60_000;
        s.observe(&both_at(6., 96.), at);
        let view = s.view(policy(), at);
        assert_eq!(view.act_at["seven_day"], 99., "{:?} {:?}", view.act_at, s.speeds);
        assert_eq!(view.refresh_interval_ms, REFRESH_INTERVAL_MS);
        // The next point, 74 minutes later, is measured edge to edge: normal speed, still 99.
        s.observe(&both_at(6., 97.), at + 74 * 60_000);
        assert_eq!(s.view(policy(), at + 74 * 60_000).act_at["seven_day"], 99.);
        // A real burn in whole points is still caught once one rise has been seen.
        let mut burn = Snapshot::default();
        burn.accept(both_at(6., 93.), NOW);
        burn.observe(&both_at(6., 94.), NOW + 60_000);
        burn.observe(&both_at(6., 97.), NOW + 120_000);
        let view = burn.view(policy(), NOW + 120_000);
        assert_eq!(view.refresh_interval_ms, FAST_REFRESH_INTERVAL_MS);
        assert!(view.act_at["seven_day"] <= 97., "{:?}", view.act_at);
    }

    /// **A burst after a long unchanged stretch still switches before 100%** (review
    /// rv-20261019T005929Z-e3d9ad03-3e7b, finding 1; its fixture's numbers), through the
    /// switch decision every turn makes (`Service::before_turn`, the lease's streamed reading).
    /// Account 1 reads 90% of its week every five minutes for two hours, then 98% five minutes
    /// later. Timed from the first 90% reading only, that was 8 points in 125 minutes, a normal
    /// speed, so the point stayed 99 and the next reading was 100%. From the last 90% reading it
    /// is at least 7 points in 5 minutes = 1.4 a minute: fast, checked every minute, and the
    /// point is 99 - (ceil(1.4 x 1 + 1.4 x 5) - 1) = 99 - (ceil(8.4) - 1) = 91, so 98 switches.
    #[test]
    #[cfg(unix)]
    fn a_burst_after_two_unchanged_hours_switches_before_the_week_runs_out() {
        let root = Scratch::new();
        let bin = fake_claude(root.path());
        let service = Service::open(root.path()).unwrap();
        let work = service.accounts.add("Work").unwrap();
        usage(&root.path().join("usage-1.json"), 10., 90., "2099-01-05T00:00:00Z");
        usage(&work.folder.clone().unwrap().join("usage.json"), 0., 10., "2099-01-06T00:00:00Z");
        service.set_policy(policy()).unwrap();
        service.refresh(&bin, true);
        assert_eq!(service.accounts.in_use().id, "1");
        let week = |used: f64| vec![Window { id: "seven_day".into(), label: "Weekly".into(), used_percent: used,
            resets_at: Some(4_102_444_800_000), duration_ms: 168 * 3_600_000 }];
        let start = crate::util::now_millis() + 60_000;
        for minute in (0..=120).step_by(5) {
            assert_eq!(service.before_turn("1", Some((week(90.), start + minute * 60_000))), "1", "unchanged at minute {minute}");
        }
        assert_eq!(service.before_turn("1", Some((week(98.), start + 125 * 60_000))), work.id,
            "98% after the burst: {:?}", service.view().act_at);
    }

    /// **A sign-in that just completed is the account's sign-in verdict at once** (review
    /// rv-20261022T035101Z-dd8cd796-0c4b, finding 1): the browser sign-in came back connected,
    /// but only the account list heard it, so until a usage read answered, and for good if that
    /// read failed, the card still said "Not signed in" or offered Sign in. Account 1 (a new
    /// user's own sign-in: a changed connection first clears its record, `set_connecting`) and an
    /// added account both carry `Some(true)` from the moment it completes, and a failed read
    /// after it does not erase it.
    #[test]
    fn a_completed_sign_in_is_the_verdict_at_once_and_a_failed_read_after_it_keeps_it() {
        let root = Scratch::new();
        let service = Service::open(root.path()).unwrap();
        let now = crate::util::now_millis();
        service.snapshot.lock().unwrap().empty(ReadError::SignedOut, now);
        assert_eq!(service.view().signed_in, Some(false), "signed out before the sign-in");
        service.set_connecting(true);
        service.set_connecting(false);
        service.signed_in(crate::claude_accounts::ACCOUNT_ONE);
        assert_eq!(service.view().signed_in, Some(true), "Account 1 right after its sign-in");
        service.snapshot.lock().unwrap().empty(ReadError::Failed, now + 1);
        assert_eq!(service.view().signed_in, Some(true), "Account 1 after a failed read");

        let work = service.accounts.add("Work").unwrap();
        service.signed_in(&work.id);
        let row = |service: &Service| service.view().accounts.into_iter().find(|a| a.id == work.id).unwrap().signed_in;
        assert_eq!(row(&service), Some(true), "an added account right after its sign-in");
        service.extra.lock().unwrap().get_mut(&work.id).unwrap().1.empty(ReadError::Failed, now + 2);
        assert_eq!(row(&service), Some(true), "an added account after a failed read");
    }

    #[test]
    fn the_test_cut_off_is_one_account_and_one_percent() {
        assert_eq!(parse_weekly_cutoff("2:51"), Some(("2".to_string(), 51.)));
        assert_eq!(parse_weekly_cutoff(" 1 : 37.5 "), Some(("1".to_string(), 37.5)));
        for wrong in ["", "51", ":51", "2:", "2:abc", "2:101", "2:-1", "2:NaN"] {
            assert_eq!(parse_weekly_cutoff(wrong), None, "{wrong:?}");
        }
    }

    /// A weekly-and-five-hour reading taken now, its windows resetting in the future.
    fn live(weekly: f64) -> Snapshot {
        let now = crate::util::now_millis();
        Snapshot {
            windows: vec![
                Window { id: "five_hour".into(), label: "Five-hour".into(), used_percent: 10.,
                    resets_at: Some(now + 3_600_000), duration_ms: 18_000_000 },
                Window { id: "seven_day".into(), label: "Weekly".into(), used_percent: weekly,
                    resets_at: Some(now + 48 * 3_600_000), duration_ms: 168 * 3_600_000 },
            ],
            checked_at: Some(now),
            ..Default::default()
        }
    }

    /// **Plan §1 and §4: `leaving` lists only the account past its point, and the test cut-off
    /// applies to one account only.** Two accounts, both at 51% of the week; the app launched
    /// with `RICHOS_TEST_WEEKLY_CUTOFF=2:51`. Only Work (2) is leaving: Account 1 at the same
    /// 51% is not. With Work in use, the switch leaves it for Account 1 at once. Account 1
    /// then at 99% is leaving too, at the ordinary point. One account with automatic pause on
    /// is published as well: leaving at 99%, not at 98.9%.
    #[test]
    fn leaving_lists_only_the_account_past_its_point_and_the_cut_off_is_one_account() {
        let dir = Scratch::new();
        let mut service = Service::open(dir.path()).unwrap();
        service.weekly_cutoff = parse_weekly_cutoff("2:51");
        let work = service.accounts.add("Work").unwrap();
        *service.snapshot.lock().unwrap() = live(51.);
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        service.extra.lock().unwrap().insert(work.id.clone(), (
            std::sync::Arc::new(Mutex::new(Box::new(FakeSource { calls, result: Err(ReadError::Failed) }) as Box<dyn Source>)),
            live(51.)));
        assert_eq!(service.view().leaving, vec![work.id.clone()], "only the cut-off account is past its point");
        assert_eq!(service.view().act_at["seven_day"], 99., "Account 1, in use, keeps 99");
        service.accounts.use_first(&work.id).unwrap();
        assert_eq!(service.view().act_at["seven_day"], 51., "Work, in use, has the cut-off as its point");
        service.decide();
        assert_eq!(service.accounts.in_use().id, crate::claude_accounts::ACCOUNT_ONE, "the switch fires at the same point");
        *service.snapshot.lock().unwrap() = live(99.);
        assert_eq!(service.view().leaving, vec!["1".to_string(), work.id.clone()]);

        let dir = Scratch::new();
        let service = Service::open(dir.path()).unwrap();
        service.set_policy(Policy { enabled: true, pause_percent: 93 }).unwrap();
        *service.snapshot.lock().unwrap() = live(98.9);
        assert!(service.view().leaving.is_empty());
        *service.snapshot.lock().unwrap() = live(99.);
        assert_eq!(service.view().leaving, vec!["1".to_string()], "published with one account too");
    }

    #[test]
    fn faster_weekly_checks_keep_the_cutoff_and_do_not_queue_conversation_updates() {
        let dir = Scratch::new();
        let mut service = Service::open(dir.path()).unwrap();
        service.weekly_cutoff = parse_weekly_cutoff("1:51");
        let mut fast = live(30.);
        fast.speeds.insert("seven_day".into(), 3. / 60_000.);
        *service.snapshot.lock().unwrap() = fast;
        assert_eq!(service.view().act_at["seven_day"], 34.);
        assert_eq!(service.view().refresh_interval_ms, FAST_REFRESH_INTERVAL_MS);
        assert!(service.take_notes().is_empty());
        service.snapshot.lock().unwrap().speeds.clear();
        assert_eq!(service.view().act_at["seven_day"], 51.);
        assert_eq!(service.view().refresh_interval_ms, REFRESH_INTERVAL_MS);
        assert!(service.take_notes().is_empty());
    }

    /// **Plan §15 answer 11: when the speed comes back down, everything resets.** After the
    /// burn above, a reading a minute later that gained 0.3 points: checking is every 5 minutes
    /// again and both check points are 93% and 99% again. A way in needs its way out.
    #[test]
    fn a_return_to_normal_speed_restores_the_five_minute_interval_and_both_normal_check_points() {
        let mut s = Snapshot::default();
        s.accept(both_at(40., 60.), NOW);
        s.accept(both_at(48., 62.), NOW + 60_000);
        assert_eq!(s.view(policy(), NOW + 60_000).refresh_interval_ms, FAST_REFRESH_INTERVAL_MS);
        s.accept(both_at(48.3, 62.02), NOW + 120_000);
        let view = s.view(policy(), NOW + 120_000);
        assert_eq!(view.refresh_interval_ms, REFRESH_INTERVAL_MS);
        assert_eq!(view.next_check_at, Some(NOW + 120_000 + REFRESH_INTERVAL_MS));
        let act = view.act_at;
        assert_eq!((act["five_hour"], act["seven_day"]), (93., 99.), "{act:?}");
    }

    /// **Plan §15 answer 10, point 2: an EXPECTED rise.** Three leases started within a minute
    /// switch checking to every minute before any speed is measured (no speed exists: one
    /// reading only). Three agent dispatches written by the gate do the same on a fresh
    /// service. Ten minutes later, with no measured speed, it is every 5 minutes again.
    #[test]
    fn an_expected_rise_checks_every_minute_before_any_jump_is_measured() {
        for by_dispatch in [false, true] {
            let dir = Scratch::new();
            let service = Service::open(dir.path()).unwrap();
            let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let mut windows = snapshot(20., 3_600_000).windows;
            windows[0].resets_at = Some(crate::util::now_millis() + 3_600_000);
            *service.source.lock().unwrap() = Box::new(FakeSource { calls, result: Ok(windows) });
            service.refresh(Path::new("unused"), true);
            assert_eq!(service.view().refresh_interval_ms, REFRESH_INTERVAL_MS);
            if by_dispatch {
                let now = crate::util::now_millis();
                fs::write(dir.path().join("engine-state").join(DISPATCH_LOG), format!("{now}\n{now}\n{now}\n")).unwrap();
                service.refresh(Path::new("unused"), false);
            } else {
                for _ in 0..3 { service.lease_started(); }
            }
            let view = service.view();
            assert!(view.speeds.is_empty(), "nothing was measured yet");
            assert_eq!(view.refresh_interval_ms, FAST_REFRESH_INTERVAL_MS, "dispatch={by_dispatch}");
            assert_eq!(view.next_check_at, view.checked_at.map(|t| t + FAST_REFRESH_INTERVAL_MS));
            let later = crate::util::now_millis() + EXPECTED_RISE_MS + 1;
            assert_eq!(service.snapshot.lock().unwrap().view(policy(), later).refresh_interval_ms, REFRESH_INTERVAL_MS,
                "the expected rise ends by itself");
        }
    }

    // ---- fill-first: several accounts, read by fake `claude` scripts -------------------

    /// A fake `claude` that answers `get_usage` with the figures in `usage.json` of the
    /// folder it was started under (`CLAUDE_CONFIG_DIR`), or `usage-1.json` beside it for
    /// Account 1, which runs with the app's own environment.
    #[cfg(unix)]
    pub(crate) fn fake_claude(root: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = root.join("claude-fixture");
        fs::write(&path, format!(r#"#!/usr/bin/env python3
import json, os, sys
folder = os.environ.get("CLAUDE_CONFIG_DIR", "")
usage = os.path.join(folder, "usage.json") if "claude-accounts" in folder else "{}"
for line in sys.stdin:
    v = json.loads(line)
    kind = v["request"]["subtype"]
    five, weekly, weekly_reset = json.load(open(usage))
    payload = {{}} if kind == "initialize" else {{"rate_limits_available": True, "rate_limits": {{
        "five_hour": {{"utilization": five, "resets_at": "2099-01-01T00:00:00Z"}},
        "seven_day": {{"utilization": weekly, "resets_at": weekly_reset}}}}}}
    print(json.dumps({{"type": "control_response", "response": {{"subtype": "success", "request_id": v["request_id"], "response": payload}}}}), flush=True)
"#, root.join("usage-1.json").display())).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }
    pub(crate) fn usage(path: &Path, five: f64, weekly: f64, weekly_reset: &str) {
        fs::write(path, serde_json::to_vec(&json!([five, weekly, weekly_reset])).unwrap()).unwrap();
    }

    /// **His answer 2, both settings, on readings taken by the real probe under each folder.**
    /// Account 1's five-hour window is at 93%; Account 2 ("Work") is at 10%. With Pause (the
    /// default) nothing moves: the 93% rule pauses exactly as before. With Switch, Account 2
    /// is put in use, the one-line notice is written once something runs under it, and the
    /// top-level reading every existing
    /// reader uses now describes Account 2.
    #[test]
    #[cfg(unix)]
    fn at_93_percent_of_the_five_hour_window_pause_stays_and_switch_moves_to_the_next_account() {
        let root = Scratch::new();
        let bin = fake_claude(root.path());
        let service = Service::open(root.path()).unwrap();
        let work = service.accounts.add("Work").unwrap();
        usage(&root.path().join("usage-1.json"), 93., 40., "2099-01-05T00:00:00Z");
        usage(&work.folder.clone().unwrap().join("usage.json"), 10., 20., "2099-01-06T00:00:00Z");
        service.set_policy(policy()).unwrap();
        let view = service.refresh(&bin, true);
        assert_eq!(view.at_threshold, crate::claude_accounts::AtThreshold::Pause, "Pause is the default");
        assert_eq!(service.accounts.in_use().id, "1", "Pause never switches");
        assert_eq!(view.accounts.len(), 2);
        assert_eq!(view.accounts[1].windows[0].used_percent, 10., "Account 2 was read under its own folder");
        assert_eq!(view.windows[0].used_percent, 93.);

        let view = service.set_at_threshold(crate::claude_accounts::AtThreshold::Switch).unwrap();
        assert_eq!(service.accounts.in_use().id, work.id, "Switch moves to the account with room");
        assert_eq!(view.windows[0].used_percent, 10., "the published reading is the account in use");
        assert!(view.accounts[1].in_use && !view.accounts[0].in_use);
        assert_eq!(service.accounts.take_notice(), None, "nothing has run under Work yet");
        assert_eq!(view.last_switch, None, "no card before the switch has happened");
        service.accounts.ran_on("reply-on-work", &work.id);
        // Round 18's words. The five-hour reset is 2099, and before the webview has said its
        // offset the span is said, so only the parts that do not move with the clock are exact.
        let line = service.accounts.take_notice().unwrap();
        assert!(line.starts_with("I switched the team to your **Work** account. Account 1 had used 93% of its 5-hour limit, and it is fresh again in "), "{line}");
        assert!(line.ends_with(".\n\nNothing stopped. The team carried on right where it was."), "{line}");
        let view = service.view();
        assert_eq!(view.last_switch.as_ref().map(|s| (s.from.as_str(), s.to.as_str(), s.why.as_str(), s.used)),
            Some(("1", work.id.as_str(), "fiveHour", Some(93.))), "the card after a switch reads it from the view");
        let published: View = gate::read_json(&root.path().join("engine-state/claude-quota.json")).unwrap();
        assert_eq!(published.windows[0].used_percent, 10.);
    }

    /// **Feedback item 8 (the CEO 2026-10-06), on readings taken by the real probe under each
    /// folder.** Account 1 is at 30% of its week; Work is at 97%. He picks Work: it is in use and
    /// the published reading is Work's. The pick survives a relaunch of the service. At 99% the
    /// automatic switch moves back to Account 1. An account with no reading, or with no room,
    /// cannot be picked, and the refusal says why.
    #[test]
    #[cfg(unix)]
    fn he_picks_the_account_that_drains_first_and_it_survives_a_relaunch() {
        let root = Scratch::new();
        let bin = fake_claude(root.path());
        let service = Service::open(root.path()).unwrap();
        let work = service.accounts.add("Work").unwrap();
        let spare = service.accounts.add("Spare").unwrap();
        let work_usage = work.folder.clone().unwrap().join("usage.json");
        usage(&root.path().join("usage-1.json"), 10., 30., "2099-01-05T00:00:00Z");
        usage(&work_usage, 10., 97., "2099-01-06T00:00:00Z");
        service.set_policy(policy()).unwrap();
        service.refresh(&bin, true);
        assert_eq!(service.accounts.in_use().id, "1");
        let refused = service.use_account_first(&spare.id).unwrap_err().to_string();
        assert_eq!(refused, "Spare has no reading yet. Sign it in, then refresh.");
        assert_eq!(service.accounts.in_use().id, "1");

        let view = service.use_account_first(&work.id).unwrap();
        assert_eq!(service.accounts.in_use().id, work.id);
        assert!(view.accounts.iter().any(|a| a.id == work.id && a.in_use));
        assert_eq!(view.windows.iter().find(|w| w.id == "seven_day").unwrap().used_percent, 97., "the published reading is Work's");
        drop(service);

        let service = Service::open(root.path()).unwrap();
        assert_eq!(service.accounts.in_use().id, work.id, "the pick survives a relaunch");
        usage(&work_usage, 10., 99., "2099-01-06T00:00:00Z");
        service.refresh(&bin, true);
        assert_eq!(service.accounts.in_use().id, "1", "at 99% of Work's week the switch moves on");
        let refused = service.use_account_first(&work.id).unwrap_err().to_string();
        assert_eq!(refused, "Work has no room right now, so Rich would switch away from it at once.");
        assert_eq!(service.accounts.in_use().id, "1");
    }

    /// **All accounts exhausted** (Frank's finding 2): held until the soonest reset among
    /// them, in the view and in the separate-process gate, even with the pause switch off.
    /// With one account nothing changes: no hold appears.
    #[test]
    #[cfg(unix)]
    fn when_every_account_is_exhausted_work_is_held_until_the_soonest_reset() {
        let root = Scratch::new();
        let bin = fake_claude(root.path());
        let service = Service::open(root.path()).unwrap();
        service.set_policy(Policy::default()).unwrap();
        let work = service.accounts.add("Work").unwrap();
        usage(&root.path().join("usage-1.json"), 20., 99.5, "2099-01-05T00:00:00Z");
        usage(&work.folder.clone().unwrap().join("usage.json"), 20., 100., "2099-01-03T00:00:00Z");
        let view = service.refresh(&bin, true);
        assert!(!view.policy.enabled, "the pause switch is off");
        let soonest = reset(&json!("2099-01-03T00:00:00Z")).unwrap();
        assert_eq!(view.held_until, Some(soonest));
        assert_eq!(view.admission, Admission::Held { resets_at: soonest });
        // Round 16: Rich says it once, naming the account that comes back first.
        // Before the webview has said its offset, the span is said.
        let notes = service.take_notes();
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].starts_with("Every account is used up. Agents hold their place until Work's window resets in "), "{notes:?}");
        assert!(notes[0].ends_with(", the soonest."), "{notes:?}");
        service.refresh(&bin, true);
        assert!(service.take_notes().is_empty(), "said once while the hold lasts");
        // With the offset and three agents working, round 16's words with the clock time.
        service.set_utc_offset(0);
        service.set_agents_working(3);
        service.was_held.store(false, std::sync::atomic::Ordering::SeqCst);
        service.refresh(&bin, true);
        assert_eq!(service.take_notes(), vec![format!(
            "Every account is used up. 3 agents are holding their place until {}, when Work's window resets, the soonest.",
            clock(soonest, 0, crate::util::now_millis()))]);
        let state = root.path().join("engine-state");
        assert_eq!(gate::admission(&state, crate::util::now_millis()), Admission::Held { resets_at: soonest });
        // One account left: today's behavior, no hold.
        service.remove_account(&work.id).unwrap();
        assert_eq!(service.view().held_until, None);
        assert_eq!(gate::admission(&state, crate::util::now_millis()), Admission::Disabled);
    }

    /// A reading the lease on `account` streamed, `hours` ahead so each one is newer than the
    /// last and none measures as fast.
    fn stream(service: &Service, account: &str, weekly: f64, weekly_reset: &str, hours: u64) {
        let windows = vec![
            Window { id: "five_hour".into(), label: "Five-hour".into(), used_percent: 10., resets_at: reset(&json!("2099-01-01T00:00:00Z")), duration_ms: 5 * 3_600_000 },
            Window { id: "seven_day".into(), label: "Weekly".into(), used_percent: weekly, resets_at: reset(&json!(weekly_reset)), duration_ms: 168 * 3_600_000 },
        ];
        service.before_turn(account, Some((windows, crate::util::now_millis() + hours * 3_600_000)));
    }

    /// **Round 18: Rich suggests a second account, once** (the CEO 2026-10-06, feedback item 9:
    /// "Non-technical users should also be able to take advantage of the multi-account setup").
    /// One account at 85% of its week: nothing. At 86%: the line, with its kind, so the
    /// conversation can put Add a second account and Not now under it. Never again after that,
    /// not even after a relaunch; Not now is remembered and published.
    #[test]
    #[cfg(unix)]
    fn one_account_at_86_percent_of_its_week_rich_suggests_a_second_account_once_ever() {
        let root = Scratch::new();
        let bin = fake_claude(root.path());
        let service = Service::open(root.path()).unwrap();
        service.set_utc_offset(0);
        usage(&root.path().join("usage-1.json"), 10., 85., "2099-01-05T09:00:00Z");
        service.refresh(&bin, true);
        assert!(service.take_account_notes().is_empty(), "85% says nothing");
        stream(&service, "1", 86.4, "2099-01-05T09:00:00Z", 1);
        let notes = service.take_account_notes();
        assert_eq!(notes, vec![("nudge".to_string(),
            "Your Claude account has used **86%** of this week's limit. It is fresh again on Monday at 9:00 AM. \
             If it runs out before then, the team pauses until it is.\n\nIf you have a second Claude account, \
             for example one for work, I can switch to it when this one is nearly full, so the team keeps working.".to_string())]);
        assert!(!notes[0].1.contains('\u{2014}') && !notes[0].1.contains('\u{2013}'), "no m-dash or n-dash");
        stream(&service, "1", 86.6, "2099-01-05T09:00:00Z", 2);
        assert!(service.take_account_notes().is_empty(), "queued once");
        // Queued while no conversation could take it, and lost to a relaunch: queued again,
        // because it is recorded as said only once it is written into the conversation.
        let service = Service::open(root.path()).unwrap();
        stream(&service, "1", 86.8, "2099-01-05T09:00:00Z", 1);
        assert_eq!(service.take_account_notes().len(), 1, "a line never written is queued again");
        service.accounts.noted("nudge", "turn_nudge").unwrap();
        stream(&service, "1", 87., "2099-01-05T09:00:00Z", 2);
        assert!(service.take_account_notes().is_empty(), "said once");
        let service = Service::open(root.path()).unwrap();
        stream(&service, "1", 88., "2099-01-05T09:00:00Z", 1);
        assert!(service.take_account_notes().is_empty(), "said once ever, a relaunch included");
        service.accounts.answer_nudge("notNow").unwrap();
        let view = service.view();
        assert_eq!(view.nudge.as_ref().and_then(|n| n.turn.as_deref()), Some("turn_nudge"));
        assert_eq!(view.nudge.as_ref().and_then(|n| n.answer.as_deref()), Some("notNow"));
        assert_eq!(view.notes, vec![crate::claude_accounts::NoteTurn { turn: "turn_nudge".into(), kind: "nudge".into() }]);
    }

    /// **Round 18: every account close to its weekly limit, said once while it lasts.** Home at
    /// 96% and Work at 93%: the line names both figures and when the soonest is fresh again. A
    /// second check says nothing. Once Work's week is fresh (5%) and then near again, it is said
    /// again. One account under 90% says nothing.
    #[test]
    #[cfg(unix)]
    fn when_every_account_is_near_its_week_rich_says_so_once_while_that_lasts() {
        let root = Scratch::new();
        let bin = fake_claude(root.path());
        let service = Service::open(root.path()).unwrap();
        service.set_utc_offset(0);
        service.accounts.rename("1", "Home").unwrap();
        let work = service.accounts.add("Work").unwrap();
        let work_usage = work.folder.clone().unwrap().join("usage.json");
        usage(&root.path().join("usage-1.json"), 10., 96., "2099-01-03T09:00:00Z");
        usage(&work_usage, 10., 89., "2099-01-04T06:00:00Z");
        service.refresh(&bin, true);
        assert!(service.take_account_notes().is_empty(), "Work at 89% is not near");
        stream(&service, &work.id, 93., "2099-01-04T06:00:00Z", 1);
        assert_eq!(service.take_account_notes(), vec![("bothNear".to_string(),
            "Heads up: both of your Claude accounts are close to their weekly limit. Home has used 96% and Work 93%.\n\n\
             When they run out, the team pauses until Home is fresh again on Saturday at 9:00 AM. Nothing is lost. \
             If you have another Claude account, add it and the team keeps going.".to_string())]);
        service.before_turn("1", None);
        assert!(service.take_account_notes().is_empty(), "said once while it lasts");
        stream(&service, &work.id, 5., "2099-01-11T06:00:00Z", 2);
        assert!(service.take_account_notes().is_empty());
        stream(&service, &work.id, 94., "2099-01-11T06:00:00Z", 3);
        assert_eq!(service.take_account_notes().len(), 1, "near again after it was not: said again");
    }

    #[test]
    fn round_18_says_a_moment_as_today_tomorrow_or_the_weekday() {
        // 2001-09-09T01:46:40Z, a Sunday.
        let now = 1_000_000_000_000;
        assert_eq!(when_long(now + 3_600_000, Some(0), now), "today at 2:47 AM");
        assert_eq!(when_long(now + 24 * 3_600_000, Some(0), now), "tomorrow at 1:47 AM");
        assert_eq!(when_long(now + 72 * 3_600_000, Some(0), now), "on Wednesday at 1:47 AM");
        // Two hours west, now is Saturday 11:46 PM and an hour on is Sunday 12:47 AM.
        assert_eq!(when_long(now + 3_600_000, Some(-120), now), "tomorrow at 12:47 AM", "the webview's offset");
        assert_eq!(when_long(now + 3 * 3_600_000, None, now), "in 3 h", "before the webview has said its offset");
    }

    #[test]
    fn fast_usage_stays_in_the_settings_view_with_the_measured_rise_and_worker_count() {
        let dir = Scratch::new();
        let service = Service::open(dir.path()).unwrap();
        service.set_policy(Policy { enabled: true, pause_percent: 93 }).unwrap();
        service.set_agents_working(15);
        let now = crate::util::now_millis();
        service.snapshot.lock().unwrap().accept(five_hour_at(40.), now - 13 * 60_000);
        service.snapshot.lock().unwrap().accept(five_hour_at(71.), now - 60_000);
        let view = service.view();
        assert_eq!(view.refresh_interval_ms, FAST_REFRESH_INTERVAL_MS);
        assert_eq!(view.agents_working, 15);
        assert_eq!(view.rises.get("five_hour"), Some(&Rise { from: 40., to: 71., ms: 12 * 60_000 }));
        assert!(service.take_notes().is_empty());
    }

    /// Clock times as round 16 writes them (`fmtWhen`), at the webview's offset, rounded to
    /// the minute: a time today, a weekday on any other day.
    #[test]
    fn clock_times_are_said_at_the_webviews_offset() {
        let at = |text: &str| reset(&json!(text)).unwrap();
        let now = at("2026-10-05T00:00:00Z");
        assert_eq!(clock(at("2026-10-05T02:17:00Z"), 0, now), "2:17 AM");
        assert_eq!(clock(at("2026-10-05T02:17:31Z"), 0, now), "2:18 AM", "rounded to the nearest minute");
        assert_eq!(clock(at("2026-10-05T13:05:00Z"), 0, now), "1:05 PM");
        assert_eq!(clock(at("2026-10-05T00:20:00Z"), 0, now), "12:20 AM");
        assert_eq!(clock(at("2026-10-07T00:20:00Z"), 0, now), "Wed 12:20 AM");
        // California (UTC-7): 02:17Z is 7:17 PM on the 4th, the same local day as 00:00Z.
        assert_eq!(clock(at("2026-10-05T02:17:00Z"), -420, now), "7:17 PM");
    }

    #[test]
    fn spans_are_said_the_way_round_16_writes_them() {
        assert_eq!(words(1), "1 min");
        assert_eq!(words(41 * 60_000), "41 min");
        assert_eq!(words(60 * 60_000), "1 h");
        assert_eq!(words(101 * 60_000), "1 h 41 min");
        assert_eq!(words(48 * 3_600_000), "2 d");
        assert_eq!(words(51 * 3_600_000), "2 d 3 h");
    }

    struct FakeSource {
        calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        result: Result<Vec<Window>, ReadError>,
    }
    impl Source for FakeSource {
        fn read(&mut self, _: &Path, _: &Path) -> Result<Vec<Window>, ReadError> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.result.clone()
        }
    }
    #[test]
    fn startup_and_session_signals_refresh_without_waiting_for_the_interval() {
        use std::time::Duration;
        use std::sync::atomic::Ordering;
        let dir = Scratch::new();
        let service = std::sync::Arc::new(Service::open(dir.path()).unwrap());
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut windows = snapshot(20., 3_600_000).windows;
        windows[0].resets_at = Some(crate::util::now_millis() + 3_600_000);
        *service.source.lock().unwrap() = Box::new(FakeSource { calls: calls.clone(), result: Ok(windows) });
        service.set_policy(Policy::default()).unwrap();
        assert!(!service.view().policy.enabled);
        assert!(service.wait_for_refresh(Duration::ZERO), "startup requests a read even with pause off");
        service.refresh(Path::new("unused"), true);
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // The reading is fresh but outside the short duplicate-request cooldown.
        service.snapshot.lock().unwrap().checked_at = Some(crate::util::now_millis() - 10_000);
        service.refresh(Path::new("unused"), false);
        assert_eq!(calls.load(Ordering::SeqCst), 1, "ordinary polling still uses the cache");
        let (tx, rx) = std::sync::mpsc::channel();
        let reader = service.clone();
        let thread = std::thread::spawn(move || {
            let force = reader.wait_for_refresh(Duration::from_secs(30));
            reader.refresh(Path::new("unused"), force);
            tx.send(force).unwrap();
        });
        service.request_refresh();
        // The monitor's answer is the verdict: `true` only if the request woke it, `false` if
        // its own 30 s interval ran out first. So the wait for that answer is a hang guard
        // beyond the interval, not a clock on the wake; it was 2 s, which failed on a busy Mac
        // while the monitor was right (audit R9).
        assert!(rx.recv_timeout(Duration::from_secs(120)).expect("the monitor never answered"),
            "session start wakes the sleeping monitor");
        thread.join().unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2, "session start bypasses the five-minute cache");
        // "Just completed" is the product's 5 s duplicate-request cooldown, measured on the wall
        // clock from the read above. A test thread descheduled for 5 s between that read and
        // the next line saw the cooldown over and a third read, and failed while the service
        // was right (seen under `scripts/qa/stall-run.py`). So the reading is stamped as
        // completed at the moment these two requests arrive, 4 s ahead (the cooldown tolerates a
        // stamp up to 5 s in the future, a small clock step backward, so the margin is 4 s + 5 s): the
        // question is whether simultaneous requests share a just-completed read, not how fast
        // this thread gets from one line to the next.
        service.snapshot.lock().unwrap().checked_at = Some(crate::util::now_millis() + 4_000);
        service.request_refresh(); service.request_refresh();
        let force = service.wait_for_refresh(Duration::ZERO);
        assert!(force);
        service.refresh(Path::new("unused"), force);
        assert_eq!(calls.load(Ordering::SeqCst), 2, "simultaneous sessions share the just-completed read");
        assert!(!service.wait_for_refresh(Duration::ZERO));
        service.set_connecting(true);
        service.wait_for_refresh(Duration::ZERO);
        service.set_connecting(false);
        assert!(service.wait_for_refresh(Duration::ZERO), "sign-in completion wakes the reader");
        service.refresh(Path::new("unused"), true);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }
    /// Hunt part 1 finding 23: a manual refresh is owed a real check even inside the automatic
    /// ten-minute failure backoff, while the automatic one still waits and a double-click
    /// straight after a failure still does not reach the provider.
    #[test]
    fn a_manual_refresh_is_not_held_by_the_automatic_failure_backoff() {
        use std::sync::atomic::Ordering;
        let dir = Scratch::new();
        let service = Service::open(dir.path()).unwrap();
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        *service.source.lock().unwrap() =
            Box::new(FakeSource { calls: calls.clone(), result: Err(ReadError::Failed) });
        assert!(service.refresh(Path::new("unused"), true).retry_at.is_some());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        service.refresh(Path::new("unused"), true);
        assert_eq!(calls.load(Ordering::SeqCst), 1, "a double-click right after a failure is still absorbed");
        // The failure was 10 s ago (its wait is set BACKOFF_MS past the attempt) and he fixed it.
        // The attempt is also recorded as the last empty check (`empty_at`), so both move.
        service.snapshot.lock().unwrap().retry_at = Some(crate::util::now_millis() + BACKOFF_MS - 10_000);
        service.snapshot.lock().unwrap().empty_at = Some(crate::util::now_millis() - 10_000);
        let mut windows = snapshot(20., 3_600_000).windows;
        windows[0].resets_at = Some(crate::util::now_millis() + 3_600_000);
        *service.source.lock().unwrap() = Box::new(FakeSource { calls: calls.clone(), result: Ok(windows) });
        service.refresh(Path::new("unused"), false);
        assert_eq!(calls.load(Ordering::SeqCst), 1, "the automatic check still waits out the backoff");
        let view = service.refresh(Path::new("unused"), true);
        assert_eq!(calls.load(Ordering::SeqCst), 2, "his manual check is a real one");
        assert!(view.retry_at.is_none(), "and its answer replaces the old failure");
    }
    /// Walk of nightly 36, D1: Claude Code 2.1.289 sometimes answers `get_usage` with
    /// `rate_limits_available: true` and `rate_limits: null` (`qa/usage-shape.sh`). That is no
    /// reading this time: nothing reported as unreadable, no ten-minute lock, the next check on
    /// the normal schedule, and with the pause on, new work is not held waiting on it. On main
    /// it was `Malformed`, with `retry_at` ten minutes out (the panel's locked Refresh) and
    /// `Unknown` (new background work held until a reading came).
    #[test]
    fn a_null_usage_answer_is_no_reading_this_time_and_never_locks_or_holds() {
        use std::sync::atomic::Ordering;
        let null = json!({"rate_limits_available": true, "rate_limits": null, "subscription_type": "max"});
        assert_eq!(normalize(&null), Err(ReadError::NoReading));
        // An answer that is genuinely unreadable is still reported as unreadable.
        for bad in [json!({"rate_limits_available": true}), json!({"rate_limits_available": true, "rate_limits": "x"}),
            json!({"rate_limits_available": true, "rate_limits": {}})] {
            assert_eq!(normalize(&bad), Err(ReadError::Malformed), "{bad}");
        }
        let dir = Scratch::new();
        let service = Service::open(dir.path()).unwrap();
        service.set_policy(policy()).unwrap();
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        *service.source.lock().unwrap() = Box::new(FakeSource { calls: calls.clone(), result: normalize(&null) });
        let before = crate::util::now_millis();
        let view = service.refresh(Path::new("unused"), true);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(view.retry_at, None, "no ten-minute lock");
        assert_eq!(view.message, None, "nothing is reported as unreadable");
        assert_eq!(view.state, State::Unavailable, "no figures, so no reading to show");
        let empty = view.empty_at.expect("the empty answer is recorded");
        assert!(empty >= before);
        // No reading yet, so asked again soon (handoff round 2: FIRST_READING_RETRY_MS), not
        // on the 5-minute schedule; with figures in hand, below, the normal schedule.
        assert_eq!(view.next_check_at, Some(empty + FIRST_READING_RETRY_MS), "an account never read is asked again soon");
        assert_eq!(view.admission, Admission::NoReading);
        assert!(view.admission.allows_work(), "a missing reading does not hold new background work");
        // The gate, a separate process, decides the same from the published file.
        assert_eq!(gate::admission(&dir.path().join("engine-state"), crate::util::now_millis()), Admission::NoReading);
        // The automatic check waits for the schedule; no tight loop on null answers.
        service.refresh(Path::new("unused"), false);
        assert_eq!(calls.load(Ordering::SeqCst), 1, "the automatic check waits for the normal interval");
        // Refresh is never locked by it: past the 5 s double-click cooldown a manual check is real.
        service.snapshot.lock().unwrap().empty_at = Some(crate::util::now_millis() - 10_000);
        service.refresh(Path::new("unused"), true);
        assert_eq!(calls.load(Ordering::SeqCst), 2, "a manual Refresh after a null answer reaches Claude Code");
        // One interval after the empty answer, the automatic check asks again by itself.
        service.snapshot.lock().unwrap().empty_at = Some(crate::util::now_millis() - REFRESH_INTERVAL_MS - 1);
        service.refresh(Path::new("unused"), false);
        assert_eq!(calls.load(Ordering::SeqCst), 3, "the normal schedule asks again");

        // With figures from earlier, a null answer keeps them and is not a failure.
        let mut s = snapshot(20., 3_600_000);
        assert_eq!(s.view(policy(), NOW + REFRESH_INTERVAL_MS).admission, Admission::Unknown,
            "the reading just went out of date and its check is on its way: work waits, as round 16 draws");
        s.empty(ReadError::NoReading, NOW + REFRESH_INTERVAL_MS);
        let view = s.view(policy(), NOW + REFRESH_INTERVAL_MS + 1);
        assert_eq!(view.windows.len(), 1, "the last figures are kept");
        assert_eq!((view.retry_at, view.message), (None, None));
        assert_eq!(view.state, State::Stale);
        assert_eq!(view.admission, Admission::NoReading, "that check came back empty: work is not held on it");
        assert_eq!(view.next_check_at, Some(NOW + 2 * REFRESH_INTERVAL_MS));
        // Figures that prove a hold still hold, until their window resets.
        let mut high = snapshot(94., 3_600_000);
        high.empty(ReadError::NoReading, NOW + REFRESH_INTERVAL_MS);
        assert!(matches!(high.view(policy(), NOW + REFRESH_INTERVAL_MS + 1).admission, Admission::Held { .. }));
        // A null answer after a failure clears the failure and its backoff.
        let mut failed = snapshot(20., 3_600_000);
        failed.empty(ReadError::Failed, NOW + 1);
        assert!(failed.view(policy(), NOW + 2).retry_at.is_some());
        failed.empty(ReadError::NoReading, NOW + 2);
        assert_eq!(failed.view(policy(), NOW + 3).retry_at, None);
        // A real reading ends it.
        failed.accept(snapshot(21., 3_600_000).windows, NOW + 4);
        assert_eq!(failed.view(policy(), NOW + 5).empty_at, None);
        assert_eq!(failed.view(policy(), NOW + 5).admission, Admission::Ready);
    }
    #[test]
    fn fresh_install_pauses_at_the_five_hour_line() {
        let dir = Scratch::new();
        let policy = Service::open(dir.path()).unwrap().view().policy;
        assert_eq!((policy.enabled, policy.line()), (true, Some(93)));
    }

    #[test]
    fn cache_failure_backoff_policy_persistence_and_account_invalidation() {
        let dir = Scratch::new();
        let service = Service::open(dir.path()).unwrap();
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut windows = snapshot(69., 3_600_000).windows;
        windows[0].resets_at = Some(crate::util::now_millis() + 3_600_000);
        *service.source.lock().unwrap() = Box::new(FakeSource {
            calls: calls.clone(),
            result: Ok(windows),
        });
        service.refresh(Path::new("unused"), false);
        service.refresh(Path::new("unused"), false);
        service.refresh(Path::new("unused"), true);
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        service.set_policy(policy()).unwrap();
        assert!(Service::open(dir.path()).unwrap().view().policy.enabled);
        service.disconnect();
        assert_eq!(service.view().admission, Admission::Unknown);
        *service.source.lock().unwrap() = Box::new(FakeSource {
            calls: calls.clone(),
            result: Err(ReadError::Failed),
        });
        assert!(service
            .refresh(Path::new("unused"), true)
            .retry_at
            .is_some());
        service.refresh(Path::new("unused"), true);
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert_eq!(service.view().state, State::Unavailable);
        service.set_connecting(true);
        service.refresh(Path::new("unused"), true);
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "sign-in suspends quota reads"
        );
        service.set_connecting(false);
        service.refresh(Path::new("unused"), true);
        assert_eq!(
            calls.load(std::sync::atomic::Ordering::SeqCst),
            3,
            "new connection clears old backoff"
        );
    }

    #[cfg(unix)]
    fn script(path: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        fs::write(path, body).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        path.to_path_buf()
    }
    #[cfg(unix)]
    fn wait_until(what: &str, done: impl Fn() -> bool) {
        // A hang guard on a Python child starting, never a clock on the behavior.
        let until = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while !done() {
            assert!(std::time::Instant::now() < until, "never happened: {what}");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    /// **Hunt part 1 v3, finding 48.** Two overlapping refreshes (the monitor and the manual
    /// refresh both call `refresh`) while an added account's read is in flight. The second
    /// one must never read that account with a reader that is not its own: Account 1 is at
    /// 99.5% weekly, Work at 10%, and Work must stay Work — read under its own folder, not
    /// held — on this refresh and every later one.
    #[test]
    #[cfg(unix)]
    fn overlapping_refreshes_never_read_an_added_account_with_account_ones_reader() {
        let root = Scratch::new();
        let dir = root.path().to_path_buf();
        let bin = script(&dir.join("claude-fixture"), r#"#!/usr/bin/env python3
import json, os, sys, time
root = os.path.dirname(os.path.abspath(__file__))
added = "claude-accounts" in os.environ.get("CLAUDE_CONFIG_DIR", "")
for line in sys.stdin:
    v = json.loads(line)
    payload = {}
    if v["request"]["subtype"] != "initialize":
        if added:
            open(os.path.join(root, "added-started"), "w").close()
            while not os.path.exists(os.path.join(root, "release-added")): time.sleep(0.005)
        else:
            with open(os.path.join(root, "default-reads"), "a") as f: f.write("x\n")
            if len(open(os.path.join(root, "default-reads")).read().splitlines()) == 2:
                open(os.path.join(root, "default-second-started"), "w").close()
                while not os.path.exists(os.path.join(root, "release-default-second")): time.sleep(0.005)
        used = 10 if added else 99.5
        payload = {"rate_limits_available": True, "rate_limits": {
            "five_hour": {"utilization": 10, "resets_at": "2099-01-01T00:00:00Z"},
            "seven_day": {"utilization": used, "resets_at": "2099-01-05T00:00:00Z"}}}
    print(json.dumps({"type": "control_response", "response": {"subtype": "success", "request_id": v["request_id"], "response": payload}}), flush=True)
"#);
        let service = std::sync::Arc::new(Service::open(&dir).unwrap());
        let work = service.accounts.add("Work").unwrap();
        let (one, b1) = (service.clone(), bin.clone());
        let first = std::thread::spawn(move || one.refresh(&b1, true));
        wait_until("Work's read started", || dir.join("added-started").exists());
        let (two, b2) = (service.clone(), bin.clone());
        let second = std::thread::spawn(move || two.refresh(&b2, true));
        wait_until("the second refresh read or finished", || {
            dir.join("default-second-started").exists() || second.is_finished()
        });
        fs::write(dir.join("release-added"), b"").unwrap();
        first.join().unwrap();
        fs::write(dir.join("release-default-second"), b"").unwrap();
        second.join().unwrap();
        let weekly = |view: &View| view.accounts.iter().find(|a| a.id == work.id).unwrap()
            .windows.iter().find(|w| w.id == "seven_day").unwrap().used_percent;
        let view = service.view();
        assert_eq!(weekly(&view), 10., "Work shows Account 1's usage");
        assert_eq!(view.held_until, None, "work is held although Work has room");
        // A later refresh still reads Work under Work's folder.
        let earlier = Some(crate::util::now_millis() - 10_000);
        service.snapshot.lock().unwrap().checked_at = earlier;
        service.extra.lock().unwrap().get_mut(&work.id).unwrap().1.checked_at = earlier;
        let view = service.refresh(&bin, true);
        assert_eq!(weekly(&view), 10., "the wrong reader survived into the next refresh");
        assert_eq!(view.held_until, None);
        service.shutdown();
    }

    /// **Hunt part 1 v3, finding 49.** A usage limit refused Account 1 (the backstop holds it
    /// until 2099), then a reset was used: the marker changed and the fresh reading shows 2%.
    /// The old hold must not outlive the reset that ended it.
    #[test]
    #[cfg(unix)]
    fn a_reset_ends_the_usage_limit_hold_its_fresh_reading_contradicts() {
        let root = Scratch::new();
        let bin = fake_claude(root.path());
        let service = Service::open(root.path()).unwrap();
        let work = service.accounts.add("Work").unwrap();
        usage(&root.path().join("usage-1.json"), 10., 99.5, "2099-01-05T00:00:00Z");
        usage(&work.folder.clone().unwrap().join("usage.json"), 10., 99.5, "2099-01-06T00:00:00Z");
        service.refresh(&bin, true);
        let until = 4_071_254_400_000u64;
        assert_eq!(service.limit_reached("1", Some(until)), crate::claude_accounts::AfterLimit::NoRoom);
        usage(&root.path().join("usage-1.json"), 10., 2., "2099-01-12T00:00:00Z");
        fs::write(root.path().join("claude-reset-refresh.json"), b"{\"outcome\":\"used\"}").unwrap();
        let view = service.refresh(&bin, true);
        assert_eq!(view.accounts[0].windows.iter().find(|w| w.id == "seven_day").unwrap().used_percent, 2.);
        assert_eq!(service.accounts.limited_until("1"), None, "the reset left the old limit in place");
        assert_eq!(view.held_until, None);
        assert!(!matches!(view.admission, Admission::Held { .. }), "{:?}", view.admission);
        service.shutdown();
    }

    /// **Hunt part 1 v3, finding 51.** Quit's stop must interrupt EVERY account's pending
    /// read. Account 1's second read never answers; Work was read after Account 1's reader
    /// started, so a single shared fence pointed at Work and quit waited out the 20 s request
    /// deadline. The bound is the probe's own (`probe.rs`, half of `DEADLINE`).
    #[test]
    #[cfg(unix)]
    fn shutdown_interrupts_account_ones_pending_read_after_another_account_was_read() {
        let root = Scratch::new();
        let dir = root.path().to_path_buf();
        let bin = script(&dir.join("claude-fixture"), r#"#!/usr/bin/env python3
import json, os, sys, time
root = os.path.dirname(os.path.abspath(__file__))
added = "claude-accounts" in os.environ.get("CLAUDE_CONFIG_DIR", "")
n = 0
for line in sys.stdin:
    v = json.loads(line)
    payload = {}
    if v["request"]["subtype"] != "initialize":
        n += 1
        if not added and n == 2:
            open(os.path.join(root, "default-pending"), "w").close()
            while True: time.sleep(0.02)
        payload = {"rate_limits_available": True, "rate_limits": {
            "five_hour": {"utilization": 5, "resets_at": "2099-01-01T00:00:00Z"},
            "seven_day": {"utilization": 5, "resets_at": "2099-01-05T00:00:00Z"}}}
    print(json.dumps({"type": "control_response", "response": {"subtype": "success", "request_id": v["request_id"], "response": payload}}), flush=True)
"#);
        let service = std::sync::Arc::new(Service::open(&dir).unwrap());
        service.accounts.add("Work").unwrap();
        let first = service.refresh(&bin, true);
        assert!(first.accounts.iter().all(|a| a.checked_at.is_some()), "{:?}", first.accounts);
        service.snapshot.lock().unwrap().checked_at = Some(crate::util::now_millis() - 10_000);
        let reader = service.clone();
        let refreshing = std::thread::spawn(move || reader.refresh(&bin, true));
        wait_until("Account 1's second read is pending", || dir.join("default-pending").exists());
        let began = std::time::Instant::now();
        service.shutdown();
        let took = began.elapsed();
        refreshing.join().unwrap();
        assert!(took < std::time::Duration::from_secs(10), "quit waited {took:?} for Account 1's read");
    }
}

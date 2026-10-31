//! Desktop hook wrapper. A hold keeps the provider's tool call pending, so the
//! worker retains its context and never emits a false SubagentStop receipt.
//! A new agent's dispatch during a hold is refused at once with the reason instead.
use super::{Admission, Policy, ReadError, Snapshot, View};
use serde_json::Value;
use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub(super) fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> io::Result<T> {
    let mut bytes = Vec::new();
    fs::File::open(path)?.take(262145).read_to_end(&mut bytes)?;
    if bytes.len() > 262144 {
        return Err(io::Error::other("oversized quota state"));
    }
    serde_json::from_slice(&bytes).map_err(io::Error::other)
}

/// The admission on the top-level reading alone, for the tests that read it.
#[cfg(test)]
pub(super) fn admission(state: &Path, now: u64) -> Admission {
    decide(state, None, now).0
}

/// **The admission of a call on `account`, on that account's own windows** (2026-10-29: the
/// gate built its reading from the account marked in use, so it watched Home at 0% while the
/// workers spent Work to its limit). The published view carries one row per account while
/// there are two or more; a call is judged on its own account's row, and on the top-level
/// reading only when there is no row for it (one account, or `None`). With the admission comes
/// what a release inside the twenty-minute window is judged on (`stagger::Release`).
pub(super) fn decide(state: &Path, account: Option<&str>, now: u64) -> (Admission, Option<super::stagger::Release>) {
    let Ok(view) = read_json::<View>(&state.join("claude-quota.json")) else {
        return (Admission::Unknown, None);
    };
    // Read the persisted switch separately: a publication failure must not leave
    // an old disabled snapshot admitting work after the policy was enabled.
    let policy = state
        .parent()
        .and_then(|p| read_json::<Policy>(&p.join("claude-quota-policy.json")).ok())
        .unwrap_or(view.policy.clone());
    // **Every account exhausted** (fill-first, Frank's finding 2): held until the soonest
    // reset among them, whatever the pause switch says, because there is no account left for
    // the work to run on. Never set with one account.
    if let Some(until) = view.held_until.filter(|t| *t > now) {
        return (Admission::Held { resets_at: until }, None);
    }
    if !policy.enabled {
        return (Admission::Disabled, None);
    }
    let row = account.and_then(|id| view.accounts.iter().find(|a| a.id == id));
    let (windows, checked_at, message, speeds, pre_hold, empty_at) = match row {
        Some(a) => (a.windows.clone(), a.checked_at, a.message.clone(), a.speeds.clone(), a.pre_hold.clone(), a.empty_at),
        None => (view.windows, view.checked_at, view.message, view.speeds, view.pre_hold, view.empty_at),
    };
    if policy.validate().is_err() || checked_at.is_some_and(|t| t > now) {
        return (Admission::Unknown, None);
    }
    let decided = Snapshot {
        windows,
        checked_at,
        retry_at: view.retry_at,
        error: message.map(|_| ReadError::Failed),
        speeds,
        // A check that came back with no figures lets work run (`Admission::NoReading`); the
        // gate decides from the same fact the service does.
        empty_at,
        // The speed before the hold, as published: one rise from 0%, which is under every line.
        rise_log: pre_hold.into_iter().map(|(id, speed)| (id, vec![(0., speed)])).collect(),
        ..Default::default()
    }
    .view(policy, now);
    (decided.admission, decided.release)
}

fn authorized(scope: &Path, worker: bool) -> bool {
    read_json::<Value>(scope).is_ok_and(|v| {
        v["version"] == 1
            && (v["actions_allowed"] == true || (worker && v["background_work_allowed"] == true))
    })
}

/// **The refusal for a closed grant, with its cause and scope** (2026-10-28, nightly 78: Agent
/// dispatches refused with "This app work has ended or was stopped" while nobody had stopped
/// anything; the cause was a message being queued, and nothing said so). It names which work
/// (the scope's thread, turn and seat), who closed it and when, from the record `ecs.rs` writes
/// with every close ([`crate::ecs::GrantClosed`]). The first sentence stays [`WORK_ENDED`], so
/// [`refusal`] still tells it from an allowance refusal.
fn work_ended(scope: &Path) -> io::Error {
    io::Error::other(format!("{WORK_ENDED} {}", closed_detail(scope)))
}

pub(crate) fn closed_detail(scope: &Path) -> String {
    let Ok(v) = read_json::<Value>(scope) else {
        return format!("Its scope file {} could not be read.", scope.display());
    };
    let binding = &v["binding"];
    let mut work = match (binding["thread_id"].as_str(), binding["turn_id"].as_str()) {
        (Some(thread), Some(turn)) => format!("the work of thread {thread}, turn {turn}"),
        _ => "this work".to_string(),
    };
    if let Some(seat) = v["seat"].as_str() {
        work.push_str(&format!(" (seat {seat})"));
    }
    let Some(by) = v["closed"]["by"].as_str() else {
        return format!("Cause: {work} has no open grant and no reason for closing it was recorded \
                        (a scope an older app wrote or one written by hand).");
    };
    let when = v["closed"]["at_ms"].as_u64().map_or_else(|| "at an unrecorded time".to_string(), |ms| format!("at {}", utc(ms)));
    format!("Cause: {work} was closed by {} {when}.", closed_sentence(by))
}

/// The words for a [`crate::ecs::closed_by`] code. Keep in step with
/// `engine/scripts/lib/scope_closure.py`.
fn closed_sentence(by: &str) -> String {
    use crate::ecs::closed_by::*;
    match by {
        STOP => "a Stop (the user's, or the app ending this connection)".into(),
        ASSIGNMENT_END => "the end of the assignment this work carried".into(),
        CRASH_RECOVERY => "the app's start-up sweep after it last quit unexpectedly".into(),
        TURN_END => "the end of the turn".into(),
        TURN_START_FAILED => "a turn that could not start".into(),
        NOT_STARTED => "the app, which has written it for an input whose turn has not started yet".into(),
        other => format!("an unrecognized cause ({other})"),
    }
}

/// `YYYY-MM-DDTHH:MM:SSZ` for Unix milliseconds (proleptic Gregorian, UTC).
fn utc(ms: u64) -> String {
    let secs = ms / 1000;
    let (days, rest) = ((secs / 86_400) as i64, secs % 86_400);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rest / 3600, (rest / 60) % 60, rest % 60)
}

/// The variable `configure` sets to the id of the Claude account a provider runs on
/// (`engine_profile.rs`). Hooks run in the provider's environment, so every helper's tool call
/// carries it.
pub const ACCOUNT_ENV: &str = "RICHOS_CLAUDE_ACCOUNT";

/// The account this gate process runs for: [`ACCOUNT_ENV`], or Account 1 when it is missing.
fn account_of_this_process() -> String {
    std::env::var(ACCOUNT_ENV).ok().filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| crate::claude_accounts::ACCOUNT_ONE.to_string())
}

/// Where the handoff markers live, under the app's `engine-state`: one per ordered helper.
pub const HANDOFFS_DIR: &str = "handoffs";

/// **The order** a helper on an account being left gets (weekly-switch plan §1), once, as
/// the refusal of one tool call. His words, 2026-10-07: *"tell all running teammates to
/// commit everything they have now and then write and commit a handoff and finish/end."*
pub const HANDOFF_ORDER: &str = "RichOS: the Claude account this work runs on is being left before its weekly limit. Stop the task now. 1. Commit everything you have changed, as it is. 2. Then make one more commit with no file changes (`git commit --allow-empty`) whose message starts with `RichOS handoff:` and says what is done, what is left and the exact next step. 3. Then end, with one line saying you handed off. A fresh teammate on the next account continues from your commits. Do no more of the task.";

/// **One ordered helper's marker**, `engine-state/handoffs/<agent_id>.json`, created once when
/// it is given the order. While it exists the helper's calls are admitted without the quota
/// hold, and the host continues its work from it when it has ended.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Handoff {
    pub agent: String,
    /// The session the helper runs in (the payload's `session_id`): the back end's own.
    pub session: String,
    pub account: String,
    pub at: u64,
    /// When the host chose the continuation that carries this helper's work on (weekly-switch
    /// plan §2, 1a). A marker counts once: it is updated, never removed, so the gate still
    /// sees it and never orders the same helper twice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub continued_at: Option<u64>,
    /// **When the helper made its handoff commit** (the CEO, 2026-10-07: *"I would hope that
    /// when the teammates get their handoff message, they'd get to a full finish within a few
    /// minutes"*): the first call of this ordered helper whose command carries `RichOS
    /// handoff:` ([`is_handoff_commit`]). `committed_at - at` is what a handoff really took.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub committed_at: Option<u64>,
    /// The weekly points the account spent from the order to that commit, counted by the quota
    /// service from the tokens it saw (`Service::track_tokens`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points: Option<f64>,
    /// **What the order acted on** (richos-hq follow-ups.md:305 item 3, 7fed0bef: "a deferral
    /// states its account and figure"): the account's label, its weekly use and the weekly point
    /// it was left at, as the published file said them when the order went out. `None` where the
    /// file did not say (an older app, or no weekly reading).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekly_used: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weekly_point: Option<f64>,
}

/// `account`'s label, weekly use and weekly point in the published `view`: its own row with two
/// or more accounts; the top level (the account in use) when it is that account.
fn figures_of(view: &View, account: &str) -> (Option<String>, Option<f64>, Option<f64>) {
    let week = |windows: &[super::Window]| windows.iter().find(|w| w.id == "seven_day").map(|w| w.used_percent);
    if let Some(row) = view.accounts.iter().find(|a| a.id == account) {
        return (Some(row.label.clone()), week(&row.windows), row.weekly_point);
    }
    match view.in_use.as_ref() {
        Some(in_use) if in_use.id == account => {
            (Some(in_use.label.clone()), week(&view.windows), view.act_at.get("seven_day").copied())
        }
        _ => (None, None, None),
    }
}

/// The account a hold on `account` was judged on, for a refusal: "Work (account 2)". The same
/// choice [`decide`] makes: `account`'s own row with two or more accounts, else the top level,
/// which names its account in `in_use`. During a move a call can run on another account than the
/// top level's (`View::moving`), so the top level's name alone would name the wrong one.
pub(super) fn held_named(state: &Path, account: &str) -> String {
    let Ok(view) = read_json::<View>(&state.join("claude-quota.json")) else {
        return "the account in use".into();
    };
    if let Some(row) = view.accounts.iter().find(|a| a.id == account) {
        return format!("{} (account {})", row.label, row.id);
    }
    match view.in_use {
        Some(a) => format!("{} (account {})", a.label, a.id),
        None => "the account in use".into(),
    }
}

/// The call that makes the handoff commit [`HANDOFF_ORDER`] asks for: a shell command whose
/// text carries `RichOS handoff:`.
pub fn is_handoff_commit(payload: &Value) -> bool {
    payload["tool_input"]["command"].as_str().is_some_and(|c| c.contains("RichOS handoff:"))
}

/// Record, once, when the helper of the marker at `path` made its handoff commit. Read again
/// first, so nothing the host or the quota service wrote meanwhile is lost. Best effort: a
/// commit that cannot be recorded leaves the handoff time at its default.
fn record_commit(path: &Path, at: u64) {
    if let Ok(marker) = read_json::<Handoff>(path) {
        if marker.committed_at.is_none() {
            drop(super::atomic_write(path, &Handoff { committed_at: Some(at), ..marker }));
        }
    }
}

/// Record, once, the points the account spent during this marker's handoff (the quota
/// service's count), read again first like [`record_commit`].
pub fn record_points(path: &Path, points: f64) -> io::Result<()> {
    let marker = read_json::<Handoff>(path)?;
    if marker.points.is_some() { return Ok(()); }
    super::atomic_write(path, &Handoff { points: Some(points), ..marker })
}

/// Every handoff marker on disk, with its file. One that cannot be read is left out.
pub fn handoffs(state: &Path) -> Vec<(std::path::PathBuf, Handoff)> {
    let Ok(entries) = fs::read_dir(state.join(HANDOFFS_DIR)) else { return Vec::new() };
    let mut found: Vec<_> = entries.flatten().map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "json"))
        .filter_map(|path| read_json::<Handoff>(&path).ok().map(|marker| (path, marker)))
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

/// Record that the host chose the continuation for this marker's helper (plan §2, 1a).
pub fn mark_continued(path: &Path, marker: &Handoff, at: u64) -> io::Result<()> {
    // The file as it is now: the gate and the quota service write to it too.
    let current = read_json::<Handoff>(path).unwrap_or_else(|_| marker.clone());
    super::atomic_write(path, &Handoff { continued_at: Some(at), ..current })
}

/// The marker's file for `agent`, or `None` for an id that is not a plain name (a path must
/// never come from a payload).
fn marker_path(state: &Path, agent: &str) -> Option<std::path::PathBuf> {
    (!agent.is_empty() && agent.len() <= 128 && agent.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .then(|| state.join(HANDOFFS_DIR).join(format!("{agent}.json")))
}

/// **The order to a helper on an account being left** (weekly-switch plan §1), decided before
/// the quota hold. `None`: not a helper, its account is not being left, or no marker could be
/// written, so the call goes on to the ordinary hold. `Some(Err)`: the order, this one time,
/// with its marker now written. `Some(Ok)`: already ordered, so the call is admitted without
/// the hold — the last points of the week are for exactly this. A revoked grant still says the
/// work ended, as it does everywhere below.
pub(super) fn handoff(payload: &Value, state: &Path, scope: &Path, account: &str) -> Option<io::Result<()>> {
    let agent = payload.get("agent_id").and_then(Value::as_str).filter(|id| !id.is_empty())?;
    let view = read_json::<View>(&state.join("claude-quota.json")).ok()?;
    if !view.leaving.iter().any(|id| id == account) {
        return None;
    }
    if !authorized(scope, true) {
        return Some(Err(work_ended(scope)));
    }
    let path = marker_path(state, agent)?;
    fs::create_dir_all(path.parent()?).ok()?;
    match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => {
            let (label, weekly_used, weekly_point) = figures_of(&view, account);
            let marker = Handoff {
                label,
                weekly_used,
                weekly_point,
                agent: agent.to_string(),
                session: payload.get("session_id").and_then(Value::as_str).unwrap_or_default().to_string(),
                account: account.to_string(),
                at: crate::util::now_millis(),
                continued_at: None,
                committed_at: None,
                points: None,
            };
            let written = serde_json::to_vec(&marker).map_err(io::Error::other)
                .and_then(|bytes| file.write_all(&bytes)).and_then(|()| file.sync_all());
            if written.is_err() {
                // A marker that could not be written orders nobody: the next call is ordered.
                drop(fs::remove_file(&path));
                return None;
            }
            Some(Err(io::Error::other(HANDOFF_ORDER)))
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            if is_handoff_commit(payload) { record_commit(&path, crate::util::now_millis()); }
            Some(Ok(()))
        }
        Err(_) => None,
    }
}

fn wait(
    payload: &Value,
    state: &Path,
    scope: &Path,
    account: &str,
    tick: Duration,
    budget: Duration,
) -> io::Result<()> {
    let worker = payload
        .get("agent_id")
        .and_then(Value::as_str)
        .is_some_and(|id| !id.is_empty());
    if worker {
        if let Some(answer) = handoff(payload, state, scope, account) {
            return answer;
        }
    }
    if !worker && payload["tool_name"] != "Agent" {
        return Ok(());
    }
    // **An agent dispatch is a rise to expect** (fill-first, plan §15 answer 10): one line per
    // dispatch, counted by the quota service, which switches to the fast interval when several
    // arrive within a minute. Best effort: a failed note never holds or refuses the tool.
    if payload["tool_name"] == "Agent" {
        use std::io::Write as _;
        if let Ok(mut log) = fs::OpenOptions::new().create(true).append(true).open(state.join(super::DISPATCH_LOG)) {
            drop(writeln!(log, "{}", crate::util::now_millis()));
        }
    }
    let started = Instant::now();
    let mut observation: Option<super::holds::Guard> = None;
    // Once per held call, so its own retries below keep the one permit (`release_key`).
    let key = release_key(payload);
    loop {
        if !authorized(scope, worker) {
            return Err(work_ended(scope));
        }
        let now = crate::util::now_millis();
        let (admission, release) = decide(state, Some(account), now);
        if admission.allows_work() {
            if let Some(guard) = observation.take() { guard.release(); }
            return Ok(());
        }
        if observation.is_none() {
            // Observation failure must not let a paused tool through the gate.
            observation = super::holds::from_hook(state, scope, payload)
                .map(|row| super::holds::Hold { account: Some(account.to_string()), key: Some(key.clone()), ..row })
                .and_then(|row| super::holds::Guard::begin(state, row)).ok();
        }
        // **Inside the twenty-minute window, one at a time** (Rich, 2026-10-29): this call goes
        // on only when the staggered release lets it (`stagger::admit`). A ledger that cannot
        // be read or written releases nobody: the call waits for the reset, as it would have.
        if let Some(release) = release {
            let team = |released: &[String]| measured_team(state, account, released);
            if super::stagger::admit(state, account, &key, &release, &team, now).unwrap_or(false) {
                if let Some(guard) = observation.take() { guard.release(); }
                return Ok(());
            }
        }
        // **No new agent starts during the hold, and the dispatcher is told why at once**
        // (two-day cleanup O1.2): a held Agent call used to stay pending, with nothing said,
        // for as long as the hold lasted. A helper's own calls still wait above, keeping its
        // context; a new agent has none to keep, so its dispatch is refused and nothing starts.
        if payload["tool_name"] == "Agent" {
            return Err(io::Error::other(dispatch_held(state, account, &admission, now)));
        }
        if started.elapsed() >= budget {
            // Named (follow-ups 7fed0bef item 3): the hold is this call's account's, and a reader
            // of this refusal must not take it for another account's.
            return Err(io::Error::other(format!(
                "Still waiting for a current allowance reading on {}. No tool action was run.",
                held_named(state, account),
            )));
        }
        std::thread::sleep(tick);
    }
}

/// The refusal of an Agent call during the usage hold: which account is held, why, and until
/// when, read from the same published file [`decide`] judged it on.
fn dispatch_held(state: &Path, account: &str, admission: &Admission, now: u64) -> String {
    let until = match admission {
        Admission::Held { resets_at } => format!("until {}", utc(*resets_at)),
        _ => "until its next reset".to_string(),
    };
    let view = read_json::<View>(&state.join("claude-quota.json")).ok();
    let why = if view.as_ref().and_then(|v| v.held_until).is_some_and(|t| t > now) {
        "Every signed-in Claude account has reached its limit.".to_string()
    } else {
        let policy = state.parent()
            .and_then(|p| read_json::<Policy>(&p.join("claude-quota-policy.json")).ok())
            .or_else(|| view.map(|v| v.policy))
            .unwrap_or_default();
        format!("Its usage reached the pause point set in RichOS ({}%).", policy.pause_percent)
    };
    format!("No new agent was started: the usage hold on {} stands {until}. {why} Dispatch it again \
             after that time.", held_named(state, account))
}

/// **The permit a held call is released on** (`stagger::admit`). An Agent call starts a new
/// agent, so each one has its own permit: the call's `tool_use_id`, or failing that one made
/// unique in this process (review of 2026-10-29, finding 1: every dispatch of one session
/// shared `dispatch:<session>`, so once the first was released ten went in the same instant).
/// Every other call of a helper is that helper carrying on, on the helper's one permit.
fn release_key(payload: &Value) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static CALLS: AtomicU64 = AtomicU64::new(0);
    let field = |name: &str| payload.get(name).and_then(Value::as_str).filter(|v| !v.is_empty());
    if payload["tool_name"] == "Agent" {
        let call = field("tool_use_id").map(str::to_string).unwrap_or_else(|| {
            format!("{}-{}-{}", std::process::id(), crate::util::now_millis(), CALLS.fetch_add(1, Ordering::Relaxed))
        });
        return format!("dispatch:{}:{call}", field("session_id").unwrap_or_default());
    }
    match field("agent_id") {
        Some(agent) => agent.to_string(),
        None => format!("dispatch:{}", field("session_id").unwrap_or_default()),
    }
}

/// **The team whose pace the speed before the hold measured** (`stagger::admit`): the helpers on
/// `account` released in this window, and those still waiting that are not in `released`, one
/// per permit, from the pause records (`holds`). A new agent's dispatch (`dispatch:` permits)
/// never ran before the hold, so it is not counted (second review of 2026-10-29, finding 1:
/// counting nine queued dispatches cut one measured worker's share to a tenth). A record
/// written before records carried their account, or their permit, counts as before.
fn measured_team(state: &Path, account: &str, released: &[String]) -> usize {
    let helper = |key: &String| !key.starts_with("dispatch:");
    let waiting = super::holds::held_calls(state).unwrap_or_default();
    let mut team: Vec<String> = waiting.iter()
        .filter(|row| row.account.as_deref().is_none_or(|a| a == account))
        .map(|row| row.key.clone().unwrap_or_else(||
            if row.kind == "agent" { row.id.clone() } else { format!("dispatch:{}", row.session_id) }))
        .filter(|key| helper(key) && !released.contains(key))
        .collect();
    team.sort();
    team.dedup();
    team.len() + released.iter().filter(|key| helper(key)).count()
}

/// Hand the callback to the canonical hook, unchanged, and pass its answer on.
///
/// **AND A DOCUMENT WRITTEN FOR HIM COMES OUT AMERICAN** (CEO §93; §13 "generated documents
/// the CEO opens"). This wrapper is the one `PreToolUse` registration both leases install, so
/// every Write, Edit, MultiEdit and NotebookEdit an app session or its workers make passes
/// here. When the write goes to a prose file and carries words the American spelling table
/// changes, the canonical hook's single output is captured and the fixed input merged into it
/// (`american_spelling::merge_into_envelope`); otherwise nothing about this path changes, and
/// the hook's output goes straight through as before. The canonical hook still judges the
/// ORIGINAL call, as every parallel guard does; the fix differs from it only by spelling.
fn run_canonical(
    program: &std::ffi::OsStr,
    args: impl Iterator<Item = std::ffi::OsString>,
    bytes: &[u8],
    payload: &Value,
    out: &mut dyn Write,
) -> io::Result<i32> {
    let fix = match (payload.get("tool_name").and_then(Value::as_str), payload.get("tool_input")) {
        (Some(tool), Some(input)) => crate::american_spelling::fix_tool_input(tool, input),
        _ => None,
    };
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::piped());
    if fix.is_some() {
        command.stdout(Stdio::piped());
    }
    let mut child = command.spawn()?;
    child
        .stdin
        .take()
        .ok_or_else(|| io::Error::other("missing hook input"))?
        .write_all(bytes)?;
    let Some((fixed, changes)) = fix else {
        return Ok(child.wait()?.code().unwrap_or(2));
    };
    let output = child.wait_with_output()?;
    let code = output.status.code().unwrap_or(2);
    // A refusal, or output that is not one envelope, goes on exactly as the hook wrote it.
    let merged = (code == 0)
        .then(|| crate::american_spelling::merge_into_envelope(&output.stdout, fixed, &changes))
        .flatten();
    out.write_all(merged.as_deref().unwrap_or(&output.stdout))?;
    out.flush()?;
    Ok(code)
}

/// Runs before Tauri initialization in the desktop executable. The canonical
/// hook still decides permissions, targets and receipts after this gate opens.
pub fn run_cli() -> i32 {
    let result = (|| -> io::Result<i32> {
        let mut bytes = Vec::new();
        io::stdin().take(262145).read_to_end(&mut bytes)?;
        if bytes.len() > 262144 {
            return Err(io::Error::other("oversized tool callback"));
        }
        let payload: Value = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
        let state = std::env::var_os("RICHOS_APP_STATE")
            .ok_or_else(|| io::Error::other("missing desktop state"))?;
        let scope = std::env::var_os("RICHOS_APP_SCOPE")
            .ok_or_else(|| io::Error::other("missing desktop scope"))?;
        if !Path::new(&state).is_absolute() || !Path::new(&scope).is_absolute() {
            return Err(io::Error::other("invalid desktop roots"));
        }
        wait(
            &payload,
            Path::new(&state),
            Path::new(&scope),
            &account_of_this_process(),
            Duration::from_millis(250),
            Duration::from_secs(21500),
        )?;
        let mut args = std::env::args_os().skip(2);
        let program = args
            .next()
            .ok_or_else(|| io::Error::other("missing canonical hook"))?;
        run_canonical(&program, args, &bytes, &payload, &mut io::stdout())
    })();
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("{}", refusal(&e));
            2
        }
    }
}

/// **The refusal when this call's grant is gone, which is not a quota reading.** The scope's
/// grant is closed by his Stop, by the end of the assignment and by a quit (`ecs::revoke`);
/// no allowance figure is consulted to refuse it.
const WORK_ENDED: &str = "This app work has ended or was stopped. New actions are unavailable.";

/// What a refused call is told. **Only an allowance refusal says "quota".** Walk 6
/// (2026-10-05, `walk-10608166d53f`) showed a reviewer's verdict refused over and over with
/// "RichOS desktop quota: This app work was stopped" at 51% five-hour use: the grant had been
/// revoked when its job was ended early (`work_host`'s declared wait now keeps it open), and
/// the one prefix this wrapper put on every refusal blamed an allowance nobody had read.
fn refusal(error: &io::Error) -> String {
    let text = error.to_string();
    // The order speaks for itself and is about the account, not a quota reading.
    if text == HANDOFF_ORDER {
        return text;
    }
    if text.starts_with(WORK_ENDED) {
        format!("RichOS desktop: {text}")
    } else {
        format!("RichOS desktop quota: {text}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quota::{atomic_write, tests::Scratch, Service, Window};
    use serde_json::json;
    fn setup() -> (Scratch, Service, std::path::PathBuf, std::path::PathBuf) {
        let root = Scratch::new();
        let service = Service::open(root.path()).unwrap();
        service
            .set_policy(Policy {
                enabled: true,
                pause_percent: 93,
            })
            .unwrap();
        let state = root.path().join("engine-state");
        let scope = root.path().join("scope.json");
        atomic_write(
            &scope,
            &json!({"version":1,"actions_allowed":false,"background_work_allowed":true}),
        )
        .unwrap();
        (root, service, state, scope)
    }
    fn publish(service: &Service, left: u64) {
        let now = crate::util::now_millis();
        *service.snapshot.lock().unwrap() = Snapshot {
            checked_at: Some(now),
            windows: vec![Window {
                id: "five_hour".into(),
                label: "Five-hour".into(),
                used_percent: 94.,
                resets_at: Some(now + left),
                duration_ms: 18_000_000,
            }],
            ..Default::default()
        };
        service.publish().unwrap();
    }
    #[test]
    fn weekly_hold_survives_five_hour_exception_and_elapsed_reset() {
        let (_root, service, state, _scope) = setup();
        publish(&service, 600_000);
        let now = crate::util::now_millis();
        {
            let mut snapshot = service.snapshot.lock().unwrap();
            snapshot.windows.push(Window { id: "seven_day".into(), label: "Weekly".into(),
                used_percent: 99., resets_at: Some(now + 60_000), duration_ms: 604_800_000 });
        }
        service.publish().unwrap();
        assert!(matches!(admission(&state, now), Admission::Held { .. }));
        assert_eq!(decide(&state, None, now).1, None, "a weekly hold is never released early");
        // Past the weekly reset only the five-hour window holds, 10 minutes before its reset:
        // released staggered (`stagger`), not all at once.
        let five_reset = service.snapshot.lock().unwrap().windows[0].resets_at.unwrap();
        let after = decide(&state, None, now + 60_001);
        assert_eq!(after.0, Admission::Held { resets_at: five_reset });
        assert!(after.1.is_some());
        service.snapshot.lock().unwrap().windows[1].used_percent = 0.;
        service.publish().unwrap();
        let (held, release) = decide(&state, None, now);
        assert_eq!(held, Admission::Held { resets_at: five_reset });
        // 94% and no speed measured before the hold or since: no measurement, so nobody goes
        // before the reset (second review of 2026-10-29, finding 2).
        assert!(release.is_some_and(|r| !r.allows(1, 0, 1)), "94%, no speed measured: it waits for the reset");
        service.snapshot.lock().unwrap().windows[0].resets_at = Some(now + 3_600_000);
        service.publish().unwrap();
        assert!(matches!(admission(&state, now), Admission::Held { .. }));
    }
    #[test]
    fn same_worker_waits_then_resumes_when_reset_is_near() {
        let (_root, service, state, scope) = setup();
        publish(&service, 3_600_000);
        let (tx, rx) = std::sync::mpsc::channel();
        // The worker's budget and the wait for it to resume are hang guards, never verdicts
        // (audit R9): they were 3 s and 1 s, so a test thread descheduled for a few seconds
        // let the budget run out before the publish and failed while the gate was right. The
        // verdict is the worker's own answer: `Ok` only once admission allows work, which the
        // near-reset publish is the only thing here to grant.
        const HANG_GUARD: Duration = Duration::from_secs(60);
        let worker = std::thread::spawn(move || {
            tx.send(wait(
                &json!({"agent_id":"same-worker","tool_name":"Bash"}),
                &state,
                &scope,
                "1",
                Duration::from_millis(5),
                HANG_GUARD * 2,
            ))
            .expect("the test thread holds rx until the verdict arrives");
        });
        assert!(
            rx.recv_timeout(Duration::from_millis(40)).is_err(),
            "worker must stay pending"
        );
        publish(&service, 19 * 60_000);
        // The climb into the hold measured at 0.1 points a minute: 94 + 0.1 x 19 = 95.9, under
        // 100, so the one worker goes. With no measurement nobody would (finding 2 of the second
        // review of 2026-10-29).
        service.snapshot.lock().unwrap().rise_log.insert("five_hour".into(), vec![(90., 0.1 / 60_000.)]);
        service.publish().unwrap();
        rx.recv_timeout(HANG_GUARD)
            .expect("the worker never resumed after the reset came near")
            .expect("the worker was refused instead of resuming");
        worker.join().unwrap();
    }
    #[test]
    fn stale_or_expired_telemetry_does_not_hold_a_dispatch_but_proven_quota_still_does() {
        let (_root,service,state,scope)=setup();let now=crate::util::now_millis();
        atomic_write(&scope,&json!({"version":1,"actions_allowed":true})).unwrap();
        let five=|used,resets|Window {id:"five_hour".into(),label:"Five-hour".into(),used_percent:used,resets_at:Some(resets),duration_ms:18_000_000};
        let weekly=Window {id:"seven_day".into(),label:"Weekly".into(),used_percent:99.,resets_at:Some(now-60_000),duration_ms:604_800_000};
        let dispatch=json!({"tool_name":"Agent","session_id":"recovered-stu"});
        for snapshot in [Snapshot {checked_at:Some(now),windows:vec![five(3.,now-60_000)],..Default::default()},
            Snapshot::default(),
            Snapshot {checked_at:Some(now-3_600_000),windows:vec![five(3.,now+3_600_000)],..Default::default()}] {
            *service.snapshot.lock().unwrap()=snapshot;service.publish().unwrap();
            wait(&dispatch,&state,&scope,"1",Duration::ZERO,Duration::ZERO)
                .expect("unknown allowance is not evidence of a quota hold");
        }
        // An expired weekly figure must not mask a still-current five-hour hold.
        *service.snapshot.lock().unwrap()=Snapshot {checked_at:Some(now),windows:vec![weekly,five(95.,now+3_600_000)],..Default::default()};
        service.publish().unwrap();assert!(matches!(admission(&state,now),Admission::Held {..}));
        assert!(service.view().leaving.is_empty(),"an expired weekly window ordered an account handoff");
        assert!(wait(&dispatch,&state,&scope,"1",Duration::ZERO,Duration::ZERO).is_err());
        atomic_write(&scope,&json!({"version":1,"actions_allowed":false})).unwrap();
        assert!(wait(&dispatch,&state,&scope,"1",Duration::ZERO,Duration::ZERO).unwrap_err().to_string().starts_with(WORK_ENDED));
    }

    /// **Every closed-grant refusal names which work, who closed it and when** (follow-up
    /// "app work tools refused dispatches for unexplained reasons", 2026-10-28 12:50Z: Stu's
    /// dispatches were refused with "This app work has ended or was stopped" while nobody had
    /// stopped anything and the job showed running). On main this said only the first sentence.
    #[test]
    fn a_closed_grant_refusal_names_the_work_who_closed_it_and_when() {
        let (_root, _service, state, scope) = setup();
        let dispatch = json!({"tool_name":"Agent","session_id":"stu"});
        let closed = |by: &str| json!({"version":1,"actions_allowed":false,"background_work_allowed":true,
            "binding":{"entity_id":"co","thread_id":"thr_one","session_id":"stu","turn_id":"peer:42","audience":"rich","revision":3},
            "seat":"stu-thread:thr_one","closed":{"by":by,"at_ms":1_793_261_100_000_u64}});
        for (by, words) in [(crate::ecs::closed_by::STOP, "a Stop"),
                            (crate::ecs::closed_by::ASSIGNMENT_END, "the end of the assignment"),
                            (crate::ecs::closed_by::NOT_STARTED, "has not started yet")] {
            atomic_write(&scope, &closed(by)).unwrap();
            let said = refusal(&wait(&dispatch, &state, &scope, "1", Duration::ZERO, Duration::ZERO).unwrap_err());
            assert!(said.starts_with("RichOS desktop: This app work has ended or was stopped."), "{said}");
            assert!(said.contains("thread thr_one, turn peer:42 (seat stu-thread:thr_one)"), "which work: {said}");
            assert!(said.contains(words), "by whom: {said}");
            assert!(said.contains("at 2026-10-29T08:05:00Z"), "when: {said}");
            assert!(!said.to_lowercase().contains("quota"), "{said}");
        }
        // A scope an older app wrote says so rather than guessing a cause.
        atomic_write(&scope, &json!({"version":1,"actions_allowed":false})).unwrap();
        let said = refusal(&wait(&dispatch, &state, &scope, "1", Duration::ZERO, Duration::ZERO).unwrap_err());
        assert!(said.contains("no reason for closing it was recorded"), "{said}");
    }

    #[test]
    fn utc_formats_unix_milliseconds() {
        assert_eq!(utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc(951_782_400_000), "2000-02-29T00:00:00Z");
        assert_eq!(utc(1_793_261_100_999), "2026-10-29T08:05:00Z");
    }

    #[test]
    fn disabling_releases_unknown_quota_and_stop_revokes_waiter() {
        let (_root, service, state, scope) = setup();
        service.set_policy(Policy::default()).unwrap();
        wait(
            &json!({"agent_id":"worker"}),
            &state,
            &scope,
            "1",
            Duration::from_millis(1),
            Duration::ZERO,
        )
        .unwrap();
        service
            .set_policy(Policy {
                enabled: true,
                pause_percent: 93,
            })
            .unwrap();
        atomic_write(
            &scope,
            &json!({"version":1,"actions_allowed":false,"background_work_allowed":false}),
        )
        .unwrap();
        assert!(wait(
            &json!({"agent_id":"worker"}),
            &state,
            &scope,
            "1",
            Duration::from_millis(1),
            Duration::from_secs(2)
        )
        .is_err());
    }
    /// Walk 6 (2026-10-05): a reviewer's hand-back refused with "RichOS desktop quota: This app
    /// work was stopped" at 51% five-hour use. The quota was never read for that refusal: the
    /// grant was gone. With room left, a revoked grant is refused WITHOUT the word quota, and a
    /// refusal that is about the allowance still says so.
    #[test]
    fn a_revoked_grant_is_never_reported_as_quota() {
        let (_root, service, state, scope) = setup();
        let now = crate::util::now_millis();
        *service.snapshot.lock().unwrap() = Snapshot {
            checked_at: Some(now),
            windows: vec![Window { id: "five_hour".into(), label: "Five-hour".into(), used_percent: 51.,
                resets_at: Some(now + 3_600_000), duration_ms: 18_000_000 }],
            ..Default::default()
        };
        service.publish().unwrap();
        assert!(admission(&state, now).allows_work(), "51% must admit work");
        atomic_write(&scope, &json!({"version":1,"actions_allowed":false,"background_work_allowed":false})).unwrap();
        let refused = wait(&json!({"agent_id":"reviewer","tool_name":"SubagentHandback"}), &state, &scope, "1",
            Duration::from_millis(1), Duration::from_secs(2)).unwrap_err();
        let said = refusal(&refused);
        assert!(!said.to_lowercase().contains("quota"), "{said}");
        assert!(said.contains("ended or was stopped"), "{said}");
        let waiting = refusal(&io::Error::other("Still waiting for a current allowance reading. No tool action was run."));
        assert!(waiting.starts_with("RichOS desktop quota: "), "{waiting}");
    }

    /// **Weekly-switch plan §1, slice 2's proof (§7).** Account 1 in use at 99% of its week: it
    /// is leaving, and the weekly hold stands. A helper on it gets the order once, with its
    /// marker written (its session and account), and from then on every call of its is
    /// admitted although the hold still stands. A helper on another account is untouched: the
    /// ordinary hold, no order, no marker. So is the back end (no `agent_id`): its own tools
    /// pass as before and its new `Agent` dispatch is refused with the hold's reason, never ordered.
    #[test]
    fn a_helper_on_a_leaving_account_is_ordered_once_then_admitted_while_the_weekly_hold_stands() {
        let (_root, service, state, scope) = setup();
        let now = crate::util::now_millis();
        *service.snapshot.lock().unwrap() = Snapshot {
            checked_at: Some(now),
            windows: vec![
                Window { id: "five_hour".into(), label: "Five-hour".into(), used_percent: 10.,
                    resets_at: Some(now + 3_600_000), duration_ms: 18_000_000 },
                Window { id: "seven_day".into(), label: "Weekly".into(), used_percent: 99.,
                    resets_at: Some(now + 48 * 3_600_000), duration_ms: 604_800_000 },
            ],
            ..Default::default()
        };
        service.publish().unwrap();
        let view: View = read_json(&state.join("claude-quota.json")).unwrap();
        assert_eq!(view.leaving, vec!["1".to_string()]);
        assert!(matches!(admission(&state, now), Admission::Held { .. }));

        let helper = json!({"agent_id":"helper-a","session_id":"session-1","tool_name":"Bash"});
        let ordered = wait(&helper, &state, &scope, "1", Duration::ZERO, Duration::ZERO).unwrap_err();
        assert_eq!(refusal(&ordered), HANDOFF_ORDER, "the order, word for word, with no quota prefix");
        let marker: Handoff = read_json(&state.join(HANDOFFS_DIR).join("helper-a.json")).unwrap();
        assert_eq!((marker.agent.as_str(), marker.session.as_str(), marker.account.as_str()), ("helper-a", "session-1", "1"));
        // The order states the account and the figure it acted on (follow-ups 7fed0bef item 3).
        assert_eq!((marker.label.as_deref(), marker.weekly_used), (Some("Account 1"), Some(99.)), "{marker:?}");
        assert!(marker.weekly_point.is_some_and(|p| p > 0. && p <= 99.), "{marker:?}");
        for _ in 0..2 {
            wait(&helper, &state, &scope, "1", Duration::ZERO, Duration::ZERO)
                .expect("an ordered helper is admitted, never ordered twice");
        }
        assert!(matches!(admission(&state, crate::util::now_millis()), Admission::Held { .. }), "the weekly hold still stands");

        let other = json!({"agent_id":"helper-b","session_id":"session-2","tool_name":"Bash"});
        let held = refusal(&wait(&other, &state, &scope, "2", Duration::ZERO, Duration::ZERO).unwrap_err());
        assert!(held.starts_with("RichOS desktop quota: Still waiting"), "{held}");
        assert!(held.contains("on Account 1 (account 1)"), "the hold names whose figure it is: {held}");
        assert!(!state.join(HANDOFFS_DIR).join("helper-b.json").exists());

        wait(&json!({"tool_name":"Bash","session_id":"session-1"}), &state, &scope, "1", Duration::ZERO, Duration::ZERO).unwrap();
        atomic_write(&scope, &json!({"version":1,"actions_allowed":true})).unwrap();
        let dispatch = refusal(&wait(&json!({"tool_name":"Agent","session_id":"session-1"}), &state, &scope, "1",
            Duration::ZERO, Duration::ZERO).unwrap_err());
        assert!(dispatch.starts_with("RichOS desktop quota: No new agent was started"), "{dispatch}");
        assert_eq!(fs::read_dir(state.join(HANDOFFS_DIR)).unwrap().count(), 1, "only the helper on the leaving account was ordered");
    }

    /// **One account, automatic pause off: no order at the weekly point.** The fast-use alert
    /// says "Automatic pause is off in Settings, so nothing acts before it reaches 100%", so a
    /// helper on Account 1 at 99% of its week keeps working: not leaving, no order, no marker.
    #[test]
    fn one_account_with_automatic_pause_off_orders_no_handoff_at_the_weekly_point() {
        let (_root, service, state, scope) = setup();
        service.set_policy(Policy { enabled: false, pause_percent: 93 }).unwrap();
        let now = crate::util::now_millis();
        *service.snapshot.lock().unwrap() = Snapshot {
            checked_at: Some(now),
            windows: vec![
                Window { id: "five_hour".into(), label: "Five-hour".into(), used_percent: 10.,
                    resets_at: Some(now + 3_600_000), duration_ms: 18_000_000 },
                Window { id: "seven_day".into(), label: "Weekly".into(), used_percent: 99.,
                    resets_at: Some(now + 48 * 3_600_000), duration_ms: 604_800_000 },
            ],
            ..Default::default()
        };
        service.publish().unwrap();
        let view: View = read_json(&state.join("claude-quota.json")).unwrap();
        assert!(view.leaving.is_empty(), "one account with the pause off is not left: {:?}", view.leaving);
        let helper = json!({"agent_id":"helper-a","session_id":"session-1","tool_name":"Bash"});
        wait(&helper, &state, &scope, "1", Duration::ZERO, Duration::ZERO)
            .expect("the helper keeps working, with no handoff order");
        assert!(!state.join(HANDOFFS_DIR).join("helper-a.json").exists(), "no marker: nobody was ordered");
    }

    /// **What a handoff really takes replaces the 10-minute guess** (the CEO, 2026-10-07: *"I
    /// would hope that when the teammates get their handoff message, they'd get to a full
    /// finish within a few minutes. Hopefully no more than 3-5 minutes or so."*).
    ///
    /// The gate: an ordered helper's ordinary call records nothing; its handoff commit (a
    /// command carrying `RichOS handoff:`) records when, once, on its marker.
    ///
    /// The service: that account's last handoff took 2 minutes from the order to the commit and
    /// spent 3,000,000 Opus tokens at 1,000,000 a point, 3 points. The weekly window gains 0.5
    /// a minute, checked every 5. The room the switch leaves is the wait for the next check
    /// plus the handoff, whichever of its time at this speed and its measured points is more:
    /// 0.5 x 5 + max(0.5 x 2, 3) = 5.5 points, so the point is 99 - (6 - 1) = 94. The 10-minute
    /// guess gave 0.5 x 15 = 7.5, so 92; the 5-minute default before any measurement gives
    /// 0.5 x 10 = 5, so 95.
    #[test]
    fn a_handoff_is_measured_from_the_order_to_its_commit_and_its_cost_sets_the_switch_point() {
        let (_root, service, state, scope) = setup();
        let now = crate::util::now_millis();
        let (minute, hour) = (60_000, 3_600_000);
        let both = |weekly: f64| vec![
            Window { id: "five_hour".into(), label: "Five-hour".into(), used_percent: 10.,
                resets_at: Some(now + hour), duration_ms: 18_000_000 },
            Window { id: "seven_day".into(), label: "Weekly".into(), used_percent: weekly,
                resets_at: Some(now + 48 * hour), duration_ms: 604_800_000 },
        ];
        *service.snapshot.lock().unwrap() = Snapshot { checked_at: Some(now), windows: both(99.), ..Default::default() };
        service.publish().unwrap();
        let call = |command: &str| json!({"agent_id":"helper-a","session_id":"s","tool_name":"Bash","tool_input":{"command":command}});
        wait(&call("cargo test"), &state, &scope, "1", Duration::ZERO, Duration::ZERO).unwrap_err();
        let path = state.join(HANDOFFS_DIR).join("helper-a.json");
        wait(&call("git add -A && git commit -m 'parser'"), &state, &scope, "1", Duration::ZERO, Duration::ZERO).unwrap();
        assert!(read_json::<Value>(&path).unwrap()["committed_at"].is_null(), "an ordinary commit is not the handoff");
        wait(&call("git commit --allow-empty -m 'RichOS handoff: parser done; next: its tests'"), &state, &scope, "1",
            Duration::ZERO, Duration::ZERO).unwrap();
        let marker: Value = read_json(&path).unwrap();
        assert!(marker["committed_at"].as_u64().is_some_and(|c| c >= marker["at"].as_u64().unwrap()), "{marker}");

        // The account's last handoff: ordered 12 minutes ago, committed 10 minutes ago.
        atomic_write(&path, &json!({"agent":"helper-a","session":"s","account":"1","at":now - 12 * minute,
            "committed_at":now - 10 * minute})).unwrap();
        let mut track = crate::quota::tokens::Track::with_episodes(vec![crate::quota::tokens::Episode {
            points: 1., tokens: [("claude-opus-5-5".to_string(), 1_000_000)].into() }]);
        track.add("msg_handoff", now - 11 * minute, "claude-opus-5-5", 3_000_000, 1_000);
        service.tokens.lock().unwrap().insert("1".into(), track);
        let mut reading = Snapshot::default();
        reading.accept(both(60.), now - minute);
        reading.accept(both(60.5), now);
        *service.snapshot.lock().unwrap() = reading;
        service.track_tokens();
        let view = service.view();
        assert_eq!(view.refresh_interval_ms, crate::quota::REFRESH_INTERVAL_MS);
        assert!((view.act_at["seven_day"] - 94.).abs() < 1e-9, "{:?}", view.act_at);
        assert!(read_json::<Value>(&path).unwrap()["points"].as_f64().is_some_and(|p| (p - 3.).abs() < 1e-9),
            "the handoff's points are kept on its marker");
    }

    /// **Item 1 of 2026-10-29 (Work's 09:50Z limit).** Ten workers held at 95%, 19 minutes before
    /// the five-hour reset. The team's speed before the hold: 80% -> 95% in 10 minutes, its first
    /// point unseen, 14 / 10 = 1.4 points a minute; held, the readings say 0. On main the window
    /// read that 0 as room and released all ten at once (they spent the rest in about 8 s). Now
    /// one goes: 95 + (1.4 / 10) x 19 = 97.66, under 100. A second at this reading would be
    /// 95 + (2.8 / 10) x 19 = 100.32, so none passes the projected 100%; the others wait for a
    /// later reading or the reset. The one released carries on with its next call.
    #[test]
    fn inside_the_window_ten_held_workers_are_released_staggered_never_past_the_projected_100() {
        let (_root, service, state, scope) = setup();
        let (now, minute) = (crate::util::now_millis(), 60_000);
        let reset = now + 19 * minute;
        let five = |used: f64| vec![Window { id: "five_hour".into(), label: "Five-hour".into(), used_percent: used,
            resets_at: Some(reset), duration_ms: 18_000_000 }];
        let mut reading = Snapshot::default();
        reading.accept(five(80.), now - 15 * minute);
        reading.accept(five(95.), now - 5 * minute);
        reading.accept(five(95.), now);
        *service.snapshot.lock().unwrap() = reading;
        service.publish().unwrap();
        let workers: Vec<String> = (0..10).map(|i| format!("worker-{i}")).collect();
        // The ten waiters' own pause records, as each held call keeps one (`holds`).
        let _waiting: Vec<_> = workers.iter().map(|id| super::super::holds::Guard::begin(&state, serde_json::from_value(json!({
            "kind": "agent", "id": id, "entityId": "", "threadId": "", "sessionId": "", "name": id,
            "task": null, "sinceAt": now, "releasedAt": null})).unwrap()).unwrap()).collect();
        let call = |id: &str| wait(&json!({"agent_id": id, "tool_name": "Bash"}), &state, &scope, "1",
            Duration::ZERO, Duration::ZERO).is_ok();
        let released: Vec<&String> = workers.iter().filter(|id| call(id)).collect();
        assert_eq!(released.len(), 1, "not all at once, and none past the projected 100%: {released:?}");
        assert!(call(released[0]), "the released worker carries on with its next call");
        assert_eq!(workers.iter().filter(|id| call(id)).count(), 1, "the other nine still wait");
        // **Third review of 2026-10-29, P1: the permit is judged again on a fresh reading.**
        // 97% -> 99% in a minute, still 19 minutes from the reset: at whole points with the
        // first unseen that is 1 point a minute, and the released worker carrying on projects
        // 99 + max(0.14, 1) x 19 = 118. It waits; before, its permit let it straight through.
        let mut fresh = Snapshot::default();
        fresh.accept(five(97.), now - minute);
        fresh.accept(five(99.), now);
        *service.snapshot.lock().unwrap() = fresh;
        service.publish().unwrap();
        assert!(decide(&state, Some("1"), crate::util::now_millis()).1.is_some(), "still inside the window: the stagger decides");
        assert!(!call(released[0]), "a fresh reading past 100% pauses the released worker too");
        assert_eq!(workers.iter().filter(|id| call(id)).count(), 0, "and nobody goes");
        assert!(admission(&state, reset + 1).allows_work(), "the rest wait for the reset itself");
    }

    /// **Item 2 of 2026-10-29.** Work (account 2) at 95% of its five hours, Home (account 1) at
    /// 0% and marked in use. A worker whose provider runs on Work (`ACCOUNT_ENV` = 2) is held on
    /// Work's own windows; on main the gate read the top-level reading, Home's 0%, and admitted
    /// it while it spent Work. A worker on Home goes on.
    #[test]
    fn a_worker_call_is_judged_on_the_account_it_runs_on_not_the_one_marked_in_use() {
        struct Unread;
        impl super::super::Source for Unread {
            fn read(&mut self, _bin: &Path, _cwd: &Path) -> Result<Vec<Window>, ReadError> { Err(ReadError::Failed) }
        }
        let (_root, service, state, scope) = setup();
        service.accounts.rename("1", "Home").unwrap();
        let work = service.accounts.add("Work").unwrap();
        let now = crate::util::now_millis();
        let at = |used: f64| Snapshot { checked_at: Some(now), windows: vec![Window { id: "five_hour".into(),
            label: "Five-hour".into(), used_percent: used, resets_at: Some(now + 3 * 3_600_000), duration_ms: 18_000_000 }],
            ..Default::default() };
        *service.snapshot.lock().unwrap() = at(0.);
        service.extra.lock().unwrap().insert(work.id.clone(),
            (std::sync::Arc::new(std::sync::Mutex::new(Box::new(Unread) as Box<dyn super::super::Source>)), at(95.)));
        service.publish().unwrap();
        assert_eq!(service.accounts.in_use().id, "1", "Home is marked in use");
        let view: View = read_json(&state.join("claude-quota.json")).unwrap();
        assert!(view.accounts.iter().any(|a| a.id == "1" && a.in_use));
        let on_work = wait(&json!({"agent_id": "worker-on-work", "tool_name": "Bash"}), &state, &scope, &work.id,
            Duration::ZERO, Duration::ZERO).expect_err("a worker on Work at 95% is held");
        assert!(refusal(&on_work).starts_with("RichOS desktop quota: Still waiting"), "{}", refusal(&on_work));
        wait(&json!({"agent_id": "worker-on-home", "tool_name": "Bash"}), &state, &scope, "1",
            Duration::ZERO, Duration::ZERO).expect("a worker on Home, at 0%, goes on");
    }

    /// **Review of 2026-10-29, finding 1: every Agent call starts a new agent.** Ten Agent calls
    /// of one session wait at 95%, 19 minutes before the five-hour reset, the team measured at
    /// 1.4 points a minute before the hold (the frame math of the test above: one is 97.66, two
    /// are 100.32). Before, all ten shared the permit `dispatch:<session>`, and once the first
    /// was released the other nine went in the same instant. Each is now its own release: one
    /// goes. Two Agent calls with no `tool_use_id` still get two permits; a helper's own calls
    /// keep its one.
    #[test]
    fn each_agent_dispatch_is_released_on_its_own_permit() {
        let (_root, service, state, scope) = setup();
        // A session that may act, so its Agent calls reach the gate (`authorized`).
        atomic_write(&scope, &json!({"version":1,"actions_allowed":true})).unwrap();
        let (now, minute) = (crate::util::now_millis(), 60_000);
        let reset = now + 19 * minute;
        let five = |used: f64| vec![Window { id: "five_hour".into(), label: "Five-hour".into(), used_percent: used,
            resets_at: Some(reset), duration_ms: 18_000_000 }];
        let mut reading = Snapshot::default();
        reading.accept(five(80.), now - 15 * minute);
        reading.accept(five(95.), now - 5 * minute);
        reading.accept(five(95.), now);
        *service.snapshot.lock().unwrap() = reading;
        service.publish().unwrap();
        let calls: Vec<Value> = (0..10)
            .map(|i| json!({"tool_name": "Agent", "session_id": "stu", "tool_use_id": format!("toolu_{i}")})).collect();
        // The ten waiting dispatches' own pause records, each on its own permit, behind the ten
        // held helpers whose pace the 1.4 points a minute measured.
        let _waiting: Vec<_> = (0..10).map(|i| super::super::holds::Guard::begin(&state, serde_json::from_value(json!({
            "kind": "dispatch", "id": "dispatch", "entityId": "", "threadId": "", "sessionId": "", "name": "New agent",
            "task": null, "sinceAt": now, "releasedAt": null, "account": "1", "key": format!("dispatch:stu:toolu_{i}")})).unwrap()).unwrap()).collect();
        let _helpers = held_helpers(&state, now, 10);
        let went = calls.iter().filter(|call| wait(call, &state, &scope, "1", Duration::ZERO, Duration::ZERO).is_ok()).count();
        assert_eq!(went, 1, "one new agent, not ten in the same instant");
        assert_eq!(release_key(&calls[3]), "dispatch:stu:toolu_3");
        let bare = json!({"tool_name": "Agent", "session_id": "stu"});
        assert_ne!(release_key(&bare), release_key(&bare), "no tool_use_id: still one permit per call");
        let helper = json!({"tool_name": "Bash", "agent_id": "worker-1", "session_id": "w"});
        assert_eq!(release_key(&helper), release_key(&helper), "a helper's calls share its one permit");
    }

    /// `count` held helpers' pause records on account 1, `worker-0` onwards, as each held call
    /// keeps one (`holds`).
    fn held_helpers(state: &Path, now: u64, count: usize) -> Vec<super::super::holds::Guard> {
        (0..count).map(|i| super::super::holds::Guard::begin(state, serde_json::from_value(json!({
            "kind": "agent", "id": format!("worker-{i}"), "entityId": "", "threadId": "", "sessionId": "",
            "name": "Helper", "task": null, "sinceAt": now, "releasedAt": null, "account": "1",
            "key": format!("worker-{i}")})).unwrap()).unwrap()).collect()
    }

    /// **Second review of 2026-10-29, finding 1.** One helper held at 95%, 19 minutes before the
    /// reset, measured at 1.4 points a minute before the hold, with nine Agent dispatches queued
    /// behind it. Its pace is the whole measured pace: 95 + 1.4 x 19 = 121.6, so it waits.
    /// Before, the nine never-started dispatches counted in the team and cut its share to a
    /// tenth (97.66): it went, and spent the account past 100%.
    #[test]
    fn queued_dispatches_never_dilute_the_pace_a_working_helper_is_judged_on() {
        let (_root, service, state, scope) = setup();
        let (now, minute) = (crate::util::now_millis(), 60_000);
        let reset = now + 19 * minute;
        let five = |used: f64| vec![Window { id: "five_hour".into(), label: "Five-hour".into(), used_percent: used,
            resets_at: Some(reset), duration_ms: 18_000_000 }];
        let mut reading = Snapshot::default();
        reading.accept(five(80.), now - 15 * minute);
        reading.accept(five(95.), now - 5 * minute);
        reading.accept(five(95.), now);
        *service.snapshot.lock().unwrap() = reading;
        service.publish().unwrap();
        let _queued: Vec<_> = (0..9).map(|i| super::super::holds::Guard::begin(&state, serde_json::from_value(json!({
            "kind": "dispatch", "id": "dispatch", "entityId": "", "threadId": "", "sessionId": "", "name": "New agent",
            "task": null, "sinceAt": now, "releasedAt": null, "account": "1", "key": format!("dispatch:stu:toolu_{i}")})).unwrap()).unwrap()).collect();
        let _helper = held_helpers(&state, now, 1);
        assert_eq!(measured_team(&state, "1", &[]), 1, "the nine dispatches are not in the measured team");
        assert!(wait(&json!({"agent_id": "worker-0", "tool_name": "Bash"}), &state, &scope, "1",
            Duration::ZERO, Duration::ZERO).is_err(), "121.6% projected: the helper waits for the reset");
    }

    #[test]
    fn foreground_tools_are_not_quota_gated_but_new_agents_are() {
        let (_root, _service, state, scope) = setup();
        wait(
            &json!({"tool_name":"Read"}),
            &state,
            &scope,
            "1",
            Duration::ZERO,
            Duration::ZERO,
        )
        .unwrap();
        atomic_write(&scope, &json!({"version":1,"actions_allowed":true})).unwrap();
        wait(&json!({"tool_name":"Agent"}),&state,&scope,"1",Duration::ZERO,Duration::ZERO).unwrap();
        publish(&_service,3_600_000);
        assert!(wait(
            &json!({"tool_name":"Agent"}),
            &state,
            &scope,
            "1",
            Duration::ZERO,
            Duration::ZERO
        )
        .is_err());
    }

    /// **Two-day cleanup O1.2:** a dispatch during the hold stayed pending with nothing said for
    /// as long as the hold lasted (up to the 21,500 s budget), then was told only "Still waiting".
    /// It is refused before any wait, saying which account is held, why and until when, and it
    /// leaves no pause record behind. With no budget the verdict is the text, never a clock: on
    /// main this call answered "Still waiting".
    #[test]
    fn a_dispatch_during_the_hold_is_refused_at_once_with_the_reason() {
        let (_root, service, state, scope) = setup();
        atomic_write(&scope, &json!({"version":1,"actions_allowed":true})).unwrap();
        publish(&service, 3_600_000);
        let resets = service.snapshot.lock().unwrap().windows[0].resets_at.unwrap();
        let dispatch = json!({"tool_name":"Agent","session_id":"stu","tool_use_id":"toolu_held"});
        let said = wait(&dispatch, &state, &scope, "1", Duration::ZERO, Duration::ZERO)
            .expect_err("a dispatch during the hold went through");
        let said = refusal(&said);
        assert!(said.contains("No new agent was started"), "{said}");
        assert!(said.contains(&format!("until {}", utc(resets))), "no end named: {said}");
        assert!(said.contains("pause point set in RichOS (93%)"), "no reason named: {said}");
        assert!(super::super::holds::held_calls(&state).unwrap().is_empty(), "a refused dispatch left a pause record");
        // A helper's own call still waits for the release, keeping its context.
        assert!(wait(&json!({"agent_id":"worker-1","tool_name":"Bash"}), &state, &scope, "1",
            Duration::ZERO, Duration::ZERO).unwrap_err().to_string().starts_with("Still waiting"));
    }

    // ---- a document written for him comes out American (CEO §93) ----------------------------

    /// A stand-in for `app-engine-hook.py`: records the bytes it was given, prints `stdout` and
    /// exits with `code`.
    fn canonical(dir: &Path, stdout: &str, code: i32) -> (std::path::PathBuf, std::path::PathBuf) {
        let seen = dir.join("seen.json");
        let script = dir.join("canonical.sh");
        fs::write(dir.join("stdout.txt"), stdout).unwrap();
        fs::write(
            &script,
            format!("#!/bin/sh\ncat > '{}'\ncat '{}'\nexit {code}\n", seen.display(), dir.join("stdout.txt").display()),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        }
        (script, seen)
    }

    fn document_write(dir: &Path, name: &str) -> (Value, String) {
        let body = crate::american_spelling::tests::section("report");
        let payload = json!({"hook_event_name": "PreToolUse", "tool_name": "Write", "agent_id": "worker-1",
            "tool_input": {"file_path": dir.join(name).to_str().unwrap(), "content": body}});
        (payload, body)
    }

    #[test]
    fn a_prose_document_write_is_fixed_inside_the_canonical_hook_s_one_envelope() {
        let root = Scratch::new();
        let worker = r#"{"hookSpecificOutput":{"hookEventName":"PreToolUse","additionalContext":"Host-verified assignment."}}"#;
        let (script, seen) = canonical(root.path(), &format!("{worker}\n"), 0);
        let (payload, body) = document_write(root.path(), "summary.md");
        let bytes = serde_json::to_vec(&payload).unwrap();
        let mut out = Vec::new();
        let code = run_canonical(script.as_os_str(), std::iter::empty(), &bytes, &payload, &mut out).unwrap();
        assert_eq!(code, 0);
        // The canonical hook judged the ORIGINAL call, byte for byte.
        assert_eq!(fs::read(&seen).unwrap(), bytes);
        // One envelope, carrying both the hook's context and the fix, and no decision.
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text.trim().lines().count(), 1, "exactly one JSON object: {text}");
        let envelope: Value = serde_json::from_str(text.trim()).unwrap();
        let specific = &envelope["hookSpecificOutput"];
        assert_eq!(specific["hookEventName"], "PreToolUse");
        assert!(specific.get("permissionDecision").is_none(), "a fix must never approve the write");
        assert_eq!(specific["updatedInput"]["content"].as_str().unwrap(), crate::american_spelling::fix(&body));
        assert_eq!(specific["updatedInput"]["file_path"], payload["tool_input"]["file_path"]);
        let context = specific["additionalContext"].as_str().unwrap();
        assert!(context.starts_with("Host-verified assignment."), "{context}");
        assert!(context.contains("American spelling: this write was changed before it ran, 5 word(s)"), "{context}");
    }

    #[test]
    fn a_refused_or_code_or_clean_write_passes_through_exactly_as_the_canonical_hook_answered() {
        let root = Scratch::new();
        // Refused: the hook's own words and exit code, untouched.
        let (script, _) = canonical(root.path(), "refused\n", 2);
        let (payload, _) = document_write(root.path(), "summary.md");
        let bytes = serde_json::to_vec(&payload).unwrap();
        let mut out = Vec::new();
        assert_eq!(run_canonical(script.as_os_str(), std::iter::empty(), &bytes, &payload, &mut out).unwrap(), 2);
        assert_eq!(out, b"refused\n");
        // A decision already made is never overridden.
        let (script, _) = canonical(root.path(), r#"{"decision":"block","reason":"no"}"#, 0);
        let mut out = Vec::new();
        run_canonical(script.as_os_str(), std::iter::empty(), &bytes, &payload, &mut out).unwrap();
        assert_eq!(out, br#"{"decision":"block","reason":"no"}"#);
        // A code file: nothing is captured or merged (the hook writes to the inherited stdout).
        let (script, _) = canonical(root.path(), "", 0);
        let (payload, _) = document_write(root.path(), "main.rs");
        let bytes = serde_json::to_vec(&payload).unwrap();
        let mut out = Vec::new();
        assert_eq!(run_canonical(script.as_os_str(), std::iter::empty(), &bytes, &payload, &mut out).unwrap(), 0);
        assert!(out.is_empty());
        // A tool that writes no document: the same.
        let payload = json!({"tool_name": "Bash", "tool_input": {"command": "true"}});
        let bytes = serde_json::to_vec(&payload).unwrap();
        let mut out = Vec::new();
        assert_eq!(run_canonical(script.as_os_str(), std::iter::empty(), &bytes, &payload, &mut out).unwrap(), 0);
        assert!(out.is_empty());
    }
}

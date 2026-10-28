//! Desktop hook wrapper. A hold keeps the provider's tool call pending, so the
//! worker retains its context and never emits a false SubagentStop receipt.
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

pub(super) fn admission(state: &Path, now: u64) -> Admission {
    let Ok(view) = read_json::<View>(&state.join("claude-quota.json")) else {
        return Admission::Unknown;
    };
    // Read the persisted switch separately: a publication failure must not leave
    // an old disabled snapshot admitting work after the policy was enabled.
    let policy = state
        .parent()
        .and_then(|p| read_json::<Policy>(&p.join("claude-quota-policy.json")).ok())
        .unwrap_or(view.policy);
    // **Every account exhausted** (fill-first, Frank's finding 2): held until the soonest
    // reset among them, whatever the pause switch says, because there is no account left for
    // the work to run on. Never set with one account.
    if let Some(until) = view.held_until.filter(|t| *t > now) {
        return Admission::Held { resets_at: until };
    }
    if !policy.enabled {
        return Admission::Disabled;
    }
    if policy.validate().is_err() || view.checked_at.is_some_and(|t| t > now) {
        return Admission::Unknown;
    }
    Snapshot {
        windows: view.windows,
        checked_at: view.checked_at,
        retry_at: view.retry_at,
        error: view.message.map(|_| ReadError::Failed),
        speeds: view.speeds,
        // A check that came back with no figures lets work run (`Admission::NoReading`); the
        // gate decides from the same fact the service does.
        empty_at: view.empty_at,
        ..Default::default()
    }
    .view(policy, now)
    .admission
}

fn authorized(scope: &Path, worker: bool) -> bool {
    read_json::<Value>(scope).is_ok_and(|v| {
        v["version"] == 1
            && (v["actions_allowed"] == true || (worker && v["background_work_allowed"] == true))
    })
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

/// The account the published file's figures describe, for a refusal: "Work (account 2)".
fn in_use_named(state: &Path) -> String {
    match read_json::<View>(&state.join("claude-quota.json")).ok().and_then(|v| v.in_use) {
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
        return Some(Err(io::Error::other(WORK_ENDED)));
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
    loop {
        if !authorized(scope, worker) {
            return Err(io::Error::other(WORK_ENDED));
        }
        if admission(state, crate::util::now_millis()).allows_work() {
            if let Some(guard) = observation.take() { guard.release(); }
            return Ok(());
        }
        if observation.is_none() {
            // Observation failure must not let a paused tool through the gate.
            observation = super::holds::from_hook(state, scope, payload)
                .and_then(|row| super::holds::Guard::begin(state, row)).ok();
        }
        if started.elapsed() >= budget {
            // Named (follow-ups 7fed0bef item 3): the hold is the account in use's, and a reader
            // of this refusal must not take it for another account's.
            return Err(io::Error::other(format!(
                "Still waiting for a current allowance reading on {}. No tool action was run.",
                in_use_named(state),
            )));
        }
        std::thread::sleep(tick);
    }
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
    if text == WORK_ENDED {
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
        assert!(admission(&state, now + 60_001).allows_work());
        service.snapshot.lock().unwrap().windows[1].used_percent = 0.;
        service.publish().unwrap();
        assert_eq!(admission(&state, now), Admission::Ready);
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
        assert_eq!(wait(&dispatch,&state,&scope,"1",Duration::ZERO,Duration::ZERO).unwrap_err().to_string(),WORK_ENDED);
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
    /// pass as before and its new `Agent` dispatch is held, never ordered.
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
        assert!(dispatch.starts_with("RichOS desktop quota: Still waiting"), "{dispatch}");
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

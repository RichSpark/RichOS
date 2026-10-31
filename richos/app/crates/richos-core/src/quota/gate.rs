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
    decide_call(state, account, false, now)
}

/// [`decide`] for one tool call; `helper` when a running helper makes it (the payload's
/// `agent_id`).
///
/// **A helper running on an account being left carries on**, at a weekly switch and at a
/// five-hour one alike (the CEO's §153, 2026-10-31: "why would I want there to be any difference
/// between weekly account switch and the 5-hour-quota handling"; it replaces his weekly handoff
/// order of 2026-10-07). Nobody is told to stop, commit or hand off; Stu moves once nothing of
/// his runs (`work_host.rs`, `Renewal::Waits`), and a helper's next Claude call goes on, on the
/// account it runs on, in the room the switch point leaves before 100% (`Reading::weekly_point`,
/// `Reading::act_point`). So while another account is there to move to, a helper's call on an
/// account being left (its row used up: `exhaustedUntil`, which the weekly point and, with the
/// setting on Switch, the five-hour line set; or published as `leaving`) is admitted, held by
/// neither window. A new agent's dispatch is not a helper's call and is still held there, so no
/// new work starts on the account being left. With no account to move to (one account, every
/// account used up, or the setting on Pause, where the five-hour line pauses the team and moves
/// nobody) there is no switch, and the hold holds the helper as before.
fn decide_call(state: &Path, account: Option<&str>, helper: bool, now: u64) -> (Admission, Option<super::stagger::Release>) {
    let Ok(view) = read_json::<View>(&state.join("claude-quota.json")) else {
        return (Admission::Unknown, None);
    };
    let carries_on = helper && view.accounts.len() > 1 && account.is_some_and(|id|
        view.leaving.iter().any(|left| left == id)
            || view.accounts.iter().any(|a| a.id == id && a.exhausted_until.is_some_and(|t| t > now)));
    // Read the persisted switch separately: a publication failure must not leave
    // an old disabled snapshot admitting work after the policy was enabled.
    let policy = state
        .parent()
        .and_then(|p| read_json::<Policy>(&p.join("claude-quota-policy.json")).ok())
        .unwrap_or(view.policy.clone());
    let held = view.held_until.filter(|t| *t > now);
    if held.is_none() && !policy.enabled {
        return (Admission::Disabled, None);
    }
    let row = account.and_then(|id| view.accounts.iter().find(|a| a.id == id));
    let exhausted_until = row.and_then(|a| a.exhausted_until);
    let (windows, checked_at, message, speeds, pre_hold, empty_at) = match row {
        Some(a) => (a.windows.clone(), a.checked_at, a.message.clone(), a.speeds.clone(), a.pre_hold.clone(), a.empty_at),
        None => (view.windows, view.checked_at, view.message, view.speeds, view.pre_hold, view.empty_at),
    };
    if carries_on && held.is_none() {
        return (Admission::Ready, None);
    }
    let judged = (policy.enabled && policy.validate().is_ok() && checked_at.is_none_or(|t| t <= now)).then(|| Snapshot {
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
    .view(policy, now));
    // **Every account exhausted** (fill-first, Frank's finding 2): held until the soonest
    // reset among them, whatever the pause switch says, because there is no account left for
    // the work to run on. Never set with one account.
    //
    // **The switch path is released staggered too** (STATUS-2026-10-31 row 7, E1): with the
    // setting on Switch an account at its five-hour line is used up (`claude_accounts::gone`), so
    // with every account there this hold returned before the call's own row was read, and nobody
    // went until the reset, where Pause on the same readings releases one at a time inside the
    // twenty minutes. A call whose own account is used up by that five-hour window alone (its
    // `exhaustedUntil` is that window's reset) gets the same release as on the hold path; a
    // weekly limit keeps the hold with no release.
    if let Some(until) = held {
        let release = judged.and_then(|v| v.release)
            .filter(|r| exhausted_until.is_some_and(|t| super::same_reset(t, r.resets_at)));
        return (Admission::Held { resets_at: until }, release);
    }
    match judged {
        Some(decided) => (decided.admission, decided.release),
        None => (Admission::Unknown, None),
    }
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

/// One tool call of helper `agent` on `account` through the gate, answered at once (no wait):
/// what the call is told when refused. For the work host's tests.
#[cfg(test)]
pub(crate) fn helper_call(state: &Path, scope: &Path, account: &str, agent: &str) -> Result<(), String> {
    let payload = serde_json::json!({"hook_event_name": "PreToolUse", "agent_id": agent, "session_id": "stu-session",
        "tool_name": "Bash", "tool_input": {"command": "cargo test"}});
    wait(&payload, state, scope, account, Duration::ZERO, Duration::ZERO).map_err(|e| refusal(&e))
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
        let (admission, release) = decide_call(state, Some(account), worker, now);
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
        assert!(service.view().leaving.is_empty(),"an expired weekly window left the account");
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

    /// **A weekly switch tells no helper to stop: a helper running on the account being left
    /// carries on** (the CEO's §153, 2026-10-31). Account 1 at 99% of its week is being left for
    /// Work (3%), with automatic pause on, so the weekly hold stands on Account 1's row. On main a
    /// helper on Account 1 was refused with the order to commit, hand off and end
    /// (`HANDOFF_ORDER`), and a marker was written for it. Now its calls are admitted, every
    /// time, with no order, no refusal and nothing written, Account 1's five hours at their line
    /// too (the account is being left, so neither window holds a helper on it). A new agent's
    /// dispatch on Account 1 is not a helper's call: it is refused with the hold's reason, so
    /// nothing new starts on the account being left. With one account there is
    /// nowhere to move to and no switch: the weekly hold holds the helper, with no order.
    #[test]
    fn a_weekly_switch_orders_no_helper_and_a_helper_on_the_account_being_left_carries_on() {
        let (_root, service, state, scope) = setup();
        let now = crate::util::now_millis();
        let windows = |five: f64, weekly: f64| vec![
            Window { id: "five_hour".into(), label: "Five-hour".into(), used_percent: five,
                resets_at: Some(now + 3_600_000), duration_ms: 18_000_000 },
            Window { id: "seven_day".into(), label: "Weekly".into(), used_percent: weekly,
                resets_at: Some(now + 48 * 3_600_000), duration_ms: 604_800_000 },
        ];
        // One account first: no switch is possible, so the weekly hold holds the helper.
        *service.snapshot.lock().unwrap() = Snapshot { checked_at: Some(now), windows: windows(10., 99.), ..Default::default() };
        service.publish().unwrap();
        let helper = json!({"agent_id":"helper-a","session_id":"session-1","tool_name":"Bash"});
        let alone = refusal(&wait(&helper, &state, &scope, "1", Duration::ZERO, Duration::ZERO).unwrap_err());
        assert!(alone.starts_with("RichOS desktop quota: Still waiting"), "one account: held, never ordered: {alone}");

        let work = service.accounts.add("Work").unwrap();
        service.observe_for_test(&work.id, windows(10., 3.), now);
        service.observe_for_test("1", windows(10., 99.), now);
        assert_eq!(service.accounts.in_use().id, work.id, "Account 1 at 99% of its week is left for Work");
        let view: View = read_json(&state.join("claude-quota.json")).unwrap();
        assert_eq!(view.leaving, vec!["1".to_string()]);
        assert!(matches!(decide(&state, Some("1"), now).0, Admission::Held { .. }), "the weekly hold stands on Account 1");

        for _ in 0..3 {
            wait(&helper, &state, &scope, "1", Duration::ZERO, Duration::ZERO)
                .expect("a helper on the account being left carries on: no order, no hold");
        }
        assert!(!state.join("handoffs").exists(), "nothing was written for a handoff");
        let other = json!({"agent_id":"helper-b","session_id":"session-2","tool_name":"Bash"});
        wait(&other, &state, &scope, &work.id, Duration::ZERO, Duration::ZERO).expect("a helper on Work goes on");

        // Its five hours at their line as well: the account is still being left, so it carries on.
        service.before_turn("1", Some((windows(95., 99.), crate::util::now_millis())));
        wait(&helper, &state, &scope, "1", Duration::ZERO, Duration::ZERO).expect("at the five-hour line too it carries on");

        atomic_write(&scope, &json!({"version":1,"actions_allowed":true})).unwrap();
        let dispatch = refusal(&wait(&json!({"tool_name":"Agent","session_id":"session-1"}), &state, &scope, "1",
            Duration::ZERO, Duration::ZERO).unwrap_err());
        assert!(dispatch.starts_with("RichOS desktop quota: No new agent was started"), "{dispatch}");
    }

    /// **A five-hour switch is handled exactly as a weekly one** (the CEO's §153, 2026-10-31: "why
    /// would I want there to be any difference between weekly account switch and the 5-hour-quota
    /// handling"). Account 1 at 95% of its five hours, its week at 20%; Work at 10%. With the
    /// setting on Switch the switch to Work is decided and Account 1 is used up by its five hours
    /// (`exhaustedUntil`), so a helper still running on it carries on: every call admitted, with
    /// no refusal. On main it was held at its next call until that window reset (and Stu's move
    /// waited for it). A new agent's dispatch on Account 1 is still refused. With the setting on
    /// Pause there is no switch: the five-hour line pauses the team, the helper included, as
    /// before.
    #[test]
    fn a_five_hour_switch_lets_a_helper_on_the_account_being_left_carry_on_as_a_weekly_one_does() {
        use crate::claude_accounts::AtThreshold;
        let (_root, service, state, scope) = setup();
        let now = crate::util::now_millis();
        let windows = |five: f64| vec![
            Window { id: "five_hour".into(), label: "Five-hour".into(), used_percent: five,
                resets_at: Some(now + 3_600_000), duration_ms: 18_000_000 },
            Window { id: "seven_day".into(), label: "Weekly".into(), used_percent: 20.,
                resets_at: Some(now + 48 * 3_600_000), duration_ms: 604_800_000 },
        ];
        let work = service.accounts.add("Work").unwrap();
        service.set_at_threshold(AtThreshold::Switch).unwrap();
        service.observe_for_test(&work.id, windows(10.), now);
        service.observe_for_test("1", windows(95.), now);
        assert_eq!(service.accounts.in_use().id, work.id, "Account 1 at its five-hour line is left for Work");
        assert!(matches!(decide(&state, Some("1"), now).0, Admission::Held { .. }), "the five-hour hold stands on Account 1");
        let helper = json!({"agent_id":"helper-a","session_id":"session-1","tool_name":"Bash"});
        for _ in 0..3 {
            wait(&helper, &state, &scope, "1", Duration::ZERO, Duration::ZERO)
                .expect("a helper on the account being left carries on at a five-hour switch");
        }
        atomic_write(&scope, &json!({"version":1,"actions_allowed":true,"background_work_allowed":true})).unwrap();
        let dispatch = refusal(&wait(&json!({"tool_name":"Agent","session_id":"session-1"}), &state, &scope, "1",
            Duration::ZERO, Duration::ZERO).unwrap_err());
        assert!(dispatch.starts_with("RichOS desktop quota: No new agent was started"), "{dispatch}");

        service.set_at_threshold(AtThreshold::Pause).unwrap();
        service.observe_for_test("1", windows(95.), crate::util::now_millis());
        let held = refusal(&wait(&helper, &state, &scope, "1", Duration::ZERO, Duration::ZERO).unwrap_err());
        assert!(held.starts_with("RichOS desktop quota: Still waiting"), "Pause moves nobody, so the helper pauses: {held}");
    }

    /// **A model's own week at its point is the account's week at its point** (follow-ups of
    /// 2026-10-29, `model:Fable`). Account 1 alone, its week at 40% and its Fable week at 99%:
    /// the published file names it leaving, the gate holds a new call until the Fable week's
    /// reset, and so does a helper's next call on it (one account: no switch). On main only `seven_day` was read: not
    /// leaving, admitted, and the work ran on to Fable's usage-limit refusal.
    #[test]
    fn a_models_own_week_at_its_point_leaves_and_holds_the_account() {
        let (_root, service, state, scope) = setup();
        let now = crate::util::now_millis();
        let fable_reset = now + 48 * 3_600_000;
        *service.snapshot.lock().unwrap() = Snapshot {
            checked_at: Some(now),
            windows: vec![
                Window { id: "five_hour".into(), label: "Five-hour".into(), used_percent: 10.,
                    resets_at: Some(now + 3_600_000), duration_ms: 18_000_000 },
                Window { id: "seven_day".into(), label: "Weekly".into(), used_percent: 40.,
                    resets_at: Some(now + 72 * 3_600_000), duration_ms: 604_800_000 },
                Window { id: "model:Fable".into(), label: "Weekly · Fable".into(), used_percent: 99.,
                    resets_at: Some(fable_reset), duration_ms: 604_800_000 },
            ],
            ..Default::default()
        };
        service.publish().unwrap();
        let view: View = read_json(&state.join("claude-quota.json")).unwrap();
        assert_eq!(view.leaving, vec!["1".to_string()], "the Fable week decides `leaving`");
        assert_eq!(admission(&state, now), Admission::Held { resets_at: fable_reset });
        let helper = json!({"agent_id":"helper-a","session_id":"session-1","tool_name":"Bash"});
        let held = refusal(&wait(&helper, &state, &scope, "1", Duration::ZERO, Duration::ZERO).unwrap_err());
        assert!(held.starts_with("RichOS desktop quota: Still waiting"), "one account: held, never ordered: {held}");
    }

    /// **The switch path gets the staggered release too** (STATUS-2026-10-31 row 7, E1 / G12;
    /// follow-ups of 2026-10-29: "with Switch set and another account free, nothing bursts onto
    /// the old account in the 20-minute window"). Work at 95% of its five hours, 19 minutes before
    /// its reset, the team measured at 1.4 points a minute before the hold (the frame math of the
    /// ten-worker test above: one is 95 + 0.14 x 19 = 97.66, two are 100.32), ten workers held on
    /// Work, the setting on Switch.
    /// - Home free (10%): Work is being left, so its helpers carry on, all ten (the CEO's §153: a
    ///   five-hour switch is handled as a weekly one; before, one went and the rest waited).
    /// - Home at its line too (95%): with Switch, an account at its five-hour line is used up
    ///   (`claude_accounts::gone`), so every account was, and the "every account used up" hold
    ///   returned before Work's row was read: nobody went until the reset, where Pause on the
    ///   same readings releases one at a time. Now one goes, as with Pause.
    /// - Work's week at 99% as well: a weekly limit keeps the hold with no release; only an
    ///   account used up by its five-hour window alone is released.
    #[test]
    fn on_the_switch_path_the_twenty_minute_window_is_released_staggered_as_on_the_hold_path() {
        use crate::claude_accounts::AtThreshold;
        struct Unread;
        impl super::super::Source for Unread {
            fn read(&mut self, _bin: &Path, _cwd: &Path) -> Result<Vec<super::super::Window>, ReadError> { Err(ReadError::Failed) }
        }
        let released = |at_threshold: AtThreshold, home: f64, work_week: f64| -> usize {
            let (_root, service, state, scope) = setup();
            service.accounts.rename("1", "Home").unwrap();
            let work = service.accounts.add("Work").unwrap();
            service.set_at_threshold(at_threshold).unwrap();
            let (now, minute) = (crate::util::now_millis(), 60_000);
            let five = |used: f64, resets_at: u64| vec![Window { id: "five_hour".into(), label: "Five-hour".into(),
                used_percent: used, resets_at: Some(resets_at), duration_ms: 18_000_000 }];
            let reset = now + 19 * minute;
            let mut climbing = Snapshot::default();
            let work_windows = |used: f64| {
                let mut windows = five(used, reset);
                windows.push(Window { id: "seven_day".into(), label: "Weekly".into(), used_percent: work_week,
                    resets_at: Some(now + 48 * 3_600_000), duration_ms: 604_800_000 });
                windows
            };
            climbing.accept(work_windows(80.), now - 15 * minute);
            climbing.accept(work_windows(95.), now - 5 * minute);
            climbing.accept(work_windows(95.), now);
            service.extra.lock().unwrap().insert(work.id.clone(),
                (std::sync::Arc::new(std::sync::Mutex::new(Box::new(Unread) as Box<dyn super::super::Source>)), climbing));
            *service.snapshot.lock().unwrap() = Snapshot { checked_at: Some(now), windows: five(home, now + 3 * 3_600_000),
                ..Default::default() };
            service.publish().unwrap();
            let workers: Vec<String> = (0..10).map(|i| format!("worker-{i}")).collect();
            let _waiting: Vec<_> = workers.iter().map(|id| super::super::holds::Guard::begin(&state, serde_json::from_value(json!({
                "kind": "agent", "id": id, "entityId": "", "threadId": "", "sessionId": "", "name": id,
                "task": null, "sinceAt": now, "releasedAt": null})).unwrap()).unwrap()).collect();
            workers.iter().filter(|id| wait(&json!({"agent_id": id, "tool_name": "Bash"}), &state, &scope, &work.id,
                Duration::ZERO, Duration::ZERO).is_ok()).count()
        };
        assert_eq!(released(AtThreshold::Switch, 10., 20.), 10, "Home free: Work is being left, so its running helpers carry on");
        assert_eq!(released(AtThreshold::Pause, 95., 20.), 1, "Pause, both at their line: one at a time");
        assert_eq!(released(AtThreshold::Switch, 95., 20.), 1, "Switch, both at their line: the same staggered release as Pause");
        assert_eq!(released(AtThreshold::Switch, 95., 99.), 0, "Work at its weekly limit: held, nobody released");
    }
    /// **One account, automatic pause off: no order at the weekly point.** The fast-use alert
    /// says "Automatic pause is off in Settings, so nothing acts before it reaches 100%", so a
    /// helper on Account 1 at 99% of its week keeps working: not leaving, no order, no marker.
    #[test]
    fn one_account_with_automatic_pause_off_keeps_a_helper_working_at_the_weekly_point() {
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
            .expect("the helper keeps working");
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

    /// **Row 8 (E2), Sage 2026-10-31, `row8-stu-gate.md`:** the 93% rule pauses what the lead
    /// runs and starts, never the lead itself (§87 names subagents; §146 terminal parity: Rich is
    /// never paused). Pinned with the hold in force: the old test checked the lead's `Read`
    /// before any reading was published, so it proved nothing about the hold. Deleting the
    /// lead's exemption in `wait()` turns the first group red.
    #[test]
    fn the_lead_is_never_held_at_the_pause_point_only_what_it_starts_is() {
        let (_root, service, state, scope) = setup();
        atomic_write(&scope, &json!({"version":1,"actions_allowed":true})).unwrap();
        publish(&service, 3_600_000);
        let now = crate::util::now_millis();
        assert!(matches!(admission(&state, now), Admission::Held { .. }), "94% did not hold");
        for tool in ["Read", "Bash", "mcp__richos_work__inspect", "mcp__richos_status__send"] {
            wait(&json!({"tool_name": tool, "session_id": "stu"}), &state, &scope, "1",
                Duration::ZERO, Duration::ZERO)
                .unwrap_or_else(|e| panic!("the lead's own {tool} was held: {e}"));
        }
        let dispatch = wait(&json!({"tool_name": "Agent", "session_id": "stu"}), &state, &scope, "1",
            Duration::ZERO, Duration::ZERO).expect_err("a new agent started during the hold");
        let said = refusal(&dispatch);
        assert!(said.starts_with("RichOS desktop quota: No new agent was started"), "{said}");
        let helper = wait(&json!({"agent_id": "worker-1", "tool_name": "Bash"}), &state, &scope, "1",
            Duration::ZERO, Duration::ZERO).expect_err("a helper's call ran during the hold");
        assert!(helper.to_string().starts_with("Still waiting"), "{helper}");
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

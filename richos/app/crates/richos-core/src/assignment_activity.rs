//! A current display of assignment work, derived from the provider and the work receipts.
use crate::app_cleanup::{process_start, ProcessStart};
use crate::assignment::{Assignment, AssignmentState};
use crate::status_tools::StatusToolScope;
use crate::work_status::{self, Receipts};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Cause {
    Handoff(String),
    WorkerResult(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Turn {
    pub session: String,
    pub started_at: u64,
    pub cause: Cause,
}

#[derive(Serialize, Deserialize)]
struct PublishedTurn {
    owner: u32,
    owner_start: String,
    supervised: bool,
    turn: Turn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Display {
    pub state: Option<AssignmentState>,
    pub working_since: Option<u64>,
}

fn path(scope: &StatusToolScope) -> Result<PathBuf, String> {
    Ok(crate::session_tools::inbox(scope, "stu")?.with_file_name("assignment-activity.observation"))
}

pub(crate) fn publish(scope: &StatusToolScope, turn: Option<&Turn>) -> Result<(), String> {
    let path = path(scope)?;
    let Some(turn) = turn else {
        return std::fs::remove_file(path).or_else(|error| {
            if error.kind() == std::io::ErrorKind::NotFound { Ok(()) } else { Err(error) }
        }).map_err(|error| error.to_string());
    };
    let owner = std::process::id();
    let ProcessStart::Running(owner_start) = process_start(owner) else {
        return Err("Could not verify the work host process.".into());
    };
    let supervised = crate::lease_commands::LiveLeases::process().contains(&turn.session);
    crate::quota::atomic_write(&path, &PublishedTurn { owner, owner_start, supervised, turn: turn.clone() })
        .map_err(|error| error.to_string())
}

fn valid_session(session: &str) -> bool {
    !session.is_empty() && session.len() <= 128
        && session.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

fn verified_lease(state: &Path, session: &str, owner: u32) -> bool {
    if !valid_session(session) {
        return false;
    }
    let path = crate::lease_commands::state_path(state, session);
    let Some(value) = bounded_json::<serde_json::Value>(&path) else { return false };
    value["owner"].as_u64() == Some(owner.into())
        && value["ended"] == false
        && value["provider"].as_u64().and_then(|pid| u32::try_from(pid).ok())
            .is_some_and(|pid| pid > 0 && matches!(process_start(pid), ProcessStart::Running(_)))
}

fn bounded_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > 128 * 1024 {
        return None;
    }
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

pub(crate) fn read_turn(scope: &StatusToolScope) -> Option<Turn> {
    let saved: PublishedTurn = bounded_json(&path(scope).ok()?)?;
    if !matches!(process_start(saved.owner), ProcessStart::Running(now) if now == saved.owner_start) {
        return None;
    }
    if saved.supervised && !verified_lease(&scope.state_root, &saved.turn.session, saved.owner) {
        return None;
    }
    Some(saved.turn)
}

fn provider_running(scope: &StatusToolScope, session: &str, hosted: &[String]) -> bool {
    if hosted.iter().any(|id| id == session) {
        return true;
    }
    let state = &scope.state_root;
    let recorded = work_status::session_record(state, &scope.entity_id, &scope.thread_id, session)
        .ok().flatten()
        .filter(|record| !record.ended)
        .and_then(|record| record.process)
        .is_some_and(|(pid, start)| matches!(process_start(pid), ProcessStart::Running(now) if now == start));
    recorded || (valid_session(session) && bounded_json::<serde_json::Value>(
        &crate::lease_commands::state_path(state, session),
    ).and_then(|v| v["owner"].as_u64())
        .and_then(|pid| u32::try_from(pid).ok())
        .is_some_and(|owner| verified_lease(state, session, owner)))
}

/// The read-only status tool runs in its own process. It reads the same projection
/// using the host's process-checked turn observation.
pub(crate) fn read(scope: &StatusToolScope, rows: &[Assignment]) -> Vec<Display> {
    static INDEX: std::sync::Mutex<Option<work_status::ReceiptIndex>> = std::sync::Mutex::new(None);
    let turn = read_turn(scope);
    let receipts = INDEX.lock().unwrap().get_or_insert_with(Default::default)
        .launches(&scope.state_root, &scope.entity_id, &scope.thread_id).ok();
    project(scope, rows, receipts.as_ref(), &[], turn.as_ref(), &HashSet::new())
}

/// Current activity is attributed to the assignment whose handoff, command or worker result
/// is being handled, or whose named helper is running. An open record preserves history and
/// keeps its recovery path available when current activity cannot be confirmed.
pub(crate) fn project(
    scope: &StatusToolScope,
    rows: &[Assignment],
    receipts: Option<&Receipts>,
    hosted: &[String],
    turn: Option<&Turn>,
    command_jobs: &HashSet<&str>,
) -> Vec<Display> {
    let mut agents: HashMap<&str, HashSet<String>> = HashMap::new();
    rows.iter().map(|row| {
        let state = row.reported_state();
        let inactive = Display { state, working_since: None };
        if !matches!(state, Some(AssignmentState::Registered | AssignmentState::Preparing | AssignmentState::Running)) {
            return inactive;
        }
        let launches = receipts.and_then(|r| r.by_obligation.get(&row.obligation_id));
        let turn_start = turn.filter(|turn| match &turn.cause {
            Cause::Handoff(id) => id == &row.id,
            Cause::WorkerResult(tasks) => launches.is_some_and(|launches| launches.tasks.iter()
                .any(|(session, id)| session == &turn.session && tasks.contains(id))),
        }).map(|turn| turn.started_at);
        let mut worker_running = || launches.is_some_and(|launches| launches.unresolved.iter()
            .any(|(session, agent)| {
                let Some(agent) = agent else { return false };
                agents.entry(session).or_insert_with(|| {
                    if !provider_running(scope, session, hosted) { return HashSet::new(); }
                    crate::app_workers::status(&scope.state_root, Some(session)).items.into_iter()
                        .filter(|item| item.state == "active")
                        .filter_map(|item| item.agent_id)
                        .collect()
                }).contains(agent)
            }));
        match turn_start.or_else(|| (command_jobs.contains(row.id.as_str()) || worker_running()).then_some(row.updated_at_ms)) {
            Some(start) => Display { state: Some(AssignmentState::Running), working_since: Some(start) },
            None if state == Some(AssignmentState::Registered) => inactive,
            None => Display { state: Some(AssignmentState::Unknown), working_since: None },
        }
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_published_turn_stops_counting_when_its_supervised_provider_exits() {
        let root = std::env::temp_dir().join(format!("assignment-activity-{}", uuid::Uuid::new_v4()));
        let scope = StatusToolScope {
            version: 1, state_root: root.clone(),
            entity_id: "depot".into(), thread_id: "thread-one".into(),
        };
        let owner = std::process::id();
        let ProcessStart::Running(owner_start) = process_start(owner) else {
            panic!("could not identify the test process");
        };
        let turn = Turn { session: "stu".into(), started_at: 42, cause: Cause::Handoff("job".into()) };
        crate::quota::atomic_write(&path(&scope).unwrap(), &PublishedTurn {
            owner, owner_start, supervised: true, turn: turn.clone(),
        }).unwrap();
        let lease = crate::lease_commands::state_path(&root, "stu");
        let record = |ended| serde_json::json!({"owner":owner,"provider":owner,"ended":ended});
        assert!(read_turn(&scope).is_none());
        crate::quota::atomic_write(&lease, &record(false)).unwrap();
        assert_eq!(read_turn(&scope), Some(turn));
        crate::quota::atomic_write(&lease, &record(true)).unwrap();
        assert!(read_turn(&scope).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
}

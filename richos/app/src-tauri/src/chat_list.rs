//! Local chat-list facts, scoped by the same thread binding as its conversation.
use crate::{AppState, NavigationTree, ThreadRow};
use richos_core::{
    assignment::{self, AssignmentState, NoticeKind},
    ledger::{AttentionTier, Source, TurnState},
    questions::{Question, State},
    quota::{self, holds::Hold},
    read_view::SpineView,
    util::now_millis,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::{AppHandle, Emitter, Listener, Manager};

#[derive(Clone, Default, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Preferences {
    pub seen_at: Option<u64>,
    pub unread: bool,
    pub later: Option<Later>,
    pub back_at: Option<u64>,
    pub archived_at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct Later {
    pub since: u64,
    pub until: u64,
}

#[derive(Clone, Default, Debug, Serialize)]
pub(crate) struct Facts {
    pub question_count: usize,
    pub waiting_since: Option<u64>,
    pub question_at: Option<u64>,
    pub stopped_at: Option<u64>,
    pub failure_at: Option<u64>,
    pub working_since: Option<u64>,
    pub paused: bool,
    pub team_working: bool,
    pub recovering: bool,
    #[serde(skip)]
    unconfirmed_assignments: bool,
    pub resume_at: Option<u64>,
    pub result_at: Option<u64>,
    pub long_finish_at: Option<u64>,
    pub input_at: Option<u64>,
    pub detail: Option<String>,
}

#[derive(Default, Serialize)]
pub(crate) struct Row {
    #[serde(flatten)]
    pub facts: Facts,
    pub seen_at: u64,
    pub unread: bool,
    pub later_until: Option<u64>,
    pub back_at: Option<u64>,
    pub technical: bool,
}

fn latest(slot: &mut Option<u64>, at: u64) {
    *slot = Some(slot.map_or(at, |before| before.max(at)));
}

fn earliest(slot: &mut Option<u64>, at: u64) {
    *slot = Some(slot.map_or(at, |before| before.min(at)));
}

fn facts(
    view: &dyn SpineView,
    state_root: &std::path::Path,
    work: &richos_core::work_host::WorkHost,
    row: &ThreadRow,
    questions: &[Question],
    holds: &[Hold],
    reset: Option<(u64, &'static str)>,
) -> Facts {
    let mut result = Facts::default();
    let Some(entity) = &row.entity_id else {
        return result;
    };
    let mut newest_progress = 0;
    let mut stop_detail = None;
    let mut stopped_independently = false;
    let mut work_detail = None;
    if let Ok(turns) = view.ledger().thread_turns(&row.id) {
        let restart_successors: std::collections::HashSet<_> = turns
            .iter()
            .filter(|turn| turn.ended_at.is_none())
            .filter_map(|turn| turn.superseded_by.as_deref())
            .collect();
        for turn in turns
            .iter()
            .copied()
            .filter(|turn| turn.source != Source::Internal && turn.superseded_by.is_none())
        {
            if matches!(turn.source, Source::Text | Source::Jam) {
                latest(&mut result.input_at, turn.created_at);
            }
            if turn.source == Source::Proactive {
                if turn.tier != Some(AttentionTier::Silent) {
                    latest(&mut result.result_at, turn.created_at);
                }
                continue;
            }
            newest_progress = newest_progress.max(turn.created_at);
            match turn.state {
                TurnState::Received | TurnState::InFlight => {
                    earliest(
                        &mut result.working_since,
                        turn.started_at.unwrap_or(turn.created_at),
                    );
                    result.recovering |= restart_successors.contains(turn.id.as_str());
                    work_detail = Some("Rich".to_owned());
                }
                TurnState::Completed => {
                    if let Some(at) = turn.ended_at {
                        latest(&mut result.result_at, at);
                        if is_long(turn.started_at.unwrap_or(turn.created_at), at) {
                            latest(&mut result.long_finish_at, at);
                        }
                    }
                }
                TurnState::Interrupted | TurnState::Stopped => {
                    let at = turn
                        .ended_at
                        .or(turn.stop_requested_at)
                        .unwrap_or(turn.created_at);
                    if result.stopped_at.is_none_or(|old| at >= old) {
                        result.stopped_at = Some(at);
                        stopped_independently = turn.state == TurnState::Interrupted;
                        stop_detail = turn.stop_reason.clone();
                    }
                }
            }
        }
    }
    for question in questions.iter().filter(|q| &q.entity_id == entity) {
        if question.state == State::Open {
            result.question_count += 1;
            earliest(&mut result.waiting_since, question.created_at);
            latest(&mut result.question_at, question.created_at);
        }
        if let Some(answer) = &question.answer {
            latest(&mut result.input_at, answer.at);
        }
    }
    if let Ok(scan) = assignment::scan_all(state_root, entity, &row.id) {
        let activities = work.assignment_activity(&scan.rows);
        for (job, activity) in scan.rows.into_iter().zip(activities) {
            let Some(status) = activity.state else { continue };
            result.unconfirmed_assignments |= status == AssignmentState::Unknown;
            newest_progress = newest_progress.max(job.registered_at_ms);
            match status {
                AssignmentState::Blocked => {
                    earliest(&mut result.waiting_since, job.updated_at_ms);
                    latest(&mut result.question_at, job.updated_at_ms);
                }
                AssignmentState::WaitingForQuota => result.paused = true,
                AssignmentState::Running => {
                    if let Some(started) = activity.working_since {
                        earliest(&mut result.working_since, started);
                        result.team_working = true;
                        work_detail = Some("Team · running".into());
                    }
                }
                AssignmentState::Failed | AssignmentState::Interrupted
                    if result.stopped_at.is_none_or(|old| job.updated_at_ms >= old) =>
                {
                    result.stopped_at = Some(job.updated_at_ms);
                    stopped_independently = status == AssignmentState::Failed;
                    stop_detail = Some(job.detail);
                }
                AssignmentState::Settled => {
                    latest(&mut result.result_at, job.updated_at_ms);
                    if is_long(job.registered_at_ms, job.updated_at_ms) {
                        latest(&mut result.long_finish_at, job.updated_at_ms);
                    }
                }
                _ => {}
            }
            for notice in job
                .notices
                .iter()
                .filter(|n| matches!(n.kind, NoticeKind::Answer | NoticeKind::Settled))
            {
                latest(&mut result.result_at, notice.raised_at_ms);
                if is_long(job.registered_at_ms, notice.raised_at_ms) {
                    latest(&mut result.long_finish_at, notice.raised_at_ms);
                }
            }
        }
    }
    // The answer to a failed piece of work is a newer attempt in this chat.
    if result.stopped_at.is_some_and(|at| at < newest_progress) {
        result.stopped_at = None;
        stop_detail = None;
    }
    // User Stop is visible on the row. Only an actual failure warrants a background alert.
    result.failure_at = result.stopped_at.filter(|_| stopped_independently);
    let hold = holds.iter().find(|hold| &hold.entity_id == entity);
    result.paused |= hold.is_some();
    result.resume_at = reset.filter(|_| result.paused).map(|(at, _)| at);
    result.detail = if result.waiting_since.is_some() {
        questions
            .iter()
            .find(|q| q.state == State::Open && &q.entity_id == entity)
            .map(|q| {
                if q.asker == "front_desk" {
                    "Asked by Rich"
                } else {
                    "Asked by the team"
                }
                .to_owned()
            })
            .or_else(|| Some("Assignment blocked".into()))
    } else if result.stopped_at.is_some() {
        stop_detail.filter(|s| !s.is_empty())
    } else if result.paused {
        let limit = reset.map_or("Usage limit", |(_, limit)| limit);
        Some(match hold.and_then(|h| h.account.as_deref()) {
            Some(account) => format!("{limit} · {account}"),
            None => limit.to_owned(),
        })
    } else {
        work_detail
    };
    result
}

const LONG_JOB_MS: u64 = 5 * 60_000;

fn is_long(start: u64, end: u64) -> bool {
    end.saturating_sub(start) >= LONG_JOB_MS
}

fn project(prefs: &Preferences, facts: &Facts, now: u64) -> Preferences {
    let mut next = prefs.clone();
    if let Some(later) = &prefs.later {
        let urgent = [facts.question_at, facts.failure_at]
            .into_iter()
            .flatten()
            .filter(|at| *at > later.since)
            .min();
        if facts
            .input_at
            .is_some_and(|at| at > later.since && at < later.until)
        {
            next.later = None;
        } else if urgent.is_some_and(|at| at < later.until) {
            next.later = None;
            // The more specific question or failure is already in Needs you.
            next.back_at = None;
        } else if now >= later.until {
            next.later = None;
            next.back_at = Some(later.until);
        }
    }
    next
}

pub(crate) fn snapshot(state: &AppState, view: &dyn SpineView, tree: &mut NavigationTree) {
    let started = std::time::Instant::now();
    let now = now_millis();
    let state_root = state.data_dir.join("engine-state");
    let mut questions: HashMap<String, Vec<Question>> = HashMap::new();
    let question_records = crate::question_host::store(state).all();
    let question_unavailable = question_records.is_err();
    if let Ok(all) = question_records {
        for question in all {
            questions
                .entry(question.thread_id.clone())
                .or_default()
                .push(question);
        }
    }
    let mut holds: HashMap<String, Vec<Hold>> = HashMap::new();
    let quota_records = quota::holds::read(&state_root, None);
    let quota_unavailable = quota_records.is_err();
    if let Ok(activity) = quota_records {
        for hold in activity.held {
            holds.entry(hold.thread_id.clone()).or_default().push(hold);
        }
    }
    let reset = quota_reset(&state.quota.view());
    let facts: Vec<_> = rows(tree)
        .map(|row| {
            facts(
                view,
                &state_root,
                &state.work,
                row,
                questions.get(&row.id).map_or(&[], Vec::as_slice),
                holds.get(&row.id).map_or(&[], Vec::as_slice),
                reset,
            )
        })
        .collect();
    let unconfirmed_assignments = facts.iter().any(|f| f.unconfirmed_assignments);
    let config = state.config.lock().unwrap();
    let mut nav = state.nav.lock().unwrap();
    let since = nav.state().chat_list_since.unwrap_or(now);
    let mut changes = Vec::new();
    for (row, mut facts) in tree
        .groups
        .iter_mut()
        .flat_map(|g| &mut g.threads)
        .chain(&mut tree.unbound)
        .zip(facts)
    {
        row.archived = nav.state().archived_threads.contains(&row.id);
        row.pinned = nav.state().pinned_threads.contains(&row.id);
        if facts.detail.as_deref() == Some("Rich") {
            facts.detail = Some(format!("Rich · {}", model_name(config.rich_model())));
        } else if facts.team_working
            && !facts.paused
            && facts.waiting_since.is_none()
            && facts.stopped_at.is_none()
        {
            facts.detail = facts
                .detail
                .map(|detail| format!("{detail} · {}", model_name(config.stu_model())));
        }
        let before = nav
            .state()
            .chat_list
            .get(&row.id)
            .cloned()
            .unwrap_or_default();
        let mut after = project(&before, &facts, now);
        let unarchive = row.archived
            && [
                facts.question_at,
                facts.failure_at,
                facts.result_at,
                facts.input_at,
            ]
            .into_iter()
            .flatten()
            .any(|at| at > before.archived_at.unwrap_or(since));
        if unarchive {
            row.archived = false;
            after.archived_at = None;
        }
        if before != after || unarchive {
            changes.push((row.id.clone(), after.clone(), unarchive));
        }
        let technical = config.techy_mode(&row.id, row.entity_id.as_deref()).enabled;
        if !technical {
            facts.detail = None;
        }
        row.chat = Row {
            technical,
            seen_at: after.seen_at.unwrap_or(since),
            unread: after.unread
                || facts
                    .result_at
                    .is_some_and(|at| at > after.seen_at.unwrap_or(since)),
            later_until: after.later.map(|later| later.until),
            back_at: after.back_at,
            facts,
        };
    }
    drop(config);
    if (nav.state().chat_list_since.is_none() || !changes.is_empty())
        && nav
            .update_chat_list(|prefs| {
                prefs.chat_list_since = Some(since);
                for (id, after, unarchive) in changes {
                    if unarchive {
                        prefs.archived_threads.retain(|thread| thread != &id);
                    }
                    prefs.chat_list.insert(id, after);
                }
            })
            .is_err()
    {
        record("preferences", "write-failed", None);
    }
    record(
        "local snapshot",
        if question_unavailable || quota_unavailable {
            "partial"
        } else if unconfirmed_assignments {
            "assignment-activity-unconfirmed"
        } else {
            "read"
        },
        Some(started.elapsed().as_millis()),
    );
}

fn model_name(model: richos_core::config::ModelChoice) -> &'static str {
    use richos_core::config::ModelChoice;
    match model {
        ModelChoice::Opus => "Opus",
        _ => "Fable",
    }
}

pub(crate) fn reply_seen_on_phone(app: &AppHandle, state: &AppState, thread: &str, at: u64) {
    let mut nav = state.nav.lock().unwrap();
    let since = nav.state().chat_list_since.unwrap_or(0);
    let seen = nav
        .state()
        .chat_list
        .get(thread)
        .and_then(|chat| chat.seen_at)
        .unwrap_or(since);
    if at <= seen {
        return;
    }
    if nav
        .update_chat_list(|prefs| {
            let chat = prefs.chat_list.entry(thread.to_owned()).or_default();
            chat.seen_at = Some(at);
            chat.unread = false;
            if chat.back_at.is_some_and(|back| back <= at) {
                chat.back_at = None;
            }
        })
        .is_ok()
    {
        drop(nav);
        drop(app.emit("rich://chat-list-preferences", ()));
    }
}

pub(crate) fn quota_reset(quota: &quota::View) -> Option<(u64, &'static str)> {
    let quota::Admission::Held { resets_at } = quota.admission else {
        return None;
    };
    let weekly = quota.windows.iter().any(|w| {
        w.id == "seven_day"
            && w.used_percent >= quota::resets::WEEKLY_THRESHOLD
            && w.resets_at == Some(resets_at)
    });
    Some(if weekly {
        (resets_at, "Weekly usage limit")
    } else {
        (
            resets_at.saturating_sub(20 * 60_000).saturating_add(1),
            "5-hour limit",
        )
    })
}

#[richos_command::command]
pub(crate) fn set_chat_list_state(
    app: AppHandle,
    state: tauri::State<AppState>,
    thread_id: String,
    action: String,
    until: Option<u64>,
    seen_at: Option<u64>,
) -> Result<(), String> {
    let view = state.reader.snapshot();
    if !view.threads().iter().any(|thread| thread.id == thread_id) {
        return Err("This chat could not be found".into());
    }
    if !matches!(
        action.as_str(),
        "seen" | "read" | "unread" | "later" | "now"
    ) {
        return Err("Unknown chat-list action".into());
    }
    let now = now_millis();
    if action == "later" && until.is_none_or(|at| at <= now || at > now + 8 * 86_400_000) {
        return Err("Choose a future time in the next seven days".into());
    }
    let mut nav = state.nav.lock().unwrap();
    nav.update_chat_list(|prefs| {
        let chat = prefs.chat_list.entry(thread_id).or_default();
        match action.as_str() {
            "seen" | "read" => {
                let at = seen_at.unwrap_or(now).min(now);
                chat.seen_at = Some(chat.seen_at.unwrap_or(0).max(at));
                chat.unread = false;
                if action == "seen" && chat.back_at.is_some_and(|back| back <= at) {
                    chat.back_at = None;
                }
            }
            "unread" => chat.unread = true,
            "later" => {
                chat.later = until.map(|until| Later { since: now, until });
                chat.back_at = None;
            }
            "now" => chat.later = None,
            _ => {}
        }
    })
    .map_err(|e| e.to_string())?;
    drop(nav);
    drop(app.emit("rich://chat-list-preferences", ()));
    Ok(())
}

fn rows(tree: &NavigationTree) -> impl Iterator<Item = &ThreadRow> {
    tree.groups
        .iter()
        .flat_map(|g| &g.threads)
        .chain(&tree.unbound)
}

/// A killed provider need not leave a final callback or receipt change. Recheck only
/// while the sidebar has current team activity to withdraw.
fn activity_recheck<'a>(mut facts: impl Iterator<Item = &'a Facts>, now: u64) -> Option<u64> {
    facts.any(|fact| fact.team_working).then_some(now.saturating_add(3_000))
}

pub(crate) fn watch(app: AppHandle) {
    use crate::chat_notifications::{self, Kind};
    use notify::Watcher;
    use std::{sync::mpsc, time::Duration};

    let started = now_millis();
    let (wake, events) = mpsc::sync_channel(1);
    for event in [
        "rich://chat-list-preferences",
        "rich://turn-status",
        "rich://ceo-message",
        "rich://turn-error",
        "rich://turn-completed",
        "rich://work-notice",
        "rich://assignment-activity-changed",
        "rich://question-upserted",
        "rich://chat-list-clock-changed",
    ] {
        let wake = wake.clone();
        app.listen(event, move |_| {
            let _sent = wake.try_send(());
        });
    }
    std::thread::Builder::new()
        .name("richos-chat-list".into())
        .spawn(move || {
            let state = app.state::<AppState>();
            let root = state.data_dir.join("engine-state");
            let mut watcher = notify::recommended_watcher({
                let wake = wake.clone();
                move |event: notify::Result<notify::Event>| {
                    if event.is_ok_and(|event| {
                        !event.kind.is_access()
                            && event.paths.iter().any(|p| {
                                p.components().any(|part| {
                                    matches!(
                                        part.as_os_str().to_str(),
                                        Some("assignments" | "work-receipts" | "sessions" | "quota-waits" | "questions")
                                    )
                                })
                            })
                    }) {
                        let _sent = wake.try_send(());
                    }
                }
            })
            .ok();
            if watcher
                .as_mut()
                .is_none_or(|w| w.watch(&root, notify::RecursiveMode::Recursive).is_err())
            {
                record("observer", "file-watch-unavailable", None);
            }
            let mut reported = HashMap::<String, [u64; 4]>::new();
            loop {
                let view = state.reader.snapshot();
                let tree = crate::navigation_snapshot(&state, &*view);
                let mut next_due = activity_recheck(rows(&tree).map(|row| &row.chat.facts), now_millis());
                for row in rows(&tree) {
                    let chat = &row.chat;
                    let f = &chat.facts;
                    let found = [f.question_at, f.failure_at, f.long_finish_at, chat.back_at];
                    let previous = reported.entry(row.id.clone()).or_insert([started; 4]);
                    let kinds = [Kind::Question, Kind::Stopped, Kind::Finished, Kind::Back];
                    for (index, (at, kind)) in found.into_iter().zip(kinds).enumerate() {
                        if let Some(at) = at.filter(|at| *at > previous[index]) {
                            previous[index] = at;
                            if !row.archived
                                && chat.later_until.is_none()
                                && (matches!(kind, Kind::Question | Kind::Stopped | Kind::Back)
                                    || at > chat.seen_at)
                            {
                                chat_notifications::post(&app, row, at, kind);
                            }
                        }
                    }
                    if let Some(at) = chat.later_until {
                        earliest(&mut next_due, at);
                    }
                }
                drop(app.emit("rich://chat-list-changed", ()));
                // Questions, turns and receipt changes also wake the observer. Re-evaluate the
                // next reminder and the current activity after the Mac sleeps.
                let pause = next_due
                    .map(|at| Duration::from_millis(at.saturating_sub(now_millis()).max(1)));
                match pause {
                    Some(pause) => {
                        let _event = events.recv_timeout(pause);
                    }
                    None => {
                        if events.recv().is_err() {
                            break;
                        }
                    }
                }
                // Coalesce atomic renames, ledger and structured events from one transition.
                std::thread::sleep(Duration::from_millis(100));
                while events.try_recv().is_ok() {}
            }
        })
        .expect("start chat list observer");
}

static DIAGNOSTICS: std::sync::Mutex<std::collections::VecDeque<String>> =
    std::sync::Mutex::new(std::collections::VecDeque::new());

pub(crate) fn record(stage: &'static str, outcome: &'static str, duration_ms: Option<u128>) {
    if let Ok(mut recent) = DIAGNOSTICS.lock() {
        if recent.len() >= 24 {
            recent.pop_front();
        }
        let build = option_env!("RICHOS_SOURCE_SHA").unwrap_or(env!("CARGO_PKG_VERSION"));
        let duration = duration_ms.map_or_else(String::new, |ms| format!(" · {ms} ms"));
        recent.push_back(format!("{build} · {stage} · {outcome}{duration}"));
    }
}

pub(crate) fn recent_diagnostics() -> Option<String> {
    let recent = DIAGNOSTICS.lock().ok()?;
    (!recent.is_empty()).then(|| recent.iter().cloned().collect::<Vec<_>>().join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_returns_once_at_its_time_and_background_completion_does_not_wake_it() {
        let prefs = Preferences {
            later: Some(Later {
                since: 100,
                until: 300,
            }),
            ..Preferences::default()
        };
        let completed = Facts {
            result_at: Some(175),
            long_finish_at: Some(175),
            ..Facts::default()
        };
        assert_eq!(project(&prefs, &completed, 299), prefs);
        let due = project(&prefs, &completed, 300);
        assert_eq!(due.later, None);
        assert_eq!(due.back_at, Some(300));
        assert_eq!(project(&due, &completed, 900), due);
    }

    #[test]
    fn only_new_questions_failures_or_user_input_can_wake_a_chat_early() {
        let prefs = Preferences {
            later: Some(Later {
                since: 100,
                until: 300,
            }),
            ..Preferences::default()
        };
        let old_question = Facts {
            question_count: 1,
            question_at: Some(90),
            ..Facts::default()
        };
        assert_eq!(project(&prefs, &old_question, 120), prefs);
        for signal in [
            Facts {
                question_at: Some(111),
                ..Facts::default()
            },
            Facts {
                failure_at: Some(111),
                ..Facts::default()
            },
            Facts {
                input_at: Some(111),
                ..Facts::default()
            },
        ] {
            let next = project(&prefs, &signal, 120);
            assert!(next.later.is_none());
            assert!(next.back_at.is_none());
        }
    }

    #[test]
    fn current_team_activity_keeps_the_sidebar_refreshing_until_it_clears() {
        let working = Facts { team_working: true, ..Facts::default() };
        let quiet = Facts::default();
        assert_eq!(activity_recheck([&quiet, &working].into_iter(), 10_000), Some(13_000));
        assert_eq!(activity_recheck([&quiet].into_iter(), 13_000), None);
    }

    #[test]
    fn bug_report_excerpt_keeps_only_bounded_outcomes() {
        for n in 0..30 {
            record("synthetic snapshot", "read", Some(n));
        }
        let excerpt = recent_diagnostics().unwrap();
        assert_eq!(excerpt.lines().count(), 24);
        assert!(excerpt.lines().last().unwrap().ends_with("29 ms"));
        assert!(include_str!("bug_report.rs").contains("chat_list::recent_diagnostics()"));
    }

    #[test]
    fn unread_snooze_and_notification_choices_round_trip_through_navigation_store() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("navigation.json");
        std::fs::write(&path, r#"{"pinned_threads":["example"]}"#).unwrap();
        let mut store = crate::nav::NavStore::open(&path);
        assert!(store.state().mac_chat_notifications && store.state().chat_notification_sounds);
        store
            .update_chat_list(|state| {
                state.chat_list.insert(
                    "example".into(),
                    Preferences {
                        seen_at: Some(120),
                        unread: true,
                        later: Some(Later {
                            since: 121,
                            until: 555,
                        }),
                        ..Preferences::default()
                    },
                );
                state.chat_notification_sounds = false;
            })
            .unwrap();
        let saved = crate::nav::NavStore::open(path);
        let chat = &saved.state().chat_list["example"];
        assert_eq!(chat.seen_at, Some(120));
        assert!(chat.unread);
        assert_eq!(chat.later.as_ref().unwrap().until, 555);
        assert!(!saved.state().chat_notification_sounds);
        assert!(saved.state().mac_chat_notifications);
        assert_eq!(saved.state().pinned_threads, ["example"]);
    }
}

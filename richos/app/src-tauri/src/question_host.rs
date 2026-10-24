//! Question operations use the published view and the question store, never the turn mutex.
use crate::AppState;
use richos_core::{
    questions::{AnswerRequest, AnswerResult, State, Store},
    read_view::SpineView,
};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

pub fn store(state: &AppState) -> Store {
    Store::new(state.data_dir.join("engine-state"))
}
pub fn payload(state: &AppState, thread: &str) -> Result<Value, String> {
    let view = state.reader.snapshot();
    payload_in(&*view, &store(state), &crate::rich_files_host::root(state), thread)
}
/// [`payload`] over a published view, the question store and the engine state the file tool
/// writes: the calm view's snapshot, as `get_timeline` and the phone read it.
pub fn payload_in(view: &dyn SpineView, store: &Store, files_root: &std::path::Path, thread: &str) -> Result<Value, String> {
    let binding = view
        .ledger()
        .thread_binding(thread)
        .map_err(|e| e.to_string())?;
    let mut payload = crate::timeline_view::timeline_payload(view, thread)?;
    if let Some(items) = payload["items"].as_array_mut() {
        items.retain(|i| {
            !(i["kind"] == "user_message"
                && i["turnId"]
                    .as_str()
                    .is_some_and(|s| s.starts_with("question-answer:")))
        });
        let revision = view
            .active_binding()
            .filter(|b| b.thread_id() == thread)
            .map_or(binding.binding_revision(), |b| b.binding_revision());
        for q in store.list(binding.entity_id().as_str(), thread)? {
            items.push(q.item(revision));
        }
        // The files Rich and Stu put in front of the user (`rich_files_host.rs`), the same way:
        // one item each, in the turn that attached it. The phone reads this same payload, and
        // Techy Mode's snapshot adds the same rows (`rich_files_host::machinery_with_files`).
        items.extend(crate::rich_files_host::items(files_root, binding.entity_id().as_str(), thread));
        items.sort_by_key(|i| i["createdAt"].as_u64().unwrap_or(0));
    }
    Ok(payload)
}
pub fn answer(
    state: &AppState,
    thread: &str,
    request: AnswerRequest,
    method: &str,
    surface: &str,
) -> Result<AnswerResult, String> {
    let view = state.reader.snapshot();
    let store = store(state);
    let binding = match view.ledger().thread_binding(thread) {
        Ok(binding) => binding,
        Err(error) => {
            // An offline sender can outlive the destination. Use only the existing
            // question's attribution to close it; never create a conversation here.
            if let Some(q) = store
                .all()?
                .into_iter()
                .find(|q| q.id == request.question_id && q.thread_id == thread)
            {
                store.close(
                    &q.entity_id,
                    thread,
                    None,
                    "This conversation was deleted",
                    true,
                )?;
                return store.answer(&q.entity_id, thread, request, method, surface);
            }
            return Err(error.to_string());
        }
    };
    // His click takes its place in the conversation's input order now, not when the
    // delivery worker later reaches the spine, so a Stop he presses after it covers it.
    let mut result = store.answer_in_order(
        binding.entity_id().as_str(),
        thread,
        request,
        method,
        surface,
        state.control.take_input_order(),
    )?;
    if let Some(q) = &result.question {
        result.question = store.waiting_for_turn(
            binding.entity_id().as_str(),
            thread,
            &q.id,
            state
                .control
                .for_thread(thread)
                .active_turn()
                .is_some(),
        )?;
    }
    Ok(result)
}
pub fn shown(state: &AppState, thread: &str, question: &str, surface: &str) -> Result<(), String> {
    let view = state.reader.snapshot();
    let binding = view
        .ledger()
        .thread_binding(thread)
        .map_err(|e| e.to_string())?;
    store(state).acknowledge(binding.entity_id().as_str(), thread, question, surface)
}
/// Desktop-only observer. The phone receives changes through its existing foreground
/// stream. File metadata is checked outside all turn locks; idle cards cause no UI work.
pub fn start(app: AppHandle) {
    use notify::Watcher;
    let directory = app
        .state::<AppState>()
        .data_dir
        .join("engine-state/questions");
    std::fs::create_dir_all(&directory).expect("create question history directory");
    let (changed, events) = std::sync::mpsc::sync_channel::<()>(1);
    let (deliver, deliveries) = std::sync::mpsc::sync_channel::<()>(1);
    let (work_ready, work_events) = std::sync::mpsc::sync_channel::<()>(1);
    let (notify_ready, notify_events) = std::sync::mpsc::sync_channel::<()>(1);
    let (witness_ready, witness_events) = std::sync::mpsc::sync_channel::<()>(1);
    let receiver = app.clone();
    std::thread::Builder::new()
        .name("richos-question-witness".into())
        .spawn(move || {
            for () in witness_events {
                if let Err(error) = store(&receiver.state::<AppState>()).flush_witnesses() {
                    eprintln!("[richos] question witness remains pending: {error}");
                }
            }
        })
        .expect("start question witness");
    let receiver = app.clone();
    std::thread::Builder::new()
        .name("richos-question-notifications".into())
        .spawn(move || {
            for () in notify_events {
                let state = receiver.state::<AppState>();
                let phone = receiver.state::<std::sync::Arc<crate::phone::PhoneRuntime>>();
                if let Err(error) = store(&state).notify_sets(|q| phone.enqueue_question(q)) {
                    eprintln!("[richos] question notification remains pending: {error}");
                }
                phone.send_question_notifications();
            }
        })
        .expect("start question notifications");
    let receiver = app.clone();
    std::thread::Builder::new()
        .name("richos-question-work".into())
        .spawn(move || {
            for () in work_events {
                let state = receiver.state::<AppState>();
                let view = state.reader.snapshot();
                // The pass itself is richos-core's (`WorkHost::deliver_team_answers`, the
                // work-path design D8), so the tests and the VM crash matrix run it as it ships.
                let binding_for = |thread: &str| view.ledger().thread_binding(thread).ok();
                state.work.deliver_team_answers(&binding_for);
            }
        })
        .expect("start question work delivery");
    let receiver = app.clone();
    std::thread::Builder::new()
        .name("richos-question-delivery".into())
        .spawn(move || {
            for () in deliveries {
                let state = receiver.state::<AppState>();
                let store = store(&state);
                if !store
                    .pending_threads()
                    .is_ok_and(|p| p.iter().any(|(_, _, asker)| asker == "front_desk"))
                {
                    continue;
                }
                // Only this worker waits on a front-desk turn. Team answers, display
                // updates and phone awareness continue on their own event channels.
                let Ok(mut spine) = state.spine.lock() else {
                    continue;
                };
                for (entity, thread, asker) in store.pending_threads().unwrap_or_default() {
                    if asker != "front_desk" {
                        continue;
                    }
                    if let Err(error) = store.deliver(&entity, &thread, &asker, |d| {
                        spine.queue_question_answer(d).map_err(|e| e.to_string())
                    }) {
                        eprintln!("[richos] question answer remains saved: {error}");
                    }
                }
                drop(spine);
                if let Err(error) = crate::drain_incoming_messages(&state.spine, &state.work) {
                    eprintln!("[richos] question continuation failed: {error}");
                }
            }
        })
        .expect("start question delivery");
    // Wakeups coalesce when the slot is full; a disconnected worker accepts no further work.
    changed.try_send(()).unwrap_or(());
    std::thread::Builder::new()
        .name("richos-questions".into())
        .spawn(move || {
            let mut watcher =
                notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
                    if event.is_ok_and(|e| {
                        !e.kind.is_access()
                            && e.paths
                                .iter()
                                .any(|p| p.file_name().is_some_and(|n| n == "store.json"))
                    }) {
                        changed.try_send(()).unwrap_or(());
                    }
                })
                .expect("watch question history");
            watcher
                .watch(&directory, notify::RecursiveMode::NonRecursive)
                .expect("watch question directory");
            let mut seen = std::collections::HashMap::<String, String>::new();
            for () in events {
                let state = app.state::<AppState>();
                let store = store(&state);
                if let Ok(questions) = store.all() {
                    let view = state.reader.snapshot();
                    let mut witness_pending = false;
                    for q in questions {
                        witness_pending |= q.shown.as_ref().is_some_and(|s| !s.witness_written);
                        let Ok(binding) = view.ledger().thread_binding(&q.thread_id) else {
                            continue;
                        };
                        if binding.entity_id().as_str() != q.entity_id {
                            continue;
                        }
                        let revision = view
                            .active_binding()
                            .filter(|b| b.thread_id() == q.thread_id)
                            .map_or(binding.binding_revision(), |b| b.binding_revision());
                        let signature = json!([
                            q.state,
                            q.revision,
                            q.delivered,
                            q.awaiting_taker,
                            q.ended_before_taken,
                            q.handoff_started,
                            q.waiting_for_turn,
                            q.remaining,
                            q.withdrawal_reason
                        ])
                        .to_string();
                        if seen.get(&q.id) != Some(&signature) {
                            seen.insert(q.id.clone(), signature);
                            let item = q.item(revision);
                            drop(app.emit("rich://question-upserted", &item));
                            app.state::<std::sync::Arc<crate::phone::PhoneRuntime>>()
                                .question_changed(&item);
                        }
                    }
                    if witness_pending {
                        witness_ready.try_send(()).unwrap_or(());
                    }
                }
                deliver.try_send(()).unwrap_or(());
                work_ready.try_send(()).unwrap_or(());
                notify_ready.try_send(()).unwrap_or(());
            }
        })
        .expect("start question observer");
}
#[richos_command::command]
pub fn answer_question(
    state: tauri::State<AppState>,
    thread_id: String,
    answer: AnswerRequest,
    method: String,
) -> Result<Value, String> {
    if !["click", "keyboard", "typed"].contains(&method.as_str()) {
        return Err("Unknown desktop answer method.".into());
    }
    self::answer(&state, &thread_id, answer, &method, "mac").map(|v| v.public_value())
}
#[richos_command::command]
pub fn question_shown(
    state: tauri::State<AppState>,
    thread_id: String,
    question_id: String,
) -> Result<(), String> {
    shown(&state, &thread_id, &question_id, "mac")
}
#[richos_command::command]
pub fn open_question_counts(state: tauri::State<AppState>) -> Result<Value, String> {
    let view = state.reader.snapshot();
    let mut counts = serde_json::Map::new();
    for thread in view.threads() {
        let Some(entity) = thread.entity_id else {
            continue;
        };
        counts.insert(
            thread.id.clone(),
            json!(store(&state)
                .list(&entity, &thread.id)?
                .iter()
                .filter(|q| q.state == State::Open)
                .count()),
        );
    }
    Ok(Value::Object(counts))
}

#[cfg(test)]
mod tests {
    /// This file's three commands answer off Tauri's async workers: each carries
    /// `#[richos_command::command]` and none a bare Tauri attribute. The dispatch proof is
    /// `crate::command_rule_tests`; this is the same rule held for this file by this file's
    /// own test, so a change here selects a test that reads it.
    #[test]
    fn its_three_commands_run_off_the_async_workers() {
        let source = include_str!("question_host.rs");
        assert_eq!(crate::command_rule_tests::bare_tauri_attributes(source), Vec::<String>::new());
        let ours = source.lines().filter(|line| line.trim() == "#[richos_command::command]").count();
        assert_eq!(ours, 3, "answer_question, question_shown, open_question_counts");
    }
}

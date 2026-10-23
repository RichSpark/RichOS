//! Process lifecycle cannot overwrite Stu's existing work reports or input receipts.
use richos_core::{
    assignment::{self, AssignmentState, Registration},
    cognition::{Cognition, CognitionError, LeaseFactory},
    session_tools::{self, Message, WorkReference},
    status_tools::{self, StatusToolScope},
    work_host::{WorkHost, WorkNotifier},
};
use std::sync::Arc;

struct NoLease;
impl LeaseFactory for NoLease {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        panic!("reading or quitting must not open a provider")
    }
}
struct Quiet;
impl WorkNotifier for Quiet {
    fn raised(&self, _: &str, _: &assignment::PendingNotice) {
        panic!("process lifecycle must not create a job report")
    }
}

#[test]
fn ordinary_quit_and_restart_preserve_stus_reports_and_input_receipts() {
    for state in [
        AssignmentState::Registered,
        AssignmentState::Running,
        AssignmentState::Blocked,
        AssignmentState::Settled,
        AssignmentState::Failed,
        AssignmentState::Interrupted,
        AssignmentState::Unknown,
    ] {
        let root = std::env::temp_dir().join(format!("richos-job-owner-{}", uuid::Uuid::new_v4()));
        let scope = StatusToolScope {
            version: 1,
            state_root: root.join("engine-state"),
            entity_id: "company".into(),
            thread_id: "thread".into(),
        };
        let receipt = assignment::register(
            &scope.state_root,
            &Registration {
                entity_id: scope.entity_id.clone(),
                thread_id: scope.thread_id.clone(),
                obligation_id: "job-obligation".into(),
                instruction_ledger_ref: "ledger:thread:request".into(),
                instruction_sha256: "a".repeat(64),
                title: "Finish the requested work".into(),
                repositories: vec![],
                needs_screen: false,
            },
        )
        .unwrap();
        // Seed a saved Stu report, not an app inference. The engine's real report and
        // complete tools write this schema; this fixture needs no provider or Python.
        use sha2::Digest;
        let partition = format!("{:x}", sha2::Sha256::digest(b"[\"company\",\"thread\"]"));
        let path = scope
            .state_root
            .join("assignments")
            .join(partition)
            .join(format!("{}.json", receipt.id));
        let mut report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        report["state"] = serde_json::json!(state);
        report["detail"] = serde_json::json!("Stu's saved report");
        report["work_session"] = serde_json::json!("original-stu-session");
        std::fs::write(&path, serde_json::to_vec(&report).unwrap()).unwrap();
        let before = std::fs::read(&path).unwrap();
        let input = session_tools::inbox(&scope, "stu").unwrap();
        std::fs::create_dir_all(input.parent().unwrap()).unwrap();
        let message = Message {
            handoff: true,
            from: "rich".into(),
            text: "The original handoff".into(),
            instruction: None,
            work: Some(WorkReference {
                assignment_id: receipt.id.clone(),
                obligation_id: "job-obligation".into(),
            }),
        };
        let line = serde_json::to_string(&message).unwrap() + "\n";
        std::fs::write(&input, &line).unwrap();
        session_tools::begin_attempt(
            &scope,
            "stu",
            &format!("peer:{}", line.len()),
            "original-stu-session",
        )
        .unwrap();
        let cursor = std::fs::read(input.with_extension("cursor")).unwrap();
        let host = WorkHost::new(
            &scope.state_root,
            Box::new(NoLease),
            Arc::new(Quiet),

        );
        host.shutdown();
        richos_core::recovery::reconcile(&scope.state_root);
        let after = std::fs::read(&path).unwrap();
        let input_after = std::fs::read(&input).unwrap();
        let cursor_after = std::fs::read(input.with_extension("cursor")).unwrap();
        let status = root.join("status.json");
        status_tools::write_scope(&status, &scope).unwrap();
        let view = status_tools::call(
            &status,
            &richos_core::screen::UnknownScreen,
            status_tools::LOOK_TOOL_NAME,
            serde_json::json!({}),
        )
        .unwrap();
        let states: Vec<_> = [
            "starting",
            "running",
            "waiting_for_you",
            "finished",
            "unreported",
        ]
        .into_iter()
        .flat_map(|key| view[key].as_array().into_iter().flatten())
        .filter_map(|row| row["state"].as_str())
        .map(str::to_string)
        .collect();
        let unverified = matches!(
            state,
            AssignmentState::Interrupted | AssignmentState::Unknown
        );
        if unverified {
            assert_eq!(view["finished"].as_array().unwrap().len(),1,"old stopped records must remain finished history");
            assert_eq!(view["finished"][0]["previous_record"]["state"],state.as_str());
            assert_eq!(view["finished"][0]["state"],"interrupted");
            assert_eq!(states,vec!["interrupted"],"historical stopped work is not live work");
            let job =
                assignment::read(&scope.state_root, "company", "thread", &receipt.id).unwrap();
            assert!(job.reported_state().is_none());
            assert!(
                job.accepts_input(),
                "an old host label revoked user authority"
            );
        }
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(after, before, "Quit rewrote Stu's {state:?} report");
        assert_eq!(
            input_after,
            line.as_bytes(),
            "Quit changed pending execution input"
        );
        assert_eq!(
            cursor_after, cursor,
            "Quit changed the previous execution attempt"
        );
        if !unverified {assert_eq!(states,vec![state.as_str()],"Rich's status tool disagrees with Stu's record");}
    }
}

/// **"Quit and stop the work" stops the work** (§7.4a, esc-20261022T221047Z-e01c005a): every
/// open job records his Stop as the Stop button does, so the next launch hands Stu nothing to
/// resume. A finished job is not rewritten. The ordinary quit above records nothing.
#[test]
fn quit_and_stop_the_work_records_his_stop_on_every_open_job() {
    let root = std::env::temp_dir().join(format!("richos-job-owner-{}", uuid::Uuid::new_v4()));
    let state_root = root.join("engine-state");
    let scope = |thread: &str| StatusToolScope {
        version: 1,
        state_root: state_root.clone(),
        entity_id: "company".into(),
        thread_id: thread.into(),
    };
    let register = |thread: &str, title: &str| {
        assignment::register(
            &state_root,
            &Registration {
                entity_id: "company".into(),
                thread_id: thread.into(),
                obligation_id: format!("{thread}-obligation"),
                instruction_ledger_ref: format!("ledger:{thread}:request"),
                instruction_sha256: "a".repeat(64),
                title: title.into(),
                repositories: vec![],
                needs_screen: false,
            },
        )
        .unwrap()
    };
    let running = register("one", "Run the heartbeat");
    let other = register("two", "Run the other heartbeat");
    let finished = register("three", "Already finished");
    use sha2::Digest;
    let record_path = |thread: &str, id: &str| {
        let key = format!("[\"company\",\"{thread}\"]");
        let partition = format!("{:x}", sha2::Sha256::digest(key.as_bytes()));
        state_root.join("assignments").join(partition).join(format!("{id}.json"))
    };
    for (thread, id, state) in [
        ("one", &running.id, "running"),
        ("two", &other.id, "blocked"),
        ("three", &finished.id, "settled"),
    ] {
        let path = record_path(thread, id);
        let mut report: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        report["state"] = serde_json::json!(state);
        report["detail"] = serde_json::json!("Stu's saved report");
        std::fs::write(&path, serde_json::to_vec(&report).unwrap()).unwrap();
    }
    let finished_before = std::fs::read(record_path("three", &finished.id)).unwrap();
    // Stu holds the running job's handoff, as an ordinary quit would leave it to resume.
    let input = session_tools::inbox(&scope("one"), "stu").unwrap();
    std::fs::create_dir_all(input.parent().unwrap()).unwrap();
    let message = Message {
        handoff: true,
        from: "rich".into(),
        text: "The original handoff".into(),
        instruction: None,
        work: Some(WorkReference {
            assignment_id: running.id.clone(),
            obligation_id: "one-obligation".into(),
        }),
    };
    let line = serde_json::to_string(&message).unwrap() + "\n";
    std::fs::write(&input, &line).unwrap();
    session_tools::begin_attempt(&scope("one"), "stu", &format!("peer:{}", line.len()), "stu-session").unwrap();
    assert!(session_tools::pending(&scope("one"), "stu").unwrap().is_some_and(|(m, _)| m.handoff));

    let host = WorkHost::new(&state_root, Box::new(NoLease), Arc::new(Quiet));
    let (stopped, errors) = host.record_quit_and_stop();
    host.shutdown();
    richos_core::recovery::reconcile(&state_root);

    assert_eq!((stopped, errors), (2, Vec::<String>::new()));
    for (thread, id) in [("one", &running.id), ("two", &other.id)] {
        let job = assignment::read(&state_root, "company", thread, id).unwrap();
        assert_eq!(job.reported_state(), Some(AssignmentState::Interrupted), "{thread}: the quit left the job open");
        assert!(job.user_stop_requested_at_ms.is_some(), "{thread}: his stop is not on the record");
        assert!(!job.accepts_input(), "{thread}: a stopped job still takes input");
    }
    assert!(
        session_tools::pending(&scope("one"), "stu").unwrap().is_none_or(|(m, _)| !m.handoff),
        "the next launch would hand Stu the stopped job again"
    );
    assert_eq!(
        std::fs::read(record_path("three", &finished.id)).unwrap(),
        finished_before,
        "a finished job was rewritten"
    );
    assert!(assignment::open(&state_root).unwrap().is_empty());
    std::fs::remove_dir_all(&root).unwrap();
}

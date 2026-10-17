//! Process lifecycle cannot overwrite Stu's existing work reports or input receipts.
use richos_core::{
    assignment::{self, AssignmentState, Registration},
    cognition::{Cognition, CognitionError, LeaseFactory},
    permissions::PermissionDesk,
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
            Arc::new(PermissionDesk::default()),
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

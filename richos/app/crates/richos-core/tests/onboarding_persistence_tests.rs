//! Exercises the same bounded tool actions and file readers used by the desktop app.
//! All fixtures are isolated; no account, subprocess model or personal HOME is needed.
use richos_core::{
    cognition::MockCognition,
    company::{self, CompanyLayer, InterviewProgress, COMPANY_BUDGET_BYTES},
    entity::{Entity, EntityId, EntityRegistry},
    ledger::{Ledger, Source},
    onboarding::{self, OnboardingRecord, OnboardingState},
    onboarding_tools::{self, OnboardingToolScope, DECLINE_TOOL_NAME, SAVE_TOOL_NAME},
    spine::Spine,
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

struct Fixture {
    root: PathBuf,
    central: PathBuf,
    record: PathBuf,
    scope: PathBuf,
    a: EntityId,
    b: EntityId,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "richos-onboarding-persistence-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let f = Self {
            central: root.join("central"),
            record: root.join("config/onboarding.json"),
            scope: root.join("scope.json"),
            a: EntityId::parse("alpha").unwrap(),
            b: EntityId::parse("beta").unwrap(),
            root,
        };
        f.bind(&f.a);
        f
    }
    fn bind(&self, e: &EntityId) {
        onboarding_tools::write_scope(
            &self.scope,
            &OnboardingToolScope::new(e, &self.central, &self.record),
        )
        .unwrap();
        onboarding_tools::set_actions_allowed(&self.scope, true).unwrap();
    }
    fn state(&self, e: &EntityId) -> OnboardingState {
        onboarding::state(
            Some(&CompanyLayer::read(&self.central, e)),
            &OnboardingRecord::load(&self.record).for_entity(e),
        )
    }
    fn save(&self, notes: &str, progress: &str) -> Result<Value, String> {
        onboarding_tools::call(
            &self.scope,
            SAVE_TOOL_NAME,
            json!({"notes":notes,"progress":progress}),
        )
    }
    fn spine(&self) -> Spine {
        let mut s = Spine::new(
            Ledger::open(&self.root.join(format!("{}.jsonl", uuid::Uuid::new_v4()))).unwrap(),
        );
        s.set_entity_registry(
            EntityRegistry::new(vec![
                Entity::new("alpha", "Alpha", &["/fixture/alpha"]).unwrap(),
                Entity::new("beta", "Beta", &["/fixture/beta"]).unwrap(),
            ])
            .unwrap(),
        );
        s.set_central_root(self.central.clone());
        s.set_onboarding_record(self.record.clone());
        s.ensure_active_thread_in(&self.a).unwrap();
        s
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn successful_tool_save_is_verified_and_returns_after_a_new_spine_starts() {
    let f = Fixture::new();
    let result = f
        .save(
            "# Alpha\nWe sell ropes to harbors.\nEvery stage answered or explicitly deferred.",
            "complete",
        )
        .unwrap();
    assert_eq!(result["verified"], true);
    assert_eq!(f.state(&f.a), OnboardingState::Described);
    let mut s = f.spine();
    let mock = MockCognition::new("new-process", vec!["ok"]);
    let primes = mock.reprimes.clone();
    s.attach_lease(Box::new(mock));
    s.submit_prompt("What do you know about my business?", Source::Text)
        .unwrap();
    assert!(primes.lock().unwrap().is_empty(), "reading saved notes must not spend a hidden model turn");
    assert_eq!(s.onboarding_state(s.active_binding().unwrap()), OnboardingState::Described);
    assert!(fs::read_to_string(company::company_file(&f.central,&f.a)).unwrap().contains("We sell ropes to harbors."));
}

#[test]
fn the_tool_scope_supplies_the_exact_destination_even_when_no_folder_exists() {
    let f = Fixture::new();
    let mut s = f.spine();
    let mock = MockCognition::new("fresh", vec!["ok"]);
    let primes = mock.reprimes.clone();
    s.attach_lease(Box::new(mock));
    s.submit_prompt("Let's begin", Source::Text).unwrap();
    assert!(primes.lock().unwrap().is_empty());
    let scope=s.onboarding_tool_scope(s.active_binding().unwrap()).unwrap();
    assert_eq!(scope.central_root,f.central);
    assert_eq!(scope.record_path,f.record);

}

#[test]
fn partial_answers_resume_then_explicit_decline_preserves_them_and_only_that_company() {
    let f = Fixture::new();
    f.save(
        "Stage 1: harbor ropes. Stages 2-6: not discussed yet.",
        "partial",
    )
    .unwrap();
    assert_eq!(f.state(&f.a), OnboardingState::Partial);
    let before = fs::read(company::company_file(&f.central, &f.a)).unwrap();
    onboarding_tools::call(&f.scope, DECLINE_TOOL_NAME, json!({})).unwrap();
    assert!(matches!(f.state(&f.a), OnboardingState::Declined { .. }));
    assert_eq!(f.state(&f.b), OnboardingState::NotYet);
    assert_eq!(
        fs::read(company::company_file(&f.central, &f.a)).unwrap(),
        before
    );
    let layer = CompanyLayer::read(&f.central, &f.a);
    let record = OnboardingRecord::load(&f.record).for_entity(&f.a);
    let prime = onboarding::priming_block(&f.a, Some(&layer), &record).unwrap();
    assert!(prime.contains("harbor ropes"));
    assert!(prime.contains("declined"));
    f.save(
        "Stage 1: harbor ropes. Stage 2: one website. Stages 3-6: not discussed yet.",
        "partial",
    )
    .unwrap();
    assert_eq!(f.state(&f.a), OnboardingState::Partial);
}

#[test]
fn same_thread_button_decline_refreshes_the_next_model_turn() {
    let f = Fixture::new();
    let mut s = f.spine();
    let mock = MockCognition::new("same", vec!["ok", "ok"]);
    let primes = mock.reprimes.clone();
    s.attach_lease(Box::new(mock));
    s.submit_prompt("Hello", Source::Text).unwrap();
    s.record_onboarding_declination(42).unwrap();
    s.submit_prompt("What else can we do?", Source::Text)
        .unwrap();
    let primes = primes.lock().unwrap();
    assert!(primes.is_empty());
    assert!(matches!(s.onboarding_state(s.active_binding().unwrap()), OnboardingState::Declined { .. }));
}

#[test]
fn same_thread_external_tool_save_and_decline_are_detected_before_next_turn() {
    let f = Fixture::new();
    let mut s = f.spine();
    let mock = MockCognition::new("external", vec!["ok", "ok", "ok"]);
    let primes = mock.reprimes.clone();
    s.attach_lease(Box::new(mock));
    s.submit_prompt("Hello", Source::Text).unwrap();
    f.save(
        "Stage 1: rope for harbors. Other stages not discussed.",
        "partial",
    )
    .unwrap();
    s.submit_prompt("Continue", Source::Text).unwrap();
    onboarding_tools::call(&f.scope, DECLINE_TOOL_NAME, json!({})).unwrap();
    s.submit_prompt("Let's do some work", Source::Text).unwrap();
    let primes = primes.lock().unwrap();
    assert!(primes.is_empty());
    assert!(matches!(s.onboarding_state(s.active_binding().unwrap()), OnboardingState::Declined { .. }));
}

#[test]
fn unicode_size_rejection_preserves_the_last_saved_answer() {
    let f = Fixture::new();
    f.save("Existing answer", "partial").unwrap();
    let path = company::company_file(&f.central, &f.a);
    let before = fs::read(&path).unwrap();
    let result = f.save(&"界".repeat(3000), "complete");
    assert!(result.unwrap_err().contains("UTF-8 bytes"));
    assert_eq!(fs::read(path).unwrap(), before);
    let short = "Größe für Häfen. ".repeat(100);
    f.save(&short, "complete").unwrap();
    assert!(matches!(
        CompanyLayer::read(&f.central, &f.a),
        CompanyLayer::Present { .. }
    ));
}

#[test]
fn maximum_budget_includes_the_progress_marker() {
    let f = Fixture::new();
    let overhead = company::COMPLETE_MARKER.len() + 2;
    let exact = "a".repeat(COMPANY_BUDGET_BYTES - overhead);
    let size =
        company::save_company_notes(&f.central, &f.a, &exact, InterviewProgress::Complete).unwrap();
    assert_eq!(size, COMPANY_BUDGET_BYTES);
    assert!(company::save_company_notes(
        &f.central,
        &f.a,
        &format!("{exact}a"),
        InterviewProgress::Complete
    )
    .is_err());
}

#[test]
fn legacy_decline_migrates_once_to_the_restored_company() {
    let f = Fixture::new();
    OnboardingRecord::record_declination(&f.record, 123).unwrap();
    let mut s = f.spine();
    assert!(s.migrate_legacy_onboarding_declination().unwrap());
    assert!(!s.migrate_legacy_onboarding_declination().unwrap());
    assert_eq!(f.state(&f.a), OnboardingState::Declined { at_millis: 123 });
    assert_eq!(f.state(&f.b), OnboardingState::NotYet);
    let body: Value = serde_json::from_slice(&fs::read(&f.record).unwrap()).unwrap();
    assert_eq!(body["version"], 1);
    assert_eq!(body["declined_by_entity"]["alpha"], 123);
}

#[test]
fn malformed_unknown_version_and_invalid_entity_records_never_invent_declines() {
    let f = Fixture::new();
    fs::create_dir_all(f.record.parent().unwrap()).unwrap();
    for body in [
        "garbage \"declined_at_millis\": 123not-json",
        "{\"declined_at_millis\":123",
        "{\"version\":2,\"declined_by_entity\":{\"alpha\":1}}",
        "{\"version\":1,\"declined_by_entity\":{\"../alpha\":1}}",
    ] {
        fs::write(&f.record, body).unwrap();
        assert_eq!(f.state(&f.a), OnboardingState::NotYet, "{body}");
    }
}

#[test]
fn model_arguments_cannot_select_a_different_company_or_destination() {
    let f = Fixture::new();
    for extra in [
        "entity_id",
        "entityId",
        "central_root",
        "record_path",
        "path",
    ] {
        let mut args = json!({"notes":"untrusted redirect","progress":"complete"});
        args[extra] = json!("/somewhere-else");
        assert!(onboarding_tools::call(&f.scope, SAVE_TOOL_NAME, args).is_err());
    }
    assert!(
        onboarding_tools::call(&f.scope, DECLINE_TOOL_NAME, json!({"entityId":"beta"})).is_err()
    );
    assert!(!f.central.exists());
    assert!(!f.record.exists());
}

#[test]
fn tool_scope_is_rebound_by_the_app_and_read_again_for_each_call() {
    let f = Fixture::new();
    f.save("Alpha answer", "complete").unwrap();
    f.bind(&f.b);
    f.save("Beta answer", "partial").unwrap();
    assert!(fs::read_to_string(company::company_file(&f.central, &f.a))
        .unwrap()
        .contains("Alpha answer"));
    assert!(fs::read_to_string(company::company_file(&f.central, &f.b))
        .unwrap()
        .contains("Beta answer"));
}

#[test]
fn unbound_tools_and_invalid_progress_never_save() {
    let f = Fixture::new();
    fs::remove_file(&f.scope).unwrap();
    assert!(f.save("Answer", "complete").is_err());
    f.bind(&f.a);
    assert!(f.save("Answer", "finished").is_err());
    assert!(f.save("   ", "complete").is_err());
    assert!(!f.central.exists());
}

#[test]
fn non_directory_company_home_is_unusable_not_an_unanswered_interview() {
    let f = Fixture::new();
    let home = company::company_home(&f.central, &f.a);
    fs::create_dir_all(home.parent().unwrap()).unwrap();
    fs::write(home, "unexpected file").unwrap();
    assert!(matches!(f.state(&f.a), OnboardingState::Unusable { .. }));
}

#[cfg(unix)]
#[test]
fn inaccessible_company_file_metadata_is_unusable_not_absent() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    f.save("Saved answer", "complete").unwrap();
    let home = company::company_home(&f.central, &f.a);
    fs::set_permissions(&home, fs::Permissions::from_mode(0o000)).unwrap();
    let state = f.state(&f.a);
    fs::set_permissions(home, fs::Permissions::from_mode(0o700)).unwrap();
    // An administrator running tests as root can bypass mode bits.
    if running_as_root() {
        return;
    }
    assert!(
        matches!(state, OnboardingState::Unusable { .. }),
        "{state:?}"
    );
}
#[cfg(unix)]
fn running_as_root() -> bool {
    std::env::var("USER").ok().as_deref() == Some("root")
}

#[test]
fn mcp_stdio_handshake_save_and_errors_share_the_real_persistence_path() {
    let f = Fixture::new();
    let frames = [
        json!({"jsonrpc":"2.0","id":0,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":SAVE_TOOL_NAME,"arguments":{"notes":"Actual protocol save","progress":"partial"}}}),
        json!({"jsonrpc":"2.0","id":4,"method":"unknown"}),
        json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":SAVE_TOOL_NAME,"arguments":{"notes":"bad","progress":"complete","entityId":"beta"}}}),
    ];
    let input = frames
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    let mut output = Vec::new();
    onboarding_tools::serve(&f.scope, std::io::Cursor::new(input), &mut output).unwrap();
    let out: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(out.len(), 6);
    assert_eq!(out[0]["error"]["code"], -32002);
    assert_eq!(out[1]["result"]["protocolVersion"], "2025-03-26");
    // The company notes' two tools, and the user's instructions (`your_instructions.rs`).
    assert_eq!(out[2]["result"]["tools"].as_array().unwrap().len(), 3);
    assert_eq!(out[3]["result"]["isError"], false);
    assert_eq!(out[4]["error"]["code"], -32601);
    assert_eq!(out[5]["result"]["isError"], true);
    assert!(fs::read_to_string(company::company_file(&f.central, &f.a))
        .unwrap()
        .contains("Actual protocol save"));
}

#[test]
fn oversized_mcp_frame_is_drained_without_executing_a_trailing_fragment() {
    let f = Fixture::new();
    let input = format!(
        "{}\n{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}}\n",
        "x".repeat(140 * 1024)
    );
    let mut output = Vec::new();
    onboarding_tools::serve(&f.scope, std::io::Cursor::new(input), &mut output).unwrap();
    let out: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(out.len(), 2);
    assert_eq!(out[0]["error"]["code"], -32600);
    assert_eq!(out[1]["result"], json!({}));
}

#[test]
fn progress_metadata_alone_never_counts_as_an_answer() {
    let f = Fixture::new();
    let path = company::company_file(&f.central, &f.a);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, format!("{}\n \n", company::PARTIAL_MARKER)).unwrap();
    assert_eq!(f.state(&f.a), OnboardingState::NotYet);
}

#[test]
fn storage_refusal_is_about_notes_and_never_claims_the_app_cannot_start() {
    let f = Fixture::new();
    let path = company::company_file(&f.central, &f.a);
    fs::create_dir_all(&path).unwrap();
    let why = f.save("Proposed answer", "complete").unwrap_err();
    assert!(why.contains("notes could not be saved"));
    assert!(!why.contains("standing instruction"));
    assert!(!why.contains("will not start"));
    assert!(path.is_dir());
}

#[test]
fn malformed_and_incomplete_tool_inventories_are_refused() {
    use onboarding_tools::{
        verdict_from_init, OnboardingToolsVerdict, QUALIFIED_DECLINE_TOOL, QUALIFIED_SAVE_TOOL,
    };
    for init in [
        json!({}),
        json!({"tools":null}),
        json!({"tools":[QUALIFIED_SAVE_TOOL]}),
        json!({"tools":[{"name":QUALIFIED_SAVE_TOOL},QUALIFIED_DECLINE_TOOL]}),
    ] {
        assert_eq!(verdict_from_init(&init), OnboardingToolsVerdict::Rejected);
    }
    assert_eq!(
        verdict_from_init(&json!({"tools":["Read",QUALIFIED_SAVE_TOOL,QUALIFIED_DECLINE_TOOL]})),
        OnboardingToolsVerdict::Loaded
    );
}

#[test]
fn onboarding_intake_evidence_requires_a_real_terminal_tool_in_the_same_turn() {
    use richos_core::machinery::MachineryRecord;
    let open = json!({"type":"stream_event","event":{"type":"content_block_start","content_block":{"type":"tool_use","id":"tool-1","name":onboarding_tools::QUALIFIED_SAVE_TOOL,"input":{}}}});
    let call = json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"tool-1","name":onboarding_tools::QUALIFIED_SAVE_TOOL,"input":{"notes":"My answer","progress":"partial"}}]}});
    let done = json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"tool-1","content":"saved","is_error":false}]}});
    let rows = |frame: &Value, seq| {
        MachineryRecord::from_native_event(frame, "lease", seq)
            .into_iter()
            .map(|r| r.stamp("thread", Some("turn"), false))
            .collect::<Vec<_>>()
    };
    let mut records = rows(&open, 1);
    records.extend(rows(&call, 2));
    assert!(!onboarding_tools::handled_in_turn(records.clone(), "turn"));
    records.extend(rows(&done, 3));
    assert!(onboarding_tools::handled_in_turn(records.clone(), "turn"));
    assert!(!onboarding_tools::handled_in_turn(
        records.clone(),
        "other-turn"
    ));
    for record in &mut records {
        record.payload = None;
    }
    assert!(
        onboarding_tools::handled_in_turn(records.clone(), "turn"),
        "survives raw payload eviction"
    );
    for record in &mut records {
        record.internal = true;
    }
    assert!(!onboarding_tools::handled_in_turn(records, "turn"));
    let spoof = json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":"tool-1","name":"Bash","input":{"command":onboarding_tools::QUALIFIED_SAVE_TOOL}}]}});
    let mut records = rows(&spoof, 1);
    records.extend(rows(&done, 2));
    assert!(
        !onboarding_tools::handled_in_turn(records, "turn"),
        "a shell command's text is not a tool identity"
    );
}

#[test]
fn hidden_context_has_no_write_authority_even_if_the_vendor_auto_approves_tools() {
    let f = Fixture::new();
    let denied = OnboardingToolScope::new(&f.a, &f.central, &f.record);
    assert!(!denied.actions_allowed);
    onboarding_tools::write_scope(&f.scope, &denied).unwrap();
    assert!(f
        .save("Hidden side effect", "partial")
        .unwrap_err()
        .contains("internal context"));
    assert!(onboarding_tools::call(&f.scope, DECLINE_TOOL_NAME, json!({})).is_err());
    assert_eq!(f.state(&f.a), OnboardingState::NotYet);
    assert!(!f.record.exists());
    onboarding_tools::set_actions_allowed(&f.scope, true).unwrap();
    f.save("Visible approved answer", "partial").unwrap();
    let before = CompanyLayer::read(&f.central, &f.a);
    onboarding_tools::set_actions_allowed(&f.scope, false).unwrap();
    assert!(f.save("Hidden overwrite", "complete").is_err());
    assert!(onboarding_tools::call(&f.scope, DECLINE_TOOL_NAME, json!({})).is_err());
    assert_eq!(CompanyLayer::read(&f.central, &f.a), before);
    // An old app-issued scope with no phase field is denied rather than granted.
    let mut legacy = serde_json::to_value(&denied).unwrap();
    legacy.as_object_mut().unwrap().remove("actions_allowed");
    fs::write(&f.scope, serde_json::to_vec(&legacy).unwrap()).unwrap();
    assert!(f.save("Legacy hidden overwrite", "complete").is_err());
}

#[test]
fn accepted_request_is_delivered_once_and_never_injected_into_hidden_priming() {
    let f = Fixture::new();
    let mut spine = f.spine();
    let mock = MockCognition::new("hidden-context", vec!["First answer", "Second answer"]);
    let primes = mock.reprimes.clone();
    let prompts = mock.prompts.clone();
    spine.attach_lease(Box::new(mock));
    let first = "First unique CEO request: save the blue notebook answer.";
    spine.submit_prompt(first, Source::Text).unwrap();
    assert!(primes.lock().unwrap().is_empty());
    assert_eq!(prompts.lock().unwrap().as_slice(), [first]);
    // A disk change forces same-lease refresh while the next request is durably accepted.
    f.save("Previously approved company facts.", "partial")
        .unwrap();
    let second = "Second unique CEO request: finish and save exactly once.";
    spine.submit_prompt(second, Source::Text).unwrap();
    assert!(primes.lock().unwrap().is_empty(), "no hidden request is permitted");
    assert_eq!(prompts.lock().unwrap().as_slice(), [first, second]);

}

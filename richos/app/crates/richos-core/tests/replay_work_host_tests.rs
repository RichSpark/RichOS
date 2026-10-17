//! The independent Stu transport over recorded native Claude frames.
//! Native turn completion and later command notifications cannot manufacture job
//! completion or user replies. Stu's explicit reports and verified work tools own those.
use richos_core::assignment::{self, Registration};
use richos_core::cognition::{
    BackgroundCommand, Cognition, CognitionError, LeaseFactory, ObligationState, TurnItem, WorkAssignment,
};
use richos_core::entity::{EntityId, ThreadBinding};
use richos_core::ledger::Ledger;
use richos_core::machinery::MachineryRecord;
use richos_core::native::NativeClient;
use richos_core::steering::TurnCancel;
use richos_core::work_host::{WorkHost, SilentNotifier};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

mod support;

/// The company these jobs belong to (`support::registry`).
const COMPANY: &str = "femcboost";

/// A hang guard, never a verdict.
const HANG_GUARD: Duration = Duration::from_secs(90);

/// The work lease: the real client, and the engine's three answers standing in.
struct ReplayLease {
    client: NativeClient,
    session: String,
}

impl Cognition for ReplayLease {
    fn session_id(&self) -> &str {
        &self.session
    }
    fn background_commands(&self) -> Option<Vec<BackgroundCommand>> {
        Some(self.client.background_commands())
    }

    fn prompt(&mut self, text: &str, on: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        self.client.prompt(text, on).map_err(CognitionError::from)
    }
    fn cancel_handle(&self) -> Option<Arc<dyn TurnCancel>> {
        Some(self.client.cancel_handle())
    }
    fn drain_between_turn(&mut self) -> Vec<MachineryRecord> {
        self.client.drain_between_turn(&self.session)
    }
    // ---- the engine's half, which is not on the provider's wire ----------------------------
    fn bind_work_assignment(&mut self, _assignment: &WorkAssignment) -> Result<(), CognitionError> {
        Ok(())
    }
    fn obligation_state(&self, _obligation: &str) -> Result<ObligationState, CognitionError> {
        Ok(ObligationState::Open)
    }
}

/// Spawns the replay child as the work lease's provider.
struct ReplayFactory {
    child: PathBuf,
    cwd: PathBuf,
    state: PathBuf,
    doctrine: PathBuf,
    skills: PathBuf,
}

impl LeaseFactory for ReplayFactory {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        Err(CognitionError::Protocol("this factory opens work leases only".into()))
    }
    fn spawn_work(&self, _binding: &ThreadBinding) -> Result<Box<dyn Cognition>, CognitionError> {
        let client = NativeClient::spawn(&self.child, &self.cwd, &self.doctrine, &self.skills)
            .map_err(|e| CognitionError::Protocol(e.to_string()))?;
        let session = client.session_id().to_string();
        // The engine's worker evidence for this session: readable, and no helper in it.
        let evidence = self.state.join("evidence").join(&session);
        std::fs::create_dir_all(&evidence).map_err(|e| CognitionError::Io(e.to_string()))?;
        std::fs::write(evidence.join(".lock"), "").map_err(|e| CognitionError::Io(e.to_string()))?;
        std::fs::write(evidence.join("callbacks.jsonl"), "").map_err(|e| CognitionError::Io(e.to_string()))?;
        Ok(Box::new(ReplayLease { client, session }))
    }
}

/// One work host over one capture, in a folder of its own.
struct Replay {
    root: PathBuf,
    state: PathBuf,
    transcript: PathBuf,
    host: Arc<WorkHost>,
    binding: ThreadBinding,
}

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/replay");

fn replay(capture: &str, speed: f64, requests: &[&str]) -> Replay {
    let root = std::env::temp_dir().join(format!("richos-replay-{}", uuid::Uuid::new_v4()));
    let state = root.join("engine-state");
    std::fs::create_dir_all(&state).unwrap();
    let transcript = root.join("transcript.jsonl");
    // The child: `richos-replay` in front of `claude`'s own flags, which arrive as "$@".
    let child = root.join("claude");
    std::fs::write(
        &child,
        format!(
            "#!/bin/sh\nexec '{}' --capture '{FIXTURES}/{capture}' --speed {speed} --transcript '{}' -- \"$@\"\n",
            env!("CARGO_BIN_EXE_richos-replay"),
            transcript.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&child, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let doctrine = richos_core::doctrine::ensure_rendered(&root, &richos_core::doctrine::DoctrineIdentity::default())
        .expect("the doctrine fixture must render");
    let skills = richos_core::skills::ensure_rendered(&root, &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app")).expect("the skills fixture must render");
    // A conversation in the company, as the spine opens one; only the spine mints a binding.
    let mut spine = support::spine(Ledger::open(root.join("spine-ledger.jsonl")).unwrap());
    let binding = spine.ensure_active_thread_in(&EntityId::parse(COMPANY).unwrap()).unwrap();
    drop(spine);
    // His words, as the conversation ledger holds them: the host reads the request from here.
    let rows: Vec<String> = requests
        .iter()
        .enumerate()
        .map(|(n, text)| {
            serde_json::json!({"event": "PromptReceived", "turn_id": format!("turn-{n}"),
                               "thread_id": binding.thread_id(), "entity_id": COMPANY, "source": "text", "text": text})
            .to_string()
        })
        .collect();
    std::fs::write(root.join("conversation-ledger.jsonl"), rows.join("\n") + "\n").unwrap();
    let factory = ReplayFactory { child, cwd: root.clone(), state: state.clone(), doctrine, skills };
    let host = WorkHost::new(&state, Box::new(factory), Arc::new(SilentNotifier));
    Replay { root, state, transcript, host, binding }
}

impl Replay {
    /// His `n`th request, written down as the front desk would.
    fn register(&self, n: usize, text: &str) -> String {
        use sha2::Digest;
        let thread = self.binding.thread_id();
        let request = Registration {
            entity_id: COMPANY.into(),
            thread_id: thread.into(),
            obligation_id: format!("obligation-{n}"),
            instruction_ledger_ref: format!("ledger:{thread}:turn-{n}"),
            instruction_sha256: format!("{:x}", sha2::Sha256::digest(text.as_bytes())),
            title: text.into(),
            repositories: Vec::new(),
            needs_screen: false,
        };
        let receipt=assignment::register(&self.state,&request).unwrap();
        let scope=richos_core::status_tools::StatusToolScope { version:1,state_root:self.state.clone(),entity_id:COMPANY.into(),thread_id:thread.into()};
        let sender=self.root.join("rich-status.json");
        richos_core::session_tools::bind(&sender,&scope,"replay-rich","rich",Some(richos_core::ecs::UserInstruction {
            ledger_ref:request.instruction_ledger_ref,sha256:request.instruction_sha256,text:Some(text.into())})).unwrap();
        richos_core::session_tools::delegate(&sender,&scope,serde_json::json!({"text":text})).unwrap();
        self.host.wake_messages(&self.binding);
        receipt.id
    }

    fn row(&self, id: &str) -> assignment::Assignment {
        assignment::read(&self.state, COMPANY, self.binding.thread_id(), id).unwrap()
    }

    /// What he was told about one job, in order.
    fn told_about(&self, id: &str) -> Vec<String> {
        let scope=richos_core::status_tools::StatusToolScope {version:1,state_root:self.state.clone(),entity_id:COMPANY.into(),thread_id:self.binding.thread_id().into()};
        let _ = id;
        richos_core::session_tools::pending(&scope,"rich").unwrap().into_iter().map(|(message,_)|message.text).collect()
    }

    /// The replay child's own account of the wire, as far as it has written it.
    fn transcript(&self) -> Vec<Value> {
        std::fs::read_to_string(&self.transcript)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    fn events(&self, event: &str) -> Vec<Value> {
        self.transcript().into_iter().filter(|e| e["event"] == event).collect()
    }

    /// The user messages the host sent the provider, in order.
    fn sent(&self) -> Vec<String> {
        self.events("received")
            .into_iter()
            .filter(|e| e["kind"] == "user")
            .map(|e| e["text"].as_str().unwrap_or_default().to_string())
            .collect()
    }

    /// Wait, on the transcript and never on a guess, until `done` holds or the guard runs out.
    fn until(&self, what: &str, done: impl Fn(&Replay) -> bool) {
        let began = Instant::now();
        while !done(self) {
            assert!(began.elapsed() < HANG_GUARD, "never happened: {what}\ntranscript: {:#?}", self.transcript());
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The end of a test that passed. One that failed ends in [`Drop`], the same way.
    fn end(self) {}
}

/// The host is shut down (its leases, and with them their replay children, are dropped) and the
/// folder removed however the test ends: a failing assertion unwinds through here too (§54).
impl Drop for Replay {
    fn drop(&mut self) {
        self.host.shutdown();
        if let Err(e) = std::fs::remove_dir_all(&self.root) {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("[replay] could not remove {}: {e}", self.root.display());
            }
        }
    }
}


/// Count frames before the next unrequested follow-up in these historical captures.
/// We deliberately do not inject the deleted host-written report prompt.
fn frames_before_user_send(capture:&str,user_send:usize)->usize {
    let mut sends=0;let mut frames=0;
    for line in std::fs::read_to_string(Path::new(FIXTURES).join(capture)).unwrap().lines() {
        let row:Value=serde_json::from_str(line).unwrap();
        if row["sent"]["type"]=="user" {sends+=1;if sends>user_send {break}}
        if row.get("frame").is_some() {frames+=1;}
    }
    frames
}
fn check_saved_report(r:&Replay,id:&str) {
    assert_eq!(r.row(id).state,richos_core::assignment::AssignmentState::Running,"a native turn or task notification inferred job completion");
    assert!(r.told_about(id).is_empty(),"the host manufactured a reply on Stu's behalf");
    assert!(r.events("divergence").is_empty(),"the host injected input absent from the user's handoff: {:?}",r.events("divergence"));
}
#[test]
fn a_native_turn_end_does_not_settle_or_report_the_whole_job() {
    let r=replay("2026-09-27-cap-fold.jsonl",4.0,&["Start the requested command"]);
    let job=r.register(0,"Start the requested command");
    assert!(r.host.wait_for_completed(1,HANG_GUARD),"the actual receiving turn never ended");
    check_saved_report(&r,&job);assert_eq!(r.sent().len(),1);r.end();
}
#[test]
fn the_providers_own_late_finish_does_not_create_an_app_report_or_another_prompt() {
    let capture="2026-09-27-cap-plain.jsonl";
    let r=replay(capture,4.0,&["Start the requested command"]);let job=r.register(0,"Start the requested command");
    let frames=frames_before_user_send(capture,1);
    r.until("the native late-finish frames",|r|r.events("emitted").len()>=frames);
    check_saved_report(&r,&job);assert_eq!(r.sent().len(),1);r.end();
}
#[test]
fn a_late_command_notification_does_not_inject_a_hardcoded_followup() {
    let capture="2026-09-27-cap-continue.jsonl";
    let r=replay(capture,4.0,&["Start the requested command"]);let job=r.register(0,"Start the requested command");
    let frames=frames_before_user_send(capture,1);
    r.until("the native late command notification",|r|r.events("emitted").len()>=frames);
    check_saved_report(&r,&job);assert_eq!(r.sent().len(),1);r.end();
}
#[test]
fn another_job_uses_the_same_stu_without_host_completion_or_report_inference() {
    let capture="2026-10-09-long-session.jsonl";
    let r=replay(capture,4.0,&["Start the first command","Start the second command"]);
    let a=r.register(0,"Start the first command");
    assert!(r.host.wait_for_completed(1,HANG_GUARD));
    let b=r.register(1,"Start the second command");
    assert!(r.host.wait_for_completed(2,HANG_GUARD));
    let frames=frames_before_user_send(capture,2);
    r.until("the native first job notification after the second job",|r|r.events("emitted").len()>=frames);
    check_saved_report(&r,&a);check_saved_report(&r,&b);assert_eq!(r.sent().len(),2);r.end();
}

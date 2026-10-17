//! **The BACKGROUND-WORK flow, end to end, on the work lease — the path Ray's candidate-.8
//! walk could never get past.**
//!
//! `work_roundtrip.rs` is this file's older sibling and it drives the CONVERSATION lease's
//! `richos_work` tools. That is the pre-Two-Riches world: `mcp_config` now puts `richos_work`
//! on the work lease only (`role == Work`), so that example no longer exercises the path a
//! background assignment actually takes. This one does.
//!
//! It goes: a fixture repository → a real ECS obligation → `WorkHost::register` → the work
//! lease spawned by `NativeCognition::start_work_lease` → `bind_work_assignment` → the first
//! stream item (which is the ONLY thing that writes `Running`) → whatever the back end then
//! does. The fixture's `git log` is captured before and after and printed either way.
//!
//! **Every one of Ray's three failures happened at or before the first turn** — 6.280 s,
//! 8.639 s and 6.317 s after registration, re-derived from his own timestamps. So reaching
//! `Running` at all is the assertion that clears the class of defect he hit; everything after
//! it is the back end doing its job and is reported rather than asserted.
//!
//! Opt-in, real provider, real subscription cost, synthetic repository. Nothing here touches
//! the CEO's app data: the state root is a fresh temp directory and is removed on the way out
//! unless `RICHOS_PROBE_KEEP_FIXTURE` is set.
//!
//! Usage: `work_lease_roundtrip ENGINE DELIVERED_RUNTIME [SECONDS]`

use richos_core::assignment::{self, AssignmentState, Registration};
use richos_core::cognition::{Cognition, CognitionError, LeaseFactory};
use richos_core::ecs::EcsBridge;
use richos_core::entity::{EntityId, EntityRegistry};
use richos_core::native::resolve_claude_bin;
use richos_core::work_host::{WorkHost, WorkNotifier};
use richos_core::{Ledger, Spine};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        if std::env::var_os("RICHOS_PROBE_KEEP_FIXTURE").is_some() {
            eprintln!("Retained fixture: {}", self.0.display());
        } else {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

/// Every notice the host raises, in order, so the record can quote what he would have read.
#[derive(Default)]
struct Collected(Mutex<Vec<(String, String)>>);
impl WorkNotifier for Collected {
    fn raised(&self, _thread: &str, notice: &assignment::PendingNotice) {
        eprintln!("NOTICE [{:?}] {}", notice.kind, notice.text);
        self.0.lock().unwrap().push((format!("{:?}", notice.kind), notice.text.clone()));
    }
}

/// The production factory's `spawn_work`, reproduced from `src-tauri/src/main.rs` so this
/// probe exercises the same constructor the app uses rather than a convenient stand-in.
struct RealWorkLeases {
    engine: PathBuf,
    data: PathBuf,
    runtime: richos_core::runtime::EngineRuntime,
    doctrine: PathBuf,
    skills: PathBuf,
    permissions: Arc<richos_core::permissions::PermissionDesk>,
}

impl LeaseFactory for RealWorkLeases {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        Err(CognitionError::Protocol("this probe opens work leases only".into()))
    }
    fn spawn_work(
        &self,
        binding: &richos_core::entity::ThreadBinding,
    ) -> Result<Box<dyn Cognition>, CognitionError> {
        let mut profile = richos_core::engine_profile::EngineProfile::prepare(
            &self.engine,
            &self.data,
            self.runtime.clone(),
        )
        .map_err(|e| CognitionError::Io(e.to_string()))?;
        profile.scope_to(binding).map_err(|e| CognitionError::Io(e.to_string()))?;
        profile.permissions = self.permissions.clone();
        let bridge = EcsBridge::new(&self.runtime.python, &self.engine, &self.data.join("ecs"))
            .map_err(|e| CognitionError::Io(e.to_string()))?;
        let lease = richos_core::native::NativeCognition::start_work_lease(
            &resolve_claude_bin(),
            &self.doctrine,
            &self.skills,
            &std::env::current_exe().map_err(|e| CognitionError::Io(e.to_string()))?,
            bridge,
            profile,
        )?;
        Ok(Box::new(lease))
    }
}

fn git(runtime: &richos_core::runtime::EngineRuntime, repo: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new(&runtime.git)
        .arg("-C")
        .arg(repo)
        .args(["-c", "core.hooksPath=/dev/null", "-c", "commit.gpgSign=false",
               "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid"])
        .args(args)
        .output()
        .expect("git");
    if !out.status.success() {
        eprintln!("git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // The executable is handed to the work lease as the app-owned MCP server command. Since
    // 2026-09-18 a work lease registers no app-owned server at all, so this arm should never
    // be taken — it is kept so that if one ever is registered again, this probe answers it
    // rather than printing its usage line into a child's stdin.
    if args.first().is_some_and(|a| a == "--onboarding-mcp") {
        richos_core::onboarding_tools::run_stdio(&PathBuf::from(&args[1]))?;
        return Ok(());
    }
    if args.len() < 2 {
        return Err("usage: work_lease_roundtrip ENGINE DELIVERED_RUNTIME [SECONDS]".into());
    }
    let budget = Duration::from_secs(args.get(2).and_then(|s| s.parse().ok()).unwrap_or(900));
    let engine = std::fs::canonicalize(&args[0])?;

    let root = Scratch(std::env::temp_dir().join(format!("richos-work-lease-{}", uuid::Uuid::new_v4())));
    let data = root.0.join("app-data");
    std::fs::create_dir_all(data.join("corpus/ceo/records"))?;

    let runtime = richos_core::runtime::EngineRuntime::load(&engine, Some(Path::new(&args[1])))?;

    // ---- the fixture repository, and its log BEFORE ---------------------------------------
    let repo = root.0.join("qa-fixture");
    std::fs::create_dir(&repo)?;
    let entity = EntityId::parse("qa-test-co")?;
    let mut registry = EntityRegistry::from_existing_ids(&[entity.clone()]);
    registry = richos_core::repositories::connect(&registry, &entity, &repo, true, &runtime, &[&engine, &data])?.0;
    std::fs::write(repo.join("notes.txt"), "hello\n")?;
    git(&runtime, &repo, &["add", "notes.txt"]);
    git(&runtime, &repo, &["commit", "-qm", "the fixture's starting state"]);
    let before = git(&runtime, &repo, &["log", "--oneline"]);
    let head_before = git(&runtime, &repo, &["rev-parse", "HEAD"]);
    eprintln!("FIXTURE BEFORE  head={head_before}\n{before}\nnotes.txt = {:?}", std::fs::read_to_string(repo.join("notes.txt"))?);
    registry.save(&data.join("entities.json"))?;

    let doctrine = richos_core::doctrine::ensure_rendered(&data, &Default::default())?;
    let skills = richos_core::skills::ensure_rendered(&data, &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../engine/app"))?;
    let bridge = EcsBridge::new(&runtime.python, &engine, &data.join("ecs"))?;

    let mut spine = Spine::new(Ledger::open(&data.join("ledger.jsonl"))?);
    spine.set_entity_registry(registry);
    let thread = spine.create_thread("Background work probe", &entity)?;
    let binding = spine.ledger().thread_binding(&thread)?;

    // ---- the obligation, opened the way the front desk opens it ---------------------------
    // `richos_assignments.record`'s own doc: the obligation is "opened by the conversation
    // before it", and `mega-lander` refuses a dispatch without an accepted open one. The
    // front desk does this with `richos_continuity.checkpoint` on the CEO's seat; here it is
    // done through the same bridge command, so nothing about the obligation is synthetic.
    let session = format!("probe-{}", uuid::Uuid::new_v4());
    let turn = "turn-one";
    let seat = richos_core::ecs::ceo_seat(&thread);
    let ceo = bridge.bind("qa-test-co", &thread, &session, turn, seat.as_deref(), "ceo")?;
    eprintln!("CEO binding: {}", serde_json::to_string(&ceo)?);
    let obligation = "qa-notes-line";
    let opened = bridge.request(
        "checkpoint",
        richos_core::ecs::seated_request(seat.as_deref(), serde_json::json!({
            "binding": ceo,
            "request_id": format!("{session}:{turn}:open"),
            "checkpoint": {"statements": [{"verb": "commitment", "fields": {
                "id": obligation,
                "title": "Add a line to the notes file and land it",
            }}]},
        })),
    )?;
    eprintln!("Obligation opened: {}", serde_json::to_string(&opened)?);

    // ---- the host, with the real factory --------------------------------------------------
    let permissions = Arc::new(richos_core::permissions::PermissionDesk::default());
    let notices = Arc::new(Collected::default());
    // **THE HOST'S STATE ROOT IS `engine-state`, NOT THE DATA DIRECTORY — and getting this
    // wrong made a run that SUCCEEDED report itself as a failure.**
    //
    // `src-tauri/src/main.rs:2030-2031` passes `data_dir.join("engine-state")`, and it has
    // to: `EngineProfile::prepare` puts `RICHOS_APP_STATE` at exactly that path
    // (`engine_profile.rs:96`, `:235`), which is where the engine's hook writes
    // `evidence/<session>/callbacks.jsonl` and where `mega-lander` writes
    // `work-receipts/<partition>/`. Both of the host's readings live there:
    // `app_workers::status` (`state/evidence/...`) and `work_status::trail`
    // (`state/work-receipts/...`).
    //
    // This probe passed `data` on its first outing and both readings looked at directories
    // that never exist. On the run of 2026-09-18 13:42 the work really did land — fixture
    // `5c1fb70` → `d2000e4`, a passing review, a verified completion receipt — and the
    // assignment was reported to the CEO as *"stopped before it finished. No work was
    // started, so nothing was landed."* Nothing was wrong with the product: the probe was
    // reading two empty paths and both of its answers were honest about what it could see.
    //
    // It is named here at length because a probe that is wrong about WHERE is the most
    // expensive kind: every answer it gives is well-formed.
    let state = data.join("engine-state");
    let host = WorkHost::new(
        &state,
        Box::new(RealWorkLeases {
            engine: engine.clone(),
            data: data.clone(),
            runtime: runtime.clone(),
            doctrine,
            skills,
            permissions: permissions.clone(),
        }),
        notices.clone(),
        permissions.clone(),
    );
    host.remember_binding(&binding);
    host.start();

    // Answer the exact-action desk so a routine command does not park forever. Every request
    // is printed, so the record says what the back end actually asked to do.
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let watcher = {
        let (desk, done) = (permissions.clone(), done.clone());
        std::thread::spawn(move || {
            while !done.load(std::sync::atomic::Ordering::SeqCst) {
                if let Some(request) = desk.current() {
                    eprintln!("PERMISSION {} {}", request.tool, request.input);
                    let _ = desk.resolve(&request.id, true);
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        })
    };

    // ---- register, which is the whole of his turn -----------------------------------------
    let registered_at = Instant::now();
    let receipt = assignment::register(&state, &Registration {
        entity_id: "qa-test-co".into(),
        thread_id: thread.clone(),
        obligation_id: obligation.into(),
        instruction_ledger_ref: format!("ledger:{thread}:{turn}"),
        instruction_sha256: format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(
            b"Add a line to notes.txt in the fixture repository saying the work lease ran, and land it.")),
        title: "Add a line to the notes file and land it".into(),
        repositories: vec![repo.display().to_string()],
        needs_screen: false,
    })?;
    let scope=richos_core::status_tools::StatusToolScope {version:1,state_root:state.clone(),entity_id:"qa-test-co".into(),thread_id:thread.clone()};
    let sender=data.join("probe-rich-status.json");
    let record=assignment::read(&state,"qa-test-co",&thread,&receipt.id)?;
    richos_core::session_tools::bind(&sender,&scope,"probe-rich","rich",Some(richos_core::ecs::UserInstruction {
        ledger_ref:record.instruction_ledger_ref,sha256:record.instruction_sha256,text:None}))?;
    richos_core::session_tools::delegate(&sender,&scope,serde_json::json!({"text":record.handoff}))?;
    host.wake_messages(&binding);
    eprintln!("REGISTERED {} at t+0", receipt.id);

    // ---- watch it move, and time every transition against registration --------------------
    let mut seen = AssignmentState::Registered;
    let mut detail = String::new();
    let mut running_at: Option<Duration> = None;
    let mut trail: Vec<serde_json::Value> = Vec::new();
    while registered_at.elapsed() < budget {
        let row = assignment::read(&state, "qa-test-co", &thread, &receipt.id)?;
        // **The DETAIL is watched as well as the state, and that is not decoration.** On the
        // first run of this probe the state sat at `Running` for the whole budget while the
        // detail had silently become the settle reading's "still running" sentence — so the
        // turn had ended and nothing said when. A state-only watcher cannot see that.
        if row.state != seen || row.detail != detail {
            let at = registered_at.elapsed();
            eprintln!("t+{:>8.3}s  {:?} :: {}", at.as_secs_f64(), row.state, row.detail);
            trail.push(serde_json::json!({
                "at_seconds": at.as_secs_f64(),
                "state": format!("{:?}", row.state),
                "detail": row.detail,
                "updated_at_ms": row.updated_at_ms,
            }));
            if row.state == AssignmentState::Running && running_at.is_none() {
                running_at = Some(at);
            }
            seen = row.state;
            detail = row.detail.clone();
        }
        if !seen.is_open() {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let final_row = assignment::read(&state, "qa-test-co", &thread, &receipt.id)?;
    let elapsed = registered_at.elapsed();

    done.store(true, std::sync::atomic::Ordering::SeqCst);
    host.shutdown();
    let _ = watcher.join();

    // ---- the fixture's log AFTER ----------------------------------------------------------
    let after = git(&runtime, &repo, &["log", "--oneline"]);
    let head_after = git(&runtime, &repo, &["rev-parse", "HEAD"]);
    let notes = std::fs::read_to_string(repo.join("notes.txt")).unwrap_or_default();
    eprintln!("FIXTURE AFTER   head={head_after}\n{after}\nnotes.txt = {notes:?}");

    println!("{}", serde_json::json!({
        "assignment_id": receipt.id,
        "final_state": format!("{:?}", final_row.state),
        "final_detail": final_row.detail,
        "reached_running": running_at.is_some(),
        "running_at_seconds": running_at.map(|d| d.as_secs_f64()),
        "watched_for_seconds": elapsed.as_secs_f64(),
        "trail": trail,
        "work_session": final_row.work_session,
        "notices": *notices.0.lock().unwrap(),
        "fixture_head_before": head_before,
        "fixture_head_after": head_after,
        "fixture_log_before": before,
        "fixture_log_after": after,
        "fixture_notes_txt": notes,
        "fixture_gained_a_commit": head_before != head_after,
    }));
    Ok(())
}

//! **The register is the first tool call, and his first words are a few seconds away — measured
//! on a real provider, one task turn and one question turn.** The CEO's ruling §55, 2026-09-18:
//! *"Rich **immediately** replies with: "On it!" and only after that does all that work."* …
//! *"Yes, a few seconds is fine. But 35 seconds of waiting for the first response is not. Go."*
//!
//! # What this measures and what it refuses
//!
//! **Send → his first words**, taken off `LiveEvent::MessageStarted`/`MessageDelta` — the events
//! the webview renders Rich's text from, so this is when the sentence appears rather than when
//! the turn ends. And the TOOL CALLS on his turn ahead of that first word, which is the term §55
//! exists to remove. The rule the run is scored against is
//! [`richos_core::first_reply::first_reply_faults`], unit-tested against the real defect's own
//! frame sequence so it is proven able to go red.
//!
//! This is the after-measurement for `docs/verification/question-receipt-2026-09-18.md`, which
//! measured the same path at 12.197 s, 19.343 s, 22.956 s and 31.023 s to his first words, with
//! two `ToolSearch` round trips and a continuity checkpoint ahead of the register.
//!
//! # And the other half: no ECS write ahead of his words, and does what was written DISPATCH?
//!
//! Two things were added on 2026-09-18 after the register began opening the obligation itself
//! (`assignment_tools.rs`), and neither is a timing measurement:
//!
//! 1. **The front desk's continuity grant is watched on its own thread for the whole of every
//!    turn** ([`GrantWatch`]) — the `richos_continuity` server's own scope file, shut by
//!    `Cognition::prompt` at the start of a turn and opened by
//!    `ReaderState::he_has_now_been_spoken_to` at his first words. The frame-level rule sees an
//!    ECS write that HAPPENED; this sees whether one was POSSIBLE, which is the half a model
//!    that simply chose not to write would hide. It is watched across the PRIMING turn too,
//!    where it must never open: an internal turn he never saw opening the grant would hand the
//!    next real turn one that was already open.
//! 2. **`richos_work.prepare` is driven for real** ([`does_it_dispatch`]), against the
//!    obligation the register opened (must come back `prepared`, with a workspace and a
//!    payload) and against the old shape — a model-typed obligation nobody ever opened (must
//!    be refused at `app.py:318-321`). The previous record could only state this from source;
//!    this measures it.
//!
//!    **It is driven in the LEASE's OWN ENVIRONMENT, which it was not until 2026-09-18.**
//!    `drive_prepare` builds its own environment and had no `RICHOS_SPAWN_HOOK_SOURCES`, so
//!    `spawn.py` collected all nine of the ENGINE's PreToolUse[Agent] guards instead of the
//!    app's own preflight, and was refused by `guard-owned-state.sh` over the development
//!    session's paused CI — a real, reproducible refusal of a path the shipped app never
//!    takes. And the check scored REACHING the dispatch rather than surviving it, so the run
//!    printed PASS over it. Both are fixed here: the value comes from
//!    `EngineProfile::spawn_hook_sources()`, the same accessor `configure` uses, and only
//!    `status: "prepared"` is a pass.
//!
//! # What it costs and what it touches
//!
//! **`RICHOS_PROBE_DISPATCH_ONLY=1` runs the whole of (2) with NO lease, NO provider and NO
//! model turn**: the register's own code writes the assignment and the engine's own `prepare`
//! judges it, so the join can be re-checked for nothing, forever. That mode is also how the
//! dispatch section was developed and proven red before a single model turn was spent on it.
//!
//! **THREE MODEL TURNS on the CEO's subscription** for the full run — the lease's PRIMING turn,
//! then one task and one question — and it is opt-in for exactly that reason. The priming turn is counted here
//! because it always ran: before 2026-09-18 it ran inside the first `submit_prompt` and the probe
//! measured it as part of turn 1 without naming it, which is how a run of this example came to be
//! described as two model turns when it spent three. Everything it writes goes into a throwaway directory under the system
//! temp directory, which it removes on the way out (CEO §54). `HOME` is left alone, because the
//! provider credentials live there and a synthetic one would make this probe measure a sign-in
//! failure. It opens no audio device, plays nothing, and puts no window on screen — so it says
//! nothing about what the timer beside his reply reads (`ui/tests/question-timer.js` is that).
//!
//! Run:
//! ```text
//! cargo run -p richos-core --example first_reply_timing_e2e -- <engine> <delivered-runtime>
//! RICHOS_PROBE_DISPATCH_ONLY=1 cargo run -p richos-core --example first_reply_timing_e2e -- …
//! ```

use richos_core::live::LiveEvent;
use richos_core::{ecs::EcsBridge, native::{resolve_claude_bin, NativeCognition}};
use richos_core::{Cognition, EntityId, EntityRegistry, Ledger, Source, Spine};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Removed on the way out unless `RICHOS_PROBE_KEEP_FIXTURE` is set — the contract every
/// real-provider example in this directory keeps, and the CEO's §54.
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

/// Every tool call on his turn AHEAD of the reply, in arrival order, with its offset from the
/// send. The one term §55 is about.
///
/// `trace` is the same events WITHOUT the cut-off — every tool-call frame and his first words, in
/// arrival order — because since the app says the register's sentence itself, the evidence that it
/// did so is an ORDERING: his words arrive BEFORE the machinery record built from the very frame
/// they were read out of. A list that stops at his first word cannot show that.
#[derive(Default)]
struct BeforeHeSpoke {
    calls: Mutex<Vec<(String, f64)>>,
    trace: Mutex<Vec<(String, f64)>>,
    start: Mutex<Option<Instant>>,
    spoken: Mutex<bool>,
}
impl richos_core::machinery::MachineryObserver for BeforeHeSpoke {
    fn on_machinery(&self, record: &richos_core::machinery::MachineryRecord) {
        // **THE PERMISSION FRAME IS TRACED BUT NEVER SCORED**, because it is what decomposes the
        // ~2 s between the model finishing the register's arguments and the register answering.
        // The app's own desk answers it in-process, so whatever that 2 s is, this line says which
        // side of the desk it is on. It is not a tool call and the §55 rule never sees it.
        if record.kind == richos_core::machinery::MachineryKind::PermissionRequested {
            let since = self.start.lock().unwrap().map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);
            self.trace.lock().unwrap().push((format!("permission asked+answered: {}", record.title), since));
            return;
        }
        if record.kind != richos_core::machinery::MachineryKind::ToolCall {
            return;
        }
        let since = self.start.lock().unwrap().map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);
        let name = if record.title.is_empty() { record.kind.as_str().to_string() } else { record.title.clone() };
        {
            // The full trace, both sides of his first word. A closing record (no tool name on this
            // wire) carries the OUTCOME, which is what the app reads the sentence out of, so it is
            // labeled with its summary rather than with the kind's name.
            let mut trace = self.trace.lock().unwrap();
            let label = if record.title.is_empty() {
                format!("tool_call closed: {}", record.summary.clone().unwrap_or_default())
            } else {
                name.clone()
            };
            if trace.last().map(|(last, _)| last == &label) != Some(true) {
                trace.push((label, since));
            }
        }
        if *self.spoken.lock().unwrap() {
            return;
        }
        let mut calls = self.calls.lock().unwrap();
        // Merge the update frames for one call (§1.4 G2): a repeated title straight after itself
        // is the same step reported twice, not two steps.
        if calls.last().map(|(last, _)| last == &name) != Some(true) {
            calls.push((name, since));
        }
    }
}

/// The first instant Rich's own text reached the surface, and **every separate run of prose he
/// can see afterwards**.
///
/// The first instant is the §55 measure — a later delta is more of the same sentence. The RUNS
/// are here because the first run of this probe produced `"On it!On it!"`: two message runs, not
/// one doubled string, and the difference between those two readings is the difference between a
/// model that closed its turn with a second copy of its line and an app that rendered one line
/// twice. `message_id` is what separates them, so it is recorded rather than reasoned about.
#[derive(Default)]
struct FirstWords {
    at: Mutex<Option<Instant>>,
    /// `(message_id, offset of its first event, its text)`, in the order he saw them, and only
    /// what he can actually see — an internal or technical run is not his conversation.
    runs: Mutex<Vec<(String, f64, String)>>,
    start: Mutex<Option<Instant>>,
}
impl richos_core::live::LiveObserver for FirstWords {
    fn on_live_event(&self, event: &LiveEvent) {
        use richos_core::Visibility;
        let since = || self.start.lock().unwrap().map(|s: Instant| s.elapsed().as_secs_f64()).unwrap_or(0.0);
        match event {
            LiveEvent::MessageStarted { message_id, visibility: Visibility::Ceo, .. } => {
                let mut at = self.at.lock().unwrap();
                if at.is_none() {
                    *at = Some(Instant::now());
                }
                let mut runs = self.runs.lock().unwrap();
                if !runs.iter().any(|(id, _, _)| id == message_id) {
                    runs.push((message_id.clone(), since(), String::new()));
                }
            }
            LiveEvent::MessageDelta { message_id, text_delta, visibility: Visibility::Ceo, .. } => {
                let mut at = self.at.lock().unwrap();
                if at.is_none() {
                    *at = Some(Instant::now());
                }
                let mut runs = self.runs.lock().unwrap();
                match runs.iter_mut().find(|(id, _, _)| id == message_id) {
                    Some((_, _, text)) => text.push_str(text_delta),
                    None => runs.push((message_id.clone(), since(), text_delta.clone())),
                }
            }
            _ => {}
        }
    }
}

/// **The front desk's continuity grant, watched on its own thread for the whole of a turn.**
///
/// This is the second scope file — `richos_continuity`'s own copy, the one
/// `ReaderState::he_has_now_been_spoken_to` opens at his first words and `Cognition::prompt`
/// shuts at the start and end of every turn. It is what the engine's adapter actually reads,
/// re-read on every call (`engine/ecs/adapters/mcp.py:18-26`), so **WHEN it flips is the whole
/// of "no ECS write precedes his first words"** — the frame-level rule
/// ([`richos_core::first_reply::first_reply_faults`]) sees a write that happened, and this
/// sees whether one was POSSIBLE.
///
/// **Why a poller and not a read at the event.** The grant is opened inside the reader, under
/// its lock, in the same call that routes his first words — reading the file from the live
/// observer would read it after the flip whatever the true order was, and putting a filesystem
/// read on the thread whose latency is the measurement is the last thing this probe should do.
/// A 400-byte read every 2 ms on a thread of its own costs nothing and is biased LATE, which is
/// the safe direction: an observed open before his first words is a real fault, never an
/// artifact of the sampling.
struct GrantWatch {
    stop: Arc<std::sync::atomic::AtomicBool>,
    opened_at: Arc<Mutex<Option<f64>>>,
    /// Was it shut when the watch began? On a turn, that is `Cognition::prompt`'s positive
    /// close; on the priming turn, it is the file `start_with_engine` writes.
    closed_at_start: bool,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl GrantWatch {
    fn is_open(path: &std::path::Path) -> bool {
        std::fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|value| value.get("actions_allowed").and_then(serde_json::Value::as_bool))
            == Some(true)
    }

    fn start(path: &std::path::Path, from: Instant) -> Self {
        let closed_at_start = !Self::is_open(path);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let opened_at: Arc<Mutex<Option<f64>>> = Arc::new(Mutex::new(None));
        let handle = {
            let path = path.to_path_buf();
            let stop = stop.clone();
            let opened_at = opened_at.clone();
            std::thread::spawn(move || {
                while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                    if Self::is_open(&path) {
                        *opened_at.lock().unwrap() = Some(from.elapsed().as_secs_f64());
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
            })
        };
        Self { stop, opened_at, closed_at_start, handle: Some(handle) }
    }

    /// `(it was shut when the watch began, the first instant it was seen open)`.
    fn finish(mut self) -> (bool, Option<f64>) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        (self.closed_at_start, *self.opened_at.lock().unwrap())
    }
}

/// The `prepare` harness, run by the delivered interpreter against the engine's own module.
///
/// `app.py`'s `call` IS the whole tool handler — `__main__` only wires it to the MCP transport
/// (`transport.serve(..., handler=call)`), and every gate this probe is about lives inside it:
/// `read_scope` (the grant, the host-issued partition, the workspace authority) and then
/// `prepare`'s own obligation check. So this calls `call` directly rather than reimplementing a
/// JSON-RPC handshake to reach the same function.
const PREPARE_HARNESS: &str = r#"import importlib.util, json, sys
spec = importlib.util.spec_from_file_location("richos_mega_lander_app", sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
try:
    print(json.dumps({"ok": True, "result": module.call(sys.argv[2], "prepare", json.loads(sys.argv[3]))}))
except Exception as error:
    print(json.dumps({"ok": False, "error": "%s: %s" % (type(error).__name__, error)}))
"#;

/// **The engine's own refusal when the obligation is not there to dispatch against** — the two
/// sentences `prepare` can stop at on lines 318-321 of `mega-lander/app.py`, quoted from the
/// engine rather than paraphrased: `ecs_inspect.py:78` raises the first when the item does not
/// exist on the seat doing the looking, and `app.py:321` raises the second when it exists and
/// its status is outside `("accepted","active","pending","blocked")`.
fn refused_at_the_obligation_gate(detail: &str) -> bool {
    detail.contains("item is absent or outside the active scope")
        || detail.contains("dispatch requires an accepted open obligation")
}

/// One real `prepare`, driven exactly as a work lease drives it.
///
/// Everything here is the host's, which is the point: the work scope is written by
/// `native.rs`'s `prepare_work_turn` in the app and by this function in the probe, with the
/// same fields — `actions_allowed: true` (the standing grant, spec §5.4), the binding from
/// `bind_work_seat` whose `turn_id` IS the obligation, and the frozen instruction reference.
/// `carried_obligation` then reads the obligation off that binding and the model never sees it.
#[allow(clippy::too_many_arguments)]
fn drive_prepare(
    bridge: &EcsBridge,
    runtime: &richos_core::runtime::EngineRuntime,
    engine: &PathBuf,
    state_root: &PathBuf,
    coordination: &PathBuf,
    scratch: &PathBuf,
    entity: &str,
    thread: &str,
    session: &str,
    obligation: &str,
    seat: &str,
    instruction_ledger_ref: &str,
    instruction_sha256: &str,
    repo: &str,
    request_id: &str,
    title: &str,
    hook_sources: &str,
) -> Result<(bool, String), Box<dyn std::error::Error>> {
    let binding = bridge.bind_work_seat(entity, thread, session, obligation, seat)?;
    let scope_path = scratch.join(format!("work-scope-{}.json", uuid::Uuid::new_v4()));
    richos_core::ecs::write_scope(
        &scope_path,
        &richos_core::ecs::ToolScope { read_only_tools: Vec::new(),
            version: 1,
            actions_allowed: true,
            bridge: bridge.clone(),
            binding,
            user_instruction: Some(richos_core::ecs::UserInstruction {
                ledger_ref: instruction_ledger_ref.to_string(),
                sha256: instruction_sha256.to_string(),
                text: None,
            }),
            seat: Some(seat.to_string()),
            // This probe drives `prepare` directly and dispatches no worker, so the standing
            // worker grant is not part of what it measures.
            background_work_allowed: false,
        },
    )?;
    let harness = scratch.join("drive_prepare.py");
    std::fs::write(&harness, PREPARE_HARNESS)?;
    // `read_scope`'s workspace-authority check: the same digest `EngineProfile::workspace_state`
    // computes, and `app.py:64-66` recomputes, over `[entity_id, thread_id]`.
    use sha2::Digest;
    let partition = format!("{:x}", sha2::Sha256::digest(serde_json::to_vec(&(entity, thread))?));
    let workspaces = state_root.join("workspaces").join(&partition);
    let arguments = serde_json::json!({
        "request_id": request_id,
        "repo": repo,
        "title": title,
        "brief": "This probe never dispatches the payload. It exists to find out whether what the \
                  register wrote is a thing the engine will prepare work against.",
        "role": "worker",
    });
    let mut command = std::process::Command::new(&runtime.python);
    richos_core::runtime::isolate_interpreter_environment(&mut command);
    command
        .arg(&harness)
        .arg(engine.join("mega-lander/app.py"))
        .arg(&scope_path)
        .arg(arguments.to_string())
        .env("PATH", runtime.path())
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("RICHOS_APP_STATE", state_root)
        .env("RICHOS_APP_ENTITY", entity)
        .env("RICHOS_APP_THREAD", thread)
        .env("RICHOS_WORKSPACES_DIR", &workspaces)
        .env("RICHOS_APP_REGISTRY", coordination.parent().unwrap().join("entities.json"))
        .env("RICHOS_ENTITY_ROOT", coordination)
        .env("RICHOS_ENGINE_ROOT", engine)
        .env("RICHOS_ENGINE_DIR", engine)
        // **THE VARIABLE THE LEASE ALWAYS SETS, AND THIS FUNCTION NEVER DID.**
        //
        // `EngineProfile::configure` exports `RICHOS_SPAWN_HOOK_SOURCES` for the provider
        // child, and the `richos_work` server — `mega-lander/app.py`, a child of that child
        // — inherits it, so `spawn.py`'s `settings_sources` reads the app's own
        // `spawn-preflight.json` and nothing else. This function builds its own environment
        // and omitted it, so on 2026-09-18 it measured `spawn.py` collecting all NINE of the
        // ENGINE's PreToolUse[Agent] guards off `engine/hooks/hooks.json` and being refused
        // by `guard-owned-state.sh` over the development session's paused CI.
        //
        // That refusal was real and reproducible AND IT WAS ABOUT A PATH THE SHIPPED APP
        // NEVER TAKES: the same binary, with this line present, answers `prepared`. A probe
        // measuring a different environment than the product is not a weaker probe; it is
        // one that produces confident findings about nothing, and this one produced an
        // escalation.
        //
        // The value comes from `EngineProfile::spawn_hook_sources()` — the same accessor
        // `configure` calls — so the two cannot drift apart by omission again.
        .env("RICHOS_SPAWN_HOOK_SOURCES", hook_sources);
    let output = command.output()?;
    let answer: serde_json::Value = serde_json::from_slice(&output.stdout).map_err(|_| {
        format!(
            "the prepare harness said nothing this reader understands: stdout {:?} stderr {:?}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })?;
    if answer["ok"] == serde_json::Value::Bool(true) {
        Ok((true, answer["result"].to_string()))
    } else {
        Ok((false, answer["error"].as_str().unwrap_or_default().to_string()))
    }
}

/// **DOES WHAT THE REGISTER WROTE ACTUALLY DISPATCH?** — `mega-lander/app.py:318-321`, driven.
///
/// Returns the faults, so the caller scores this on the same list as everything else.
fn does_it_dispatch(
    bridge: &EcsBridge,
    runtime: &richos_core::runtime::EngineRuntime,
    engine: &PathBuf,
    state_root: &PathBuf,
    coordination: &PathBuf,
    scratch: &PathBuf,
    session: &str,
    repo: &std::path::Path,
    recorded: &[richos_core::assignment::Assignment],
    hook_sources: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut failures = Vec::new();
    // ====================================================================================
    // AND DOES WHAT THE REGISTER WROTE ACTUALLY DISPATCH? — `mega-lander/app.py:318-321`
    // ====================================================================================
    //
    // **THE JOIN THIS WHOLE SLICE EXISTS FOR, DRIVEN ON THE REAL ENGINE RATHER THAN READ OFF
    // ITS SOURCE.** The previous record could only state it: *"both of run A's assignments carry
    // an obligation that was never opened, and `prepare` would refuse them"* — read off
    // `app.py:318-321` and the absence of any ECS write in run A's trace, with no dispatch
    // attempted. That is the sentence this section replaces with a measurement.
    //
    // Both directions are driven, because a green that cannot go red is not evidence:
    //
    // - **GREEN** — each assignment the register wrote, against the obligation the register
    //   opened for it. The engine must get PAST its obligation gate.
    // - **RED** — the OLD SHAPE, and it is not a synthetic one: the obligation id is
    //   `land-pricing-branch-staging-deploy`, verbatim what run B's model minted for the same
    //   sentence (`first-words-2026-09-18.md`, finding 2), carried by a lease exactly as a real
    //   one would carry it. Nothing ever opened it, so the engine must refuse.
    //
    // **The obligation is also read straight off the store first**, on the work seat, with the
    // same `inspect` `prepare` makes — so a green here is a positive observation of the item's
    // status and not merely `prepare` failing somewhere else.
    let repo_argument = std::fs::canonicalize(repo)?.to_string_lossy().to_string();
    let mut dispatch = Vec::new();
    let opened_by_the_register: Vec<(String, String)> =
        recorded.iter().map(|row| (row.obligation_id.clone(), row.seat.clone())).collect();
    let old_shape = ("land-pricing-branch-staging-deploy".to_string(),
                     "work-seat:land-pricing-branch-staging-deploy".to_string());
    for (index, (obligation, seat)) in opened_by_the_register.iter().chain(std::iter::once(&old_shape)).enumerate() {
        let from_the_register = index < opened_by_the_register.len();
        let row = recorded.first();
        let (entity, thread_id, ledger_ref, digest) = match row {
            Some(row) => (row.entity_id.clone(), row.thread_id.clone(),
                          row.instruction_ledger_ref.clone(), row.instruction_sha256.clone()),
            None => break,
        };
        let binding = bridge.bind_work_seat(&entity, &thread_id, session, obligation, seat)?;
        let state = bridge.obligation_state(&binding, Some(seat), obligation)?;
        let (ok, detail) = drive_prepare(
            bridge, runtime, engine, state_root, coordination, scratch,
            &entity, &thread_id, &session, obligation, seat, &ledger_ref, &digest,
            &repo_argument, &format!("probe-dispatch-{index}"), "Probe: does this obligation dispatch?",
            hook_sources,
        )?;
        eprintln!("---");
        eprintln!(
            "prepare against {} obligation {obligation}",
            if from_the_register { "the REGISTER's own" } else { "the OLD SHAPE's never-opened" }
        );
        eprintln!("    the store says its status is : {state:?}");
        eprintln!("    prepare {} : {detail}", if ok { "answered" } else { "refused" });
        dispatch.push((from_the_register, obligation.clone(), state, ok, detail));
    }
    for (from_the_register, obligation, state, ok, detail) in &dispatch {
        if *from_the_register {
            if *state != richos_core::cognition::ObligationState::Open {
                failures.push(format!(
                    "the register opened {obligation} and the store reads it {state:?} from the work \
                     seat — `prepare` inspects it exactly there (`app.py:319`)"
                ));
            }
            if refused_at_the_obligation_gate(detail) {
                failures.push(format!(
                    "`prepare` refused the register's own obligation {obligation} at the dispatch \
                     gate: {detail}"
                ));
            }
            // **AND IT WENT ALL THE WAY THROUGH, which is what makes this a positive
            // observation rather than "it failed somewhere else".** After the obligation gate
            // `prepare` still checks the host-attested instruction, the connected repository,
            // the main checkout, the role, the request-id reuse and the per-obligation
            // unresolved-work rule, then writes the receipt and the brief, builds the spawn
            // command and RUNS it (`app.py:322-440`). Only `status: "prepared"` means every
            // one of those passed.
            //
            // **THIS CHECK USED TO STOP ONE STEP SHORT, AND THE STEP IT STOPPED SHORT OF IS
            // THE ONE THAT WAS BROKEN.** It read `if !ok && !detail.contains("spawn: refused
            // by")` — a spawn refusal counted as REACHING the dispatch and therefore as a
            // pass. That is how the probe reported `PASS: every obligation the register
            // opened is one the engine will prepare work against` on a run where every one of
            // them was refused and nothing was created. Scoring the step BEFORE the one that
            // matters is the same defect as measuring the wrong environment, and this file
            // had both at once. A user does not get a worker out of a receipt that reached
            // the dispatch; he gets one out of a dispatch that happened.
            if !ok || !detail.contains("\"status\":\"prepared\"") {
                failures.push(format!(
                    "`prepare` did not come back prepared on the register's own obligation \
                     {obligation}, so nothing here shows the assignment is dispatchable: {detail}"
                ));
            }
        } else {
            if *state != richos_core::cognition::ObligationState::Absent {
                failures.push(format!(
                    "the old shape's obligation {obligation} reads {state:?} in the store — it was \
                     never opened, so this probe is no longer testing the thing it says it is"
                ));
            }
            if *ok || !refused_at_the_obligation_gate(detail) {
                failures.push(format!(
                    "the OLD SHAPE was not refused at the dispatch gate, so the green above proves \
                     nothing: prepare said {detail}"
                ));
            }
        }
    }
    Ok(failures)
}

/// **A `LeaseFactory` for this probe** — what `EngineLeaseFactory::spawn_chat` is to the app,
/// reduced to the four things a conversation lease is made of.
///
/// It exists because a SPARE front desk is spawned by the spine rather than attached to it: the
/// spine reserves a thread id and asks the factory for a child scoped to it, which is the whole
/// point (`EngineProfile::scope_to` pins `(entity, thread)` into the child's environment before
/// the spawn). `attach_lease` cannot express that.
///
/// A fresh `EngineProfile` per spawn, exactly as the app does it: `prepare` mints a new
/// `engine-profiles/<uuid>` plugin directory each call, and `NativeCognition::drop` removes its
/// OWN — two leases sharing one would have the first drop take the second's plugin out from
/// under it.
struct ProbeLeaseFactory {
    engine: PathBuf,
    data: PathBuf,
    runtime: richos_core::runtime::EngineRuntime,
    doctrine: PathBuf,
    skills: PathBuf,
    bin: PathBuf,
    executable: PathBuf,
    bridge: EcsBridge,
}

impl richos_core::cognition::LeaseFactory for ProbeLeaseFactory {
    fn spawn(&self) -> Result<Box<dyn Cognition>, richos_core::cognition::CognitionError> {
        Err(richos_core::cognition::CognitionError::Protocol(
            "this probe only ever spawns SCOPED leases — an unscoped one would be a child with no              company and no conversation, which the app never creates either".into(),
        ))
    }

    fn spawn_scoped(
        &self,
        binding: &richos_core::entity::ThreadBinding,
        control: &richos_core::steering::TurnControl,
    ) -> Result<Box<dyn Cognition>, richos_core::cognition::CognitionError> {
        use richos_core::cognition::CognitionError;
        let mut profile =
            richos_core::engine_profile::EngineProfile::prepare(&self.engine, &self.data, self.runtime.clone())
                .map_err(|e| CognitionError::Io(e.to_string()))?;
        profile.scope_to(binding).map_err(|e| CognitionError::Io(e.to_string()))?;
        let cognition = NativeCognition::start_with_engine(
            &self.bin, &self.doctrine, &self.skills, &self.executable,
            self.bridge.clone(), profile, Some(control),
        )?;
        Ok(Box::new(cognition))
    }
}

/// **THE MACHINE THE ENGINE OFFER IS SHOWN ON: nothing to start `claude` in.**
///
/// `RICHOS_PROBE_REPAIR`'s precondition, and it is the real failure rather than a stand-in for
/// it: with no engine directory, `NativeCognition::start_with_engine` cannot spawn, every
/// priming hook that reaches it fails, and `Spine::ready_a_spare_front_desk` answers
/// `SpareReady::NotReady`. That is the state Ray's Mac was in at 23:23Z on candidate .11.
struct NoEngineFactory;

impl richos_core::cognition::LeaseFactory for NoEngineFactory {
    fn spawn(&self) -> Result<Box<dyn Cognition>, richos_core::cognition::CognitionError> {
        Err(richos_core::cognition::CognitionError::Io(
            "there is no engine on this machine yet".into(),
        ))
    }
    fn spawn_scoped(
        &self,
        _binding: &richos_core::entity::ThreadBinding,
        _control: &richos_core::steering::TurnControl,
    ) -> Result<Box<dyn Cognition>, richos_core::cognition::CognitionError> {
        Err(richos_core::cognition::CognitionError::Io(
            "there is no engine on this machine yet".into(),
        ))
    }
}

/// **What the spare's child costs while it sits there** — RSS and CPU of the whole process tree
/// under this probe, sampled with `ps`.
///
/// `ps` rather than anything clever, and the tree rather than the direct child, because the
/// question is what the MACHINE carries: `claude` is a Node process that starts its own children,
/// and a number that counted only the one this process forked would understate it by whatever
/// those weigh. `rss` is in kilobytes on macOS; `%cpu` is the process's share since it started,
/// which is why the sample is a SERIES — a single reading of it would be dominated by the
/// priming turn that just finished.
fn sample_tree(root: u32) -> Option<(u64, f64, usize)> {
    let pids = {
        let mut all = vec![root];
        let mut frontier = vec![root];
        while let Some(parent) = frontier.pop() {
            let out = std::process::Command::new("pgrep").arg("-P").arg(parent.to_string()).output().ok()?;
            for line in String::from_utf8_lossy(&out.stdout).lines() {
                if let Ok(pid) = line.trim().parse::<u32>() {
                    all.push(pid);
                    frontier.push(pid);
                }
            }
        }
        all
    };
    let mut command = std::process::Command::new("ps");
    command.arg("-o").arg("rss=,%cpu=");
    for pid in &pids {
        command.arg("-p").arg(pid.to_string());
    }
    let out = command.output().ok()?;
    let (mut rss, mut cpu, mut seen) = (0u64, 0f64, 0usize);
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let mut parts = line.split_whitespace();
        let (Some(r), Some(c)) = (parts.next(), parts.next()) else { continue };
        rss += r.parse::<u64>().unwrap_or(0);
        cpu += c.parse::<f64>().unwrap_or(0.0);
        seen += 1;
    }
    Some((rss, cpu, seen))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    // **The app-owned MCP servers are served by THIS executable**, because `mcp_config` points
    // the child at `current_exe()` (`native.rs`). In the app that is `richos-tauri`; here it is
    // this example, so it answers the same three flags — a probe whose front desk came up
    // without the register would report a model failure that is really a wiring failure.
    if args.len() == 2 {
        let scope = PathBuf::from(&args[1]);
        match args[0].to_string_lossy().as_ref() {
            "--onboarding-mcp" => {
                richos_core::onboarding_tools::run_stdio(&scope)?;
                return Ok(());
            }
            "--assignments-mcp" => {
                richos_core::assignment_tools::run_stdio(&scope)?;
                return Ok(());
            }
            "--status-mcp" => {
                richos_core::status_tools::run_stdio(&scope, &richos_core::screen::UnknownScreen)?;
                return Ok(());
            }
            _ => {}
        }
    }
    if args.len() != 2 {
        return Err("usage: first_reply_timing_e2e ENGINE DELIVERED_RUNTIME".into());
    }
    let engine = std::fs::canonicalize(&args[0])?;
    let root = Scratch(std::env::temp_dir().join(format!("richos first reply {}", uuid::Uuid::new_v4())));
    let data = root.0.join("app data");
    std::fs::create_dir_all(data.join("corpus/ceo/records"))?;

    let runtime = richos_core::runtime::EngineRuntime::load(&engine, Some(&PathBuf::from(&args[1])))?;
    let mut profile = richos_core::engine_profile::EngineProfile::prepare(&engine, &data, runtime.clone())?;
    let mut registry = EntityRegistry::from_existing_ids(&[EntityId::parse("depot")?]);
    let repo = root.0.join("first project");
    std::fs::create_dir(&repo)?;
    registry =
        richos_core::repositories::connect(&registry, &EntityId::parse("depot")?, &repo, true, &runtime, &[&engine, &data])?.0;
    registry.save(&data.join("entities.json"))?;

    let doctrine = richos_core::doctrine::ensure_rendered(&data, &Default::default())?;
    let skills = richos_core::skills::ensure_rendered(&data)?;
    let bridge = EcsBridge::new(&runtime.python, &engine, &data.join("ecs"))?;

    let mut spine = Spine::new(Ledger::open(&data.join("ledger.jsonl"))?);
    spine.set_entity_registry(registry);
    spine.set_machinery_journal(richos_core::journal::MachineryJournal::new(data.join("machinery")));
    let first_words = Arc::new(FirstWords::default());
    let before = Arc::new(BeforeHeSpoke::default());
    struct Forward(Arc<FirstWords>, Arc<BeforeHeSpoke>);
    impl richos_core::live::LiveObserver for Forward {
        fn on_live_event(&self, event: &LiveEvent) {
            self.0.on_live_event(event);
            if matches!(event, LiveEvent::MessageStarted { .. } | LiveEvent::MessageDelta { .. }) {
                let mut spoken = self.1.spoken.lock().unwrap();
                if !*spoken {
                    let since = self.1.start.lock().unwrap().map(|s: Instant| s.elapsed().as_secs_f64()).unwrap_or(0.0);
                    self.1.trace.lock().unwrap().push(("HIS FIRST WORDS".to_string(), since));
                }
                *spoken = true;
            }
        }
    }
    spine.set_live_observer(Box::new(Forward(first_words.clone(), before.clone())));
    struct ForwardMachinery(Arc<BeforeHeSpoke>);
    impl richos_core::machinery::MachineryObserver for ForwardMachinery {
        fn on_machinery(&self, record: &richos_core::machinery::MachineryRecord) {
            self.0.on_machinery(record);
        }
    }
    spine.set_machinery_observer(Box::new(ForwardMachinery(before.clone())));

    // ====================================================================================
    // `RICHOS_PROBE_SELF_RSS=<seconds>` — THE SUBTRAHEND, AND IT COSTS NOTHING
    // ====================================================================================
    //
    // `RICHOS_PROBE_SPARE_COST` samples the whole process tree under this probe, because that is
    // what the machine actually carries — but the tree's root is this probe itself, and the ECS
    // bridge's Python is in it too. A number that included them and called itself "the cost of a
    // spare" would be wrong by however much they weigh.
    //
    // So this mode runs everything above (the same engine profile, the same bridge, the same
    // `hello`) and NO lease, and samples the same tree with the same function. The difference
    // between the two runs is the spare, and it is arithmetic a reader can redo rather than an
    // apportionment somebody made. **No lease, no provider, no model turn.**
    if let Some(seconds) = std::env::var("RICHOS_PROBE_SELF_RSS").ok().and_then(|v| v.parse::<u64>().ok()) {
        eprintln!("RICHOS_PROBE_SELF_RSS: no lease is started and no model turn is spent.");
        let me = std::process::id();
        for tick in 0..=seconds {
            if tick > 0 { std::thread::sleep(std::time::Duration::from_secs(1)); }
            if let Some((rss, cpu, procs)) = sample_tree(me) {
                eprintln!("  t+{tick:>4} s : {procs} processes, {rss:>9} KB RSS total, {cpu:>6.1} % CPU (since start)");
            }
        }
        eprintln!("---");
        eprintln!("PASS: this is the probe's own tree with no lease in it — the subtrahend for \
                   RICHOS_PROBE_SPARE_COST.");
        return Ok(());
    }

    // ====================================================================================
    // `RICHOS_PROBE_PRIMED=1` — THE DESK IS PRIMED BEFORE THE THREAD EXISTS (CEO §55)
    // ====================================================================================
    //
    // **The shape run F could not measure, because it did not exist.** Run F opened a brand-new
    // thread and typed 500 ms into its pre-prime: the Send was accepted in 4 ms and his first
    // words still arrived at 9.647 s, 2.368 s of which was the remainder of the priming turn he
    // was queued behind (`docs/verification/first-words-2026-09-18-sendlock.md` §2).
    //
    // And the app's real path is worse than that remainder. `ui/main.js:1688` creates the thread
    // INSIDE the Send handler, so the prime and the send are back-to-back and he pays the whole
    // priming turn. This mode measures the answer: a spare front desk, spawned and primed against
    // a RESERVED thread id while no thread exists, adopted by `create_thread` when he sends.
    //
    // **The sequence is the app's, in order:** ready the spare (one model turn, before anything
    // he does) -> he presses Send, which creates the thread (`create_thread_in`) -> the timeline
    // read primes the desk (`get_timeline` -> `ready_the_front_desk`) -> he types 500 ms later,
    // exactly as in run F. If the spare worked, the third step finds nothing to do and the fourth
    // is uncontended.
    //
    // **TWO MODEL TURNS**, the same as `RICHOS_PROBE_CONTENDED` — the spare's priming turn (which
    // is the SAME turn run F spent, moved off his clock, not an extra one) and one task turn.
    //
    // `RICHOS_PROBE_SPARE_COST=<seconds>` is the other half of §6's question 3, and it spends
    // ONE model turn: ready a spare, then sample the child's RSS and CPU for that many seconds
    // and stop. What an idle primed desk costs, measured rather than estimated.
    let primed_mode = std::env::var_os("RICHOS_PROBE_PRIMED").is_some();
    let spare_cost_secs = std::env::var("RICHOS_PROBE_SPARE_COST").ok().and_then(|v| v.parse::<u64>().ok());
    if primed_mode || spare_cost_secs.is_some() {
        use richos_core::steering::TurnControl;
        let entity = EntityId::parse("depot")?;
        let control = TurnControl::open(data.join("intake.jsonl"))?;
        spine.set_turn_control(control.clone());
        spine.set_central_root(data.join("corpus"));
        spine.set_onboarding_record(data.join("onboarding.json"));
        // **BOTH SETTERS BEFORE THE SPARE, NOT AFTER.** Each calls `unprime_every_front_desk`,
        // which now reaches the spare too — priming first and configuring second would throw the
        // turn away and measure the un-primed fallback while printing the primed shape.
        spine.set_lease_factory(Box::new(ProbeLeaseFactory {
            engine: engine.clone(), data: data.clone(), runtime: runtime.clone(),
            doctrine: doctrine.clone(), skills: skills.clone(),
            bin: resolve_claude_bin(), executable: std::env::current_exe()?,
            bridge: bridge.clone(),
        }));

        // ================================================================================
        // `RICHOS_PROBE_REPAIR=1` — THE MESSAGE AFTER THE ENGINE REFRESH (Ray candidate-.11 §2.1)
        // ================================================================================
        //
        // **The premise, measured on the real window and not assumed here.** On candidate .11 the
        // Mac had no engine, so every priming hook reached `spawn_scoped` and failed; the CEO
        // pressed "Set it up", the engine installed cleanly at 23:23:40Z, and nothing re-primed.
        // His first message at 23:27:41.06Z then paid the whole priming turn: the turn did not
        // start until ~+11 s and "On it!" reached the screen at **+18.3 s**
        // (`docs/verification/2026-09-19-nightly-1.2.0-nightly.20260918.6-mac-and-android-audit.md` §2).
        //
        // This mode reproduces that sequence in order, and it costs NO extra model turn: the
        // failed attempt spawns nothing.
        //
        //   1. a factory that cannot spawn — the machine the offer is shown on;
        //   2. ask for a spare, and REQUIRE `NotReady` (a run where step 1 quietly worked would
        //      be measuring the ordinary primed shape while printing this one);
        //   3. the install: the real factory is installed, which is what `run_setup` does when
        //      it rewrites `AppState::engine_dir`;
        //   4. ask again, which is `ready_the_front_desk_after_a_repair` — and then the ordinary
        //      PRIMED sequence below, unchanged, so the two runs are comparable term by term.
        if std::env::var_os("RICHOS_PROBE_REPAIR").is_some() {
            let mut before_the_install = Spine::new(Ledger::open(&data.join("repair-probe.jsonl"))?);
            before_the_install.set_entity_registry(spine.entity_registry().clone());
            before_the_install.set_lease_factory(Box::new(NoEngineFactory));
            let refused = before_the_install.ready_a_spare_front_desk(&entity);
            match refused {
                richos_core::spine::SpareReady::NotReady(ref why) => {
                    eprintln!("---");
                    eprintln!("RICHOS_PROBE_REPAIR: before the install, the spare was refused: {why}");
                    eprintln!("  (nothing was spawned and no model turn was spent — this is the state");
                    eprintln!("   his Mac was in when the engine offer was on screen)");
                }
                ref other => {
                    return Err(format!(
                        "the pre-install attempt must be refused, and it answered {other:?} — this run                          would be the ordinary primed shape wearing the repair's name"
                    )
                    .into())
                }
            }
            eprintln!("RICHOS_PROBE_REPAIR: the install completes; the desk is asked for again, now.");
        }

        let spare_started = Instant::now();
        let spare_verdict = spine.ready_a_spare_front_desk(&entity);
        let spare_took = match spare_verdict {
            richos_core::spine::SpareReady::Ready { millis } => millis as f64 / 1000.0,
            ref other => return Err(format!("no spare front desk was made ready: {other:?}").into()),
        };
        let reserved = spine.spare_front_desk_reserved_thread()
            .ok_or("the spare reported Ready and then held no reserved thread")?.to_string();
        eprintln!("---");
        eprintln!("the spare front desk was primed in   : {spare_took:.3} s  (reserved thread {reserved})");
        eprintln!("no thread record exists yet          : {}", spine.threads().len() == 0);
        eprintln!("(the priming thread returned after {:.3} s)", spare_started.elapsed().as_secs_f64());

        // ------------------------------------------------------------------ what it costs idle
        if let Some(seconds) = spare_cost_secs {
            eprintln!("---");
            eprintln!("RICHOS_PROBE_SPARE_COST: sampling the spare's process tree for {seconds} s.");
            eprintln!("  (one model turn was spent, above; nothing below spends anything)");
            let me = std::process::id();
            let mut samples = Vec::new();
            for tick in 0..=seconds {
                if tick > 0 { std::thread::sleep(std::time::Duration::from_secs(1)); }
                if let Some((rss, cpu, procs)) = sample_tree(me) {
                    // The tree includes this probe and the ECS bridge's Python.
                    // `RICHOS_PROBE_SELF_RSS` measures exactly those with no lease in them, and
                    // the difference is the spare — arithmetic a reader can redo rather than an
                    // apportionment somebody made.
                    samples.push((tick, rss, cpu, procs));
                }
            }
            for (tick, rss, cpu, procs) in &samples {
                eprintln!("  t+{tick:>4} s : {procs} processes, {rss:>9} KB RSS total, {cpu:>6.1} % CPU (since start)");
            }
            if let (Some(first), Some(last)) = (samples.first(), samples.last()) {
                eprintln!("  RSS drift over the window : {} KB -> {} KB", first.1, last.1);
                eprintln!("  CPU at the end            : {:.1} % of one core, cumulative since each process started", last.2);
            }
            eprintln!("---");
            eprintln!("PASS: the cost of one idle primed spare is above, measured with ps and not estimated.");
            return Ok(());
        }

        // ------------------------------------------------------------------ he sends
        let text = "Land the pricing branch and get the staging deploy done.";
        // Step 1 of the app's own sequence: `create_thread_in`. The thread takes the spare's
        // reserved id — asserted rather than assumed, because if it did not, everything below
        // would be measuring a fresh desk and saying otherwise.
        let thread = spine.create_thread("First reply probe", &entity)?;
        if thread != reserved {
            return Err(format!(
                "the thread was created as {thread} but the spare was scoped to {reserved} — the                  desk below is not the one that was primed"
            ).into());
        }
        let spine = Arc::new(std::sync::Mutex::new(spine));
        // Step 2: the timeline read primes the desk, on its own thread exactly as
        // `ready_the_front_desk` does it.
        let prime_started = Instant::now();
        let priming = {
            let spine = spine.clone();
            let thread = thread.clone();
            std::thread::spawn(move || spine.lock().unwrap().prime_front_desk(&thread))
        };
        // Step 3: **he types 500 ms in** — run F's condition, unchanged, so the two runs are
        // comparable term by term.
        std::thread::sleep(std::time::Duration::from_millis(500));
        let sent = Instant::now();
        *first_words.at.lock().unwrap() = None;
        first_words.runs.lock().unwrap().clear();
        *first_words.start.lock().unwrap() = Some(sent);
        *before.start.lock().unwrap() = Some(sent);
        before.trace.lock().unwrap().clear();

        let shut_at_send = spine.try_lock().is_err();
        let asked_at = Instant::now();
        let accepted = control.defer_send(&thread, Some(entity.clone()), text)?;
        let accepted_in = asked_at.elapsed();
        let deferred = accepted.is_some();
        // A send that was NOT deferred took the ordinary road, which means the prime was already
        // over — which is the whole point. It is submitted here, as `send_message` would.
        if !deferred {
            let mut spine = spine.lock().unwrap();
            spine.switch_thread(&thread)?;
            spine.submit_prompt(text, Source::Text)?;
        }

        let verdict = priming.join().unwrap();
        let prime_took = match verdict {
            richos_core::spine::FrontDeskReady::Ready { millis, spawned } => {
                if spawned {
                    return Err("the desk was SPAWNED for his thread — the spare was not adopted,                                 and the numbers below would be a fresh desk wearing this shape".into());
                }
                millis as f64 / 1000.0
            }
            richos_core::spine::FrontDeskReady::AlreadyReady => 0.0,
            ref other => return Err(format!("the desk was not ready: {other:?}").into()),
        };
        let joined_after = prime_started.elapsed().as_secs_f64();
        let to_first_words = first_words.at.lock().unwrap().map(|at| at.duration_since(sent).as_secs_f64());
        let runs = first_words.runs.lock().unwrap().clone();

        let spine = spine.lock().unwrap();
        let binding = spine.ledger().thread_binding(&thread)?;
        let turns = spine.ledger().thread_turns_scoped(&binding)?;
        let primings = turns.iter().filter(|t| t.source == Source::Internal && t.user_text == "[re-prime]").count();
        let spare_primings = spine.ledger().actions().iter()
            .filter(|a| a.kind == "spare_front_desk_reprime").count();
        let visible: Vec<&richos_core::ledger::Turn> =
            turns.iter().copied().filter(|t| t.source == Source::Text).collect();

        eprintln!("---");
        eprintln!("PRE-PRIMED: the desk was primed before the thread existed; he opened it and typed 500 ms later.");
        eprintln!("the pre-prime he actually waited for : {prime_took:.3} s  ({verdict:?})");
        eprintln!("the priming thread returned after    : {joined_after:.3} s");
        eprintln!("the spine was shut when he pressed Send : {shut_at_send}");
        eprintln!(
            "his Send was ACCEPTED in               : {} ms  ({})",
            accepted_in.as_millis(),
            match &accepted {
                Some(r) => format!("deferred behind a prime, intake {}", r.id()),
                None => "taken straight — nothing was priming".to_string(),
            }
        );
        match to_first_words {
            Some(d) => {
                eprintln!("send -> first words                  : {d:.3} s");
                eprintln!("    of which the prime's remainder   : {:.3} s", (prime_took - 0.500).max(0.0));
                eprintln!("    of which the turn itself         : {:.3} s", d - (prime_took - 0.500).max(0.0));
            }
            None => eprintln!("send -> first words                  : NOT OBSERVED"),
        }
        eprintln!("priming turns charged to HIS thread  : {primings}");
        eprintln!("spare primings in the action ledger  : {spare_primings}");
        eprintln!("visible turns in the ledger          : {}", visible.len());
        for turn in &visible {
            eprintln!("    {:?} -> {:?}", turn.user_text, turn.assistant_text);
        }
        eprintln!("intake still pending after the turn  : {}", control.pending_intake().len());
        eprintln!("runs of prose he can see             : {}", runs.len());
        eprintln!("the turn, in arrival order:");
        for (name, at) in before.trace.lock().unwrap().iter() {
            eprintln!("  {at:>9.3} s  {name}");
        }

        let mut failures = Vec::new();
        if primings != 0 {
            failures.push(format!(
                "his thread was primed {primings} time(s) on his own clock — the spare was not adopted"
            ));
        }
        if spare_primings != 1 {
            failures.push(format!("expected exactly one spare priming turn, the ledger holds {spare_primings}"));
        }
        if visible.len() != 1 {
            failures.push(format!("expected one visible turn, got {}", visible.len()));
        }
        if !control.pending_intake().is_empty() {
            failures.push("something he said is still sitting in the intake".into());
        }
        // ============================================================================
        // WHAT IS SCORED, AND WHY IT IS NOT THE TOTAL
        // ============================================================================
        //
        // **The scored rule is that the PRIME is gone from his wait**, because that is the
        // term this slice moves and the only one it can move. It is scored at ZERO rather
        // than under a threshold: an adopted desk primes in no time at all, so there is no
        // number to pick and nothing to tune.
        //
        // **The brief's target was `send -> first words` inside run E's 5.995 s + 1 s, and it
        // is REPORTED rather than scored, because it is a target built from a different day's
        // TURN.** Three measurements of this same sentence exist on this path: run E 5.995 s,
        // run F 7.279 s, run I 7.263 s — and the last two are within 16 ms of each other, on
        // either side of every line of this slice. The turn is what the provider charges for
        // the sentence today, and run F measured it outside that target on `e2c59243`, before
        // any of this existed. Scoring a total against it would make this probe go red for
        // something no code here touches, and green only on a fast morning, which is how a
        // check stops being read.
        //
        // The sendlock record's own §2.3 already refused to conclude anything from run F
        // against run E — "one run against one run, both against a live service". This is the
        // same refusal, stated where it is enforced.
        if prime_took > 0.001 {
            failures.push(format!(
                "he waited {prime_took:.3} s for the desk on a thread whose desk was already \
                 primed — the spare was not adopted"
            ));
        }
        if shut_at_send {
            failures.push(
                "the spine was SHUT when he pressed Send — a pre-primed thread has nothing \
                 holding it, so a Send that had to take the deferred road means a prime was \
                 still running".into(),
            );
        }
        match to_first_words {
            None => failures.push("his first words never arrived".into()),
            Some(d) => {
                let prime_remainder = (prime_took - 0.500).max(0.0);
                eprintln!("---");
                eprintln!("AGAINST THE EARLIER RUNS — reported, not concluded from:");
                eprintln!("  run F (`e2c59243`, no spare) : 9.647 s send -> first words, 2.368 s of it prime");
                eprintln!("  this run                     : {d:.3} s, {prime_remainder:.3} s of it prime");
                eprintln!("  the turn alone               : run E 5.995 s, run F 7.279 s, this run {:.3} s", d - prime_remainder);
                eprintln!("  the brief's target was 6.995 s (run E + 1 s); this run is {:+.3} s against it —", d - 6.995);
                eprintln!("  a target built from run E's TURN, which run F had already measured at 7.279 s");
                eprintln!("  before any of this existed. The PRIME is what this slice removes, and it is gone.");
            }
        }
        if !failures.is_empty() {
            for why in &failures { eprintln!("FAIL: {why}"); }
            return Err(format!("{} check(s) failed", failures.len()).into());
        }
        eprintln!("---");
        eprintln!(
            "PASS: the priming turn was spent before his thread existed; his thread adopted that \
             desk, was never primed on his own clock, and his Send found nothing holding the \
             spine. His first words arrived in {:.3} s, ALL of it the turn itself.",
            to_first_words.unwrap()
        );
        return Ok(());
    }

    let thread = spine.create_thread("First reply probe", &EntityId::parse("depot")?)?;
    profile.scope_to(&spine.ledger().thread_binding(&thread)?)?;
    let state_root = profile.state.clone();
    let coordination = profile.coordination.clone();
    // **WHAT THE LEASE PINS `spawn.py`'s GUARD SURFACE TO**, taken from the profile's own
    // accessor rather than rebuilt here, so `does_it_dispatch` judges the environment the
    // product runs in. See the comment on the `.env` call in `drive_prepare`.
    let hook_sources = profile.spawn_hook_sources();

    // ====================================================================================
    // `RICHOS_PROBE_DISPATCH_ONLY=1` — THE JOIN, WITH NO MODEL AND NO PROVIDER AT ALL
    // ====================================================================================
    //
    // **The dispatch half of this probe costs nothing and therefore must be runnable for
    // nothing.** Everything below this block spends the CEO's subscription — a priming turn and
    // two visible turns — and the question "does what the register wrote dispatch?" has no model
    // in it: the register's own code writes the assignment, the engine's own `prepare` judges it.
    // So this mode calls `assignment_tools::call` directly, on the real `EcsObligations` opener
    // and the real bridge, and then runs the same `does_it_dispatch` the measured run runs.
    //
    // It was also how the dispatch section was developed and proven before a single model turn
    // was spent on it, which is the reason it exists rather than a side effect of it.
    if std::env::var_os("RICHOS_PROBE_DISPATCH_ONLY").is_some() {
        eprintln!("RICHOS_PROBE_DISPATCH_ONLY: no lease is started and no model turn is spent.");
        let session = format!("probe-session-{}", uuid::Uuid::new_v4());
        let seat = richos_core::ecs::ceo_seat(&thread).filter(|_| bridge.supports_ceo_thread_seats());
        let binding = bridge.bind("depot", &thread, &session, "probe-turn", seat.as_deref(), "ceo")?;
        let scope_path = root.0.join("assignments-dispatch-only.json");
        use sha2::Digest;
        richos_core::assignment_tools::write_scope(&scope_path, &richos_core::assignment_tools::AssignmentToolScope {
            version: 1,
            actions_allowed: true,
            state_root: state_root.clone(),
            entity_id: "depot".into(),
            thread_id: thread.clone(),
            instruction_ledger_ref: format!("ledger:{thread}:probe-turn"),
            instruction_sha256: format!("{:x}", sha2::Sha256::digest(b"dispatch-only rehearsal")),
            obligation_desk: Some(richos_core::assignment_tools::ObligationDesk {
                bridge: bridge.clone(), binding, seat,
            }),
        })?;
        // One of each kind, the same two the measured run puts in front of the model — so the
        // `commitment`/`open_loop` split is exercised here too, and both must dispatch.
        // `task` and `investigate` are two of the three kinds `AssignmentKind::parse` accepts
        // (`assignment.rs:259-261`) and they are the two the measured run produces: the task
        // opens a `commitment`, the investigate an `open_loop` (§58). Both must dispatch, and
        // the split is exactly what `EcsObligations::open` decides on.
        for (kind, assignment) in [("task", "Land the pricing branch and get the staging deploy done."),
                                   ("investigate", "Why has the nightly build been failing since Tuesday?")] {
            let answer = richos_core::assignment_tools::call(
                &scope_path,
                richos_core::assignment_tools::RECORD_TOOL_NAME,
                serde_json::json!({"assignment": assignment, "kind": kind}),
            )?;
            eprintln!("the register said : {answer}");
        }
        let recorded = richos_core::assignment::read_all(&state_root, "depot", &thread)?;
        eprintln!("assignments written: {}", recorded.len());
        let failures = does_it_dispatch(
            &bridge, &runtime, &engine, &state_root, &coordination, &root.0, &session, &repo, &recorded,
            &hook_sources,
        )?;
        if !failures.is_empty() {
            for why in &failures { eprintln!("FAIL: {why}"); }
            return Err(format!("{} dispatch check(s) failed", failures.len()).into());
        }
        eprintln!("---");
        eprintln!("PASS: every obligation the register opened came back PREPARED — a workspace \
                   and a payload, judged by the app's own user-work guards — and the old shape \
                   is refused at the same gate.");
        return Ok(());
    }

    let cognition = NativeCognition::start_with_engine(
        &resolve_claude_bin(),
        &doctrine,
        &skills,
        &std::env::current_exe()?,
        bridge.clone(),
        profile,
        None,
    )?;
    let session = cognition.session_id().to_string();
    spine.set_central_root(data.join("corpus"));
    spine.set_onboarding_record(data.join("onboarding.json"));
    spine.attach_lease(Box::new(cognition));

    // **THE SECOND SCOPE FILE, FOUND RATHER THAN CONSTRUCTED.** `start_with_engine` names it
    // `<uuid>-continuity-tools.json` under the profile's `scopes` directory and hands the path
    // to the child; the probe looks it up by that suffix so a rename shows up as this line
    // failing rather than as a watch that silently watches nothing. Exactly one lease exists
    // here, so exactly one file must match.
    let grant_path = {
        let mut found: Vec<PathBuf> = std::fs::read_dir(state_root.join("scopes"))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.to_string_lossy().ends_with("-continuity-tools.json"))
            .collect();
        match found.len() {
            1 => found.pop().unwrap(),
            other => return Err(format!(
                "expected exactly one continuity-tools scope under {}, found {other} — the file \
                 `ReaderState::he_has_now_been_spoken_to` opens is not where this probe looks",
                state_root.join("scopes").display()
            ).into()),
        }
    };

    // ================================================================================
    // `RICHOS_PROBE_CONTENDED=1` — HE TYPES WHILE THE DESK IS STILL BEING PRIMED
    // ================================================================================
    //
    // Ray's measurement 1 on candidate .10, exactly: a brand-new thread opened and typed into
    // straight away. It is a MODE and an early return rather than an extra row, because a run
    // of this probe costs the CEO's subscription real model turns and this shape costs TWO —
    // the priming turn and one task turn. `docs/verification/first-words-2026-09-18-*.md` is
    // where the runs are recorded.
    //
    // **WHAT IT PROVES, and what it deliberately refuses to claim.** It proves the Send is
    // ACCEPTED while the spine is shut, that his sentence is durable at that instant, that it
    // reaches the lease exactly once after the prime with NO second priming turn, and that the
    // intake is drained by the prime on its way out. It does NOT claim his first words arrive
    // sooner, and the arithmetic it prints is the reason: the priming is a model turn and the
    // lease is serial (continuity §3.1), so `send -> first words` is
    // `prime_remainder + turn` under this shape and under the blocking one alike. The turn-only
    // figure is what is comparable with an uncontended run, and it is the figure scored.
    if std::env::var_os("RICHOS_PROBE_CONTENDED").is_some() {
        use richos_core::steering::TurnControl;
        // The durable intake the shipping app has. Without it `defer_send` refuses rather than
        // accepting a message it cannot write down, and the probe would measure the fallback.
        let control = TurnControl::open(data.join("intake.jsonl"))?;
        spine.set_turn_control(control.clone());
        let entity = EntityId::parse("depot")?;
        let text = "Land the pricing branch and get the staging deploy done.";

        let spine = Arc::new(std::sync::Mutex::new(spine));
        let prime_started = Instant::now();
        let priming = {
            let spine = spine.clone();
            let thread = thread.clone();
            std::thread::spawn(move || spine.lock().unwrap().prime_front_desk(&thread))
        };
        // **He types 500 ms in** — the brief's own condition, and well inside a prime that
        // measured 2.927 s in run E and 4412 ms in candidate .10's `app.log`.
        std::thread::sleep(std::time::Duration::from_millis(500));
        let sent = Instant::now();
        *first_words.at.lock().unwrap() = None;
        first_words.runs.lock().unwrap().clear();
        *first_words.start.lock().unwrap() = Some(sent);
        *before.start.lock().unwrap() = Some(sent);
        before.trace.lock().unwrap().clear();

        // The spine is shut right now, and this is the assertion that says so rather than
        // assuming it — without it the rest of this block could pass on an idle spine.
        let shut_at_send = spine.try_lock().is_err();
        let asked_at = Instant::now();
        let accepted = control.defer_send(&thread, Some(entity.clone()), text)?;
        let accepted_in = asked_at.elapsed();

        let verdict = priming.join().unwrap();
        // **THE PRIME'S OWN CLOCK, FROM THE VERDICT — never `prime_started.elapsed()`.** The
        // priming thread does not return until it has also DRAINED his message, which runs his
        // whole turn, so the wall clock across the join is `prime + turn` and subtracting it
        // from anything gives nonsense. Run F's first attempt did exactly that and printed a
        // turn of -1.431 s; `FrontDeskReady::Ready.millis` is now stamped before the drain for
        // the same reason.
        let prime_took = match verdict {
            richos_core::spine::FrontDeskReady::Ready { millis, .. } => millis as f64 / 1000.0,
            ref other => return Err(format!(
                "the desk was not made ready, so nothing below is measuring what it says: {other:?}"
            ).into()),
        };
        let joined_after = prime_started.elapsed().as_secs_f64();
        let to_first_words = first_words.at.lock().unwrap().map(|at| at.duration_since(sent).as_secs_f64());
        let runs = first_words.runs.lock().unwrap().clone();

        let spine = spine.lock().unwrap();
        let binding = spine.ledger().thread_binding(&thread)?;
        let turns = spine.ledger().thread_turns_scoped(&binding)?;
        let primings = turns.iter().filter(|t| t.source == Source::Internal && t.user_text == "[re-prime]").count();
        let visible: Vec<&richos_core::ledger::Turn> =
            turns.iter().copied().filter(|t| t.source == Source::Text).collect();

        // The remainder he was actually queued behind — the seconds Ray's frames showed as
        // "Sending your message / Waiting for Rich to accept it".
        let remainder = prime_took - 0.500;
        eprintln!("---");
        eprintln!("CONTENDED: he opened a brand-new thread and typed 500 ms into its pre-prime.");
        eprintln!("the pre-prime, its own turn only     : {prime_took:.3} s  ({verdict:?})");
        eprintln!("the priming thread returned after     : {joined_after:.3} s  (prime + the turn it drained)");
        eprintln!("the spine was shut when he pressed Send : {shut_at_send}");
        eprintln!(
            "his Send was ACCEPTED in               : {} ms  ({})",
            accepted_in.as_millis(),
            match &accepted {
                Some(r) => format!("durable, intake {}", r.id()),
                None => "NOT DEFERRED — it went to the blocking path".to_string(),
            }
        );
        eprintln!("the remainder of the prime he queued behind : {remainder:.3} s");
        match to_first_words {
            Some(d) => {
                eprintln!("send -> first words                  : {d:.3} s");
                eprintln!("    of which the prime's remainder   : {remainder:.3} s");
                eprintln!("    of which the turn itself         : {:.3} s", d - remainder);
            }
            None => eprintln!("send -> first words                  : NOT OBSERVED"),
        }
        eprintln!("priming turns in the ledger          : {primings} (one, or a desk was primed twice)");
        eprintln!("visible turns in the ledger          : {}", visible.len());
        for t in &visible {
            eprintln!("    {:?} -> {:?}", t.user_text, t.assistant_text.trim());
        }
        eprintln!("intake still pending after the prime : {}", control.pending_intake().len());
        eprintln!("runs of prose he can see             : {}", runs.len());
        eprintln!("the turn, in arrival order:");
        for (label, at) in before.trace.lock().unwrap().iter() {
            eprintln!("    {at:>7.3} s  {label}");
        }

        let mut failures: Vec<String> = Vec::new();
        if !shut_at_send {
            failures.push("the spine was NOT held when he pressed Send — there was no contention to measure".into());
        }
        if accepted.is_none() {
            failures.push("his Send was not taken off the lock: `defer_send` declined while a prime was running".into());
        }
        // A SMOKE ALARM, NOT A BUDGET. What is being told apart is "one local `fsync`" from
        // "the remainder of a model turn", and those differ by an order of magnitude. The
        // shell's own `send_wait_notice` cannot fire on this path at all — it sits after
        // `state.spine.lock()`, which the deferred branch returns before reaching
        // (`src-tauri/src/main.rs`, pinned by
        // `the_send_asks_for_the_deferred_road_before_it_takes_the_spine`).
        if accepted_in.as_millis() > 250 {
            failures.push(format!(
                "his Send took {} ms to be accepted, against a prime remainder of {remainder:.3} s — \
                 that is the shape of a block, not of an fsync",
                accepted_in.as_millis()
            ));
        }
        if primings != 1 {
            failures.push(format!("{primings} priming turns — a deferred send must not prime a desk that is ready"));
        }
        if visible.len() != 1 || visible.first().map(|t| t.user_text.as_str()) != Some(text) {
            failures.push(format!("his one sentence became {} visible turn(s): {visible:?}", visible.len()));
        }
        if !control.pending_intake().is_empty() {
            failures.push("the prime left his message in the intake log for some later boundary to find".into());
        }
        // **THE TURN ITSELF is the figure that is comparable with an uncontended run**, and it
        // is scored against the lease's-first-visible-turn budget, which is what this is. The
        // send-to-first-words total is NOT scored, because it necessarily carries the prime's
        // remainder and scoring it would be scoring the CEO's typing speed.
        match to_first_words {
            Some(d) => {
                let turn_only = d - remainder;
                if turn_only > richos_core::first_reply::FIRST_TURN_BUDGET.as_secs_f64() {
                    failures.push(format!(
                        "the turn itself took {turn_only:.3} s, over the {:.0} s budget for a lease's \
                         first visible turn",
                        richos_core::first_reply::FIRST_TURN_BUDGET.as_secs_f64()
                    ));
                }
            }
            None => failures.push("no live text event ever arrived, so nothing above is a measurement".into()),
        }

        if !failures.is_empty() {
            for why in &failures {
                eprintln!("FAIL: {why}");
            }
            return Err(format!("{} check(s) failed", failures.len()).into());
        }
        eprintln!("---");
        eprintln!(
            "PASS: his Send was taken in {} ms while the spine was shut, made durable there, and \
             handed over exactly once after the prime — one priming turn, one visible turn, \
             nothing left in the intake.",
            accepted_in.as_millis()
        );
        eprintln!(
            "AND THE HONEST HALF: send -> first words was {} — {remainder:.3} s of it is the \
             prime's own remainder, which his message queues behind whether it waits on the mutex \
             or on the intake log. The lease is serial (continuity §3.1) and an already-primed \
             thread never primes twice, so this slice does not move that number and does not \
             claim to. Two model turns were spent by this run.",
            to_first_words.map(|d| format!("{d:.3} s")).unwrap_or_else(|| "not observed".into())
        );
        return Ok(());
    }

    // ================================================================================
    // THE PRIMING TURN, BEFORE HE TYPES (CEO §55)
    // ================================================================================
    //
    // **This is the ~8 s that used to be the front of his first message**, and it is a MODEL TURN
    // rather than a process start: `Spine::prime_front_desk` -> `prime_lease_if_needed` ->
    // `Cognition::reprime` -> `prompt_context_only`. The lease above is already spawned and its
    // handshake is already answered, which is exactly why the old turn 1 was still ~8 s slower
    // than turn 2. Measured here, named here, and spent BEFORE the first thing he says.
    //
    // `RICHOS_PROBE_UNPRIMED=1` skips it, which reproduces the shape every earlier run of this
    // probe measured — turn 1 paying for the priming while he waits. It is a mode rather than a
    // second run because a run costs his subscription three model turns.
    let unprimed = std::env::var_os("RICHOS_PROBE_UNPRIMED").is_some();
    let primed_in_ms = if unprimed {
        eprintln!("RICHOS_PROBE_UNPRIMED: the desk is NOT primed first — turn 1 pays for it, as it did before");
        None
    } else {
        let started = Instant::now();
        // **A PRIMING TURN IS NOT HIM, AND THIS IS WHERE THAT IS MEASURED RATHER THAN ASSERTED.**
        // `he_has_now_been_spoken_to` returns early on `context_only`, so the grant must still
        // be shut when the priming turn ends. If it opened here, the next real turn would start
        // with an already-open grant and the whole deferral would be undone by a turn he never
        // saw — silently, because the priming turn's text is discarded and never rendered.
        let watch = GrantWatch::start(&grant_path, started);
        let verdict = spine.prime_front_desk(&thread);
        let elapsed = started.elapsed().as_secs_f64();
        let (shut_before_priming, opened_during_priming) = watch.finish();
        eprintln!("---");
        eprintln!("front desk made ready before he types : {elapsed:.3} s  ({verdict:?})");
        eprintln!(
            "the continuity grant across the priming turn : shut at its start = {shut_before_priming}, \
             opened at = {opened_during_priming:?}"
        );
        if !shut_before_priming {
            return Err("the continuity grant was already open before the priming turn ran".into());
        }
        if let Some(at) = opened_during_priming {
            return Err(format!(
                "the continuity grant opened at {at:.3} s DURING the priming turn — an internal \
                 turn he never saw handed the next real turn an open grant (`context_only`)"
            ).into());
        }
        match verdict {
            richos_core::spine::FrontDeskReady::Ready { .. } => Some(elapsed),
            other => return Err(format!("the desk was not made ready, so nothing below is measuring what it says: {other:?}").into()),
        }
    };

    // **One of each kind of thing he says that ends in a register row.** A task, which §55 is
    // written about, and a question the front desk cannot answer from the conversation or from
    // the status read, which §58 inherits §55's measure. Neither is a question about how work is
    // going — that one is answered from the read and is deliberately not this probe's subject.
    let says = [
        ("task", "Land the pricing branch and get the staging deploy done."),
        ("question", "Why has the nightly build been failing since Tuesday? I want to know what's actually causing it."),
    ];

    let mut rows = Vec::new();
    for (kind, text) in says {
        *first_words.at.lock().unwrap() = None;
        first_words.runs.lock().unwrap().clear();
        before.calls.lock().unwrap().clear();
        before.trace.lock().unwrap().clear();
        *before.spoken.lock().unwrap() = false;
        let sent = Instant::now();
        *before.start.lock().unwrap() = Some(sent);
        *first_words.start.lock().unwrap() = Some(sent);
        let watch = GrantWatch::start(&grant_path, sent);
        let turn = spine.submit_prompt(text, Source::Text)?;
        let turn_ended = sent.elapsed();
        let (grant_shut_at_send, grant_opened_at) = watch.finish();
        let to_first_words = first_words.at.lock().unwrap().map(|at| at.duration_since(sent).as_secs_f64());
        let said = spine.ledger().turn(&turn).unwrap().assistant_text.trim().to_string();
        let ahead = before.calls.lock().unwrap().clone();
        let trace = before.trace.lock().unwrap().clone();
        let runs = first_words.runs.lock().unwrap().clone();

        eprintln!("---");
        eprintln!("he said    : {text}");
        eprintln!("Rich said  : {said:?}");
        match to_first_words {
            Some(d) => eprintln!("send -> first words : {d:.3} s"),
            None => eprintln!("send -> first words : NOT OBSERVED (no live text event)"),
        }
        eprintln!("send -> turn ended  : {:.3} s", turn_ended.as_secs_f64());
        if ahead.is_empty() {
            eprintln!("tool calls before he spoke : none — the time is the model's own latency");
        } else {
            eprintln!("tool calls before he spoke : {}", ahead.len());
            for (name, at) in &ahead {
                eprintln!("    {at:>7.3} s  {name}");
            }
        }
        eprintln!("runs of prose he can see : {}", runs.len());
        for (id, at, text) in &runs {
            eprintln!("    {at:>7.3} s  {text:?}  ({id})");
        }
        // **THE ORDERING THAT IS THE EVIDENCE.** His first words sit BETWEEN the register's
        // arguments and the machinery record that closes the register's row — which can only
        // happen if the app said them off that frame, as it went past.
        eprintln!("the turn, in arrival order:");
        for (label, at) in &trace {
            eprintln!("    {at:>7.3} s  {label}");
        }
        eprintln!(
            "the continuity grant : shut at the send = {grant_shut_at_send}, first seen open at = {}",
            grant_opened_at.map(|at| format!("{at:.3} s")).unwrap_or_else(|| "never".into())
        );
        rows.push((kind, said, to_first_words, ahead, runs, grant_shut_at_send, grant_opened_at));
    }

    // What the register actually holds. A right reply over a wrong record would be half the
    // feature, and the kind is what the timer and the status read use.
    let recorded = richos_core::assignment::read_all(&state_root, "depot", &thread)?;
    eprintln!("---");
    eprintln!("assignments written: {}", recorded.len());
    for row in &recorded {
        eprintln!("  kind={} state={} title={:?}", row.kind.as_str(), row.state.as_str(), row.title);
    }

    // ---- the rule ----------------------------------------------------------------------
    //
    // Score response latency and avoidable discovery/bookkeeping without prescribing
    // the order of a quick lookup and a handoff or the words Rich chooses.
    let mut failures = Vec::new();
    for (index, (kind, said, to_first_words, ahead, runs, grant_shut_at_send, grant_opened_at))
        in rows.iter().enumerate()
    {
        // **The lease's FIRST visible turn is a different measurement and gets its own budget.**
        // Measured on the same run: the model's first tool call at 11.714 s cold against 3.122 s
        // warm, all of it in front of the register. Handing the cold turn the warm budget would
        // fail the app for waking up; handing every turn the cold one would let a warm
        // regression through.
        // The lease's FIRST VISIBLE turn gets its own budget, still — it measured 7.998 s
        // against the warm turn's 6.358 s on a desk primed 2.768 s earlier (run A, 2026-09-18),
        // so the difference is real and it is no longer the priming turn. In
        // `RICHOS_PROBE_UNPRIMED=1` this same arm is what FAILS, by design: that mode puts the
        // priming turn back inside his first message, which measured 13.464 s to 15.714 s and is
        // over the 11 s this budget now is.
        let budget = if index == 0 {
            richos_core::first_reply::FIRST_TURN_BUDGET
        } else {
            richos_core::first_reply::FIRST_WORDS_BUDGET
        };
        for fault in richos_core::first_reply::first_reply_faults(ahead, *to_first_words, budget) {
            failures.push(format!("[{kind}] {fault}"));
        }
        // **SCORED, because this probe's own first run produced it.** With the checkpoint moved
        // to after the reply, the model closed its turn with a SECOND copy of the line it had
        // already said, and he read `"On it!On it!"`. Which words he is handed is §58's leg and
        // not this one's; being handed them twice is a defect this change introduced, so it is
        // caught here rather than left for him to find.
        if runs.len() > 1 {
            let texts: Vec<&str> = runs.iter().map(|(_, _, text)| text.trim()).collect();
            if texts.windows(2).any(|pair| pair[0] == pair[1] && !pair[0].is_empty()) {
                failures.push(format!("[{kind}] he was told the same thing twice: {texts:?}"));
            }
        }
        // **SCORED: NO ECS WRITE COULD HAVE PRECEDED HIS FIRST WORDS.** The rule above catches a
        // write that HAPPENED; this catches the turn where one was possible and the model simply
        // did not make it. The grant is the `richos_continuity` server's own scope file, which
        // the engine's adapter re-reads on every call (`engine/ecs/adapters/mcp.py:18-26`), so
        // while it reads false every continuity tool is refused inside the server — not at a
        // permission desk the child may never consult, which is exactly how the gate that landed
        // on 2026-09-18 came to enforce nothing (finding 1 of the previous record).
        if !grant_shut_at_send {
            failures.push(format!(
                "[{kind}] the front desk's continuity grant was already OPEN when he pressed send — \
                 an ECS write could have preceded his first words"
            ));
        }
        match (grant_opened_at, to_first_words) {
            // The observation is biased LATE by the 2 ms poll, so an open seen before his first
            // words is a real ordering fault and never a sampling artifact.
            (Some(opened), Some(spoke)) if opened < spoke => failures.push(format!(
                "[{kind}] the continuity grant opened at {opened:.3} s, {:.3} s BEFORE his first \
                 words at {spoke:.3} s — the front desk could write to ECS while he waited",
                spoke - opened
            )),
            // A grant that never opens is the other failure: the bookkeeping §55 defers is
            // bookkeeping the front desk still has to be able to do, on this turn, after the reply.
            (None, Some(spoke)) => failures.push(format!(
                "[{kind}] the continuity grant never opened, though he was spoken to at {spoke:.3} s — \
                 the front desk's own record is deferred into never"
            )),
            _ => {}
        }
        // Reported, never scored: which words he is handed is the other ruling's subject. It is
        // printed so a drift in it is visible on the same run as the timing.
        eprintln!("[{kind}] reply was {said:?}");
    }
    if recorded.is_empty() {
        failures.push("nothing was written down at all, so neither turn exercised the register".into());
    }

    failures.extend(does_it_dispatch(
        &bridge, &runtime, &engine, &state_root, &coordination, &root.0, &session, &repo, &recorded,
        &hook_sources,
    )?);

    if !failures.is_empty() {
        for why in &failures {
            eprintln!("FAIL: {why}");
        }
        return Err(format!("{} check(s) failed", failures.len()).into());
    }
    eprintln!("---");
    eprintln!("PASS: response latency and pre-reply discovery/bookkeeping checks passed.");
    let slowest = rows.iter().filter_map(|(_, _, d, _, _, _, _)| *d).fold(0.0, f64::max);
    eprintln!(
        "slowest send -> first words this run: {slowest:.3} s (budgets: {:.0} s warm, {:.0} s on the \
         lease's first VISIBLE turn; §55: a few seconds, 35 s is a defect)",
        richos_core::first_reply::FIRST_WORDS_BUDGET.as_secs_f64(),
        richos_core::first_reply::FIRST_TURN_BUDGET.as_secs_f64()
    );
    match primed_in_ms {
        Some(seconds) => eprintln!(
            "the priming turn, which every figure above is now WITHOUT: {seconds:.3} s, spent before \
             he typed. Three model turns were spent by this run."
        ),
        None => eprintln!("no priming turn was run first, so turn 1 above paid for it — the old shape."),
    }
    Ok(())
}

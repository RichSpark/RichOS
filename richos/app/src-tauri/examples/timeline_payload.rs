//! What `get_timeline` actually puts on the wire — printed from the REAL ledger, the REAL
//! machinery journal and the REAL command body.
//!
//! ## Why this exists
//!
//! The renderer in `app/ui/timeline.js` is written against a JSON shape. Every browser test
//! of it feeds that shape from `mock.js`, which means the browser tests prove the RENDERER
//! and prove nothing about whether the backend emits what the renderer reads. One
//! disagreement — `active_ms` where the renderer expects `activeMs`, a nested `base` where
//! it expects a flattened one, `snake_case` variant tags where it expects `snake_case` on a
//! `kind` field — and the timeline is blank against the real shell while every mock test
//! stays green.
//!
//! So this prints the payload from the actual chain: `Ledger` on disk -> `MachineryJournal`
//! on disk -> `Spine::timeline` -> `Timeline::view(ViewMode::Ceo)` -> `payload()`, via
//! `timeline_view::timeline_payload`, which is the same file `get_timeline` calls. Nothing
//! here re-implements the command.
//!
//! ## What it does NOT prove
//!
//! There is no live lease — no `claude` child is spawned — so a real model turn cannot run
//! and the assistant text below is scripted. What is REAL: every durable write, the shared
//! per-turn `seq` counter, the tool-id merge, the activity classification
//! (`MachineryRecord::from_native_event` is fed the exact frames the 2026-08-31 native
//! capture recorded), the visibility gate, the item ids and the JSON encoding. For a real
//! model turn use `cargo run -p richos-core --example live_events_roundtrip`, which needs
//! Claude Code installed and logged in.
//!
//! Run:
//!   cargo run --example timeline_payload            # from app/src-tauri
//!   cargo run --example timeline_payload -- --json  # payload only, for piping

#[path = "../src/timeline_view.rs"]
mod timeline_view;

use richos_core::cognition::{Cognition, CognitionError, TurnItem};
use richos_core::entity::{Entity, EntityId, EntityRegistry};
use richos_core::journal::MachineryJournal;
use richos_core::ledger::{Ledger, Source};
use richos_core::machinery::MachineryRecord;
use richos_core::spine::{Spine, WorkerEventsSource};
use serde_json::json;
use timeline_view::timeline_payload;

/// A lease that emits TEXT and MACHINERY interleaved on ONE shared `seq` counter, which is
/// the property the whole ordering model rests on (§1.4 G1).
///
/// The machinery payloads are the VERBATIM frame shapes from
/// `docs/verification/native-claude-stream-json-2026-08-31/raw/`, including the two facts
/// that matter most and are easy to get wrong from memory:
///   1. the OPENING frame carries the classification (`content_block.name`) and the CLOSING
///      `tool_result` does not — it carries no tool name at all, which is why the
///      classification has to be resolved over the RAW records before the merge;
///   2. status is a POSITION on this wire, not a field, so a call whose `tool_result` never
///      arrives has no terminal status at all — the native shape of the measured
///      status-less case.
struct ScriptedLease {
    session: String,
    script: Vec<Step>,
    fail_after: Option<usize>,
}

enum Step {
    Text(&'static str),
    Machinery(serde_json::Value),
}

impl Cognition for ScriptedLease {
    fn session_id(&self) -> &str {
        &self.session
    }



    fn prompt(&mut self, _text: &str, on_item: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        let mut seq: u64 = 0;
        for (i, step) in self.script.iter().enumerate() {
            if Some(i) == self.fail_after {
                // A POSITIVE termination signal, mid-turn — never inferred from silence.
                return Err(CognitionError::Io("compute lease died mid-turn".into()));
            }
            match step {
                Step::Text(t) => on_item(TurnItem::Text { seq, text: t }),
                Step::Machinery(v) => {
                    for rec in MachineryRecord::from_native_event(v, &self.session, seq) {
                        on_item(TurnItem::Machinery(rec));
                    }
                }
            }
            seq += 1;
        }
        Ok("end_turn".into())
    }
}

/// `queued` is OPTIONAL here on purpose, and on this wire it selects a different FRAME
/// rather than a different field — status is a position now, not a string.
///
///   * `true`  -> the streamed `content_block_start`, which is where `Pending` comes from;
///   * `false` -> the whole-message `assistant` frame, which carries the complete arguments
///                and NO status at all.
///
/// The distinction is kept because it is the one a fixture that always sends a status never
/// reproduces: a row that never reaches a terminal position must project as `unknown`, not
/// as a completion.
fn tool_open(id: &str, tool: &str, title: &str, queued: bool) -> serde_json::Value {
    if queued {
        json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
               "content_block":{"type":"tool_use","id":id,"name":tool,
                                "input":{"command":title}}}})
    } else {
        json!({"type":"assistant","message":{"role":"assistant","content":[
               {"type":"tool_use","id":id,"name":tool,"input":{"command":title}}]}})
    }
}

/// The OPENING half of a DELEGATED-WORK call: the vendor tool name `Task`.
///
/// The measured title for a Task call is the tool's own name — there is no delegated
/// objective anywhere on this wire, which is why the inspector does not show one.
fn task_open(id: &str) -> serde_json::Value {
    json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
           "content_block":{"type":"tool_use","id":id,"name":"Task","input":{}}}})
}

/// The CLOSING half: the harness's async-launch acknowledgement with an extractable
/// `agentId`. **It carries no tool name**, so on its own it is not a witness of anything —
/// the two halves have to be read together, which is exactly what
/// `timeline::resolve_agent_ids` does in two passes.
fn task_ack(id: &str, agent_id: &str) -> serde_json::Value {
    json!({"type":"user","message":{"role":"user","content":[
           {"tool_use_id":id,"type":"tool_result",
            "content":format!("Async agent launched successfully. agentId: {agent_id}")}]}})
}

/// The CLOSING frame, exactly as measured: an outcome, no tool name, and `is_error`
/// deciding the terminal position.
fn tool_close(id: &str, failed: bool) -> serde_json::Value {
    json!({"type":"user","message":{"role":"user","content":[
           {"tool_use_id":id,"type":"tool_result","is_error":failed,"content":"done"}]}})
}

fn main() {
    let json_only = std::env::args().any(|a| a == "--json");
    let dir = std::env::temp_dir().join(format!("richos-timeline-payload-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");

    let ledger = Ledger::open(dir.join("ledger.jsonl")).expect("open ledger");
    let mut spine = Spine::new(ledger);
    // A SPINE ARRIVES WITH NO COMPANIES since 2026-09-04. The registry is the person's own
    // file (`docs/entity-registry.md`) and the shell installs it at boot; this fixture
    // installs one company of its own, which is what the shell does and is one line. Before
    // today `Spine::new` carried a `const` table of the app author's six real companies, and
    // this example reached into it for `richos` — so it was, incidentally, a fixture that
    // could only run on one man's list.
    spine.set_entity_registry(
        EntityRegistry::new(vec![
            Entity::new("harbor", "Harbor Analytics", &["/Users/example/Projects/harbor"]).unwrap()
        ])
        .expect("one company is distinct from nothing"),
    );
    spine.set_machinery_journal(MachineryJournal::new(dir.join("machinery")));

    // THE ENGINE'S WORKER-LIFECYCLE STREAM, in the emitters' own format
    // (`engine/docs/worker-lifecycle-events.md`). Three workers, one in each of the three
    // states the engine can actually witness — `created`, `started`, `run_ended`. There is
    // no `completed`, `failed`, `interrupted` or `waiting` row here because no emitter can
    // write one; `parse_stream` would drop it if there were.
    //
    // The `session_id` on every row is `sess_payload_1`, the session the Task call was
    // made in. That is CLAUSE 3 of the join, and it is the whole reason this fixture proves
    // anything: a row under any other session id is refused.
    let worker_stream = dir.join("worker-events.jsonl");
    std::fs::write(
        &worker_stream,
        concat!(
            r#"{"timestamp":"2026-08-29T04:00:00+00:00","lifecycle_state":"created","source_hook":"PostToolUse[Agent]","agent_id":"agt_sage","worker_name":"Sage","agent_type":"architecture","session_id":"sess_payload_1","host_pid":4242}"#, "\n",
            r#"{"timestamp":"2026-08-29T04:00:01+00:00","lifecycle_state":"created","source_hook":"PostToolUse[Agent]","agent_id":"agt_frank","worker_name":"Frank","agent_type":"red team","session_id":"sess_payload_1","host_pid":4242}"#, "\n",
            r#"{"timestamp":"2026-08-29T04:00:02+00:00","lifecycle_state":"started","source_hook":"SubagentStart","agent_id":"agt_frank","agent_type":"red team","session_id":"sess_payload_1","host_pid":4242}"#, "\n",
            r#"{"timestamp":"2026-08-29T04:00:03+00:00","lifecycle_state":"created","source_hook":"PostToolUse[Agent]","agent_id":"agt_clark","worker_name":"Clark","agent_type":"research","session_id":"sess_payload_1","host_pid":4242}"#, "\n",
            r#"{"timestamp":"2026-08-29T04:00:04+00:00","lifecycle_state":"started","source_hook":"SubagentStart","agent_id":"agt_clark","session_id":"sess_payload_1","host_pid":4242}"#, "\n",
            r#"{"timestamp":"2026-08-29T04:03:40+00:00","lifecycle_state":"updated","source_hook":"PostToolUse[SendMessage]","agent_id":"agt_clark","session_id":"sess_payload_1","summary":"Pulled 14 sources on Claude Code memory","host_pid":4242}"#, "\n",
            r#"{"timestamp":"2026-08-29T04:07:31+00:00","lifecycle_state":"run_ended","source_hook":"SubagentStop","agent_id":"agt_clark","session_id":"sess_payload_1","host_pid":4242}"#, "\n",
            // ANOTHER SESSION, reusing agt_sage's id — the engine's own residue does exactly
            // this. Its name and its authored summary must appear nowhere below.
            r#"{"timestamp":"2026-08-29T04:00:09+00:00","lifecycle_state":"updated","source_hook":"PostToolUse[SendMessage]","agent_id":"agt_sage","worker_name":"deeply-analyst","agent_type":"deeply","session_id":"some-other-session","summary":"deeply's Q4 term sheet numbers","host_pid":4242}"#, "\n",
        ),
    )
    .expect("worker stream");
    spine.set_worker_events(WorkerEventsSource::File(worker_stream.clone()));
    let entity = EntityId::parse("harbor").expect("entity");
    let thread = spine.create_thread("Timeline payload proof", &entity).expect("thread");

    // ---- turn 1: prose, three reads, a status-less command, more prose ------------------
    spine.attach_lease(Box::new(ScriptedLease {
        session: "sess_payload_1".into(),
        fail_after: None,
        script: vec![
            Step::Text("Pulling the comparables now."),
            Step::Machinery(tool_open("tc_r1", "Read", "/abs/one.rs", true)),
            Step::Machinery(tool_close("tc_r1", false)),
            Step::Machinery(tool_open("tc_r2", "Read", "/abs/two.rs", true)),
            Step::Machinery(tool_close("tc_r2", false)),
            Step::Machinery(tool_open("tc_r3", "Read", "/abs/three.rs", true)),
            Step::Machinery(tool_close("tc_r3", false)),
            // NO STATUS ON EITHER UPDATE — the measured majority case. It must project as
            // `unknown`, never as `completed`.
            Step::Machinery(tool_open("tc_c1", "Bash", "git status --short", false)),
                        // And one that IS still open when the turn ends: `pending` => `queued`.
            Step::Machinery(tool_open("tc_c3", "Bash", "wc -l notes.md", true)),
            // THREE DELEGATIONS. Each must project as a `worker_activity` row carrying the
            // worker's name, role and last-witnessed state — never as the nameless "Worked"
            // activity row the app produced before the read path was wired.
            Step::Machinery(task_open("tc_t1")),
            Step::Machinery(task_ack("tc_t1", "agt_sage")),
            Step::Machinery(task_open("tc_t2")),
            Step::Machinery(task_ack("tc_t2", "agt_frank")),
            Step::Machinery(task_open("tc_t3")),
            Step::Machinery(task_ack("tc_t3", "agt_clark")),
            // An accounting update. Untyped vendor kind => TECHNICAL, so it must be ABSENT
            // from the payload below. Slice 3 fixed a live defect where six of these
            // rendered as CEO rows reading "Worked" against one real command.
            Step::Machinery(json!({ "sessionUpdate": "usage_update", "usage": { "inputTokens": 1200 } })),
            Step::Text("Their counter is 8% below list and inside the comparable range."),
        ],
    }));
    let t1 = spine.submit_prompt("what's the status on Acme?", Source::Text).expect("turn 1");

    // ---- turn 2: a mid-turn failure, with partial text already durable ------------------
    spine.attach_lease(Box::new(ScriptedLease {
        session: "sess_payload_2".into(),
        fail_after: Some(2),
        script: vec![
            Step::Text("Starting on the carry split —"),
            Step::Machinery(tool_open("tc_c2", "Bash", "cat partners.csv", false)),
            Step::Text("(never reached)"),
        ],
    }));
    let t2 = spine.submit_prompt("and the carry split?", Source::Text);

    let payload = timeline_payload(&spine, &thread).expect("payload");

    if json_only {
        println!("{}", serde_json::to_string_pretty(&payload).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }

    eprintln!("[payload] ledger   = {}", dir.join("ledger.jsonl").display());
    eprintln!("[payload] thread   = {thread}");
    eprintln!("[payload] turn 1   = {t1}");
    eprintln!("[payload] turn 2   = {t2:?}");
    eprintln!();

    let items = payload["items"].as_array().cloned().unwrap_or_default();
    eprintln!("[payload] mode = {}  items = {}", payload["mode"], items.len());
    eprintln!("[payload] --- one line per item, in the order the renderer receives them ---");
    for it in &items {
        let kind = it["kind"].as_str().unwrap_or("?");
        let seq = if it["sequence"].is_null() { "-".to_string() } else { it["sequence"].to_string() };
        let extra = match kind {
            "user_message" => format!("text={:?}", it["text"].as_str().unwrap_or("")),
            "rich_message" => format!("phase={} text={:?}", it["phase"], it["text"].as_str().unwrap_or("")),
            "activity" => format!("type={} state={} summary={}", it["activityType"], it["state"], it["summary"]),
            "work_duration" => format!(
                "state={} startedAt={} activeMs={} detail={}",
                it["state"], it["startedAt"], it["activeMs"], it["detail"]
            ),
            "worker_activity" => format!(
                "name={} role={} observed={} state={} update={} events={}",
                it["worker"]["workerName"],
                it["worker"]["agentType"],
                it["worker"]["observedState"],
                it["worker"]["state"],
                it["worker"]["latestUpdate"],
                it["worker"]["eventsObserved"]
            ),
            _ => String::new(),
        };
        eprintln!(
            "  {:<14} slot={:<9} seq={:<4} id={:<28} {}",
            kind,
            it["slot"].as_str().unwrap_or("?"),
            seq,
            it["id"].as_str().unwrap_or("?"),
            extra
        );
    }

    eprintln!("\n[payload] --- the assertions the renderer depends on ---");
    let mut ok = true;
    let mut say = |label: &str, cond: bool, detail: String| {
        if !cond {
            ok = false;
        }
        eprintln!("  {} {:<52} {}", if cond { "PASS" } else { "FAIL" }, label, detail);
    };

    let has = |k: &str| items.iter().any(|i| i["kind"] == k);
    say("every item is camelCase and flat (no `base`)",
        items.iter().all(|i| i.get("base").is_none() && i.get("entityId").is_some() && i.get("turnId").is_some()),
        format!("first item keys: {:?}", items.first().and_then(|i| i.as_object()).map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default()));
    say("visibility is `ceo` on every item",
        items.iter().all(|i| i["visibility"] == "ceo"),
        format!("{} items", items.len()));
    say("the CEO's prompt, Rich's prose, activity and duration all present",
        has("user_message") && has("rich_message") && has("activity") && has("work_duration"),
        format!("user_message={} rich_message={} activity={} work_duration={}",
            items.iter().filter(|i| i["kind"] == "user_message").count(),
            items.iter().filter(|i| i["kind"] == "rich_message").count(),
            items.iter().filter(|i| i["kind"] == "activity").count(),
            items.iter().filter(|i| i["kind"] == "work_duration").count()));
    say("EVERY streamed phase is `unknown`, never `final`",
        items.iter().filter(|i| i["kind"] == "rich_message").all(|i| i["phase"] == "unknown"),
        format!("{:?}", items.iter().filter(|i| i["kind"] == "rich_message").map(|i| i["phase"].clone()).collect::<Vec<_>>()));
    say("a status-less tool call is `unknown`, NOT `completed`",
        items.iter().any(|i| i["kind"] == "activity" && i["state"] == "unknown"),
        format!("{:?}", items.iter().filter(|i| i["kind"] == "activity").map(|i| (i["summary"].clone(), i["state"].clone())).collect::<Vec<_>>()));
    say("no activity row carries a `detail` (removed, not masked)",
        items.iter().filter(|i| i["kind"] == "activity").all(|i| i.get("detail").is_none()),
        "the CEO view was never handed the command text".into());
    say("no raw command or path anywhere in the payload",
        !serde_json::to_string(&payload).unwrap().contains("git status")
            && !serde_json::to_string(&payload).unwrap().contains("/abs/"),
        "searched the serialized payload for `git status` and `/abs/`".into());
    say("the accounting update produced NO CEO row",
        !items.iter().any(|i| i["kind"] == "activity" && i["summary"] == "Worked"),
        "usage_update is an untyped vendor kind => Technical => absent".into());
    let dur1 = items.iter().find(|i| i["kind"] == "work_duration" && i["turnId"] == t1.as_str());
    say("a completed turn's duration is MEASURED",
        dur1.map(|d| d["state"] == "completed" && d["activeMs"].is_number()).unwrap_or(false),
        format!("{:?}", dur1.map(|d| (d["state"].clone(), d["activeMs"].clone()))));
    // CORRECTED BY RUNNING IT. The first version asserted "an interrupted turn claims NO
    // duration" and failed with `activeMs: 0`. The assertion was wrong, not the runtime: a
    // turn that ends by a CAUGHT error still writes a terminal event, so its span IS
    // measured — here a genuine sub-millisecond one, because the lease is scripted.
    // `active_ms` is `None` only after a HARD KILL, which writes no terminal event at all
    // and therefore never records when the turn stopped (ledger.rs). The renderer already
    // draws both: a measured-but-sub-second span reads `Stopped`, an unrecorded one reads
    // `Stopped before it finished`. Two different statements, two different sentences.
    let dur2 = items.iter().find(|i| i["kind"] == "work_duration" && i["turnId"] != t1.as_str());
    say("an interrupted turn is `interrupted` and never `completed`",
        dur2.map(|d| d["state"] == "interrupted").unwrap_or(false),
        format!("{:?} — activeMs is present because the error was CAUGHT and a terminal event was written; it is absent only after a hard kill",
            dur2.map(|d| (d["state"].clone(), d.get("activeMs").cloned()))));
    say("no duration row leaks the raw stop reason",
        items.iter().filter(|i| i["kind"] == "work_duration").all(|d| d["detail"].is_null() || d.get("detail").is_none()),
        "`WorkDetail.stop_reason` is technical and is removed from a CEO view".into());
    // ---- UX §7: the delegated workers ------------------------------------------------
    let workers: Vec<&serde_json::Value> = items.iter().filter(|i| i["kind"] == "worker_activity").collect();
    say("a delegated Task call is a WORKER row, not a nameless activity row",
        workers.len() == 3,
        format!("{} worker rows: {:?}", workers.len(),
            workers.iter().map(|w| w["worker"]["workerName"].clone()).collect::<Vec<_>>()));
    say("every worker is NAMED on the CEO wire",
        workers.iter().all(|w| w["worker"]["workerName"].is_string()),
        format!("{:?}", workers.iter().map(|w| (w["worker"]["workerName"].clone(), w["worker"]["agentType"].clone())).collect::<Vec<_>>()));
    // CORRECTED BY RUNNING IT. The first version also asserted the STRING `"state":"completed"`
    // appeared nowhere in the payload, and failed — on three `Read a file` ACTIVITY rows that
    // are legitimately completed. An activity's state and a worker's state are different
    // vocabularies over the same field name, and the claim being made here is only about the
    // second one. Scoped to the worker objects, where it belongs.
    say("`run_ended` crosses the wire as `unknown`, NEVER as `completed`",
        workers.iter().filter(|w| w["worker"]["observedState"] == "run_ended").all(|w| w["worker"]["state"] == "unknown")
            && !workers.iter().any(|w| w["worker"]["state"] == "completed"),
        format!("{:?}", workers.iter().map(|w| (w["worker"]["observedState"].clone(), w["worker"]["state"].clone())).collect::<Vec<_>>()));
    say("only the three witnessable worker states appear",
        workers.iter().all(|w| ["pending_init", "running", "unknown"].contains(&w["worker"]["state"].as_str().unwrap_or(""))),
        format!("{:?}", workers.iter().map(|w| w["worker"]["state"].clone()).collect::<Vec<_>>()));
    say("no worker row carries a duration, a result_ref or a completed_at",
        workers.iter().all(|w| {
            let o = w["worker"].as_object().unwrap();
            !o.contains_key("resultRef") && !o.contains_key("errorRef") && !o.contains_key("completedAt")
                && !o.contains_key("durationMs") && !o.contains_key("elapsedMs")
        }),
        "§22: elapsed active time, output and failure reason have no witness".into());
    say("a worker row's technical half is REMOVED from the CEO view",
        workers.iter().all(|w| w.get("detail").is_none()),
        "the vendor tool title and paths were dropped, not flagged".into());
    say("CLAUSE 3: another session's row never attaches, name or summary",
        !serde_json::to_string(&payload).unwrap().contains("deeply-analyst")
            && !serde_json::to_string(&payload).unwrap().contains("Q4 term sheet"),
        format!("agt_sage is reused across sessions in the fixture; it projects as {:?}",
            workers.iter().find(|w| w["worker"]["agentId"] == "agt_sage").map(|w| (w["worker"]["workerName"].clone(), w["worker"]["observedState"].clone()))));
    say("the worker's own authored update DOES reach the CEO (§7.2 item 4)",
        workers.iter().any(|w| w["worker"]["latestUpdate"] == "Pulled 14 sources on Claude Code memory"),
        format!("{:?}", workers.iter().map(|w| w["worker"]["latestUpdate"].clone()).collect::<Vec<_>>()));

    say("the partial text that streamed before the failure survived",
        items.iter().any(|i| i["kind"] == "rich_message" && i["text"].as_str().unwrap_or("").contains("carry split")),
        format!("{:?}", items.iter().filter(|i| i["kind"] == "rich_message").map(|i| i["text"].as_str().unwrap_or("")).collect::<Vec<_>>()));

    eprintln!("\n[payload] {}", if ok { "ALL ASSERTIONS PASS" } else { "SOMETHING FAILED" });
    eprintln!("[payload] full JSON: cargo run --example timeline_payload -- --json");
    let _ = std::fs::remove_dir_all(&dir);
    if !ok {
        std::process::exit(1);
    }
}

//! What `get_machinery` actually puts on the wire — from the REAL ledger, the REAL
//! machinery journal and the REAL command body, in all FOUR of its states.
//!
//! ## Why this exists
//! `examples/timeline_payload.rs` explains the general form of the problem: the browser
//! tests feed `app/ui/` from `mock.js`, so they prove the RENDERER and prove nothing about
//! whether the backend emits what the renderer reads. Techy mode adds a second way to be
//! wrong that a mock cannot catch at all — **the four empty states**. A renderer that says
//! *"no machinery was recorded for this conversation"* over a directory the OS refused to
//! open is not a cosmetic bug; it is the product lying about its own record.
//!
//! So this runs the real chain — `Ledger` on disk -> `MachineryJournal` on disk ->
//! `Spine::timeline` -> `Timeline::view(ViewMode::Technical)` -> `payload()`, via
//! `machinery_view::machinery_payload`, the same file `get_machinery` calls — and drives it
//! into each state with the real cause of that state, not a stub. The unreadable state is a
//! real `chmod 000`.
//!
//! It also writes `app/ui/tests/fixtures/machinery-payload.json`, which the browser suite
//! `app/ui/tests/techy.js` asserts `mock.js` agrees with. That is what keeps the chain from
//! having a free end: the renderer is tested against a mock, and the mock is tested against
//! the bytes this file produced from the live types.
//!
//! ## What it does NOT prove
//! There is no live lease — no `claude` child is spawned here, so the
//! assistant text is scripted. What is REAL: every durable write, the shared per-turn `seq`,
//! the `toolCallId` merge, the visibility gate, the four states and the JSON encoding.
//!
//! Run:
//!   cargo run --example machinery_payload                    # from app/src-tauri
//!   cargo run --example machinery_payload -- --json          # payload only
//!   cargo run --example machinery_payload -- --write-fixture # refresh the browser fixture

#[path = "../src/machinery_view.rs"]
mod machinery_view;

use machinery_view::machinery_payload;
use richos_core::cognition::{Cognition, CognitionError, TurnItem};
use richos_core::entity::EntityId;
use richos_core::journal::MachineryJournal;
use richos_core::ledger::{Ledger, Source};
use richos_core::machinery::MachineryRecord;
use richos_core::spine::Spine;
use serde_json::json;

struct ScriptedLease {
    session: String,
    script: Vec<Step>,
    /// §1.5's between-turn lane: raw native frames the agent emitted with NO turn in
    /// flight. Held as wire JSON and normalized in `drain_between_turn`, so the fixture's
    /// rows come out of `MachineryRecord::from_native_between_turn` rather than out of a
    /// hand-built record.
    between: Vec<serde_json::Value>,
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
        for step in &self.script {
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
    fn drain_between_turn(&mut self) -> Vec<MachineryRecord> {
        let mut out = Vec::new();
        let mut seq = 0u64;
        for f in std::mem::take(&mut self.between).iter() {
            let records = MachineryRecord::from_native_between_turn(f, &self.session, seq);
            seq += records.len() as u64;
            out.extend(records);
        }
        out
    }
}

/// EVERY frame below is VERBATIM from
/// `docs/verification/native-claude-stream-json-2026-08-31/raw/` and
/// `docs/verification/native-claude-tool-status-2026-08-31/raw/`, copied out of the raw
/// JSONL by line, not paraphrased from the write-up. That matters because four of the
/// field-level facts the renderer depends on are only visible in the real bytes:
///
///   1. **The OPEN frame carries `input: {}`** (`run9:5`). The arguments arrive on the
///      following whole-message `assistant` frame (`run9:15`), so §1.4 G2's merge is what
///      puts what-was-run on the row at all.
///   2. **The CLOSING `tool_result` carries no tool name** (`run9:18`) — which is why the
///      activity classification has to be resolved over the RAW records before the merge,
///      and why the identity witness for a delegated call spans two frames.
///   3. **Status is a POSITION, not a field.** There is no `status` string anywhere on this
///      wire: `tool_use` opens `Pending`, `tool_progress` means `InProgress`, `tool_result`
///      closes `Completed` or `Failed` from a BOOLEAN `is_error`.
///   4. **`tool_progress`'s own `tool_use_id` is a synthetic `<real-id>-heartbeat-<n>`**
///      that matches no row anywhere (`run13:22`). The row it belongs to is
///      `parent_tool_use_id`. A consumer keying on the obvious field updates nothing,
///      silently — and this fixture would show it, because the heartbeat row would never
///      reach `running`.
mod probe {
    use serde_json::{json, Value};

    /// `run9:5` — the row OPENS on the stream, with the real tool name and `input: {}`.
    pub fn tool_open(id: &str, tool: &str) -> Value {
        json!({"type":"stream_event","event":{"type":"content_block_start","index":0,
               "content_block":{"type":"tool_use","id":id,"name":tool,"input":{},
                                "caller":{"type":"direct"}}}})
    }
    /// `run9:15` — the complete arguments, on the whole-message frame. No status.
    pub fn tool_args(id: &str, tool: &str, input: Value) -> Value {
        json!({"type":"assistant","message":{"role":"assistant","content":[
               {"type":"tool_use","id":id,"name":tool,"input":input}]}})
    }
    /// `run9:18` — the terminal frame: an outcome, `is_error`, and NO tool name.
    pub fn tool_result(id: &str, is_error: bool, output: &str) -> Value {
        json!({"type":"user","message":{"role":"user","content":[
               {"tool_use_id":id,"type":"tool_result","is_error":is_error,"content":output}]},
               "tool_use_result":Value::Null})
    }
    /// `run9:18`'s Write variant, which also carries the structured `tool_use_result` the
    /// path in `locations` is recovered from.
    pub fn write_result(id: &str, path: &str) -> Value {
        json!({"type":"user","message":{"role":"user","content":[
               {"tool_use_id":id,"type":"tool_result","is_error":false,
                "content":format!("File created successfully at: {path} (file state is current in your context \u{2014} no need to Read it back)")}]},
               "tool_use_result":{"type":"create","filePath":path,"content":"RUSTOK"}})
    }
    /// `run13:22` — THE HEARTBEAT, and its trap. Keyed on `parent_tool_use_id`; its own
    /// `tool_use_id` is synthetic and matches nothing. Measured cadence: 30.002 s.
    pub fn tool_progress(id: &str, tool: &str, elapsed: u64) -> Value {
        json!({"type":"tool_progress","tool_use_id":format!("{id}-heartbeat-0"),
               "tool_name":tool,"parent_tool_use_id":id,
               "elapsed_time_seconds":elapsed,"heartbeat":true})
    }
    /// `run9:19` — the accounting frame. Untyped, so it lands as `Unknown`; it is also the
    /// frame the watermark numerator is summed from (2 + 25_737 + 3_603 = 29_342).
    pub fn message_delta(cache_read: u64) -> Value {
        json!({"type":"stream_event","event":{"type":"message_delta",
               "delta":{"stop_reason":"tool_use"},
               "usage":{"input_tokens":2,"cache_creation_input_tokens":3603,
                        "cache_read_input_tokens":cache_read,"output_tokens":95}}})
    }
    /// `run9:1` — emitted once per turn. Big, near-static, and repeated with only a new
    /// `uuid` — which is why the SessionMeta slot compares on `machinery::meta_identity`
    /// rather than on the frame.
    pub fn system_init() -> Value {
        json!({"type":"system","subtype":"init","model":"claude-sonnet-5",
               "cwd":"/Users/alex/ab/richos",
               "tools":["Bash","Read","Write","Edit","Task"],
               "slash_commands":["compact","resume","rewind"],
               "permissionMode":"default","apiKeySource":"none",
               "session_id":"sess_mach_1","uuid":"u-init-1"})
    }
    /// `run9:2` — the other once-per-turn frame, same moment.
    pub fn system_status() -> Value {
        json!({"type":"system","subtype":"status","status":"requesting",
               "session_id":"sess_mach_1","uuid":"u-status-1"})
    }
}

fn main() {
    let json_only = std::env::args().any(|a| a == "--json");
    let write_fixture = std::env::args().any(|a| a == "--write-fixture");
    let dir = std::env::temp_dir().join(format!("richos-machinery-payload-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch dir");

    let ledger = Ledger::open(dir.join("ledger.jsonl")).expect("open ledger");
    let mut spine = Spine::new(ledger);
    spine.set_entity_registry(richos_core::entity::EntityRegistry::new(vec![
        richos_core::entity::Entity::new("richos","RichOS",&["/fixture/richos"]).unwrap(),
    ]).unwrap());
    spine.set_machinery_journal(MachineryJournal::new(dir.join("machinery")));
    let entity = EntityId::parse("richos").expect("entity");

    // ---- the thread that ran AFTER routing --------------------------------------------
    let thread = spine.create_thread("Machinery payload proof", &entity).expect("thread");
    spine.attach_lease(Box::new(ScriptedLease {
        session: "sess_mach_1".into(),
        // §1.5's between-turn traffic: what the adapter said with no turn running. It is
        // pumped below, BEFORE the turn, which is where `deliver` pumps it.
        between: vec![probe::system_init(), probe::system_status()],
        script: vec![
            Step::Text("Checking the client now."),
            // A Bash call that SUCCEEDED, exactly as run9 emitted it: open on the stream
            // with no arguments, the command on the whole-message frame, the outcome on a
            // `tool_result` that names no tool.
            Step::Machinery(probe::tool_open("toolu_A", "Bash")),
            Step::Machinery(probe::tool_args("toolu_A", "Bash", json!({"command":"cat engine/VERSION"}))),
            Step::Machinery(probe::tool_result("toolu_A", false, "1.0.0")),
            // A Bash call that FAILED. `is_error: true` is the whole difference — the other
            // terminal value the status dot has to draw.
            Step::Machinery(probe::tool_open("toolu_B", "Bash")),
            Step::Machinery(probe::tool_args(
                "toolu_B", "Bash",
                json!({"command":"cat /Users/alex/no-such-file-richos-probe-xyz.txt"}),
            )),
            Step::Machinery(probe::tool_result(
                "toolu_B", true,
                "Exit code 1\ncat: /Users/alex/no-such-file-richos-probe-xyz.txt: No such file or directory",
            )),
            // A Write, so `locations` is not empty on every row. On this wire the path is
            // recovered from the tool's own `input` and from `tool_use_result.filePath` —
            // there is no `locations` array (deviation 4).
            Step::Machinery(probe::tool_open("toolu_C", "Write")),
            Step::Machinery(probe::tool_args(
                "toolu_C", "Write",
                json!({"file_path":"/tmp/richos-native-probe-write-target.txt","content":"RUSTOK"}),
            )),
            Step::Machinery(probe::write_result("toolu_C", "/tmp/richos-native-probe-write-target.txt")),
            // A LONG call that is still RUNNING when the turn ends. Two things at once:
            // `ActivityState::Running` is reachable for the first time (the ACP path never
            // once emitted `in_progress` across five runs), and the heartbeat is keyed on
            // `parent_tool_use_id` — key it on `tool_use_id` and this row silently stays
            // `queued`.
            Step::Machinery(probe::tool_open("toolu_D", "Bash")),
            Step::Machinery(probe::tool_args(
                "toolu_D", "Bash", json!({"command":"grep -rn \"tool_use\" app | wc -l"}),
            )),
            Step::Machinery(probe::tool_progress("toolu_D", "Bash", 30)),
            // ...and a call that is still open with NO heartbeat either, which is what a
            // sub-30s tool looks like when the turn ends first. It must stay `queued`:
            // not a completion, not an error, and not a claim that it is running.
            Step::Machinery(probe::tool_open("toolu_E", "Read")),
            Step::Machinery(probe::tool_args("toolu_E", "Read", json!({"file_path":"/abs/notes.md"}))),
            // The accounting frame. Untyped vendor frame => TECHNICAL: absent from the calm
            // view (slice 3 fixed a live 6:1 noise defect there), present here as one dim
            // line carrying its own frame name (§1.4 G5).
            Step::Machinery(probe::message_delta(41991)),
            Step::Text("It does not — every non-text frame is routed, and only six are dropped."),
        ],
    }));
    // The lane, drained the way `Spine::deliver` and `get_machinery` drain it.
    let pumped = spine.pump_between_turn();
    let t1 = spine.submit_prompt("check whether the client drops tool calls", Source::Text).expect("turn 1");

    // ---- a thread that ran BEFORE routing: prose, and no machinery directory, ever -----
    let old_thread = spine.create_thread("Before the routing commit", &entity).expect("old thread");
    spine.switch_thread(&old_thread).expect("switch");
    spine.attach_lease(Box::new(ScriptedLease {
        session: "sess_mach_2".into(),
        // No between-turn traffic at all — the honest empty lane, which is a real state and
        // not a missing fixture.
        between: vec![],
        script: vec![Step::Text("Two partners pushed back on the carry split.")],
    }));
    let _ = spine.submit_prompt("how did the partner review land?", Source::Text);
    // Whatever the scripted turn wrote, remove it: a pre-`48561e4` thread's directory was
    // never created at all, and that is the state under test.
    let _ = std::fs::remove_dir_all(dir.join("machinery").join(&old_thread));

    let payload = machinery_payload(&spine, &thread).expect("payload");
    let empty = machinery_payload(&spine, &old_thread).expect("empty payload");

    if json_only {
        println!("{}", serde_json::to_string_pretty(&payload).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
        return;
    }

    if write_fixture {
        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../ui/tests/fixtures/machinery-payload.json");
        let bundle = json!({
            "_source": "app/src-tauri/examples/machinery_payload.rs — regenerate with `cargo run --example machinery_payload -- --write-fixture`",
            "recorded": payload,
            "nothingRecorded": empty,
        });
        std::fs::write(&out, serde_json::to_string_pretty(&bundle).unwrap() + "\n").expect("fixture");
        eprintln!("[machinery] wrote {}", out.display());
    }

    eprintln!("[machinery] thread = {thread}  turn = {t1}  between-turn records pumped = {pumped}");
    let items = payload["timeline"]["items"].as_array().cloned().unwrap_or_default();
    eprintln!("[machinery] state = {}  rowCount = {}  items = {}", payload["state"], payload["rowCount"], items.len());
    eprintln!("[machinery] --- one line per item, in the order the renderer receives them ---");
    for it in &items {
        let seq = if it["sequence"].is_null() { "-".into() } else { it["sequence"].to_string() };
        let extra = match it["kind"].as_str().unwrap_or("") {
            "activity" => format!(
                "type={} state={} summary={} title={} vendorKind={} locations={}",
                it["activityType"], it["state"], it["summary"],
                it["detail"]["title"], it["detail"]["vendorKind"], it["detail"]["locations"]
            ),
            "rich_message" | "user_message" => format!("text={:?}", it["text"].as_str().unwrap_or("")),
            _ => String::new(),
        };
        eprintln!("  {:<14} seq={:<4} vis={:<10} {}", it["kind"].as_str().unwrap_or("?"), seq, it["visibility"], extra);
    }

    eprintln!("\n[machinery] --- the assertions the renderer depends on ---");
    let mut ok = true;
    let mut say = |label: &str, cond: bool, detail: String| {
        if !cond {
            ok = false;
        }
        eprintln!("  {} {:<58} {}", if cond { "PASS" } else { "FAIL" }, label, detail);
    };
    let acts: Vec<&serde_json::Value> = items.iter().filter(|i| i["kind"] == "activity").collect();
    let serialized = serde_json::to_string(&payload).unwrap();

    say("mode is `technical`", payload["timeline"]["mode"] == "technical", format!("{}", payload["timeline"]["mode"]));
    say("state is `recorded` for a thread that ran after routing",
        payload["state"] == "recorded", format!("{} rows", payload["rowCount"]));
    say("no sentence is offered when there IS machinery",
        payload["sentence"].is_null(), "a state line over a full column would be noise".into());
    say("EVERY activity row carries its technical half",
        !acts.is_empty() && acts.iter().all(|a| a["detail"]["title"].is_string()),
        format!("{} activity rows", acts.len()));
    say("the G2 merge keeps the COMMAND, not the placeholder title",
        acts.iter().any(|a| a["detail"]["title"] == "cat engine/VERSION")
            && !acts.iter().any(|a| a["detail"]["title"] == "Terminal"),
        format!("titles: {:?}", acts.iter().map(|a| a["detail"]["title"].clone()).collect::<Vec<_>>()));
    say("...and the OUTCOME, which arrived on a different event than the title",
        acts.iter().any(|a| a["detail"]["title"] == "cat engine/VERSION" && a["state"] == "completed"),
        "last-write-wins per field PRESENT — a whole-record overwrite would blank one".into());
    say("a file path reaches the technical view through `locations`",
        acts.iter().any(|a| a["detail"]["locations"].as_array().map(|l| !l.is_empty()).unwrap_or(false)),
        format!("{:?}", acts.iter().map(|a| a["detail"]["locations"].clone()).collect::<Vec<_>>()));
    say("a call still open at turn end is `queued`, NEVER `completed`",
        acts.iter().any(|a| a["state"] == "queued"),
        format!("{:?}", acts.iter().map(|a| (a["detail"]["title"].clone(), a["state"].clone())).collect::<Vec<_>>()));
    // NEW ON THIS WIRE, and it is the one thing the ACP path could never show: a long tool
    // reports `running` from its own 30s heartbeat. `machinery.rs`'s own measurement of the
    // ACP path says `in_progress` NEVER appeared across five runs, so `ActivityState::Running`
    // was a branch that existed and was not taken. Keying the heartbeat on `tool_use_id`
    // instead of `parent_tool_use_id` makes this row silently fall back to `queued`.
    say("a long tool reports `running` from its 30s heartbeat",
        acts.iter().any(|a| a["state"] == "running"),
        "unreachable on the ACP path: in_progress never appeared in five runs".into());
    say("a failed tool call is `failed`",
        acts.iter().any(|a| a["state"] == "failed"), "the status dot's other terminal value".into());
    say("the untyped vendor frame IS here, with its frame name (§1.4 G5)",
        acts.iter().any(|a| a["detail"]["vendorKind"] == "stream_event:message_delta"),
        "absent from the calm view, one dim line here".into());
    say("NOTHING internal reaches even the technical view",
        items.iter().all(|i| i["visibility"] != "internal"),
        "`Visibility::renders_in` is false for Internal in EVERY mode".into());
    say("no thought row (thinking text is empty on every observed frame)",
        !acts.iter().any(|a| a["detail"]["vendorKind"] == "agent_thought_chunk"),
        "an always-empty affordance is a lie about the system".into());
    say("ordering is (turn, slot, sequence) and never the clock",
        items.windows(2).all(|w| {
            let a = w[0]["sequence"].as_u64();
            let b = w[1]["sequence"].as_u64();
            match (a, b) { (Some(x), Some(y)) => x <= y, _ => true }
        }),
        "so text and machinery interleave the way they happened".into());
    // CORRECTED BY RUNNING IT. The first version also searched for the tool's OUTPUT text
    // and failed — on `detail.summary`, which is §2.4's bounded 84-char preview and is
    // supposed to be here. "The raw blob does not travel" and "no output text travels" are
    // different claims and only the first one is true or wanted.
    say("no RAW payload travels on this command (the bounded summary does)",
        !serialized.contains("rawOutput") && !serialized.contains("rawInput") && !serialized.contains("_meta"),
        format!("summaries: {:?}", acts.iter().map(|a| a["detail"]["summary"].clone()).collect::<Vec<_>>()));

    eprintln!("\n[machinery] --- §1.5's between-turn lane ---");
    let lane = payload["timeline"]["betweenTurns"].as_array().cloned().unwrap_or_default();
    say("the frame that used to hit no sink is on the wire",
        lane.iter().any(|r| r["vendorKind"] == "system:init"),
        format!("kinds: {:?}", lane.iter().map(|r| r["vendorKind"].clone()).collect::<Vec<_>>()));
    say("every lane row is TECHNICAL, so a CEO view is handed an empty lane",
        !lane.is_empty() && lane.iter().all(|r| r["visibility"] == "technical"),
        format!("{} rows", lane.len()));
    say("no lane row carries a turn — §1.4 G4, `turn_id: None` is first-class",
        lane.iter().all(|r| r.get("turnId").is_none()),
        "attached to the thread, never to an exchange it did not belong to".into());
    say("no lane row carries a session id — that is where a rotation would show",
        !serde_json::to_string(&lane).unwrap().contains("sess_mach_1"),
        "rotation stays reconstructible in the journal and invisible on the wire".into());
    say("no sentence is offered when the lane HAS rows",
        payload["betweenTurnsSentence"].is_null(),
        "a state line over a full lane would be noise".into());

    eprintln!("\n[machinery] --- the four states, each from its real cause ---");
    say("a thread from before routing: `nothing_recorded`",
        empty["state"] == "nothing_recorded" && empty["sentence"].as_str().unwrap_or("").starts_with("No machinery was recorded"),
        format!("{}", empty["sentence"]));
    say("an empty lane says WHY it is empty, and never implies a broken feature",
        empty["betweenTurnsSentence"].as_str() == Some(machinery_view::BETWEEN_TURNS_QUIET),
        format!("{:?}", empty["betweenTurnsSentence"]));
    say("...and it still renders its conversation",
        empty["timeline"]["items"].as_array().map(|v| !v.is_empty()).unwrap_or(false),
        format!("{} items", empty["timeline"]["items"].as_array().map(|v| v.len()).unwrap_or(0)));

    // NOT RETAINED: a spine with no journal attached at all.
    let mut bare = Spine::new(Ledger::open(dir.join("bare.jsonl")).expect("bare ledger"));
    let bare_thread = bare.create_thread("No journal on this install", &entity).expect("bare thread");
    let bare_payload = machinery_payload(&bare, &bare_thread).expect("bare payload");
    say("no journal attached at all: `not_retained`",
        bare_payload["state"] == "not_retained",
        format!("{}", bare_payload["sentence"]));

    // UNREADABLE: a real chmod 000 on the thread's directory.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let thread_dir = dir.join("machinery").join(&thread);
        std::fs::set_permissions(&thread_dir, std::fs::Permissions::from_mode(0o000)).expect("chmod");
        let locked = machinery_payload(&spine, &thread).expect("locked payload");
        say("a directory the OS refuses: `unreadable`, NOT empty",
            locked["state"] == "unreadable" && locked["sentence"] != empty["sentence"],
            format!("{} / reason: {}", locked["sentence"], locked["reason"]));
        say("...and it names the path for whoever owns the fix",
            locked["reason"].as_str().unwrap_or("").contains(&thread),
            format!("{}", locked["reason"]));
        std::fs::set_permissions(&thread_dir, std::fs::Permissions::from_mode(0o755)).expect("chmod back");
    }

    eprintln!("\n[machinery] {}", if ok { "ALL ASSERTIONS PASS" } else { "SOMETHING FAILED" });
    let _ = std::fs::remove_dir_all(&dir);
    if !ok {
        std::process::exit(1);
    }
}

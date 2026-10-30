//! Scoped work records, native transcript reads and durable peer messages.
use crate::assignment::{self, Assignment, AssignmentState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

pub const SERVER_NAME: &str = "richos_status";
pub const LOOK_TOOL_NAME: &str = "background_work";
pub const QUALIFIED_LOOK_TOOL: &str = "mcp__richos_status__background_work";

const MAX_FRAME_BYTES: usize = 256 * 1024;
const MAX_SCOPE_BYTES: u64 = 16 * 1024;
/// Rows returned per section before the rest are counted rather than listed. The register
/// is per (company, conversation) and an answer he can act on is a handful of sentences,
/// not a directory listing — `work_status.rs` takes 100 for the same reason.
const MAX_ROWS: usize = 50;

/// The scope the app writes for this lease, and the only thing this server trusts.
///
/// **There is no `actions_allowed` here, and its absence is a decision.** Every other
/// app-owned scope carries the grant because every other app-owned tool WRITES: the grant
/// is what makes a write belong to a visible turn of his (`assignment_tools.rs`,
/// `onboarding_tools.rs`, `ecs.rs`). This tool changes nothing, so gating it on a rule
/// written for writes would only produce a front desk that goes blind between turns —
/// which is precisely the state finding 6 is about. What still bounds it is the scope
/// itself: no scope, no answer, and the scope names the one conversation it may read.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusToolScope {
    pub version: u32,
    /// `<data_dir>/engine-state`. The assignment register and the work receipts live here.
    pub state_root: PathBuf,
    pub entity_id: String,
    pub thread_id: String,
}

pub fn write_scope(path: &Path, scope: &StatusToolScope) -> Result<(), String> {
    if scope.version != 1 || !scope.state_root.is_absolute() {
        return Err("The app has not supplied a valid status scope. Nothing was read.".into());
    }
    let text = serde_json::to_string(scope).map_err(|e| e.to_string())?;
    crate::doctrine::write_verified(path, &text).map_err(|e| e.to_string())
}

fn read_scope(path: &Path) -> Result<StatusToolScope, String> {
    use std::io::Read;
    let file = std::fs::File::open(path)
        .map_err(|_| "RichOS has not opened this conversation for status yet.".to_string())?;
    let mut text = String::new();
    file.take(MAX_SCOPE_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|_| "The status scope could not be read. Nothing was read.".to_string())?;
    if text.len() as u64 > MAX_SCOPE_BYTES {
        return Err("The status scope is too large. Nothing was read.".into());
    }
    let scope: StatusToolScope = serde_json::from_str(&text)
        .map_err(|_| "The status scope is unreadable. Nothing was read.".to_string())?;
    if scope.version != 1 || !scope.state_root.is_absolute() {
        return Err("The status scope is not usable. Nothing was read.".into());
    }
    Ok(scope)
}

pub fn tools() -> Value {
    json!({"tools":[
        {"name":crate::system_resources::TOOL,
         "description":"Read the Mac's current CPU, RAM, memory pressure, swap and last minute of samples. Consult when current host resources matter. Reads the shared one-second sampler immediately; never starts a measurement. Check status and age_ms before relying on a reading. Observations only; this tool changes nothing.",
         "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
         "annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}},
        {"name":LOOK_TOOL_NAME,
         "description":"Read the scoped background-work record for this conversation. Lists starting, running, waiting and finished assignments, including each work kind and observed state. Registration is distinct from execution; a running state does not prove progress. This tool changes nothing and cannot start, stop, approve or retry work.",
         "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
         "annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}
        ,{"name":"sessions","description":"List recorded native Rich and Stu sessions in this company and conversation.",
          "inputSchema":{"type":"object","properties":{},"additionalProperties":false},"annotations":{"readOnlyHint":true}}
        ,{"name":"transcript","description":"Read original native session JSONL directly, including live messages, tool calls and results. No peer request or turn lock. Defaults to the latest 64 KiB; use byte offsets to page earlier content. Optional worker selects a native subagent transcript within that session.",
          "inputSchema":{"type":"object","required":["session_id"],"properties":{"session_id":{"type":"string"},"worker":{"type":"string"},"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":131072}},"additionalProperties":false},"annotations":{"readOnlyHint":true}}
        ,{"name":"send","description":"Communicate corrections, findings or reports to the other top-level agent. Use delegate to hand a new execution request to Stu. Delivery does not require an assignment and does not wait for the recipient. The sender and recipient are fixed by the host.",
          "inputSchema":{"type":"object","required":["text"],"properties":{"text":{"type":"string"}},"additionalProperties":false}}
        ,{"name":"delegate","description":"Hand this turn's original user request and your context to Stu for execution. Saves once per original user request, including retries and restart. If already_saved is true, Stu already owns it; use send for additional findings or corrections. Returns the existing operational job reference saved with the handoff. Only Rich can hand work to Stu.",
          "inputSchema":{"type":"object","required":["text"],"properties":{"text":{"type":"string"}},"additionalProperties":false}}
    ]})
}

/// Scoped work facts, including the real identifiers used by independent Stu.
fn row(item: &Assignment) -> Value {
    let notices: Vec<Value> = item
        .notices
        .iter()
        .map(|notice| json!({"say": notice.text, "already_told_him": notice.delivered_at_ms.is_some()}))
        .collect();
    let mut facts=json!({
        "assignment_id": item.id,
        "obligation_id": item.obligation_id,
        "instruction_ledger_ref": item.instruction_ledger_ref,
        "what": item.title,
        // **WORK HE ASKED FOR, OR A QUESTION HE ASKED** — the CEO's ruling §58, 2026-09-18.
        //
        // Without it the front desk reads its own question back under the heading "running"
        // and tells him his question is a job in progress. The two are different things to
        // be told about, and the difference is on the record rather than guessable from the
        // title, so it travels.
        //
        // It is the KIND HE WAS TOLD, not a re-estimate: what the model reported when it
        // answered him. The sixty-second flip to the word "investigating" happens on his
        // screen and deliberately does not rewrite this — the record keeps the estimate,
        // which is the only thing that can be compared against what actually happened.
        "kind": item.kind.as_str(),
        "state": item.reported_state().unwrap_or(AssignmentState::Interrupted).as_str(),
        "detail": item.detail,
        "asked_for_at_ms": item.registered_at_ms,
        "last_moved_at_ms": item.updated_at_ms,
        "user_stop_requested_at_ms": item.user_stop_requested_at_ms,
        "notices": notices,
    });
    if item.reported_state().is_none() {
        facts.as_object_mut().unwrap().remove("detail");
        facts["previous_record"]=json!({"state":item.state,"detail":item.detail});
    }
    facts
}

fn section(rows: &[&Assignment]) -> (Vec<Value>, usize) {
    (rows.iter().take(MAX_ROWS).map(|item| row(item)).collect(), rows.len().saturating_sub(MAX_ROWS))
}

/// The answer, shared by the protocol adapter and the tests.
pub fn call(
    scope_path: &Path,
    screen: &dyn crate::screen::ScreenSource,
    name: &str,
    arguments: Value,
) -> Result<Value, String> {
    if name == crate::system_resources::TOOL {
        if arguments != json!({}) { return Err("system_resources takes no arguments.".into()); }
        let scope = read_scope(scope_path)?;
        return serde_json::to_value(crate::system_resources::read(&scope.state_root)).map_err(|e| e.to_string());
    }
    if name == "delegate" {return crate::session_tools::delegate(scope_path, &read_scope(scope_path)?, arguments)}
    if name == "send" {return crate::session_tools::send(scope_path, &read_scope(scope_path)?, arguments)}
    if name == "transcript" || name == "sessions" {
        if name == "sessions" && arguments != json!({}) {return Err("sessions takes no arguments".into())}
        return crate::session_tools::call(&read_scope(scope_path)?,name,arguments)
    }
    if name != LOOK_TOOL_NAME {
        return Err("That status tool does not exist. Nothing was read.".into());
    }
    // An argument here could only ever be an attempt to point the read somewhere else, so
    // there are none and a supplied one is refused rather than ignored — the same rule
    // `assignment_tools.rs` keeps about extra fields, for the same reason.
    if arguments.as_object().is_some_and(|fields| !fields.is_empty()) || !arguments.is_object() {
        return Err("This tool takes no arguments: it reads this conversation's own record. Nothing was read.".into());
    }
    let scope = read_scope(scope_path)?;
    let items = assignment::read_all(&scope.state_root, &scope.entity_id, &scope.thread_id)
        .map_err(|e| format!("The background work record could not be read ({e})."))?;
    let mut starting: Vec<&Assignment> = Vec::new();
    let mut running: Vec<&Assignment> = Vec::new();
    let mut waiting: Vec<&Assignment> = Vec::new();
    let mut waiting_for_screen: Vec<&Assignment> = Vec::new();
    let mut waiting_for_quota: Vec<&Assignment> = Vec::new();
    let mut finished: Vec<&Assignment> = Vec::new();
    let mut unreported:Vec<&Assignment>=Vec::new();
    for item in &items {
        let Some(reported)=item.reported_state() else {finished.push(item);continue};
        match reported {
            AssignmentState::WaitingForScreen=>waiting_for_screen.push(item),
            AssignmentState::WaitingForQuota=>waiting_for_quota.push(item),
            AssignmentState::Blocked=>waiting.push(item),
            AssignmentState::Unknown=>unreported.push(item),
            AssignmentState::Registered|AssignmentState::Preparing=>starting.push(item),
            AssignmentState::Running=>running.push(item),
            AssignmentState::Settled|AssignmentState::Failed|AssignmentState::Interrupted=>finished.push(item),
        }
    }
    // Newest movement first in every section: the thing that just changed is the thing he
    // is most likely asking about.
    for list in [&mut starting, &mut running, &mut waiting, &mut waiting_for_screen, &mut waiting_for_quota, &mut finished, &mut unreported] {
        list.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
    }
    let (starting_rows, starting_omitted) = section(&starting);
    let (running_rows, running_omitted) = section(&running);
    let (waiting_rows, waiting_omitted) = section(&waiting);
    let (screen_rows, screen_omitted) = section(&waiting_for_screen);
    let (quota_rows, quota_omitted) = section(&waiting_for_quota);
    let (finished_rows, finished_omitted) = section(&finished);
    let (unreported_rows,unreported_omitted)=section(&unreported);
    Ok(json!({
        "as_of_ms": assignment::now_ms(),
        "open_questions": crate::questions::Store::new(&scope.state_root).list(&scope.entity_id,&scope.thread_id)?
            .into_iter().filter(|q|q.state==crate::questions::State::Open).collect::<Vec<_>>(),
        "starting": starting_rows,
        "running": running_rows,
        "waiting_for_you": waiting_rows,
        "waiting_for_the_screen": screen_rows,
        "waiting_for_allowance": quota_rows,
        "unreported":unreported_rows,"unreported_omitted":unreported_omitted,
        "finished": finished_rows,
        "omitted": starting_omitted
            + running_omitted
            + waiting_omitted
            + screen_omitted
            + quota_omitted
            + finished_omitted
            + unreported_omitted,
        "screen_wait_resumes_automatically": true,
        // **THE SCREEN AS IT IS RIGHT NOW** — the CEO's ruling §56's read half.
        //
        // A LIVE reading, and the only thing in this answer that is not read off disk. It is
        // here because the section above is a list of things that are stuck, and a list of
        // stuck things with no reason beside it is what makes a front desk guess at one. This
        // server is the app's own executable (`main.rs`'s `--status-mcp`), so the shell hands
        // it a real reader and the whole reading is one CoreGraphics call.
        //
        // **`unknown` is a third answer and never a synonym for `unlocked`** — the refusal
        // this crate makes everywhere else. `this_answer_covers` tells the model so.
        "screen": screen_reading(screen),
        // The honest boundary of this answer, in the result itself rather than in a
        // comment the model never sees — and the `starting`/`running` line is in it for the
        // same reason: the distinction is only worth having if the thing reading it is told
        // what it means.
        "this_answer_covers": "Background work in this conversation only, as recorded on disk. Other conversations are outside this result. A running state is not evidence of current progress. Starting means recorded without observed execution. Screen state is observed separately; unknown is not the same as unlocked and establishes neither locked nor unlocked. waiting_for_the_screen resumes automatically after unlock; no user response is required. `task` is requested work; `check` or `investigate` is a QUESTION delegated to Stu. These kinds describe intent, not a result.",
    }))
}

/// One live screen reading, rendered for the front desk — the CEO's ruling §56.
///
/// **No number, no timestamp and no promise about how long a lock lasts.** The three words
/// are the three answers `crate::screen::Screen` allows, and `display_asleep` is carried
/// separately because a sleeping display on an unlocked session is still a usable session —
/// this Mac's own 300-second screen-lock delay is why that is a real distinction rather than
/// an academic one.
fn screen_reading(screen: &dyn crate::screen::ScreenSource) -> Value {
    let reading = screen.read();
    json!({
        "state": reading.screen.as_str(),
        "display_asleep": reading.display_asleep,
        // Whether this reading is the kind that makes screen-bound work wait. Given as a
        // fact so the model never has to re-derive the polarity rule and get it backwards.
        "blocks_screen_bound_work": reading.blocks(),
    })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

fn response(
    scope: &Path,
    screen: &dyn crate::screen::ScreenSource,
    request: Value,
    initialized: &mut bool,
) -> Option<Value> {
    let id = request.get("id").cloned();
    let method = request.get("method").and_then(Value::as_str);
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") || method.is_none() {
        return Some(error(id.unwrap_or(Value::Null), -32600, "Invalid JSON-RPC request"));
    }
    // MCP notifications never receive a response or execute a tool.
    let id = id?;
    if !id.is_string() && !id.is_number() {
        return Some(error(Value::Null, -32600, "Invalid request id"));
    }
    let result = match method.unwrap() {
        "initialize" => {
            *initialized = true;
            let requested =
                request.pointer("/params/protocolVersion").and_then(Value::as_str).unwrap_or("");
            let protocol = match requested {
                "2024-11-05" | "2025-03-26" | "2025-06-18" | "2025-11-25" => requested,
                _ => "2025-11-25",
            };
            json!({"protocolVersion":protocol,"capabilities":{"tools":{}},"serverInfo":{"name":SERVER_NAME,"version":"1.0.0"}})
        }
        "ping" => json!({}),
        _ if !*initialized => return Some(error(id, -32002, "Initialize before reading status")),
        "tools/list" => tools(),
        "tools/call" => {
            let Some(name) = request.pointer("/params/name").and_then(Value::as_str) else {
                return Some(error(id, -32602, "A tool name is required"));
            };
            let args = request.pointer("/params/arguments").cloned().unwrap_or_else(|| json!({}));
            match call(scope, screen, name, args) {
                Ok(value) => json!({"content":[{"type":"text","text":value.to_string()}],"isError":false}),
                Err(why) => json!({"content":[{"type":"text","text":why}],"isError":true}),
            }
        }
        _ => return Some(error(id, -32601, "Method not found")),
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}

/// Newline-delimited JSON-RPC with bounded allocation. An oversized message is drained
/// through its newline, rejected, and cannot be read as the command that follows it.
pub fn serve(
    scope_path: &Path,
    screen: &dyn crate::screen::ScreenSource,
    mut reader: impl BufRead,
    mut writer: impl Write,
) -> io::Result<()> {
    let mut initialized = false;
    loop {
        let mut frame = Vec::new();
        let mut oversized = false;
        loop {
            let buf = reader.fill_buf()?;
            if buf.is_empty() {
                break;
            }
            let n = buf.iter().position(|b| *b == b'\n').map(|i| i + 1).unwrap_or(buf.len());
            let ends = buf[n - 1] == b'\n';
            if frame.len() + n <= MAX_FRAME_BYTES && !oversized {
                frame.extend_from_slice(&buf[..n]);
            } else {
                oversized = true;
            }
            reader.consume(n);
            if ends {
                break;
            }
        }
        if frame.is_empty() && !oversized {
            break;
        }
        let reply = if oversized {
            Some(error(Value::Null, -32600, "Message too large"))
        } else {
            match serde_json::from_slice::<Value>(&frame) {
                Ok(value) => response(scope_path, screen, value, &mut initialized),
                Err(_) => Some(error(Value::Null, -32700, "Invalid JSON")),
            }
        };
        if let Some(reply) = reply {
            serde_json::to_writer(&mut writer, &reply)?;
            writer.write_all(b"\n")?;
            writer.flush()?;
        }
    }
    Ok(())
}

/// Entry point used before the desktop shell initializes. Stdout is exclusively MCP frames.
pub fn run_stdio(scope_path: &Path, screen: &dyn crate::screen::ScreenSource) -> io::Result<()> {
    serve(scope_path, screen, io::stdin().lock(), io::stdout().lock())
}

/// Did the child actually load this tool? Read from its own init inventory, never from the
/// fact that the config was accepted — the same rule `assignment_tools::loaded_from_init`
/// follows, and here it matters more than anywhere else: once the front desk's work tools
/// are refused, THIS is the only thing it can see with, and a server that silently failed
/// to connect would produce a Rich who answers "nothing is running" because he cannot look.
pub fn loaded_from_init(init: &Value) -> bool {
    init.get("tools")
        .and_then(Value::as_array)
        .is_some_and(|tools| tools.iter().any(|tool| tool.as_str() == Some(QUALIFIED_LOOK_TOOL)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assignment::{AssignmentKind, Registration};
    use crate::screen::{FakeScreen, ScreenReading, UnknownScreen};

    fn fixture() -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("status-tools-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let scope_path = root.join("status.json");
        write_scope(
            &scope_path,
            &StatusToolScope {
                version: 1,
                state_root: root.join("engine-state"),
                entity_id: "depot".into(),
                thread_id: "thread-one".into(),
            },
        )
        .unwrap();
        (root, scope_path)
    }

    fn register(root: &Path, thread: &str, title: &str) -> String {
        register_kind(root, thread, title, AssignmentKind::Task)
    }

    fn register_kind(root: &Path, thread: &str, title: &str, kind: AssignmentKind) -> String {
        assignment::register_kind(
            &root.join("engine-state"),
            &Registration {
                entity_id: "depot".into(),
                thread_id: thread.into(),
                obligation_id: format!("obligation-{}", uuid::Uuid::new_v4()),
                instruction_ledger_ref: format!("ledger:{thread}:turn-7"),
                instruction_sha256: "a".repeat(64),
                title: title.into(),
                repositories: vec![],
                needs_screen: false,
            },
            kind,
        )
        .unwrap()
        .id
    }

    /// The whole of finding 6, in one assertion: the front desk can answer "what is
    /// running", "what is waiting for me" and "what finished" without calling anything
    /// that acts.
    #[test]
    fn the_front_desk_can_see_what_is_running_waiting_and_finished() {
        let (root, scope) = fixture();
        let state = root.join("engine-state");
        let running = register(&root, "thread-one", "landing the three branches");
        let waiting = register(&root, "thread-one", "the pricing review");
        let done = register(&root, "thread-one", "the nightly build");
        assignment::advance(&state, "depot", "thread-one", &running, AssignmentState::Running, "a worker start was recorded").unwrap();
        assignment::advance(&state, "depot", "thread-one", &waiting, AssignmentState::Blocked, "it is ready for you to approve the change to your repository").unwrap();
        assignment::advance(&state, "depot", "thread-one", &done, AssignmentState::Settled, "it finished").unwrap();

        let answer = call(&scope, &UnknownScreen, LOOK_TOOL_NAME, json!({})).unwrap();
        let running_rows = answer["running"].as_array().unwrap();
        assert_eq!(running_rows.len(), 1);
        assert_eq!(running_rows[0]["what"], "landing the three branches");
        let waiting_rows = answer["waiting_for_you"].as_array().unwrap();
        assert_eq!(waiting_rows.len(), 1);
        assert_eq!(waiting_rows[0]["what"], "the pricing review");
        assert!(waiting_rows[0]["detail"].as_str().unwrap().contains("approve"));
        let finished_rows = answer["finished"].as_array().unwrap();
        assert_eq!(finished_rows.len(), 1);
        assert_eq!(finished_rows[0]["what"], "the nightly build");
        assert_eq!(answer["omitted"], 0);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// **Waiting for the screen is a heading of its own, and it is NOT `waiting_for_you`** —
    /// the CEO's ruling §56 on the read side.
    ///
    /// The bucket label is what the model reads (Ray's candidate-.7 row 2, in the test below
    /// this one), so the wrong heading is the whole defect: under `waiting_for_you` the front
    /// desk would tell him he has a decision to make about a locked screen, and under
    /// `starting` it would say something is getting under way that is deliberately not.
    ///
    /// **Positive controls in the same test on purpose**, so this cannot pass on a reader that
    /// has quietly stopped classifying anything: a genuinely blocked job is still asserted
    /// under `waiting_for_you`, and a running one still under `running`.
    #[test]
    fn work_waiting_for_the_screen_has_its_own_heading_and_is_not_waiting_for_him() {
        let (root, scope) = fixture();
        let state = root.join("engine-state");
        let screen = register(&root, "thread-one", "the on-screen walk of the nightly");
        let his = register(&root, "thread-one", "the pricing review");
        let running = register(&root, "thread-one", "landing the three branches");
        assignment::advance(
            &state,
            "depot",
            "thread-one",
            &screen,
            AssignmentState::WaitingForScreen,
            crate::screen::says::detail(),
        )
        .unwrap();
        assignment::advance(&state, "depot", "thread-one", &his, AssignmentState::Blocked, "it needs your word")
            .unwrap();
        assignment::advance(&state, "depot", "thread-one", &running, AssignmentState::Running, "the back end took it")
            .unwrap();

        let answer = call(&scope, &UnknownScreen, LOOK_TOOL_NAME, json!({})).unwrap();

        let screen_rows = answer["waiting_for_the_screen"].as_array().unwrap();
        assert_eq!(screen_rows.len(), 1);
        assert_eq!(screen_rows[0]["what"], "the on-screen walk of the nightly");
        assert!(screen_rows[0]["detail"].as_str().unwrap().contains("screen is locked"));

        // It is NOT in either heading it could have been sorted into by accident.
        let his_rows = answer["waiting_for_you"].as_array().unwrap();
        assert_eq!(his_rows.len(), 1, "the screen wait was put in front of him");
        assert_eq!(his_rows[0]["what"], "the pricing review");
        assert!(answer["starting"].as_array().unwrap().is_empty());
        let running_rows = answer["running"].as_array().unwrap();
        assert_eq!(running_rows.len(), 1);
        assert_eq!(running_rows[0]["what"], "landing the three branches");
        assert!(answer["finished"].as_array().unwrap().is_empty());
        assert_eq!(answer["omitted"], 0);

        assert_eq!(answer["screen_wait_resumes_automatically"], true);
        assert!(answer.get("waiting_for_the_screen_says").is_none());
        let covers = answer["this_answer_covers"].as_str().unwrap();
        assert!(covers.contains("waiting_for_the_screen"), "{covers}");
        assert!(covers.contains("no user response is required"), "{covers}");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// **The one LIVE reading in an answer that is otherwise entirely from disk** — the CEO's
    /// ruling §56's read half, and its three answers kept apart.
    ///
    /// `unknown` is the one that matters: it is not a synonym for `unlocked`, and a front desk
    /// that collapsed them would tell him the screen is fine on a build that cannot see it.
    #[test]
    fn the_status_read_carries_the_screen_as_it_is_now_and_keeps_unknown_apart_from_unlocked() {
        let (root, scope) = fixture();

        let locked = call(&scope, &*FakeScreen::locked(), LOOK_TOOL_NAME, json!({})).unwrap();
        assert_eq!(locked["screen"]["state"], "locked");
        assert_eq!(locked["screen"]["blocks_screen_bound_work"], true);

        let unlocked = call(&scope, &*FakeScreen::unlocked(), LOOK_TOOL_NAME, json!({})).unwrap();
        assert_eq!(unlocked["screen"]["state"], "unlocked");
        assert_eq!(unlocked["screen"]["blocks_screen_bound_work"], false);
        assert_eq!(unlocked["screen"]["display_asleep"], false);

        // **A build that cannot read the screen says so, and it still does not block.**
        let unknown = call(&scope, &UnknownScreen, LOOK_TOOL_NAME, json!({})).unwrap();
        assert_eq!(unknown["screen"]["state"], "unknown");
        assert_ne!(unknown["screen"]["state"], unlocked["screen"]["state"]);
        assert_eq!(unknown["screen"]["blocks_screen_bound_work"], false);
        assert!(unknown["screen"]["display_asleep"].is_null(), "an unestablished axis is null, not false");

        // A sleeping display on an unlocked session is reported and does NOT block — this
        // Mac's 300-second screen-lock delay is why that is a real distinction.
        let asleep = ScreenReading { screen: crate::screen::Screen::Unlocked, display_asleep: Some(true) };
        let dozing = call(&scope, &*FakeScreen::new(asleep), LOOK_TOOL_NAME, json!({})).unwrap();
        assert_eq!(dozing["screen"]["display_asleep"], true);
        assert_eq!(dozing["screen"]["blocks_screen_bound_work"], false);

        // The model is told what `unknown` means, in the answer rather than in a comment.
        let covers = unknown["this_answer_covers"].as_str().unwrap();
        assert!(covers.contains("not the same as unlocked"), "{covers}");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// **A QUESTION IN FLIGHT IS TOLD APART FROM A TASK** — the CEO's ruling §58,
    /// 2026-09-18.
    ///
    /// Without the kind on the row, the front desk reads its own question back under the
    /// heading "running" and tells him a question he asked is a job in progress. The
    /// positive control is in the same test: a real task is still a task, so this cannot
    /// pass by a reader that has stopped classifying anything.
    #[test]
    fn a_question_in_flight_is_not_reported_as_a_job_that_is_running() {
        let (root, scope) = fixture();
        let state = root.join("engine-state");
        let quick = register_kind(&root, "thread-one", "whether the pricing review landed", AssignmentKind::Check);
        let deep = register_kind(&root, "thread-one", "why the nightly is red", AssignmentKind::Investigate);
        let task = register(&root, "thread-one", "landing the three branches");
        for id in [&quick, &deep, &task] {
            assignment::advance(&state, "depot", "thread-one", id, AssignmentState::Running, "The back end has started on it.")
                .unwrap();
        }
        // The screen source the landed §56 slice added: this test is about the kind on the
        // row and says nothing about the screen, so the honest source is the one that
        // establishes nothing.
        let answer = call(&scope, &UnknownScreen, LOOK_TOOL_NAME, json!({})).unwrap();
        let running = answer["running"].as_array().unwrap();
        assert_eq!(running.len(), 3);
        let kind_of = |what: &str| -> String {
            running
                .iter()
                .find(|r| r["what"] == what)
                .unwrap_or_else(|| panic!("{what} is missing from the read"))["kind"]
                .as_str()
                .unwrap()
                .to_string()
        };
        assert_eq!(kind_of("whether the pricing review landed"), "check");
        assert_eq!(kind_of("why the nightly is red"), "investigate");
        // Positive control: work is still work.
        assert_eq!(kind_of("landing the three branches"), "task");
        // The model is TOLD what the kind means, in the payload it reads rather than in a
        // tool description it may have summarized — the same rule §55's `say_nothing_else`
        // follows.
        let covers = answer["this_answer_covers"].as_str().unwrap();
        assert!(covers.contains("`check` or `investigate` is a QUESTION"), "{covers}");
        assert!(covers.contains("These kinds describe intent, not a result"), "{covers}");
        assert!(answer.to_string().contains(&quick), "the scoped work identity was lost");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// **Work that has been written down is not work that is running — Ray's candidate-.7
    /// row 2, on the read rather than on the write.**
    ///
    /// All three of `registered`, `preparing` and `running` used to share the `running` key, so
    /// a job the back end had not touched reached the front desk under the heading "running"
    /// and the front desk said so. The row always carried the honest state word; a bucket label
    /// outranks a field the model has to read twice.
    ///
    /// The positive control is in the same test deliberately: a job that really IS running is
    /// asserted to be under `running`, so this cannot pass by a reader that has simply stopped
    /// reporting anything as running.
    #[test]
    fn work_that_is_only_written_down_reads_as_starting_and_never_as_running() {
        let (root, scope) = fixture();
        let state = root.join("engine-state");
        let written = register(&root, "thread-one", "landing the three branches");
        let opening = register(&root, "thread-one", "the pricing review");
        let truly = register(&root, "thread-one", "the nightly build");
        // `registered` is left as `register` wrote it — untouched, which is the case that was
        // wrong. The other two are advanced.
        assignment::advance(&state, "depot", "thread-one", &opening, AssignmentState::Preparing, "Opening the work connection.").unwrap();
        assignment::advance(&state, "depot", "thread-one", &truly, AssignmentState::Running, "The back end has started on it.").unwrap();

        let answer = call(&scope, &UnknownScreen, LOOK_TOOL_NAME, json!({})).unwrap();
        let starting: Vec<&str> =
            answer["starting"].as_array().unwrap().iter().map(|r| r["what"].as_str().unwrap()).collect();
        assert_eq!(starting.len(), 2, "{starting:?}");
        assert!(starting.contains(&"landing the three branches"));
        assert!(starting.contains(&"the pricing review"));
        // Positive control: the one job the back end has actually taken up.
        let running = answer["running"].as_array().unwrap();
        assert_eq!(running.len(), 1, "{running:?}");
        assert_eq!(running[0]["what"], "the nightly build");
        assert_eq!(running[0]["state"], "running");
        // Nothing was dropped on the way: the two sections together are still `is_open`.
        assert_eq!(answer["omitted"], 0);
        assert!(answer["waiting_for_you"].as_array().unwrap().is_empty());
        assert!(answer["finished"].as_array().unwrap().is_empty());
        // And the model is told what the distinction means where it will actually read it.
        let covers = answer["this_answer_covers"].as_str().unwrap();
        assert!(covers.contains("These kinds describe intent, not a result"), "{covers}");
        // The assignment written down but untouched is the one Ray's walk mislabeled.
        let row = assignment::read(&state, "depot", "thread-one", &written).unwrap();
        assert_eq!(row.state, AssignmentState::Registered);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// **It answers about this conversation and no other**, and the positive control is in
    /// the same test so a passing negative cannot be a reader that returns nothing at all.
    #[test]
    fn another_conversations_work_is_not_in_this_conversations_answer() {
        let (root, scope) = fixture();
        register(&root, "thread-two", "the other conversation's job");
        let answer = call(&scope, &UnknownScreen, LOOK_TOOL_NAME, json!({})).unwrap();
        for part in ["starting", "running", "waiting_for_you", "finished"] {
            assert!(answer[part].as_array().unwrap().is_empty(), "thread-two leaked into {part}");
        }
        // Positive control: the same register, read under thread-two's own scope, has it.
        let other = root.join("status-two.json");
        write_scope(
            &other,
            &StatusToolScope {
                version: 1,
                state_root: root.join("engine-state"),
                entity_id: "depot".into(),
                thread_id: "thread-two".into(),
            },
        )
        .unwrap();
        let theirs = call(&other, &UnknownScreen, LOOK_TOOL_NAME, json!({})).unwrap();
        // `starting`, not `running`: it was registered and never advanced, and those are now
        // two different answers — see the bucket comment in `call`.
        assert_eq!(theirs["starting"].as_array().unwrap()[0]["what"], "the other conversation's job");
        assert!(theirs["running"].as_array().unwrap().is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// **It cannot be pointed anywhere, and it cannot be made to act.** Every redirection
    /// field and every verb is refused outright rather than ignored.
    #[test]
    fn work_read_takes_no_arguments_and_peer_tools_keep_their_own_contracts() {
        let (root, scope) = fixture();
        for sneaky in [
            json!({"thread_id": "thread-two"}),
            json!({"entity_id": "other"}),
            json!({"state_root": "/tmp"}),
        ] {
            assert!(call(&scope, &UnknownScreen, LOOK_TOOL_NAME, sneaky).is_err());
        }
        for verb in ["stop", "approve", "decline", "retry", "prepare", "integrate", "record"] {
            assert!(call(&scope, &UnknownScreen, verb, json!({})).is_err(), "{verb} was answered by the status server");
        }
        // Positive control: the plain call answers.
        assert!(call(&scope, &UnknownScreen, LOOK_TOOL_NAME, json!({})).is_ok());
        // Direct session reads are read-only; peer send has a separate host-bound contract.
        let listed = tools();
        let listed = listed["tools"].as_array().unwrap();
        assert_eq!(listed.len(), 6);
        assert!(listed[..4].iter().all(|t|t["annotations"]["readOnlyHint"]==true));
        assert_eq!(listed[4]["name"],"send");
        assert_eq!(listed[5]["name"],"delegate");
        assert!(call(&scope,&UnknownScreen,"delegate",json!({"text":"hello","from":"stu"})).is_err());
        assert!(call(&scope,&UnknownScreen,"send",json!({"text":"hello","from":"stu"})).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resource_read_needs_a_scope_but_no_mutation_grant_and_refuses_arguments() {
        let (root,scope)=fixture();
        let value=call(&scope,&UnknownScreen,crate::system_resources::TOOL,json!({})).unwrap();
        assert_eq!(value["status"],"unavailable");
        assert!(value["current"].is_null());
        assert!(call(&scope,&UnknownScreen,crate::system_resources::TOOL,json!({"state_root":"elsewhere"})).is_err());
        std::fs::remove_file(&scope).unwrap();
        assert!(call(&scope,&UnknownScreen,crate::system_resources::TOOL,json!({})).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A result he has already been told is marked as such, so the front desk can answer
    /// "what finished" without telling him the same thing twice as though it were new —
    /// and an undelivered one is still visible, which is the half of finding 6 about a
    /// result that reached his screen while Rich never heard of it.
    #[test]
    fn a_result_says_whether_he_has_already_been_told() {
        let (root, scope) = fixture();
        let state = root.join("engine-state");
        let id = register(&root, "thread-one", "the pricing review");
        assignment::raise_notice(
            &state,
            "depot",
            "thread-one",
            &id,
            assignment::NoticeKind::Settled,
            &assignment::says::settled("the pricing review", "It landed on cc/pricing in depot."),
        )
        .unwrap();
        let before = call(&scope, &UnknownScreen, LOOK_TOOL_NAME, json!({})).unwrap();
        // The row is under `starting`: registered, never advanced. A notice is visible wherever
        // its assignment sits, which is the half of finding 6 this test is about.
        let notices = before["starting"][0]["notices"].as_array().unwrap();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0]["already_told_him"], false);
        assert!(notices[0]["say"].as_str().unwrap().contains("the pricing review"));
        // Delivery marks it, and the same read then says so.
        assignment::take_pending_notices(&state, "depot", "thread-one").unwrap();
        let after = call(&scope, &UnknownScreen, LOOK_TOOL_NAME, json!({})).unwrap();
        assert_eq!(after["starting"][0]["notices"][0]["already_told_him"], true);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn work_facts_expose_real_scoped_ids_for_independent_stu() {
        let (root, scope) = fixture();
        let id = register(&root, "thread-one", "landing the three branches");
        let rows = assignment::read_all(&root.join("engine-state"), "depot", "thread-one").unwrap();
        let answer = call(&scope, &UnknownScreen, LOOK_TOOL_NAME, json!({})).unwrap();
        assert_eq!(answer["starting"][0]["assignment_id"],id);
        assert_eq!(answer["starting"][0]["obligation_id"],rows[0].obligation_id);
        assert_eq!(answer["starting"][0]["instruction_ledger_ref"],rows[0].instruction_ledger_ref);
        assert_eq!(answer["starting"][0]["what"],"landing the three branches");
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A missing or damaged scope answers with a refusal, never with an empty record —
    /// "there is nothing running" and "I could not look" are different sentences and the
    /// front desk must never say the first when it means the second.
    #[test]
    fn a_missing_or_damaged_scope_refuses_rather_than_reporting_an_empty_record() {
        let root = std::env::temp_dir().join(format!("status-tools-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let absent = root.join("nothing.json");
        let refused = call(&absent, &UnknownScreen, LOOK_TOOL_NAME, json!({})).unwrap_err();
        assert!(refused.contains("Nothing was read.") || refused.contains("has not opened"));
        let damaged = root.join("damaged.json");
        std::fs::write(&damaged, "{\"version\":").unwrap();
        assert!(call(&damaged, &UnknownScreen, LOOK_TOOL_NAME, json!({})).is_err());
        let relative = root.join("relative.json");
        std::fs::write(
            &relative,
            json!({"version":1,"state_root":"engine-state","entity_id":"depot","thread_id":"thread-one"})
                .to_string(),
        )
        .unwrap();
        assert!(call(&relative, &UnknownScreen, LOOK_TOOL_NAME, json!({})).is_err());
        // Positive control: a good scope over an EMPTY register answers, and says empty.
        let good = root.join("good.json");
        write_scope(
            &good,
            &StatusToolScope {
                version: 1,
                state_root: root.join("engine-state"),
                entity_id: "depot".into(),
                thread_id: "thread-one".into(),
            },
        )
        .unwrap();
        let answer = call(&good, &UnknownScreen, LOOK_TOOL_NAME, json!({})).unwrap();
        assert!(answer["running"].as_array().unwrap().is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }

    /// The readiness verdict is a fact from the child, not from the config being accepted.
    #[test]
    fn the_status_tool_is_only_loaded_when_the_child_says_it_is() {
        assert!(loaded_from_init(&json!({"tools":[QUALIFIED_LOOK_TOOL]})));
        assert!(!loaded_from_init(&json!({"tools":["mcp__richos_assignments__record"]})));
        assert!(!loaded_from_init(&json!({})));
        assert!(!loaded_from_init(&json!({"tools":"background_work"})));
    }

    /// The transport: initialize gates everything, an oversized frame cannot be read as
    /// the command behind it, and a tool call reaches `call`.
    #[test]
    fn the_status_stdio_transport_gates_on_initialize_and_bounds_a_frame() {
        let (root, scope) = fixture();
        register(&root, "thread-one", "landing the three branches");
        let mut input = String::new();
        input.push_str(&json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}).to_string());
        input.push('\n');
        input.push_str(&json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{}}).to_string());
        input.push('\n');
        input.push_str(&json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":LOOK_TOOL_NAME,"arguments":{}}}).to_string());
        input.push('\n');
        input.push_str(&format!("{}\n", "x".repeat(MAX_FRAME_BYTES + 10)));
        input.push_str(&json!({"jsonrpc":"2.0","id":4,"method":"ping"}).to_string());
        input.push('\n');
        let mut out = Vec::new();
        serve(&scope, &UnknownScreen, io::Cursor::new(input.into_bytes()), &mut out).unwrap();
        let replies: Vec<Value> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(replies[0]["error"]["code"], -32002, "tools/list ran before initialize");
        assert_eq!(replies[1]["result"]["serverInfo"]["name"], SERVER_NAME);
        assert_eq!(replies[2]["result"]["isError"], false);
        assert!(replies[2]["result"]["content"][0]["text"].as_str().unwrap().contains("landing the three branches"));
        assert_eq!(replies[3]["error"]["code"], -32600);
        assert_eq!(replies[4]["id"], 4);
        std::fs::remove_dir_all(root).unwrap();
    }
}

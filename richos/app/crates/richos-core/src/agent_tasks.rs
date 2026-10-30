//! THE TASK BOOK: which of the provider's own background tasks is which, and how each last
//! stood, read off the CLI's stream one frame at a time; and the `stop_task` frame that stops
//! one of them by its task id.
//!
//! The regular app's Stop of one worker and its worker status run on this (`native.rs`:
//! `ReaderState::agent_tasks`, `stop_worker`, `stop_background_command`). It was written for the
//! operator client (operator back-end spec r4 §1.1; the frame shapes are the ones recorded by
//! operator probe P12, richos-hq `docs/verification/2026-09-24-operator-probes/`) and moved here
//! unchanged when that client was retired (ruling §137: one app for everyone).
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

/// Stop one task, by the task id the stream gave it (r3 (d) item 2; P12).
pub fn stop_task_request(request_id: &str, task_id: &str) -> Value {
    json!({"type": "control_request", "request_id": request_id,
           "request": {"subtype": "stop_task", "task_id": task_id}})
}

/// Move one foreground command to the background, by the tool call that started it, without
/// interrupting it: Ctrl+B for that one call. Measured on Claude Code 2.1.314 in print mode
/// on an async helper's foreground `Bash` call: answered `{"backgrounded": true}` at once,
/// the helper's call returned "Command was manually backgrounded by user", and the same
/// process ran on and finished with its own notice (richos-hq 2026-10-29-two-day-cleanup).
pub fn background_task_request(request_id: &str, tool_use_id: &str) -> Value {
    json!({"type": "control_request", "request_id": request_id,
           "request": {"subtype": "background_tasks", "tool_use_id": tool_use_id}})
}

// =============================================================================================
// the task book: which named agent is which task, and how it last stood (r4 §1.1)
// =============================================================================================

/// An agent's last status, from the stream's own words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskStatus {
    Running,
    Completed,
    Failed,
    Stopped,
    Killed,
}

impl TaskStatus {
    fn parse(word: &str) -> Option<Self> {
        match word {
            "running" | "started" | "in_progress" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "stopped" => Some(Self::Stopped),
            "killed" => Some(Self::Killed),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
            Self::Killed => "killed",
        }
    }
    pub fn is_running(self) -> bool {
        self == Self::Running
    }
}

/// One named agent of the lead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTask {
    pub name: String,
    pub task_id: String,
    pub tool_use_id: String,
    pub status: TaskStatus,
}

/// Which named agent is which task, and its last status. Pure: fed one frame at a time.
///
/// **Only agents started by an `Agent`/`Task` tool call with a `name` are named.** A Bash
/// background task has a task id too (`task_type: local_bash`, P12), and it is never a name
/// he can stop. An agent started with no name cannot be named in a stop either, so it is kept
/// out of the name map (the host still counts it through `agent-liveness.sh`, (m)).
#[derive(Clone, Debug, Default)]
pub struct TaskBook {
    /// `Agent` tool_use id → its optional name (empty for an unnamed worker).
    names: HashMap<String, String>,
    /// task id → tool_use id, for tasks whose `task_started` arrived.
    task_tool: HashMap<String, String>,
    /// Status seen for a task id before its name was known.
    early_status: HashMap<String, TaskStatus>,
    /// Native agents in start order. A named agent started twice keeps both rows.
    agents: Vec<AgentTask>,
    /// `Bash` tool_use ids the session's own model called (`parent_tool_use_id` null), never a
    /// helper's: the commands a job's Stop may end on this session's behalf. → the call's own
    /// `description`, where Stu names the job the command is for (stu.md), and when the call
    /// was read.
    own_bash: HashMap<String, (String, u64)>,
    /// Tool calls whose result has come back: one with no task by then never had a command
    /// left to stop.
    answered: HashSet<String>,
    /// task id → when its `task_started` was read (ms since the epoch, `assignment::now_ms`).
    started_at_ms: HashMap<String, u64>,
    /// A helper's `Bash` calls still running in the foreground, from the stream's own words
    /// (`task_started`: `local_bash`, `owned_by_subagent`, not `is_backgrounded`): tool_use id
    /// → (task id, the helper's agent id, which is the task's `parent_task_id`, and the
    /// frame's `session_id`). Gone once the host says the task moved to the background or
    /// ended. [`Self::held_foreground`].
    helper_foreground: HashMap<String, (String, String, String)>,
}

impl TaskBook {
    /// Read one frame. Returns the named agents whose row this frame created or changed.
    pub fn observe(&mut self, frame: &Value) -> Vec<AgentTask> {
        let mut changed = Vec::new();
        match (frame.get("type").and_then(Value::as_str), frame.get("subtype").and_then(Value::as_str)) {
            (Some("assistant"), _) => {
                let blocks = frame.pointer("/message/content").and_then(Value::as_array).cloned().unwrap_or_default();
                let own = frame.get("parent_tool_use_id").is_none_or(Value::is_null);
                for block in blocks {
                    if own && block.get("type").and_then(Value::as_str) == Some("tool_use")
                        && block.get("name").and_then(Value::as_str) == Some("Bash") {
                        if let Some(id) = block.get("id").and_then(Value::as_str).filter(|id| !id.is_empty()) {
                            let description = block.pointer("/input/description").and_then(Value::as_str).unwrap_or("");
                            self.own_bash.entry(id.to_string())
                                .or_insert_with(|| (description.to_string(), crate::assignment::now_ms()));
                        }
                    }
                    let is_agent = block.get("type").and_then(Value::as_str) == Some("tool_use")
                        && matches!(block.get("name").and_then(Value::as_str), Some("Agent" | "Task"));
                    let name = block.pointer("/input/name").and_then(Value::as_str).map(str::trim).unwrap_or("");
                    let id = block.get("id").and_then(Value::as_str).unwrap_or("");
                    if is_agent && !id.is_empty() && !self.names.contains_key(id) {
                        self.names.insert(id.to_string(), name.to_string());
                        // task_started may have arrived first; bind it now.
                        let bound: Vec<String> = self.task_tool.iter().filter(|(_, tool)| tool.as_str() == id)
                            .map(|(task, _)| task.clone()).collect();
                        for task in bound {
                            changed.extend(self.bind(&task, id));
                        }
                    }
                }
            }
            (Some("user"), _) => {
                for block in frame.pointer("/message/content").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) {
                    if block.get("type").and_then(Value::as_str) == Some("tool_result") {
                        if let Some(id) = block.get("tool_use_id").and_then(Value::as_str) {
                            if self.own_bash.contains_key(id) { self.answered.insert(id.to_string()); }
                        }
                    }
                }
            }
            (Some("system"), Some("task_started")) => {
                let task = frame.get("task_id").and_then(Value::as_str).unwrap_or("");
                let tool = frame.get("tool_use_id").and_then(Value::as_str).unwrap_or("");
                if !task.is_empty() {
                    self.started_at_ms.entry(task.to_string()).or_insert_with(crate::assignment::now_ms);
                    if !tool.is_empty() {
                        self.task_tool.insert(task.to_string(), tool.to_string());
                    }
                    let helper = frame.get("parent_task_id").and_then(Value::as_str).unwrap_or("");
                    if frame.get("task_type").and_then(Value::as_str) == Some("local_bash")
                        && frame.get("owned_by_subagent").and_then(Value::as_bool) == Some(true)
                        && frame.get("is_backgrounded").and_then(Value::as_bool) == Some(false)
                        && !tool.is_empty() && !helper.is_empty() {
                        let session = frame.get("session_id").and_then(Value::as_str).unwrap_or("");
                        self.helper_foreground.insert(tool.to_string(), (task.to_string(), helper.to_string(), session.to_string()));
                    }
                    if frame.get("task_type").and_then(Value::as_str) == Some("local_agent") || self.names.contains_key(tool) {
                        changed.extend(self.bind(task, tool));
                    }
                }
            }
            (Some("system"), Some("task_notification")) => {
                let task = frame.get("task_id").and_then(Value::as_str).unwrap_or("");
                if let Some(status) = frame.get("status").and_then(Value::as_str).and_then(TaskStatus::parse) {
                    changed.extend(self.set_status(task, status));
                }
            }
            (Some("system"), Some("task_updated")) => {
                let task = frame.get("task_id").and_then(Value::as_str).unwrap_or("");
                if frame.pointer("/patch/is_backgrounded").and_then(Value::as_bool) == Some(true) {
                    self.helper_foreground.retain(|_, (t, _, _)| t != task);
                }
                if let Some(status) = frame.pointer("/patch/status").and_then(Value::as_str).and_then(TaskStatus::parse) {
                    changed.extend(self.set_status(task, status));
                }
            }
            _ => {}
        }
        changed
    }

    fn bind(&mut self, task: &str, tool: &str) -> Option<AgentTask> {
        if let Some(row) = self.agents.iter_mut().find(|a| a.task_id == task) {
            // The assistant tool frame can arrive after task_started.
            if row.name.is_empty() {
                row.name = self.names.get(tool).cloned().unwrap_or_default();
                row.tool_use_id = tool.to_string();
                if !row.name.is_empty() { return Some(row.clone()); }
            }
            return None;
        }
        let name = self.names.get(tool).cloned().unwrap_or_default();
        let status = self.early_status.remove(task).unwrap_or(TaskStatus::Running);
        let row = AgentTask { name, task_id: task.to_string(), tool_use_id: tool.to_string(), status };
        self.agents.push(row.clone());
        Some(row)
    }

    fn set_status(&mut self, task: &str, status: TaskStatus) -> Option<AgentTask> {
        if task.is_empty() {
            return None;
        }
        match self.agents.iter_mut().find(|a| a.task_id == task) {
            Some(row) if row.status == status => None,
            // `killed` arrives beside `stopped` for one stop (P12, same batch): the later of the
            // two does not undo the first's meaning, and `stopped` is the one he asked for.
            Some(row) if row.status == TaskStatus::Stopped && status == TaskStatus::Killed => None,
            Some(row) => {
                row.status = status;
                Some(row.clone())
            }
            None => {
                self.early_status.insert(task.to_string(), status);
                None
            }
        }
    }

    /// Native task identity for one tool call, including calls without a model-supplied name.
    pub fn task_for_tool(&self, tool: &str) -> Option<String> {
        let mut tasks = self.task_tool.iter().filter(|(_, id)| id.as_str() == tool).map(|(task, _)| task);
        let task = tasks.next()?.clone();
        if tasks.next().is_some() { return None; }
        Some(task)
    }

    /// The session's own shell commands still running, foreground or background, oldest first:
    /// A helper's commands are its own, not these. A call whose `task_started` has not come
    /// yet (a foreground one's comes seconds after the call) is listed with no task id, from
    /// the moment the call was read, until its task or its result arrives.
    pub fn own_running_commands(&self) -> Vec<crate::steering::OwnCommand> {
        let mut out: Vec<crate::steering::OwnCommand> = self.task_tool.iter()
            .filter(|(task, _)| !self.task_ended(task))
            .filter_map(|(task, tool)| Some(crate::steering::OwnCommand {
                task_id: task.clone(),
                tool_use_id: tool.clone(),
                started_at_ms: *self.started_at_ms.get(task)?,
                description: self.own_bash.get(tool)?.0.clone(),
            }))
            .collect();
        let has_task: HashSet<&String> = self.task_tool.values().collect();
        out.extend(self.own_bash.iter()
            .filter(|(tool, _)| !has_task.contains(tool) && !self.answered.contains(*tool))
            .map(|(tool, (description, at))| crate::steering::OwnCommand {
                task_id: String::new(), tool_use_id: tool.clone(), started_at_ms: *at, description: description.clone() }));
        out.sort_by(|a, b| (a.started_at_ms, &a.task_id).cmp(&(b.started_at_ms, &b.task_id)));
        out
    }

    /// **The foreground calls of the helpers a pause holds, each handed out once.** A pause
    /// freezes a helper's commands at once (`agent_hold.py`), but its WAIT message reaches it
    /// only when its running call returns: measured on Claude Code 2.1.314, a lead's message
    /// does not move a helper's foreground call, which in the app now runs as in the terminal
    /// (engine `docs/foreground-bash.md`). `held(session id, agent id)` says whether that
    /// helper is held (the session id is empty when the frame named none);
    /// the tool_use ids returned are moved to the background by the caller
    /// ([`background_task_request`]) and forgotten here, so each is asked for once.
    pub fn held_foreground(&mut self, held: impl Fn(&str, &str) -> bool) -> Vec<String> {
        let ended: Vec<String> = self.helper_foreground.iter()
            .filter(|(_, (task, _, _))| self.task_ended(task)).map(|(tool, _)| tool.clone()).collect();
        for tool in ended { self.helper_foreground.remove(&tool); }
        let mut out: Vec<String> = self.helper_foreground.iter()
            .filter(|(_, (_, helper, session))| held(session, helper)).map(|(tool, _)| tool.clone()).collect();
        out.sort();
        for tool in &out { self.helper_foreground.remove(tool); }
        out
    }

    pub fn task_ended(&self, task: &str) -> bool {
        self.early_status.get(task).is_some_and(|status| !status.is_running())
            || self.agents.iter().any(|row| row.task_id == task && !row.status.is_running())
    }

    /// The latest row for `name`, which is the one a stop means.
    pub fn latest(&self, name: &str) -> Option<&AgentTask> {
        self.agents.iter().rev().find(|a| a.name == name)
    }

    /// Every named agent, latest row per name, in start order.
    pub fn agents(&self) -> Vec<AgentTask> {
        let mut seen = HashSet::new();
        let mut out: Vec<AgentTask> = self.agents.iter().rev().filter(|a| !a.name.is_empty() && seen.insert(a.name.clone())).cloned().collect();
        out.reverse();
        out
    }

    /// Names whose latest row is still running.
    pub fn running(&self) -> Vec<String> {
        self.agents().into_iter().filter(|a| a.status.is_running()).map(|a| a.name).collect()
    }

    /// All running native workers, including unnamed workers and tasks without a tool id.
    pub fn running_tasks(&self) -> Vec<AgentTask> {
        self.agents.iter().filter(|a| a.status.is_running()).cloned().collect()
    }

    /// Project native task facts into the existing desktop worker payload.
    pub fn worker_status(&self) -> crate::worker_status::WorkerStatusView {
        crate::worker_status::WorkerStatusView {
            active: self.agents.iter().filter(|a| a.status.is_running()).count(),
            items: self.agents.iter().map(|task| crate::worker_status::WorkerItem {
                label: if task.name.is_empty() {task.task_id.clone()} else {task.name.clone()},
                state: match task.status {
                    TaskStatus::Running=>"active", TaskStatus::Completed=>"done",
                    other=>other.as_str(),
                }.into(),
                agent_id: Some(task.task_id.clone()),
            }).collect(),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- the task book, from the recorded shapes (P12) -------------------------------------

    fn agent_use(id: &str, name: &str) -> Value {
        json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":id,"name":"Agent",
               "input":{"name":name,"description":"d","prompt":"p","run_in_background":true}}]}})
    }
    fn started(task: &str, tool: &str) -> Value {
        json!({"type":"system","subtype":"task_started","task_id":task,"tool_use_id":tool})
    }
    fn notified(task: &str, status: &str) -> Value {
        json!({"type":"system","subtype":"task_notification","task_id":task,"status":status})
    }

    #[test]
    fn a_named_agent_is_mapped_to_its_task_id_and_bash_tasks_never_are() {
        let mut book = TaskBook::default();
        book.observe(&agent_use("toolu_a", "mark-sonnet-x1"));
        let changed = book.observe(&started("a79a88c2", "toolu_a"));
        assert_eq!(changed[0].name, "mark-sonnet-x1");
        assert_eq!(book.latest("mark-sonnet-x1").unwrap().task_id, "a79a88c2");
        // A background Bash task (task_type local_bash) has a task id and no name.
        book.observe(&started("b5zdytibr", "toolu_bash"));
        assert_eq!(book.agents().len(), 1);
        assert_eq!(book.running(), ["mark-sonnet-x1"]);
    }

    /// The frame shape recorded on Claude Code 2.1.314 for an async helper's foreground `Bash`.
    fn helper_bash(task: &str, tool: &str, helper: &str, backgrounded: bool) -> Value {
        json!({"type":"system","subtype":"task_started","task_id":task,"tool_use_id":tool,
               "owned_by_subagent":true,"is_backgrounded":backgrounded,"parent_task_id":helper,
               "task_type":"local_bash","description":"d","session_id":"s1"})
    }

    #[test]
    fn a_held_helpers_foreground_calls_are_handed_out_once_and_nobody_elses() {
        let mut book = TaskBook::default();
        book.observe(&helper_bash("b1", "toolu_fg1", "aheld", false));
        book.observe(&helper_bash("b2", "toolu_fg2", "aheld", false));
        book.observe(&helper_bash("b3", "toolu_other", "afree", false));
        book.observe(&helper_bash("b4", "toolu_bg", "aheld", true));        // already background
        book.observe(&helper_bash("b5", "toolu_done", "aheld", false));
        book.observe(&notified("b5", "completed"));                            // already ended
        book.observe(&helper_bash("b6", "toolu_moved", "aheld", false));
        book.observe(&json!({"type":"system","subtype":"task_updated","task_id":"b6","patch":{"is_backgrounded":true}}));
        // The lead's own foreground Bash (no owned_by_subagent) is never a helper's.
        book.observe(&json!({"type":"system","subtype":"task_started","task_id":"b7","tool_use_id":"toolu_lead",
                             "is_backgrounded":false,"task_type":"local_bash"}));
        assert!(book.held_foreground(|_, _| false).is_empty(), "nothing is held: nothing is moved");
        assert!(book.held_foreground(|session, agent| session == "s2" && agent == "aheld").is_empty(),
                "a hold in another session is not this one");
        assert_eq!(book.held_foreground(|session, agent| session == "s1" && agent == "aheld"), ["toolu_fg1", "toolu_fg2"]);
        assert!(book.held_foreground(|_, agent| agent == "aheld").is_empty(), "each call is asked for once");
        assert_eq!(book.held_foreground(|_, _| true), ["toolu_other"]);
    }

    #[test]
    fn the_background_request_names_the_one_tool_call() {
        assert_eq!(background_task_request("r1", "toolu_x"),
                   json!({"type":"control_request","request_id":"r1",
                          "request":{"subtype":"background_tasks","tool_use_id":"toolu_x"}}));
    }

    #[test]
    fn task_started_before_its_tool_use_is_bound_when_the_name_arrives() {
        let mut book = TaskBook::default();
        assert!(book.observe(&started("t-1", "toolu_b")).is_empty());
        assert!(book.observe(&notified("t-1", "completed")).is_empty());
        let changed = book.observe(&agent_use("toolu_b", "ray-opus-w1"));
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].status, TaskStatus::Completed, "the status seen early is kept");
    }

    #[test]
    fn statuses_follow_the_stream_and_killed_does_not_undo_stopped() {
        let mut book = TaskBook::default();
        book.observe(&agent_use("toolu_a", "a"));
        book.observe(&started("t-a", "toolu_a"));
        book.observe(&notified("t-a", "stopped"));
        book.observe(&json!({"type":"system","subtype":"task_updated","task_id":"t-a","patch":{"status":"killed"}}));
        assert_eq!(book.latest("a").unwrap().status, TaskStatus::Stopped, "P12: killed and stopped arrive together");
        book.observe(&agent_use("toolu_b", "b"));
        book.observe(&started("t-b", "toolu_b"));
        book.observe(&json!({"type":"system","subtype":"task_updated","task_id":"t-b","patch":{"status":"killed"}}));
        assert_eq!(book.latest("b").unwrap().status, TaskStatus::Killed);
        assert!(book.running().is_empty());
    }

    #[test]
    fn a_name_started_again_is_the_latest_row() {
        let mut book = TaskBook::default();
        book.observe(&agent_use("toolu_1", "same"));
        book.observe(&started("t-1", "toolu_1"));
        book.observe(&notified("t-1", "completed"));
        book.observe(&agent_use("toolu_2", "same"));
        book.observe(&started("t-2", "toolu_2"));
        assert_eq!(book.latest("same").unwrap().task_id, "t-2");
        assert_eq!(book.agents().len(), 1);
        assert_eq!(book.running(), ["same"]);
    }
}

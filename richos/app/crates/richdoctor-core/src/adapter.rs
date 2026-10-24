//! **THE RESTRICTED MULTI-TURN CLAUDE CODE ADAPTER** (PRD §4, the slice-1 feasibility point).
//!
//! # THE BASELINE AND THE DELTA
//!
//! The argument baseline is [`richos_core::bug_report::writer_args`], measured with claude
//! 2.1.295 on 2026-10-09 (one stream-json turn, no tools, no settings, no session left on disk,
//! no MCP servers). PRD §9: it is "the sole argument baseline". The delta this file adds, and
//! the reason for each:
//!
//! | Added | Why |
//! |---|---|
//! | `--mcp-config` names RichDoctor's own support server (instead of `{"mcpServers":{}}`) | the two read-only tools, and nothing else, under `--strict-mcp-config` from the baseline |
//! | `--allowedTools` with the two exact `mcp__richdoctor__…` names | pre-approves exactly them |
//! | `--permission-mode dontAsk` | anything not pre-approved is denied, never asked |
//! | `--permission-prompts none` | nobody answers a prompt: it is denied automatically |
//! | `--restricted` | removes the code-running built-ins and WebFetch, ignores user/project/local settings files; managed policy still applies (PRD §4: never bypassed) |
//! | `--settings {"autoMemoryEnabled":false}` | no auto-memory folder is created under `~/.claude/projects/` |
//! | `--disable-slash-commands` | no skills or commands |
//! | `--append-system-prompt` the bundled [`PLAYBOOK`] | appended to the baseline system guidance (PRD §4) |
//!
//! The process runs in a RichDoctor-owned empty folder ([`Session::start`]'s `cwd`), with an
//! environment built from nothing ([`environment`]): `CLAUDE_CONFIG_DIR` can never reach it, so
//! the CLI's default configuration and default sign-in are the ones used (PRD §4), and no
//! RichOS account list is read.
//!
//! # MEASURED, 2026-10-22, claude 2.1.307 (the installed CLI on the build Mac)
//!
//! One process, two user turns on standard input, in a folder holding a `CLAUDE.md` ("end every
//! reply with PINEAPPLE-MARKER") and an `AGENTS.md` ("start every reply with MANGO-MARKER"):
//!
//!   - the `system/init` line listed exactly `mcp__richdoctor__collect_diagnostics` and
//!     `mcp__richdoctor__read_evidence` as tools, `permissionMode: dontAsk`, no slash commands,
//!     no skills, one MCP server (`richdoctor`, connected) and only Claude Code's three built-in
//!     plugins (`path: builtin`);
//!   - turn 1 called both tools and answered from them (8.1 s); turn 2 asked it to `cat
//!     ~/.ssh/id_rsa`, read `/etc/hosts`, write a file and fetch a web page: none happened (no
//!     file written, no denial needed because no such tool existed), and it named only the two
//!     tools;
//!   - neither marker appeared: no `CLAUDE.md` or `AGENTS.md` was loaded;
//!   - nothing was written under `~/.claude/projects/` (without `autoMemoryEnabled: false` an
//!     empty `memory/` folder was created there; with it, nothing);
//!   - the default sign-in was used: both turns succeeded with no `CLAUDE_CONFIG_DIR`.
//!
//! The record of that measurement, and of its rerun through this file's own code, is
//! `richos/app/crates/richdoctor-core/MEASURED.md`.
//!
//! # NOT TRUSTED: THE CLI'S DEFAULTS
//!
//! PRD §4: "Current CLI defaults are not a security contract." So every session's own
//! `system/init` line is checked ([`check_init`]) the moment it arrives, after the first message
//! and before any model answer can lead to a tool call ([`Session::begin`]): a tool, MCP
//! server, plugin, skill or permission mode other than the measured ones ends the session and
//! RichDoctor continues without AI ([`Failure::Unsupported`]). It never weakens the isolation to
//! make an incompatible CLI work.
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The MCP server's name in the CLI's configuration.
pub const SERVER_NAME: &str = "richdoctor";
/// The two tools, by their bare names.
pub const TOOL_COLLECT: &str = "collect_diagnostics";
pub const TOOL_READ: &str = "read_evidence";
/// The two exact tool names the CLI sees, and the only ones allowed (PRD §4).
pub const ALLOWED: [&str; 2] = ["mcp__richdoctor__collect_diagnostics", "mcp__richdoctor__read_evidence"];

/// The bundled, versioned support playbook (PRD §4, §11: it ships with the signed app).
pub const PLAYBOOK: &str = include_str!("playbook.md");

/// One investigation operation: one message to Claude and its answer (PRD §4).
pub const OPERATION_BOUND: Duration = Duration::from_secs(90);

/// Diagnostic tool calls the host allows per operation (PRD §4). Enforced by the support
/// server ([`crate::mcp`]), not by the CLI.
pub const CALLS_PER_OPERATION: u32 = 8;

/// The message that asks Claude for the report itself (the playbook's second shape).
pub const PREPARE_REPORT: &str = "PREPARE_REPORT";

/// The command-line flag that runs RichDoctor's own executable as the support server.
pub const SERVER_FLAG: &str = "--support-adapter";

/// Where the support server is and which draft it serves.
#[derive(Debug, Clone)]
pub struct Spec {
    /// RichDoctor's own executable, run as `<exe> --support-adapter <draft dir>`.
    pub server_exe: PathBuf,
    /// The draft's private working folder ([`crate::store`]).
    pub draft_dir: PathBuf,
}

/// The `--mcp-config` value: RichDoctor's support server and nothing else.
pub fn mcp_config(spec: &Spec) -> String {
    serde_json::json!({ "mcpServers": { SERVER_NAME: {
        "type": "stdio",
        "command": spec.server_exe.to_string_lossy(),
        "args": [SERVER_FLAG, spec.draft_dir.to_string_lossy()],
    }}})
    .to_string()
}

/// The full argument list: [`writer_args`](richos_core::bug_report::writer_args) with its empty
/// MCP configuration replaced by [`mcp_config`], then the delta in this file's table.
pub fn args(spec: &Spec) -> Vec<String> {
    let mut out = richos_core::bug_report::writer_args();
    let at = out.iter().position(|a| a == "--mcp-config").expect("writer_args names --mcp-config");
    out[at + 1] = mcp_config(spec);
    out.extend(
        [
            "--allowedTools",
            &ALLOWED.join(","),
            "--permission-mode",
            "dontAsk",
            "--permission-prompts",
            "none",
            "--restricted",
            "--settings",
            "{\"autoMemoryEnabled\":false}",
            "--disable-slash-commands",
            "--append-system-prompt",
            PLAYBOOK,
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    out
}

/// The environment the CLI runs with, built from nothing: who and where the user is, a plain
/// `PATH`, and the network settings a Mac behind a proxy needs to reach Anthropic. Nothing
/// RichOS sets (`CLAUDE_CONFIG_DIR` above all) and nothing of the CLI's own nesting markers
/// (`CLAUDECODE`) can reach it, because nothing is inherited that is not named here.
pub fn environment(from: impl Fn(&str) -> Option<String>) -> Vec<(String, String)> {
    const KEEP: [&str; 15] = [
        "HOME", "USER", "LOGNAME", "LANG", "LC_ALL", "TMPDIR", "HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy", "NO_PROXY", "no_proxy",
        "ALL_PROXY", "SSL_CERT_FILE", "NODE_EXTRA_CA_CERTS",
    ];
    let mut out: Vec<(String, String)> = KEEP.iter().filter_map(|k| from(k).filter(|v| !v.is_empty()).map(|v| (k.to_string(), v))).collect();
    out.push(("PATH".into(), "/usr/bin:/bin:/usr/sbin:/sbin".into()));
    out
}

// ---------------------------------------------------------------------------------------
// THE STREAM
// ---------------------------------------------------------------------------------------

/// One line of the CLI's stream-json output, as RichDoctor reads it.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    /// The `system/init` line.
    Init(Value),
    /// Words Claude wrote in an assistant message.
    Text(String),
    /// Claude called a tool.
    ToolUse { name: String, input: Value },
    /// A turn ended. `is_error` with the CLI's own text when it failed.
    Result { is_error: bool, text: String, subtype: String },
    /// The CLI reported a rate limit state.
    RateLimit(Value),
    /// Anything else (tool results echoed back, partial messages, hooks): ignored.
    Other,
}

/// Read one stream-json line. Raw provider replies stay inside this function: only the fields
/// above leave it (PRD §4: "exclude raw provider replies").
pub fn parse_line(line: &str) -> StreamEvent {
    let Ok(v) = serde_json::from_str::<Value>(line.trim()) else { return StreamEvent::Other };
    match (v["type"].as_str(), v["subtype"].as_str()) {
        (Some("system"), Some("init")) => StreamEvent::Init(v),
        (Some("result"), _) => StreamEvent::Result {
            is_error: v["is_error"].as_bool().unwrap_or(false) || v["subtype"].as_str().is_some_and(|s| s != "success"),
            text: v["result"].as_str().unwrap_or("").to_string(),
            subtype: v["subtype"].as_str().unwrap_or("").to_string(),
        },
        (Some("rate_limit_event"), _) => StreamEvent::RateLimit(v["rate_limit_info"].clone()),
        (Some("assistant"), _) => {
            let content = v["message"]["content"].as_array().cloned().unwrap_or_default();
            for block in &content {
                if block["type"] == "tool_use" {
                    return StreamEvent::ToolUse { name: block["name"].as_str().unwrap_or("").to_string(), input: block["input"].clone() };
                }
            }
            let text: String = content.iter().filter(|b| b["type"] == "text").filter_map(|b| b["text"].as_str()).collect();
            if text.is_empty() { StreamEvent::Other } else { StreamEvent::Text(text) }
        }
        _ => StreamEvent::Other,
    }
}

/// **THE SESSION'S OWN INVENTORY, CHECKED BEFORE ANYTHING IS SENT.** Exactly the two tools,
/// exactly the one MCP server and it connected, the `dontAsk` permission mode, no slash
/// commands or skills, and no plugin but Claude Code's own built-in ones. Anything else is
/// refused with what differed, and the session ends ([`Failure::Unsupported`]).
pub fn check_init(init: &Value) -> Result<(), String> {
    let strings = |key: &str| -> Vec<String> { init[key].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default() };
    let mut tools = strings("tools");
    tools.sort();
    let mut want: Vec<String> = ALLOWED.iter().map(|s| s.to_string()).collect();
    want.sort();
    if tools != want {
        return Err(format!("tools were {tools:?}"));
    }
    if init["permissionMode"].as_str() != Some("dontAsk") {
        return Err(format!("permission mode was {}", init["permissionMode"]));
    }
    let servers = init["mcp_servers"].as_array().cloned().unwrap_or_default();
    if servers.len() != 1 || servers[0]["name"] != SERVER_NAME || servers[0]["status"] != "connected" {
        return Err(format!("MCP servers were {}", Value::Array(servers)));
    }
    if !strings("slash_commands").is_empty() {
        return Err("slash commands were available".into());
    }
    if init["skills"].as_array().is_some_and(|a| !a.is_empty()) {
        return Err("skills were available".into());
    }
    if let Some(plugins) = init["plugins"].as_array() {
        if let Some(p) = plugins.iter().find(|p| p["path"] != "builtin") {
            return Err(format!("a plugin that is not built in was loaded: {}", p["name"]));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------
// WHAT FAILED, IN THE WORDS THE PERSON SEES
// ---------------------------------------------------------------------------------------

/// Why Claude Code could not help, as far as was observed (PRD §4, §8; round 39's AI-failure
/// card). Only a confirmed negative is "signed out"; anything unclear is "could not check".
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Failure {
    /// The CLI was not found on this Mac.
    NotInstalled,
    /// Claude Code said, in so many words, that the default sign-in cannot be used.
    SignedOut,
    /// RichDoctor could not find out whether Claude Code is signed in.
    CouldNotCheck,
    /// No answer within [`OPERATION_BOUND`].
    Timeout,
    /// The provider said to wait (a rate limit). Not proof the subscription is used up.
    RateLimited,
    /// The account is restricted (a disabled organization, a permission error).
    Restricted,
    /// The service failed or was overloaded.
    Service,
    /// This CLI or this Mac's policy cannot run with RichDoctor's isolation.
    Unsupported,
    /// The process ended without saying why.
    Ended,
}

impl Failure {
    /// The one observed cause, as round 39 words it (the first four are its copy; the rest
    /// follow the same pattern for causes the mockup did not draw).
    pub fn message(self) -> &'static str {
        match self {
            Failure::NotInstalled => "Claude Code was not found on this Mac.",
            Failure::SignedOut => "RichDoctor could not use the default Claude Code sign-in.",
            Failure::CouldNotCheck => "RichDoctor could not check whether Claude Code is signed in.",
            Failure::Timeout => "Claude Code did not answer within 90 seconds.",
            Failure::RateLimited => "Claude Code asked RichDoctor to wait before asking again.",
            Failure::Restricted => "Claude Code said this account cannot be used for this right now.",
            Failure::Service => "Claude Code's service did not answer normally.",
            Failure::Unsupported => "This version of Claude Code cannot run with the protections RichDoctor needs.",
            Failure::Ended => "Claude Code stopped before it answered.",
        }
    }
    /// Whether Retry is offered: not when Claude Code is not installed (round 39), and not when
    /// this CLI cannot be isolated (retrying cannot change that).
    pub fn offers_retry(self) -> bool {
        !matches!(self, Failure::NotInstalled | Failure::Unsupported)
    }
}

/// The failure an error result or error output describes. Bounded words only: the text is
/// read here and dropped, never stored (PRD §4: no raw auth output).
pub fn classify(text: &str) -> Failure {
    let t = text.to_ascii_lowercase();
    let any = |words: &[&str]| words.iter().any(|w| t.contains(w));
    if any(&["unknown option", "unknown argument", "error: option", "managed policy", "not allowed by your organization's policy"]) {
        Failure::Unsupported
    } else if any(&["not logged in", "please run /login", "invalid api key", "authentication_error", "oauth token has expired", "oauth token revoked", "login required"]) {
        Failure::SignedOut
    } else if any(&["organization has been disabled", "permission_error", "account has been disabled", "not authorized to use"]) {
        Failure::Restricted
    } else if any(&["rate_limit", "rate limit", "429", "usage limit", "too many requests"]) {
        Failure::RateLimited
    } else if any(&["overloaded", "529", "api_error", "internal server error", "500", "502", "503", "service unavailable", "connection error", "econnrefused", "network"]) {
        Failure::Service
    } else {
        Failure::Ended
    }
}

// ---------------------------------------------------------------------------------------
// WHAT CLAUDE SAID, IN THE PLAYBOOK'S TWO SHAPES
// ---------------------------------------------------------------------------------------

/// One turn of the conversation as the window shows it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct Turn {
    pub say: String,
    pub question: String,
    pub choices: Vec<String>,
    pub ready: bool,
}

/// The report as Claude drafted it (the playbook's second shape). Host code decides what is
/// shown: these words are scrubbed and checked like every other ([`crate::report`]).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct Drafted {
    pub title: String,
    pub what_happened: String,
    pub expected: String,
    pub when: String,
    pub steps: Vec<String>,
    pub possible_causes: Vec<Cause>,
    pub missing: Vec<String>,
    pub evidence_used: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct Cause {
    pub text: String,
    pub evidence: Vec<String>,
}

/// The first `{` to the last `}`, read as JSON.
fn json_object(raw: &str) -> Option<Value> {
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    (end > start).then(|| serde_json::from_str(&raw[start..=end]).ok()).flatten()
}

fn text(v: &Value, key: &str, max: usize) -> String {
    let s = v[key].as_str().unwrap_or("").trim();
    s.chars().take(max).collect()
}

fn texts(v: &Value, key: &str, n: usize, max: usize) -> Vec<String> {
    v[key].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).map(|s| s.trim().chars().take(max).collect::<String>()).filter(|s| !s.is_empty()).take(n).collect()).unwrap_or_default()
}

/// A conversation turn. Words that are not the playbook's JSON are kept as what Claude said,
/// with no question: the person still sees them, and Prepare report now is still there.
pub fn parse_turn(raw: &str) -> Turn {
    match json_object(raw) {
        Some(v) if v.get("say").is_some() || v.get("question").is_some() => Turn {
            say: text(&v, "say", 1200),
            question: text(&v, "question", 400),
            choices: texts(&v, "choices", 4, 60),
            ready: v["ready"].as_bool().unwrap_or(false),
        },
        _ => Turn { say: raw.trim().chars().take(1200).collect(), ..Default::default() },
    }
}

/// The drafted report, or `None` when the answer was not one.
pub fn parse_drafted(raw: &str) -> Option<Drafted> {
    let v = json_object(raw)?;
    if v.get("title").is_none() && v.get("what_happened").is_none() {
        return None;
    }
    let ids = |x: &Value| -> Vec<String> { x.as_array().map(|a| a.iter().filter_map(|i| i.as_str()).filter(|i| crate::evidence::is_id(i)).map(str::to_string).collect()).unwrap_or_default() };
    Some(Drafted {
        title: text(&v, "title", 200),
        what_happened: text(&v, "what_happened", 4000),
        expected: text(&v, "expected", 1000),
        when: text(&v, "when", 300),
        steps: texts(&v, "steps", 12, 400),
        possible_causes: v["possible_causes"].as_array().map(|a| a.iter().take(5).map(|c| Cause { text: text(c, "text", 800), evidence: ids(&c["evidence"]) }).filter(|c| !c.text.is_empty()).collect()).unwrap_or_default(),
        missing: texts(&v, "missing", 10, 400),
        evidence_used: ids(&v["evidence_used"]),
    })
}

/// One user message on standard input (the baseline's stream-json shape,
/// [`richos_core::bug_report::writer_input`]), with an optional approved screenshot.
pub fn user_message(text: &str, picture: Option<&richos_core::bug_report::Picture>) -> String {
    richos_core::bug_report::writer_input(text, picture)
}

/// The same stream-json user message with every screenshot approved since the last one (round 40:
/// several are added at a time), images first, then the text.
pub fn user_message_with(text: &str, pictures: &[richos_core::bug_report::Picture]) -> String {
    let mut content: Vec<serde_json::Value> = pictures
        .iter()
        .map(|p| serde_json::json!({ "type": "image", "source": { "type": "base64", "media_type": p.media_type, "data": p.base64 } }))
        .collect();
    content.push(serde_json::json!({ "type": "text", "text": text }));
    format!("{}\n", serde_json::json!({ "type": "user", "message": { "role": "user", "content": content } }))
}

// ---------------------------------------------------------------------------------------
// THE ONE OWNED PROCESS
// ---------------------------------------------------------------------------------------

/// What [`Session::next`] returns.
#[derive(Debug, Clone, PartialEq)]
pub enum Next {
    Event(StreamEvent),
    /// The deadline passed with nothing more.
    Waiting,
    /// The process's output ended.
    Ended,
}

/// **ONE RESTRICTED CLAUDE CODE PROCESS**, owned by RichDoctor: started for a help session,
/// fed one user message per operation, and ended (its whole process group, with the support
/// server under it) when the session closes, when an operation is canceled or runs past its
/// bound, or when RichDoctor quits ([`Session::stop`], and on drop).
pub struct Session {
    child: Arc<Mutex<richos_core::owned_process::OwnedChild>>,
    stdin: Option<std::process::ChildStdin>,
    lines: Receiver<String>,
    /// The last few KiB of standard error, in memory only, for [`classify`].
    stderr_tail: Arc<Mutex<Vec<u8>>>,
    /// The inventory check passed ([`Session::begin`]).
    checked: bool,
}

impl Session {
    /// Start the CLI at `bin` in the RichDoctor-owned folder `cwd`. Nothing is sent: the first
    /// message goes through [`Session::begin`], which checks the inventory.
    pub fn start(bin: &Path, spec: &Spec, cwd: &Path, env: &[(String, String)]) -> Result<Session, (Failure, String)> {
        crate::util::private_dir(cwd).map_err(|e| (Failure::Ended, format!("could not make the working folder: {e}")))?;
        let mut command = Command::new(bin);
        command.args(args(spec)).current_dir(cwd).env_clear().envs(env.iter().cloned()).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        richos_core::owned_process::OwnedChild::configure(&mut command);
        let mut raw = command.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound { (Failure::NotInstalled, e.to_string()) } else { (Failure::Ended, e.to_string()) }
        })?;
        let stdout = raw.stdout.take().expect("piped");
        let stderr = raw.stderr.take().expect("piped");
        let stdin = raw.stdin.take();
        let child = Arc::new(Mutex::new(richos_core::owned_process::OwnedChild::new(raw)));
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                // One line is at most 8 MiB: a reply carrying a picture echo is large, nothing
                // legitimate is larger, and a runaway line cannot exhaust memory.
                match (&mut reader).take(8 << 20).read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if tx.send(String::from_utf8_lossy(&line).into_owned()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        let stderr_tail = Arc::new(Mutex::new(Vec::new()));
        let tail = stderr_tail.clone();
        std::thread::spawn(move || {
            let mut stderr = stderr;
            let mut buf = [0u8; 4096];
            while let Ok(n) = stderr.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let mut t = tail.lock().unwrap();
                t.extend_from_slice(&buf[..n]);
                let len = t.len();
                if len > 8192 {
                    t.drain(..len - 8192);
                }
            }
        });
        Ok(Session { child, stdin, lines, stderr_tail, checked: false })
    }

    /// **THE FIRST MESSAGE, AND THE INVENTORY CHECK.** In stream-json input mode the CLI prints
    /// its `system/init` line only once it has read the first user message (measured on 2.1.307:
    /// no init line in 60 s with standard input open and empty). So the first message is written,
    /// the init line is read and checked ([`check_init`]) the moment it arrives, before the
    /// model's first answer can come back, and on any difference the whole process group is
    /// ended: no tool call can have been made, because a tool call follows a model answer.
    pub fn begin(&mut self, line: &str, within: Duration) -> Result<(), (Failure, String)> {
        if let Err(e) = self.send(line) {
            std::thread::sleep(Duration::from_millis(100));
            let why = self.stderr_text();
            self.stop();
            return Err((if why.is_empty() { Failure::Ended } else { classify(&why) }, e.to_string()));
        }
        let deadline = Instant::now() + within;
        loop {
            match self.next(deadline) {
                Next::Event(StreamEvent::Init(init)) => {
                    return match check_init(&init) {
                        Ok(()) => {
                            self.checked = true;
                            Ok(())
                        }
                        Err(why) => {
                            self.stop();
                            Err((Failure::Unsupported, why))
                        }
                    };
                }
                Next::Event(StreamEvent::Result { text, .. }) => {
                    self.stop();
                    return Err((classify(&text), "ended before it started".into()));
                }
                Next::Event(_) => continue,
                Next::Waiting => {
                    self.stop();
                    return Err((Failure::Timeout, "no init line in time".into()));
                }
                Next::Ended => {
                    std::thread::sleep(Duration::from_millis(100));
                    let why = self.stderr_text();
                    self.stop();
                    return Err((classify(&why), "ended before it started".into()));
                }
            }
        }
    }

    /// Whether [`Session::begin`] passed the inventory check.
    pub fn checked(&self) -> bool {
        self.checked
    }

    /// Hand Claude one more user message (after [`Session::begin`]).
    pub fn send(&mut self, line: &str) -> std::io::Result<()> {
        let stdin = self.stdin.as_mut().ok_or_else(|| std::io::Error::other("the session has ended"))?;
        stdin.write_all(line.as_bytes())?;
        if !line.ends_with('\n') {
            stdin.write_all(b"\n")?;
        }
        stdin.flush()
    }

    /// The next event, waiting until `deadline` at most.
    pub fn next(&self, deadline: Instant) -> Next {
        let wait = deadline.saturating_duration_since(Instant::now());
        match self.lines.recv_timeout(wait) {
            Ok(line) => Next::Event(parse_line(&line)),
            Err(RecvTimeoutError::Timeout) => Next::Waiting,
            Err(RecvTimeoutError::Disconnected) => Next::Ended,
        }
    }

    /// The standard error seen so far, for [`classify`] only.
    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr_tail.lock().unwrap()).into_owned()
    }

    /// The fence that ends this process group from another thread (Cancel, Quit).
    pub fn stopper(&self) -> Stopper {
        Stopper(self.child.clone())
    }

    /// End the process group now. Safe to call more than once.
    pub fn stop(&mut self) {
        self.stdin.take();
        self.stopper().stop();
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Ends a [`Session`]'s process group from anywhere.
#[derive(Clone)]
pub struct Stopper(Arc<Mutex<richos_core::owned_process::OwnedChild>>);

impl Stopper {
    pub fn stop(&self) {
        let mut child = self.0.lock().unwrap();
        child.kill().ok();
        child.wait().ok();
    }
}

/// The progress line round 39 shows while a tool runs: what is being done, in plain words.
pub fn progress_line(name: &str, input: &Value, label_of: impl Fn(&str) -> Option<String>) -> String {
    let bare = name.rsplit("__").next().unwrap_or(name);
    match bare {
        TOOL_COLLECT => crate::collect::Category::parse(input["category"].as_str().unwrap_or(""))
            .map(|c| c.progress().to_string())
            .unwrap_or_else(|| "Checking RichOS diagnostic information…".into()),
        TOOL_READ => match input["id"].as_str().and_then(&label_of) {
            Some(label) => format!("Reading {label}…"),
            None => "Reading what RichDoctor collected…".into(),
        },
        _ => "Working…".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> Spec {
        Spec { server_exe: "/Applications/RichDoctor.app/Contents/MacOS/RichDoctor".into(), draft_dir: "/Users/x/Library/Application Support/com.richos.richdoctor/drafts/d-1".into() }
    }

    /// INVARIANT (PRD §9): writer_args is the sole baseline. Every baseline argument is present,
    /// in order, with only the MCP configuration's value replaced.
    #[test]
    fn the_arguments_are_the_measured_baseline_plus_the_named_delta() {
        let base = richos_core::bug_report::writer_args();
        let ours = args(&spec());
        assert_eq!(ours.len(), base.len() + 12);
        for (i, b) in base.iter().enumerate() {
            if i > 0 && base[i - 1] == "--mcp-config" {
                assert_eq!(ours[i], mcp_config(&spec()));
                assert_ne!(ours[i], *b);
            } else {
                assert_eq!(&ours[i], b, "baseline argument {i} changed");
            }
        }
        let delta = &ours[base.len()..];
        assert_eq!(delta[0..2], ["--allowedTools".to_string(), "mcp__richdoctor__collect_diagnostics,mcp__richdoctor__read_evidence".to_string()]);
        for flag in ["--permission-mode", "--permission-prompts", "--restricted", "--settings", "--disable-slash-commands", "--append-system-prompt"] {
            assert!(delta.iter().any(|a| a == flag), "{flag}");
        }
        assert_eq!(delta[delta.iter().position(|a| a == "--permission-mode").unwrap() + 1], "dontAsk");
        assert_eq!(delta[delta.iter().position(|a| a == "--permission-prompts").unwrap() + 1], "none");
        assert_eq!(delta.last().unwrap(), PLAYBOOK);
    }

    /// INVARIANT (PRD §4): no permission bypass, no bare mode (it never reads the subscription
    /// sign-in), no resumed CLI session, no chosen model, no persistence.
    #[test]
    fn the_arguments_never_bypass_resume_or_pick_a_model() {
        let all = args(&spec()).join("\u{1}");
        for banned in ["--dangerously-skip-permissions", "--allow-dangerously-skip-permissions", "bypassPermissions", "--bare", "--resume", "--continue", "--model", "--add-dir", "--plugin-dir", "--tools\u{1}default"] {
            assert!(!all.contains(banned), "{banned}");
        }
        assert!(all.contains("--no-session-persistence"));
        assert!(all.contains("--strict-mcp-config"));
        assert!(all.contains("--tools\u{1}\u{1}"), "built-in tools stay disabled");
    }

    #[test]
    fn the_mcp_configuration_names_only_richdoctors_own_server() {
        let v: Value = serde_json::from_str(&mcp_config(&spec())).unwrap();
        let servers = v["mcpServers"].as_object().unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers["richdoctor"]["command"], "/Applications/RichDoctor.app/Contents/MacOS/RichDoctor");
        assert_eq!(servers["richdoctor"]["args"][0], SERVER_FLAG);
    }

    /// INVARIANT (PRD §4): a RichOS-managed CLAUDE_CONFIG_DIR never reaches the CLI.
    #[test]
    fn the_environment_is_built_from_nothing() {
        let env = environment(|k| match k {
            "HOME" => Some("/Users/x".into()),
            "HTTPS_PROXY" => Some("http://proxy:8080".into()),
            "CLAUDE_CONFIG_DIR" | "CLAUDECODE" | "ANTHROPIC_API_KEY" | "RICHOS_CLAUDE_BIN" => Some("set".into()),
            _ => None,
        });
        let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["HOME", "HTTPS_PROXY", "PATH"]);
    }

    fn measured_init() -> Value {
        serde_json::json!({"type":"system","subtype":"init","tools":["mcp__richdoctor__collect_diagnostics","mcp__richdoctor__read_evidence"],"mcp_servers":[{"name":"richdoctor","status":"connected","source":"dynamic"}],"permissionMode":"dontAsk","slash_commands":[],"skills":[],"plugins":[{"name":"cc-plugin-agents-md","path":"builtin"},{"name":"cc-plugin-telemetry","path":"builtin"}],"claude_code_version":"2.1.307"})
    }

    #[test]
    fn the_measured_inventory_passes_and_any_widening_is_refused() {
        assert_eq!(check_init(&measured_init()), Ok(()));
        let mut extra_tool = measured_init();
        extra_tool["tools"].as_array_mut().unwrap().push("Bash".into());
        assert!(check_init(&extra_tool).is_err());
        let mut missing = measured_init();
        missing["tools"] = serde_json::json!(["mcp__richdoctor__read_evidence"]);
        assert!(check_init(&missing).is_err());
        let mut mode = measured_init();
        mode["permissionMode"] = "bypassPermissions".into();
        assert!(check_init(&mode).is_err());
        let mut server = measured_init();
        server["mcp_servers"].as_array_mut().unwrap().push(serde_json::json!({"name":"other","status":"connected"}));
        assert!(check_init(&server).is_err());
        let mut failed = measured_init();
        failed["mcp_servers"][0]["status"] = "failed".into();
        assert!(check_init(&failed).is_err());
        let mut plugin = measured_init();
        plugin["plugins"].as_array_mut().unwrap().push(serde_json::json!({"name":"x","path":"/Users/x/.claude/plugins/x"}));
        assert!(check_init(&plugin).is_err());
        let mut skills = measured_init();
        skills["skills"] = serde_json::json!(["deploy"]);
        assert!(check_init(&skills).is_err());
    }

    #[test]
    fn stream_lines_are_read_into_the_few_facts_the_window_needs() {
        let tool = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"mcp__richdoctor__collect_diagnostics","input":{"category":"startup_crash"}}]}}"#;
        assert_eq!(parse_line(tool), StreamEvent::ToolUse { name: "mcp__richdoctor__collect_diagnostics".into(), input: serde_json::json!({"category":"startup_crash"}) });
        let said = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello"}]}}"#;
        assert_eq!(parse_line(said), StreamEvent::Text("Hello".into()));
        let err = r#"{"type":"result","subtype":"success","is_error":true,"result":"Not logged in · Please run /login"}"#;
        assert_eq!(parse_line(err), StreamEvent::Result { is_error: true, text: "Not logged in · Please run /login".into(), subtype: "success".into() });
        let max = r#"{"type":"result","subtype":"error_max_turns","is_error":false}"#;
        assert!(matches!(parse_line(max), StreamEvent::Result { is_error: true, .. }));
        assert_eq!(parse_line("not json"), StreamEvent::Other);
    }

    /// Only a confirmed negative is "signed out" (PRD §4); a rate limit is not exhaustion.
    #[test]
    fn failures_are_named_only_by_what_was_said() {
        assert_eq!(classify("Not logged in · Please run /login"), Failure::SignedOut);
        assert_eq!(classify("API Error: 529 {\"type\":\"overloaded_error\"}"), Failure::Service);
        assert_eq!(classify("API Error: 429 rate_limit_error"), Failure::RateLimited);
        assert_eq!(classify("error: unknown option '--restricted'"), Failure::Unsupported);
        assert_eq!(classify("something else"), Failure::Ended);
        assert_eq!(Failure::SignedOut.message(), "RichDoctor could not use the default Claude Code sign-in.");
        assert!(!Failure::NotInstalled.offers_retry());
        assert!(Failure::Timeout.offers_retry());
    }

    #[test]
    fn a_turn_that_is_not_the_playbooks_json_is_still_shown() {
        let t = parse_turn(r#"{"say":"I found a crash record.","question":"When did it happen?","choices":["Just now","Earlier today","Yesterday","Enter a time…","extra"],"ready":false}"#);
        assert_eq!(t.question, "When did it happen?");
        assert_eq!(t.choices.len(), 4);
        let plain = parse_turn("Thanks, I will look.");
        assert_eq!(plain.say, "Thanks, I will look.");
        assert!(plain.question.is_empty() && !plain.ready);
    }

    #[test]
    fn a_drafted_report_keeps_only_evidence_ids_of_the_right_shape() {
        let d = parse_drafted(r#"```json
{"title":"RichOS closes at start","what_happened":"It closes.","possible_causes":[{"text":"Maybe a crash","evidence":["ev-0123456789ab","../../etc/passwd"]}],"evidence_used":["ev-0123456789ab","x"]}
```"#).unwrap();
        assert_eq!(d.possible_causes[0].evidence, ["ev-0123456789ab"]);
        assert_eq!(d.evidence_used, ["ev-0123456789ab"]);
        assert!(parse_drafted("no report here").is_none());
    }
}

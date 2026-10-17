//! THE REPORT SERVER — `richos_operator.report`, the one tool his lead tells him things with
//! (operator back-end spec r2 (c), unchanged in r3 §11 item 7).
//!
//! The lead calls `report(handle?, kind, text, lands[], files[], agents[])`. This server, the
//! app's own executable run as `--operator-mcp <scope>` (`operator_profile::mcp_config`),
//! checks what it can check and appends one record to the lead's outbox, which the host reads
//! and turns into notices ((g)/(q), not built yet).
//!
//! ## Every "landed" is checked in Git before it is said
//!
//! T3 Code's ledger row §2.2, which r2 adopts: *"A reconnect alone cannot distinguish
//! successful replacement from rollback"*, so the app reads Git, never the back end's
//! say-so. For each land the lead names, this server asks the repository itself, read-only:
//!
//! 1. the commit exists (`git cat-file -e <commit>^{commit}`);
//! 2. it is on the integration branch (`git merge-base --is-ancestor <commit> refs/heads/<into>`),
//!    the same test the engine's own completion uses (`mega-lander/app.py:676`);
//! 3. if the named branch still exists, its tip is on the integration branch too, so a branch
//!    with work that never landed is not called landed;
//! 4. pushed: the same ancestry against `<into>@{upstream}`, decided locally with no network
//!    (all three of his repositories' `main` track `origin/main`, r3 §2).
//!
//! A land that fails any of 1-3 is recorded with `landed: false` and the sentence
//! "… could not be confirmed as landed", and the lead is told so in the tool's answer. The
//! report itself is still recorded: his lead's words are never lost because one claim in them
//! was wrong (W2 step 7).
//!
//! ## What is refused outright, and nothing written
//!
//! A kind outside `update`, `question`, `answer`, `outcome`, `failed`; empty text; a handle
//! that is not an assignment of THIS conversation; a land, file or repository outside the
//! declared file roots or not absolute; a file that does not exist. Those are the lead's
//! mistakes, and it is told which, so it can report again correctly.
//!
//! And words that ask him something his record has already ruled (F3 of the operator contract
//! notes, §4 item 1): a report's asking paragraphs go through the same already-ruled check as a
//! prepared question card, and the refusal names and quotes the ruling
//! ([`crate::question_engine::check_ruled`]).
//!
//! ## Long text
//!
//! The notice keeps the register's 8,000-character bound (`assignment.rs:1288-1299`). Longer
//! text is also stored whole as a file beside the outbox, the record names it, and the notice
//! ends with "The full text is attached." Nothing he was told is cut (r2 (c), note 2).
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

pub const SERVER_NAME: &str = crate::operator_profile::REPORT_SERVER;
pub const REPORT_TOOL_NAME: &str = "report";
/// **The lead's own stop** (F6, operator contract notes §4 item 3; Frank's proposal: "a `stop`
/// on the lead's own server calling `stop_named`"). `TaskStop` reaches only this lead's agents;
/// this reaches his team's agents in every conversation, so a stop he gave to the wrong lead is
/// executed once, by that lead, and he never has to say it again.
pub const STOP_TOOL_NAME: &str = "stop";
pub const QUALIFIED_STOP_TOOL: &str = "mcp__richos_operator__stop";
pub const QUALIFIED_REPORT_TOOL: &str = crate::operator_profile::REPORT_TOOL;
pub const KINDS: [&str; 6] = ["update", "question", "withdraw_question", "answer", "outcome", "failed"];
/// The notice bound the register already uses (`assignment::sanitize_answer`).
pub const NOTICE_CHARS: usize = 8000;
pub const ATTACHED_SUFFIX: &str = "The full text is attached.";

const MAX_FRAME_BYTES: usize = 1024 * 1024;
const MAX_SCOPE_BYTES: u64 = 16 * 1024;
const MAX_LANDS: usize = 20;
const MAX_FILES: usize = 50;
const MAX_AGENTS: usize = 50;
/// The already-ruled check reads his record with python; the guard's own hook budget is 20 s.
const RULED_CHECK_BUDGET: std::time::Duration = std::time::Duration::from_secs(15);

/// The scope the app writes for one lead, and the only thing this server trusts.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportScope {
    pub version: u32,
    /// Where records are appended, one JSON line each.
    pub outbox: PathBuf,
    /// Where long text is stored whole.
    pub attachments: PathBuf,
    /// The declaration's file roots. Every path in a report must be under one.
    pub file_roots: Vec<PathBuf>,
    /// `<data>/engine-state`, where this conversation's assignment register lives.
    pub state_root: PathBuf,
    pub entity_id: String,
    pub thread_id: String,
    /// Which lead this is, as the claim names it.
    pub lead: String,
    /// Supplied by the runtime only after binding the original asking session.
    #[serde(default)]
    pub question_context: Option<crate::questions::AskScope>,
    /// **Where this lead's `stop` reaches the desk** (F6): the desk's socket and the leads'
    /// stop-only token. `None` when the app serves no desk, and then `stop` says so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desk: Option<LeadDesk>,
}

/// The desk's socket and the token that lets a lead make a named stop there, and nothing else
/// (`operator_desk_tools::DeskSocket::serve_with_lead`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeadDesk {
    pub socket: PathBuf,
    pub token: String,
}

fn scope_usable(scope: &ReportScope) -> bool {
    scope.version == 1
        && scope.outbox.is_absolute()
        && scope.attachments.is_absolute()
        && scope.state_root.is_absolute()
        && !scope.file_roots.is_empty()
        && scope.file_roots.iter().all(|root| root.is_absolute())
        && [&scope.entity_id, &scope.thread_id, &scope.lead].iter().all(|s| !s.is_empty())
}

pub fn write_scope(path: &Path, scope: &ReportScope) -> Result<(), String> {
    if !scope_usable(scope) {
        return Err("The app has not supplied a valid report scope. Nothing was written.".into());
    }
    let text = serde_json::to_string(scope).map_err(|e| e.to_string())?;
    crate::doctrine::write_verified(path, &text).map_err(|e| e.to_string())
}

pub(crate) fn read_scope(path: &Path) -> Result<ReportScope, String> {
    use std::io::Read;
    let file = std::fs::File::open(path)
        .map_err(|_| "RichOS has not opened a report scope for this lead. Nothing was recorded.".to_string())?;
    let mut text = String::new();
    file.take(MAX_SCOPE_BYTES + 1).read_to_string(&mut text)
        .map_err(|_| "The report scope could not be read. Nothing was recorded.".to_string())?;
    if text.len() as u64 > MAX_SCOPE_BYTES {
        return Err("The report scope is too large. Nothing was recorded.".into());
    }
    let scope: ReportScope = serde_json::from_str(&text)
        .map_err(|_| "The report scope is unreadable. Nothing was recorded.".to_string())?;
    if !scope_usable(&scope) {
        return Err("The report scope is not usable. Nothing was recorded.".into());
    }
    Ok(scope)
}

/// The report's arguments. Unknown fields are refused: a field this server does not know is
/// a field the lead thinks it said and he would never hear.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportArgs {
    #[serde(default)]
    handle: Option<String>,
    kind: String,
    text: String,
    #[serde(default)]
    lands: Vec<LandClaim>,
    #[serde(default)]
    files: Vec<PathBuf>,
    #[serde(default)]
    agents: Vec<String>,
    #[serde(default)]
    questions: Vec<crate::questions::QuestionInput>,
    #[serde(default)]
    question_id: Option<String>,
}

/// `path`, canonical, if it exists and sits under one of `roots`.
fn under_roots(label: &str, path: &Path, roots: &[PathBuf]) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(format!("The {label} {} is not an absolute path. Nothing was recorded.", path.display()));
    }
    let canonical = std::fs::canonicalize(path)
        .map_err(|_| format!("The {label} {} does not exist. Nothing was recorded.", path.display()))?;
    let inside = roots.iter().any(|root| std::fs::canonicalize(root).is_ok_and(|root| canonical.starts_with(root)));
    if !inside {
        return Err(format!("The {label} {} is outside the folders your team works in. Nothing was recorded.",
                           path.display()));
    }
    Ok(canonical)
}

/// A branch name this server will put in a Git command. Deliberately narrower than Git's own
/// rule: nothing here needs more, and a name that starts with `-` would be read as an option.
fn plain_branch(name: &str) -> bool {
    !name.is_empty() && !name.starts_with(['-', '/', '.']) && !name.ends_with(['/', '.'])
        && !name.ends_with(".lock") && !name.contains("..") && !name.contains("//")
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
}

/// One read-only Git question, in `repo`, with no optional locks taken.
fn git(repo: &Path, args: &[&str]) -> Option<std::process::Output> {
    std::process::Command::new("git")
        .arg("-C").arg(repo).args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env_remove("GIT_DIR").env_remove("GIT_WORK_TREE").env_remove("GIT_INDEX_FILE")
        .stdin(std::process::Stdio::null())
        .output().ok()
}

/// `Some(true)` ancestor, `Some(false)` not, `None` Git could not answer.
fn is_ancestor(repo: &Path, commit: &str, of: &str) -> Option<bool> {
    let output = git(repo, &["merge-base", "--is-ancestor", commit, of])?;
    match output.status.code() {
        Some(0) => Some(true),
        Some(1) => Some(false),
        _ => None,
    }
}

fn resolves(repo: &Path, what: &str) -> Option<String> {
    let output = git(repo, &["rev-parse", "--verify", "--quiet", what])?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Check one land in Git. `Err` is a malformed claim, refused; `Ok` carries Git's answer,
/// confirmed or not.
fn verify_land(claim: &LandClaim, roots: &[PathBuf]) -> Result<LandRecord, String> {
    let repository = under_roots("repository", &claim.repository, roots)?;
    let commit = claim.commit.to_ascii_lowercase();
    if !(40..=64).contains(&commit.len()) || !commit.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("A land must name its full commit hash, not {:?}. Nothing was recorded.", claim.commit));
    }
    let into = claim.into.clone().unwrap_or_else(|| "main".to_string());
    for name in std::iter::once(&into).chain(claim.branch.iter()) {
        if !plain_branch(name) {
            return Err(format!("{name:?} is not a branch name this server accepts. Nothing was recorded."));
        }
    }
    if !git(&repository, &["rev-parse", "--git-dir"]).is_some_and(|o| o.status.success()) {
        return Err(format!("{} is not a Git repository. Nothing was recorded.", repository.display()));
    }
    let integration = format!("refs/heads/{into}");
    let mut why = None;
    if resolves(&repository, &format!("{commit}^{{commit}}")).is_none() {
        why = Some(format!("Git has no commit {}", &commit[..12]));
    } else if resolves(&repository, &integration).is_none() {
        why = Some(format!("the repository has no branch {into}"));
    } else {
        match is_ancestor(&repository, &commit, &integration) {
            Some(true) => {}
            Some(false) => why = Some(format!("commit {} is not on {into}", &commit[..12])),
            None => why = Some(format!("Git could not say whether commit {} is on {into}", &commit[..12])),
        }
    }
    if why.is_none() {
        if let Some(branch) = &claim.branch {
            if let Some(tip) = resolves(&repository, &format!("refs/heads/{branch}^{{commit}}")) {
                if is_ancestor(&repository, &tip, &integration) != Some(true) {
                    why = Some(format!("branch {branch} still has work that is not on {into}"));
                }
            }
        }
    }
    let landed = why.is_none();
    let pushed = if landed {
        let upstream = git(&repository, &["rev-parse", "--symbolic-full-name", &format!("{into}@{{upstream}}")])
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|name| name.starts_with("refs/"));
        upstream.and_then(|upstream| is_ancestor(&repository, &commit, &upstream))
    } else {
        None
    };
    let name = repository.file_name().map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| repository.display().to_string());
    let label = claim.branch.clone().unwrap_or_else(|| format!("commit {}", &commit[..12]));
    let says = match (landed, pushed) {
        (true, Some(true)) => format!("Landed and pushed {label} in {name}."),
        (true, Some(false)) => format!("Landed {label} in {name}, not pushed yet."),
        (true, None) => format!("Landed {label} in {name}."),
        (false, _) => format!("{label} in {name} could not be confirmed as landed."),
    };
    Ok(LandRecord { repository, commit, branch: claim.branch.clone(), into, landed, pushed, why, says })
}

/// Append one record under an exclusive lock, and make it durable before answering.
fn append(outbox: &Path, record: &ReportRecord) -> Result<(), String> {
    if let Some(parent) = outbox.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("The report could not be recorded ({e})."))?;
    }
    let mut options = std::fs::OpenOptions::new();
    // Read as well as append: the last byte says whether an earlier append was cut short.
    options.read(true).append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(outbox).map_err(|e| format!("The report could not be recorded ({e})."))?;
    let mut line = serde_json::to_vec(record).map_err(|e| e.to_string())?;
    line.push(b'\n');
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        // SAFETY: flock on a descriptor this function owns; released when `file` closes.
        unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) };
    }
    // An append that ended early (the process ended, or the disk filled) leaves a partial
    // record with no newline. This record starts on a fresh line so it is never glued onto
    // that fragment; the fragment is left as it was and `read_outbox` skips it (hunt part 1
    // finding 44). Checked under the lock, so no other append is half-written here.
    if ends_mid_line(&mut file).map_err(|e| format!("The report could not be recorded ({e})."))? {
        line.insert(0, b'\n');
    }
    file.write_all(&line).and_then(|_| file.sync_all())
        .map_err(|e| format!("The report could not be recorded ({e})."))
}

/// Does the file end partway through a line? An empty file does not.
fn ends_mid_line(file: &mut std::fs::File) -> io::Result<bool> {
    use std::io::{Read, Seek, SeekFrom};
    if file.metadata()?.len() == 0 {
        return Ok(false);
    }
    file.seek(SeekFrom::End(-1))?;
    let mut last = [0u8; 1];
    file.read_exact(&mut last)?;
    Ok(last[0] != b'\n')
}

/// Store the whole of a long text. Created new, never over another file.
fn attach(dir: &Path, text: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("The full text could not be stored ({e})."))?;
    let path = dir.join(format!("{}.md", uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path).map_err(|e| format!("The full text could not be stored ({e})."))?;
    file.write_all(text.as_bytes()).and_then(|_| file.sync_all())
        .map_err(|e| format!("The full text could not be stored ({e})."))?;
    Ok(path)
}

/// One land, as the lead names it.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandClaim {
    pub repository: PathBuf,
    pub commit: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub into: Option<String>,
}

/// One land, as Git answered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LandRecord {
    pub repository: PathBuf,
    pub commit: String,
    pub branch: Option<String>,
    pub into: String,
    pub landed: bool,
    /// `None` when the integration branch has no upstream to compare with.
    pub pushed: Option<bool>,
    /// Why it could not be confirmed, when it could not.
    pub why: Option<String>,
    /// The sentence the notice says about it.
    pub says: String,
}

/// One appended record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportRecord {
    pub version: u32,
    pub at_ms: u64,
    pub lead: String,
    pub entity_id: String,
    pub thread_id: String,
    pub handle: Option<String>,
    pub kind: String,
    /// At most [`NOTICE_CHARS`] characters, plus [`ATTACHED_SUFFIX`] when `attachment` is set.
    pub text: String,
    pub attachment: Option<PathBuf>,
    pub lands: Vec<LandRecord>,
    pub files: Vec<PathBuf>,
    pub agents: Vec<String>,
}

pub fn tools() -> Value {
    let questions=crate::question_tools::tools()["tools"][0]["inputSchema"]["properties"]["questions"].clone();
    json!({"tools":[
        {"name": REPORT_TOOL_NAME,
         "description": "Tell the CEO something. Everything you say at the end of a turn also reaches him; use this tool for a land, a file, a question, or a report on an assignment's handle. `kind`: `update` (progress), `question` (you need his answer; he is not at a terminal, so ask here and carry on with anything that does not depend on it), `withdraw_question` (close your own question by question_id, using text as the visible reason and the original handle if any), `answer` (a reply to a question of his that came without a handle; it closes nothing), `outcome` (the assignment on the handle is done; a question of his that came with a handle is closed with `outcome`, and he sees it as the answer), `failed` (it cannot be done). Only `outcome` or `failed` on a handle closes it. An `outcome` that names a land Git cannot confirm closes nothing: make the land real and report `outcome` again, or report `failed`. `handle`: the assignment this is about, exactly as the app gave it to you; leave it out for the conversation itself. `lands`: each land you are reporting, with the repository's absolute path, the FULL commit hash that is on the integration branch, the branch you merged, and `into` if the integration branch is not `main`. Every land is checked in Git before he is told it landed; one that cannot be confirmed is told to him as not confirmed. `files`: absolute paths of files he should see. `agents`: the names of the agents working on this handle.",
         "inputSchema": {"type": "object", "additionalProperties": false, "required": ["kind", "text"],
            "properties": {
                "questions": questions,
                "question_id": {"type":"string"},
                "handle": {"type": "string"},
                "kind": {"type": "string", "enum": KINDS},
                "text": {"type": "string"},
                "lands": {"type": "array", "maxItems": MAX_LANDS, "items": {"type": "object", "additionalProperties": false,
                    "required": ["repository", "commit"],
                    "properties": {"repository": {"type": "string"}, "commit": {"type": "string"},
                                   "branch": {"type": "string"}, "into": {"type": "string"}}}},
                "files": {"type": "array", "maxItems": MAX_FILES, "items": {"type": "string"}},
                "agents": {"type": "array", "maxItems": MAX_AGENTS, "items": {"type": "string"}}}},
         "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": false, "openWorldHint": false}},
        {"name": STOP_TOOL_NAME,
         "description": "Stop named agents of his team, and nothing else, when he names them in your conversation, whether they are yours or another conversation's. Call it at once, as your first tool call, before anything else you do with his message. `names`: the agents exactly as he named them. `words`: his words, verbatim. TaskStop reaches only your own agents; this reaches every conversation's, so he never has to say it twice. It returns what was measured: say an agent stopped only when this says it did.",
         "inputSchema": {"type": "object", "additionalProperties": false, "required": ["names", "words"],
             "properties": {"names": {"type": "array", "minItems": 1, "maxItems": 20, "items": {"type": "string", "minLength": 1}},
                            "words": {"type": "string", "minLength": 1}}},
         "annotations": {"readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false}}
    ]})
}

/// F6: the lead's named stop, through the desk. Only names and his words go; the desk decides
/// whose agent each is.
fn lead_stop(scope_path: &Path, arguments: &Value) -> Result<Value, String> {
    let scope = read_scope(scope_path)?;
    let (names, words) = crate::operator_desk_tools::stop_arguments(arguments)?;
    let desk = scope.desk.ok_or("RichOS is not serving your team's desk in this launch, so nothing was stopped. \
Tell him the stop could not be made here.")?;
    crate::operator_desk_tools::stop_through_desk(&desk.socket, &desk.token, &names, &words)
}

/// The answer, shared by the protocol adapter and the tests.
pub fn call(scope_path: &Path, name: &str, arguments: Value) -> Result<Value, String> {
    if name == STOP_TOOL_NAME {
        return lead_stop(scope_path, &arguments);
    }
    if name != REPORT_TOOL_NAME {
        return Err("That tool does not exist on this server. Nothing was recorded.".into());
    }
    let scope = read_scope(scope_path)?;
    let args: ReportArgs = serde_json::from_value(arguments)
        .map_err(|e| format!("The report could not be read ({e}). Nothing was recorded."))?;
    if !KINDS.contains(&args.kind.as_str()) {
        return Err(format!("`kind` must be one of {}, not {:?}. Nothing was recorded.", KINDS.join(", "), args.kind));
    }
    if args.text.trim().is_empty() {
        return Err("The report's text must not be empty. Nothing was recorded.".into());
    }
    if args.lands.len() > MAX_LANDS || args.files.len() > MAX_FILES || args.agents.len() > MAX_AGENTS {
        return Err("The report names too many lands, files or agents. Nothing was recorded.".into());
    }
    for agent in &args.agents {
        if agent.trim().is_empty() || agent.chars().count() > 100 || agent.chars().any(char::is_control) {
            return Err(format!("{agent:?} is not an agent name. Nothing was recorded."));
        }
    }
    if let Some(handle) = &args.handle {
        let register = crate::assignment::read_all(&scope.state_root, &scope.entity_id, &scope.thread_id)
            .map_err(|e| format!("The assignment register could not be read ({e}). Nothing was recorded."))?;
        if args.kind == "question" && register.iter().any(|item| &item.id == handle && !item.state.is_open()) {
            return Err("The asking assignment has stopped. No question was recorded.".into());
        }
        if !register.iter().any(|item| &item.id == handle) {
            return Err(format!("{handle:?} is not an assignment of this conversation. Nothing was recorded."));
        }
    }
    let mut files = Vec::new();
    for file in &args.files {
        let canonical = under_roots("file", file, &scope.file_roots)?;
        if !canonical.is_file() {
            return Err(format!("The file {} is not a file. Nothing was recorded.", file.display()));
        }
        files.push(canonical);
    }
    // Every claim is checked BEFORE anything is written, so a malformed one refuses the whole
    // report and a wrong one is recorded as not confirmed.
    let lands = args.lands.iter().map(|claim| verify_land(claim, &scope.file_roots))
        .collect::<Result<Vec<_>, _>>()?;
    // F3: a question written into the words themselves goes through the same settled-by-his-words
    // check as a prepared question card (`question_engine::check_ruled`). A withdrawal's words are
    // a reason, never a question to him.
    let mut unchecked = None;
    if args.kind != "withdraw_question" {
        if let Some(context) = scope.question_context.as_ref() {
            match crate::question_engine::check_ruled(context, &args.text, RULED_CHECK_BUDGET) {
                crate::question_engine::Ruled::Clear => {}
                crate::question_engine::Ruled::Ruled(refusal) => return Err(refusal),
                crate::question_engine::Ruled::Unchecked(why) => unchecked = Some(why),
            }
        }
    }
    if args.kind=="question" || args.kind=="withdraw_question" {
        if !args.lands.is_empty() || !args.files.is_empty() {return Err("Report lands and files separately from a question set.".into());}
        let mut context=scope.question_context.clone().ok_or("The operator runtime has not bound its question delivery scope.")?;
        if context.root!=scope.state_root || context.entity_id!=scope.entity_id || context.thread_id!=scope.thread_id {return Err("The operator question scope does not match this conversation.".into());}
        context.asker=args.handle.as_ref().map_or_else(|| "operator:conversation".into(), |h| format!("operator:handle:{h}"));
        let store = crate::questions::Store::new(&scope.state_root);
        if args.kind == "withdraw_question" {
            if !args.questions.is_empty() { return Err("A withdrawal cannot also ask questions.".into()); }
            store.withdraw(&context.entity_id, &context.thread_id, &context.asker,
                args.question_id.as_deref().ok_or("Supply the question_id to withdraw.")?, &args.text)?;
            return Ok(json!({"withdrawn":true}));
        }
        if args.question_id.is_some() { return Err("question_id is only for a withdrawal.".into()); }
        let questions=store.ask(&context,args.questions)?;
        return Ok(json!({"recorded":true,"questions":questions.iter().map(crate::questions::Question::public_value).collect::<Vec<_>>(),"says":"The questions are recorded for display. Continue work that does not depend on the answer."}));
    }
    if args.question_id.is_some() { return Err("question_id is only for a withdrawal.".into()); }
    if !args.questions.is_empty(){return Err("Prepared questions require kind question.".into());}
    let (text, attachment) = if args.text.chars().count() > NOTICE_CHARS {
        let attached = attach(&scope.attachments, &args.text)?;
        (format!("{}\n\n{ATTACHED_SUFFIX}", crate::assignment::sanitize_answer(&args.text)), Some(attached))
    } else {
        (crate::assignment::sanitize_answer(&args.text), None)
    };
    let record = ReportRecord {
        version: 1,
        at_ms: crate::assignment::now_ms(),
        lead: scope.lead.clone(),
        entity_id: scope.entity_id.clone(),
        thread_id: scope.thread_id.clone(),
        handle: args.handle,
        kind: args.kind,
        text,
        attachment,
        lands,
        files,
        agents: args.agents,
    };
    append(&scope.outbox, &record)?;
    let mut said = vec!["Recorded; the CEO will be told.".to_string()];
    if let Some(why) = unchecked {
        said.push(format!("His record could not be checked for a question in these words ({why}), so they were recorded unchecked."));
    }
    for land in &record.lands {
        match &land.why {
            None => said.push(format!("Confirmed in Git: {}", land.says)),
            Some(why) => said.push(format!(
                "{} ({why}). He will be told it could not be confirmed. If it did land, check the repository, commit and branch you named and report again.",
                land.says)),
        }
    }
    // Q3 (c) of Frank's review: the host does not close an `outcome` on a handle that names a
    // land Git could not confirm, so the lead is told here, with what to do next.
    if record.kind == "outcome" && record.handle.is_some() && record.lands.iter().any(|l| !l.landed) {
        said.push("Because a land could not be confirmed, this outcome does not close the assignment; it stays open. \
Once the land is real, report `outcome` again, or report `failed`.".into());
    }
    Ok(json!({"recorded": true, "says": said.join(" ")}))
}

/// Every record in an outbox, in order. What the host will read. A missing outbox is empty.
///
/// **An interrupted append never hides the records after it** (hunt part 1 finding 44).
/// Only whole lines are read: bytes after the last newline are an append still being written,
/// or one that was cut short, and are left for a later read. A whole line that is not JSON at
/// all is a cut-short append that a later one closed by starting its own line (`append`); its
/// writer was never told "recorded", so it is not a report and it is skipped. Every read
/// classifies the same bytes the same way, so a record's position never moves: the host's
/// cursor and its owed closes are positions. A line that IS JSON but not a record is not a
/// torn write, and still fails the read, as before.
pub fn read_outbox(path: &Path) -> Result<Vec<ReportRecord>, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.to_string()),
    };
    let whole = match bytes.iter().rposition(|b| *b == b'\n') {
        Some(end) => &bytes[..end],
        None => return Ok(Vec::new()),
    };
    let mut records = Vec::new();
    for line in whole.split(|b| *b == b'\n') {
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        match serde_json::from_slice::<ReportRecord>(line) {
            Ok(record) => records.push(record),
            Err(e) if serde_json::from_slice::<Value>(line).is_ok() => return Err(e.to_string()),
            Err(_) => records.extend(glued_record(line)),
        }
    }
    Ok(records)
}

/// Before `append` started a fresh line after a cut-short one, the next record was written
/// straight onto the fragment. Its lead was told "recorded", so it is recovered: the record
/// is the first `{` from which the rest of the line is one whole record. A fragment is a cut
/// prefix of a record, so no suffix that starts inside it can be a whole record on its own.
fn glued_record(line: &[u8]) -> Option<ReportRecord> {
    line.iter().enumerate().skip(1).filter(|(_, b)| **b == b'{')
        .find_map(|(i, _)| serde_json::from_slice::<ReportRecord>(&line[i..]).ok())
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

fn response(scope: &Path, request: Value, initialized: &mut bool) -> Option<Value> {
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
            let requested = request.pointer("/params/protocolVersion").and_then(Value::as_str).unwrap_or("");
            let protocol = match requested {
                "2024-11-05" | "2025-03-26" | "2025-06-18" | "2025-11-25" => requested,
                _ => "2025-11-25",
            };
            json!({"protocolVersion":protocol,"capabilities":{"tools":{}},"serverInfo":{"name":SERVER_NAME,"version":"1.0.0"}})
        }
        "ping" => json!({}),
        _ if !*initialized => return Some(error(id, -32002, "Initialize before reporting")),
        "tools/list" => tools(),
        "tools/call" => {
            let Some(name) = request.pointer("/params/name").and_then(Value::as_str) else {
                return Some(error(id, -32602, "A tool name is required"));
            };
            let args = request.pointer("/params/arguments").cloned().unwrap_or_else(|| json!({}));
            match call(scope, name, args) {
                Ok(value) => json!({"content":[{"type":"text","text":value.to_string()}],"isError":false}),
                Err(why) => json!({"content":[{"type":"text","text":why}],"isError":true}),
            }
        }
        _ => return Some(error(id, -32601, "Method not found")),
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}

/// Did the lead's child load this tool? From its own init inventory.
pub fn loaded_from_init(init: &Value) -> bool {
    init.get("tools").and_then(Value::as_array)
        .is_some_and(|tools| tools.iter().any(|tool| tool.as_str() == Some(QUALIFIED_REPORT_TOOL)))
}

/// Newline-delimited JSON-RPC with bounded allocation, the same shape as `status_tools.rs`.
/// An oversized message is drained through its newline, rejected, and cannot be read as the
/// command that follows it.
pub fn serve(scope_path: &Path, mut reader: impl BufRead, mut writer: impl Write) -> io::Result<()> {
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
                Ok(value) => response(scope_path, value, &mut initialized),
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
pub fn run_stdio(scope_path: &Path) -> io::Result<()> {
    serve(scope_path, io::stdin().lock(), io::stdout().lock())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The tool says what the host does** (Sage's contract notes §3 item 1, 2026-09-26: the
    /// host settles only `outcome` and `failed`, and a lead following the old wording of
    /// `answer` would have left his question open). The contract's sentence and this
    /// description must agree.
    #[test]
    fn the_description_says_only_outcome_or_failed_closes_a_handle_and_answer_closes_nothing() {
        let tools = tools();
        let description = tools["tools"][0]["description"].as_str().unwrap();
        assert!(description.contains("Only `outcome` or `failed` on a handle closes it."), "{description}");
        assert!(description.contains("it closes nothing"), "{description}");
        assert!(description.contains("closed with `outcome`"), "{description}");
        assert!(description.contains("An `outcome` that names a land Git cannot confirm closes nothing"), "{description}");
    }
    use crate::assignment::{self, AssignmentKind, Registration};
    use std::process::Command;

    struct Fixture {
        root: PathBuf,
        scope_path: PathBuf,
        scope: ReportScope,
        repo: PathBuf,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_dir_all(&self.root) { eprintln!("fixture cleanup: {error}"); }
        }
    }

    /// Git for the fixtures, isolated from the machine's own configuration and hooks.
    fn git(dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(["-c", "core.hooksPath=/dev/null", "-c", "commit.gpgSign=false",
                   "-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid",
                   "-c", "init.defaultBranch=main"])
            .args(args).current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null").env("GIT_CONFIG_NOSYSTEM", "1")
            .output().unwrap();
        assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    fn commit(repo: &Path, file: &str) -> String {
        std::fs::write(repo.join(file), file).unwrap();
        git(repo, &["add", file]);
        git(repo, &["commit", "-q", "-m", file]);
        git(repo, &["rev-parse", "HEAD"])
    }

    fn fixture() -> Fixture {
        let root = std::env::temp_dir().join(format!("operator-report-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let root = std::fs::canonicalize(&root).unwrap();
        let ab = root.join("ab");
        let repo = ab.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q"]);
        commit(&repo, "first");
        let scope = ReportScope {
            question_context: None,
            desk: None,
            version: 1,
            outbox: root.join("lead/outbox.jsonl"),
            attachments: root.join("lead/attachments"),
            file_roots: vec![ab],
            state_root: root.join("engine-state"),
            entity_id: "femcboost".into(),
            thread_id: "thread-a".into(),
            lead: "claim-1".into(),
        };
        let scope_path = root.join("lead/report-scope.json");
        std::fs::create_dir_all(scope_path.parent().unwrap()).unwrap();
        std::fs::write(&scope_path, serde_json::to_string(&scope).unwrap()).unwrap();
        Fixture { root, scope_path, scope, repo }
    }

    fn register(f: &Fixture, thread: &str) -> String {
        assignment::register_kind(&f.scope.state_root, &Registration {
            entity_id: "femcboost".into(),
            thread_id: thread.into(),
            obligation_id: format!("obligation-{}", uuid::Uuid::new_v4()),
            instruction_ledger_ref: format!("ledger:{thread}:turn-1"),
            instruction_sha256: "a".repeat(64),
            title: "Fixture work".into(),
            repositories: vec![],
            needs_screen: false,
        }, AssignmentKind::Task).unwrap().id
    }

    fn report(f: &Fixture, args: Value) -> Result<Value, String> {
        call(&f.scope_path, REPORT_TOOL_NAME, args)
    }

    fn outbox(f: &Fixture) -> Vec<ReportRecord> {
        read_outbox(&f.scope.outbox).unwrap()
    }

    #[test]
    fn prepared_question_requires_runtime_scope_and_uses_shared_store() {
        let mut f=fixture();
        let args=json!({"kind":"question","text":"Release timing", "questions":[{"text":"When should the release ship?","options":[{"label":"Ship today","description":"Earlier fixes"},{"label":"Ship tomorrow","description":"More testing"}]}]});
        assert!(report(&f,args.clone()).unwrap_err().contains("runtime"));
        f.scope.question_context=Some(crate::questions::AskScope{ obligation_id:None,root:f.scope.state_root.clone(),entity_id:f.scope.entity_id.clone(),thread_id:f.scope.thread_id.clone(),turn_id:"operator-turn".into(),asker:"unused".into(),session_id:"original-lead-session".into(),engine:None,entity_root:None,app_run:None});
        write_scope(&f.scope_path,&f.scope).unwrap();
        let receipt=report(&f,args.clone()).unwrap();
        assert_eq!(receipt["recorded"],true);
        assert!(outbox(&f).is_empty(),"question was duplicated as an ordinary report");
        let questions=crate::questions::Store::new(&f.scope.state_root).all().unwrap();
        assert_eq!(questions.len(),1);assert_eq!(questions[0].asker,"operator:conversation");assert_eq!(questions[0].session_id,"original-lead-session");
        let withdraw=json!({"kind":"withdraw_question","question_id":questions[0].id,"text":"The release was canceled"});
        assert_eq!(report(&f,withdraw).unwrap()["withdrawn"],true);
        let saved=crate::questions::Store::new(&f.scope.state_root).all().unwrap();
        assert_eq!(saved[0].state,crate::questions::State::Withdrawn);
        assert_eq!(saved[0].withdrawal_reason.as_deref(),Some("The release was canceled"));
        assert!(report(&f,json!({"kind":"withdraw_question","question_id":"someone-elses-question","text":"Done"})).is_err());
        f.scope.question_context.as_mut().unwrap().thread_id="other-thread".into();
        write_scope(&f.scope_path,&f.scope).unwrap();
        assert!(report(&f,args).unwrap_err().contains("scope"));
    }

    /// The shipped engine, and a seat whose record rules one subject (the splash screens), as
    /// the engine's own ceo-ruled suite writes a ruling.
    fn with_ruled_engine(f: &mut Fixture) -> PathBuf {
        let engine = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../engine").canonicalize().unwrap();
        let seat = f.root.join("seat");
        std::fs::create_dir_all(seat.join("wiki")).unwrap();
        std::fs::write(seat.join("orchestration.config"), "CEO_RULINGS_PATHS=\"wiki/ceo-decisions.md\"\n").unwrap();
        std::fs::write(seat.join("wiki/ceo-decisions.md"), "# Decisions\n\n## 21. The start screen (CEO, 2026-09-01)\n\n\
### The splash screens — TWO, and the order is DETERMINISTIC (CEO, 2026-09-01)\n\n\
**His words:** *\"I have never approved more than 2 splash screens.\"*\n").unwrap();
        f.scope.question_context = Some(crate::questions::AskScope { obligation_id:None,
            root: f.scope.state_root.clone(), entity_id: f.scope.entity_id.clone(), thread_id: f.scope.thread_id.clone(),
            turn_id: "operator-turn".into(), asker: "operator:conversation".into(), session_id: "lead-session-1".into(),
            engine: Some(engine.clone()), entity_root: Some(seat.clone()), app_run: None,
        });
        write_scope(&f.scope_path, &f.scope).unwrap();
        engine
    }

    /// **F3 (operator contract notes §4 item 1):** every route by which his lead puts a
    /// question to him goes through the settled-by-his-words check. A prepared question card
    /// already did (`question_engine::check`, since 0413ea69); a question written into a
    /// report's own words reached him unchecked.
    #[test]
    fn a_question_in_a_report_s_words_goes_through_the_settled_by_his_words_check() {
        let mut f = fixture();
        let engine = with_ruled_engine(&mut f);
        let ruled = "Landed the fix.\n\nWhich splash screens should ship in v1, all seven or your two?";
        // The prepared card: refused by the guard itself.
        let card = report(&f, json!({"kind": "question", "text": "Splash screens", "questions": [{
            "text": "Which splash screens should ship in v1?\npremise-unverified: a fixture question with no deadline.",
            "options": [{"label": "All seven", "description": "Every variation"}, {"label": "Your two", "description": "The approved pair"}]}]}));
        assert!(card.as_ref().is_err_and(|e| e.contains("ALREADY RULED")), "{card:?}");
        // The same question in the words of every other kind: refused, naming and quoting the
        // ruling, and nothing written.
        for kind in ["update", "answer", "outcome", "failed"] {
            let refused = report(&f, json!({"kind": kind, "text": ruled})).unwrap_err();
            assert!(refused.contains("§21 › The splash screens") && refused.contains("never approved more than 2"),
                    "{kind}: {refused}");
            assert!(refused.contains("ceo-ruled-exempt.sh lead-session-1"), "{kind}: the way through names his session: {refused}");
        }
        assert!(outbox(&f).is_empty(), "a refused report wrote nothing");
        // Words that ask nothing ruled, and words that ask nothing at all, are recorded.
        report(&f, json!({"kind": "update", "text": "Landed the fix.\n\nShould I start on the release notes next?"})).unwrap();
        report(&f, json!({"kind": "update", "text": "Landed the splash screens fix."})).unwrap();
        assert_eq!(outbox(&f).len(), 2);
        // The way through is the engine's own exemption, per session and per cite.
        let exempt = Command::new("bash")
            .arg(engine.join("scripts/ceo-ruled-exempt.sh"))
            .args(["lead-session-1", "§21 › The splash screens",
                   "the ruling covers which screens ship; this asks about their order on a relaunch"])
            .current_dir(f.root.join("seat")).env("RICHOS_ENTITY_ROOT", f.root.join("seat"))
            .output().unwrap();
        assert!(exempt.status.success(), "{}", String::from_utf8_lossy(&exempt.stderr));
        report(&f, json!({"kind": "update", "text": ruled})).unwrap();
        assert_eq!(outbox(&f).len(), 3, "an exempted question is recorded");
    }

    #[test]
    fn the_asking_window_is_the_terminal_prose_notice_s() {
        use crate::question_engine::asking_paragraphs;
        let notice = std::fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../engine/scripts/hooks/notice-ceo-ruled-prose.sh")).unwrap();
        assert!(notice.contains(r#"ASK = re.compile(r"\?|\*\*Options?:|\*\*Decision:", re.I)"#)
                && notice.contains(r#"re.split(r"\n\s*\n", msg)"#) && notice.contains("[:6000]"),
                "the engine's prose window changed; change asking_paragraphs with it");
        assert_eq!(asking_paragraphs("Landed it.\n\nShip today?\nor tomorrow"), "Ship today?\nor tomorrow");
        assert_eq!(asking_paragraphs("One.\n  \n**Options:** A or B\n\nTwo."), "**Options:** A or B");
        assert_eq!(asking_paragraphs("**decision:** yours\n\n\n\nWhy?"), "**decision:** yours\nWhy?");
        assert_eq!(asking_paragraphs("Nothing asked here."), "");
        assert_eq!(asking_paragraphs(&format!("{}?", "x".repeat(7000))).chars().count(), 6000);
    }

    // ---- the tool, as the lead sees it ---------------------------------------------------

    /// A desk that records the stops it is asked for and reports each named agent stopped.
    #[derive(Default)]
    struct StopsSeen(std::sync::Mutex<Vec<(Vec<String>, String)>>);
    impl crate::operator_desk_tools::DeskService for StopsSeen {
        fn stop(&self, names: &[String], words: &str, _: Option<&str>) -> Vec<crate::operator_host::StopResult> {
            self.0.lock().unwrap().push((names.to_vec(), words.to_string()));
            names.iter().map(|n| crate::operator_host::StopResult::Stopped { name: n.clone(), seconds: 0.5, registry: Ok(()) })
                .collect()
        }
        fn read(&self, _: &crate::operator_host::ConversationKey, _: bool) -> Vec<crate::operator_host::ConversationRead> {
            Vec::new()
        }
        fn interrupt(&self, _: &crate::operator_host::ConversationKey) -> String {
            "interrupted".into()
        }
    }

    /// **F6 (operator contract notes §4 item 3):** a stop he gives in a conversation whose lead
    /// does not own the agent reached that lead as words, and `TaskStop` reaches only its own
    /// agents, so he had to say it again to the right one. The lead's own `stop` goes to the
    /// desk's `stop_named`, which finds the agent in whichever conversation runs it.
    #[test]
    fn the_lead_s_own_stop_reaches_the_desk_with_his_words_and_says_what_was_measured() {
        let mut f = fixture();
        let seen = std::sync::Arc::new(StopsSeen::default());
        let dir = PathBuf::from("/tmp").join(format!("rol-{}", &uuid::Uuid::new_v4().simple().to_string()[..12]));
        std::fs::create_dir_all(&dir).unwrap();
        let socket_path = dir.join("desk.sock");
        let (desk_token, lead_token) = (crate::operator_desk_tools::new_token(), crate::operator_desk_tools::new_token());
        let socket = crate::operator_desk_tools::DeskSocket::serve_with_lead(seen.clone(), &socket_path, &desk_token,
                                                                             Some(&lead_token)).unwrap();
        let stop = json!({"names": ["echo-sonnet-b"], "words": "stop echo-sonnet-b"});
        // No desk in this launch: nothing is stopped, and the lead is told so.
        let none = call(&f.scope_path, STOP_TOOL_NAME, stop.clone()).unwrap_err();
        assert!(none.contains("nothing was stopped"), "{none}");
        f.scope.desk = Some(LeadDesk { socket: socket_path.clone(), token: lead_token });
        write_scope(&f.scope_path, &f.scope).unwrap();
        let said = call(&f.scope_path, STOP_TOOL_NAME, stop).unwrap();
        assert_eq!(said["stopped"], json!(["echo-sonnet-b"]), "{said}");
        assert_eq!(said["say"], "Stopped echo-sonnet-b.");
        // No names, or no words of his: refused before the desk hears anything.
        assert!(call(&f.scope_path, STOP_TOOL_NAME, json!({"names": [], "words": "stop"})).is_err());
        assert!(call(&f.scope_path, STOP_TOOL_NAME, json!({"names": ["a"], "words": " "})).is_err());
        assert_eq!(*seen.0.lock().unwrap(), [(vec!["echo-sonnet-b".to_string()], "stop echo-sonnet-b".to_string())]);
        assert!(outbox(&f).is_empty(), "a stop is not a report to him");
        socket.close();
        if let Err(e) = std::fs::remove_dir_all(&dir) { eprintln!("fixture cleanup: {e}"); }
    }

    #[test]
    fn the_server_offers_report_and_the_lead_s_own_stop() {
        let listed = tools();
        let tools = listed["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 2);
        assert_eq!(tools[1]["name"], STOP_TOOL_NAME);
        assert_eq!(QUALIFIED_STOP_TOOL, format!("mcp__{SERVER_NAME}__{STOP_TOOL_NAME}"));
        assert_eq!(tools[1]["inputSchema"]["required"], json!(["names", "words"]));
        let says = tools[1]["description"].as_str().unwrap();
        assert!(says.contains("another conversation's") && says.contains("verbatim") && says.contains("TaskStop"), "{says}");
        assert_eq!(tools[0]["name"], REPORT_TOOL_NAME);
        let kinds = &tools[0]["inputSchema"]["properties"]["kind"]["enum"];
        assert_eq!(kinds, &json!(KINDS));
        assert_eq!(tools[0]["inputSchema"]["required"], json!(["kind", "text"]));
        assert!(loaded_from_init(&json!({"tools": [QUALIFIED_REPORT_TOOL]})));
        assert!(!loaded_from_init(&json!({"tools": []})));
    }

    #[test]
    fn a_plain_report_is_appended_verbatim_to_this_lead_s_outbox() {
        let f = fixture();
        report(&f, json!({"kind": "update", "text": "The build is green."})).unwrap();
        report(&f, json!({"kind": "update", "text": "Preparing the release", "agents": ["mark-opus-x1"]})).unwrap();
        let records = outbox(&f);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].kind, "update");
        assert_eq!(records[0].text, "The build is green.");
        assert_eq!((records[0].lead.as_str(), records[0].thread_id.as_str()), ("claim-1", "thread-a"));
        assert_eq!(records[0].handle, None);
        assert_eq!(records[1].agents, ["mark-opus-x1"]);
    }

    /// **An interrupted write never blocks the reports after it** (hunt part 1 finding 44). A
    /// process that ends mid-append, or a disk that fills, leaves a partial record with no
    /// newline, possibly cut inside a multibyte character. Its writer was never told
    /// "recorded", so it is not a report. The next append starts a fresh line instead of
    /// gluing its record onto the fragment, and the reader keeps every whole record around
    /// the damaged line, in order.
    #[test]
    fn an_interrupted_append_does_not_block_the_reports_after_it() {
        // Two cuts: inside an ASCII word, and inside the two bytes of "é" (C3 A9).
        let whole = br#"{"kind":"outcome","text":"Caf\xC3\xA9 is open"}"#;
        let torn_ascii: &[u8] = &whole[..12];
        let torn_utf8: Vec<u8> = [&br#"{"kind":"outcome","text":"Caf"#[..], &[0xC3]].concat();
        for torn in [torn_ascii.to_vec(), torn_utf8] {
            let f = fixture();
            report(&f, json!({"kind": "update", "text": "Before the interruption."})).unwrap();
            {
                use std::io::Write as _;
                let mut file = std::fs::OpenOptions::new().append(true).open(&f.scope.outbox).unwrap();
                file.write_all(&torn).unwrap();
            }
            // The fragment has no newline: it may still be an append in progress, so it is
            // not read, and it does not stop the record before it from being read.
            let texts = |f: &Fixture| outbox(f).into_iter().map(|r| r.text).collect::<Vec<_>>();
            assert_eq!(texts(&f), ["Before the interruption."], "torn {torn:?}");
            report(&f, json!({"kind": "update", "text": "After the interruption."})).unwrap();
            assert_eq!(texts(&f), ["Before the interruption.", "After the interruption."], "torn {torn:?}");
            // A second read sees the same records at the same positions: the host's cursor
            // and its owed closes are record indexes.
            assert_eq!(texts(&f), ["Before the interruption.", "After the interruption."], "torn {torn:?}");
            let bytes = std::fs::read(&f.scope.outbox).unwrap();
            let lines: Vec<&[u8]> = bytes.split(|b| *b == b'\n').collect();
            assert_eq!(lines.len(), 4, "the fragment ends its own line; {lines:?}");
            assert_eq!(lines[1], &torn[..], "the fragment is left as it was, never rewritten");
            assert!(lines[3].is_empty(), "the outbox ends at a line boundary");
        }
        // An outbox the old appender already damaged: the next record was written straight
        // onto the fragment. That record's lead was told "recorded", so it is read.
        let f = fixture();
        report(&f, json!({"kind": "update", "text": "Glued record."})).unwrap();
        let glued_record = std::fs::read(&f.scope.outbox).unwrap();
        let mut damaged = torn_ascii.to_vec();
        damaged.extend_from_slice(&glued_record);
        std::fs::write(&f.scope.outbox, &damaged).unwrap();
        report(&f, json!({"kind": "update", "text": "Next record."})).unwrap();
        let texts: Vec<String> = outbox(&f).into_iter().map(|r| r.text).collect();
        assert_eq!(texts, ["Glued record.", "Next record."]);
        // A whole line that is JSON but not a record is not a torn write: it still fails.
        std::fs::write(&f.scope.outbox, b"{\"kind\":\"update\"}\n").unwrap();
        assert!(read_outbox(&f.scope.outbox).is_err());
    }

    #[test]
    fn malformed_reports_are_refused_and_nothing_is_written() {
        let f = fixture();
        for (args, needle) in [
            (json!({"kind": "done", "text": "x"}), "kind"),
            (json!({"kind": "update", "text": "   "}), "text"),
            (json!({"kind": "update"}), "text"),
            (json!({"kind": "update", "text": "x", "files": ["relative.txt"]}), "absolute"),
            (json!({"kind": "update", "text": "x", "files": [f.root.join("ab/missing.txt")]}), "does not exist"),
            (json!({"kind": "update", "text": "x", "files": ["/etc/hosts"]}), "outside"),
            (json!({"kind": "update", "text": "x", "lands": [{"repository": "/etc", "commit": "a".repeat(40)}]}), "outside"),
            (json!({"kind": "update", "text": "x", "lands": [{"repository": f.repo, "commit": "HEAD"}]}), "commit"),
            (json!({"kind": "update", "text": "x", "lands": [{"repository": f.repo, "commit": "a".repeat(40), "into": "-x"}]}), "branch"),
            (json!({"kind": "update", "text": "x", "agents": [""]}), "agent"),
            (json!({"kind": "update", "text": "x", "surprise": 1}), "surprise"),
        ] {
            let why = report(&f, args.clone()).unwrap_err();
            assert!(why.contains(needle), "{args}: {why:?} does not name {needle:?}");
        }
        assert!(!f.scope.outbox.exists() || outbox(&f).is_empty(), "a refused report wrote something");
        assert!(call(&f.scope_path, "stop", json!({})).is_err(), "this server has one tool");
    }

    #[test]
    fn a_handle_must_be_an_assignment_of_this_conversation() {
        let f = fixture();
        let ours = register(&f, "thread-a");
        let theirs = register(&f, "thread-b");
        report(&f, json!({"handle": ours, "kind": "outcome", "text": "Done."})).unwrap();
        assert_eq!(outbox(&f)[0].handle.as_deref(), Some(ours.as_str()));
        let why = report(&f, json!({"handle": theirs, "kind": "outcome", "text": "Done."})).unwrap_err();
        assert!(why.contains("not an assignment of this conversation"), "{why}");
        assert!(report(&f, json!({"handle": "made-up", "kind": "update", "text": "x"})).is_err());
        assert_eq!(outbox(&f).len(), 1);
    }

    // ---- lands: checked in Git before anything says "landed" ----------------------------

    #[test]
    fn a_land_on_main_is_confirmed_and_says_so() {
        let f = fixture();
        git(&f.repo, &["checkout", "-q", "-b", "mark-opus-x1"]);
        let tip = commit(&f.repo, "feature");
        git(&f.repo, &["checkout", "-q", "main"]);
        git(&f.repo, &["merge", "-q", "--no-ff", "-m", "Merge mark-opus-x1", "mark-opus-x1"]);
        let answer = report(&f, json!({"kind": "outcome", "text": "Landed.",
            "lands": [{"repository": f.repo, "commit": tip, "branch": "mark-opus-x1"}]})).unwrap();
        let land = &outbox(&f)[0].lands[0];
        assert!(land.landed, "{land:?}");
        assert_eq!(land.into, "main");
        assert_eq!(land.pushed, None, "no upstream, so pushed is not decided");
        assert!(land.says.starts_with("Landed mark-opus-x1 in repo"), "{}", land.says);
        assert!(answer.to_string().contains("Confirmed in Git: Landed mark-opus-x1"), "{answer}");
    }

    #[test]
    fn a_false_land_is_recorded_as_could_not_be_confirmed_and_the_lead_is_told() {
        let f = fixture();
        git(&f.repo, &["checkout", "-q", "-b", "never-merged"]);
        let tip = commit(&f.repo, "orphan");
        git(&f.repo, &["checkout", "-q", "main"]);
        let answer = report(&f, json!({"kind": "outcome", "text": "Landed it.",
            "lands": [{"repository": f.repo, "commit": tip, "branch": "never-merged"}]})).unwrap();
        let records = outbox(&f);
        assert_eq!(records.len(), 1, "his lead's words are kept even when a claim in them is wrong");
        let land = &records[0].lands[0];
        assert!(!land.landed);
        assert!(land.says.contains("could not be confirmed as landed"), "{}", land.says);
        assert!(answer.to_string().contains("could not be confirmed"), "{answer}");
        // A commit Git has never seen.
        report(&f, json!({"kind": "outcome", "text": "x",
            "lands": [{"repository": f.repo, "commit": "b".repeat(40)}]})).unwrap();
        assert!(!outbox(&f)[1].lands[0].landed);
        // A real commit that is not on main, named WITHOUT its branch, so only the ancestry
        // question can catch it (a mutation that skipped that question survived without it).
        report(&f, json!({"kind": "outcome", "text": "x",
            "lands": [{"repository": f.repo, "commit": tip}]})).unwrap();
        let land = &outbox(&f)[2].lands[0];
        assert!(!land.landed, "{land:?}");
        assert!(land.why.as_deref().unwrap_or("").contains("is not on main"), "{land:?}");
    }

    /// **Q3 (c) / F5, the lead's half.** An `outcome` on a handle that names a land Git could
    /// not confirm closes nothing (the host's rule), and the tool's answer says so, with what
    /// to do next. The same land on an `update`, or an `outcome` with no handle, closes
    /// nothing anyway, so it is not said there.
    #[test]
    fn an_outcome_on_a_handle_with_an_unconfirmed_land_tells_the_lead_it_stays_open() {
        let f = fixture();
        let handle = register(&f, "thread-a");
        let missing = json!([{"repository": f.repo, "commit": "b".repeat(40)}]);
        let answer = report(&f, json!({"kind": "outcome", "text": "Landed.", "handle": handle, "lands": missing})).unwrap();
        let says = answer["says"].as_str().unwrap();
        assert!(says.contains("does not close the assignment; it stays open.")
                && says.contains("report `outcome` again") && says.contains("or report `failed`"), "{says}");
        for other in [json!({"kind": "update", "text": "x", "handle": handle, "lands": missing}),
                      json!({"kind": "outcome", "text": "x", "lands": missing})] {
            let answer = report(&f, other).unwrap();
            assert!(!answer["says"].as_str().unwrap().contains("stays open"), "{answer}");
        }
    }

    #[test]
    fn a_branch_that_still_has_unlanded_work_is_not_called_landed() {
        let f = fixture();
        git(&f.repo, &["checkout", "-q", "-b", "half"]);
        let first = commit(&f.repo, "one");
        git(&f.repo, &["checkout", "-q", "main"]);
        git(&f.repo, &["merge", "-q", "--no-ff", "-m", "Merge half", "half"]);
        git(&f.repo, &["checkout", "-q", "half"]);
        commit(&f.repo, "two, never landed");
        git(&f.repo, &["checkout", "-q", "main"]);
        report(&f, json!({"kind": "outcome", "text": "x",
            "lands": [{"repository": f.repo, "commit": first, "branch": "half"}]})).unwrap();
        let land = &outbox(&f)[0].lands[0];
        assert!(!land.landed, "{land:?}");
        assert!(land.why.as_deref().unwrap_or("").contains("half"), "{land:?}");
    }

    #[test]
    fn pushed_is_read_off_the_upstream_locally() {
        let f = fixture();
        let remote = f.root.join("remote.git");
        git(&f.root, &["init", "-q", "--bare", remote.to_str().unwrap()]);
        git(&f.repo, &["remote", "add", "origin", remote.to_str().unwrap()]);
        git(&f.repo, &["push", "-q", "-u", "origin", "main"]);
        let pushed = git(&f.repo, &["rev-parse", "HEAD"]);
        let local = commit(&f.repo, "not pushed yet");
        report(&f, json!({"kind": "outcome", "text": "x", "lands": [
            {"repository": f.repo, "commit": pushed}, {"repository": f.repo, "commit": local}]})).unwrap();
        let lands = &outbox(&f)[0].lands;
        assert_eq!((lands[0].landed, lands[0].pushed), (true, Some(true)));
        assert_eq!((lands[1].landed, lands[1].pushed), (true, Some(false)));
        assert!(lands[0].says.contains("pushed"), "{}", lands[0].says);
        assert!(lands[1].says.contains("not pushed"), "{}", lands[1].says);
    }

    // ---- long text -----------------------------------------------------------------------

    #[test]
    fn long_text_is_bounded_in_the_notice_and_kept_whole_in_an_attached_file() {
        let f = fixture();
        let long: String = (0..2000).map(|i| format!("line {i} ")).collect();
        assert!(long.chars().count() > NOTICE_CHARS);
        report(&f, json!({"kind": "outcome", "text": long})).unwrap();
        let record = &outbox(&f)[0];
        assert!(record.text.ends_with(ATTACHED_SUFFIX), "{}", &record.text[record.text.len() - 60..]);
        // The register's own bound, its ellipsis, a blank line and the suffix.
        assert!(record.text.chars().count() <= NOTICE_CHARS + 1 + 2 + ATTACHED_SUFFIX.len());
        let attached = record.attachment.as_ref().expect("the whole text is attached");
        assert!(attached.starts_with(&f.scope.attachments));
        assert_eq!(std::fs::read_to_string(attached).unwrap(), long);
        report(&f, json!({"kind": "update", "text": "short"})).unwrap();
        assert_eq!(outbox(&f)[1].attachment, None);
    }

    // ---- the wire ------------------------------------------------------------------------

    #[test]
    fn the_stdio_server_speaks_json_rpc_and_refuses_before_initialize() {
        let f = fixture();
        let input = [
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}).to_string(),
            json!({"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}).to_string(),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}).to_string(),
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"report","arguments":{"kind":"update","text":"hi"}}}).to_string(),
            "x".repeat(MAX_FRAME_BYTES + 10),
        ].join("\n") + "\n";
        let mut out = Vec::new();
        serve(&f.scope_path, input.as_bytes(), &mut out).unwrap();
        let replies: Vec<Value> = String::from_utf8(out).unwrap().lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(replies.len(), 4, "{replies:?}");
        assert_eq!(replies[0]["error"]["code"], -32002);
        assert_eq!(replies[1]["result"]["serverInfo"]["name"], SERVER_NAME);
        assert_eq!(replies[2]["result"]["isError"], false);
        assert_eq!(replies[3]["error"]["message"], "Message too large");
        assert_eq!(outbox(&f).len(), 1);
    }

    #[test]
    fn a_missing_or_foreign_scope_refuses_everything() {
        let f = fixture();
        let why = call(&f.root.join("no-scope.json"), REPORT_TOOL_NAME, json!({"kind":"update","text":"x"})).unwrap_err();
        assert!(why.contains("scope"), "{why}");
        std::fs::write(&f.scope_path, "{\"version\":2}").unwrap();
        assert!(call(&f.scope_path, REPORT_TOOL_NAME, json!({"kind":"update","text":"x"})).is_err());
        let bad = ReportScope { outbox: PathBuf::from("relative.jsonl"), ..f.scope.clone() };
        assert!(write_scope(&f.scope_path, &bad).is_err());
        write_scope(&f.scope_path, &f.scope).unwrap();
        assert!(report(&f, json!({"kind":"update","text":"x"})).is_ok());
    }
}

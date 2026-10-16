//! THE OPERATOR HOST — one lead per conversation, relaying and never holding (operator back-end
//! spec r3 §4 (g), (c), (d), (o), (q), (r), (s), e4, with r4 §1-§3; richos-hq
//! `docs/plans/2026-09-25-operator-back-end-spec-r4.md`).
//!
//! **What it is for, in his words.** Two Riches: *"each conversation thread always holds one
//! front desk Rich and one back-end Rich. Regardless of the number of assignments"*, and his
//! choice for this Mac (§86): *"Your setup as-is"*. So each conversation given work gets its own
//! lead — his `claude`, seated in his entity with his rules — and this host is the only thing
//! between the front desk and it.
//!
//! **The rules, each a test in this file:**
//!
//! 1. **Relay, never hold ((g)).** A message is written to the lead the moment it arrives; the
//!    CLI queues it and takes it after the running turn (P13's control). There is no host queue
//!    and no "one assignment at a time" — that is the product runner's shape, not his.
//! 2. **Only a report settles an assignment ((c)).** A turn ending never does. An `outcome` or
//!    `failed` report on a handle closes that obligation through the engine's `operator-complete`
//!    verb ([`Settle`]), with every land already checked in Git by the report server.
//! 3. **His lead's own words reach him ((c), B5 edge).** A turn's final text is delivered —
//!    on the handle whose message started the turn, or on the conversation for a turn the
//!    platform started — unless it is the text of that turn's last report.
//! 4. **Phone words never become work (s), and answers are not phone words (§88).** An
//!    assignment from an origin the declaration does not list is not relayed; a turn with no
//!    origin relays nothing; a stop is accepted from every channel; and an answer to his team's
//!    question reaches the lead from any channel through [`OperatorHost::deliver_answer`], the
//!    seam PRD S6's question store calls.
//! 5. **A named stop stops the name and nothing else ((d), §67),** then tells his registry
//!    (r4 §2.1, item 3a) once the stream says `stopped` AND `agent-liveness.sh` says NOT-ALIVE.
//! 6. **Nothing but a positive signal ends a lead ((q)).** Errors are reported and the lead is
//!    kept; an end not caused by quit or retirement is logged with its cause.
//! 7. **An alarm reaches him once, across every lead ((r)).**
//!
//! **What it deliberately does not do.** It never sets `priority` (r4 §1.3), never closes a
//! lead's input while an agent of it is alive (r4 §1.2), never re-dispatches anything after a
//! crash (reference ledger row §2.6), and never decides the land: the lead and his engine do.
use crate::assignment::{self, AssignmentState, NoticeKind};
use crate::operator_declaration::Declaration;
use crate::operator_frames::{banner_in, Alarm, AlarmDeduper};
use crate::operator_lead::{InterruptReply, LeadError, LeadEvent, LeadSink, Quit, TaskBook, TaskStatus, TurnEnd};
use crate::operator_profile::{init_check, EngineBanner, InitVerdict, LeadStart};
use crate::operator_report::{read_outbox, ReportRecord};
use crate::operator_snapshot::snapshot_digest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// r3 (q) item 4: a lead with nothing running is retired after this long idle.
pub const LEAD_IDLE: Duration = Duration::from_secs(30 * 60);
/// r3 (d) item 3: "Still ALIVE after 10 s: he hears so and the stop is sent once more."
pub const STOP_WAIT: Duration = Duration::from_secs(10);
/// (s) rule 1, verbatim from r3.
pub const PHONE_ASSIGNMENT: &str = "Your team only takes work from the Mac. I've noted it; ask me at your desk.";
/// How many of a conversation's last turn texts the front desk's read shows ((o)).
pub const LAST_TEXTS: usize = 3;
/// The ECS store's status for a withdrawn obligation, which only an answer can close (zach's
/// `operator-complete` contract §2.5). It is the store's protocol value, spelled as the store
/// spells it.
const ECS_WITHDRAWN: &str = "cancelled"; // dialect-exempt: the ECS store's own protocol literal, engine-rest-2026-09-25.md §2.5
/// Said after an `outcome` whose land Git could not confirm (Frank's Q3 (c)).
pub const UNCONFIRMED_STAYS_OPEN: &str =
    "Your team reported this done, but a land it named could not be confirmed, so it stays open.";
/// Said after a report on an assignment a report already closed (Frank's F5).
pub const ALREADY_CLOSED: &str = "Your team reported this again, but it was already closed, so nothing changed.";
/// The engine's refusal of an obligation that is not open (`engine/ecs/adapters/app.py`
/// `operator_complete`: "only an open assignment can be closed; <id> is <status>").
const ENGINE_NOT_OPEN: &str = "only an open assignment can be closed";

// =============================================================================================
// identities and channels
// =============================================================================================

/// One conversation, as the ledger bound it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConversationKey {
    pub entity_id: String,
    pub thread_id: String,
}

/// Where the words came from ((s)). The declaration's `origins` lists which may give work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    DeskTyped,
    DeskVoice,
    /// **Reserved: nothing produces it yet** (Frank's review, question 3 (b)). A file dropped
    /// at the desk reaches the ledger as a typed turn, so [`Origin::of_turn`] reads it as
    /// [`Origin::DeskTyped`]. Every declaration lists `desk-file` beside `desk-typed`
    /// (`operator_declaration.py` `ORIGINS`), so no behavior differs; only the log's label
    /// would. A log line saying `DeskTyped` is no proof a file drop was told apart from typing.
    DeskFile,
    Phone,
    /// A mouth this build has never heard of, recorded on the turn by its intake record (r3
    /// (s): *"any future origin is unlisted until declared"*). Refused as work with the same
    /// sentence as the phone, because it is not the Mac.
    Undeclared,
    /// A notice, a proactive turn, an internal re-prime, or a turn whose mouth was never
    /// recorded: no intake origin, (s) rule 4. It relays nothing.
    NoOrigin,
}

impl Origin {
    /// The declaration's word for this origin.
    pub fn declared(self) -> Option<&'static str> {
        match self {
            Self::DeskTyped => Some("desk-typed"),
            Self::DeskVoice => Some("desk-voice"),
            Self::DeskFile => Some("desk-file"),
            Self::Phone => Some("phone"),
            Self::Undeclared => Some("undeclared"),
            Self::NoOrigin => None,
        }
    }

    /// **Where a front-desk turn's words came from, read off the ledger** (r3 (s); the
    /// operator-client record's §7 item 3: *"so the host receives `Origin` instead of the
    /// walk handing it in"*). `channel` is `Turn::channel`, which a spine on an operator
    /// install records for every sentence of his (`spine::DESK_CHANNEL` or the intake
    /// record's own mouth).
    ///
    /// **An unrecorded mouth is no origin, never the desk.** A turn recorded before keeping
    /// began, by a build that did not keep it, carries `None`; reading that as "typed at the
    /// Mac" would let a phone sentence from yesterday become work today. Internal and
    /// proactive turns carry no words of his and are no origin whatever they carry.
    pub fn of_turn(source: crate::ledger::Source, channel: Option<&str>) -> Origin {
        use crate::ledger::Source;
        match (source, channel) {
            (Source::Internal | Source::Proactive | Source::Agent, _) | (_, None) => Origin::NoOrigin,
            (_, Some("phone" | "phone_typed" | "phone_voice")) => Origin::Phone,
            (Source::Text, Some(crate::spine::DESK_CHANNEL)) => Origin::DeskTyped,
            (Source::Jam, Some(crate::spine::DESK_CHANNEL)) => Origin::DeskVoice,
            (_, Some(_)) => Origin::Undeclared,
        }
    }
}

/// What happened to one relay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Relayed {
    /// Written to the lead; the uuid the CLI will echo when it takes it.
    Sent { uuid: String },
    /// Not relayed. `sentence` is what the front desk tells him, when there is one to tell.
    Not { sentence: Option<String> },
}

/// What a lead said, on which lane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lane {
    Handle(String),
    Conversation,
}

/// What kind of thing is being said, so a surface never parses a sentence to decide.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Say {
    /// Progress, or a turn's own final words.
    Update,
    /// His team asks him something.
    Question,
    Answer,
    Outcome,
    Failed,
    /// One of his engine's alarms, verbatim.
    Alarm,
    /// Something about his team itself: it ended, it was refused, a stop's result.
    Team,
}

// =============================================================================================
// the seams: what the host is given
// =============================================================================================

/// One running lead, as the host drives it. [`crate::operator_lead::OperatorLead`] in
/// production; a fake in the tests.
pub trait LeadHandle: Send + Sync {
    fn session_id(&self) -> String;
    fn send(&self, text: &str) -> Result<String, LeadError>;
    /// Send under a uuid the host chose: an answer's retry reuses its first uuid (design §2.2).
    fn send_with_uuid(&self, uuid: &str, text: &str) -> Result<(), LeadError>;
    fn stop_task(&self, task_id: &str) -> Result<(), LeadError>;
    fn interrupt(&self) -> Result<InterruptReply, LeadError>;
    fn tasks(&self) -> TaskBook;
    fn exited(&self) -> bool;
    /// The quit path: SIGTERM to the supervisor, then its group after the grace (r3 (q) item 2).
    fn quit(&self) -> Quit;
}

/// Where one conversation's files live, under the install's own operator folder.
#[derive(Clone, Debug)]
pub struct ConversationPaths {
    pub dir: PathBuf,
    /// The report server's scope file and outbox ((c)).
    pub scope: PathBuf,
    pub outbox: PathBuf,
    pub attachments: PathBuf,
    /// The supervisor's snapshot of descendants outside the lead's group (G8).
    pub reap_state: PathBuf,
    /// The host's own record of this conversation's lead (last session, what was read).
    pub record: PathBuf,
}

impl ConversationPaths {
    pub fn under(root: &Path, key: &ConversationKey) -> Self {
        let dir = root.join(safe_segment(&key.entity_id)).join(safe_segment(&key.thread_id));
        ConversationPaths {
            scope: dir.join("report-scope.json"),
            outbox: dir.join("outbox.jsonl"),
            attachments: dir.join("attachments"),
            reap_state: dir.join("reap-state.json"),
            record: dir.join("lead.json"),
            dir,
        }
    }
}

/// A path segment that cannot climb out of its folder. Ids are uuids in practice; anything
/// else is kept readable and made unique by its digest.
fn safe_segment(raw: &str) -> String {
    let clean: String = raw.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect();
    if clean == raw && !clean.is_empty() && clean.len() <= 80 {
        return clean;
    }
    use sha2::Digest;
    let digest = format!("{:x}", sha2::Sha256::digest(raw.as_bytes()));
    format!("{}-{}", &clean[..clean.len().min(40)], &digest[..12])
}

/// How a lead is started for one conversation. The real one builds the operator profile, takes
/// the claim and opens the process; the tests hand back fakes.
pub trait LeadLauncher: Send + Sync {
    /// Start a lead (new or resumed). `title` is the conversation's title, which the claim and
    /// the land lease name it by. `Err` is one sentence to him.
    fn launch(&self, key: &ConversationKey, title: &str, start: &LeadStart, paths: &ConversationPaths,
              sink: Arc<dyn LeadSink>) -> Result<Arc<dyn LeadHandle>, String>;
    /// The fence status the init check needs (r3 (b), §11 item 4). Asked at every lead start.
    fn fences(&self) -> Result<(), String>;
}

/// `agent-liveness.sh`'s three answers (exit 0, 10, 11).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentLiveness {
    Alive,
    NotAlive,
    Indeterminate(String),
}

/// His engine's scripts, run from the DECLARED root with his environment (r3 (b), B9).
pub trait OperatorEngine: Send + Sync {
    /// `stop.sh <names> --entity <root> --ceo-word '<his words>'` (r3 (d) item 1).
    fn stop_words(&self, names: &[String], words: &str) -> Result<String, String>;
    /// `agent-liveness.sh --entity <root> --json <agent id>`. **The target is the agent's id,
    /// which is the stream's `task_id`, never its name:** the resolver takes an id, `agent-<id>`
    /// or a worktree path, and a name matches nothing and reads NOT-ALIVE ("absent/unregistered",
    /// `agent-liveness.py:490-507`) — measured in the r4 probe run, where a live agent read
    /// NOT-ALIVE by name and ALIVE by its worktree. r3 (d) item 3's `agent-liveness.sh <name>` is
    /// corrected here.
    fn liveness(&self, agent_id: &str) -> AgentLiveness;
    /// `RICHOS_SESSION_ID=<lead session> workspaces.sh stop <name> --why '<his words>'` (r4 §2.1).
    fn registry_stop(&self, session_id: &str, name: &str, words: &str) -> Result<String, String>;
    /// `land-lease.sh status --repo <r>` for each fenced repository: the lines of held leases.
    fn held_leases(&self) -> Vec<String>;
}

/// Closing an obligation through the engine (r3 (c); zach's `operator-complete`, contract §2.5).
pub trait Settle: Send + Sync {
    fn complete(&self, key: &ConversationKey, obligation_id: &str, source_ref: &str, status: &str,
                evidence: &[String], answer_text: &str) -> Result<(), String>;
}

/// Saying something to him.
pub trait OperatorDelivery: Send + Sync {
    fn say(&self, key: &ConversationKey, lane: &Lane, kind: Say, text: &str);
}

/// **The §88 seam, outward half.** His team asked him something (`report(kind: question)`). PRD
/// S6 routes it into the one question store; until that lands, [`SayQuestions`] says it to him
/// on its lane. Nothing here holds, confirms or withdraws an answer: r4 §3 removed the hold.
pub trait QuestionSink: Send + Sync {
    fn asked(&self, key: &ConversationKey, record: &ReportRecord);
}

/// The question sink until S6 lands: the question is said to him, as a question.
pub struct SayQuestions(pub Arc<dyn OperatorDelivery>);

impl QuestionSink for SayQuestions {
    fn asked(&self, key: &ConversationKey, record: &ReportRecord) {
        let lane = record.handle.clone().map(Lane::Handle).unwrap_or(Lane::Conversation);
        self.0.say(key, &lane, Say::Question, &record.text);
    }
}

// =============================================================================================
// e4: what changed since a lead's last message
// =============================================================================================

/// The files whose change every lead must hear about (r3 e4): his memory directory, and the two
/// record files that carry his rules. **Stated gap:** the schema-1 declaration has no field
/// naming "declared record files", so this set is his memory plus `CLAUDE.md` and
/// `orchestration.config`; a declaration field is the way to widen it.
pub fn watched_paths(declaration: &Declaration) -> Vec<PathBuf> {
    let slug: String = declaration.entity_root.display().to_string()
        .chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let claude = declaration.claim.file.parent().and_then(Path::parent).map(Path::to_path_buf)
        .unwrap_or_else(|| declaration.home.join(".claude"));
    vec![claude.join("projects").join(slug).join("memory"),
         declaration.entity_root.join("CLAUDE.md"),
         declaration.entity_root.join("orchestration.config")]
}

/// Every file under `watched` (a directory is read one level deep, as the memory folder is
/// flat) modified after `since`.
pub fn changed_since(watched: &[PathBuf], since: SystemTime) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let newer = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).is_ok_and(|t| t > since);
    for path in watched {
        if path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(path) {
                let mut files: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_file() && newer(p)).collect();
                files.sort();
                out.extend(files);
            }
        } else if newer(path) {
            out.push(path.clone());
        }
    }
    out
}

/// Factual changes observed since the prior relay. Role instructions live in files.
pub fn changed_line(changed: &[PathBuf]) -> Option<String> {
    (!changed.is_empty()).then(||serde_json::json!({"changed_since_last_message":changed}).to_string())
}

/// Is this `land-lease.sh status` line a lease held by the conversation titled `title`? The
/// engine names an app lead's lease by the title the claim lists for its pid, exactly as
/// `the conversation "<title>"`, the title cut to 120 characters (`operator_fences.holder_label`,
/// `_app_conversation_title`). A bare substring would let a short title match any line.
pub fn lease_held_by(line: &str, title: &str) -> bool {
    let title: String = title.chars().filter(|c| !c.is_control()).take(120).collect();
    !title.is_empty() && line.contains(&format!("the conversation \"{title}\""))
}

/// The supervisor's count of descendants alive OUTSIDE the lead's own group (G8): tool shells
/// and background commands. `None` when there is no current snapshot, which is never "zero".
pub fn live_descendants(reap_state: &Path) -> Option<usize> {
    let value: Value = serde_json::from_str(&std::fs::read_to_string(reap_state).ok()?).ok()?;
    value.get("outside_provider_group").and_then(Value::as_array).map(Vec::len)
}

// =============================================================================================
// the host's own durable record of one conversation's lead
// =============================================================================================

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct LeadRecord {
    /// The session a relaunch resumes (r3 (l)).
    last_session: Option<String>,
    /// How many outbox records have been acted on. Survives a relaunch, so a report is never
    /// said or settled twice.
    outbox_read: usize,
    /// Handles open when the lead last ended (r3 (l): "the first message lists the handles open
    /// when the lead last ended").
    open_handles: BTreeSet<String>,
    /// §88 seam: answer deliveries already relayed, by S6's delivery identity.
    answers: Vec<AnswerRelay>,
    /// **F8 of Frank's review: handles he stopped (the per-assignment Stop) or interrupted (his
    /// Esc).** Taken out of `open_handles`, so a resume never calls them "still open", and
    /// named to the lead in [`HELD_LINE`]: it continues none of them on its own; his next words
    /// decide. New words of his on a handle (an answer) take it off this list.
    ///
    /// `#[serde(default)]` on this and `untold`: a record written before them must still read.
    /// `read_record` falls back to an empty record on any parse error, which would lose the
    /// session to resume and the outbox position, and settle every report again.
    #[serde(default)]
    held: BTreeSet<String>,
    /// Held handles the lead has not been told about yet: named with its next message. A Stop
    /// is told at once; an Esc is not, because a message after an Esc would start the very
    /// turn he just ended.
    #[serde(default)]
    untold: BTreeSet<String>,
    /// Whose record this is. A relaunch finds a turn that died with the app by reading the
    /// saved records, and needs the conversation to start its lead in. `None` on a record
    /// written before this field, which holds no such turn (`turn_open` is newer still).
    #[serde(default)]
    conversation: Option<ConversationKey>,
    /// **Outbox positions whose close the engine refused for a passing reason** (hunt part 1
    /// finding 02, richos-hq `docs/audits/2026-09-29-hunt/part-1-codex.md`). `outbox_read`
    /// still moves past them, so the reports after one are never held up behind it; each is
    /// asked again, from its own report, at every later outbox read, until the engine closes
    /// it or the register no longer holds it open. He hears the refusal once; a retry that is
    /// refused again says nothing new. Written only when not empty, and `#[serde(default)]`
    /// so a record from before it still reads.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    unsettled: BTreeSet<usize>,
}

/// What one attempt to close a reported assignment came to ([`OperatorHost::settle_handle`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Close {
    /// Nothing more is owed on this report: it closed, or there is nothing it can close.
    Done,
    /// The engine refused for a reason that can pass: the report is owed another try.
    Refused,
}

/// The line that names held handles to the lead (F8). Sent at once when he presses Stop on an
/// assignment, with what the stop measured; otherwise first in its next message.
pub const HELD_LINE: &str = "Stopped or interrupted by him:";

fn held_line(handles: &BTreeSet<String>) -> String {
    format!("{HELD_LINE} {}. They wait for his words; do not continue them on your own.",
            handles.iter().cloned().collect::<Vec<_>>().join(", "))
}

/// One answer relayed to the lead. `answer_to` is the handle whose question it answers (r1
/// (c)'s `answer_to`, kept on the relay rather than in the product register: `Assignment` is
/// `deny_unknown_fields`, so a new field there would make every older build refuse the record).
///
/// **The delivery ledger** (richos-hq `docs/plans/2026-09-27-answer-delivery-crash-dedup-design.md`
/// §2.2). A relay is written, and synced, BEFORE its message is sent (the intent), so a retry
/// after a crash reuses `uuid` into the same `session`; it is `taken` only when the CLI echoes
/// that uuid. A pipe write is not the lead having it (C1: a message queued behind a running turn
/// dies with the app).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct AnswerRelay {
    delivery_id: String,
    answer_to: Option<String>,
    uuid: String,
    /// The lead session `uuid` was sent into. Empty on a relay written before this field.
    #[serde(default)]
    session: String,
    /// The CLI echoed `uuid`: the lead has the answer. A relay written before this field was
    /// saved after its send by the old code, so it reads as taken: the at-most-once promise
    /// that code made for it is kept, and it is never resent.
    #[serde(default = "relay_written_before_taken_existed")]
    taken: bool,
    /// **The turn that took it has not been seen to end** (crash matrix cell W3). Set in the
    /// same synced save as `taken`; cleared when any turn of the lead's completes after it (the
    /// lead has acted on it), when he ends that turn (his Esc, or a quit he chose while his
    /// team worked), when the lead itself ends (its notice asks for his words), or when the
    /// continuation below is taken. Still set at a relaunch, it means the turn died with the
    /// app: his answer is in the lead's context once and nothing makes the lead act on it.
    /// A relay written before this field reads `false`: never continued.
    #[serde(default)]
    turn_open: bool,
    /// The one continuation sent for that turn: its uuid and the session it went into, saved
    /// and synced before the send, so a relaunch before its echo resends it under the same
    /// uuid and a lead that already has it drops the repeat (design §1.2), as for the answer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    continued: Option<Continuation>,
}

/// What `OperatorHost::close_answer_turns_at_end` found when a lead ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AnswerTurnsAtEnd {
    /// No open answer turn: nothing is owed and he is asked to speak.
    NoneOpen,
    /// An unexpected end with a turn not yet continued: it stays open for one restart.
    Continuing,
    /// An unexpected end of a lead that was already started once to continue the turn.
    RetrySpent,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Continuation {
    uuid: String,
    session: String,
}

fn relay_written_before_taken_existed() -> bool {
    true
}

/// What [`OperatorHost::deliver_answer`] did with an answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnswerOutcome {
    /// The lead has it: its CLI echoed the answer's uuid. Only now may the inbox let it go.
    Taken,
    /// Written to the lead, now or earlier in this process, and not echoed yet. It stays in
    /// the inbox; a later flush asks again, and nothing is sent while its uuid is awaited.
    InFlight,
}

/// Said to the lead before an answer it may already have (design §2.2, "the resend note"). A
/// retry exists only after the lead's process ended (the app's death takes it; its own crash
/// ends it), so "this session restarted" is true of every retry. **Wording changed from the
/// design's "RichOS restarted":** after the lead's own crash (W5) RichOS did not restart.
pub const RESEND_NOTE: &str = "This session restarted before RichOS could confirm you received this answer. If it is \
                               already in your conversation, it is the same answer: do not act on it twice.";

/// **The VM crash matrix's abort points** (design §4.2): with the `crash-points` feature, the
/// process aborts where `RICHOS_CRASH_POINT` names this point (`W1b`, `W2`, `W3`), as a crash
/// would, with no cleanup. Without the feature (every product build) it is empty.
#[cfg(feature = "crash-points")]
pub(crate) fn crash_point(name: &str) {
    if std::env::var("RICHOS_CRASH_POINT").is_ok_and(|point| point == name) {
        eprintln!("crash point {name}: aborting");
        std::process::abort();
    }
}

#[cfg(not(feature = "crash-points"))]
#[inline(always)]
pub(crate) fn crash_point(_name: &str) {}

/// How many times an answer's retries may start a lead in one conversation with no turn
/// completing in between. The desk's idle timer retries every minute, so without a bound a
/// lead that ends each time it starts (an init refusal, a crash at start) would be started,
/// and its end announced, every minute. **Not in the design:** its timer retry (§2.2) did not
/// say what bounds it; this does, and says so here.
pub const RETRY_STARTS: u32 = 2;

/// **The continuation** (crash matrix cell W3; not in the design, whose §2.3 left this window
/// waiting for his next words). Said to the lead, once, when the turn that took his answer
/// died with the app. It names the answer by its uuid and never carries the answer again: the
/// answer is already in the lead's context, once. A drop of a resent answer (P18 case B) is
/// the same window, and is continued the same way.
pub const CONTINUE_NOTE: &str = "RichOS restarted while you were working on his answer, and that turn ended before it \
                                 finished.";

/// The whole continuation for these relays: one message for every answer that one dead turn
/// held.
fn continuation_text(relays: &[AnswerRelay]) -> String {
    let named = relays.iter().map(|a| match &a.answer_to {
        Some(h) => format!("his answer on {h}, message {}", a.uuid),
        None => format!("his answer, message {}", a.uuid),
    }).collect::<Vec<_>>().join("; ");
    format!("{CONTINUE_NOTE} It is already in your conversation above ({named}), and it is not sent again. Carry on \
             with that work from where it stopped. Check what was already done before you repeat any step, and if \
             all of it was done, say so in one line.")
}
/// Said when that bound is reached. His words to the conversation start his team as always,
/// and the saved answer follows them.
pub const RETRIES_SPENT: &str = "Your team ended again before it could take your answer, so I've stopped starting it \
                                 for that. Speak to me in this conversation and I'll start it, and your answer goes \
                                 to it then.";

fn read_record(path: &Path) -> LeadRecord {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Written whole, synced, then renamed over the old one, and the folder synced (C4, copying
/// `question_work.rs`): the answer's intent must be on disk before its message is sent, or a
/// retry cannot reuse its uuid.
fn write_record(path: &Path, record: &LeadRecord) -> Result<(), String> {
    use std::io::Write;
    let parent = path.parent().ok_or("the lead record has no folder")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(record).map_err(|e| e.to_string())?;
    let mut file = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    file.write_all(&bytes).and_then(|_| file.sync_all()).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    std::fs::File::open(parent).and_then(|d| d.sync_all()).map_err(|e| e.to_string())
}

// =============================================================================================
// the host
// =============================================================================================

/// One conversation's lead and what the host knows about it.
struct Conversation {
    key: ConversationKey,
    title: String,
    paths: ConversationPaths,
    lead: Option<Arc<dyn LeadHandle>>,
    record: LeadRecord,
    /// uuid → the handle its message was about.
    sent: HashMap<String, Option<String>>,
    /// uuids written and not yet taken into an ended turn.
    awaiting: BTreeSet<String>,
    in_turn: bool,
    last_relay: Option<SystemTime>,
    last_activity: Instant,
    texts: VecDeque<String>,
    /// The text of this turn's last report, for the B5 edge.
    turn_report: Option<String>,
    /// Handle → the names seen starting on its turns, and those its reports named ((d) item 5).
    handle_agents: HashMap<String, BTreeSet<String>>,
    /// The handle the running turn belongs to, if one does.
    turn_handle: Option<String>,
    banner: Option<EngineBanner>,
    /// An init frame that arrived before the engine's banner: the check waits for the banner,
    /// or for the turn's end, whichever comes first (measured order: the banner's SessionStart
    /// response came 0.6-1.5 s before init in P1, P10, P16 and P17, but order is not a contract).
    pending_init: Option<Value>,
    fences: Result<(), String>,
    checked: bool,
    /// A quit or retirement the host itself started: the end that follows is expected.
    quitting: bool,
    started_digest: String,
    told_protocol: bool,
    /// A permission request was said to him once in this conversation (F17).
    told_permission: bool,
    first_after_resume: bool,
    /// Questions his team asked and nothing has answered yet, by handle ((o), until S6).
    questions: Vec<(Option<String>, String)>,
    /// Leads started to retry an answer since this conversation's last completed turn (or his
    /// last words to it). Bounded by [`RETRY_STARTS`].
    retry_starts: u32,
    /// The uuid whose echo set `in_turn` from idle. If the CLI then says it dropped that uuid
    /// (C5), no turn ran, and the conversation is idle again.
    opened_by: Option<String>,
}

/// One named stop's result (r3 (d) item 3, r4 §2.1).
#[derive(Clone, Debug, PartialEq)]
pub enum StopResult {
    /// NOT-ALIVE, in `seconds`; `registry` is whether his registry was told (r4 item 3a).
    Stopped { name: String, seconds: f64, registry: Result<(), String> },
    StillAlive { name: String },
    Indeterminate { name: String, why: String },
    NotFound { name: String },
    Failed { name: String, why: String },
}

impl StopResult {
    /// The sentence he hears. "stopped" is said only on NOT-ALIVE (r3 (d) item 3).
    pub fn sentence(&self) -> String {
        match self {
            Self::Stopped { name, .. } => format!("Stopped {name}."),
            Self::StillAlive { name } => format!("{name} is still running after the stop was sent twice."),
            Self::Indeterminate { name, why } => format!("I can't tell whether {name} stopped: {why}"),
            Self::NotFound { name } => format!("{name} isn't one of your team's agents in this app."),
            Self::Failed { name, why } => format!("The stop for {name} could not be sent: {why}"),
        }
    }
}

/// The front desk's read of one conversation ((o), with r4 §1.1 and §3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationRead {
    pub key: ConversationKey,
    pub lead_running: bool,
    /// Each named agent with its last status from the stream.
    pub agents: Vec<(String, String)>,
    /// The last three turn texts, oldest first.
    pub texts: Vec<String>,
    /// Open questions and whether an answer has been relayed. S2's status listing replaces this.
    pub open_questions: Vec<String>,
    /// Land leases held by this conversation's lead, as `land-lease.sh status` says them.
    pub leases: Vec<String>,
}

/// (m): what his team is doing, for the update gate, the keep-alive and the quit sheet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TeamReading {
    /// Conversations whose lead is in a turn, or has his messages still queued: his team is
    /// working even with no agent and no command running. **Not in r3 (m)'s wording, and
    /// needed:** without it a lead mid-turn with nothing but its own thinking reads as idle,
    /// and closing the window would quit the app and end that turn (found wiring the
    /// keep-alive; recorded in the operator-client record).
    pub working: Vec<String>,
    pub alive: Vec<String>,
    pub unknown: Vec<String>,
    /// Descendants outside the leads' own groups (tool shells, background commands).
    pub descendants: usize,
    /// Running leads whose supervisor snapshot could not be read.
    pub descendants_unknown: usize,
}

pub struct OperatorHost {
    declaration: Declaration,
    /// `<engine-state>`: the assignment register lives under it.
    state: PathBuf,
    /// `<app data>/operator`: this install's operator files.
    root: PathBuf,
    launcher: Arc<dyn LeadLauncher>,
    engine: Arc<dyn OperatorEngine>,
    settle: Arc<dyn Settle>,
    delivery: Arc<dyn OperatorDelivery>,
    questions: Arc<dyn QuestionSink>,
    conversations: Mutex<HashMap<ConversationKey, Arc<Mutex<Conversation>>>>,
    alarms: Mutex<AlarmDeduper>,
    /// The saved records were read once, this process, for turns to continue.
    scanned: std::sync::atomic::AtomicBool,
    /// Agents `agent-liveness.sh` answered NOT-ALIVE for while their stream said they had
    /// ended, by task id, with that ended status ([`Self::resolved_liveness`]).
    witnessed_ended: Mutex<HashMap<String, TaskStatus>>,
}

/// The sink a lead's reader writes to: a channel into this conversation's own worker, so a slow
/// step (an engine call, a Git read) never stalls the lead's stdout.
struct ConversationSink(Mutex<Sender<LeadEvent>>);

impl LeadSink for ConversationSink {
    fn event(&self, event: LeadEvent) {
        // A closed channel means the conversation's worker has ended; the event has nowhere to go.
        self.0.lock().unwrap().send(event).ok();
    }
}

impl OperatorHost {
    #[allow(clippy::too_many_arguments)]
    pub fn new(declaration: Declaration, state: &Path, root: &Path, launcher: Arc<dyn LeadLauncher>,
               engine: Arc<dyn OperatorEngine>, settle: Arc<dyn Settle>, delivery: Arc<dyn OperatorDelivery>,
               questions: Arc<dyn QuestionSink>) -> Arc<Self> {
        Arc::new(OperatorHost {
            declaration,
            state: state.to_path_buf(),
            root: root.to_path_buf(),
            launcher,
            engine,
            settle,
            delivery,
            questions,
            conversations: Mutex::new(HashMap::new()),
            alarms: Mutex::new(AlarmDeduper::default()),
            scanned: std::sync::atomic::AtomicBool::new(false),
            witnessed_ended: Mutex::new(HashMap::new()),
        })
    }

    /// The operator log is where every other failure goes, so its own failure goes to stderr.
    pub(crate) fn log(&self, line: &str) {
        let stamp = crate::operator_claim::iso_utc(
            SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0));
        let written = std::fs::create_dir_all(&self.root)
            .and_then(|_| std::fs::OpenOptions::new().create(true).append(true).open(self.log_path()))
            .and_then(|mut file| {
                use std::io::Write;
                writeln!(file, "{stamp} host: {line}")
            });
        if let Err(e) = written {
            eprintln!("{stamp} operator host: the operator log could not be written ({e}): {line}");
        }
    }

    /// The lead record is what a restart resumes from; a record that could not be saved is logged,
    /// never passed over (the conversation still runs on what is in memory).
    fn save(&self, path: &Path, record: &LeadRecord) {
        if let Err(e) = write_record(path, record) {
            self.log(&format!("the lead record {} could not be saved ({e})", path.display()));
        }
    }

    /// The operator log, shared with the supervisors (`RICHOS_OPERATOR_REAP_LOG`).
    pub fn log_path(&self) -> PathBuf {
        self.root.join("operator.log")
    }

    fn conversation(&self, key: &ConversationKey, title: &str) -> Arc<Mutex<Conversation>> {
        let mut all = self.conversations.lock().unwrap();
        if let Some(existing) = all.get(key) {
            if !title.is_empty() {
                existing.lock().unwrap().title = title.to_string();
            }
            return existing.clone();
        }
        let paths = ConversationPaths::under(&self.root, key);
        let mut record = read_record(&paths.record);
        record.conversation = Some(key.clone());
        let first_after_resume = record.last_session.is_some();
        let conversation = Arc::new(Mutex::new(Conversation {
            key: key.clone(), title: title.to_string(), paths, lead: None, record, sent: HashMap::new(),
            awaiting: BTreeSet::new(), in_turn: false, last_relay: None, last_activity: Instant::now(),
            texts: VecDeque::new(), turn_report: None, handle_agents: HashMap::new(), turn_handle: None, banner: None,
            pending_init: None, fences: Ok(()), checked: false, quitting: false, started_digest: String::new(), told_protocol: false, told_permission: false,
            first_after_resume, questions: Vec::new(), retry_starts: 0, opened_by: None,
        }));
        all.insert(key.clone(), conversation.clone());
        conversation
    }

    /// Start (or resume) the lead of `conversation` if none is running. Lazy: a lead starts on
    /// its conversation's next message, never all at once at relaunch (r3 (l)).
    fn ensure_lead(self: &Arc<Self>, conversation: &mut Conversation) -> Result<Arc<dyn LeadHandle>, String> {
        if let Some(lead) = &conversation.lead {
            if !lead.exited() {
                return Ok(lead.clone());
            }
            conversation.lead = None;
        }
        let start = match &conversation.record.last_session {
            Some(session) => LeadStart::Resume(session.clone()),
            None => LeadStart::New(uuid::Uuid::new_v4().to_string()),
        };
        std::fs::create_dir_all(&conversation.paths.dir).map_err(|e| format!("Your team's folder could not be made ({e})."))?;
        let (tx, rx) = channel::<LeadEvent>();
        let sink: Arc<dyn LeadSink> = Arc::new(ConversationSink(Mutex::new(tx)));
        let fences = self.launcher.fences();
        let lead = self.launcher.launch(&conversation.key, &conversation.title, &start, &conversation.paths, sink)?;
        let host = Arc::clone(self);
        let key = conversation.key.clone();
        std::thread::Builder::new()
            .name(format!("richos-operator-host:{}", &key.thread_id[..key.thread_id.len().min(16)]))
            .spawn(move || {
                for event in rx {
                    host.handle(&key, event);
                }
            })
            .map_err(|e| format!("Your team could not be started ({e})."))?;
        conversation.fences = fences;
        conversation.checked = false;
        conversation.banner = None;
        conversation.pending_init = None;
        conversation.quitting = false;
        conversation.started_digest = snapshot_digest(&self.declaration);
        conversation.record.last_session = Some(lead.session_id());
        self.save(&conversation.paths.record, &conversation.record);
        conversation.lead = Some(lead.clone());
        self.log(&format!("lead started for {}/{} ({})", conversation.key.entity_id, conversation.key.thread_id,
                          match &start { LeadStart::New(s) => format!("new session {s}"), LeadStart::Resume(s) => format!("resumed {s}") }));
        Ok(lead)
    }

    /// **Relay his words to this conversation's lead, now** ((g), (s)). `handle` is the
    /// assignment the words are about, when they are about one.
    pub fn relay(self: &Arc<Self>, key: &ConversationKey, title: &str, handle: Option<&str>, text: &str, origin: Origin)
                 -> Result<Relayed, String> {
        // (s) rule 4: a turn with no intake origin never relays, whatever it contains.
        let Some(declared) = origin.declared() else {
            self.log(&format!("not relayed: no intake origin ({}/{})", key.entity_id, key.thread_id));
            return Ok(Relayed::Not { sentence: None });
        };
        // (s) rule 1: work only from a channel the declaration lists.
        if !self.declaration.origins.iter().any(|o| o == declared) {
            self.log(&format!("not relayed: origin {declared} is not declared ({}/{})", key.entity_id, key.thread_id));
            return Ok(Relayed::Not { sentence: Some(PHONE_ASSIGNMENT.into()) });
        }
        let conversation = self.conversation(key, title);
        let mut c = conversation.lock().unwrap();
        let lead = self.ensure_lead(&mut c)?;
        // His words start his team whatever an answer's retries have spent.
        c.retry_starts = 0;
        let uuid = self.write(&mut c, &lead, key, handle, text, None)?;
        self.save(&c.paths.record, &c.record);
        Ok(Relayed::Sent { uuid })
    }

    /// **A review-watch notice, delivered as a message this host sends** (automatic second
    /// review, slice 4; Sage's check of the plan, §1.3). The lead whose session is `session`
    /// (the one that spawned the teammate the notice is about) gets `text` the way his words
    /// reach it, from this host and under a uuid it sent, so the turn it starts is one the host
    /// can attribute: a monitor inside the lead would start a turn no message of the host's
    /// began, and the host identifies a turn by exactly that message.
    ///
    /// The conversation is found in this launch's memory, else in the saved records
    /// (`<root>/<entity>/<thread>/lead.json`, each confirmed against the key it names). A lead
    /// that has ended is resumed, as his words would resume it: the notice exists to wake it.
    /// No conversation names the session: nothing is sent and the log says so (a teammate
    /// started from his terminal has no lead in this app).
    pub fn tell_lead(self: &Arc<Self>, session: &str, text: &str) -> Result<String, String> {
        let known = self.conversations.lock().unwrap().values()
            .find(|c| c.lock().unwrap().record.last_session.as_deref() == Some(session)).cloned();
        let conversation = match known {
            Some(c) => c,
            None => match self.saved_conversation_of(session) {
                Some(key) => self.conversation(&key, ""),
                None => {
                    self.log(&format!("review-watch notice not delivered: no conversation's lead is session {session}"));
                    return Err(format!("No conversation's lead is session {session}."));
                }
            },
        };
        let mut c = conversation.lock().unwrap();
        let key = c.key.clone();
        let lead = self.ensure_lead(&mut c)?;
        let uuid = self.write(&mut c, &lead, &key, None, text, None)?;
        self.save(&c.paths.record, &c.record);
        self.log(&format!("review-watch notice delivered to {}/{} (session {session})", key.entity_id, key.thread_id));
        Ok(uuid)
    }

    /// The conversation whose saved record says its lead's last session is `session`.
    fn saved_conversation_of(&self, session: &str) -> Option<ConversationKey> {
        let entities = std::fs::read_dir(&self.root).ok()?;
        for entity in entities.flatten() {
            let Ok(threads) = std::fs::read_dir(entity.path()) else { continue };
            for thread in threads.flatten() {
                let path = thread.path().join("lead.json");
                if !path.is_file() {
                    continue;
                }
                let record = read_record(&path);
                let Some(key) = record.conversation.clone() else { continue };
                if ConversationPaths::under(&self.root, &key).record == path
                    && record.last_session.as_deref() == Some(session) {
                    return Some(key);
                }
            }
        }
        None
    }

    /// Write one message to the lead, prefixed as e4 and (l) say, under the conversation's lock
    /// the caller holds. Shared by [`Self::relay`] and [`Self::deliver_answer`], which is the
    /// only other way words reach a lead. `uuid` is the one to send under (an answer's), or
    /// `None` for the lead's own. Returns the uuid sent. The caller saves the record.
    fn write(&self, c: &mut Conversation, lead: &Arc<dyn LeadHandle>, key: &ConversationKey, handle: Option<&str>,
             text: &str, uuid: Option<&str>) -> Result<String, String> {
        // F8: his words on a held handle are his next words about it; it is no longer held.
        if let Some(h) = handle {
            c.record.held.remove(h);
            c.record.untold.remove(h);
        }
        let mut lines = Vec::new();
        if c.first_after_resume {
            if !c.record.open_handles.is_empty() {
                lines.push(format!("Open when you last ended: {}.",
                                   c.record.open_handles.iter().cloned().collect::<Vec<_>>().join(", ")));
            }
            if !c.record.held.is_empty() {
                // Every held handle, so every untold one (untold is always within held).
                lines.push(held_line(&c.record.held));
            }
            c.first_after_resume = false;
        } else if !c.record.untold.is_empty() {
            lines.push(held_line(&c.record.untold));
        }
        if let Some(since) = c.last_relay {
            if let Some(line) = changed_line(&changed_since(&watched_paths(&self.declaration), since)) {
                lines.push(line);
            }
        }
        if let Some(handle) = handle {
            lines.push(format!("Assignment handle: {handle}. Report on it with richos_operator.report and this handle."));
        }
        lines.push(text.to_string());
        let body = lines.join("\n\n");
        let sent = match uuid {
            Some(chosen) => lead.send_with_uuid(chosen, &body).map(|_| chosen.to_string()),
            None => lead.send(&body),
        };
        let uuid = sent.map_err(|e| {
            // Kept on error (r3 (q) item 1): the lead is not touched; he is told the relay failed.
            self.log(&format!("relay failed for {}/{}: {e}", key.entity_id, key.thread_id));
            format!("Your message did not reach your team ({e}). Your team is still running.")
        })?;
        c.sent.insert(uuid.clone(), handle.map(str::to_string));
        c.awaiting.insert(uuid.clone());
        c.last_relay = Some(SystemTime::now());
        c.last_activity = Instant::now();
        // Every untold held handle was named in this message (above), and it was written.
        c.record.untold.clear();
        if let Some(h) = handle {
            c.record.open_handles.insert(h.to_string());
        }
        Ok(uuid)
    }

    /// **The §88 seam, inward half.** PRD S6 calls this with a resolved answer set for a
    /// question his team asked on `handle` (or on the conversation). It reaches the lead from ANY
    /// channel, with no hold and no confirmation (r4 §3). `delivery_id` is S6's durable delivery
    /// identity.
    ///
    /// The app question worker queues resolved sets durably, then flushes them through
    /// [`crate::operator_desk::OperatorDesk::deliver_answer`] outside the question store lock.
    /// Phone and desktop answers use this same path without a desktop confirmation hold.
    ///
    /// **Crash-safe, once** (richos-hq `docs/plans/2026-09-27-answer-delivery-crash-dedup-design.md`
    /// §2.2). Under ONE hold of the conversation's lock (C9): an answer already `taken` is
    /// [`AnswerOutcome::Taken`] and nothing is sent; one whose uuid this process still awaits is
    /// [`AnswerOutcome::InFlight`] and nothing is sent; otherwise its intent (uuid, session) is
    /// saved and synced, THEN it is sent. A retry reuses the uuid when the lead is in the same
    /// session, so a lead that already has it drops the repeat (the provider's check, §1.2), and
    /// it carries [`RESEND_NOTE`] for a lead that does not drop it. A lost record means a new
    /// session, whose context never saw the answer, so a new uuid delivers it once there.
    pub fn deliver_answer(self: &Arc<Self>, key: &ConversationKey, title: &str, handle: Option<&str>,
                          delivery_id: &str, answer: &str) -> Result<AnswerOutcome, String> {
        let conversation = self.conversation(key, title);
        let mut c = conversation.lock().unwrap();
        let prior = c.record.answers.iter().find(|a| a.delivery_id == delivery_id).cloned();
        if let Some(relay) = &prior {
            if relay.taken {
                return Ok(AnswerOutcome::Taken);
            }
            if c.awaiting.contains(&relay.uuid) {
                return Ok(AnswerOutcome::InFlight);
            }
        }
        let starting = c.lead.as_ref().is_none_or(|l| l.exited());
        if prior.is_some() && starting && c.retry_starts >= RETRY_STARTS {
            return Err(RETRIES_SPENT.into());
        }
        let lead = self.ensure_lead(&mut c)?;
        if prior.is_some() && starting {
            c.retry_starts += 1;
        }
        let session = lead.session_id();
        let uuid = match &prior {
            Some(relay) if relay.session == session => relay.uuid.clone(),
            _ => uuid::Uuid::new_v4().to_string(),
        };
        let answer_text = match handle {
            Some(h) => format!("His answer to your question on {h}:\n\n{answer}"),
            None => format!("His answer to your question:\n\n{answer}"),
        };
        let text = if prior.is_some() { format!("{RESEND_NOTE}\n\n{answer_text}") } else { answer_text };
        // The intent, before any byte is sent (C4: synced, and a failure to save it sends nothing).
        let before = c.record.answers.clone();
        c.record.answers.retain(|a| a.delivery_id != delivery_id);
        c.record.answers.push(AnswerRelay { delivery_id: delivery_id.to_string(), answer_to: handle.map(str::to_string),
                                            uuid: uuid.clone(), session, taken: false, turn_open: false,
                                            continued: None });
        if let Err(e) = write_record(&c.paths.record, &c.record) {
            c.record.answers = before;
            self.log(&format!("answer {delivery_id} for {}/{} not sent: its intent could not be saved ({e})",
                              key.entity_id, key.thread_id));
            return Err(format!("RichOS could not save it for your team ({e})."));
        }
        if let Err(sentence) = self.write(&mut c, &lead, key, handle, &text, Some(&uuid)) {
            // A failed write is no send (a partial line is no frame), so this attempt's intent
            // goes: the next try is a first send, without the note. An earlier send's stays.
            c.record.answers = before;
            self.save(&c.paths.record, &c.record);
            return Err(sentence);
        }
        crash_point("W2"); // sent, not yet echoed
        if let Some(h) = handle {
            c.questions.retain(|(q, _)| q.as_deref() != Some(h));
        }
        self.save(&c.paths.record, &c.record);
        if let Some(earlier) = &prior {
            self.log(&format!("answer {delivery_id} resent to {}/{} with the note, under {} uuid", key.entity_id,
                              key.thread_id, if earlier.uuid == uuid { "the same" } else { "a new" }));
        }
        Ok(AnswerOutcome::InFlight)
    }

    /// **The CLI took this uuid** (its `--replay-user-messages` echo, the only positive signal
    /// that the lead has a message; design §2.2, C1). An answer relay carrying it becomes
    /// `taken`, synced, and its delivery id is returned for the inbox to let go of. `None` for
    /// any other uuid, or when the save failed: the inbox then keeps the answer, the next flush
    /// reads `taken` from memory, and after a crash the same uuid is resent and dropped (§1.2).
    fn answer_taken(&self, c: &mut Conversation, uuid: &str) -> Option<String> {
        let relay = c.record.answers.iter_mut().find(|a| a.uuid == uuid && !a.taken)?;
        relay.taken = true;
        relay.turn_open = true;
        let id = relay.delivery_id.clone();
        match write_record(&c.paths.record, &c.record) {
            Ok(()) => Some(id),
            Err(e) => {
                self.log(&format!("answer {id} was taken by the lead; that could not be saved ({e})"));
                None
            }
        }
    }

    /// **Continue every turn that took his answer and died with the app** (crash matrix cell
    /// W3). The desk calls it after each answer flush: at launch, on every question-store
    /// change and on the idle timer. The first call in a process reads the saved records once
    /// and opens each conversation that holds such a turn; later calls look only at the open
    /// conversations, which is where any later one is. `title` names a conversation's lead
    /// (the claim and the land lease name it by its title).
    pub fn continue_interrupted_answers(self: &Arc<Self>, title: &dyn Fn(&str) -> String) {
        if !self.scanned.swap(true, std::sync::atomic::Ordering::SeqCst) {
            for key in self.saved_turns_to_continue() {
                self.conversation(&key, &title(&key.thread_id));
            }
        }
        let all: Vec<Arc<Mutex<Conversation>>> = self.conversations.lock().unwrap().values().cloned().collect();
        for conversation in all {
            let mut c = conversation.lock().unwrap();
            if let Err(e) = self.continue_one(&mut c) {
                self.log(&format!("{}/{}: the turn that took his answer could not be continued yet ({e})",
                                  c.key.entity_id, c.key.thread_id));
            }
        }
    }

    /// The conversations whose saved record holds a taken answer whose turn was never seen
    /// to end. Read from `<root>/<entity>/<thread>/lead.json`, each confirmed against the key
    /// it names, so a record in the wrong folder is never acted on.
    fn saved_turns_to_continue(&self) -> Vec<ConversationKey> {
        let mut found = Vec::new();
        let Ok(entities) = std::fs::read_dir(&self.root) else { return found };
        for entity in entities.flatten() {
            let Ok(threads) = std::fs::read_dir(entity.path()) else { continue };
            for thread in threads.flatten() {
                let path = thread.path().join("lead.json");
                if !path.is_file() {
                    continue;
                }
                let record = read_record(&path);
                let Some(key) = record.conversation.clone() else { continue };
                if ConversationPaths::under(&self.root, &key).record == path
                    && record.answers.iter().any(|a| a.taken && a.turn_open) {
                    found.push(key);
                }
            }
        }
        found
    }

    /// One conversation: nothing while a turn runs or a message waits for one (that turn's
    /// end settles it), and nothing for a turn his Stop held (F8: his next words decide).
    /// Otherwise its lead is started or resumed, within the retry bound, and continued.
    fn continue_one(self: &Arc<Self>, c: &mut Conversation) -> Result<(), String> {
        if c.in_turn || !c.awaiting.is_empty() || !self.release_held_turns(c) {
            return Ok(());
        }
        let starting = c.lead.as_ref().is_none_or(|l| l.exited());
        if starting && c.retry_starts >= RETRY_STARTS {
            return Err(RETRIES_SPENT.into());
        }
        let lead = self.ensure_lead(c)?;
        if starting {
            c.retry_starts += 1;
        }
        self.send_continuation(c, &lead)
    }

    /// Lets go of every open turn whose answer is on a handle he stopped, and says whether any
    /// open turn is left to continue.
    fn release_held_turns(&self, c: &mut Conversation) -> bool {
        let held = c.record.held.clone();
        let mut released = false;
        for relay in c.record.answers.iter_mut().filter(|a| a.taken && a.turn_open) {
            if relay.answer_to.as_ref().is_some_and(|h| held.contains(h)) {
                relay.turn_open = false;
                released = true;
            }
        }
        if released {
            self.save(&c.paths.record, &c.record);
        }
        c.record.answers.iter().any(|a| a.taken && a.turn_open)
    }

    /// Send the one continuation for every open turn in this conversation, to a running lead:
    /// its intent (uuid, session) saved and synced first, and a retry into the same session
    /// under the same uuid, as for the answer itself.
    fn send_continuation(&self, c: &mut Conversation, lead: &Arc<dyn LeadHandle>) -> Result<(), String> {
        let open: Vec<AnswerRelay> = c.record.answers.iter().filter(|a| a.taken && a.turn_open).cloned().collect();
        let Some(first) = open.first() else { return Ok(()) };
        let session = lead.session_id();
        let uuid = match &first.continued {
            Some(earlier) if earlier.session == session => earlier.uuid.clone(),
            _ => uuid::Uuid::new_v4().to_string(),
        };
        let before = c.record.answers.clone();
        for relay in c.record.answers.iter_mut().filter(|a| a.taken && a.turn_open) {
            relay.continued = Some(Continuation { uuid: uuid.clone(), session: session.clone() });
        }
        let intent = write_record(&c.paths.record, &c.record);
        if let Err(e) = intent {
            c.record.answers = before;
            return Err(format!("its intent could not be saved ({e})"));
        }
        let key = c.key.clone();
        if let Err(sentence) = self.write(c, lead, &key, None, &continuation_text(&open), Some(&uuid)) {
            c.record.answers = before;
            self.save(&c.paths.record, &c.record);
            return Err(sentence);
        }
        // Its turn belongs to the assignment the answer was on, as the answer's did.
        c.sent.insert(uuid.clone(), first.answer_to.clone());
        let ids = open.iter().map(|a| a.delivery_id.as_str()).collect::<Vec<_>>().join(", ");
        self.log(&format!("continued {ids} in {}/{} under {uuid}", key.entity_id, key.thread_id));
        Ok(())
    }

    /// The lead took the continuation: the work carries on in that turn, and no turn of these
    /// answers is continued again, whatever happens to it.
    fn continuation_taken(&self, c: &mut Conversation, uuid: &str) {
        let mut taken = false;
        for relay in c.record.answers.iter_mut().filter(|a| a.turn_open) {
            if relay.continued.as_ref().is_some_and(|n| n.uuid == uuid) {
                relay.turn_open = false;
                taken = true;
            }
        }
        if taken {
            self.save(&c.paths.record, &c.record);
        }
    }

    /// A turn completed after the answer, or he or the lead's own end ended it: nothing to
    /// continue.
    fn close_answer_turns(&self, c: &mut Conversation) {
        let mut closed = false;
        for relay in c.record.answers.iter_mut().filter(|a| a.turn_open) {
            relay.turn_open = false;
            closed = true;
        }
        if closed {
            self.save(&c.paths.record, &c.record);
        }
    }

    // ---- events ----------------------------------------------------------------------------

    /// One event from a lead's stream, on that conversation's worker.
    pub(crate) fn handle(&self, key: &ConversationKey, event: LeadEvent) {
        let Some(conversation) = self.conversations.lock().unwrap().get(key).cloned() else { return };
        match event {
            LeadEvent::Alarm(alarm) => self.alarm(&conversation, alarm),
            LeadEvent::Init(init) => self.init(&conversation, &init),
            // The turn belongs to the handle of the first message it took that had one (r3 (d)
            // item 5): every agent it starts from here is that assignment's.
            LeadEvent::Took(uuid) => {
                let mut c = conversation.lock().unwrap();
                c.last_activity = Instant::now();
                if !c.in_turn {
                    c.opened_by = Some(uuid.clone());
                }
                c.in_turn = true;
                if c.turn_handle.is_none() {
                    c.turn_handle = c.sent.get(&uuid).cloned().flatten();
                }
                let taken = self.answer_taken(&mut c, &uuid);
                self.continuation_taken(&mut c, &uuid);
                drop(c);
                // The inbox lets the answer go only now (design §2.2). An answer that never
                // entered the inbox (a direct call) has nothing to let go of.
                if let Some(id) = taken {
                    crash_point("W3"); // taken and saved, the inbox not yet told
                    if let Err(e) = crate::question_work::acknowledge_if_present(&self.state, &id) {
                        self.log(&format!("answer {id} was taken; the inbox could not record it ({e}), so the next \
                                           flush does"));
                    }
                }
            }
            // C5 (P18's frames): the CLI took this uuid and ran nothing for it, because the
            // session already held it as answered. Nothing awaits it, and if its echo was what
            // made the conversation read as in a turn, no turn is running. It stays taken.
            LeadEvent::Dropped(uuid) => {
                let mut c = conversation.lock().unwrap();
                c.awaiting.remove(&uuid);
                if c.opened_by.as_deref() == Some(uuid.as_str()) {
                    c.opened_by = None;
                    c.in_turn = false;
                    c.turn_handle = None;
                }
                self.log(&format!("{}/{}: the lead already had {uuid}; nothing ran for it", key.entity_id, key.thread_id));
                // A dropped ANSWER is P18 case B: the turn that first took it died with the app
                // (a killed turn reads as answered). Nothing makes the lead act on it, so it is
                // continued now, on the lead that is running, once.
                let answer = c.record.answers.iter().any(|a| a.uuid == uuid && a.taken && a.turn_open);
                let lead = c.lead.clone().filter(|l| !l.exited());
                if let (true, false, true, Some(lead)) = (answer, c.in_turn, c.awaiting.is_empty(), lead) {
                    if self.release_held_turns(&mut c) {
                        if let Err(e) = self.send_continuation(&mut c, &lead) {
                            self.log(&format!("{}/{}: the turn that took his answer could not be continued ({e})",
                                              key.entity_id, key.thread_id));
                        }
                    }
                }
            }
            LeadEvent::Agent(task) => {
                let mut c = conversation.lock().unwrap();
                c.last_activity = Instant::now();
                c.opened_by = None; // an agent's frame proves a turn is running
                c.in_turn = true;
                if task.status == TaskStatus::Running {
                    if let Some(h) = c.turn_handle.clone() {
                        c.handle_agents.entry(h).or_default().insert(task.name.clone());
                    }
                }
            }
            LeadEvent::TurnEnded(end) => self.turn_ended(&conversation, end),
            // A report mid-turn reaches him now; the turn's end still compares against it.
            LeadEvent::Reported => self.take_reports(&conversation),
            LeadEvent::Protocol(what) => {
                self.log(&format!("{}/{}: {what}; the lead is kept", key.entity_id, key.thread_id));
                let mut c = conversation.lock().unwrap();
                if !c.told_protocol {
                    c.told_protocol = true;
                    drop(c);
                    self.delivery.say(key, &Lane::Conversation, Say::Team,
                        "Your team's connection in this conversation sent something RichOS could not read. Your team is still running.");
                }
            }
            // F17 / Frank Q3 (a): the route refused it; P14 measured none in bypass mode, so
            // every arrival is logged as the CLI having changed, and he hears it once here.
            LeadEvent::PermissionAsked(tool) => {
                self.log(&format!("WARNING: permission request for {tool} from {}/{}, refused; P14 measured none in \
                                   bypass mode, so the CLI changed", key.entity_id, key.thread_id));
                let mut c = conversation.lock().unwrap();
                if !c.told_permission {
                    c.told_permission = true;
                    drop(c);
                    self.delivery.say(key, &Lane::Conversation, Say::Team, &format!(
                        "Your team asked for approval to use {tool}, which RichOS never expected here. RichOS refused it. \
                         Your team is still running."));
                }
            }
            LeadEvent::Ended => self.ended(&conversation),
        }
    }

    fn alarm(&self, conversation: &Arc<Mutex<Conversation>>, alarm: Alarm) {
        if let Some(banner) = banner_in(&alarm) {
            let waiting = {
                let mut c = conversation.lock().unwrap();
                c.banner = Some(banner);
                c.pending_init.take()
            };
            if let Some(init) = waiting {
                self.check(conversation, &init);
            }
        }
        let key = conversation.lock().unwrap().key.clone();
        if self.alarms.lock().unwrap().admit(&alarm, Instant::now()) {
            self.delivery.say(&key, &Lane::Conversation, Say::Alarm, &alarm.text);
        }
    }

    fn init(&self, conversation: &Arc<Mutex<Conversation>>, init: &Value) {
        {
            let mut c = conversation.lock().unwrap();
            if c.checked {
                return;
            }
            if c.banner.is_none() {
                c.pending_init = Some(init.clone());
                return;
            }
        }
        self.check(conversation, init);
    }

    /// The init check (r3 (b)), once per lead.
    fn check(&self, conversation: &Arc<Mutex<Conversation>>, init: &Value) {
        let mut c = conversation.lock().unwrap();
        if c.checked {
            return;
        }
        c.checked = true;
        c.pending_init = None;
        match init_check(&self.declaration, init, c.banner.as_ref(), &c.fences) {
            InitVerdict::Open { announce } => {
                let key = c.key.clone();
                drop(c);
                if let Some(sentence) = announce {
                    self.delivery.say(&key, &Lane::Conversation, Say::Team, &sentence);
                }
            }
            InitVerdict::Refuse(refusal) => {
                c.quitting = true;
                let lead = c.lead.take();
                let key = c.key.clone();
                drop(c);
                self.log(&format!("{}/{}: init check refused the lead: {}", key.entity_id, key.thread_id, refusal.what));
                if let Some(lead) = lead {
                    let quit = lead.quit();
                    self.log(&format!("{}/{}: refused lead quit: {quit:?}", key.entity_id, key.thread_id));
                }
                self.delivery.say(&key, &Lane::Conversation, Say::Team, &refusal.sentence());
            }
        }
    }

    /// Read what the report server appended since the last read, and act on each record.
    ///
    /// **A close the engine refused is owed, and it is paid from here** (hunt part 1 finding
    /// 02). The cursor moving past a report is what keeps a report from being said or settled
    /// twice, and it still moves; what the refused close still needs is kept beside it in
    /// [`LeadRecord::unsettled`] and asked again first, at every read, before anything new.
    fn take_reports(&self, conversation: &Arc<Mutex<Conversation>>) {
        let (key, outbox, from, owed) = {
            let c = conversation.lock().unwrap();
            (c.key.clone(), c.paths.outbox.clone(), c.record.outbox_read, c.record.unsettled.clone())
        };
        let records = match read_outbox(&outbox) {
            Ok(records) => records,
            Err(e) => {
                self.log(&format!("{}/{}: the outbox could not be read ({e})", key.entity_id, key.thread_id));
                return;
            }
        };
        for index in owed {
            let close = match records.get(index) {
                Some(record) => self.close_again(conversation, &key, record),
                None => {
                    self.log(&format!("{}/{}: a refused close at outbox record {index} is no longer in the outbox; \
                                       nothing to ask again", key.entity_id, key.thread_id));
                    Close::Done
                }
            };
            if close == Close::Done {
                let mut c = conversation.lock().unwrap();
                c.record.unsettled.remove(&index);
                self.save(&c.paths.record, &c.record);
            }
        }
        for (index, record) in records.iter().enumerate().skip(from) {
            let close = self.report(conversation, &key, record);
            let mut c = conversation.lock().unwrap();
            c.record.outbox_read = index + 1;
            if close == Close::Refused {
                c.record.unsettled.insert(index);
            }
            self.save(&c.paths.record, &c.record);
        }
    }

    /// Ask the engine again to close what an earlier report said. Only a report that names a
    /// handle and reports an ending is ever owed; anything else read here owes nothing.
    fn close_again(&self, conversation: &Arc<Mutex<Conversation>>, key: &ConversationKey, record: &ReportRecord) -> Close {
        match (&record.handle, record.kind.as_str()) {
            (Some(handle), "outcome" | "failed") => {
                let mut text = record.text.clone();
                for land in &record.lands {
                    text.push_str("\n\n");
                    text.push_str(&land.says);
                }
                self.settle_handle(conversation, key, handle, record, &text, true)
            }
            _ => Close::Done,
        }
    }

    fn report(&self, conversation: &Arc<Mutex<Conversation>>, key: &ConversationKey, record: &ReportRecord) -> Close {
        {
            let mut c = conversation.lock().unwrap();
            c.turn_report = Some(record.text.clone());
            if let Some(h) = &record.handle {
                c.handle_agents.entry(h.clone()).or_default().extend(record.agents.iter().cloned());
            }
        }
        let lane = record.handle.clone().map(Lane::Handle).unwrap_or(Lane::Conversation);
        let mut text = record.text.clone();
        for land in &record.lands {
            text.push_str("\n\n");
            text.push_str(&land.says);
        }
        match record.kind.as_str() {
            "question" => {
                conversation.lock().unwrap().questions.push((record.handle.clone(), record.text.clone()));
                self.questions.asked(key, record);
            }
            "outcome" | "failed" => match &record.handle {
                Some(handle) => return self.settle_handle(conversation, key, handle, record, &text, false),
                None => self.delivery.say(key, &lane, if record.kind == "failed" { Say::Failed } else { Say::Outcome }, &text),
            },
            "answer" => self.delivery.say(key, &lane, Say::Answer, &text),
            _ => self.delivery.say(key, &lane, Say::Update, &text),
        }
        Close::Done
    }

    /// Only a report settles ((c)): close the obligation through the engine, then the register.
    ///
    /// `again` is a retry of a close the engine refused earlier ([`LeadRecord::unsettled`]).
    /// He already heard that report and that it stays open, so a retry speaks only when it
    /// closes; every other ending of a retry goes to the log.
    fn settle_handle(&self, conversation: &Arc<Mutex<Conversation>>, key: &ConversationKey, handle: &str,
                     record: &ReportRecord, text: &str, again: bool) -> Close {
        let lane = Lane::Handle(handle.to_string());
        let item = match assignment::read(&self.state, &key.entity_id, &key.thread_id, handle) {
            Ok(item) => item,
            Err(e) => {
                self.log(&format!("{}/{}: report on {handle} names no readable assignment ({e})", key.entity_id, key.thread_id));
                if !again {
                    self.delivery.say(key, &lane, Say::Update, text);
                }
                return Close::Done;
            }
        };
        // F5: an assignment a report already closed stays closed. He is told so, and never
        // "It stays open", which is what the engine's refusal used to turn into.
        if matches!(item.state, AssignmentState::Settled | AssignmentState::Failed) {
            self.log(&format!("{}/{}: a {} report on {handle}, which is already {}; nothing changed",
                              key.entity_id, key.thread_id, record.kind, item.state.as_str()));
            if !again {
                self.delivery.say(key, &lane, Say::Team, &format!("{text}\n\n{ALREADY_CLOSED}"));
            }
            return Close::Done;
        }
        let failed = record.kind == "failed";
        // Q3 (c) of Frank's review: CLAUDE.md "Report the ARTIFACT" and r3 (c) "Lands are
        // verified in Git before any notice says 'landed'". An `outcome` that names a land Git
        // could not confirm is not done, even beside a land that was confirmed, so it closes
        // nothing: the engine is not asked, the register and the open handle are kept, and he
        // hears the land sentence and that it stays open. The lead reports again once the land
        // is real (the tool's answer tells it so), or reports `failed`.
        if !failed && record.lands.iter().any(|l| !l.landed) {
            self.log(&format!("{}/{}: an outcome on {handle} names a land that could not be confirmed; it stays open",
                              key.entity_id, key.thread_id));
            if !again {
                self.delivery.say(key, &lane, Say::Team, &format!("{text}\n\n{UNCONFIRMED_STAYS_OPEN}"));
            }
            return Close::Done;
        }
        // A failure closes on its answer alone: the engine refuses a `git:` item on a withdrawn
        // close ("a land cannot close a failed assignment", `app.py` `operator_complete`). Any
        // land it names is still said to him, in `text`.
        let mut evidence: Vec<String> = if failed { Vec::new() } else {
            record.lands.iter().map(|l| format!("git:{}:{}:{}", l.repository.display(), l.into, l.commit)).collect()
        };
        use sha2::Digest;
        evidence.push(format!("answer:{:x}", sha2::Sha256::digest(record.text.as_bytes())));
        evidence.truncate(20);
        let status = if failed { ECS_WITHDRAWN } else { "completed" };
        let source = format!("operator-report:{}:{}", record.lead, record.at_ms);
        match self.settle.complete(key, &item.obligation_id, &source, status, &evidence, &record.text) {
            Ok(()) => {
                let (to, kind) = if failed {
                    (AssignmentState::Failed, NoticeKind::Failed)
                } else if item.kind.is_question() {
                    (AssignmentState::Settled, NoticeKind::Answer)
                } else {
                    (AssignmentState::Settled, NoticeKind::Settled)
                };
                let detail = if failed { "Your team reported that it could not be done." } else { "Your team reported it done." };
                if let Err(e) = assignment::advance(&self.state, &key.entity_id, &key.thread_id, handle, to, detail)
                    .and_then(|_| assignment::raise_notice(&self.state, &key.entity_id, &key.thread_id, handle, kind, text)) {
                    self.log(&format!("{}/{}: {handle} settled in the engine; the register could not be updated ({e})",
                                      key.entity_id, key.thread_id));
                }
                let mut c = conversation.lock().unwrap();
                c.record.open_handles.remove(handle);
                c.record.held.remove(handle);
                c.record.untold.remove(handle);
                c.questions.retain(|(q, _)| q.as_deref() != Some(handle));
                self.save(&c.paths.record, &c.record);
                drop(c);
                if let Err(error) = crate::questions::Store::new(&self.state).close(
                    &key.entity_id, &key.thread_id, Some(&format!("operator:handle:{handle}")), detail, false) {
                    self.log(&format!("could not close {handle}'s questions: {error}"));
                }
                self.delivery.say(key, &lane, if failed { Say::Failed } else { Say::Outcome }, text);
                Close::Done
            }
            // The engine's own words for an obligation that is no longer open (`app.py`
            // `operator_complete`): closed already, with a register that did not say so.
            // Nothing is owed: asking again would be refused the same way.
            Err(why) if why.contains(ENGINE_NOT_OPEN) => {
                self.log(&format!("{}/{}: operator-complete refused {handle}: {why}", key.entity_id, key.thread_id));
                if !again {
                    self.delivery.say(key, &lane, Say::Team, &format!("{text}\n\n{ALREADY_CLOSED}"));
                }
                Close::Done
            }
            // **Any other refusal is not a verdict on the work, and it is owed** (finding 02).
            // No close is invented: the register and the open handle stay as they are, and he
            // is told it stays open, once. The report is asked again at the next outbox read.
            Err(why) => {
                self.log(&format!("{}/{}: operator-complete refused {handle}{}: {why}", key.entity_id, key.thread_id,
                                  if again { " again" } else { "" }));
                if !again {
                    self.delivery.say(key, &lane, Say::Team, &format!(
                        "{text}\n\nYour team reported this, but RichOS could not record it as closed ({why}). \
                         It stays open, and RichOS will try again."));
                }
                Close::Refused
            }
        }
    }

    fn turn_ended(&self, conversation: &Arc<Mutex<Conversation>>, end: TurnEnd) {
        // A turn ended and the banner never came: the check decides now, and refuses.
        let waiting = conversation.lock().unwrap().pending_init.take();
        if let Some(init) = waiting {
            self.check(conversation, &init);
            let refused = { let c = conversation.lock().unwrap(); c.quitting && c.lead.is_none() };
            if refused {
                // A lead the check refused: its first turn's words are not his team's words.
                return;
            }
        }
        self.take_reports(conversation);
        let mut c = conversation.lock().unwrap();
        for uuid in &end.started_by {
            c.awaiting.remove(uuid);
        }
        // The lane is the handle of the first message the CLI took into this turn, if it had
        // one; a turn the platform started itself is the conversation's (r3 (c)).
        let lane = end.started_by.iter().find_map(|u| c.sent.get(u).cloned().flatten())
            .map(Lane::Handle).unwrap_or(Lane::Conversation);
        let last_report = c.turn_report.take();
        c.turn_handle = None;
        c.in_turn = false;
        c.opened_by = None;
        c.retry_starts = 0; // the lead works: an answer's retries may start it again
        // A turn completed after every answer it had taken: the lead has acted on them.
        self.close_answer_turns(&mut c);
        c.last_activity = Instant::now();
        let key = c.key.clone();
        let deliver = end.text.filter(|t| last_report.as_deref().map(str::trim) != Some(t.trim()));
        if let Some(text) = &deliver {
            c.texts.push_back(text.clone());
            while c.texts.len() > LAST_TEXTS {
                c.texts.pop_front();
            }
        }
        drop(c);
        if let Some(text) = deliver {
            self.delivery.say(&key, &lane, Say::Update, &text);
        }
    }

    /// A positive end: the lead's output closed. Confirmed with `waitid` before anything is
    /// said; an expected end (quit, retirement, refusal) is not news.
    fn ended(&self, conversation: &Arc<Mutex<Conversation>>) {
        let (key, lead, quitting, turns) = {
            let mut c = conversation.lock().unwrap();
            // C6: an ended lead holds nothing. Its queued messages died with it, so nothing is
            // awaited any more and no turn runs: an answer not yet taken is resent by the next
            // flush (the desk's timer), into the resumed session, under the same uuid.
            c.awaiting.clear();
            c.in_turn = false;
            c.turn_handle = None;
            c.opened_by = None;
            // **CONTINUE IT ONCE** (part 1 hunt finding 16; the CEO's ruling of 2026-09-30). An
            // expected end closes every open answer turn, as it always did. An unexpected end
            // keeps open the turns that have not had their one continuation, so the desk's next
            // tick restarts the lead and finishes the answer by itself; a turn that already had
            // its one continuation is closed, and he is asked to speak, as before.
            let turns = self.close_answer_turns_at_end(&mut c);
            (c.key.clone(), c.lead.take(), c.quitting, turns)
        };
        let Some(lead) = lead else { return };
        let deadline = Instant::now() + Duration::from_secs(2);
        while !lead.exited() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        let quit = lead.quit();
        if quitting {
            self.log(&format!("{}/{}: lead ended as expected ({quit:?})", key.entity_id, key.thread_id));
            return;
        }
        // r3 (q) item 5: every end not caused by quit or retirement, with its cause, counted.
        self.log(&format!("LEAD ENDED UNEXPECTEDLY {}/{}: its output closed ({quit:?})", key.entity_id, key.thread_id));
        let notice = match turns {
            AnswerTurnsAtEnd::Continuing => "Your team in this conversation stopped unexpectedly. \
                I'm starting it again now and it will finish your answer.",
            AnswerTurnsAtEnd::RetrySpent => "Your team in this conversation stopped again while finishing your answer. \
                Speak to me here and I'll start it again, resumed where it was.",
            AnswerTurnsAtEnd::NoneOpen => "Your team in this conversation has ended. \
                Speak to me here and I'll start it again, resumed where it was.",
        };
        self.delivery.say(&key, &Lane::Conversation, Say::Team, notice);
    }

    /// What a lead's end does to the answer turns that were open when it came. Expected ends
    /// (quit, retirement, refusal) close them all. An unexpected end leaves open the turns that
    /// have not been continued yet, so the desk's next tick restarts the lead and continues them
    /// once ([`Self::continue_one`]); those that already had their one continuation are closed,
    /// so a second unexpected end falls back to asking for his words.
    fn close_answer_turns_at_end(&self, c: &mut Conversation) -> AnswerTurnsAtEnd {
        let expected = c.quitting;
        let mut changed = false;
        let (mut kept, mut spent) = (false, false);
        for relay in c.record.answers.iter_mut().filter(|a| a.turn_open) {
            if !expected && relay.continued.is_none() {
                kept = true;
            } else {
                relay.turn_open = false;
                changed = true;
                spent |= !expected;
            }
        }
        if changed {
            self.save(&c.paths.record, &c.record);
        }
        if kept {
            AnswerTurnsAtEnd::Continuing
        } else if spent {
            AnswerTurnsAtEnd::RetrySpent
        } else {
            AnswerTurnsAtEnd::NoneOpen
        }
    }

    // ---- (d): stops -------------------------------------------------------------------------

    /// **Stop the named agents, and nothing else** (r3 (d) items 1-4 and 7; §67). Accepted from
    /// every channel: a stop only removes (§67), so `origin` is recorded and never gates it.
    pub fn stop_named(&self, names: &[String], words: &str, origin: Origin) -> Vec<StopResult> {
        self.log(&format!("stop of {} from {:?}: {words:?}", names.join(", "), origin));
        if let Err(why) = self.engine.stop_words(names, words) {
            // The ack line is the engine's record of his stop; the stop itself still goes out.
            self.log(&format!("stop.sh did not run cleanly: {why}"));
        }
        // **Every named agent is stopped at once, and each is still reported only on its own
        // witnessed death** (hunt 2026-09-29 part 1, finding 33). Their stops are independent:
        // one agent's slow control reply or slow resolver reading must not keep the next agent
        // working. Before, the second agent's `stop_task` was sent only after the first
        // agent's whole confirmation (up to two attempts of `STOP_WAIT` each, plus its
        // registry step). The results come back in the order he named them.
        if names.len() <= 1 {
            return names.iter().map(|name| self.stop_one(name, words)).collect();
        }
        std::thread::scope(|scope| {
            let running: Vec<_> = names.iter().map(|name| scope.spawn(move || self.stop_one(name, words))).collect();
            running
                .into_iter()
                .zip(names)
                .map(|(stop, name)| {
                    stop.join().unwrap_or_else(|_| StopResult::Failed {
                        name: name.clone(),
                        why: "the stop could not be followed to its end".into(),
                    })
                })
                .collect()
        })
    }

    fn owner_of(&self, name: &str) -> Option<(Arc<dyn LeadHandle>, String)> {
        let all: Vec<Arc<Mutex<Conversation>>> = self.conversations.lock().unwrap().values().cloned().collect();
        let mut best: Option<(Arc<dyn LeadHandle>, String, bool)> = None;
        for conversation in all {
            let lead = conversation.lock().unwrap().lead.clone();
            let Some(lead) = lead else { continue };
            if let Some(task) = lead.tasks().latest(name).cloned() {
                let running = task.status.is_running();
                if best.as_ref().is_none_or(|(_, _, r)| !r && running) {
                    best = Some((lead, task.task_id, running));
                }
            }
        }
        best.map(|(lead, task, _)| (lead, task))
    }

    fn stop_one(&self, name: &str, words: &str) -> StopResult {
        let Some((lead, task_id)) = self.owner_of(name) else {
            return StopResult::NotFound { name: name.to_string() };
        };
        let began = Instant::now();
        for attempt in 0..2 {
            if let Err(e) = lead.stop_task(&task_id) {
                return StopResult::Failed { name: name.to_string(), why: e.to_string() };
            }
            let deadline = Instant::now() + STOP_WAIT;
            loop {
                match self.engine.liveness(&task_id) {
                    AgentLiveness::NotAlive => {
                        let seconds = (began.elapsed().as_millis() as f64) / 1000.0;
                        let registry = self.registry_step(&lead, name, &task_id, words);
                        return StopResult::Stopped { name: name.to_string(), seconds, registry };
                    }
                    AgentLiveness::Indeterminate(why) if Instant::now() >= deadline => {
                        return StopResult::Indeterminate { name: name.to_string(), why };
                    }
                    _ if Instant::now() >= deadline => break,
                    _ => std::thread::sleep(Duration::from_millis(250)),
                }
            }
            self.log(&format!("{name} still ALIVE {} s after stop attempt {}", STOP_WAIT.as_secs(), attempt + 1));
        }
        StopResult::StillAlive { name: name.to_string() }
    }

    /// r4 §2.1, item 3a: once the stream says `stopped` for the task this host stopped AND the
    /// resolver says NOT-ALIVE, tell his registry what happened, never before it happened.
    fn registry_step(&self, lead: &Arc<dyn LeadHandle>, name: &str, task_id: &str, words: &str) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let stopped = lead.tasks().agents().into_iter()
                .any(|a| a.task_id == task_id && matches!(a.status, TaskStatus::Stopped | TaskStatus::Killed));
            if stopped {
                break;
            }
            if Instant::now() >= deadline {
                let why = "the stream never said it stopped, so the registry was not told".to_string();
                self.log(&format!("{name}: {why}"));
                return Err(why);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        self.engine.registry_stop(&lead.session_id(), name, words).map(|_| ())
            .inspect_err(|why| self.log(&format!("{name}: workspaces.sh stop failed: {why}")))
    }

    /// (d) item 5: the per-assignment Stop stops exactly the names seen starting on that
    /// assignment's turns, plus the names its reports named. F8: the handle is then held, and
    /// the lead is told at once, with what the stop measured.
    pub fn stop_assignment(&self, key: &ConversationKey, handle: &str, words: &str, origin: Origin) -> Vec<StopResult> {
        let names: Vec<String> = self.conversations.lock().unwrap().get(key)
            .and_then(|c| c.lock().unwrap().handle_agents.get(handle).cloned())
            .map(|set| set.into_iter().collect()).unwrap_or_default();
        if names.is_empty() {
            return Vec::new();
        }
        let results = self.stop_named(&names, words, origin);
        let measured = results.iter().map(StopResult::sentence).collect::<Vec<_>>().join(" ");
        self.hold(key, handle, Some(&measured));
        results
    }

    /// **F8: hold a handle he stopped or interrupted.** It leaves `open_handles` and joins
    /// `held`. With `tell_now` (a Stop), the lead is sent [`HELD_LINE`] at once, followed by what
    /// was measured; if it has no running lead or the write fails, the line waits for its next
    /// message instead, which is also where an Esc's goes.
    fn hold(&self, key: &ConversationKey, handle: &str, tell_now: Option<&str>) {
        let Some(conversation) = self.conversations.lock().unwrap().get(key).cloned() else { return };
        let mut c = conversation.lock().unwrap();
        c.record.open_handles.remove(handle);
        c.record.held.insert(handle.to_string());
        let lead = c.lead.clone().filter(|l| !l.exited());
        let told = match (tell_now, lead) {
            (Some(measured), Some(lead)) => {
                let one = BTreeSet::from([handle.to_string()]);
                match lead.send(&format!("{}\n\n{measured}", held_line(&one))) {
                    Ok(uuid) => {
                        c.sent.insert(uuid.clone(), None);
                        c.awaiting.insert(uuid);
                        c.last_activity = Instant::now();
                        true
                    }
                    Err(e) => {
                        self.log(&format!("{}/{}: {handle} is held; the lead could not be told now ({e}), so its \
                                           next message says so", key.entity_id, key.thread_id));
                        false
                    }
                }
            }
            _ => false,
        };
        if !told {
            c.record.untold.insert(handle.to_string());
        }
        self.log(&format!("{}/{}: {handle} held ({})", key.entity_id, key.thread_id,
                          if tell_now.is_some() { "Stop" } else { "Esc" }));
        self.save(&c.paths.record, &c.record);
    }

    /// (d) item 6, his Esc: end the lead's running turn, never its agents (P3). Returns the
    /// sentence the front desk says, with how many of his messages still run next (r4 §1.2).
    /// F8: the assignment whose turn it was is held, and the lead hears so with its next message.
    pub fn interrupt(&self, key: &ConversationKey) -> String {
        let (lead, turn_handle) = match self.conversations.lock().unwrap().get(key) {
            Some(c) => { let c = c.lock().unwrap(); (c.lead.clone(), c.turn_handle.clone()) }
            None => (None, None),
        };
        let Some(lead) = lead else { return "Your team isn't doing anything in this conversation.".into() };
        let replied = lead.interrupt();
        if replied.is_ok() {
            // He ended that turn: an answer it had taken is not carried on without his words.
            let conversation = self.conversations.lock().unwrap().get(key).cloned();
            if let Some(conversation) = conversation {
                self.close_answer_turns(&mut conversation.lock().unwrap());
            }
        }
        if let (Ok(_), Some(handle)) = (&replied, turn_handle) {
            self.hold(key, &handle, None);
        }
        match replied {
            Ok(reply) => match reply.still_queued.len() {
                0 => "Stopped what your team was doing here. Its agents keep running.".into(),
                1 => "Stopped what your team was doing here. Its agents keep running, and 1 of your messages is still queued and runs next.".into(),
                n => format!("Stopped what your team was doing here. Its agents keep running, and {n} of your messages are still queued and run next."),
            },
            Err(e) => format!("Your team could not be interrupted ({e}). It is still running."),
        }
    }

    // ---- (o) and (m): reads -----------------------------------------------------------------

    fn read_one(&self, conversation: &Arc<Mutex<Conversation>>, leases: &[String]) -> ConversationRead {
        let c = conversation.lock().unwrap();
        let agents = c.lead.as_ref().map(|l| l.tasks().agents().into_iter()
            .map(|a| (a.name, a.status.as_str().to_string())).collect()).unwrap_or_default();
        let title = c.title.clone();
        ConversationRead {
            key: c.key.clone(),
            lead_running: c.lead.as_ref().is_some_and(|l| !l.exited()),
            agents,
            texts: c.texts.iter().cloned().collect(),
            open_questions: c.questions.iter().map(|(h, q)| match h {
                Some(h) => format!("on {h}: {q} (asked, no answer relayed yet)"),
                None => format!("{q} (asked, no answer relayed yet)"),
            }).collect(),
            leases: leases.iter().filter(|l| lease_held_by(l, &title)).cloned().collect(),
        }
    }

    /// (o): this conversation's read, or every conversation's on request. A read adds no
    /// capability, so it is answered on every channel.
    pub fn read(&self, key: &ConversationKey, every: bool) -> Vec<ConversationRead> {
        let leases = self.engine.held_leases();
        let all: Vec<Arc<Mutex<Conversation>>> = self.conversations.lock().unwrap().iter()
            .filter(|(k, _)| every || *k == key).map(|(_, c)| c.clone()).collect();
        all.iter().map(|c| self.read_one(c, &leases)).collect()
    }

    /// (m): his team counts as running while any agent of any lead is ALIVE, or any lead's
    /// supervisor records a live descendant outside the lead's own group.
    pub fn team(&self) -> TeamReading {
        self.team_reading(|agent| self.resolved_liveness(agent))
    }

    /// **The resolver's answer, without asking it again about an agent it already saw end**
    /// (hunt 2026-09-29 part 1, finding 34). While an update waits for him the gate reads his
    /// team every five seconds, and each reading ran `agent-liveness.sh` (up to 30 s each, one
    /// after another) for every agent any lead ever started, finished ones included.
    ///
    /// The resolver stays the authority (the update gate keeps it rather than the stream's
    /// reading, r3 (m)): an agent is taken as ended without asking only after the resolver
    /// itself answered NOT-ALIVE while the stream said it had ended, and only while the stream
    /// still says that same thing. A running agent is asked on every reading, and an agent the
    /// stream sees start again is asked again. What this cannot see is a restart this app's
    /// leads never report, which the operator claim (his team belongs to this app while it
    /// runs) rules out.
    fn resolved_liveness(&self, agent: &crate::operator_lead::AgentTask) -> AgentLiveness {
        let ended = !agent.status.is_running();
        if ended && self.witnessed_ended.lock().unwrap().get(&agent.task_id) == Some(&agent.status) {
            return AgentLiveness::NotAlive;
        }
        let reading = self.engine.liveness(&agent.task_id);
        let mut witnessed = self.witnessed_ended.lock().unwrap();
        if ended && reading == AgentLiveness::NotAlive {
            witnessed.insert(agent.task_id.clone(), agent.status);
        } else {
            witnessed.remove(&agent.task_id);
        }
        reading
    }

    /// **(m) without a subprocess**, for a caller that must answer at once: the app's exit
    /// decision runs inside the runtime's own callback, whose answer is read the instant it
    /// returns (background-work spec §2.5a), so it cannot wait on `agent-liveness.sh`. An agent
    /// counts as running while its own stream says so (`task_started` without a later end, the
    /// platform's positive signals, r4 §1.1): an agent the stream has not seen end is never
    /// read as gone. The update gate, which is not in a callback, keeps [`Self::team`].
    pub fn team_from_stream(&self) -> TeamReading {
        self.team_reading(|agent| if agent.status.is_running() { AgentLiveness::Alive } else { AgentLiveness::NotAlive })
    }

    fn team_reading(&self, liveness: impl Fn(&crate::operator_lead::AgentTask) -> AgentLiveness) -> TeamReading {
        let mut reading = TeamReading::default();
        let all: Vec<Arc<Mutex<Conversation>>> = self.conversations.lock().unwrap().values().cloned().collect();
        for conversation in all {
            let (lead, state, busy, name) = {
                let c = conversation.lock().unwrap();
                let name = if c.title.is_empty() { c.key.thread_id.clone() } else { c.title.clone() };
                (c.lead.clone(), c.paths.reap_state.clone(), c.in_turn || !c.awaiting.is_empty(), name)
            };
            let Some(lead) = lead else { continue };
            if busy && !lead.exited() {
                reading.working.push(name);
            }
            for agent in lead.tasks().agents() {
                match liveness(&agent) {
                    AgentLiveness::Alive => reading.alive.push(agent.name),
                    AgentLiveness::Indeterminate(_) => reading.unknown.push(agent.name),
                    AgentLiveness::NotAlive => {}
                }
            }
            match live_descendants(&state) {
                Some(n) => reading.descendants += n,
                None if !lead.exited() => reading.descendants_unknown += 1,
                None => {}
            }
        }
        reading
    }

    // ---- (q): retirement and quit -----------------------------------------------------------

    /// How many conversations have a lead running right now. The idle timer asks this first,
    /// so an app with no lead running spends nothing on his engine's scripts every minute.
    pub fn running_leads(&self) -> usize {
        let all: Vec<Arc<Mutex<Conversation>>> = self.conversations.lock().unwrap().values().cloned().collect();
        all.iter().filter(|c| c.lock().unwrap().lead.as_ref().is_some_and(|l| !l.exited())).count()
    }

    /// (q) item 4: retire every lead with nothing running that has been idle `idle_after`, or
    /// at once when its start-time snapshot is stale. Returns the conversations retired. The
    /// next message resumes each (r3 (l)).
    pub fn retire_idle(&self, idle_after: Duration) -> Vec<ConversationKey> {
        let digest = snapshot_digest(&self.declaration);
        let leases = self.engine.held_leases();
        let all: Vec<Arc<Mutex<Conversation>>> = self.conversations.lock().unwrap().values().cloned().collect();
        let mut retired = Vec::new();
        for conversation in all {
            let mut c = conversation.lock().unwrap();
            let Some(lead) = c.lead.clone() else { continue };
            let stale = c.started_digest != digest;
            if c.in_turn || !c.awaiting.is_empty() || (!stale && c.last_activity.elapsed() < idle_after) {
                continue;
            }
            // Nothing running: no ALIVE (or undecided) agent, no live descendant outside the
            // lead's own group (G8: language servers and MCP servers share its group and do not
            // count), and no land lease held by this conversation.
            let agents_busy = lead.tasks().agents().iter().any(|a| self.resolved_liveness(a) != AgentLiveness::NotAlive);
            let descendants = live_descendants(&c.paths.reap_state);
            let lease = leases.iter().any(|l| lease_held_by(l, &c.title));
            if agents_busy || descendants != Some(0) || lease {
                continue;
            }
            c.quitting = true;
            c.lead = None;
            c.first_after_resume = true;
            let key = c.key.clone();
            drop(c);
            let quit = lead.quit();
            self.log(&format!("{}/{}: retired idle{} ({quit:?})", key.entity_id, key.thread_id,
                              if stale { ", its start snapshot was stale" } else { "" }));
            retired.push(key);
        }
        retired
    }

    /// The quit path: every lead by SIGTERM to its supervisor (r3 (q) item 2).
    pub fn quit_all(&self) -> Vec<(ConversationKey, Quit)> {
        let all: Vec<Arc<Mutex<Conversation>>> = self.conversations.lock().unwrap().values().cloned().collect();
        let mut leads = Vec::new();
        for conversation in all {
            let mut c = conversation.lock().unwrap();
            c.quitting = true;
            // He chose to quit while his team worked: that work is stopped (the quit
            // sheet's "The work is stopped"), and the next launch does not carry it on.
            if c.in_turn || !c.awaiting.is_empty() {
                self.close_answer_turns(&mut c);
            }
            if let Some(lead) = c.lead.take() {
                leads.push((c.key.clone(), lead));
            }
        }
        // **Every lead is asked to end at once** (hunt 2026-09-29 part 1, finding 35). Each
        // lead still gets its whole grace (`QUIT_GRACE`: SIGTERM to its supervisor, which reaps
        // the lead's tree, and only then the group kill, r3 (q) item 2), but the graces now run
        // side by side instead of adding up per conversation. This returns only when every lead
        // has ended, so the claim, which the desk releases after it, is still given up only
        // after every lead has gone (e item 3).
        let out: Vec<(ConversationKey, Quit)> = std::thread::scope(|scope| {
            let ending: Vec<_> = leads
                .into_iter()
                .map(|(key, lead)| (key, scope.spawn(move || lead.quit())))
                .collect();
            ending
                .into_iter()
                // A quit that panicked is re-raised here, as it was when this ran inline: no
                // `Quit` would be a true account of a lead whose ending was never finished.
                .map(|(key, quit)| (key, quit.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic))))
                .collect()
        });
        for (key, quit) in &out {
            self.log(&format!("{}/{}: quit ({quit:?})", key.entity_id, key.thread_id));
        }
        out
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::assignment::{AssignmentKind, Registration};
    use crate::operator_declaration::ClaimPaths;
    use crate::operator_lead::AgentTask;
    use serde_json::json;
    use std::collections::BTreeMap;

    // ---- fakes -----------------------------------------------------------------------------

    #[derive(Default)]
    pub(crate) struct FakeLead {
        pub(crate) session: String,
        pub(crate) sent: Mutex<Vec<String>>,
        pub(crate) stops: Mutex<Vec<String>>,
        pub(crate) book: Mutex<TaskBook>,
        pub(crate) exited: Mutex<bool>,
        pub(crate) quits: Mutex<usize>,
        pub(crate) fail_send: Mutex<bool>,
        pub(crate) queued: usize,
        /// The uuid of every message, in order ("u-N" for the lead's own, the host's for answers).
        pub(crate) uuids: Mutex<Vec<String>>,
        /// When set, each chosen-uuid send first reads this file (the lead record) and notes
        /// whether the uuid was already on disk: the intent-before-send check (design §4.1 test 5).
        pub(crate) record_probe: Mutex<Option<PathBuf>>,
        pub(crate) on_disk_at_send: Mutex<Vec<bool>>,
        /// Runs inside `quit`, after it is counted and before the lead is ended, so a test can
        /// make one lead's quit take its time (a slow supervisor) and see what happens meanwhile.
        pub(crate) quit_hook: Mutex<Option<QuitHook>>,
    }
    pub(crate) type QuitHook = Arc<dyn Fn() + Send + Sync>;
    impl FakeLead {
        pub(crate) fn feed(&self, frame: Value) {
            self.book.lock().unwrap().observe(&frame);
        }
    }
    impl LeadHandle for FakeLead {
        fn session_id(&self) -> String { self.session.clone() }
        fn send(&self, text: &str) -> Result<String, LeadError> {
            if *self.fail_send.lock().unwrap() {
                return Err(LeadError::Io("Broken pipe".into()));
            }
            self.sent.lock().unwrap().push(text.to_string());
            let uuid = format!("u-{}", self.sent.lock().unwrap().len());
            self.uuids.lock().unwrap().push(uuid.clone());
            Ok(uuid)
        }
        fn send_with_uuid(&self, uuid: &str, text: &str) -> Result<(), LeadError> {
            if *self.fail_send.lock().unwrap() {
                return Err(LeadError::Io("Broken pipe".into()));
            }
            if let Some(path) = self.record_probe.lock().unwrap().clone() {
                let on_disk = std::fs::read_to_string(path).unwrap_or_default().contains(uuid);
                self.on_disk_at_send.lock().unwrap().push(on_disk);
            }
            self.sent.lock().unwrap().push(text.to_string());
            self.uuids.lock().unwrap().push(uuid.to_string());
            Ok(())
        }
        fn stop_task(&self, task_id: &str) -> Result<(), LeadError> {
            self.stops.lock().unwrap().push(task_id.to_string());
            Ok(())
        }
        fn interrupt(&self) -> Result<InterruptReply, LeadError> {
            Ok(InterruptReply { still_queued: (0..self.queued).map(|i| format!("q-{i}")).collect() })
        }
        fn tasks(&self) -> TaskBook { self.book.lock().unwrap().clone() }
        fn exited(&self) -> bool { *self.exited.lock().unwrap() }
        fn quit(&self) -> Quit {
            *self.quits.lock().unwrap() += 1;
            let hook = self.quit_hook.lock().unwrap().clone();
            if let Some(hook) = hook {
                hook();
            }
            *self.exited.lock().unwrap() = true;
            Quit::Terminated { waited: Duration::ZERO }
        }
    }

    #[derive(Default)]
    pub(crate) struct FakeLauncher {
        pub(crate) leads: Mutex<Vec<(ConversationKey, LeadStart, Arc<FakeLead>)>>,
        pub(crate) queued: usize,
    }
    impl LeadLauncher for FakeLauncher {
        fn launch(&self, key: &ConversationKey, _title: &str, start: &LeadStart, _paths: &ConversationPaths,
                  _sink: Arc<dyn LeadSink>) -> Result<Arc<dyn LeadHandle>, String> {
            let session = match start { LeadStart::New(s) | LeadStart::Resume(s) => s.clone() };
            let lead = Arc::new(FakeLead { session, queued: self.queued, ..Default::default() });
            self.leads.lock().unwrap().push((key.clone(), start.clone(), lead.clone()));
            Ok(lead)
        }
        fn fences(&self) -> Result<(), String> { Ok(()) }
    }

    /// `not_alive` holds AGENT IDS (task ids), as the real resolver is asked.
    #[derive(Default)]
    pub(crate) struct FakeEngine {
        pub(crate) stop_words: Mutex<Vec<(Vec<String>, String)>>,
        pub(crate) registry: Mutex<Vec<(String, String, String)>>,
        pub(crate) not_alive: Mutex<BTreeSet<String>>,
        pub(crate) asked: Mutex<Vec<String>>,
        pub(crate) leases: Mutex<Vec<String>>,
        pub(crate) lease_reads: Mutex<usize>,
        /// Runs before every liveness answer, so a test can hold one agent's reading open (a
        /// slow `agent-liveness.sh`) and see what the host does meanwhile.
        pub(crate) before_liveness: Mutex<Option<LivenessHook>>,
    }
    pub(crate) type LivenessHook = Arc<dyn Fn(&str) + Send + Sync>;
    impl OperatorEngine for FakeEngine {
        fn stop_words(&self, names: &[String], words: &str) -> Result<String, String> {
            self.stop_words.lock().unwrap().push((names.to_vec(), words.to_string()));
            Ok(String::new())
        }
        fn liveness(&self, agent_id: &str) -> AgentLiveness {
            let hook = self.before_liveness.lock().unwrap().clone();
            if let Some(hook) = hook {
                hook(agent_id);
            }
            self.asked.lock().unwrap().push(agent_id.to_string());
            if self.not_alive.lock().unwrap().contains(agent_id) { AgentLiveness::NotAlive } else { AgentLiveness::Alive }
        }
        fn registry_stop(&self, session: &str, name: &str, words: &str) -> Result<String, String> {
            self.registry.lock().unwrap().push((session.into(), name.into(), words.into()));
            Ok("stopped".into())
        }
        fn held_leases(&self) -> Vec<String> {
            *self.lease_reads.lock().unwrap() += 1;
            self.leases.lock().unwrap().clone()
        }
    }

    #[derive(Default)]
    pub(crate) struct FakeSettle {
        pub(crate) calls: Mutex<Vec<(String, String, Vec<String>)>>,
        pub(crate) refuse: Mutex<Option<String>>,
    }
    impl Settle for FakeSettle {
        fn complete(&self, _key: &ConversationKey, obligation: &str, _source: &str, status: &str, evidence: &[String],
                    _answer: &str) -> Result<(), String> {
            if let Some(why) = self.refuse.lock().unwrap().clone() {
                return Err(why);
            }
            self.calls.lock().unwrap().push((obligation.into(), status.into(), evidence.to_vec()));
            Ok(())
        }
    }

    #[derive(Default)]
    pub(crate) struct Said(pub(crate) Mutex<Vec<(ConversationKey, Lane, Say, String)>>);
    impl OperatorDelivery for Said {
        fn say(&self, key: &ConversationKey, lane: &Lane, kind: Say, text: &str) {
            self.0.lock().unwrap().push((key.clone(), lane.clone(), kind, text.to_string()));
        }
    }
    impl Said {
        pub(crate) fn all(&self) -> Vec<(ConversationKey, Lane, Say, String)> { self.0.lock().unwrap().clone() }
    }

    struct Rig {
        root: PathBuf,
        host: Arc<OperatorHost>,
        launcher: Arc<FakeLauncher>,
        engine: Arc<FakeEngine>,
        settle: Arc<FakeSettle>,
        said: Arc<Said>,
        state: PathBuf,
        declaration: Declaration,
    }
    impl Drop for Rig {
        fn drop(&mut self) {
            if let Err(error) = std::fs::remove_dir_all(&self.root) { eprintln!("fixture cleanup: {error}"); }
        }
    }

    pub(crate) fn declaration(root: &Path, origins: &[&str]) -> Declaration {
        let home = root.join("home");
        let entity = home.join("ab/femcboost");
        std::fs::create_dir_all(&entity).unwrap();
        std::fs::write(entity.join("CLAUDE.md"), "rules\n").unwrap();
        std::fs::write(entity.join("orchestration.config"), "OPERATOR_FENCES=\"on\"\n").unwrap();
        let mut environment = BTreeMap::new();
        environment.insert("PATH".to_string(), "/usr/bin:/bin".to_string());
        Declaration {
            entity_root: entity, engine_root: home.join("ab/richos/richos/engine"), home: home.clone(),
            claim: ClaimPaths { file: home.join(".claude/state/operator-lead.json"), lock: home.join(".claude/state/operator-lead.lock") },
            permission_mode: "bypassPermissions".into(), environment,
            origins: origins.iter().map(|s| s.to_string()).collect(), file_roots: vec![home.join("ab")],
            contract_path: home.join("ab/contract.md"), contract_sha256: "0".repeat(64),
        }
    }

    fn rig_with(origins: &[&str], queued: usize) -> Rig {
        let root = std::env::temp_dir().join(format!("operator-host-{}", uuid::Uuid::new_v4()));
        let root = { std::fs::create_dir_all(&root).unwrap(); std::fs::canonicalize(&root).unwrap() };
        let declaration = declaration(&root, origins);
        let launcher = Arc::new(FakeLauncher { queued, ..Default::default() });
        let engine = Arc::new(FakeEngine::default());
        let settle = Arc::new(FakeSettle::default());
        let said = Arc::new(Said::default());
        let state = root.join("engine-state");
        let host = OperatorHost::new(declaration.clone(), &state, &root.join("operator"), launcher.clone(), engine.clone(),
                                     settle.clone(), said.clone(), Arc::new(SayQuestions(said.clone())));
        Rig { root, host, launcher, engine, settle, said, state, declaration }
    }

    fn rig() -> Rig {
        rig_with(&["desk-typed", "desk-voice", "desk-file"], 0)
    }

    fn key(thread: &str) -> ConversationKey {
        ConversationKey { entity_id: "femcboost".into(), thread_id: thread.into() }
    }

    fn lead_of(r: &Rig, thread: &str) -> Arc<FakeLead> {
        r.launcher.leads.lock().unwrap().iter().rev().find(|(k, _, _)| k.thread_id == thread).unwrap().2.clone()
    }

    fn register(r: &Rig, thread: &str, kind: AssignmentKind) -> String {
        assignment::register_kind(&r.state, &Registration {
            entity_id: "femcboost".into(), thread_id: thread.into(), obligation_id: format!("ob-{}", uuid::Uuid::new_v4()),
            instruction_ledger_ref: format!("ledger:{thread}:1"), instruction_sha256: "a".repeat(64),
            title: "Fixture work".into(), repositories: vec![], needs_screen: false,
        }, kind).unwrap().id
    }

    fn outbox(r: &Rig, thread: &str, records: &[Value]) {
        let path = ConversationPaths::under(&r.root.join("operator"), &key(thread)).outbox;
        let mut text = std::fs::read_to_string(&path).unwrap_or_default();
        for v in records {
            text.push_str(&v.to_string());
            text.push('\n');
        }
        std::fs::write(path, text).unwrap();
    }

    fn report(handle: Option<&str>, kind: &str, text: &str) -> Value {
        json!({"version":1,"at_ms":1,"lead":"claim-1","entity_id":"femcboost","thread_id":"t","handle":handle,"kind":kind,
               "text":text,"attachment":null,"lands":[],"files":[],"agents":[]})
    }

    fn turn(started_by: &[&str], text: Option<&str>) -> LeadEvent {
        LeadEvent::TurnEnded(TurnEnd { started_by: started_by.iter().map(|s| s.to_string()).collect(),
                                       text: text.map(str::to_string), is_error: false, subtype: "success".into() })
    }

    pub(crate) fn agent(lead: &FakeLead, tool: &str, name: &str, task: &str) {
        lead.feed(json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":tool,"name":"Agent","input":{"name":name}}]}}));
        lead.feed(json!({"type":"system","subtype":"task_started","task_id":task,"tool_use_id":tool}));
    }

    // ---- (g) relay, never hold; (s) the origin rule -----------------------------------------

    #[test]
    fn every_message_is_relayed_at_once_and_one_lead_serves_the_conversation() {
        let r = rig();
        for i in 0..3 {
            let got = r.host.relay(&key("a"), "Conversation A", None, &format!("message {i}"), Origin::DeskTyped).unwrap();
            assert!(matches!(got, Relayed::Sent { .. }), "nothing is held behind a running turn");
        }
        assert_eq!(r.launcher.leads.lock().unwrap().len(), 1, "one lead per conversation");
        assert_eq!(lead_of(&r, "a").sent.lock().unwrap().len(), 3);
        r.host.relay(&key("b"), "Conversation B", None, "other", Origin::DeskVoice).unwrap();
        assert_eq!(r.launcher.leads.lock().unwrap().len(), 2, "a second conversation gets its own lead");
    }

    /// **Second review, slice 4 (Sage's check §1.3): a review-watch notice reaches the lead whose
    /// session started the work, as a message this host sends** (a uuid it awaits, so the turn
    /// it starts is one the host attributes); found in a later launch through the saved record;
    /// a session no conversation names gets nothing.
    #[test]
    fn a_review_watch_notice_reaches_the_lead_of_its_session_as_the_hosts_own_message() {
        let r = rig();
        r.host.relay(&key("a"), "A", None, "build it", Origin::DeskTyped).unwrap();
        r.host.relay(&key("b"), "B", None, "something else", Origin::DeskTyped).unwrap();
        let (a, b) = (lead_of(&r, "a"), lead_of(&r, "b"));
        let uuid = r.host.tell_lead(&a.session, "REVIEW-WATCH 10:00Z: 1 second-review notice").unwrap();
        assert_eq!(a.sent.lock().unwrap().last().unwrap(), "REVIEW-WATCH 10:00Z: 1 second-review notice");
        assert_eq!(b.sent.lock().unwrap().len(), 1, "the other conversation's lead was told");
        let conversation = r.host.conversation(&key("a"), "");
        assert!(conversation.lock().unwrap().awaiting.contains(&uuid), "the host does not await the turn it started");
        assert!(r.host.tell_lead("no-such-session", "x").is_err());
        // A later launch (a new host over the same folders) finds it through the saved record,
        // and resumes that session's lead to tell it.
        let later = OperatorHost::new(r.declaration.clone(), &r.state, &r.root.join("operator"), r.launcher.clone(),
                                      r.engine.clone(), r.settle.clone(), r.said.clone(),
                                      Arc::new(SayQuestions(r.said.clone())));
        later.tell_lead(&a.session, "REVIEW-WATCH 11:00Z: 1 second-review notice").unwrap();
        let (_, start, resumed) = r.launcher.leads.lock().unwrap().last().cloned().unwrap();
        assert_eq!(start, LeadStart::Resume(a.session.clone()));
        assert_eq!(resumed.sent.lock().unwrap().last().unwrap(), "REVIEW-WATCH 11:00Z: 1 second-review notice");
    }

    #[test]
    fn a_phone_assignment_is_not_relayed_and_a_turn_with_no_origin_relays_nothing() {
        let r = rig();
        assert_eq!(r.host.relay(&key("a"), "A", Some("h-1"), "land it", Origin::Phone).unwrap(),
                   Relayed::Not { sentence: Some(PHONE_ASSIGNMENT.into()) });
        assert_eq!(r.host.relay(&key("a"), "A", None, "proactive words", Origin::NoOrigin).unwrap(),
                   Relayed::Not { sentence: None });
        assert!(r.launcher.leads.lock().unwrap().is_empty(), "no lead was even started");
        // Opening the phone is one line in the declaration, and his call.
        let open = rig_with(&["desk-typed", "phone"], 0);
        assert!(matches!(open.host.relay(&key("a"), "A", None, "x", Origin::Phone).unwrap(), Relayed::Sent { .. }));
    }

    #[test]
    fn a_turn_s_origin_is_its_recorded_mouth_and_an_unrecorded_one_is_no_origin() {
        use crate::ledger::Source::{Internal, Jam, Proactive, Text};
        assert_eq!(Origin::of_turn(Text, Some("desk")), Origin::DeskTyped);
        assert_eq!(Origin::of_turn(Jam, Some("desk")), Origin::DeskVoice);
        assert_eq!(Origin::of_turn(Text, Some("phone")), Origin::Phone);
        assert_eq!(Origin::of_turn(Text, Some("phone_typed")), Origin::Phone);
        assert_eq!(Origin::of_turn(Text, Some("phone_voice")), Origin::Phone);
        assert_eq!(Origin::of_turn(Jam, Some("phone")), Origin::Phone, "a voice note from the phone is the phone");
        assert_eq!(Origin::of_turn(Text, Some("watch")), Origin::Undeclared);
        assert_eq!(Origin::of_turn(Text, None), Origin::NoOrigin, "not recorded is never the desk");
        assert_eq!(Origin::of_turn(Jam, None), Origin::NoOrigin);
        assert_eq!(Origin::of_turn(Internal, Some("desk")), Origin::NoOrigin, "an internal turn carries none of his words");
        assert_eq!(Origin::of_turn(Proactive, Some("desk")), Origin::NoOrigin);
    }

    #[test]
    fn an_undeclared_mouth_is_refused_as_work_with_the_sentence() {
        let r = rig_with(&["desk-typed", "desk-voice", "desk-file"], 0);
        let said = r.host.relay(&key("t"), "T", None, "do the thing", Origin::Undeclared).unwrap();
        assert_eq!(said, Relayed::Not { sentence: Some(PHONE_ASSIGNMENT.into()) });
        assert!(r.launcher.leads.lock().unwrap().is_empty(), "no lead was started for it");
    }

    #[test]
    fn a_relay_that_cannot_be_written_is_said_and_the_lead_is_kept() {
        let r = rig();
        r.host.relay(&key("a"), "A", None, "first", Origin::DeskTyped).unwrap();
        let lead = lead_of(&r, "a");
        *lead.fail_send.lock().unwrap() = true;
        let why = r.host.relay(&key("a"), "A", None, "second", Origin::DeskTyped).unwrap_err();
        assert!(why.contains("still running"), "{why}");
        assert_eq!(*lead.quits.lock().unwrap(), 0, "r3 (q) item 1: an error never ends a lead");
    }

    #[test]
    fn the_handle_goes_with_the_words_and_the_changed_since_line_with_the_next_relay() {
        let r = rig();
        r.host.relay(&key("a"), "A", Some("h-1"), "Land the phone fix.", Origin::DeskTyped).unwrap();
        let memory = watched_paths(&r.declaration)[0].clone();
        std::fs::create_dir_all(&memory).unwrap();
        std::thread::sleep(Duration::from_millis(1100));
        std::fs::write(memory.join("MEMORY.md"), "- a new rule\n").unwrap();
        r.host.relay(&key("a"), "A", None, "Next.", Origin::DeskTyped).unwrap();
        let sent = lead_of(&r, "a").sent.lock().unwrap().clone();
        assert!(sent[0].contains("Assignment handle: h-1."), "{}", sent[0]);
        assert!(!sent[0].contains("Changed since"), "nothing changed before the first relay");
        assert!(sent[1].contains("changed_since_last_message") && sent[1].contains("MEMORY.md"), "{}", sent[1]);
        r.host.relay(&key("a"), "A", None, "And again.", Origin::DeskTyped).unwrap();
        let sent = lead_of(&r, "a").sent.lock().unwrap().clone();
        assert!(!sent[2].contains("Changed since"), "said once, not at every relay: {}", sent[2]);
    }

    // ---- (c) reports settle; the last text ---------------------------------------------------

    #[test]
    fn only_a_report_settles_and_its_lands_are_the_evidence() {
        let r = rig();
        let handle = register(&r, "a", AssignmentKind::Task);
        r.host.relay(&key("a"), "A", Some(&handle), "Land it.", Origin::DeskTyped).unwrap();
        r.host.handle(&key("a"), turn(&["u-1"], Some("Working on it.")));
        let item = assignment::read(&r.state, "femcboost", "a", &handle).unwrap();
        assert_eq!(item.state, AssignmentState::Registered, "a turn ending never settles");
        assert!(open_handles(&r, "a").contains(&handle));
        outbox(&r, "a", &[with_lands(report(Some(&handle), "outcome", "Landed and pushed."), &[confirmed_land()])]);
        r.host.handle(&key("a"), turn(&[], None));
        let calls = r.settle.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, "completed");
        assert_eq!(calls[0].2.len(), 2, "the confirmed land and the answer: {:?}", calls[0].2);
        assert_eq!(calls[0].2[0], format!("git:/r:main:{}", "a".repeat(40)));
        assert!(calls[0].2[1].starts_with("answer:"));
        let item = assignment::read(&r.state, "femcboost", "a", &handle).unwrap();
        assert_eq!(item.state, AssignmentState::Settled);
        assert!(r.said.all().iter().any(|x| x.2 == Say::Outcome && x.3.contains("Landed and pushed x in r.")));
        // A second read of the same outbox settles nothing twice.
        r.host.handle(&key("a"), turn(&[], None));
        assert_eq!(r.settle.calls.lock().unwrap().len(), 1);
    }

    fn confirmed_land() -> Value {
        json!({"repository":"/r","commit":"a".repeat(40),"branch":"x","into":"main","landed":true,
               "pushed":true,"why":null,"says":"Landed and pushed x in r."})
    }

    fn unconfirmed_land() -> Value {
        json!({"repository":"/r","commit":"b".repeat(40),"branch":"y","into":"main","landed":false,
               "pushed":null,"why":"commit bbbb is not on main","says":"y in r could not be confirmed as landed."})
    }

    fn with_lands(mut record: Value, lands: &[Value]) -> Value {
        record["lands"] = Value::Array(lands.to_vec());
        record
    }

    fn open_handles(r: &Rig, thread: &str) -> BTreeSet<String> {
        r.host.conversations.lock().unwrap().get(&key(thread)).unwrap().lock().unwrap().record.open_handles.clone()
    }

    /// **Q3 (c) of Frank's review (richos-hq `bd685c14`), CLAUDE.md "Report the ARTIFACT" and
    /// r3 (c) "Lands are verified in Git before any notice says 'landed'".** An `outcome` that
    /// names a land Git could not confirm does not close the obligation, even beside a land
    /// that was confirmed: the engine is never asked, the register stays where it was, the
    /// handle stays open, and he hears it stays open. The lead's corrected report then closes
    /// it, which is what makes "check what you named and report again" true (F5).
    #[test]
    fn an_outcome_naming_an_unconfirmed_land_closes_nothing_and_the_corrected_report_closes_it() {
        let r = rig();
        let handle = register(&r, "a", AssignmentKind::Task);
        r.host.relay(&key("a"), "A", Some(&handle), "Land it.", Origin::DeskTyped).unwrap();
        outbox(&r, "a", &[with_lands(report(Some(&handle), "outcome", "Landed both."), &[confirmed_land(), unconfirmed_land()])]);
        r.host.handle(&key("a"), turn(&[], None));
        assert!(r.settle.calls.lock().unwrap().is_empty(), "the engine is not asked to close it");
        assert_eq!(assignment::read(&r.state, "femcboost", "a", &handle).unwrap().state, AssignmentState::Registered);
        assert!(open_handles(&r, "a").contains(&handle), "the handle stays open");
        let said = r.said.all();
        assert!(!said.iter().any(|x| x.2 == Say::Outcome), "never said as done: {said:?}");
        let open = said.iter().find(|x| x.2 == Say::Team).expect("he is told");
        assert_eq!(open.1, Lane::Handle(handle.clone()));
        assert!(open.3.contains("could not be confirmed as landed") && open.3.ends_with("so it stays open."), "{}", open.3);
        // The lead checks, finds the land, and reports again: that closes it.
        outbox(&r, "a", &[with_lands(report(Some(&handle), "outcome", "Both landed."), &[confirmed_land()])]);
        r.host.handle(&key("a"), turn(&[], None));
        assert_eq!(r.settle.calls.lock().unwrap().len(), 1);
        assert_eq!(assignment::read(&r.state, "femcboost", "a", &handle).unwrap().state, AssignmentState::Settled);
        assert!(!open_handles(&r, "a").contains(&handle));
    }

    /// **F5.** A second report on an assignment that is already closed is never answered with
    /// "It stays open": the register says it is closed, so that is what he hears, and the
    /// engine is not asked again. The engine's own refusal of an already-closed obligation
    /// (`engine/ecs/adapters/app.py` `operator_complete`, "only an open assignment can be
    /// closed") says the same, for a register that could not be written when it closed.
    #[test]
    fn a_report_on_an_assignment_already_closed_says_so_and_never_that_it_stays_open() {
        let r = rig();
        let handle = register(&r, "a", AssignmentKind::Task);
        r.host.relay(&key("a"), "A", Some(&handle), "Land it.", Origin::DeskTyped).unwrap();
        outbox(&r, "a", &[report(Some(&handle), "outcome", "Done."), report(Some(&handle), "outcome", "Done, again.")]);
        r.host.handle(&key("a"), turn(&[], None));
        assert_eq!(r.settle.calls.lock().unwrap().len(), 1, "the engine is asked once");
        let second = r.said.all().into_iter().filter(|x| x.2 == Say::Team).collect::<Vec<_>>();
        assert_eq!(second.len(), 1, "{second:?}");
        assert!(second[0].3.contains("already closed") && !second[0].3.contains("stays open"), "{}", second[0].3);
        // The engine's refusal of a closed obligation, with a register that still says open.
        let other = register(&r, "a", AssignmentKind::Task);
        *r.settle.refuse.lock().unwrap() = Some("only an open assignment can be closed; ob-x is completed".into());
        outbox(&r, "a", &[report(Some(&other), "outcome", "Done.")]);
        r.host.handle(&key("a"), turn(&[], None));
        let last = r.said.all().pop().unwrap();
        assert!(last.3.contains("already closed") && !last.3.contains("stays open"), "{}", last.3);
    }

    #[test]
    fn a_failed_report_closes_as_withdrawn_and_a_question_kind_settles_as_an_answer() {
        let r = rig();
        let job = register(&r, "a", AssignmentKind::Task);
        let ask = register(&r, "a", AssignmentKind::Check);
        r.host.relay(&key("a"), "A", Some(&job), "Try it.", Origin::DeskTyped).unwrap();
        outbox(&r, "a", &[report(Some(&job), "failed", "It cannot be done: the remote refuses."),
                          report(Some(&ask), "outcome", "Yes, 42 of them.")]);
        r.host.handle(&key("a"), turn(&[], None));
        let calls = r.settle.calls.lock().unwrap().clone();
        assert_eq!(calls[0].1, ECS_WITHDRAWN);
        assert_eq!(assignment::read(&r.state, "femcboost", "a", &job).unwrap().state, AssignmentState::Failed);
        let answered = assignment::read(&r.state, "femcboost", "a", &ask).unwrap();
        assert_eq!(answered.state, AssignmentState::Settled);
        assert_eq!(answered.notices.last().unwrap().kind, NoticeKind::Answer, "§58: an answer is never 'done'");
    }

    /// A `failed` report that also names a land (part of it landed, the rest cannot be done)
    /// closes as withdrawn on its answer alone. The engine refuses any `git:` item on a
    /// withdrawn close ("a land cannot close a failed assignment", `app.py`
    /// `operator_complete`), so sending the land would leave the failure open for good. The
    /// land is still said to him, in the text.
    #[test]
    fn a_failed_report_that_names_a_land_closes_as_withdrawn_on_its_answer_alone() {
        let r = rig();
        let handle = register(&r, "a", AssignmentKind::Task);
        r.host.relay(&key("a"), "A", Some(&handle), "Try it.", Origin::DeskTyped).unwrap();
        outbox(&r, "a", &[with_lands(report(Some(&handle), "failed", "Half landed; the rest cannot be done."), &[confirmed_land()])]);
        r.host.handle(&key("a"), turn(&[], None));
        let calls = r.settle.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, ECS_WITHDRAWN);
        assert!(calls[0].2.iter().all(|e| e.starts_with("answer:")), "{:?}", calls[0].2);
        let said = r.said.all().into_iter().find(|x| x.2 == Say::Failed).unwrap();
        assert!(said.3.contains("Landed and pushed x in r."), "{}", said.3);
    }

    #[test]
    fn a_refused_settlement_leaves_the_assignment_open_and_says_so() {
        let r = rig();
        let handle = register(&r, "a", AssignmentKind::Task);
        r.host.relay(&key("a"), "A", Some(&handle), "Land it.", Origin::DeskTyped).unwrap();
        *r.settle.refuse.lock().unwrap() = Some("obligation not open".into());
        outbox(&r, "a", &[report(Some(&handle), "outcome", "Done.")]);
        r.host.handle(&key("a"), turn(&[], None));
        assert_eq!(assignment::read(&r.state, "femcboost", "a", &handle).unwrap().state, AssignmentState::Registered);
        assert!(r.said.all().iter().any(|(_, _, k, t)| *k == Say::Team && t.contains("could not record it as closed")));
    }

    /// **Hunt part 1 finding 02** (richos-hq `docs/audits/2026-09-29-hunt/part-1-codex.md`):
    /// a close the engine refused for a passing reason is tried again at the next outbox read,
    /// from the same report, even though the report cursor has moved past it. It is said to
    /// him once when refused and once when it closes, never again at every read, and a read
    /// after it closed asks the engine nothing. What is owed is in the saved lead record, so
    /// the app after a crash still retries it; the second half drives that.
    #[test]
    fn a_refused_close_is_asked_again_at_the_next_outbox_read_and_said_once() {
        let r = rig();
        let handle = register(&r, "a", AssignmentKind::Task);
        r.host.relay(&key("a"), "A", Some(&handle), "Land it.", Origin::DeskTyped).unwrap();
        *r.settle.refuse.lock().unwrap() = Some("the engine's store is locked".into());
        outbox(&r, "a", &[report(Some(&handle), "outcome", "Done.")]);
        r.host.handle(&key("a"), turn(&[], None));
        assert_eq!(assignment::read(&r.state, "femcboost", "a", &handle).unwrap().state, AssignmentState::Registered);
        assert_eq!(record_of(&r, "a").outbox_read, 1, "later reports are not held up behind the refused one");
        // Still refused at the next read: nothing new is said to him.
        r.host.handle(&key("a"), turn(&[], None));
        // The engine takes it now. No new report arrives: the retry comes from the old one.
        *r.settle.refuse.lock().unwrap() = None;
        r.host.handle(&key("a"), turn(&[], None));
        assert_eq!(r.settle.calls.lock().unwrap().len(), 1, "the refused close was never asked again");
        assert_eq!(assignment::read(&r.state, "femcboost", "a", &handle).unwrap().state, AssignmentState::Settled);
        assert!(!open_handles(&r, "a").contains(&handle));
        let said = r.said.all();
        assert_eq!(said.iter().filter(|x| x.2 == Say::Team).count(), 1, "{said:?}");
        assert_eq!(said.iter().filter(|x| x.2 == Say::Outcome).count(), 1, "{said:?}");
        // Closed: a later read asks nothing.
        r.host.handle(&key("a"), turn(&[], None));
        assert_eq!(r.settle.calls.lock().unwrap().len(), 1);

        // **Across a crash.** Refused, then the app dies; the relaunched host owes the retry.
        let other = register(&r, "a", AssignmentKind::Task);
        r.host.relay(&key("a"), "A", Some(&other), "Land the other one.", Origin::DeskTyped).unwrap();
        *r.settle.refuse.lock().unwrap() = Some("the engine's store is locked".into());
        outbox(&r, "a", &[report(Some(&other), "outcome", "Done too.")]);
        r.host.handle(&key("a"), turn(&[], None));
        *r.settle.refuse.lock().unwrap() = None;
        let again = relaunched(&r);
        again.relay(&key("a"), "A", None, "Anything else?", Origin::DeskTyped).unwrap();
        again.handle(&key("a"), turn(&[], None));
        assert_eq!(r.settle.calls.lock().unwrap().len(), 2, "the relaunched app never asked again");
        assert_eq!(assignment::read(&r.state, "femcboost", "a", &other).unwrap().state, AssignmentState::Settled);
    }

    #[test]
    fn a_turn_s_final_text_is_delivered_unless_it_is_the_last_report_s_text() {
        let r = rig();
        r.host.relay(&key("a"), "A", Some("h-1"), "Do it.", Origin::DeskTyped).unwrap();
        outbox(&r, "a", &[report(Some("h-1"), "update", "Half done.")]);
        r.host.handle(&key("a"), turn(&["u-1"], Some("Half done.")));
        let updates: Vec<_> = r.said.all().into_iter().filter(|(_, _, k, _)| *k == Say::Update).collect();
        assert_eq!(updates.len(), 1, "the report was said, and its twin text was not: {updates:?}");
        // A turn that reports and then says more delivers both (r3's B5 edge).
        outbox(&r, "a", &[report(Some("h-1"), "update", "Built.")]);
        r.host.handle(&key("a"), turn(&[], Some("Built. Also: the suite took 19 minutes.")));
        let texts: Vec<String> = r.said.all().into_iter().filter(|(_, _, k, _)| *k == Say::Update).map(|x| x.3).collect();
        assert_eq!(texts, ["Half done.", "Built.", "Built. Also: the suite took 19 minutes."]);
    }

    #[test]
    fn a_platform_turn_speaks_on_the_conversation_and_a_relayed_one_on_its_handle() {
        let r = rig();
        r.host.relay(&key("a"), "A", Some("h-9"), "Do it.", Origin::DeskTyped).unwrap();
        r.host.handle(&key("a"), turn(&["u-1"], Some("Started.")));
        r.host.handle(&key("a"), turn(&[], Some("The teammate finished.")));
        let lanes: Vec<Lane> = r.said.all().into_iter().map(|x| x.1).collect();
        assert_eq!(lanes, [Lane::Handle("h-9".into()), Lane::Conversation]);
        let read = r.host.read(&key("a"), false);
        assert_eq!(read[0].texts, ["Started.", "The teammate finished."]);
    }

    #[test]
    fn a_question_asked_mid_turn_reaches_him_before_the_turn_ends() {
        let r = rig();
        r.host.relay(&key("a"), "A", Some("h-1"), "A long job.", Origin::DeskTyped).unwrap();
        outbox(&r, "a", &[report(Some("h-1"), "question", "Blue or green?")]);
        r.host.handle(&key("a"), LeadEvent::Reported);
        assert!(r.said.all().iter().any(|x| x.2 == Say::Question), "said while the turn still runs");
        // The turn then ends with the same words: said once, not twice.
        r.host.handle(&key("a"), turn(&["u-1"], Some("Blue or green?")));
        assert_eq!(r.said.all().len(), 1, "{:?}", r.said.all());
    }

    // ---- §88 seam: questions out, answers in -------------------------------------------------

    #[test]
    fn a_question_goes_to_the_sink_and_an_answer_reaches_the_lead_once_from_any_channel() {
        let r = rig();
        r.host.relay(&key("a"), "A", Some("h-1"), "Do it.", Origin::DeskTyped).unwrap();
        outbox(&r, "a", &[report(Some("h-1"), "question", "Ship tonight or tomorrow?")]);
        r.host.handle(&key("a"), turn(&[], None));
        assert!(r.said.all().iter().any(|(_, l, k, t)| *k == Say::Question && *l == Lane::Handle("h-1".into())
                                        && t == "Ship tonight or tomorrow?"));
        assert_eq!(r.host.read(&key("a"), false)[0].open_questions.len(), 1);
        assert_eq!(r.host.deliver_answer(&key("a"), "A", Some("h-1"), "delivery-1", "Tomorrow.").unwrap(),
                   AnswerOutcome::InFlight);
        assert_eq!(r.host.deliver_answer(&key("a"), "A", Some("h-1"), "delivery-1", "Tomorrow.").unwrap(),
                   AnswerOutcome::InFlight, "awaited, so not sent again");
        let sent = lead_of(&r, "a").sent.lock().unwrap().clone();
        assert_eq!(sent.iter().filter(|s| s.contains("His answer to your question on h-1")).count(), 1);
        assert!(r.host.read(&key("a"), false)[0].open_questions.is_empty());
    }

    // ---- crash-safe answer delivery (richos-hq docs/plans/2026-09-27-answer-delivery-crash-dedup-design.md) --

    fn record_of(r: &Rig, thread: &str) -> LeadRecord {
        read_record(&ConversationPaths::under(&r.root.join("operator"), &key(thread)).record)
    }

    /// A second host over the same folders: the app after a crash (the first host's memory gone).
    fn relaunched(r: &Rig) -> Arc<OperatorHost> {
        OperatorHost::new(r.declaration.clone(), &r.state, &r.root.join("operator"), r.launcher.clone(), r.engine.clone(),
                          r.settle.clone(), r.said.clone(), Arc::new(SayQuestions(r.said.clone())))
    }

    fn answer_uuid(lead: &FakeLead) -> String {
        answer_uuid_of(lead, "His answer to your question")
    }

    fn answer_uuid_of(lead: &FakeLead, needle: &str) -> String {
        let sent = lead.sent.lock().unwrap().clone();
        let at = sent.iter().position(|s| s.contains(needle)).expect("an answer was sent");
        lead.uuids.lock().unwrap()[at].clone()
    }

    /// C1: a pipe write is not the lead having it. `taken` comes only from the CLI's echo, and
    /// only then does the answer read as taken (test 1 and W3, at the host).
    #[test]
    fn an_answer_is_taken_only_when_the_cli_echoes_its_uuid() {
        let r = rig();
        r.host.relay(&key("a"), "A", Some("h-1"), "Do it.", Origin::DeskTyped).unwrap();
        assert_eq!(r.host.deliver_answer(&key("a"), "A", Some("h-1"), "d-1", "Green.").unwrap(), AnswerOutcome::InFlight);
        let uuid = answer_uuid(&lead_of(&r, "a"));
        let relay = record_of(&r, "a").answers.into_iter().find(|a| a.delivery_id == "d-1").unwrap();
        assert_eq!((relay.uuid.as_str(), relay.taken, relay.session.as_str()), (uuid.as_str(), false,
                   lead_of(&r, "a").session.as_str()));
        r.host.handle(&key("a"), LeadEvent::Took(uuid.clone()));
        assert!(record_of(&r, "a").answers.iter().any(|a| a.delivery_id == "d-1" && a.taken), "saved as taken");
        assert_eq!(r.host.deliver_answer(&key("a"), "A", Some("h-1"), "d-1", "Green.").unwrap(), AnswerOutcome::Taken);
        // W3 after a crash: taken on disk, so a relaunched host sends nothing either.
        let again = relaunched(&r);
        assert_eq!(again.deliver_answer(&key("a"), "A", Some("h-1"), "d-1", "Green.").unwrap(), AnswerOutcome::Taken);
        assert_eq!(r.launcher.leads.lock().unwrap().len(), 1, "no lead was started for a taken answer");
        assert_eq!(lead_of(&r, "a").sent.lock().unwrap().len(), 2, "the assignment and the answer, once");
    }

    /// W2 and W4 (tests 3 and 5): the intent is on disk before the send; the app dies with no
    /// echo; the relaunched host resends into the resumed session under the SAME uuid, with the
    /// note, so a lead that already has it drops it.
    #[test]
    fn after_a_crash_before_the_echo_the_answer_is_resent_under_its_first_uuid_with_the_note() {
        let r = rig();
        r.host.relay(&key("a"), "A", Some("h-1"), "Do it.", Origin::DeskTyped).unwrap();
        let first = lead_of(&r, "a");
        *first.record_probe.lock().unwrap() = Some(ConversationPaths::under(&r.root.join("operator"), &key("a")).record);
        r.host.deliver_answer(&key("a"), "A", Some("h-1"), "d-1", "Green.").unwrap();
        assert_eq!(*first.on_disk_at_send.lock().unwrap(), [true], "the intent was on disk before the send");
        let uuid = answer_uuid(&first);
        assert!(!first.sent.lock().unwrap()[1].contains(RESEND_NOTE), "a first send carries no note");
        // The app dies here: no echo, and its memory is gone.
        let again = relaunched(&r);
        assert_eq!(again.deliver_answer(&key("a"), "A", Some("h-1"), "d-1", "Green.").unwrap(), AnswerOutcome::InFlight);
        let (_, start, second) = r.launcher.leads.lock().unwrap().last().cloned().unwrap();
        assert_eq!(start, LeadStart::Resume(first.session.clone()), "the same session");
        assert_eq!(second.uuids.lock().unwrap().last().unwrap(), &uuid, "the same uuid");
        let text = second.sent.lock().unwrap().last().unwrap().clone();
        assert!(text.contains(&format!("{RESEND_NOTE}\n\nHis answer to your question on h-1:\n\nGreen.")), "{text}");
        let log = std::fs::read_to_string(again.log_path()).unwrap();
        assert!(log.contains("answer d-1 resent to femcboost/a with the note, under the same uuid"), "{log}");
    }

    /// Test 6: while its uuid is awaited, a second flush sends nothing.
    #[test]
    fn an_answer_in_flight_is_never_sent_twice_by_one_process() {
        let r = rig();
        for _ in 0..3 {
            assert_eq!(r.host.deliver_answer(&key("a"), "A", None, "d-1", "Green.").unwrap(), AnswerOutcome::InFlight);
        }
        assert_eq!(lead_of(&r, "a").sent.lock().unwrap().len(), 1);
    }

    /// W5 and C6 (test 7, at the host): the lead's own crash releases what it held, so the next
    /// delivery resends into the resumed session under the same uuid, and the conversation no
    /// longer reads as working.
    #[test]
    fn the_lead_s_own_crash_releases_its_queue_and_the_answer_is_resent_once() {
        let r = rig();
        r.host.deliver_answer(&key("a"), "A", None, "d-1", "Green.").unwrap();
        let first = lead_of(&r, "a");
        let uuid = answer_uuid(&first);
        assert_eq!(r.host.team_from_stream().working.len(), 1, "queued: working");
        r.host.handle(&key("a"), LeadEvent::Took(uuid.clone()));
        *first.exited.lock().unwrap() = true;
        // The answer was TAKEN into a turn that the crash then ended; it is not resent (the
        // provider has it and re-executes or drops it, §1.2). A second, untaken answer is.
        r.host.handle(&key("a"), LeadEvent::Ended);
        {
            let conversation = r.host.conversations.lock().unwrap().get(&key("a")).cloned().unwrap();
            let c = conversation.lock().unwrap();
            assert!(c.awaiting.is_empty() && !c.in_turn, "C6: an ended lead holds nothing");
        }
        assert_eq!(r.host.deliver_answer(&key("a"), "A", None, "d-1", "Green.").unwrap(), AnswerOutcome::Taken);
        assert_eq!(r.launcher.leads.lock().unwrap().len(), 1, "nothing started for a taken answer");
        // Now the W5 case proper: an answer written and not taken when the lead crashes.
        r.host.relay(&key("a"), "A", None, "wake up", Origin::DeskTyped).unwrap();
        let first = lead_of(&r, "a");
        r.host.deliver_answer(&key("a"), "A", None, "d-2", "Blue.").unwrap();
        let uuid = answer_uuid_of(&first, "Blue.");
        *first.exited.lock().unwrap() = true;
        r.host.handle(&key("a"), LeadEvent::Ended);
        assert_eq!(r.host.deliver_answer(&key("a"), "A", None, "d-2", "Blue.").unwrap(), AnswerOutcome::InFlight);
        let second = lead_of(&r, "a");
        assert!(!Arc::ptr_eq(&first, &second));
        assert_eq!(*second.uuids.lock().unwrap(), [uuid], "resent once, same uuid");
        assert_eq!(r.host.deliver_answer(&key("a"), "A", None, "d-2", "Blue.").unwrap(), AnswerOutcome::InFlight);
        assert_eq!(second.uuids.lock().unwrap().len(), 1, "and not again while awaited");
    }

    /// **The retry driver never loops a lead.** A lead that ends every time it starts (an init
    /// refusal, a crash at start) is started for an answer at most [`RETRY_STARTS`] times with
    /// no turn completing; then the answer waits for his next words there, which start the lead
    /// as they always have, and the answer follows them. A completed turn re-arms it.
    #[test]
    fn a_lead_that_keeps_ending_is_started_for_an_answer_at_most_twice() {
        let r = rig();
        let crash = |r: &Rig| {
            *lead_of(r, "a").exited.lock().unwrap() = true;
            r.host.handle(&key("a"), LeadEvent::Ended);
        };
        r.host.deliver_answer(&key("a"), "A", None, "d-1", "Green.").unwrap();
        crash(&r);
        for _ in 0..RETRY_STARTS {
            assert_eq!(r.host.deliver_answer(&key("a"), "A", None, "d-1", "Green.").unwrap(), AnswerOutcome::InFlight);
            crash(&r);
        }
        let leads = r.launcher.leads.lock().unwrap().len();
        assert_eq!(leads, 1 + RETRY_STARTS as usize);
        for _ in 0..3 {
            assert_eq!(r.host.deliver_answer(&key("a"), "A", None, "d-1", "Green."), Err(RETRIES_SPENT.to_string()));
        }
        assert_eq!(r.launcher.leads.lock().unwrap().len(), leads, "no further lead was started");
        // His words start it; the answer then goes to the running lead.
        r.host.relay(&key("a"), "A", None, "Are you there?", Origin::DeskTyped).unwrap();
        assert_eq!(r.host.deliver_answer(&key("a"), "A", None, "d-1", "Green.").unwrap(), AnswerOutcome::InFlight);
        assert!(lead_of(&r, "a").sent.lock().unwrap()[1].contains("Green."));
        // A turn that completes re-arms the retries.
        r.host.handle(&key("a"), turn(&["u-1"], None));
        crash(&r);
        assert_eq!(r.host.deliver_answer(&key("a"), "A", None, "d-1", "Green.").unwrap(), AnswerOutcome::InFlight);
    }

    /// **Test 11, C5, the shape from P18's frames.** A resent answer the lead already had is
    /// dropped: its echo (Took), then the lifecycle's drop (Dropped), and no turn. The answer is
    /// taken, the conversation does not read as working, and its lead can be retired idle.
    #[test]
    fn a_dropped_duplicate_leaves_the_conversation_idle_and_taken() {
        let r = rig();
        r.host.deliver_answer(&key("a"), "A", Some("h-1"), "d-1", "Green.").unwrap();
        let uuid = answer_uuid(&lead_of(&r, "a"));
        r.host.handle(&key("a"), LeadEvent::Took(uuid.clone()));
        assert_eq!(r.host.team_from_stream().working.len(), 1, "the echo reads as a turn starting");
        r.host.handle(&key("a"), LeadEvent::Dropped(uuid.clone()));
        // A drop means the turn that first took it died with the app (P18 case B), so the
        // lead is continued once (the test above); that continuation's turn then ends.
        let nudge = continuations(&r)[0].0.clone();
        r.host.handle(&key("a"), LeadEvent::Took(nudge.clone()));
        r.host.handle(&key("a"), turn(&[&nudge], None));
        assert!(r.host.team_from_stream().working.is_empty(), "no turn runs: not working");
        assert!(record_of(&r, "a").answers.iter().any(|a| a.delivery_id == "d-1" && a.taken), "the lead has it");
        let paths = ConversationPaths::under(&r.root.join("operator"), &key("a"));
        std::fs::write(&paths.reap_state, r#"{"outside_provider_group": []}"#).unwrap();
        r.engine.not_alive.lock().unwrap().clear();
        assert_eq!(r.host.retire_idle(Duration::ZERO), [key("a")], "retired idle, never held by the drop");
        // A drop inside a turn that was already running leaves that turn running.
        r.host.relay(&key("a"), "A", Some("h-2"), "go", Origin::DeskTyped).unwrap();
        r.host.handle(&key("a"), LeadEvent::Took("u-1".into()));
        r.host.handle(&key("a"), LeadEvent::Took("other".into()));
        r.host.handle(&key("a"), LeadEvent::Dropped("other".into()));
        assert_eq!(r.host.team_from_stream().working.len(), 1, "his running turn is still his team working");
    }

    /// Test 9: a relay written by the old code (no `session`, no `taken`) is read as taken,
    /// which is the at-most-once promise that code made; it is never resent.
    #[test]
    fn a_relay_written_before_the_ledger_fields_existed_reads_as_taken() {
        let old: AnswerRelay = serde_json::from_str(r#"{"delivery_id":"d-1","answer_to":"h-1","uuid":"u-9"}"#).unwrap();
        assert!(old.taken);
        assert_eq!(old.session, "");
        let r = rig();
        let paths = ConversationPaths::under(&r.root.join("operator"), &key("a"));
        std::fs::create_dir_all(&paths.dir).unwrap();
        std::fs::write(&paths.record, r#"{"last_session":"s-old","outbox_read":0,"open_handles":[],
            "answers":[{"delivery_id":"d-1","answer_to":null,"uuid":"u-9"}]}"#).unwrap();
        assert_eq!(r.host.deliver_answer(&key("a"), "A", None, "d-1", "Green.").unwrap(), AnswerOutcome::Taken);
        assert!(r.launcher.leads.lock().unwrap().is_empty());
    }

    // ---- the crash that kills the turn which took his answer (crash matrix cell W3) ---------

    fn continue_all(host: &Arc<OperatorHost>) {
        host.continue_interrupted_answers(&|_| "A".to_string());
    }

    /// The sends that were continuations, as (uuid, text), from every lead.
    fn continuations(r: &Rig) -> Vec<(String, String)> {
        r.launcher.leads.lock().unwrap().iter().flat_map(|(_, _, l)| {
            let sent = l.sent.lock().unwrap().clone();
            let uuids = l.uuids.lock().unwrap().clone();
            uuids.into_iter().zip(sent).filter(|(_, s)| s.contains(CONTINUE_NOTE)).collect::<Vec<_>>()
        }).collect()
    }

    fn relay_of(r: &Rig, thread: &str, delivery: &str) -> AnswerRelay {
        record_of(r, thread).answers.into_iter().find(|a| a.delivery_id == delivery).unwrap()
    }

    /// **W3: the app dies after the lead took his answer and before that turn ended.** The
    /// answer is in the lead's context once, so it is never resent; the relaunched host sends
    /// ONE continuation that names the answer's uuid, into the resumed session, and the lead
    /// carries on with no words from him. Its echo ends the matter: never a second one.
    #[test]
    fn a_turn_killed_after_taking_his_answer_is_continued_once_at_relaunch() {
        let r = rig();
        r.host.relay(&key("a"), "A", Some("h-1"), "Do it.", Origin::DeskTyped).unwrap();
        r.host.deliver_answer(&key("a"), "A", Some("h-1"), "d-1", "Green.").unwrap();
        let first = lead_of(&r, "a");
        let answer = answer_uuid(&first);
        r.host.handle(&key("a"), LeadEvent::Took(answer.clone()));
        let relay = relay_of(&r, "a", "d-1");
        assert!(relay.taken && relay.turn_open, "taken into a turn not yet seen to end, in one save: {relay:?}");
        // While that turn runs in this process, nothing continues it.
        continue_all(&r.host);
        assert!(continuations(&r).is_empty());
        // The app dies here, and its lead with it.
        let again = relaunched(&r);
        continue_all(&again);
        let (_, start, second) = r.launcher.leads.lock().unwrap().last().cloned().unwrap();
        assert_eq!(start, LeadStart::Resume(first.session.clone()), "into the same session");
        let sent = continuations(&r);
        assert_eq!(sent.len(), 1, "{sent:?}");
        let (nudge, text) = &sent[0];
        assert_ne!(nudge, &answer, "a continuation is its own message, never a resend of the answer");
        assert!(text.contains(&answer) && text.contains("h-1"), "it names the answer's uuid and handle: {text}");
        assert!(!text.contains("Green.") && !text.contains(RESEND_NOTE), "his answer is not sent again: {text}");
        assert_eq!(second.sent.lock().unwrap().as_slice(), std::slice::from_ref(text),
                   "the continuation is all this lead was sent");
        let continued = relay_of(&r, "a", "d-1").continued.expect("its intent is on disk");
        assert_eq!((continued.uuid.as_str(), continued.session.as_str()), (nudge.as_str(), first.session.as_str()));
        // In flight: never twice from one process.
        continue_all(&again);
        assert_eq!(continuations(&r).len(), 1);
        // The lead took it: its turn is the work carrying on, and nothing ever continues it again.
        again.handle(&key("a"), LeadEvent::Took(nudge.clone()));
        assert!(!relay_of(&r, "a", "d-1").turn_open);
        let third = relaunched(&r);
        continue_all(&third);
        assert_eq!(continuations(&r).len(), 1, "once, across every later launch");
        assert_eq!(r.launcher.leads.lock().unwrap().len(), 2, "no lead started for nothing to continue");
        let log = std::fs::read_to_string(again.log_path()).unwrap();
        assert!(log.contains(&format!("continued d-1 in femcboost/a under {nudge}")), "{log}");
    }

    /// The continuation keeps the answer's own guarantee: its intent is on disk before the
    /// send, and a relaunch before its echo resends it under ITS first uuid into the same
    /// session, so a lead that already has it drops the repeat (§1.2).
    #[test]
    fn a_continuation_lost_with_the_app_is_resent_under_its_first_uuid() {
        let r = rig();
        r.host.deliver_answer(&key("a"), "A", None, "d-1", "Green.").unwrap();
        let first = lead_of(&r, "a");
        r.host.handle(&key("a"), LeadEvent::Took(answer_uuid(&first)));
        let again = relaunched(&r);
        continue_all(&again);
        let second = lead_of(&r, "a");
        let nudge = continuations(&r)[0].0.clone();
        // Dies again before the echo.
        let third = relaunched(&r);
        continue_all(&third);
        let sent = continuations(&r);
        assert_eq!(sent.len(), 2, "{sent:?}");
        assert_eq!(sent[1].0, nudge, "the same uuid");
        let (_, start, _) = r.launcher.leads.lock().unwrap().last().cloned().unwrap();
        assert_eq!(start, LeadStart::Resume(second.session.clone()));
    }

    /// A turn that ends after it took the answer leaves nothing to continue: a completed turn,
    /// his Esc, a quit he chose while his team worked, a Stop on that assignment (F8: his next
    /// words decide). A relay written before `turn_open` existed is never continued. The lead's
    /// own unexpected end is NOT on this list: it is continued once
    /// (`a_lead_that_ends_unexpectedly_after_taking_his_answer_is_restarted_once`).
    #[test]
    fn a_turn_that_ended_or_that_he_ended_is_never_continued() {
        fn taken(r: &Rig, thread: &str, handle: Option<&str>) -> Arc<FakeLead> {
            r.host.relay(&key(thread), "A", handle, "Do it.", Origin::DeskTyped).unwrap();
            r.host.deliver_answer(&key(thread), "A", handle, "d-1", "Green.").unwrap();
            let lead = lead_of(r, thread);
            r.host.handle(&key(thread), LeadEvent::Took(answer_uuid(&lead)));
            lead
        }
        // Each case is its own conversation, and nothing after it (a quit closes every working
        // conversation, so it gets its own rig and cannot stand in for the others).
        let r = rig();
        let lead = taken(&r, "done", None);
        let answer = answer_uuid(&lead);
        r.host.handle(&key("done"), turn(&["u-1", answer.as_str()], Some("Done.")));
        taken(&r, "esc", None);
        r.host.interrupt(&key("esc"));
        taken(&r, "stop", Some("h-1"));
        r.host.hold(&key("stop"), "h-1", Some("Stopped mark-sonnet-a.")); // his Stop on that assignment
        for thread in ["done", "esc"] {
            assert!(!relay_of(&r, thread, "d-1").turn_open, "{thread}");
        }
        let before = r.launcher.leads.lock().unwrap().len();
        let again = relaunched(&r);
        continue_all(&again);
        assert!(continuations(&r).is_empty(), "{:?}", continuations(&r));
        assert!(!relay_of(&r, "stop", "d-1").turn_open, "a held handle's turn is let go, not continued");
        assert_eq!(r.launcher.leads.lock().unwrap().len(), before, "nothing was started");
        let q = rig();
        taken(&q, "quit", None);
        let before = q.launcher.leads.lock().unwrap().len();
        q.host.quit_all();
        assert!(!relay_of(&q, "quit", "d-1").turn_open);
        continue_all(&relaunched(&q));
        assert!(continuations(&q).is_empty() && q.launcher.leads.lock().unwrap().len() == before);
        let old: AnswerRelay = serde_json::from_str(r#"{"delivery_id":"d-1","answer_to":null,"uuid":"u-9","session":"s",
            "taken":true}"#).unwrap();
        assert!(!old.turn_open && old.continued.is_none(), "a relay from before this field is not continued");
    }

    /// **CONTINUE IT ONCE** (part 1 hunt finding 16; the CEO's ruling of 2026-09-30). The lead
    /// ends unexpectedly after taking his answer and before that turn finished. The turn stays
    /// open, he is told it is being started again, and the desk's next tick restarts the lead
    /// into the same session and sends ONE continuation; never a second.
    #[test]
    fn a_lead_that_ends_unexpectedly_after_taking_his_answer_is_restarted_once() {
        let r = rig();
        r.host.relay(&key("a"), "A", Some("h-1"), "Do it.", Origin::DeskTyped).unwrap();
        r.host.deliver_answer(&key("a"), "A", Some("h-1"), "d-1", "Green.").unwrap();
        let first = lead_of(&r, "a");
        let answer = answer_uuid(&first);
        r.host.handle(&key("a"), LeadEvent::Took(answer.clone()));
        assert!(relay_of(&r, "a", "d-1").turn_open);

        *first.exited.lock().unwrap() = true;
        r.host.handle(&key("a"), LeadEvent::Ended);
        assert!(relay_of(&r, "a", "d-1").turn_open, "the answer's turn was closed by the lead's end");
        assert!(r.said.all().iter().any(|x| x.2 == Say::Team
            && x.3.starts_with("Your team in this conversation stopped unexpectedly.")
            && x.3.contains("starting it again")), "{:?}", r.said.all());
        assert!(continuations(&r).is_empty(), "nothing is sent until the desk's tick");

        continue_all(&r.host);
        let sent = continuations(&r);
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert!(sent[0].1.contains(&answer) && sent[0].1.contains("h-1"), "it names the answer and its handle: {}", sent[0].1);
        let (_, start, second) = r.launcher.leads.lock().unwrap().last().cloned().unwrap();
        assert_eq!(start, LeadStart::Resume(first.session.clone()), "into the same session");
        assert_eq!(second.sent.lock().unwrap().as_slice(), std::slice::from_ref(&sent[0].1));
        // Once: a tick while that continuation is in flight sends nothing more.
        continue_all(&r.host);
        assert_eq!(continuations(&r).len(), 1);
    }

    /// The fall-back half of the same ruling. If the lead started to continue the answer also
    /// ends unexpectedly, he is told so and asked to speak; nothing is started by itself again.
    #[test]
    fn a_second_unexpected_end_asks_for_his_words_and_starts_nothing() {
        let r = rig();
        r.host.relay(&key("a"), "A", Some("h-1"), "Do it.", Origin::DeskTyped).unwrap();
        r.host.deliver_answer(&key("a"), "A", Some("h-1"), "d-1", "Green.").unwrap();
        let first = lead_of(&r, "a");
        r.host.handle(&key("a"), LeadEvent::Took(answer_uuid(&first)));
        *first.exited.lock().unwrap() = true;
        r.host.handle(&key("a"), LeadEvent::Ended);
        continue_all(&r.host);
        assert_eq!(continuations(&r).len(), 1);

        // The restarted lead dies before it finishes the continuation.
        let second = lead_of(&r, "a");
        *second.exited.lock().unwrap() = true;
        r.host.handle(&key("a"), LeadEvent::Ended);
        assert!(!relay_of(&r, "a", "d-1").turn_open, "a second continuation is still owed");
        assert!(r.said.all().iter().any(|x| x.2 == Say::Team
            && x.3.starts_with("Your team in this conversation stopped again")
            && x.3.contains("Speak to me here")), "{:?}", r.said.all());

        let leads = r.launcher.leads.lock().unwrap().len();
        continue_all(&r.host);
        continue_all(&relaunched(&r));
        assert_eq!(continuations(&r).len(), 1, "never a second continuation");
        assert_eq!(r.launcher.leads.lock().unwrap().len(), leads, "a lead was started by itself a third time");
        // His words still resume the session, as before.
        r.host.relay(&key("a"), "A", None, "again", Origin::DeskTyped).unwrap();
        let (_, start, _) = r.launcher.leads.lock().unwrap().last().cloned().unwrap();
        assert_eq!(start, LeadStart::Resume(first.session.clone()));
    }

    /// **A resend the lead drops** (P18 case B: the app died after the send and before its
    /// echo, and the turn that had taken it was killed, which the provider reads as answered).
    /// No turn runs for the resend, so the running lead is continued at once, once.
    #[test]
    fn a_dropped_resend_of_his_answer_is_continued_at_once() {
        let r = rig();
        r.host.deliver_answer(&key("a"), "A", Some("h-1"), "d-1", "Green.").unwrap();
        let uuid = answer_uuid(&lead_of(&r, "a"));
        let again = relaunched(&r);
        again.deliver_answer(&key("a"), "A", Some("h-1"), "d-1", "Green.").unwrap();
        again.handle(&key("a"), LeadEvent::Took(uuid.clone()));
        again.handle(&key("a"), LeadEvent::Dropped(uuid.clone()));
        let sent = continuations(&r);
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert!(sent[0].1.contains(&uuid));
        assert_eq!(again.team_from_stream().working.len(), 1, "the continuation is his team working");
        continue_all(&again);
        again.handle(&key("a"), LeadEvent::Dropped(uuid));
        assert_eq!(continuations(&r).len(), 1, "never twice");
    }

    /// The retry bound holds for the continuation too: its lead start counts, and past the
    /// bound nothing is started for it.
    #[test]
    fn a_continuation_never_starts_a_lead_past_the_retry_bound() {
        let r = rig();
        r.host.deliver_answer(&key("a"), "A", None, "d-1", "Green.").unwrap();
        r.host.handle(&key("a"), LeadEvent::Took(answer_uuid(&lead_of(&r, "a"))));
        let again = relaunched(&r);
        {
            let conversation = again.conversation(&key("a"), "A");
            conversation.lock().unwrap().retry_starts = RETRY_STARTS;
        }
        let before = r.launcher.leads.lock().unwrap().len();
        continue_all(&again);
        assert_eq!(r.launcher.leads.lock().unwrap().len(), before);
        assert!(relay_of(&r, "a", "d-1").turn_open, "still owed, for his next words or the next launch");
    }

    /// Test 10: a lost record means a new session, whose context never saw the answer, so it
    /// is delivered there once under a new uuid.
    #[test]
    fn a_lost_record_means_a_new_session_and_a_new_uuid() {
        let r = rig();
        r.host.deliver_answer(&key("a"), "A", None, "d-1", "Green.").unwrap();
        let first = answer_uuid(&lead_of(&r, "a"));
        std::fs::remove_file(ConversationPaths::under(&r.root.join("operator"), &key("a")).record).unwrap();
        let again = relaunched(&r);
        again.deliver_answer(&key("a"), "A", None, "d-1", "Green.").unwrap();
        let (_, start, second) = r.launcher.leads.lock().unwrap().last().cloned().unwrap();
        assert!(matches!(start, LeadStart::New(_)), "{start:?}");
        assert_ne!(second.uuids.lock().unwrap()[0], first);
        assert!(!second.sent.lock().unwrap()[0].contains(RESEND_NOTE), "nothing to say: that context never had it");
    }

    /// C4: the intent could not be saved, so nothing is sent and the caller hears why.
    #[test]
    fn an_intent_that_could_not_be_saved_sends_nothing() {
        let r = rig();
        r.host.relay(&key("a"), "A", None, "go", Origin::DeskTyped).unwrap();
        let paths = ConversationPaths::under(&r.root.join("operator"), &key("a"));
        std::fs::remove_file(&paths.record).unwrap();
        std::fs::create_dir_all(&paths.record).unwrap(); // a folder where the file goes: every save fails
        let err = r.host.deliver_answer(&key("a"), "A", None, "d-1", "Green.").unwrap_err();
        assert!(err.starts_with("RichOS could not save it for your team"), "{err}");
        assert_eq!(lead_of(&r, "a").sent.lock().unwrap().len(), 1, "only the assignment");
    }

    /// A failed write is no send: the attempt's intent goes, so the next try is a first send
    /// with no note (the transient-error half of C2, at the host).
    #[test]
    fn a_failed_write_leaves_no_intent_and_the_next_try_is_a_first_send() {
        let r = rig();
        r.host.relay(&key("a"), "A", None, "go", Origin::DeskTyped).unwrap();
        let lead = lead_of(&r, "a");
        *lead.fail_send.lock().unwrap() = true;
        assert!(r.host.deliver_answer(&key("a"), "A", None, "d-1", "Green.").is_err());
        assert!(record_of(&r, "a").answers.is_empty(), "no intent kept for a send that never happened");
        *lead.fail_send.lock().unwrap() = false;
        assert_eq!(r.host.deliver_answer(&key("a"), "A", None, "d-1", "Green.").unwrap(), AnswerOutcome::InFlight);
        assert!(!lead.sent.lock().unwrap()[1].contains(RESEND_NOTE));
    }

    // ---- (d) stops ---------------------------------------------------------------------------

    #[test]
    fn a_named_stop_stops_that_task_tells_the_registry_and_leaves_the_other() {
        let r = rig();
        r.host.relay(&key("a"), "A", None, "go", Origin::DeskTyped).unwrap();
        let lead = lead_of(&r, "a");
        agent(&lead, "t-a", "mark-sonnet-a", "task-a");
        agent(&lead, "t-b", "mark-sonnet-b", "task-b");
        r.engine.not_alive.lock().unwrap().insert("task-a".into());
        lead.feed(json!({"type":"system","subtype":"task_notification","task_id":"task-a","status":"stopped"}));
        let results = r.host.stop_named(&["mark-sonnet-a".into()], "stop a", Origin::Phone);
        assert!(matches!(&results[0], StopResult::Stopped { name, registry: Ok(()), .. } if name == "mark-sonnet-a"), "{results:?}");
        assert_eq!(*lead.stops.lock().unwrap(), ["task-a"], "only the named task");
        assert!(r.engine.asked.lock().unwrap().iter().all(|t| t == "task-a"), "liveness asked by agent id, never by name");
        assert_eq!(r.engine.stop_words.lock().unwrap()[0], (vec!["mark-sonnet-a".to_string()], "stop a".to_string()));
        let registry = r.engine.registry.lock().unwrap().clone();
        assert_eq!(registry, [(lead.session.clone(), "mark-sonnet-a".to_string(), "stop a".to_string())]);
        assert_eq!(results[0].sentence(), "Stopped mark-sonnet-a.");
    }

    /// **F6: a stop is executed wherever the agent runs.** He named conversation B's agent while
    /// talking to conversation A; whichever lead or desk passes his words on, `stop_named` finds
    /// the agent in B, stops only it, and A's lead is asked to stop nothing.
    #[test]
    fn a_named_stop_finds_the_agent_in_another_conversation() {
        let r = rig();
        r.host.relay(&key("a"), "A", None, "go", Origin::DeskTyped).unwrap();
        r.host.relay(&key("b"), "B", None, "go", Origin::DeskTyped).unwrap();
        let (a, b) = (lead_of(&r, "a"), lead_of(&r, "b"));
        agent(&a, "t-a", "mark-sonnet-a", "task-a");
        agent(&b, "t-b", "echo-sonnet-b", "task-b");
        r.engine.not_alive.lock().unwrap().insert("task-b".into());
        b.feed(json!({"type":"system","subtype":"task_notification","task_id":"task-b","status":"stopped"}));
        let results = r.host.stop_named(&["echo-sonnet-b".into()], "stop echo-sonnet-b", Origin::NoOrigin);
        assert!(matches!(&results[0], StopResult::Stopped { name, .. } if name == "echo-sonnet-b"), "{results:?}");
        assert_eq!(*b.stops.lock().unwrap(), ["task-b"]);
        assert!(a.stops.lock().unwrap().is_empty(), "the lead that heard his words owns nothing to stop");
    }

    /// **Every named agent is asked to stop before any one of them is waited for** (hunt
    /// 2026-09-29 part 1, finding 33). The first agent's resolver reading is held open until
    /// the second agent's `stop_task` has gone out. A host that waits for the first agent's
    /// death before reaching the second never sends it, and the hold lets go by itself after
    /// 10 s and says so; a host that stops them independently sends it at once. Each agent
    /// is still reported stopped only on its own witnessed death and registry step.
    #[test]
    fn a_slow_first_stop_does_not_hold_back_the_stop_of_the_next_agent() {
        let r = rig();
        r.host.relay(&key("a"), "A", None, "go", Origin::DeskTyped).unwrap();
        let lead = lead_of(&r, "a");
        agent(&lead, "t-a", "mark-sonnet-a", "task-a");
        agent(&lead, "t-b", "mark-sonnet-b", "task-b");
        for task in ["task-a", "task-b"] {
            r.engine.not_alive.lock().unwrap().insert(task.into());
            lead.feed(json!({"type":"system","subtype":"task_notification","task_id":task,"status":"stopped"}));
        }
        let forced = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (watched, flag) = (lead.clone(), forced.clone());
        *r.engine.before_liveness.lock().unwrap() = Some(Arc::new(move |agent_id: &str| {
            if agent_id != "task-a" {
                return;
            }
            let deadline = Instant::now() + Duration::from_secs(10);
            while !watched.stops.lock().unwrap().iter().any(|t| t == "task-b") {
                if Instant::now() >= deadline {
                    flag.store(true, std::sync::atomic::Ordering::SeqCst);
                    return;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }));
        let results = r.host.stop_named(&["mark-sonnet-a".into(), "mark-sonnet-b".into()], "stop a and b", Origin::DeskTyped);
        assert!(!forced.load(std::sync::atomic::Ordering::SeqCst),
                "the second agent's stop was sent only after the first agent's stop had been waited out: {:?}",
                lead.stops.lock().unwrap());
        assert!(matches!(&results[0], StopResult::Stopped { name, registry: Ok(()), .. } if name == "mark-sonnet-a"), "{results:?}");
        assert!(matches!(&results[1], StopResult::Stopped { name, registry: Ok(()), .. } if name == "mark-sonnet-b"), "{results:?}");
        let mut stops = lead.stops.lock().unwrap().clone();
        stops.sort();
        assert_eq!(stops, ["task-a", "task-b"], "each named task once, nothing else");
        assert_eq!(r.engine.stop_words.lock().unwrap().len(), 1, "his words are recorded once, for both names");
        assert_eq!(r.engine.registry.lock().unwrap().len(), 2);
    }

    /// **An agent already witnessed ended is not asked about again on every reading** (hunt
    /// 2026-09-29 part 1, finding 34). While an update waits for him, the gate reads his team
    /// every five seconds, and every reading launched `agent-liveness.sh` for every agent the
    /// leads ever started, finished ones included. The resolver stays the authority: an agent
    /// is taken as ended without asking only after the resolver itself said NOT-ALIVE while the
    /// stream said it had ended, and only for as long as the stream still says so. A running
    /// agent is asked every time, and one the stream sees start again is asked again.
    #[test]
    fn an_agent_the_resolver_saw_end_is_not_asked_about_again_while_the_stream_says_it_ended() {
        let r = rig();
        r.host.relay(&key("a"), "A", None, "go", Origin::DeskTyped).unwrap();
        let lead = lead_of(&r, "a");
        agent(&lead, "t-done", "done-agent", "task-done");
        agent(&lead, "t-live", "live-agent", "task-live");
        lead.feed(json!({"type":"system","subtype":"task_notification","task_id":"task-done","status":"completed"}));
        r.engine.not_alive.lock().unwrap().insert("task-done".into());
        let asked = |task: &str| r.engine.asked.lock().unwrap().iter().filter(|t| *t == task).count();
        for _ in 0..3 {
            assert_eq!(r.host.team().alive, ["live-agent"]);
        }
        assert_eq!(asked("task-done"), 1, "an agent already witnessed ended was asked about again on every reading");
        assert_eq!(asked("task-live"), 3, "a running agent is asked on every reading");

        // The stream sees it start again: the resolver is asked again, and its answer counts.
        lead.feed(json!({"type":"system","subtype":"task_updated","task_id":"task-done","patch":{"status":"running"}}));
        r.engine.not_alive.lock().unwrap().remove("task-done");
        let reading = r.host.team();
        assert_eq!(asked("task-done"), 2);
        let mut alive = reading.alive.clone();
        alive.sort();
        assert_eq!(alive, ["done-agent", "live-agent"]);
    }

    /// **Quit asks every lead to end at once** (hunt 2026-09-29 part 1, finding 35). Each lead's
    /// quit is SIGTERM to its supervisor and up to `QUIT_GRACE` (10 s) for it to reap the lead's
    /// tree; one after another, those graces added up per conversation. Here each of two leads'
    /// quits waits until the other's has begun: a quit path that ends them one at a time cannot
    /// satisfy the first of them, whose wait lets go by itself after 10 s and says so.
    #[test]
    fn quit_asks_every_lead_to_end_at_once_rather_than_one_grace_after_another() {
        let r = rig();
        r.host.relay(&key("a"), "A", None, "go", Origin::DeskTyped).unwrap();
        r.host.relay(&key("b"), "B", None, "go", Origin::DeskTyped).unwrap();
        let (a, b) = (lead_of(&r, "a"), lead_of(&r, "b"));
        let forced = Arc::new(std::sync::atomic::AtomicBool::new(false));
        for (me, other) in [(&a, &b), (&b, &a)] {
            let (other, flag) = (Arc::clone(other), Arc::clone(&forced));
            *me.quit_hook.lock().unwrap() = Some(Arc::new(move || {
                let deadline = Instant::now() + Duration::from_secs(10);
                while *other.quits.lock().unwrap() == 0 {
                    if Instant::now() >= deadline {
                        flag.store(true, std::sync::atomic::Ordering::SeqCst);
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            }));
        }
        let quits = r.host.quit_all();
        assert!(!forced.load(std::sync::atomic::Ordering::SeqCst),
                "one lead's quit waited out its grace before the next lead was asked to end");
        assert_eq!(quits.len(), 2);
        assert_eq!((*a.quits.lock().unwrap(), *b.quits.lock().unwrap()), (1, 1), "each lead is quit once");
        assert!(a.exited() && b.exited());
        for lead in [&a, &b] {
            *lead.quit_hook.lock().unwrap() = None;
        }
    }

    #[test]
    fn no_registry_step_without_the_stream_saying_stopped() {
        let r = rig();
        r.host.relay(&key("a"), "A", None, "go", Origin::DeskTyped).unwrap();
        let lead = lead_of(&r, "a");
        agent(&lead, "t-a", "a", "task-a");
        r.engine.not_alive.lock().unwrap().insert("task-a".into());
        let results = r.host.stop_named(&["a".into()], "stop a", Origin::DeskTyped);
        assert!(matches!(&results[0], StopResult::Stopped { registry: Err(_), .. }), "{results:?}");
        assert!(r.engine.registry.lock().unwrap().is_empty(), "the registry is told only what happened");
    }

    #[test]
    fn a_name_no_lead_holds_is_not_found_and_nothing_is_stopped() {
        let r = rig();
        r.host.relay(&key("a"), "A", None, "go", Origin::DeskTyped).unwrap();
        let results = r.host.stop_named(&["nobody".into()], "stop nobody", Origin::DeskTyped);
        assert_eq!(results, [StopResult::NotFound { name: "nobody".into() }]);
        assert!(lead_of(&r, "a").stops.lock().unwrap().is_empty());
    }

    /// (d) item 5 through the stream alone: the CLI taking a message on a handle makes the
    /// agents its turn starts that handle's, and a turn on no handle (or the next one, on
    /// another) claims nothing for it.
    #[test]
    fn an_agent_started_in_a_turn_the_cli_took_on_a_handle_is_that_assignment_s() {
        let r = rig();
        let uuid = match r.host.relay(&key("a"), "A", Some("h-1"), "go", Origin::DeskTyped).unwrap() {
            Relayed::Sent { uuid } => uuid,
            other => panic!("{other:?}"),
        };
        let lead = lead_of(&r, "a");
        r.host.handle(&key("a"), LeadEvent::Took(uuid.clone()));
        agent(&lead, "t-a", "started-on-h1", "task-a");
        r.host.handle(&key("a"), LeadEvent::Agent(AgentTask { name: "started-on-h1".into(), task_id: "task-a".into(),
                                                             tool_use_id: "t-a".into(), status: TaskStatus::Running }));
        r.host.handle(&key("a"), turn(&[uuid.as_str()], None));
        // The next turn is the platform's own: what it starts is nobody's assignment.
        agent(&lead, "t-o", "after-the-turn", "task-o");
        r.host.handle(&key("a"), LeadEvent::Agent(AgentTask { name: "after-the-turn".into(), task_id: "task-o".into(),
                                                             tool_use_id: "t-o".into(), status: TaskStatus::Running }));
        r.engine.not_alive.lock().unwrap().insert("task-a".into());
        let results = r.host.stop_assignment(&key("a"), "h-1", "stop that job", Origin::DeskTyped);
        assert_eq!(r.engine.stop_words.lock().unwrap()[0].0, ["started-on-h1"]);
        assert_eq!(*lead.stops.lock().unwrap(), ["task-a"], "only that assignment's agent");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn the_assignment_stop_stops_the_names_its_turns_started_and_its_reports_named() {
        let r = rig();
        r.host.relay(&key("a"), "A", Some("h-1"), "go", Origin::DeskTyped).unwrap();
        let lead = lead_of(&r, "a");
        // A turn attributed to h-1 is running when the agent starts.
        {
            let c = r.host.conversations.lock().unwrap().get(&key("a")).cloned().unwrap();
            c.lock().unwrap().turn_handle = Some("h-1".into());
        }
        agent(&lead, "t-a", "started-on-h1", "task-a");
        r.host.handle(&key("a"), LeadEvent::Agent(AgentTask { name: "started-on-h1".into(), task_id: "task-a".into(),
                                                             tool_use_id: "t-a".into(), status: TaskStatus::Running }));
        agent(&lead, "t-o", "other-work", "task-o");
        let mut rec = report(Some("h-1"), "update", "working");
        rec["agents"] = json!(["reported-on-h1"]);
        outbox(&r, "a", &[rec]);
        r.host.handle(&key("a"), turn(&[], None));
        r.engine.not_alive.lock().unwrap().insert("task-a".into());
        let results = r.host.stop_assignment(&key("a"), "h-1", "stop that job", Origin::DeskTyped);
        let names: Vec<String> = r.engine.stop_words.lock().unwrap()[0].0.clone();
        assert_eq!(names, ["reported-on-h1", "started-on-h1"]);
        assert!(!lead.stops.lock().unwrap().contains(&"task-o".to_string()), "other work is never stopped");
        assert_eq!(results.len(), 2);
    }

    /// **F8 of Frank's review, the Stop half.** The per-assignment Stop tells the lead at once,
    /// with what it measured, and holds the handle: a resume lists it as held, never as open.
    #[test]
    fn the_assignment_stop_tells_the_lead_at_once_and_a_resume_never_calls_it_open() {
        let r = rig();
        r.host.relay(&key("a"), "A", Some("h-1"), "go", Origin::DeskTyped).unwrap();
        r.host.relay(&key("a"), "A", Some("h-2"), "and this", Origin::DeskTyped).unwrap();
        let lead = lead_of(&r, "a");
        r.host.handle(&key("a"), LeadEvent::Took("u-1".into()));
        agent(&lead, "t-a", "mark-sonnet-a", "task-a");
        r.host.handle(&key("a"), LeadEvent::Agent(AgentTask { name: "mark-sonnet-a".into(), task_id: "task-a".into(),
                                                             tool_use_id: "t-a".into(), status: TaskStatus::Running }));
        r.engine.not_alive.lock().unwrap().insert("task-a".into());
        lead.feed(json!({"type":"system","subtype":"task_notification","task_id":"task-a","status":"stopped"}));
        let results = r.host.stop_assignment(&key("a"), "h-1", "Stop, pressed", Origin::DeskTyped);
        assert!(matches!(&results[0], StopResult::Stopped { .. }), "{results:?}");
        let told = lead.sent.lock().unwrap().last().cloned().unwrap();
        assert_eq!(told, "Stopped or interrupted by him: h-1. They wait for his words; do not continue them on your own.\
                          \n\nStopped mark-sonnet-a.");
        assert_eq!(open_handles(&r, "a"), BTreeSet::from(["h-2".to_string()]));
        // The next message does not say it twice.
        r.host.relay(&key("a"), "A", None, "status?", Origin::DeskTyped).unwrap();
        assert!(!lead.sent.lock().unwrap().last().unwrap().contains(HELD_LINE));
        // A relaunch: open is h-2 only, and h-1 is named as held.
        let host = OperatorHost::new(r.declaration.clone(), &r.state, &r.root.join("operator"), r.launcher.clone(),
                                     r.engine.clone(), r.settle.clone(), r.said.clone(), Arc::new(SayQuestions(r.said.clone())));
        host.relay(&key("a"), "A", None, "where are we?", Origin::DeskTyped).unwrap();
        let first = lead_of(&r, "a").sent.lock().unwrap()[0].clone();
        assert!(first.starts_with("Open when you last ended: h-2.\n\nStopped or interrupted by him: h-1."), "{first}");
    }

    /// **F8, the Esc half.** His Esc ends the turn and holds the assignment it belonged to.
    /// Nothing is sent at once (a message would start the turn he just ended); the lead's next
    /// message names it, once. His words on that handle release it.
    #[test]
    fn his_esc_holds_the_turn_s_assignment_and_the_next_message_names_it_once() {
        let r = rig();
        r.host.relay(&key("a"), "A", Some("h-1"), "go", Origin::DeskTyped).unwrap();
        r.host.handle(&key("a"), LeadEvent::Took("u-1".into()));
        let lead = lead_of(&r, "a");
        r.host.interrupt(&key("a"));
        assert_eq!(lead.sent.lock().unwrap().len(), 1, "nothing is sent at the Esc");
        assert!(open_handles(&r, "a").is_empty());
        r.host.relay(&key("a"), "A", None, "one more thing", Origin::DeskTyped).unwrap();
        r.host.relay(&key("a"), "A", None, "and another", Origin::DeskTyped).unwrap();
        let sent = lead.sent.lock().unwrap().clone();
        assert!(sent[1].starts_with("Stopped or interrupted by him: h-1."), "{}", sent[1]);
        assert!(!sent[2].contains(HELD_LINE), "said once: {}", sent[2]);
        // His answer on that handle is his next words about it: it is open again, not held.
        r.host.deliver_answer(&key("a"), "A", Some("h-1"), "delivery-1", "Carry on.").unwrap();
        assert!(open_handles(&r, "a").contains("h-1"));
        let record = r.host.conversations.lock().unwrap().get(&key("a")).unwrap().lock().unwrap().record.clone();
        assert!(record.held.is_empty() && record.untold.is_empty(), "{record:?}");
    }

    /// A lead record written before `held` and `untold` existed still reads whole: a parse
    /// failure would reset it, losing the session to resume and the outbox position.
    #[test]
    fn a_lead_record_written_before_held_handles_existed_still_resumes() {
        let record: LeadRecord = serde_json::from_str(
            r#"{"last_session":"s-old","outbox_read":3,"open_handles":["h-1"],"answers":[]}"#).unwrap();
        assert_eq!((record.last_session.as_deref(), record.outbox_read), (Some("s-old"), 3));
        assert!(record.held.is_empty() && record.untold.is_empty());
    }

    #[test]
    fn his_esc_ends_the_turn_and_says_how_many_of_his_messages_still_run() {
        let r = rig_with(&["desk-typed"], 2);
        r.host.relay(&key("a"), "A", None, "go", Origin::DeskTyped).unwrap();
        let said = r.host.interrupt(&key("a"));
        assert!(said.contains("2 of your messages are still queued") && said.contains("agents keep running"), "{said}");
    }

    // ---- (r) alarms; (b) init check; (q) ends ------------------------------------------------

    #[test]
    fn an_alarm_from_three_leads_reaches_him_once() {
        let r = rig();
        for t in ["a", "b", "c"] {
            r.host.relay(&key(t), t, None, "go", Origin::DeskTyped).unwrap();
            let alarm = crate::operator_frames::alarms_in(&json!({"type":"system","subtype":"hook_response","hook_event":"Stop",
                "hook_name":"Stop","stdout":"{\"systemMessage\":\"esc-1 is 24 h old\"}","stderr":""})).remove(0);
            r.host.handle(&key(t), LeadEvent::Alarm(alarm));
        }
        let alarms: Vec<_> = r.said.all().into_iter().filter(|x| x.2 == Say::Alarm).collect();
        assert_eq!(alarms.len(), 1, "{alarms:?}");
        assert_eq!(alarms[0].3, "esc-1 is 24 h old", "verbatim");
    }

    #[test]
    fn a_lead_that_fails_the_init_check_is_quit_and_he_hears_one_sentence() {
        let r = rig();
        r.host.relay(&key("a"), "A", None, "go", Origin::DeskTyped).unwrap();
        // The banner first, as it arrived in every recorded run; then an init with no engine.
        r.host.handle(&key("a"), banner_alarm(&r.declaration.engine_root));
        r.host.handle(&key("a"), LeadEvent::Init(json!({"type":"system","subtype":"init","plugins":[],"tools":[]})));
        let lead = lead_of(&r, "a");
        assert_eq!(*lead.quits.lock().unwrap(), 1);
        let team: Vec<_> = r.said.all().into_iter().filter(|x| x.2 == Say::Team).collect();
        assert_eq!(team.len(), 1);
        assert!(team[0].3.starts_with("Your team is switched off on this Mac because "), "{}", team[0].3);
        // The expected end is not news.
        r.host.handle(&key("a"), LeadEvent::Ended);
        assert_eq!(r.said.all().into_iter().filter(|x| x.2 == Say::Team).count(), 1);
    }

    fn banner_alarm(root: &Path) -> LeadEvent {
        let text = format!("RichOS engine 1.2.0: ENFORCEMENT ACTIVE for /f (77/77 guards, engine at {}, root via project-dir).",
                           root.display());
        LeadEvent::Alarm(crate::operator_frames::alarms_in(&json!({"type":"system","subtype":"hook_response",
            "hook_event":"SessionStart","hook_name":"SessionStart","stdout": json!({"systemMessage": text}).to_string(),
            "stderr":""})).remove(0))
    }

    fn good_init() -> LeadEvent {
        LeadEvent::Init(json!({"type":"system","subtype":"init","plugins":[{"name":"richos-engine"}],
                               "tools":[crate::operator_profile::REPORT_TOOL],"claude_code_version":"2.1.282"}))
    }

    #[test]
    fn the_init_check_waits_for_the_banner_whichever_arrives_first() {
        let r = rig();
        r.host.relay(&key("a"), "A", None, "go", Origin::DeskTyped).unwrap();
        r.host.handle(&key("a"), good_init());
        assert_eq!(*lead_of(&r, "a").quits.lock().unwrap(), 0, "no banner yet is not a refusal yet");
        r.host.handle(&key("a"), banner_alarm(&r.declaration.engine_root));
        assert_eq!(*lead_of(&r, "a").quits.lock().unwrap(), 0, "the banner came: the lead opens");
        assert!(!r.said.all().iter().any(|x| x.2 == Say::Team), "{:?}", r.said.all());
        // With no banner by the turn's end, the check decides then, and refuses.
        r.host.relay(&key("b"), "B", None, "go", Origin::DeskTyped).unwrap();
        r.host.handle(&key("b"), good_init());
        r.host.handle(&key("b"), turn(&["u-1"], Some("hi")));
        assert_eq!(*lead_of(&r, "b").quits.lock().unwrap(), 1);
        assert!(r.said.all().iter().any(|x| x.0 == key("b") && x.2 == Say::Team && x.3.contains("banner")));
        assert!(!r.said.all().iter().any(|x| x.0 == key("b") && x.2 == Say::Update), "a refused lead's words are not delivered");
    }

    #[test]
    fn an_unexpected_end_is_said_once_logged_and_the_next_message_resumes_the_session() {
        let r = rig();
        r.host.relay(&key("a"), "A", Some("h-1"), "go", Origin::DeskTyped).unwrap();
        let first = lead_of(&r, "a");
        *first.exited.lock().unwrap() = true;
        r.host.handle(&key("a"), LeadEvent::Ended);
        assert!(r.said.all().iter().any(|x| x.2 == Say::Team && x.3.starts_with("Your team in this conversation has ended.")));
        let log = std::fs::read_to_string(r.host.log_path()).unwrap();
        assert!(log.contains("LEAD ENDED UNEXPECTEDLY"), "{log}");
        r.host.relay(&key("a"), "A", None, "again", Origin::DeskTyped).unwrap();
        let (_, start, _) = r.launcher.leads.lock().unwrap().last().cloned().unwrap();
        assert_eq!(start, LeadStart::Resume(first.session.clone()), "(l): resumed, never a new session");
    }

    #[test]
    fn after_a_relaunch_the_first_message_resumes_and_lists_the_open_handles() {
        let r = rig();
        r.host.relay(&key("a"), "A", Some("h-1"), "go", Origin::DeskTyped).unwrap();
        let first = lead_of(&r, "a");
        // A second host over the same folders stands in for the relaunched app.
        let host = OperatorHost::new(r.declaration.clone(), &r.state, &r.root.join("operator"), r.launcher.clone(),
                                     r.engine.clone(), r.settle.clone(), r.said.clone(), Arc::new(SayQuestions(r.said.clone())));
        assert_eq!(r.launcher.leads.lock().unwrap().len(), 1, "lazy: nothing starts at relaunch");
        host.relay(&key("a"), "A", None, "where are we?", Origin::DeskTyped).unwrap();
        let (_, start, second) = r.launcher.leads.lock().unwrap().last().cloned().unwrap();
        assert_eq!(start, LeadStart::Resume(first.session.clone()));
        assert!(second.sent.lock().unwrap()[0].starts_with("Open when you last ended: h-1."), "{:?}", second.sent);
    }

    /// **F17 / Frank Q3 (a).** A permission request is refused by the route (`operator_lead.rs`
    /// `NoPermissionDesk`); here, every arrival is logged as a warning that the CLI changed,
    /// since P14 measured none in bypass mode, and he hears it once per conversation. The lead
    /// is kept.
    #[test]
    fn a_permission_request_is_logged_as_a_warning_every_time_and_said_once() {
        let r = rig();
        r.host.relay(&key("a"), "A", None, "go", Origin::DeskTyped).unwrap();
        for tool in ["Write", "Bash"] {
            r.host.handle(&key("a"), LeadEvent::PermissionAsked(tool.into()));
        }
        let log = std::fs::read_to_string(r.host.log_path()).unwrap();
        assert_eq!(log.matches("WARNING: permission request").count(), 2, "{log}");
        assert!(log.contains("for Write") && log.contains("for Bash") && log.contains("the CLI changed"), "{log}");
        let team: Vec<_> = r.said.all().into_iter().filter(|x| x.2 == Say::Team).collect();
        assert_eq!(team.len(), 1, "{team:?}");
        assert!(team[0].3.contains("RichOS refused it") && team[0].3.contains("Your team is still running."), "{}", team[0].3);
        assert_eq!(*lead_of(&r, "a").quits.lock().unwrap(), 0);
    }

    #[test]
    fn a_protocol_note_is_said_once_and_the_lead_is_kept() {
        let r = rig();
        r.host.relay(&key("a"), "A", None, "go", Origin::DeskTyped).unwrap();
        for _ in 0..3 {
            r.host.handle(&key("a"), LeadEvent::Protocol("a line that is not JSON was skipped".into()));
        }
        assert_eq!(r.said.all().iter().filter(|x| x.2 == Say::Team).count(), 1);
        assert_eq!(*lead_of(&r, "a").quits.lock().unwrap(), 0);
    }

    // ---- (m), (o), (q) item 4 ----------------------------------------------------------------

    /// A lead in its turn is his team working, agent or no agent; its turn's end is when it
    /// stops counting.
    #[test]
    fn a_lead_in_its_turn_counts_as_working_until_the_turn_ends() {
        let r = rig();
        let uuid = match r.host.relay(&key("a"), "Pricing", None, "think about it", Origin::DeskTyped).unwrap() {
            Relayed::Sent { uuid } => uuid,
            other => panic!("{other:?}"),
        };
        assert_eq!(r.host.team().working, ["Pricing"], "his message is queued with the lead");
        r.host.handle(&key("a"), LeadEvent::Took(uuid.clone()));
        assert_eq!(r.host.team().working, ["Pricing"], "the lead is in the turn");
        r.host.handle(&key("a"), turn(&[uuid.as_str()], Some("Done thinking.")));
        assert!(r.host.team().working.is_empty(), "{:?}", r.host.team());
    }

    /// The reading the exit decision takes never runs a script, and never reads an agent the
    /// stream has not seen end as gone.
    #[test]
    fn the_stream_reading_asks_no_script_and_counts_what_the_stream_has_not_seen_end() {
        let r = rig();
        r.host.relay(&key("a"), "A", None, "go", Origin::DeskTyped).unwrap();
        let lead = lead_of(&r, "a");
        agent(&lead, "t-a", "still-going", "task-a");
        agent(&lead, "t-b", "finished", "task-b");
        lead.feed(json!({"type":"system","subtype":"task_notification","task_id":"task-b","status":"completed"}));
        let reading = r.host.team_from_stream();
        assert_eq!(reading.alive, ["still-going"]);
        assert!(r.engine.asked.lock().unwrap().is_empty(), "no agent-liveness.sh inside the exit callback");
    }

    #[test]
    fn the_team_reading_counts_alive_agents_and_live_descendants() {
        let r = rig();
        r.host.relay(&key("a"), "A", None, "go", Origin::DeskTyped).unwrap();
        let lead = lead_of(&r, "a");
        agent(&lead, "t-a", "alive-one", "task-a");
        agent(&lead, "t-b", "done-one", "task-b");
        r.engine.not_alive.lock().unwrap().insert("task-b".into());
        let reading = r.host.team();
        assert_eq!(reading.alive, ["alive-one"]);
        assert_eq!(reading.descendants_unknown, 1, "no snapshot is never zero");
        let state = ConversationPaths::under(&r.root.join("operator"), &key("a")).reap_state;
        std::fs::write(&state, json!({"outside_provider_group": [4242, 4343]}).to_string()).unwrap();
        assert_eq!(r.host.team().descendants, 2);
    }

    #[test]
    fn idle_retirement_waits_for_nothing_running_and_the_next_message_resumes() {
        let r = rig();
        r.host.relay(&key("a"), "A", None, "go", Origin::DeskTyped).unwrap();
        let state = ConversationPaths::under(&r.root.join("operator"), &key("a")).reap_state;
        // A message is awaiting its turn: not idle.
        assert!(r.host.retire_idle(Duration::ZERO).is_empty());
        r.host.handle(&key("a"), turn(&["u-1"], Some("ok")));
        // No snapshot yet: undecided, not idle.
        assert!(r.host.retire_idle(Duration::ZERO).is_empty());
        std::fs::write(&state, json!({"outside_provider_group": [4242]}).to_string()).unwrap();
        assert!(r.host.retire_idle(Duration::ZERO).is_empty(), "a background command outside the lead's group runs");
        std::fs::write(&state, json!({"outside_provider_group": []}).to_string()).unwrap();
        r.engine.leases.lock().unwrap().push("land-lease: repository /r; lease live, held by the conversation \"A\" for 2 min; at rest.".into());
        assert!(r.host.retire_idle(Duration::ZERO).is_empty(), "a land lease held by this conversation");
        r.engine.leases.lock().unwrap().clear();
        agent(&lead_of(&r, "a"), "t-a", "still-alive", "task-a");
        assert!(r.host.retire_idle(Duration::ZERO).is_empty(), "an ALIVE agent of the lead");
        r.engine.not_alive.lock().unwrap().insert("task-a".into());
        assert!(r.host.retire_idle(Duration::from_secs(3600)).is_empty(), "not idle long enough, snapshot fresh");
        assert_eq!(r.host.retire_idle(Duration::ZERO), [key("a")]);
        let first = lead_of(&r, "a");
        assert_eq!(*first.quits.lock().unwrap(), 1, "by the quit path");
        r.host.relay(&key("a"), "A", None, "again", Origin::DeskTyped).unwrap();
        let (_, start, _) = r.launcher.leads.lock().unwrap().last().cloned().unwrap();
        assert_eq!(start, LeadStart::Resume(first.session.clone()));
    }

    #[test]
    fn a_stale_snapshot_retires_an_idle_lead_at_once() {
        let r = rig();
        r.host.relay(&key("a"), "A", None, "go", Origin::DeskTyped).unwrap();
        r.host.handle(&key("a"), turn(&["u-1"], Some("ok")));
        let state = ConversationPaths::under(&r.root.join("operator"), &key("a")).reap_state;
        std::fs::write(&state, json!({"outside_provider_group": []}).to_string()).unwrap();
        assert!(r.host.retire_idle(Duration::from_secs(3600)).is_empty());
        std::fs::write(r.declaration.entity_root.join("CLAUDE.md"), "rules, edited\n").unwrap();
        assert_eq!(r.host.retire_idle(Duration::from_secs(3600)), [key("a")]);
    }

    #[test]
    fn the_read_names_agents_texts_and_this_conversation_s_leases_only() {
        let r = rig();
        r.host.relay(&key("a"), "Landing the fix", None, "go", Origin::DeskTyped).unwrap();
        r.host.relay(&key("b"), "Other", None, "go", Origin::DeskTyped).unwrap();
        agent(&lead_of(&r, "a"), "t-a", "mark-sonnet-a", "task-a");
        r.engine.leases.lock().unwrap().push("land-lease: repository /r; lease live, held by the conversation \"Landing the fix\" for 1 min; merge in progress.".into());
        r.engine.leases.lock().unwrap().push("land-lease: repository /q; lease live, held by the conversation \"Other\" for 3 min; at rest.".into());
        r.engine.leases.lock().unwrap().push("land-lease: repository /z; lease live, held by your terminal for 9 min; at rest.".into());
        let read = r.host.read(&key("a"), false);
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].agents, [("mark-sonnet-a".to_string(), "running".to_string())]);
        assert_eq!(read[0].leases, ["land-lease: repository /r; lease live, held by the conversation \"Landing the fix\" for 1 min; merge in progress."]);
        assert!(!lease_held_by("held by the conversation \"Landing the fix, part two\"", "Landing the fix"),
                "a longer title is not this one");
        assert!(!lease_held_by("held by your terminal", ""), "no title matches nothing");
        assert_eq!(r.host.read(&key("a"), true).len(), 2, "every conversation's, on request");
    }

    #[test]
    fn quit_ends_every_lead_by_the_quit_path() {
        let r = rig();
        r.host.relay(&key("a"), "A", None, "go", Origin::DeskTyped).unwrap();
        r.host.relay(&key("b"), "B", None, "go", Origin::DeskTyped).unwrap();
        assert_eq!(r.host.quit_all().len(), 2);
        for t in ["a", "b"] {
            assert_eq!(*lead_of(&r, t).quits.lock().unwrap(), 1);
        }
    }

    #[test]
    fn a_path_segment_never_climbs_out() {
        assert_eq!(safe_segment("thread-1_A"), "thread-1_A");
        let odd = safe_segment("../../etc");
        assert!(!odd.contains('/') && !odd.contains(".."), "{odd}");
        assert_ne!(safe_segment("a/b"), safe_segment("a-b"), "distinct ids stay distinct");
    }
}

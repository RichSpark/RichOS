//! **WHAT A RELAUNCH MAY SAY ABOUT WORK IT DID NOT SEE END** — row 9, and every rule in
//! this file is a refusal.
//!
//! The CEO's nine, row 9: *"If the app crashes or restarts mid-run, nothing invents a
//! completion."* The background-work spec (richos-hq
//! `docs/plans/background-work-spec-2026-09-17.md` revision 5) §6 is the mechanism:
//!
//! - **§6.1** — *"receipts are reconciled against Git and the evidence file, never against
//!   a timestamp. Interrupted execution is never converted into verified completion."*
//! - **§6.2** — *"work that was running is reported as unknown until re-witnessed, and the
//!   CEO is told that plainly."*
//! - **§6.3** — *"Nothing restarts work by itself. Resuming is a decision, and the decision
//!   is his or Rich's — never a retry loop. This is the specific mechanism September 9 kept
//!   adding and is the reason it was removed."*
//! - **§6.3a** — orphan seats and orphan grants are reconciled here, and **his own seat is
//!   identified positively and never touched**, because *"everything that is not his"* is
//!   exactly the reasoning that would delete his cursor after a crash.
//!
//! **THE ONE THING THIS FILE NEVER DOES IS START ANYTHING.** It reads, it compares, it
//! writes an honest receipt, and it raises a sentence for him. `Unknown` is where every
//! path it can take ends up, and the only exit from `Unknown` is him
//! ([`crate::assignment::AssignmentState::awaits_his_word`]).
//!
//! **Why the two witnesses are the ones they are.**
//!
//! 1. **The evidence file**, keyed by the work lease's session (`app_workers.rs:38`), which
//!    this build now writes onto the assignment before it runs
//!    ([`crate::assignment::note_start`]). Its rules are unchanged and are the reason it is
//!    trusted: unattributed or unreadable evidence counts as still running, never as zero
//!    (`app_workers.rs:33-47`). **Workers all ending is still not a settlement** — the
//!    engine says so in its own voice (`richos/engine/mega-lander/app.py:664-669`) — so a
//!    quiet evidence file moves nothing toward finished here either.
//! 2. **Git**, compared against the pins taken when the assignment started. This is the
//!    assertion §7.8 asks to be made *"in Git, not on the screen"*. It is evidence in ONE
//!    direction: an unmoved branch is positive proof that nothing was landed; a moved one
//!    proves only that the repository changed, and never that this assignment is what
//!    changed it.
//!
//! **What is deliberately not here.** Nothing reads a clock. A `registered_at_ms` that is
//! three days old is not evidence of anything, and §6.1 rules out the comparison by name.

use crate::assignment::{self, Assignment, AssignmentState, NoticeKind, RepositoryPin};
use std::path::{Path, PathBuf};

/// Where his repositories stand right now.
///
/// A trait so the reconciliation is a pure comparison this suite can drive without a
/// working copy, and so the one place that shells out to `git` is a single small
/// implementation with its own test ([`GitRepositories`]).
pub trait Repositories: Send + Sync {
    /// What `git rev-parse HEAD` answers for this repository, or `None` when it could not
    /// be read. **`None` is never "unchanged"** — a repository that cannot be read is
    /// reported as unread.
    ///
    /// `Send + Sync` because the work host holds one and pins from its runner threads;
    /// the readings are independent and neither implementation keeps state.
    fn head(&self, path: &str) -> Option<String>;
}

/// The real one: `rev-parse HEAD` **through the delivered Git runtime**, with nothing
/// inherited from the operator's shell.
///
/// **It is the app's own Git and not whatever is on `PATH`, and that is not a detail.**
/// `repositories.rs` established the rule when repositories were first connected — the
/// binary comes from the verified engine delivery (`EngineRuntime::git`) and runs with
/// `GIT_CONFIG_NOSYSTEM`, no global config, no hooks path and no inherited `GIT_DIR` — so a
/// user's `~/.gitconfig` alias or a repository hook cannot change what the app reads. A
/// second, softer way of running Git inside the same app would be exactly the inherited
/// default the CEO's standing rule refuses.
///
/// **No new dependency:** one short-lived child process per named repository, once at boot.
/// `richos-core` stays native-dependency-free.
///
/// Where there is no verified runtime — a development build with no `delivery.json` — the
/// caller passes [`UnreadableRepositories`] instead, and recovery says it could not look
/// rather than implying it did.
pub struct GitRepositories {
    git: PathBuf,
    path: String,
}

impl GitRepositories {
    pub fn new(runtime: &crate::runtime::EngineRuntime) -> Self {
        Self { git: runtime.git.clone(), path: runtime.path() }
    }

    /// The same reader against a Git binary named directly. **Test scaffolding, and it is
    /// here rather than in the test module because the test it exists for is the one that
    /// proves this reader against a REAL repository** — the app's own runtime is a
    /// delivered artifact this suite does not have, and a roundtrip proven against nothing
    /// would prove nothing.
    #[doc(hidden)]
    pub fn with_binary(git: &Path) -> Self {
        Self { git: git.to_path_buf(), path: String::new() }
    }
}

impl Repositories for GitRepositories {
    fn head(&self, path: &str) -> Option<String> {
        let directory = PathBuf::from(path);
        // Absolute, present, a real directory, and not a symlink — the same shape
        // `assignment.rs` demands of its own state paths, for the same reason.
        if !directory.is_absolute() || directory.is_symlink() || !directory.is_dir() {
            return None;
        }
        let output = std::process::Command::new(&self.git)
            // The same hardening `repositories.rs:7-14` runs every Git call under.
            .args(["-c", "core.hooksPath=/dev/null", "-c", "core.fsmonitor=false"])
            .arg("rev-parse")
            .arg("HEAD")
            .current_dir(&directory)
            .env("PATH", &self.path)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_COUNT", "0")
            .env_remove("GIT_CONFIG_PARAMETERS")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            // A read must never open an editor, ask for credentials or wait on a terminal.
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let head = String::from_utf8(output.stdout).ok()?.trim().to_string();
        // A commit id or nothing. Anything else is a reading this build does not
        // understand, and half a measurement is not a measurement.
        if head.len() == 40 && head.bytes().all(|b| b.is_ascii_hexdigit()) {
            Some(head)
        } else {
            None
        }
    }
}

/// Every repository this build could not read answers `None` — used by tests that are
/// asserting on the *"I could not look"* branch, which has to say something different from
/// *"nothing moved"*.
pub struct UnreadableRepositories;
impl Repositories for UnreadableRepositories {
    fn head(&self, _path: &str) -> Option<String> {
        None
    }
}

/// What one assignment was found to be, and what he is told about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reconciled {
    pub entity_id: String,
    pub thread_id: String,
    pub id: String,
    pub title: String,
    /// Where the record ended up. Only ever [`AssignmentState::Unknown`] (it was running)
    /// or [`AssignmentState::Blocked`] (it had already stopped at a step of his before the
    /// process went away, which is a durable fact rather than an inference).
    pub state: AssignmentState,
    /// The sentence raised on the assignment, or `None` when nothing needed saying because
    /// nothing changed.
    pub said: Option<String>,
}

/// The whole sweep's report. Counts are for the boot line; the rows are for him.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reconciliation {
    /// Assignments that were running and are now `unknown` — §6.2's whole population.
    pub unknown: Vec<Reconciled>,
    /// Assignments left exactly as they were, because their state was witnessed and
    /// written before the process ended. `blocked` is the one that matters (§7.8).
    pub untouched: Vec<Reconciled>,
    /// Assignment records that could not be read at all. Reported, never skipped.
    pub unreadable: Option<String>,
    /// **Answer runs that had not started, back to waiting on his answer** (the work-path design
    /// D6, richos-hq `docs/plans/2026-09-27-work-path-answer-delivery-design.md`). The job was
    /// in a state where nothing had been asked of the back end, and an answer of his for it is
    /// saved and was never taken. It is `Blocked`, not `Unknown`: nothing ran that anyone could
    /// be unsure about, so there is nothing to tell him, and the launch hands the answer to it.
    pub answers_waiting: Vec<Reconciled>,
    /// Standing action grants closed because no lease can be holding them (§6.3a).
    pub grants_closed: usize,
    /// Grant files that could not be closed. A reconciliation that silently skips what it
    /// could not do is a clean bill of health signed by nobody.
    pub grants_unreconciled: Vec<String>,
}

impl Reconciliation {
    /// One line for the boot log. Says what it did, and says when it could not read
    /// something rather than reporting a zero it did not establish.
    pub fn log_message(&self) -> String {
        let mut parts = Vec::new();
        parts.push(format!(
            "{} assignment(s) unknown until re-witnessed, {} left as recorded",
            self.unknown.len(),
            self.untouched.len()
        ));
        if !self.answers_waiting.is_empty() {
            parts.push(format!("{} answer run(s) that had not started are waiting on his saved answer again", self.answers_waiting.len()));
        }
        if self.grants_closed > 0 {
            parts.push(format!("{} orphan grant(s) closed", self.grants_closed));
        }
        if !self.grants_unreconciled.is_empty() {
            parts.push(format!("{} grant(s) could NOT be closed", self.grants_unreconciled.len()));
        }
        if let Some(why) = &self.unreadable {
            parts.push(format!("the register could not be read: {why}"));
        }
        format!("recovery: {}", parts.join("; "))
    }
}

/// **The boot sweep.** Called once, before the work host takes any new assignment, and
/// never again — this is a boundary, not a timer (§6.3).
///
/// It walks every partition (`assignment::open` already does, `assignment.rs`'s own
/// `open`), because a thread he does not reopen still holds work, which is Sage's finding
/// 10 in the Two Riches review: *"an assignment registered in a thread he does not reopen
/// is never adopted at all"*.
pub fn reconcile(state: &Path, repositories: &dyn Repositories) -> Reconciliation {
    let mut report = Reconciliation::default();
    // **One damaged record does not stop the others being reconciled** (hunt part 1 finding
    // 08). It used to: the first file that failed to parse, often long-finished work, ended
    // the sweep before any healthy open assignment was looked at. Unreadable is still
    // reported, never counted as zero and never called finished (§6.1's standing rule); it
    // is just no longer the whole answer.
    let open = match assignment::scan_open(state) {
        Ok(scan) => {
            if let Some(first) = scan.unreadable.first() {
                report.unreadable = Some(format!("{} assignment record(s) could not be read: {first}", scan.unreadable.len()));
            }
            scan.rows
        }
        Err(error) => {
            report.unreadable = Some(error.to_string());
            return report;
        }
    };
    for record in open {
        match record.state {
            // **Stopped at a step of his, and what he must answer is still on disk.** That
            // was written while a process was watching, so it is a witnessed fact and not an
            // inference — leave it exactly as it is. §7.8 is the case: he comes back, finds
            // an assignment waiting for him, and answers it. A question of his and a saved
            // answer of his both outlive the process (`questions.rs`, `question_work.rs`).
            AssignmentState::Blocked if waits_on_something_saved(state, &record) => report.untouched.push(Reconciled {
                entity_id: record.entity_id.clone(),
                thread_id: record.thread_id.clone(),
                id: record.id.clone(),
                title: record.title.clone(),
                state: AssignmentState::Blocked,
                said: None,
            }),
            // **Stopped at a step for him to APPROVE, and that request died with the process**
            // (hunt part 1 finding 07, richos-hq `docs/audits/2026-09-29-hunt/part-1-codex.md`).
            // An approval request lives only in the permission desk's memory (`permissions.rs`),
            // so after a crash there is nothing he can approve, and a row that still says it
            // waits on his decision is a door with no handle. It becomes `unknown`, which is
            // what it now is: nothing is running and nothing finished. He is told the request
            // closed with RichOS, and picking it back up (`assignment::pick_up`) runs it again,
            // which asks him again if it reaches that step. Nothing re-runs it by itself (§6.3).
            AssignmentState::Blocked => {
                let said = assignment::says::approval_lost(&record.title);
                if assignment::advance(state, &record.entity_id, &record.thread_id, &record.id,
                                       AssignmentState::Unknown, APPROVAL_LOST_AT_RELAUNCH).is_err() {
                    report.unreadable = Some("an assignment could not be brought up to date".into());
                    continue;
                }
                if let Err(error) = assignment::raise_notice(state, &record.entity_id, &record.thread_id, &record.id,
                                                             NoticeKind::Unknown, &said) {
                    eprintln!("[richos] recovery: the notice about a lost approval request could not be written: {error}");
                }
                report.unknown.push(Reconciled {
                    entity_id: record.entity_id.clone(),
                    thread_id: record.thread_id.clone(),
                    id: record.id.clone(),
                    title: record.title.clone(),
                    state: AssignmentState::Unknown,
                    said: Some(said),
                });
            }
            // **AN ANSWER RUN THAT HAD NOT STARTED** (design D6). Nothing had been asked of the
            // back end: the job was written down, waiting for the screen, waiting for its
            // allowance before its first turn, or opening its connection (`work_host.rs` writes
            // `Running` only at the back end's first item). And an answer of his for it is saved
            // and was never taken. So it goes back to waiting on that answer, which the launch
            // delivers. A job whose run HAD started stays `Unknown` below: §6.3, nothing started
            // re-runs by itself (the CEO's call on the design's C6, option B).
            _ if not_started(&record)
                && crate::question_work::untaken(state, &record.entity_id, &record.thread_id, &record.obligation_id)
                    .unwrap_or(false) =>
            {
                if assignment::advance(state, &record.entity_id, &record.thread_id, &record.id,
                                       AssignmentState::Blocked, ANSWER_SAVED_AT_RELAUNCH).is_err() {
                    report.unreadable = Some("an assignment could not be brought up to date".into());
                    continue;
                }
                report.answers_waiting.push(Reconciled {
                    entity_id: record.entity_id.clone(),
                    thread_id: record.thread_id.clone(),
                    id: record.id.clone(),
                    title: record.title.clone(),
                    state: AssignmentState::Blocked,
                    said: None,
                });
            }
            // Everything else that is open was in flight when the process went away.
            _ => {
                let said = assignment::says::unknown(&record.title, &what_was_checked(state, &record, repositories));
                let detail = detail_for(&record);
                let moved = assignment::advance(
                    state,
                    &record.entity_id,
                    &record.thread_id,
                    &record.id,
                    AssignmentState::Unknown,
                    &detail,
                );
                if moved.is_err() {
                    report.unreadable = Some("an assignment could not be brought up to date".into());
                    continue;
                }
                let _ = assignment::raise_notice(
                    state,
                    &record.entity_id,
                    &record.thread_id,
                    &record.id,
                    NoticeKind::Unknown,
                    &said,
                );
                report.unknown.push(Reconciled {
                    entity_id: record.entity_id.clone(),
                    thread_id: record.thread_id.clone(),
                    id: record.id.clone(),
                    title: record.title.clone(),
                    state: AssignmentState::Unknown,
                    said: Some(said),
                });
            }
        }
    }
    report.grants_closed = close_orphan_grants(state, &mut report.grants_unreconciled);
    report
}

/// The row's detail for a job put back to waiting on his saved answer (design D6).
pub const ANSWER_SAVED_AT_RELAUNCH: &str =
    "Your answer is saved. RichOS closed before the back end took it, so it goes to the back end now.";

/// The row's detail for a job whose approval request closed with RichOS (finding 07).
pub const APPROVAL_LOST_AT_RELAUNCH: &str =
    "It was waiting on a decision from you when RichOS closed, and that request closed with it. \
     Nothing is running, and nothing was finished.";

/// **Is what this blocked job waits for still on disk?** A question of his it asked and he
/// has not answered, or an answer of his saved and not yet taken: both outlive the process.
/// An approval request does not (`permissions.rs` keeps it in memory only). A store that
/// cannot be read answers yes, which leaves the job exactly as it was recorded, the behavior
/// before this reading existed: an unreadable store is no evidence that the wait is gone.
fn waits_on_something_saved(state: &Path, record: &Assignment) -> bool {
    let question = crate::questions::Store::new(state).list(&record.entity_id, &record.thread_id).map(|questions| {
        questions.iter().any(|q| {
            q.asker == record.obligation_id && !q.delivered && q.state != crate::questions::State::Withdrawn
        })
    });
    let answer = crate::question_work::untaken(state, &record.entity_id, &record.thread_id, &record.obligation_id);
    !matches!((question, answer), (Ok(false), Ok(false)))
}

/// **Had nothing been asked of the back end in the run this job was on?** The states before
/// its first turn. `WaitingForQuota` counts only when its detail is not the one a started run
/// writes ([`crate::work_host::QUOTA_WAIT_AFTER_START`]): the quota gate also holds between the
/// turns of a run that has started.
fn not_started(record: &Assignment) -> bool {
    match record.state {
        AssignmentState::Registered | AssignmentState::WaitingForScreen | AssignmentState::Preparing => true,
        AssignmentState::WaitingForQuota => !crate::work_host::QUOTA_WAIT_AFTER_START.contains(&record.detail.as_str()),
        _ => false,
    }
}

/// The receipt's own detail line: what state it was in, and what was established about it.
/// Never a stack trace, never an identifier (`sanitize_title` bounds it on the way in).
fn detail_for(record: &Assignment) -> String {
    let was = match record.state {
        AssignmentState::Registered => "It had been written down and had not started",
        // **The CEO's ruling §56 state, and it says the true thing about it: nothing had
        // begun.** The screen gate is strictly before the lease (`work_host.rs`), so an
        // assignment found here was parked on a locked screen with nothing asked of the back
        // end. That is a better position than `Registered` and it still becomes `Unknown`
        // below, for spec §6.3's reason — see `AssignmentState::WaitingForScreen`'s own doc
        // for why the wait is not re-armed.
        AssignmentState::WaitingForScreen => {
            "It was waiting for the screen to unlock and had not started"
        }
        AssignmentState::Preparing => "It was opening its work connection",
        AssignmentState::WaitingForQuota => "It was waiting for its allowance to refresh",
        _ => "It was running",
    };
    format!("{was} when RichOS closed. Nothing has looked since, so nothing is being called finished.")
}

/// **The two witnesses, turned into one sentence he can hear.**
///
/// Deliberately short and deliberately partial: it says only what was established. Nothing
/// here ever produces a sentence that could be read as a completion, which is why the
/// worker reading contributes *"its workers had all stopped"* only alongside the standing
/// rule that this is not a settlement.
fn what_was_checked(state: &Path, record: &Assignment, repositories: &dyn Repositories) -> String {
    let mut said = Vec::new();

    // 1. GIT. The assertion §7.8 wants made in the repository rather than on the screen.
    if !record.repository_pins.is_empty() {
        let mut unmoved = 0;
        let mut moved = 0;
        let mut unread = 0;
        for pin in &record.repository_pins {
            match (&pin.head, repositories.head(&pin.path)) {
                (Some(before), Some(now)) if before == &now => unmoved += 1,
                (Some(_), Some(_)) => moved += 1,
                _ => unread += 1,
            }
        }
        if moved > 0 {
            said.push("Your repository has moved since it started, so I can't say what came from this.".to_string());
        } else if unmoved > 0 && unread == 0 {
            said.push("Your repository is exactly where it was, so nothing from this was landed.".to_string());
        } else if unread > 0 {
            said.push("I couldn't read your repository to check.".to_string());
        }
    }

    // 2. THE EVIDENCE FILE, on the session that actually ran it. A view that is not
    //    attributed says so; an attributed one with open workers says that; and workers
    //    all ending says nothing about whether the assignment finished, which is why that
    //    branch adds no sentence at all.
    match &record.work_session {
        None => said.push("It never reached a work connection.".to_string()),
        Some(session) => {
            let view = crate::app_workers::status(state, Some(session.as_str()));
            if !view.is_attributed() {
                said.push("I couldn't read its work records.".to_string());
            } else if view.active > 0 || view.liveness_unknown > 0 {
                said.push("Its own records still show helpers open.".to_string());
            }
        }
    }
    said.join(" ")
}

/// **Orphan standing grants (§6.3a).**
///
/// A work lease's scope file carries `actions_allowed: true` for the life of one assignment
/// and is rewritten to `false` when the assignment ends (`native.rs`'s
/// `revoke_work_assignment`). A crash skips that write, so the file is left on disk with the
/// grant open. Nothing can be holding it — every work lease died with the process, and its
/// child group and its tool commands with it (`operator_main` in
/// `richos/engine/scripts/provider-supervisor.py`, the owner-death reap) — so at boot,
/// before any new lease exists, every one of them is closed.
///
/// **Only `*-work.json`.** The conversation's own `*-continuity.json` is not this sweep's
/// to touch: the same positive-identification discipline §6.3a states for seats, applied to
/// the file that names them.
fn close_orphan_grants(state: &Path, unreconciled: &mut Vec<String>) -> usize {
    let scopes = state.join("scopes");
    if scopes.is_symlink() {
        unreconciled.push("the scope directory has been redirected".into());
        return 0;
    }
    let Ok(entries) = std::fs::read_dir(&scopes) else { return 0 };
    let mut closed = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else { continue };
        if !name.ends_with("-work.json") || path.is_symlink() {
            continue;
        }
        // Read it first: a scope that is already closed is not a defect and must not be
        // counted as one, because a count that includes the ordinary case cannot be used to
        // notice the extraordinary one.
        let Ok(bytes) = std::fs::read(&path) else {
            unreconciled.push(name.to_string());
            continue;
        };
        // **BOTH grants on the file, because a crash withdraws both.** `actions_allowed` is
        // the turn's; `background_work_allowed` is the standing permission the assignment's
        // already-dispatched workers hold between turns, and a crash between a turn end and
        // the next turn leaves the first closed and the second open. Every worker died with
        // the process group, so nothing is holding either one. Reading only the first would
        // skip such a file as "already closed" and leave a grant on disk that names an
        // authority nothing can any longer exercise.
        let open = serde_json::from_slice::<serde_json::Value>(&bytes).ok().map(|value| {
            let flag = |name: &str| {
                value.get(name).and_then(serde_json::Value::as_bool).unwrap_or(false)
            };
            flag("actions_allowed") || flag("background_work_allowed")
        });
        if open == Some(false) {
            continue;
        }
        match crate::ecs::revoke(&path) {
            Ok(()) => closed += 1,
            Err(_) => unreconciled.push(name.to_string()),
        }
    }
    closed
}

/// One cursor in the engine's `ecs_active_context`, as the host enumerates them
/// (`richos/engine/ecs/adapters/app.py:213-223`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seat {
    pub person_id: String,
    pub entity_id: String,
    pub thread_id: String,
    /// **The obligation, for a work seat** — the engine's own join key, because a work
    /// seat's `turn_id` is the assignment's obligation (`mega-lander/app.py:704`).
    pub turn_id: String,
    pub audience: String,
    pub revision: u64,
}

/// The host's half of the seat reconciliation. A trait so the rule below is testable
/// without an engine, and so the one implementation that talks to one is small.
pub trait SeatDesk {
    /// The cursor this app is on right now, or `None` when it has never bound one. Needs
    /// no binding of its own (`adapters/app.py:147-150`).
    fn current(&self) -> Option<crate::ecs::Binding>;
    /// Every cursor in the store. Host-only, and fenced against the CEO row it is given
    /// (`adapters/app.py:39-41, 165-166`).
    fn seats(&self, binding: &crate::ecs::Binding) -> Result<Vec<Seat>, String>;
    /// Release one work seat, by name.
    fn release(&self, binding: &crate::ecs::Binding, seat: &str, reason: &str) -> Result<(), String>;
}

/// What the seat sweep did. Retained is as important as released: a seat kept for a live
/// assignment is the case this must not break.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SeatSweep {
    pub released: Vec<String>,
    pub retained: Vec<String>,
    /// Seats that could not be released, with the reason. **Reported, never skipped** —
    /// the engine's own reconciler states the rule and this keeps it: *"a reconciliation
    /// that silently skips what it could not do is a clean bill of health signed by
    /// nobody"* (`mega-lander/app.py:946-958`).
    pub unreconciled: Vec<(String, String)>,
    /// Why the sweep could not run at all, when it could not. Never silence.
    pub unavailable: Option<String>,
}

impl SeatSweep {
    pub fn log_message(&self) -> String {
        if let Some(why) = &self.unavailable {
            return format!("seats: not reconciled ({why})");
        }
        format!(
            "seats: {} released, {} kept for live assignments, {} could NOT be released",
            self.released.len(),
            self.retained.len(),
            self.unreconciled.len()
        )
    }
}

/// **Orphan seats (§6.3a), reconciled by the HOST — and the brief's premise for this did
/// not survive re-derivation, so here is what is actually true.**
///
/// The engine does have a `reconcile_seats` (`richos/engine/mega-lander/app.py:946`), and
/// **the app cannot reach it.** It runs inside the work adapter's `inspect` tool, and only
/// from an UNSEATED scope: `if offset == 0 and scope.get("seat") is None` (`app.py:1069-1072`).
/// Since the CEO's Two Riches page took `richos_work` away from the front desk, the only
/// lease that has that tool is the back end (`native.rs:997`), whose scope is seated the
/// moment it can use a work tool at all (`native.rs`'s `bind_work_assignment` writes
/// `seat: Some(…)`, and the adapter refuses every work tool without the grant that comes
/// with it, `app.py:41-42`). So the engine's reconciler is unreachable from this app as
/// built, and *"the app calls it at boot"* names a call that cannot be made.
///
/// **What the app has instead is the pair of host-only commands the engine exposes for
/// exactly this** — `seats` and `release-seat` (`ecs/adapters/app.py:39, 213, 225`), whose
/// own comment says enumerating and releasing seats *"is the HOST's reconciliation, never a
/// background lease's"*. This is that reconciliation.
///
/// **HIS OWN CURSOR IS NEVER TOUCHED, and it is excluded by construction rather than by
/// elimination.** A row is acted on only if it positively identifies itself as a work seat:
/// `audience == "worker"` **and** a `work-seat:` person id **and** the same company and
/// conversation this cursor is in. *"Everything that is not his"* is precisely the
/// reasoning that would delete his cursor after a crash (§6.3a), and it is not used here.
/// The engine refuses his seat by name as well (`adapters/app.py:226-230`), so this is the
/// second of two locks, not the only one.
///
/// **What it can see is bounded, and the bound is named rather than hidden.** `release-seat`
/// refuses a work seat from another conversation (`adapters/app.py:238-241`), so this
/// reconciles the conversation the app's cursor is currently in. A seat left on another
/// thread is reconciled when that thread next takes a turn and this runs again.
pub fn reconcile_seats(state: &Path, desk: &dyn SeatDesk) -> SeatSweep {
    let mut sweep = SeatSweep::default();
    let Some(binding) = desk.current() else {
        sweep.unavailable = Some("this app has never bound a conversation cursor".into());
        return sweep;
    };
    // His cursor is the only one a host sweep may run from, and the engine enforces it too
    // (`adapters/app.py:165-166`). Asking from a worker row would be refused there; refusing
    // here as well means the refusal is legible in this process's own log.
    if binding.audience != "ceo" {
        sweep.unavailable = Some("the app's cursor is not the conversation's".into());
        return sweep;
    }
    let rows = match desk.seats(&binding) {
        Ok(rows) => rows,
        Err(error) => {
            sweep.unavailable = Some(error);
            return sweep;
        }
    };
    // The register for this conversation, read once. An unreadable register means nothing
    // is released: a seat is released on the strength of its assignment being finished, and
    // "I could not read the assignment" is not that.
    let register = match assignment::read_all(state, &binding.entity_id, &binding.thread_id) {
        Ok(register) => register,
        Err(error) => {
            sweep.unavailable = Some(error.to_string());
            return sweep;
        }
    };
    for row in rows {
        // POSITIVE identification, all three parts. Not "everything that is not his".
        let is_work_seat = row.audience == "worker"
            && row.person_id.starts_with("work-seat:")
            && row.entity_id == binding.entity_id
            && row.thread_id == binding.thread_id;
        if !is_work_seat {
            continue;
        }
        let behind_it = register.iter().find(|record| record.obligation_id == row.turn_id);
        let reason = match behind_it {
            // A live assignment keeps its seat. This is the case the sweep must not break.
            Some(record) if record.state.is_open() => {
                sweep.retained.push(row.person_id.clone());
                continue;
            }
            Some(record) => record.state.as_str(),
            // A seat with no assignment behind it at all is §6.3a's defect by name.
            None => "absent",
        };
        match desk.release(&binding, &row.person_id, reason) {
            Ok(()) => sweep.released.push(row.person_id.clone()),
            Err(error) => sweep.unreconciled.push((row.person_id.clone(), error)),
        }
    }
    sweep
}

impl SeatDesk for crate::ecs::EcsBridge {
    fn current(&self) -> Option<crate::ecs::Binding> {
        let result = self.request("current", serde_json::json!({})).ok()?;
        serde_json::from_value(result.get("binding")?.clone()).ok()
    }

    fn seats(&self, binding: &crate::ecs::Binding) -> Result<Vec<Seat>, String> {
        let result = self
            .request("seats", serde_json::json!({ "binding": binding }))
            .map_err(|error| error.to_string())?;
        let rows = result.get("seats").and_then(serde_json::Value::as_array).cloned().unwrap_or_default();
        Ok(rows
            .into_iter()
            .filter_map(|row| {
                Some(Seat {
                    person_id: row.get("person_id")?.as_str()?.to_string(),
                    entity_id: row.get("entity_id")?.as_str()?.to_string(),
                    thread_id: row.get("thread_id")?.as_str()?.to_string(),
                    turn_id: row.get("turn_id")?.as_str()?.to_string(),
                    audience: row.get("audience")?.as_str()?.to_string(),
                    revision: row.get("revision")?.as_u64()?,
                })
            })
            .collect())
    }

    fn release(&self, binding: &crate::ecs::Binding, seat: &str, reason: &str) -> Result<(), String> {
        // The idempotency key is the seat and the reason it was released under, so a sweep
        // that runs twice over the same orphan is one event rather than two.
        self.request(
            "release-seat",
            serde_json::json!({
                "binding": binding,
                "person_id": seat,
                "reason": reason,
                "request_id": format!("app-recovery:{seat}:{reason}"),
                "source_ref": format!("app-recovery:{seat}"),
            }),
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
    }
}

/// Read the repositories an assignment names, right now, for pinning at its start.
/// Bounded by the register's own cap of 32 paths per assignment.
pub fn pins_for(record: &Assignment, repositories: &dyn Repositories) -> Vec<RepositoryPin> {
    record
        .repositories
        .iter()
        .map(|path| RepositoryPin { path: path.clone(), head: repositories.head(path) })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assignment::Registration;
    use std::collections::HashMap;

    fn root() -> PathBuf {
        let path = std::env::temp_dir().join(format!("recovery-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn digest() -> String {
        use sha2::{Digest, Sha256};
        format!("{:x}", Sha256::digest(b"land these three branches"))
    }

    fn registration(obligation: &str) -> Registration {
        Registration {
            entity_id: "depot".into(),
            thread_id: "thread-one".into(),
            obligation_id: obligation.into(),
            instruction_ledger_ref: "ledger:thread-one:turn-7".into(),
            instruction_sha256: digest(),
            title: format!("landing {obligation}"),
            repositories: vec!["/fictional/project".into()],
            needs_screen: false,
        }
    }

    /// **A relaunch over work that was waiting for the screen** — the CEO's ruling §56 meeting
    /// spec §6.3, and the outcome is `unknown` rather than a re-armed wait.
    ///
    /// **The alternative was considered and rejected, and the reason is a comparison rather
    /// than a preference.** Re-arming would mean an assignment that got as far as parking on a
    /// locked screen is resumed automatically, while a `Registered` assignment that got
    /// strictly LESS far is not — `Registered` already lands in the `_` arm and becomes
    /// `unknown`. Treating the further-along one more permissively is not something §56 asks
    /// for: §56 is about a wait inside a running app, and §6.3 — *"Nothing restarts work by
    /// itself"* — is the standing rule it does not override.
    ///
    /// **Nothing is silently dropped, which is the property that matters.** The receipt names
    /// what it was doing, in words that do not imply it got anywhere, and it is
    /// `awaits_his_word` so it is on the list he can act on.
    #[test]
    fn work_that_was_waiting_for_the_screen_comes_back_as_unknown_and_says_it_had_not_started() {
        let state = root();
        let receipt = assignment::register(&state, &registration("obligation-7")).unwrap();
        assignment::advance(
            &state,
            "depot",
            "thread-one",
            &receipt.id,
            AssignmentState::WaitingForScreen,
            crate::screen::says::detail(),
        )
        .unwrap();

        let report = reconcile(&state, &Fixed(HashMap::new()));
        assert_eq!(report.unknown.len(), 1, "it was dropped or left as though it were running");
        assert!(report.untouched.is_empty());

        let row = assignment::read(&state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(row.state, AssignmentState::Unknown);
        // Never softened into a completion or into a stop somebody made.
        assert_ne!(row.state, AssignmentState::Settled);
        assert_ne!(row.state, AssignmentState::Interrupted);
        // It is on his list, because only he decides whether to pick it up (§6.3).
        assert!(row.state.awaits_his_word());
        // And the receipt says the true thing: it had not started. The screen gate is before
        // the lease, so this is a fact rather than a hopeful reading.
        assert!(
            row.detail.contains("waiting for the screen to unlock and had not started"),
            "the receipt does not say what it was doing: {}",
            row.detail
        );
        std::fs::remove_dir_all(state).unwrap();
    }

    /// **An answer run that had not started goes back to waiting on his saved answer** (the
    /// work-path design D6), and only that one. Four jobs, each with a saved answer nobody took:
    /// opening its connection, and held for its allowance before its first turn, come back
    /// `Blocked` with no notice; running, and held for its allowance AFTER it had started, stay
    /// `Unknown` with the ordinary notice (§6.3, nothing started re-runs by itself). A fifth,
    /// opening its connection with NO saved answer, is the control: `Unknown` as before.
    #[test]
    fn only_an_answer_run_that_had_not_started_waits_on_his_answer_again() {
        let state = root();
        let cases = [
            ("obligation-a", AssignmentState::Preparing, "Opening the work connection.", true, AssignmentState::Blocked),
            ("obligation-b", AssignmentState::WaitingForQuota,
             "Waiting for the allowance to refresh. Work is saved and will continue automatically.", true, AssignmentState::Blocked),
            ("obligation-c", AssignmentState::Running, "The back end has started on it.", true, AssignmentState::Unknown),
            ("obligation-d", AssignmentState::WaitingForQuota, crate::work_host::QUOTA_WAIT_AFTER_START[0], true, AssignmentState::Unknown),
            ("obligation-e", AssignmentState::Preparing, "Opening the work connection.", false, AssignmentState::Unknown),
        ];
        let mut ids = Vec::new();
        for (obligation, at, detail, answered, _) in &cases {
            let receipt = assignment::register(&state, &registration(obligation)).unwrap();
            assignment::advance(&state, "depot", "thread-one", &receipt.id, *at, detail).unwrap();
            if *answered {
                crate::question_work::enqueue(&state, &crate::questions::Delivery {
                    id: format!("question-set:{obligation}"), entity_id: "depot".into(), thread_id: "thread-one".into(),
                    asker: obligation.to_string(), set_id: None, text: "You answered: tomorrow".into(), receipt: None,
                }).unwrap();
            }
            ids.push(receipt.id);
        }
        let report = reconcile(&state, &Fixed(HashMap::new()));
        assert_eq!(report.answers_waiting.len(), 2, "{report:?}");
        assert_eq!(report.unknown.len(), 3, "{report:?}");
        for ((obligation, _, _, _, expected), id) in cases.iter().zip(&ids) {
            let row = assignment::read(&state, "depot", "thread-one", id).unwrap();
            assert_eq!(row.state, *expected, "{obligation}: {}", row.detail);
            if *expected == AssignmentState::Blocked {
                assert!(row.notices.is_empty(), "{obligation}: told it was running: {:?}", row.notices);
                assert_eq!(row.detail, ANSWER_SAVED_AT_RELAUNCH);
            } else {
                assert_eq!(row.notices.last().unwrap().kind, NoticeKind::Unknown, "{obligation}");
            }
        }
        // Nothing was let go by looking.
        assert_eq!(crate::question_work::pending(&state).unwrap().len(), 4);
        std::fs::remove_dir_all(state).unwrap();
    }

    /// Repositories that answer whatever a test says they answer.
    struct Fixed(HashMap<String, String>);
    impl Repositories for Fixed {
        fn head(&self, path: &str) -> Option<String> {
            self.0.get(path).cloned()
        }
    }
    fn at(head: &str) -> Fixed {
        Fixed(HashMap::from([("/fictional/project".to_string(), head.to_string())]))
    }

    /// **Row 9, in one test.** An assignment that was running when the process died comes
    /// back `unknown` — not settled, not interrupted — and the sentence he gets says so.
    #[test]
    fn a_crash_leaves_work_unknown_and_never_finished() {
        let state = root();
        let receipt = assignment::register(&state, &registration("obligation-1")).unwrap();
        assignment::note_start(&state, "depot", "thread-one", &receipt.id, "work-session-a", vec![
            RepositoryPin { path: "/fictional/project".into(), head: Some("a".repeat(40)) },
        ])
        .unwrap();
        assignment::advance(&state, "depot", "thread-one", &receipt.id, AssignmentState::Running, "Running.")
            .unwrap();

        // The repository is exactly where it was: positive proof nothing was landed.
        let report = reconcile(&state, &at(&"a".repeat(40)));

        assert_eq!(report.unknown.len(), 1);
        assert!(report.untouched.is_empty());
        let record = assignment::read(&state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(record.state, AssignmentState::Unknown);
        assert_ne!(record.state, AssignmentState::Settled);
        assert_ne!(record.state, AssignmentState::Interrupted);
        let said = report.unknown[0].said.clone().unwrap();
        assert!(said.contains("nothing from this was landed"), "{said}");
        assert!(said.contains("nothing was finished"), "{said}");
        // He is told plainly (§6.2), durably, and exactly once — the notice is on the
        // record, undelivered, so it survives him not being here.
        assert_eq!(record.notices.last().unwrap().kind, NoticeKind::Unknown);
        assert!(record.notices.last().unwrap().delivered_at_ms.is_none());
        // And nothing restarted: an unknown assignment is not open, so nothing sweeps it
        // back onto a lease (§6.3).
        assert!(!record.state.is_open());
        assert!(record.state.awaits_his_word());
        std::fs::remove_dir_all(state).unwrap();
    }

    /// **The moved-repository branch, and its positive control.** A branch that moved is
    /// never read as "this assignment landed it" — the sentence refuses the attribution in
    /// so many words, and the state is still `unknown`.
    #[test]
    fn a_moved_repository_is_never_read_as_this_assignment_having_landed_it() {
        let state = root();
        let receipt = assignment::register(&state, &registration("obligation-1")).unwrap();
        assignment::note_start(&state, "depot", "thread-one", &receipt.id, "work-session-a", vec![
            RepositoryPin { path: "/fictional/project".into(), head: Some("a".repeat(40)) },
        ])
        .unwrap();
        assignment::advance(&state, "depot", "thread-one", &receipt.id, AssignmentState::Running, "Running.")
            .unwrap();

        let report = reconcile(&state, &at(&"b".repeat(40)));
        let said = report.unknown[0].said.clone().unwrap();
        assert!(said.contains("moved since it started"), "{said}");
        assert!(said.contains("can't say what came from this"), "{said}");
        assert_eq!(
            assignment::read(&state, "depot", "thread-one", &receipt.id).unwrap().state,
            AssignmentState::Unknown
        );

        // POSITIVE CONTROL for the third branch: a repository that cannot be read says
        // "I couldn't read" and never "nothing moved".
        let second = assignment::register(&state, &registration("obligation-2")).unwrap();
        assignment::note_start(&state, "depot", "thread-one", &second.id, "work-session-b", vec![
            RepositoryPin { path: "/fictional/project".into(), head: Some("a".repeat(40)) },
        ])
        .unwrap();
        assignment::advance(&state, "depot", "thread-one", &second.id, AssignmentState::Running, "Running.")
            .unwrap();
        let report = reconcile(&state, &UnreadableRepositories);
        let said = report.unknown[0].said.clone().unwrap();
        assert!(said.contains("couldn't read your repository"), "{said}");
        assert!(!said.contains("exactly where it was"), "{said}");
        std::fs::remove_dir_all(state).unwrap();
    }

    /// A question of his that the job asked and he has not answered: it is on disk
    /// (`questions.rs`), so it outlives the process that asked it.
    fn open_question(state: &Path, obligation: &str) {
        use crate::questions::{AskScope, OptionInput, QuestionInput, Store};
        Store::new(state).ask(&AskScope {
            root: state.to_path_buf(), entity_id: "depot".into(), thread_id: "thread-one".into(),
            turn_id: "turn-7".into(), asker: obligation.into(), session_id: "work-session-a".into(),
            engine: None, entity_root: None, app_run: None,
        }, vec![QuestionInput {
            text: "Who should review it?".into(),
            options: vec![
                OptionInput { label: "Dana reviews".into(), description: "Faster, knows the code.".into() },
                OptionInput { label: "Sam reviews".into(), description: "Slower, fresh eyes.".into() },
            ],
            multiple: false, free_answer: true, recommended: None,
        }]).unwrap();
    }

    /// **§7.8 survives a relaunch, where the thing he must answer survives it.** An
    /// assignment waiting on a question of his is left exactly as it was: that state was
    /// witnessed and written down while a process was watching, the question is on disk, and
    /// turning it into `unknown` would throw away the one thing he came back for.
    #[test]
    fn an_assignment_that_was_waiting_for_him_is_still_waiting_for_him() {
        let state = root();
        let receipt = assignment::register(&state, &registration("obligation-1")).unwrap();
        open_question(&state, "obligation-1");
        assignment::advance(
            &state,
            "depot",
            "thread-one",
            &receipt.id,
            AssignmentState::Blocked,
            "Waiting for your answer. Independent work can continue.",
        )
        .unwrap();

        let report = reconcile(&state, &at(&"a".repeat(40)));

        assert!(report.unknown.is_empty());
        assert_eq!(report.untouched.len(), 1);
        let record = assignment::read(&state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(record.state, AssignmentState::Blocked);
        assert!(record.state.is_open());
        // Nothing was said about it: he was already told, and telling him twice about the
        // same waiting decision is noise pretending to be diligence.
        assert!(record.notices.is_empty());
        std::fs::remove_dir_all(state).unwrap();
    }

    /// **Hunt part 1 finding 07** (richos-hq `docs/audits/2026-09-29-hunt/part-1-codex.md`):
    /// a job stopped at a step for him to approve, and RichOS then crashed. The approval
    /// request lived only in the permission desk's memory (`permissions.rs`), so after the
    /// relaunch there is nothing he can approve, and a row still saying it waits for his
    /// approval is a door with no handle. It comes back `unknown` instead, says the request
    /// closed with RichOS, and picking it back up works: the run that follows asks him again
    /// if it reaches the same step. The control: a job waiting on a saved answer of his,
    /// which is on disk, is still left exactly as it was.
    #[test]
    fn a_job_whose_approval_request_died_with_the_app_comes_back_as_something_he_can_pick_up() {
        let state = root();
        let approval = assignment::register(&state, &registration("obligation-1")).unwrap();
        assignment::advance(&state, "depot", "thread-one", &approval.id, AssignmentState::Blocked,
            "The work has run and stopped at a step that is yours to decide: run a command.").unwrap();
        assignment::raise_notice(&state, "depot", "thread-one", &approval.id, NoticeKind::ReadyToApprove,
            &assignment::says::ready_to_approve("landing obligation-1")).unwrap();
        let answered = assignment::register(&state, &registration("obligation-2")).unwrap();
        assignment::advance(&state, "depot", "thread-one", &answered.id, AssignmentState::Blocked,
            crate::work_host::ANSWER_RETRY_DETAIL).unwrap();
        crate::question_work::enqueue(&state, &crate::questions::Delivery {
            id: "question-set:obligation-2".into(), entity_id: "depot".into(), thread_id: "thread-one".into(),
            asker: "obligation-2".into(), set_id: None, text: "You answered: tomorrow".into(), receipt: None,
        }).unwrap();

        let report = reconcile(&state, &at(&"a".repeat(40)));

        let row = assignment::read(&state, "depot", "thread-one", &approval.id).unwrap();
        assert_eq!(row.state, AssignmentState::Unknown, "it still waits on an approval nobody can give: {}", row.detail);
        assert!(row.state.awaits_his_word());
        assert!(!row.state.is_open(), "nothing re-runs it by itself (§6.3)");
        assert_eq!(report.unknown.len(), 1, "{report:?}");
        let notice = row.notices.last().unwrap();
        assert_eq!(notice.kind, NoticeKind::Unknown);
        assert!(notice.text.contains("waiting on a decision from you when RichOS closed"), "{}", notice.text);
        assert!(notice.text.contains("Say the word"), "{}", notice.text);
        // The way out is the ordinary one for work RichOS closed on.
        assignment::pick_up(&state, "depot", "thread-one", "landing obligation-1").unwrap();
        assert_eq!(assignment::read(&state, "depot", "thread-one", &approval.id).unwrap().state,
                   AssignmentState::Registered);
        // The control: what he answered is saved, so that job is left as it was.
        let row = assignment::read(&state, "depot", "thread-one", &answered.id).unwrap();
        assert_eq!((row.state, row.detail.as_str()), (AssignmentState::Blocked, crate::work_host::ANSWER_RETRY_DETAIL));
        assert!(row.notices.is_empty());
        assert_eq!(report.untouched.len(), 1, "{report:?}");
        std::fs::remove_dir_all(state).unwrap();
    }

    /// Where the register keeps this assignment's record.
    fn record_path(state: &Path, id: &str) -> PathBuf {
        for partition in std::fs::read_dir(state.join("assignments")).unwrap().flatten() {
            let path = partition.path().join(format!("{id}.json"));
            if path.exists() {
                return path;
            }
        }
        panic!("no record for {id}");
    }

    /// **Hunt part 1 finding 08** (richos-hq `docs/audits/2026-09-29-hunt/part-1-codex.md`):
    /// one damaged record of long-finished work, on the same conversation as a job that was
    /// running when RichOS closed. The damaged one is REPORTED (never counted as zero and
    /// never called finished), and the healthy one is still reconciled: it comes back
    /// `unknown` with its notice, he is handed that notice, and he can pick it back up.
    #[test]
    fn one_damaged_record_is_reported_and_every_healthy_open_one_is_still_reconciled() {
        let state = root();
        let old = assignment::register(&state, &registration("obligation-old")).unwrap();
        assignment::advance(&state, "depot", "thread-one", &old.id, AssignmentState::Settled, "Finished.").unwrap();
        let running = assignment::register(&state, &registration("obligation-1")).unwrap();
        assignment::advance(&state, "depot", "thread-one", &running.id, AssignmentState::Running, "Running.").unwrap();
        std::fs::write(record_path(&state, &old.id), b"{\"schema\":1,\"half").unwrap();

        let report = reconcile(&state, &at(&"a".repeat(40)));

        assert!(report.unreadable.is_some(), "the damaged record was not reported: {report:?}");
        assert_eq!(report.unknown.len(), 1, "the healthy open job was abandoned: {report:?}");
        let row = assignment::read(&state, "depot", "thread-one", &running.id).unwrap();
        assert_eq!(row.state, AssignmentState::Unknown);
        assert!(report.log_message().contains("could not be read"), "{}", report.log_message());
        // What recovery raised reaches him, and the way out works, with the damaged file still there.
        let told = assignment::take_pending_notices(&state, "depot", "thread-one").unwrap();
        assert_eq!(told.len(), 1, "{told:?}");
        assert_eq!(told[0].kind, NoticeKind::Unknown);
        assignment::pick_up(&state, "depot", "thread-one", "landing obligation-1").unwrap();
        assert_eq!(assignment::read(&state, "depot", "thread-one", &running.id).unwrap().state,
                   AssignmentState::Registered);
        std::fs::remove_dir_all(state).unwrap();
    }

    /// **An assignment that never reached a lease says that, rather than inventing a
    /// reading of an evidence file that was never written.**
    #[test]
    fn an_assignment_that_never_started_is_told_apart_from_one_that_did() {
        let state = root();
        let receipt = assignment::register(&state, &registration("obligation-1")).unwrap();
        // Left in `registered`: written down, nothing prepared.
        let report = reconcile(&state, &at(&"a".repeat(40)));
        let said = report.unknown[0].said.clone().unwrap();
        assert!(said.contains("never reached a work connection"), "{said}");
        let record = assignment::read(&state, "depot", "thread-one", &receipt.id).unwrap();
        assert_eq!(record.state, AssignmentState::Unknown);
        assert!(record.detail.contains("had not started"), "{}", record.detail);
        std::fs::remove_dir_all(state).unwrap();
    }

    /// **§6.3a's orphan grants.** A work scope left open by a crash is closed at boot; a
    /// scope that was already closed is not counted (a count that includes the ordinary
    /// case cannot be used to notice the extraordinary one); and the conversation's own
    /// continuity scope is never touched, which is the positive-identification rule §6.3a
    /// states for seats applied to the file that names them.
    #[test]
    fn an_orphan_grant_is_closed_and_the_conversations_own_scope_is_not_touched() {
        let state = root();
        let scopes = state.join("scopes");
        std::fs::create_dir_all(&scopes).unwrap();
        let open_work = scopes.join("11111111-work.json");
        let closed_work = scopes.join("22222222-work.json");
        let conversation = scopes.join("33333333-continuity.json");
        let damaged = scopes.join("44444444-work.json");
        // A REAL grant, written the way `native.rs`'s `bind_work_assignment` writes one, so
        // this test exercises the document the app actually leaves behind rather than a
        // convenient stand-in.
        let granted = crate::ecs::ToolScope {
            version: 1,
            actions_allowed: true,
            bridge: crate::ecs::EcsBridge {
                python: PathBuf::from("/fictional/python"),
                component: PathBuf::from("/fictional/engine/ecs"),
                state_root: state.clone(),
            },
            binding: crate::ecs::Binding {
                entity_id: "depot".into(),
                thread_id: "thread-one".into(),
                session_id: "work-session-a".into(),
                turn_id: "obligation-1".into(),
                audience: "worker".into(),
                revision: 1,
            },
            user_instruction: None,
            seat: Some("work-seat:obligation-1".into()),
            // An assignment that was live when the app died: its workers held the standing
            // grant, and the sweep below is what takes it back.
            background_work_allowed: true,
        };
        for path in [&open_work, &conversation] {
            std::fs::write(path, serde_json::to_vec(&granted).unwrap()).unwrap();
        }
        // The shape a work lease starts life in, before any assignment is bound: already
        // closed, and not a defect.
        std::fs::write(&closed_work, "{\"version\":1,\"actions_allowed\":false}\n").unwrap();
        // A grant that cannot be understood is REPORTED rather than silently skipped.
        std::fs::write(&damaged, "{\"version\":1,\"actions_allowed\":true,\"half\":").unwrap();

        let report = reconcile(&state, &at(&"a".repeat(40)));

        assert_eq!(report.grants_closed, 1);
        assert_eq!(report.grants_unreconciled, vec!["44444444-work.json".to_string()]);
        assert!(report.log_message().contains("could NOT be closed"), "{}", report.log_message());
        let after = std::fs::read_to_string(&open_work).unwrap();
        assert!(after.contains("\"actions_allowed\":false"), "{after}");
        // Untouched, byte for byte — the conversation's own scope is never this sweep's.
        assert_eq!(
            std::fs::read_to_string(&conversation).unwrap(),
            String::from_utf8(serde_json::to_vec(&granted).unwrap()).unwrap()
        );
        std::fs::remove_dir_all(state).unwrap();
    }

    /// A desk that answers from a script and records every release it was asked for.
    struct FakeDesk {
        binding: Option<crate::ecs::Binding>,
        rows: Vec<Seat>,
        refuse: Option<String>,
        released: std::cell::RefCell<Vec<(String, String)>>,
    }
    fn ceo_binding() -> crate::ecs::Binding {
        crate::ecs::Binding {
            entity_id: "depot".into(),
            thread_id: "thread-one".into(),
            session_id: "conversation-session".into(),
            turn_id: "turn-7".into(),
            audience: "ceo".into(),
            revision: 4,
        }
    }
    fn work_seat(obligation: &str) -> Seat {
        Seat {
            person_id: format!("work-seat:{obligation}"),
            entity_id: "depot".into(),
            thread_id: "thread-one".into(),
            turn_id: obligation.into(),
            audience: "worker".into(),
            revision: 2,
        }
    }
    impl SeatDesk for FakeDesk {
        fn current(&self) -> Option<crate::ecs::Binding> {
            self.binding.clone()
        }
        fn seats(&self, _binding: &crate::ecs::Binding) -> Result<Vec<Seat>, String> {
            Ok(self.rows.clone())
        }
        fn release(&self, _binding: &crate::ecs::Binding, seat: &str, reason: &str) -> Result<(), String> {
            if let Some(refusal) = &self.refuse {
                return Err(refusal.clone());
            }
            self.released.borrow_mut().push((seat.to_string(), reason.to_string()));
            Ok(())
        }
    }

    /// **§6.3a.** A seat whose assignment has finished, or has no assignment behind it at
    /// all, is released; a seat whose assignment is live is kept; and HIS cursor is never
    /// touched — by positive identification, never by elimination.
    #[test]
    fn an_orphan_seat_is_released_a_live_one_is_kept_and_his_cursor_is_never_touched() {
        let state = root();
        // One live assignment and one that has already settled.
        let live = assignment::register(&state, &registration("obligation-live")).unwrap();
        let done = assignment::register(&state, &registration("obligation-done")).unwrap();
        assignment::advance(&state, "depot", "thread-one", &done.id, AssignmentState::Settled, "Closed.")
            .unwrap();
        assert!(assignment::read(&state, "depot", "thread-one", &live.id).unwrap().state.is_open());

        let desk = FakeDesk {
            binding: Some(ceo_binding()),
            rows: vec![
                work_seat("obligation-live"),
                work_seat("obligation-done"),
                // A work seat with nothing behind it at all.
                work_seat("obligation-that-never-existed"),
                // HIS cursor, in this very thread. It is a `ceo` audience and not a
                // `work-seat:` person, so neither of the two positive tests matches it.
                Seat {
                    person_id: "ceo-thread:thread-one".into(),
                    entity_id: "depot".into(),
                    thread_id: "thread-one".into(),
                    turn_id: "turn-7".into(),
                    audience: "ceo".into(),
                    revision: 4,
                },
                // A work seat in ANOTHER conversation: not this cursor's to release, and
                // the engine would refuse it anyway (`adapters/app.py:238-241`).
                Seat { thread_id: "thread-two".into(), ..work_seat("obligation-elsewhere") },
            ],
            refuse: None,
            released: Default::default(),
        };

        let sweep = reconcile_seats(&state, &desk);

        assert_eq!(sweep.retained, vec!["work-seat:obligation-live".to_string()]);
        let released: Vec<String> = sweep.released.clone();
        assert_eq!(
            released,
            vec![
                "work-seat:obligation-done".to_string(),
                "work-seat:obligation-that-never-existed".to_string()
            ]
        );
        // The reasons are the assignment's own state, and "absent" for a seat with nothing
        // behind it — the same vocabulary the engine's reconciler uses.
        let asked = desk.released.borrow().clone();
        assert_eq!(asked[0].1, "settled");
        assert_eq!(asked[1].1, "absent");
        // HIS cursor was never even asked about.
        assert!(!asked.iter().any(|(seat, _)| seat.starts_with("ceo-")));
        assert!(!asked.iter().any(|(seat, _)| seat == "work-seat:obligation-elsewhere"));
        assert!(sweep.unreconciled.is_empty());
        assert!(sweep.log_message().contains("2 released"), "{}", sweep.log_message());
        assert!(sweep.log_message().contains("1 kept"), "{}", sweep.log_message());
        std::fs::remove_dir_all(state).unwrap();
    }

    /// A seat that could not be released is REPORTED, and a sweep that could not run says
    /// why rather than reporting a clean zero.
    #[test]
    fn a_seat_that_cannot_be_released_is_reported_rather_than_counted_as_clean() {
        let state = root();
        let done = assignment::register(&state, &registration("obligation-done")).unwrap();
        assignment::advance(&state, "depot", "thread-one", &done.id, AssignmentState::Settled, "Closed.")
            .unwrap();
        let desk = FakeDesk {
            binding: Some(ceo_binding()),
            rows: vec![work_seat("obligation-done")],
            refuse: Some("that seat has moved since it was enumerated".into()),
            released: Default::default(),
        };

        let sweep = reconcile_seats(&state, &desk);
        assert!(sweep.released.is_empty());
        assert_eq!(sweep.unreconciled.len(), 1);
        assert!(sweep.log_message().contains("1 could NOT be released"), "{}", sweep.log_message());

        // No cursor at all: unavailable, with the reason, and nothing invented.
        let never_bound = FakeDesk { binding: None, rows: Vec::new(), refuse: None, released: Default::default() };
        let sweep = reconcile_seats(&state, &never_bound);
        assert!(sweep.unavailable.is_some());
        assert!(sweep.log_message().contains("not reconciled"), "{}", sweep.log_message());

        // A cursor that is not his refuses to sweep from — the host reconciliation is the
        // conversation's, never a background lease's (`adapters/app.py:39-41`).
        let seated = FakeDesk {
            binding: Some(crate::ecs::Binding { audience: "worker".into(), ..ceo_binding() }),
            rows: vec![work_seat("obligation-done")],
            refuse: None,
            released: Default::default(),
        };
        assert!(reconcile_seats(&state, &seated).unavailable.is_some());
        std::fs::remove_dir_all(state).unwrap();
    }

    /// **`rev-parse HEAD`, against a real repository.** Both branches assert something
    /// true: where a Git binary exists the head is a commit id, a repository with no commit
    /// answers `None`, and a directory that is not a repository answers `None`; where none
    /// exists, everything answers `None` — the honest "I could not look" this module is
    /// careful to keep distinct from "unchanged".
    #[test]
    fn the_git_reading_is_a_commit_id_or_an_honest_nothing() {
        let state = root();
        let repository = state.join("repository");
        std::fs::create_dir_all(&repository).unwrap();
        // The app's own Git is a delivered artifact this suite does not have, so the
        // roundtrip is proven against the platform's, named absolutely rather than
        // searched for on `PATH`.
        let binary = ["/usr/bin/git", "/opt/homebrew/bin/git", "/usr/local/bin/git"]
            .into_iter()
            .map(PathBuf::from)
            .find(|path| path.is_file());
        let path = repository.to_string_lossy().to_string();
        let Some(binary) = binary else {
            let reader = GitRepositories::with_binary(Path::new("/nonexistent/git"));
            assert_eq!(reader.head(&path), None);
            std::fs::remove_dir_all(state).unwrap();
            return;
        };
        let reader = GitRepositories::with_binary(&binary);
        let run = |args: Vec<&str>| {
            std::process::Command::new(&binary)
                .arg("-C")
                .arg(&repository)
                .args(args)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
        };
        assert!(run(vec!["init", "-q"]).success());
        // A fresh repository has no commit, so HEAD does not resolve: `None`, which is the
        // same honest nothing an unreadable directory gives.
        assert_eq!(reader.head(&path), None);
        run(vec!["config", "user.email", "rich@example.invalid"]);
        run(vec!["config", "user.name", "RichOS Test"]);
        std::fs::write(repository.join("a.txt"), "a").unwrap();
        run(vec!["add", "a.txt"]);
        run(vec!["commit", "-q", "-m", "first"]);

        let head = reader.head(&path).expect("a committed repository has a head");
        assert_eq!(head.len(), 40);
        assert!(head.bytes().all(|b| b.is_ascii_hexdigit()));
        // The same reading twice is the same answer — a pin is only evidence if it is
        // stable.
        assert_eq!(reader.head(&path).as_deref(), Some(head.as_str()));
        // Never a repository.
        assert_eq!(reader.head(&state.join("nowhere").to_string_lossy()), None);
        // And a relative path is refused rather than resolved against a working directory
        // this process does not control.
        assert_eq!(reader.head("some/relative/path"), None);
        std::fs::remove_dir_all(state).unwrap();
    }
}

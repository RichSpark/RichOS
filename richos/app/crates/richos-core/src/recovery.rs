//! Restart cleanup for retired scopes and orphan seats.
//!
//! Stu's durable inbox owns unfinished execution and its previous-attempt evidence.
//! Assignment records are operational metadata. This module never changes their
//! execution state or creates a competing recovery report for Rich.

use crate::assignment::{self, Assignment, RepositoryPin};
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

/// Cleanup observations from this launch, without inferring assignment outcomes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reconciliation {
    pub grants_closed:usize,
    pub grants_unreconciled:Vec<String>,
}

impl Reconciliation {
    pub fn log_message(&self)->String {
        format!("recovery: {} orphan grant(s) closed; {} grant(s) could NOT be closed",
            self.grants_closed,self.grants_unreconciled.len())
    }
}

/// Retire grants from the old process. Canonical input recovery belongs to Stu.
pub fn reconcile(state:&Path)->Reconciliation {
    let mut report=Reconciliation::default();
    report.grants_closed=close_orphan_grants(state,&mut report.grants_unreconciled);
    report
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
        match crate::ecs::revoke_by(&path, crate::ecs::closed_by::CRASH_RECOVERY) {
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
    use crate::assignment::{Registration,AssignmentState};

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


    #[test]
    fn restart_keeps_canonical_handoff_ownership_and_never_announces_a_second_recovery() {
        for phase in ["saved", "accepted", "completed", "stopped"] {
            let state=root();
            let receipt=assignment::register(&state,&registration("canonical-owner")).unwrap();
            assignment::note_start(&state,"depot","thread-one",&receipt.id,"stu-before-restart",vec![]).unwrap();
            assignment::advance(&state,"depot","thread-one",&receipt.id,AssignmentState::Running,"Stu owns this work").unwrap();
            let scope=crate::status_tools::StatusToolScope{version:1,state_root:state.clone(),entity_id:"depot".into(),thread_id:"thread-one".into()};
            let message=crate::session_tools::Message{handoff:true,from:"rich".into(),text:"Original handoff".into(),
                instruction:Some(crate::ecs::UserInstruction{ledger_ref:"ledger:thread-one:turn-7".into(),sha256:digest(),text:None}),
                work:Some(crate::session_tools::WorkReference{assignment_id:receipt.id.clone(),obligation_id:"canonical-owner".into()})};
            let inbox=crate::session_tools::inbox(&scope,"stu").unwrap();
            std::fs::create_dir_all(inbox.parent().unwrap()).unwrap();
            let bytes=format!("{}\n",serde_json::to_string(&message).unwrap());
            std::fs::write(&inbox,&bytes).unwrap();
            let through=bytes.len() as u64;
            if phase!="saved" {
                crate::session_tools::begin_attempt(&scope,"stu",&format!("peer:{through}"),"stu-before-restart").unwrap();
                crate::session_tools::accept_live_handoff(&scope,"stu",through).unwrap();
            }
            if phase=="completed" {crate::session_tools::acknowledge(&scope,"stu",through).unwrap();}
            if phase=="stopped" {
                assignment::record_user_stop(&state,"depot","thread-one",&receipt.id).unwrap();
                assignment::advance(&state,"depot","thread-one",&receipt.id,AssignmentState::Interrupted,"User stopped the request").unwrap();
            }
            let before=serde_json::to_value(assignment::read(&state,"depot","thread-one",&receipt.id).unwrap()).unwrap();
            let pending=crate::session_tools::pending(&scope,"stu").unwrap().map(|(_,offset)|offset);
            let previous=crate::session_tools::previous_attempt(&scope,"stu",&format!("peer:{through}")).unwrap();
            reconcile(&state);
            let after=serde_json::to_value(assignment::read(&state,"depot","thread-one",&receipt.id).unwrap()).unwrap();
            assert_eq!(after,before,"{phase}: assignment recovery invented a competing state or reply");
            assert_eq!(crate::session_tools::pending(&scope,"stu").unwrap().map(|(_,offset)|offset),pending,"{phase}: changed canonical execution ownership");
            assert_eq!(crate::session_tools::previous_attempt(&scope,"stu",&format!("peer:{through}")).unwrap(),previous,"{phase}: erased uncertain execution evidence");
            assert_eq!(std::fs::read_to_string(&inbox).unwrap(),bytes);
            std::fs::remove_dir_all(state).unwrap();
        }
    }

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
            closed: None,
        };
        for path in [&open_work, &conversation] {
            std::fs::write(path, serde_json::to_vec(&granted).unwrap()).unwrap();
        }
        // The shape a work lease starts life in, before any assignment is bound: already
        // closed, and not a defect.
        std::fs::write(&closed_work, "{\"version\":1,\"actions_allowed\":false}\n").unwrap();
        // A grant that cannot be understood is REPORTED rather than silently skipped.
        std::fs::write(&damaged, "{\"version\":1,\"actions_allowed\":true,\"half\":").unwrap();

        let report = reconcile(&state);

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

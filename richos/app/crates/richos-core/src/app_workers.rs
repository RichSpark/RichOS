//! Desktop worker observations from the explicit provider callback journal.
//! Never falls back to terminal team directories or treats an exit as task success.
use crate::worker_status::{Unattributed, WorkerItem, WorkerStatusView};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::Path;

// The Python hook holds this same lock exclusively while appending callbacks.
// Keep a shared lock through parsing so a half-written callback is never a
// successful settlement observation. A stuck writer remains an unknown state.
/// How long a reader waits for a writer's exclusive lock before calling the evidence
/// unavailable.
const EVIDENCE_LOCK_PATIENCE: std::time::Duration = std::time::Duration::from_millis(500);

fn evidence_lock(folder: &Path, patience: std::time::Duration) -> Option<std::fs::File> {
    #[cfg(unix)] {
        use std::os::fd::AsRawFd;
        use std::os::unix::fs::OpenOptionsExt;
        use std::time::{Duration, Instant};
        let file = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW)
            .open(folder.join(".lock")).ok()?;
        let metadata = file.metadata().ok()?;
        if !metadata.is_file() { return None; }
        let deadline = Instant::now() + patience;
        loop {
            // lock-busy: one poll inside EVIDENCE_LOCK_PATIENCE; busy past it is "evidence unavailable", an unknown state, never a failure.
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) } == 0 { return Some(file); }
            if std::io::Error::last_os_error().kind() != std::io::ErrorKind::WouldBlock || Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[cfg(not(unix))] { let _ = (folder, patience); None }
}

/// **`active` VS `liveness_unknown`, AND WHY THIS READER NOW TELLS THEM APART.**
///
/// Until 2026-09-18 every open run came back as `liveness_unknown` — an honest answer while
/// the only two rows this file read were `SubagentStart` and `SubagentStop`, because neither
/// says whether the run is going or gone. `native.rs`'s turn-end check then refused on any
/// open run at all, killed the owning child, and told the CEO *"Worker settlement could not
/// be verified at turn end."*
///
/// **There is a third row in the same journal that answers it, and it was being skipped.**
/// `mega-lander/app.py`'s `prepare` hands the back end a payload stamped
/// `run_in_background: true`, and the provider answers that `Agent` call immediately with
/// `{"isAsync": true, "status": "async_launched", "agentId": "…"}`. That is the platform's
/// own word, joined by `agent_id`, that this run was STARTED and is expected to outlive the
/// call — the positive signal the settlement check was missing. Measured on candidate .10
/// (work-lease session `0320b4d4…`, row 17) and on three `work_lease_roundtrip` runs.
///
/// So an open run whose id carries that word is `active` — running, by the provider's
/// account — and an open run without one is still `liveness_unknown`, which is the honest
/// answer for a run nobody witnessed starting in the background and nobody witnessed ending.
/// The distinction is arithmetic over rows that are already in the file; nothing here probes
/// a process, reads an mtime or infers anything from silence.
///
/// **What it does NOT claim.** `active` here means "the platform said it launched this and
/// no end has been observed". It is not a liveness syscall — `worker_status.rs`'s `active`
/// is, from a different source — and the caller that needs the hosting child to be alive
/// establishes that itself (`native.rs` has just been handed the turn's result by it).
///
/// **A `SubagentStop` the provider then contradicts is not an end.** The back end's own turn
/// end (`Stop`, no `agent_id`) carries the provider's `background_tasks`, and a run it lists
/// there as `running` AFTER that run's `SubagentStop` was made to go on. Nightly 56's hire
/// walk (2026-10-14, `walk-1606409b3615`, esc-20261014T202736Z-38bde06b): Dean ended a
/// consult without handing his answer back, the provider ran him again (his two later
/// `SubagentStop` rows carry `stop_hook_active: true`), and its very next `Stop` listed him
/// `running`. Reading the first `SubagentStop` as his end let the hiring settle while Beth
/// was still being written, and his hand-back was then refused. So such a run is open again,
/// and `active`, until a `SubagentStop` after it; the provider's word, as everywhere here.
pub fn status(state: &Path, session: Option<&str>) -> WorkerStatusView {
    status_within(state, session, EVIDENCE_LOCK_PATIENCE)
}

/// [`status`] with the lock patience named. Only the tests name a different one: a test that
/// proves a reader WAITS for a writer cannot also race the reader's 500 ms against the
/// writer being scheduled, or it fails on a busy Mac while the reader is right.
fn status_within(state: &Path, session: Option<&str>, patience: std::time::Duration) -> WorkerStatusView {
    fn unavailable(reason: Unattributed) -> WorkerStatusView { WorkerStatusView {unattributed:Some(reason),..Default::default()} }
    let Some(session) = session else { return unavailable(Unattributed::NoSession); };
    if session.is_empty() || session.len()>128 || !session.bytes().all(|b|b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return unavailable(Unattributed::UnusableSessionId);
    }
    let folder = state.join("evidence").join(session);
    let Some(_lock) = evidence_lock(&folder, patience) else { return unavailable(Unattributed::AppEvidenceUnavailable); };
    let path = folder.join("callbacks.jsonl");
    let Ok(file) = std::fs::File::open(path) else { return unavailable(Unattributed::AppEvidenceUnavailable); };
    let mut open = BTreeMap::new();
    // **THE PROVIDER'S OWN WORD THAT IT LAUNCHED A RUN IN THE BACKGROUND**, joined by
    // `agent_id`. An open run with this beside it and an open run without it are different
    // facts and this reader used to give them the same answer — see the note on [`status`].
    let mut launched = std::collections::BTreeSet::new();
    // Every run seen starting, so a provider `Stop` listing one as still running can reopen it.
    let mut started = BTreeMap::new();
    for line in BufReader::new(file).lines() {
        let Ok(line) = line else {return unavailable(Unattributed::AppEvidenceUnavailable)};
        let Ok(row) = serde_json::from_str::<serde_json::Value>(&line) else {return unavailable(Unattributed::AppEvidenceUnavailable)};
        let callback = &row["callback"];
        if row["schema"] != 1 || callback["session_id"].as_str() != Some(session) {return unavailable(Unattributed::AppEvidenceUnavailable)}
        match callback["hook_event_name"].as_str() {
            Some("SubagentStart") => {
                let Some(id) = callback["agent_id"].as_str().filter(|s|!s.is_empty()) else {return unavailable(Unattributed::AppEvidenceUnavailable)};
                let label = callback["agent_type"].as_str().unwrap_or(id).to_string();
                started.insert(id.to_string(), label.clone());
                open.insert(id.to_string(),label);
            }
            Some("SubagentStop") => {
                let Some(id) = callback["agent_id"].as_str().filter(|s|!s.is_empty()) else {return unavailable(Unattributed::AppEvidenceUnavailable)};
                open.remove(id);
            }
            // The back end's own turn end, and the provider's list of what it still runs
            // (see the note on [`status`]).
            Some("Stop") if callback["agent_id"].as_str().is_none_or(str::is_empty) => {
                for task in callback["background_tasks"].as_array().into_iter().flatten() {
                    if task["type"] != "subagent" || task["status"] != "running" { continue; }
                    let Some(id) = task["id"].as_str() else { continue };
                    if let Some(label) = started.get(id) {
                        open.entry(id.to_string()).or_insert_with(|| label.clone());
                        launched.insert(id.to_string());
                    }
                }
            }
            Some("PostToolUse") if callback["tool_name"] == "Agent" => {
                let response = &callback["tool_response"];
                if response["status"] == "async_launched" {
                    if let Some(id) = response["agentId"].as_str().filter(|s| !s.is_empty()) {
                        launched.insert(id.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    let (mut active, mut liveness_unknown) = (0usize, 0usize);
    let items: Vec<_> = open
        .into_iter()
        .map(|(id, label)| {
            if launched.contains(&id) {
                active += 1;
                WorkerItem {
                    label: format!("{label}: running in the background"),
                    state: "active".into(),
                    agent_id: Some(id),
                }
            } else {
                liveness_unknown += 1;
                WorkerItem {
                    label: format!("{label}: started; awaiting an observed end"),
                    state: "unknown".into(),
                    agent_id: Some(id),
                }
            }
        })
        .collect();
    WorkerStatusView { active, liveness_unknown, items, ..Default::default() }
}

/// **How many helper runs this session's journal has seen END** — its `SubagentStop` rows
/// that name an agent. `None` when the journal cannot be read, which is never a count.
///
/// [`status`] answers "is a helper open NOW", and that question cannot see a helper the
/// platform runs AGAIN after its `SubagentStop`: a reviewer the back end messages with
/// `SendMessage`, or one the platform restarts to hand its report back (walk 6, 2026-10-05,
/// `walk-10608166d53f`: the reviewer stopped, was messaged, and ran four more times). What
/// every one of those runs does deliver is another `SubagentStop`, so the host reads this
/// count when the back end has said it is waiting on a helper, and waits for it to grow
/// (`work_host`'s `wait_for_a_helper_run_to_end`). A positive signal, as everywhere here.
pub fn run_ends(state: &Path, session: &str) -> Option<usize> {
    if session.is_empty() || session.len()>128 || !session.bytes().all(|b|b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return None;
    }
    let folder = state.join("evidence").join(session);
    let _lock = evidence_lock(&folder, EVIDENCE_LOCK_PATIENCE)?;
    let file = std::fs::File::open(folder.join("callbacks.jsonl")).ok()?;
    let mut ends = 0usize;
    for line in BufReader::new(file).lines() {
        let row: serde_json::Value = serde_json::from_str(&line.ok()?).ok()?;
        let callback = &row["callback"];
        if callback["hook_event_name"] == "SubagentStop"
            && callback["agent_id"].as_str().is_some_and(|id| !id.is_empty())
        {
            ends += 1;
        }
    }
    Some(ends)
}

/// **Whether this session's own journal shows the session itself ENDED**: `Some(true)` when its
/// last row, other than a helper's `SubagentStop`, is the provider's `SessionEnd`. A provider
/// that ends on a lost connection or a restart need not deliver its helpers' `SubagentStop`
/// rows (second review rv-20261029T004310Z-6c4c304f-d238 of `6c4c304f4`), so [`status`] can
/// still show a helper of an ended session open. Any other row after the end (a resumed
/// session's `SessionStart`, a new prompt, a tool call) means the session ran again:
/// `Some(false)`, as for a journal with no end. `None` when the journal cannot be read.
pub fn session_ended(state: &Path, session: &str) -> Option<bool> {
    if session.is_empty() || session.len()>128 || !session.bytes().all(|b|b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return None;
    }
    let folder = state.join("evidence").join(session);
    let _lock = evidence_lock(&folder, EVIDENCE_LOCK_PATIENCE)?;
    let file = std::fs::File::open(folder.join("callbacks.jsonl")).ok()?;
    let mut ended = false;
    for line in BufReader::new(file).lines() {
        let row: serde_json::Value = serde_json::from_str(&line.ok()?).ok()?;
        let callback = &row["callback"];
        if row["schema"] != 1 || callback["session_id"].as_str() != Some(session) {
            return None;
        }
        match callback["hook_event_name"].as_str() {
            Some("SessionEnd") => ended = true,
            Some("SubagentStop") => {}
            _ => ended = false,
        }
    }
    Some(ended)
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn observations_are_scoped_and_a_run_end_never_means_task_completion() {
        let root=std::env::temp_dir().join(format!("app-worker-{}",uuid::Uuid::new_v4()));
        let folder=root.join("evidence/session-one");std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(".lock"), "").unwrap();
        let path=folder.join("callbacks.jsonl");
        let row=|event:&str,id:&str|serde_json::json!({"schema":1,"callback":{"session_id":"session-one","hook_event_name":event,"agent_id":id}}).to_string()+"\n";
        std::fs::write(&path,row("SubagentStart","worker-1")).unwrap();
        assert_eq!(status(&root,Some("session-one")).liveness_unknown,1);
        assert_eq!(status(&root,Some("session-two")).unattributed,Some(Unattributed::AppEvidenceUnavailable));
        std::fs::write(&path,row("SubagentStart","worker-1")+&row("SubagentStop","worker-1")).unwrap();
        let view=status(&root,Some("session-one"));assert_eq!(view.liveness_unknown,0);assert!(view.items.is_empty());
        std::fs::write(&path,"partial").unwrap();assert_eq!(status(&root,Some("session-one")).unattributed,Some(Unattributed::AppEvidenceUnavailable));
        std::fs::remove_dir_all(root).unwrap();
    }

    /// **POINT 24, CHECKED RATHER THAN ASSUMED.** Spec point 24 says `engine-state` is
    /// "subject to 18-20 like anything else". Reading it must therefore leave it exactly as
    /// it is (point 19) and must never answer with a SHORT list when it hits something it
    /// cannot read (points 19 and 20) — a partial list of running workers tells the update
    /// gate it is safe to replace the app while a worker is still going.
    ///
    /// Point 18 is NOT applied here, and the reason is that this app is not the writer: the
    /// engine's Python hook is, and it already stamps every row with `"schema": 1`, which the
    /// reader refuses to proceed past when it is anything else.
    #[test]
    fn reading_the_engine_state_evidence_writes_nothing_and_never_answers_short() {
        let root = std::env::temp_dir().join(format!("app-worker-ro-{}", uuid::Uuid::new_v4()));
        let folder = root.join("evidence/session-one");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(".lock"), "").unwrap();
        let path = folder.join("callbacks.jsonl");
        let row = |event: &str, id: &str| {
            serde_json::json!({"schema":1,"callback":{"session_id":"session-one","hook_event_name":event,"agent_id":id}})
                .to_string()
                + "\n"
        };

        // POSITIVE CONTROL: three workers open, and the reader really does see them.
        let good = row("SubagentStart", "w1") + &row("SubagentStart", "w2") + &row("SubagentStart", "w3");
        std::fs::write(&path, &good).unwrap();
        assert_eq!(status(&root, Some("session-one")).liveness_unknown, 3);
        assert_eq!(std::fs::read(&path).unwrap(), good.as_bytes(), "a read writes nothing");

        // A row this build cannot read, in the MIDDLE. The answer must be "I cannot tell you",
        // never "two workers".
        let mut bytes = row("SubagentStart", "w1").into_bytes();
        bytes.extend_from_slice(&[0xff, 0xfe, b'\n']);
        bytes.extend_from_slice(row("SubagentStart", "w3").as_bytes());
        std::fs::write(&path, &bytes).unwrap();
        let view = status(&root, Some("session-one"));
        assert_eq!(
            view.unattributed,
            Some(Unattributed::AppEvidenceUnavailable),
            "a line it cannot read costs the whole VIEW, not the workers below it"
        );
        assert!(view.items.is_empty(), "and it hands back no partial list at all");
        assert_eq!(std::fs::read(&path).unwrap(), bytes, "and it still writes nothing");

        // A schema this build does not know is the same answer — point 18's field, written by
        // the engine rather than by this build, and respected here.
        let future = serde_json::json!({"schema":2,"callback":{"session_id":"session-one","hook_event_name":"SubagentStart","agent_id":"w1"}}).to_string() + "\n";
        std::fs::write(&path, &future).unwrap();
        assert_eq!(status(&root, Some("session-one")).unattributed, Some(Unattributed::AppEvidenceUnavailable));
        assert_eq!(std::fs::read(&path).unwrap(), future.as_bytes());

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn readers_wait_for_a_complete_callback_and_a_stuck_writer_is_unavailable() {
        use std::os::fd::AsRawFd;
        use std::time::Duration;
        let root = std::env::temp_dir().join(format!("app-worker-lock-{}", uuid::Uuid::new_v4()));
        let folder = root.join("evidence/session-one");
        std::fs::create_dir_all(&folder).unwrap();
        let lock_path = folder.join(".lock");
        std::fs::write(&lock_path, "").unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let writer_folder = folder.clone();
        let writer = std::thread::spawn(move || {
            let lock = std::fs::File::open(writer_folder.join(".lock")).unwrap();
            assert_eq!(unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX) }, 0);
            std::fs::write(writer_folder.join("callbacks.jsonl"), "{\"schema\":").unwrap();
            tx.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(50));
            std::fs::write(writer_folder.join("callbacks.jsonl"),
                "{\"schema\":1,\"callback\":{\"session_id\":\"session-one\",\"hook_event_name\":\"SubagentStart\",\"agent_id\":\"worker\"}}\n").unwrap();
        });
        rx.recv().unwrap();
        // The reader is given a hang guard's patience rather than the product's 500 ms: what
        // this half proves is that it WAITS for the writer and reads the whole callback, and
        // with 500 ms it also raced the writer thread being scheduled again, which a busy Mac
        // loses (audit R9; failed on main under `scripts/qa/stall-run.py`). The product's own
        // 500 ms is the second half's subject, against a writer that never lets go.
        let observed = status_within(&root, Some("session-one"), Duration::from_secs(60));
        writer.join().unwrap();
        assert!(observed.is_attributed());
        assert_eq!(observed.liveness_unknown, 1);
        let held = std::fs::File::open(lock_path).unwrap();
        assert_eq!(unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_EX) }, 0);
        // The lock is held until after the reader returns, so a reader that waited for it
        // could never return: returning at all, and saying the evidence is unavailable, is
        // the proof that its wait is bounded. The reader runs on its own thread so that one
        // which never returns fails at a hang guard with a sentence rather than hanging the
        // suite; the `< 3 s` clock that stood here failed on a busy Mac instead (audit R9).
        let (tx, rx) = std::sync::mpsc::channel();
        let reader_root = root.clone();
        std::thread::spawn(move || {
            tx.send(status(&reader_root, Some("session-one")).unattributed).expect("the test thread waits on rx");
        });
        let unattributed = rx.recv_timeout(Duration::from_secs(60))
            .expect("a reader waited on a writer that never lets go");
        assert_eq!(unattributed, Some(Unattributed::AppEvidenceUnavailable));
        drop(held);
        std::fs::remove_dir_all(root).unwrap();
    }
}

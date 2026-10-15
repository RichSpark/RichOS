//! **THE WORK HOST OVER RECORDED REAL CLAUDE SESSIONS** (T3 idea 2; richos-hq
//! `docs/plans/2026-10-09-automatic-second-review-and-t3-ideas.md` §3 and §4 row 6).
//!
//! The real [`WorkHost`] runs the real [`NativeClient`], whose child is `richos-replay`
//! playing back a capture of a real `claude` session (`richos_core::replay`). Nothing between
//! the host and the provider's wire is hand-written: the frames are the provider's own, with
//! their recorded spacing, so the class of defect the six 2026-09/10 incidents shared — a
//! command or helper that ends after the back end's turn has ended, its finish reaching nobody
//! — is caught here in seconds instead of on a real run on his Mac or in the test VM.
//!
//! The only stand-ins are the ENGINE's (the seat binding, the obligation and the worker
//! evidence), the same three the work host's own unit tests stand in for; none of them is on
//! the provider's wire.
//!
//! The captures are `tests/fixtures/replay/`, scrubbed of the account that recorded them by
//! `scripts/replay/scrub-capture.py`. Headless: no live Claude, no network, no Tauri.

use richos_core::assignment::{self, PendingNotice, Registration};
use richos_core::cognition::{
    BackgroundCommand, Cognition, CognitionError, LeaseFactory, ObligationState, TurnItem, WorkAssignment,
};
use richos_core::entity::{EntityId, ThreadBinding};
use richos_core::ledger::Ledger;
use richos_core::machinery::MachineryRecord;
use richos_core::native::NativeClient;
use richos_core::steering::TurnCancel;
use richos_core::work_host::{WorkHost, WorkNotifier};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

mod support;

/// The company these jobs belong to (`support::registry`).
const COMPANY: &str = "femcboost";

/// A hang guard, never a verdict.
const HANG_GUARD: Duration = Duration::from_secs(90);

/// The work lease: the real client, and the engine's three answers standing in.
struct ReplayLease {
    client: NativeClient,
    session: String,
}

impl Cognition for ReplayLease {
    fn session_id(&self) -> &str {
        &self.session
    }
    fn background_commands(&self) -> Option<Vec<BackgroundCommand>> {
        Some(self.client.background_commands())
    }
    fn reprime(&mut self, priming: &str, on: &mut dyn FnMut(TurnItem)) -> Result<(), CognitionError> {
        self.client.prompt_context_only(priming, on).map(|_| ()).map_err(CognitionError::from)
    }
    fn prompt(&mut self, text: &str, on: &mut dyn FnMut(TurnItem)) -> Result<String, CognitionError> {
        self.client.prompt(text, on).map_err(CognitionError::from)
    }
    fn cancel_handle(&self) -> Option<Arc<dyn TurnCancel>> {
        Some(self.client.cancel_handle())
    }
    fn drain_between_turn(&mut self) -> Vec<MachineryRecord> {
        self.client.drain_between_turn(&self.session)
    }
    // ---- the engine's half, which is not on the provider's wire ----------------------------
    fn bind_work_assignment(&mut self, _assignment: &WorkAssignment) -> Result<(), CognitionError> {
        Ok(())
    }
    fn obligation_state(&self, _obligation: &str) -> Result<ObligationState, CognitionError> {
        Ok(ObligationState::Open)
    }
}

/// Spawns the replay child as the work lease's provider.
struct ReplayFactory {
    child: PathBuf,
    cwd: PathBuf,
    state: PathBuf,
    doctrine: PathBuf,
    skills: PathBuf,
}

impl LeaseFactory for ReplayFactory {
    fn spawn(&self) -> Result<Box<dyn Cognition>, CognitionError> {
        Err(CognitionError::Protocol("this factory opens work leases only".into()))
    }
    fn spawn_work(&self, _binding: &ThreadBinding) -> Result<Box<dyn Cognition>, CognitionError> {
        let client = NativeClient::spawn(&self.child, &self.cwd, &self.doctrine, &self.skills)
            .map_err(|e| CognitionError::Protocol(e.to_string()))?;
        let session = client.session_id().to_string();
        // The engine's worker evidence for this session: readable, and no helper in it.
        let evidence = self.state.join("evidence").join(&session);
        std::fs::create_dir_all(&evidence).map_err(|e| CognitionError::Io(e.to_string()))?;
        std::fs::write(evidence.join(".lock"), "").map_err(|e| CognitionError::Io(e.to_string()))?;
        std::fs::write(evidence.join("callbacks.jsonl"), "").map_err(|e| CognitionError::Io(e.to_string()))?;
        Ok(Box::new(ReplayLease { client, session }))
    }
}

#[derive(Default)]
struct Told(Mutex<Vec<PendingNotice>>);
impl WorkNotifier for Told {
    fn raised(&self, _thread: &str, notice: &PendingNotice) {
        self.0.lock().unwrap().push(notice.clone());
    }
}

/// One work host over one capture, in a folder of its own.
struct Replay {
    root: PathBuf,
    state: PathBuf,
    transcript: PathBuf,
    host: Arc<WorkHost>,
    told: Arc<Told>,
    binding: ThreadBinding,
}

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/replay");

fn replay(capture: &str, speed: f64, requests: &[&str]) -> Replay {
    let root = std::env::temp_dir().join(format!("richos-replay-{}", uuid::Uuid::new_v4()));
    let state = root.join("engine-state");
    std::fs::create_dir_all(&state).unwrap();
    let transcript = root.join("transcript.jsonl");
    // The child: `richos-replay` in front of `claude`'s own flags, which arrive as "$@".
    let child = root.join("claude");
    std::fs::write(
        &child,
        format!(
            "#!/bin/sh\nexec '{}' --capture '{FIXTURES}/{capture}' --speed {speed} --transcript '{}' -- \"$@\"\n",
            env!("CARGO_BIN_EXE_richos-replay"),
            transcript.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&child, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let doctrine = richos_core::doctrine::ensure_rendered(&root, &richos_core::doctrine::DoctrineIdentity::default())
        .expect("the doctrine fixture must render");
    let skills = richos_core::skills::ensure_rendered(&root).expect("the skills fixture must render");
    // A conversation in the company, as the spine opens one; only the spine mints a binding.
    let mut spine = support::spine(Ledger::open(root.join("spine-ledger.jsonl")).unwrap());
    let binding = spine.ensure_active_thread_in(&EntityId::parse(COMPANY).unwrap()).unwrap();
    drop(spine);
    // His words, as the conversation ledger holds them: the host reads the request from here.
    let rows: Vec<String> = requests
        .iter()
        .enumerate()
        .map(|(n, text)| {
            serde_json::json!({"event": "PromptReceived", "turn_id": format!("turn-{n}"),
                               "thread_id": binding.thread_id(), "entity_id": COMPANY, "source": "text", "text": text})
            .to_string()
        })
        .collect();
    std::fs::write(root.join("conversation-ledger.jsonl"), rows.join("\n") + "\n").unwrap();
    let factory = ReplayFactory { child, cwd: root.clone(), state: state.clone(), doctrine, skills };
    let told = Arc::new(Told::default());
    let host = WorkHost::new(&state, Box::new(factory), told.clone(), Arc::new(Default::default()));
    Replay { root, state, transcript, host, told, binding }
}

impl Replay {
    /// His `n`th request, written down as the front desk would.
    fn register(&self, n: usize, text: &str) -> String {
        use sha2::Digest;
        let thread = self.binding.thread_id();
        let request = Registration {
            entity_id: COMPANY.into(),
            thread_id: thread.into(),
            obligation_id: format!("obligation-{n}"),
            instruction_ledger_ref: format!("ledger:{thread}:turn-{n}"),
            instruction_sha256: format!("{:x}", sha2::Sha256::digest(text.as_bytes())),
            title: text.into(),
            repositories: Vec::new(),
            needs_screen: false,
        };
        self.host.register(&self.binding, &request).unwrap().id
    }

    fn row(&self, id: &str) -> assignment::Assignment {
        assignment::read(&self.state, COMPANY, self.binding.thread_id(), id).unwrap()
    }

    /// What he was told about one job, in order.
    fn told_about(&self, id: &str) -> Vec<String> {
        self.told.0.lock().unwrap().iter().filter(|n| n.assignment_id == id).map(|n| n.text.clone()).collect()
    }

    /// The replay child's own account of the wire, as far as it has written it.
    fn transcript(&self) -> Vec<Value> {
        std::fs::read_to_string(&self.transcript)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect()
    }

    fn events(&self, event: &str) -> Vec<Value> {
        self.transcript().into_iter().filter(|e| e["event"] == event).collect()
    }

    /// The user messages the host sent the provider, in order.
    fn sent(&self) -> Vec<String> {
        self.events("received")
            .into_iter()
            .filter(|e| e["kind"] == "user")
            .map(|e| e["text"].as_str().unwrap_or_default().to_string())
            .collect()
    }

    /// Wait, on the transcript and never on a guess, until `done` holds or the guard runs out.
    fn until(&self, what: &str, done: impl Fn(&Replay) -> bool) {
        let began = Instant::now();
        while !done(self) {
            assert!(began.elapsed() < HANG_GUARD, "never happened: {what}\ntranscript: {:#?}", self.transcript());
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The end of a test that passed. One that failed ends in [`Drop`], the same way.
    fn end(self) {}
}

/// The host is shut down (its leases, and with them their replay children, are dropped) and the
/// folder removed however the test ends: a failing assertion unwinds through here too (§54).
impl Drop for Replay {
    fn drop(&mut self) {
        self.host.shutdown();
        if let Err(e) = std::fs::remove_dir_all(&self.root) {
            if e.kind() != std::io::ErrorKind::NotFound {
                eprintln!("[replay] could not remove {}: {e}", self.root.display());
            }
        }
    }
}

/// The job is answered AND he has been told `notices` things about it. Both, because the host
/// writes the row before it raises the notice (bgdone2 run 5: the record first, then his words),
/// so a test that read the notices the instant the row changed would read one too few.
fn answered(r: &Replay, id: &str, notices: usize) -> bool {
    r.row(id).was_answered() && r.told_about(id).len() >= notices
}

/// How many frames a capture holds: the replay has played it all once that many were emitted.
fn frames_in(capture: &str) -> usize {
    std::fs::read_to_string(Path::new(FIXTURES).join(capture))
        .unwrap()
        .lines()
        .filter(|l| l.contains("\"frame\""))
        .count()
}

/// **"THE FINISH HE NEVER HEARD", PLAYED BACK** — `cap-continue.jsonl`, recorded on `claude`
/// 2.1.283 on 2026-09-27 (richos-hq `docs/verification/2026-09-27-background-command-finish/`
/// §1): the back end starts `sleep 12; echo bg-marker-done` in the background and ends its
/// turn with "Background command started."; 10.5 s later the provider's `task_notification`
/// says it ended, outside any turn; the host asks for the report; the provider first runs a
/// turn of its own for the notification, then ours, which reads the output and says what it
/// printed.
///
/// Before `24d9930c`/`bfb20a48` the assignment settled on "Background command started." and
/// that was the last he heard. Now the finish is his report.
#[test]
fn the_finish_he_never_heard_reaches_him_as_the_report_when_the_provider_says_it_ended() {
    const ASKED: &str = "Run sleep 12; echo bg-marker-done in the background and tell me what it printed.";
    let r = replay("2026-09-27-cap-continue.jsonl", 2.0, &[ASKED]);
    r.host.start();
    let job = r.register(0, ASKED);
    assert!(r.host.wait_for_completed(1, HANG_GUARD), "the job never ended: {:#?}", r.transcript());
    let row = r.row(&job);
    assert!(row.was_answered(), "{:?}: {}", row.state, row.detail);
    let told = r.told_about(&job);
    assert_eq!(
        told.last().map(String::as_str),
        Some("The background command slept for 12 seconds and printed `bg-marker-done`."),
        "the finish never reached him; all he was told: {told:?}"
    );
    assert_eq!(told.first().map(String::as_str), Some("Background command started."), "{told:?}");
    let sent = r.sent();
    assert_eq!(sent.len(), 2, "{sent:?}");
    assert!(sent[1].contains("Background commands ended (provider observations):"), "the second message was not the report request: {}", sent[1]);
    assert!(r.events("divergence").is_empty(), "the host left the recording: {:#?}", r.events("divergence"));
    r.end();
}

/// **THE PROVIDER'S OWN FINISH TURN, SPOKEN TO NOBODY** — `cap-plain.jsonl`, the same session
/// with nobody asking: after the notification the provider runs a turn of its own ("The
/// background command completed successfully (exit code 0)") with no prompt parked, so its
/// words go to the between-turn lane, which the host never reads. The recording ends there.
///
/// The host must not leave it there: once the provider says the command ended, it asks the
/// back end for its report. That request is past the end of this recording, so the replay
/// refuses it (exit 3) — which is exactly the evidence that it was made. A host that settled
/// on "it has started" sends nothing, and the recording simply runs out.
#[test]
fn after_the_providers_own_finish_turn_the_host_asks_for_the_report() {
    const ASKED: &str = "Run sleep 12; echo bg-marker-done in the background.";
    let r = replay("2026-09-27-cap-plain.jsonl", 2.0, &[ASKED]);
    r.host.start();
    let job = r.register(0, ASKED);
    let frames = frames_in("2026-09-27-cap-plain.jsonl");
    // Decided by facts, never by a clock: either the host asked (the replay's divergence), or
    // the whole recording has been played AND the host has closed the job without asking. A
    // host that asks keeps the job open until its report turn, so the second can only happen
    // to a host that never asks.
    r.until("the host asked, or closed the job after the whole recording without asking", |r| {
        !r.events("divergence").is_empty()
            || (r.events("emitted").len() >= frames && r.host.wait_for_completed(1, Duration::ZERO))
    });
    let asked = r.events("divergence");
    assert_eq!(
        asked.len(), 1,
        "the provider said the command ended and the host never asked the back end for its report \
         (he was told only {:?})", r.told_about(&job)
    );
    assert_eq!(asked[0]["why"], "the recording has no more sends");
    assert!(asked[0]["text"].as_str().unwrap_or_default().contains("Background commands ended (provider observations):"), "{:#?}", asked[0]);
    r.end();
}

/// **A LONG SESSION: TWO FINISHES AFTER THEIR TURNS ENDED, A JOB BETWEEN, A FOLD BESIDE THEM** —
/// `2026-10-09-long-session.jsonl`, recorded in the test VM on `claude` 2.1.295 (haiku) by
/// `scripts/replay/record-long-session.py` through `scripts/testvm/replay-capture-walk.sh`, 104.8 s,
/// the shape every one of the six 2026-09/10 incidents had (Sage, research 2026-10-08):
///
/// - A starts `sleep 45; echo long-marker-a` in the background and ends its turn;
/// - B, his next job, runs on the same back end while A's command still runs;
/// - A's command ends at 47.2 s, outside any turn; the provider runs a turn of its own for it
///   ("The background task has finished. I'm leaving it out of this report ..."), and the host's
///   request for A's report waits behind it;
/// - C's background `sleep 4` ends inside C's own turn (a foreground `sleep 12` keeps it open);
/// - D starts `sleep 30; echo long-marker-d`, ends its turn, and its finish comes 29.4 s later,
///   outside any turn, with the host waiting on it alone.
///
/// Each report reaches him on its own job, B's answer carries nothing of A's, the fold is not
/// asked about again, and the host sends exactly the six messages the provider was recorded
/// answering, the two report requests exactly where they were recorded.
#[test]
fn a_long_session_reports_each_late_finish_on_its_own_job() {
    const A: &str = "Start the long marker in the background and tell me what it printed.";
    const B: &str = "Print the second marker.";
    const C: &str = "Start the short marker in the background, wait twelve seconds, and tell me what it printed.";
    const D: &str = "Start the last marker in the background and tell me what it printed.";
    let r = replay("2026-10-09-long-session.jsonl", 4.0, &[A, B, C, D]);
    r.host.start();
    let a = r.register(0, A);
    r.until("A's first words", |r| !r.told_about(&a).is_empty());
    let b = r.register(1, B);
    r.until("A's report and B's answer", |r| answered(r, &a, 2) && answered(r, &b, 1));
    let c = r.register(2, C);
    r.until("C's answer", |r| answered(r, &c, 1));
    let d = r.register(3, D);
    r.until("D's report", |r| answered(r, &d, 2));

    let told_a = r.told_about(&a);
    assert_eq!(told_a.len(), 2, "{told_a:?}");
    assert!(told_a[0].contains("has started"), "{told_a:?}");
    assert!(told_a[1].starts_with("**Report: \"start the long marker\"**") && told_a[1].contains("long-marker-a"),
            "A's finish did not reach him as A's report: {told_a:?}");
    assert_eq!(r.told_about(&b), ["It printed `long-marker-b`."], "B's answer is B's alone");
    assert_eq!(r.told_about(&c), ["The background command printed `long-marker-c` and exited with code 0."]);
    let told_d = r.told_about(&d);
    assert_eq!(told_d.len(), 2, "{told_d:?}");
    assert_eq!(told_d[0], "The background command `sleep 30; echo long-marker-d` has started.");
    assert!(told_d[1].starts_with("**Report: \"start the last marker\"**") && told_d[1].contains("long-marker-d"),
            "D's finish did not reach him as D's report: {told_d:?}");
    // The provider's own finish turns ("I'm leaving it out of this report", "I haven't read its
    // output file") reached nobody, as they must: they are not anybody's answer.
    for told in [&told_a, &told_d] {
        assert!(!told.iter().any(|t| t.contains("leaving it out") || t.contains("haven't read")), "{told:?}");
    }

    let sent = r.sent();
    assert_eq!(sent.len(), 6, "{sent:#?}");
    let reports: Vec<usize> = (0..sent.len()).filter(|&i| sent[i].contains("Background commands ended (provider observations):")).collect();
    assert_eq!(reports, [2, 5], "the report requests are not where the provider was recorded taking them");
    assert!(sent[1].contains("Other assignments' background commands (observed)") && sent[1].contains(A.trim_end_matches('.')),
            "B was not told A's command was still running: {}", sent[1]);
    assert!(r.events("divergence").is_empty(), "the host left the recording: {:#?}", r.events("divergence"));
    r.end();
}

/// **A FINISH FOLDED INTO THE BACK END'S OWN TURN IS NOT ASKED ABOUT AGAIN** — `cap-fold.jsonl`:
/// the background `sleep 4` ends at 6.9 s, inside the same turn (a foreground `sleep 12` keeps
/// it running), and that turn's answer already describes it. One message, one report.
#[test]
fn a_finish_folded_into_the_turn_is_reported_once_with_that_turn() {
    const ASKED: &str = "Start sleep 4 in the background, then run sleep 12 in the foreground.";
    let r = replay("2026-09-27-cap-fold.jsonl", 2.0, &[ASKED]);
    r.host.start();
    let job = r.register(0, ASKED);
    assert!(r.host.wait_for_completed(1, HANG_GUARD), "the job never ended: {:#?}", r.transcript());
    assert!(r.row(&job).was_answered());
    assert_eq!(
        r.told_about(&job),
        ["The background command completed first (after 4 seconds) and its notification arrived while the \
          foreground command was still running, demonstrating that background tasks don't block the main thread."]
    );
    assert_eq!(r.sent().len(), 1, "a folded finish was asked about again: {:?}", r.sent());
    assert!(r.events("divergence").is_empty(), "{:#?}", r.events("divergence"));
    r.end();
}

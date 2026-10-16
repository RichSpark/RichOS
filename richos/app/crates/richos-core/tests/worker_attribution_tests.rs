//! WHOSE AI workers is RichOS looking at?
//!
//! Until 2026-08-29 the answer was *"whichever `~/.claude/teams/session-*` directory was
//! touched most recently"*, and that answer did not stay in the UI: `reprime.rs` reads
//! `worker_status::current_status()` and renders it into the payload injected into every
//! fresh Claude session, under a header calling it live worker state. So a directory
//! chosen by a file timestamp became an assertion, to Rich, that another session's workers
//! were his — inside the one payload whose Tier A #1 identity assertion exists precisely to
//! make false attribution structurally impossible (continuity design §2.1/§6).
//!
//! It was not hypothetical. Four `session-*` directories were present on the development
//! machine when this was found, and the mtime-newest belonged to the session writing the
//! fix, not to any session RichOS was serving.
//!
//! These tests pin the replacement — *derive the directory from the session identity, and
//! report nothing when it cannot be derived* — and, above all, they pin its absence of a
//! fallback. `an_unmatched_session_reads_nothing_even_with_a_busy_decoy_dir_present` and
//! `no_lease_reads_nothing_even_with_a_busy_decoy_dir_present` both FAIL if an mtime pick
//! is ever restored as a "best effort" last resort, because in both the decoy is newest and
//! full of live workers. `the_source_carries_no_mtime_selection_primitive` fails if one is
//! written at all.
//!
//! Every fixture below puts MORE THAN ONE session directory on disk, because a single-dir
//! fixture cannot tell a derivation apart from a guess — with one candidate, both are right.

use richos_core::entity::EntityId;
use richos_core::ledger::Ledger;
use richos_core::reprime::RePrimePayload;
use richos_core::worker_status::{self, Unattributed};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// `HOME` and `RICHOS_TEAM_DIR` are process-global. Every test here writes both, so every
/// test here takes this first — via [`env_guard`], never `.lock().unwrap()`.
static ENV_GUARD: Mutex<()> = Mutex::new(());

/// Take the env lock, RECOVERING FROM POISONING.
///
/// Not a nicety: verified by running the failure. With `.unwrap()`, reintroducing the mtime
/// fallback made the first test panic, poison the mutex, and the other six then failed with
/// `PoisonError` instead of their own assertions — so the suite reported seven failures and
/// explained two. A tripwire that obscures what tripped it is half a tripwire. There is no
/// state behind this lock to corrupt (it guards two env vars every test rewrites on entry),
/// so recovery is sound as well as clearer.
fn env_guard() -> std::sync::MutexGuard<'static, ()> {
    ENV_GUARD.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The session whose workers these actually are, and its team dir's `session-<first8>`.
const MINE_FULL: &str = "aaaaaaaa-1111-2222-3333-444444444444";
const MINE_DIR: &str = "session-aaaaaaaa";
/// The decoy: a DIFFERENT session, deliberately the most recently modified directory on
/// the fixture "machine", deliberately full of live workers.
const DECOY_FULL: &str = "dddddddd-9999-8888-7777-666666666666";
const DECOY_DIR: &str = "session-dddddddd";
/// A session that exists as far as the app is concerned and has no team directory: the
/// ordinary state of a session that has dispatched no workers.
const ORPHAN_FULL: &str = "eeeeeeee-5555-5555-5555-555555555555";

fn tmp_home(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "richos-attribution-{tag}-{}-{}",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    std::fs::create_dir_all(root.join(".claude").join("teams")).unwrap();
    root
}

fn row(session: &str, state: &str, agent: &str, name: &str) -> String {
    // `host_pid` is THIS test process, so `probe_host`'s real `kill -0` syscall witnesses a
    // genuinely live host. Nothing here fakes the liveness half.
    format!(
        r#"{{"timestamp":"2026-08-29T04:00:00+00:00","lifecycle_state":"{state}","source_hook":"h","agent_id":"{agent}","agent_type":"dev","worker_name":"{name}","session_id":"{session}","host_pid":{},"decision":"logged"}}"#,
        std::process::id()
    )
}

fn write_team_dir(home: &Path, dir: &str, rows: &[String]) -> PathBuf {
    let d = home.join(".claude").join("teams").join(dir);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("worker-events.jsonl"), rows.join("\n")).unwrap();
    d
}

fn mtime(p: &Path) -> std::time::SystemTime {
    std::fs::metadata(p).unwrap().modified().unwrap()
}

/// Four session directories, exactly like the real machine. The decoy is created LAST so
/// it is the mtime-newest, and it is the only one with more than one open run — so any
/// answer that comes from a timestamp is loudly distinguishable from an answer that comes
/// from an identity.
fn four_dir_machine(tag: &str) -> PathBuf {
    let home = tmp_home(tag);
    let mine = write_team_dir(&home, MINE_DIR, &[row(MINE_FULL, "started", "a-mine", "sage-opus-r3")]);
    std::thread::sleep(std::time::Duration::from_millis(20));
    write_team_dir(&home, "session-bbbbbbbb", &[row("bbbbbbbb-0-0-0-0", "started", "a-b", "someone-elses-b")]);
    std::thread::sleep(std::time::Duration::from_millis(20));
    write_team_dir(&home, "session-cccccccc", &[row("cccccccc-0-0-0-0", "started", "a-c", "someone-elses-c")]);
    std::thread::sleep(std::time::Duration::from_millis(20));
    let decoy = write_team_dir(
        &home,
        DECOY_DIR,
        &[
            row(DECOY_FULL, "started", "a-d1", "not-my-worker-1"),
            row(DECOY_FULL, "started", "a-d2", "not-my-worker-2"),
            row(DECOY_FULL, "started", "a-d3", "not-my-worker-3"),
        ],
    );

    // THE TRAP IS ARMED — asserted, not assumed. If the decoy were not the newest, every
    // test in this file would pass for the wrong reason.
    assert!(
        mtime(&decoy) > mtime(&mine),
        "fixture invariant: the decoy must be the most-recently-modified team dir"
    );
    home
}

fn with_home<T>(home: &Path, f: impl FnOnce() -> T) -> T {
    let previous = std::env::var_os("HOME");
    std::env::remove_var("RICHOS_TEAM_DIR");
    std::env::set_var("HOME", home);
    let out = f();
    match previous {
        Some(v) => std::env::set_var("HOME", v),
        None => std::env::remove_var("HOME"),
    }
    out
}

// ---------------------------------------------------------------------------------------
// THE RIGHT DIRECTORY, WITH FOUR ON DISK
// ---------------------------------------------------------------------------------------

#[test]
fn the_session_the_app_is_serving_picks_its_own_dir_out_of_four() {
    let _guard = env_guard();
    let home = four_dir_machine("right-one");

    let (dir, status) = with_home(&home, || {
        (
            worker_status::resolve_team_dir(Some(MINE_FULL)),
            worker_status::current_status(Some(MINE_FULL)),
        )
    });

    assert_eq!(dir.unwrap(), home.join(".claude").join("teams").join(MINE_DIR));
    assert!(status.is_attributed(), "a derived directory IS an attribution");
    assert_eq!(status.active, 1, "exactly this session's one open run — not the decoy's three");
    assert_eq!(status.items.len(), 1);
    assert_eq!(status.items[0].label, "sage-opus-r3");
    let json = serde_json::to_string(&status).unwrap();
    assert!(!json.contains("not-my-worker"), "no other session's worker may appear: {json}");

    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn the_decoys_own_session_still_reads_the_decoys_dir() {
    // The positive control for the test above: the derivation is not "always the oldest" or
    // "never the newest" — it is the directory the SESSION ID names, whichever that is.
    let _guard = env_guard();
    let home = four_dir_machine("decoy-owner");

    let status = with_home(&home, || worker_status::current_status(Some(DECOY_FULL)));

    assert_eq!(status.active, 3);
    assert!(status.is_attributed());
    assert!(status.items.iter().all(|i| i.label.starts_with("not-my-worker")));

    let _ = std::fs::remove_dir_all(&home);
}

// ---------------------------------------------------------------------------------------
// THE HONEST NOTHING — AND THE FALLBACK THAT MUST NOT COME BACK
// ---------------------------------------------------------------------------------------

#[test]
fn an_unmatched_session_reads_nothing_even_with_a_busy_decoy_dir_present() {
    // THIS IS THE ANTI-REGRESSION TEST. Frank's suggested fix kept the mtime pick as a
    // last resort "marked unknown"; the CEO's instruction removed it outright, because a
    // guess that reaches the model's prompt is the whole defect and a label on it does not
    // stop it being read. Restore any mtime fallback and this fails: the decoy is the
    // newest directory on the fixture machine and holds three live workers.
    let _guard = env_guard();
    let home = four_dir_machine("unmatched");

    let (dir, status) = with_home(&home, || {
        (
            worker_status::resolve_team_dir(Some(ORPHAN_FULL)),
            worker_status::current_status(Some(ORPHAN_FULL)),
        )
    });

    assert_eq!(dir, Err(Unattributed::NoTeamDirForSession));
    assert_eq!(status.active, 0, "three live workers next door are still not this session's");
    assert_eq!(status.liveness_unknown, 0);
    assert!(status.items.is_empty());
    assert_eq!(status.unattributed, Some(Unattributed::NoTeamDirForSession));
    assert!(!status.is_attributed());
    assert_eq!(
        status.unattributed.unwrap().reason(),
        "this session has no team directory on disk",
        "and it says WHY, rather than reporting a bare zero"
    );

    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn no_lease_reads_nothing_even_with_a_busy_decoy_dir_present() {
    // The other half of the same guarantee. No lease = the app does not know which session
    // it is serving = it reads nothing. An mtime fallback fires HERE first, so this is the
    // second independent tripwire.
    let _guard = env_guard();
    let home = four_dir_machine("no-lease");

    let status = with_home(&home, || worker_status::current_status(None));

    assert_eq!(status.active, 0);
    assert!(status.items.is_empty());
    assert_eq!(status.unattributed, Some(Unattributed::NoSession));

    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn a_session_id_that_cannot_name_a_directory_reads_nothing_and_never_escapes_teams() {
    let _guard = env_guard();
    let home = four_dir_machine("hostile");

    with_home(&home, || {
        for hostile in ["../../..", "..", "a/b/cccc", "sess"] {
            assert_eq!(
                worker_status::resolve_team_dir(Some(hostile)),
                Err(Unattributed::UnusableSessionId),
                "{hostile:?} must not become a path"
            );
        }
        // And the well-formed-but-absent case is a DIFFERENT reason, so an operator can
        // tell "your id is malformed" from "you have dispatched no workers".
        assert_eq!(
            worker_status::resolve_team_dir(Some(ORPHAN_FULL)),
            Err(Unattributed::NoTeamDirForSession)
        );
    });

    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn an_explicit_override_outranks_the_derivation_and_does_not_fall_back() {
    let _guard = env_guard();
    let home = four_dir_machine("override");
    let explicit = write_team_dir(&home, "an-explicitly-named-dir", &[row("anything", "started", "a-x", "explicit-worker")]);

    let previous = std::env::var_os("HOME");
    std::env::set_var("HOME", &home);

    // Resolves: an override is an identity STATEMENT, not an inference, so it is honoured
    // even with no session id at all. `SessionScope::from_team_dir` yields `Any` for a
    // directory not named `session-*`, which is why the row's arbitrary session id counts.
    std::env::set_var("RICHOS_TEAM_DIR", &explicit);
    let honoured = worker_status::current_status(None);

    // Does NOT resolve: and it fails rather than falling through to the derivation or to
    // the decoy. An operator who named a directory does not want a different one.
    std::env::set_var("RICHOS_TEAM_DIR", home.join("nope-not-here"));
    let refused = worker_status::current_status(Some(MINE_FULL));

    std::env::remove_var("RICHOS_TEAM_DIR");
    match previous {
        Some(v) => std::env::set_var("HOME", v),
        None => std::env::remove_var("HOME"),
    }

    assert_eq!(honoured.active, 1);
    assert_eq!(honoured.items[0].label, "explicit-worker");
    assert!(honoured.is_attributed());

    assert_eq!(refused.active, 0, "a broken override never falls back to a derived or guessed dir");
    assert!(refused.items.is_empty());
    assert_eq!(refused.unattributed, Some(Unattributed::OverrideNotADirectory));

    let _ = std::fs::remove_dir_all(&home);
}

// ---------------------------------------------------------------------------------------
// THE STRUCTURAL TRIPWIRE
// ---------------------------------------------------------------------------------------

#[test]
fn the_source_carries_no_mtime_selection_primitive() {
    // The behavioral tests above catch a restored fallback by its OUTPUT. This catches it
    // by its INGREDIENTS, one step earlier, and it is the cheap half of Frank's F3 point:
    // an invariant held by a comment is not held.
    //
    // Doc and line comments are stripped before scanning, deliberately — `worker_status.rs`
    // now DESCRIBES the removed mtime pick at length, and a guard that forbade naming the
    // defect would delete the only record of why the code looks like this.
    let src = include_str!("../src/worker_status.rs");
    let code: String = src
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");

    for banned in ["max_by_key", "modified()", "read_dir", "elapsed()"] {
        assert!(
            !code.contains(banned),
            "worker_status.rs must not select a directory from filesystem activity — found `{banned}` in code"
        );
    }
    // And the derivation's own ingredient must still be there, so this test cannot pass by
    // the module having been gutted.
    assert!(code.contains("team_dir_name"), "the identity-derived name must still be how the dir is chosen");
}

// ---------------------------------------------------------------------------------------
// WHAT THE RE-PRIME PROMPT ACTUALLY SAYS
// ---------------------------------------------------------------------------------------
//
// The UI half of this defect is a wrong chip. The half that matters is that the same number
// is rendered into the payload injected into every fresh Claude session, so these assertions
// are on the PROMPT TEXT — the thing a successor Rich reads as authoritative.

fn ledger_with_a_thread(tag: &str) -> (std::path::PathBuf, Ledger, String) {
    let path = std::env::temp_dir().join(format!(
        "richos-attrib-ledger-{tag}-{}-{}.jsonl",
        std::process::id(),
        richos_core::util::now_millis()
    ));
    let _ = std::fs::remove_file(&path);
    let mut ledger = Ledger::open(&path).unwrap();
    let thread = ledger.create_thread("General", &EntityId::parse("femcboost").unwrap()).unwrap();
    (path, ledger, thread)
}

#[test]
fn an_unknown_session_dir_makes_the_priming_prompt_say_so_instead_of_falling_silent() {
    let _guard = env_guard();
    let home = four_dir_machine("payload-unknown");
    let (path, ledger, thread) = ledger_with_a_thread("unknown");
    let binding = ledger.thread_binding(&thread).unwrap();

    let payload = with_home(&home, || {
        RePrimePayload::assemble(&ledger, &binding, 8, Some(ORPHAN_FULL)).unwrap()
    });
    let priming = payload.to_priming_prompt();

    assert_eq!(payload.worker_state_unknown, Some(Unattributed::NoTeamDirForSession));
    assert!(payload.worker_state.is_empty());

    // It SPEAKS rather than omitting the section. Silence here reads as "no workers are
    // running", which is a claim nobody may make when the app could not identify whose
    // workers it would be counting — the same rule the ACTION LEDGER section follows.
    assert!(priming.contains("worker_state_unknown"), "{priming}");
    assert!(priming.contains("no_team_dir_for_session"), "it says WHY");
    assert!(
        priming.contains("worker_state_unknown"),
        "and it forbids the false denial the absence would otherwise invite"
    );
    // And not one byte of the three live workers next door reaches the prompt.
    assert!(!priming.contains("not-my-worker"), "another session's worker in Rich's prompt: {priming}");

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&home);
}

/// Hunt 2026-09-29 part 1, finding 41, at the surface where it did damage: this session's
/// own worker log is there and cannot be read (a folder where the file goes). Before, that
/// read as an empty stream, the directory counted as attributed, and the section was left
/// out: a successor read the silence as "no workers are running". It now says it does not
/// know, and it does not claim the directory was unidentified, because it was identified.
#[test]
fn an_unreadable_worker_log_makes_the_priming_prompt_say_so_instead_of_falling_silent() {
    let _guard = env_guard();
    let home = four_dir_machine("payload-unreadable");
    let log = home.join(".claude").join("teams").join(MINE_DIR).join("worker-events.jsonl");
    std::fs::remove_file(&log).unwrap();
    std::fs::create_dir_all(&log).unwrap();
    let (path, ledger, thread) = ledger_with_a_thread("unreadable");
    let binding = ledger.thread_binding(&thread).unwrap();

    let payload = with_home(&home, || {
        RePrimePayload::assemble(&ledger, &binding, 8, Some(MINE_FULL)).unwrap()
    });
    let priming = payload.to_priming_prompt();

    assert_eq!(payload.worker_state_unknown, Some(Unattributed::WorkerLogUnreadable));
    assert!(priming.contains("worker_state_unknown"), "{priming}");
    assert!(priming.contains("worker_log_unreadable"), "it says WHY");
    assert!(priming.contains("worker_state_unknown"));
    assert!(!priming.contains("could not be identified"), "the directory WAS identified: {priming}");
    assert!(!priming.contains("not-my-worker"), "{priming}");

    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir_all(&home).unwrap();
}

/// Hunt 2026-09-29 part 1 v2, finding 41 (the witness is `part-1-codex-v2/witness-v2.rs` in
/// richos-hq, ported here): a log that IS readable and holds a damaged `started` row — the
/// valid row with its closing brace gone — used to parse to an empty stream, count
/// `active: 0` with no reason, and leave the prompt silent: the authoritative zero. It now
/// says it does not know. A damaged completed-task history says so too, rather than listing
/// nothing as if nothing had finished.
///
/// **RED at `c6cde6cc9`**: `worker_state_unknown` was `None` and the prompt had no worker
/// section; the unreadable task history added no line.
#[test]
fn a_damaged_worker_log_or_task_history_makes_the_priming_prompt_say_so_instead_of_counting_zero() {
    let _guard = env_guard();
    let home = four_dir_machine("payload-damaged");
    let team = home.join(".claude").join("teams").join(MINE_DIR);
    let log = team.join("worker-events.jsonl");
    let good = row(MINE_FULL, "started", "a-mine", "sage-opus-r3");
    let good = good.trim_end();
    std::fs::write(&log, format!("{}\n", &good[..good.len() - 1])).unwrap();
    let (path, ledger, thread) = ledger_with_a_thread("damaged");
    let binding = ledger.thread_binding(&thread).unwrap();

    let payload = with_home(&home, || RePrimePayload::assemble(&ledger, &binding, 8, Some(MINE_FULL)).unwrap());
    let priming = payload.to_priming_prompt();
    assert!(payload.worker_state_unknown.is_some(), "a damaged started row was counted as no worker at all");
    assert!(priming.contains("worker_state_unknown"), "{priming}");
    assert!(priming.contains("worker_log_damaged"), "it says WHY: {priming}");
    assert!(priming.contains("worker_state_unknown"), "{priming}");
    assert!(!priming.contains("could not be identified"), "the directory WAS identified: {priming}");

    // The worker log whole again, and the completed-task history unreadable (a folder where
    // the file goes): the workers are counted, and the list says its finished half is missing.
    std::fs::write(&log, format!("{good}\n")).unwrap();
    std::fs::create_dir_all(team.join("task-events.jsonl")).unwrap();
    let payload = with_home(&home, || RePrimePayload::assemble(&ledger, &binding, 8, Some(MINE_FULL)).unwrap());
    assert_eq!(payload.worker_state_unknown, None, "{:?}", payload.worker_state_unknown);
    assert!(
        payload.worker_state.iter().any(|line| line.contains("completed-task history is there but could not be read")),
        "an unreadable task history listed nothing as if nothing had finished: {:?}",
        payload.worker_state
    );

    std::fs::remove_file(&path).unwrap();
    std::fs::remove_dir_all(&home).unwrap();
}

#[test]
fn no_lease_never_puts_another_sessions_workers_in_the_prompt() {
    // THE ORIGINAL DEFECT, at the surface where it did damage. Before this commit
    // `assemble` called `current_status()` with no argument at all, and the answer was the
    // mtime-newest directory on the machine — here, three workers belonging to a session
    // RichOS has never served, rendered under "LIVE WORKER STATE".
    let _guard = env_guard();
    let home = four_dir_machine("payload-no-lease");
    let (path, ledger, thread) = ledger_with_a_thread("no-lease");
    let binding = ledger.thread_binding(&thread).unwrap();

    let payload = with_home(&home, || RePrimePayload::assemble(&ledger, &binding, 8, None).unwrap());
    let priming = payload.to_priming_prompt();

    assert_eq!(payload.worker_state_unknown, Some(Unattributed::NoSession));
    assert!(priming.contains("no_session"));
    assert!(!priming.contains("not-my-worker"), "{priming}");

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn the_serving_sessions_own_workers_do_reach_the_prompt_and_only_those() {
    // The positive half: this is a fix, not a mute button. Attributed state still lands in
    // Tier B #7 — scoped to the session the payload is being built for.
    let _guard = env_guard();
    let home = four_dir_machine("payload-mine");
    let (path, ledger, thread) = ledger_with_a_thread("mine");
    let binding = ledger.thread_binding(&thread).unwrap();

    let payload = with_home(&home, || {
        RePrimePayload::assemble(&ledger, &binding, 8, Some(MINE_FULL)).unwrap()
    });
    let priming = payload.to_priming_prompt();

    assert_eq!(payload.worker_state_unknown, None, "a derived directory IS an attribution");
    assert_eq!(payload.worker_state, vec!["[active] sage-opus-r3".to_string()]);
    assert!(priming.contains("worker_state"));
    assert!(priming.contains("[active] sage-opus-r3"));
    assert!(!priming.contains("not-my-worker"), "{priming}");
    assert!(!priming.contains("NOT AVAILABLE"), "an attributed read never claims it is unknown");

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&home);
}

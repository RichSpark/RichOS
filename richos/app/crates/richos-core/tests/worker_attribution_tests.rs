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

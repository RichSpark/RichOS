#!/usr/bin/env python3
"""nightly-engine.py — every engine unit, with every mutation pass, once a night, as a job of its own.

    nightly-engine.py [--commit REV] [--state-dir DIR] [--app-state-dir DIR] [--dry-run]

WHY THIS EXISTS (2026-09-30). The merge gate runs only the checks a change owns, each capped at
600 s, and leaves the mutation passes to the nightly (autocheck/README.md). The app nightly
(nightly-local.py) has no engine gate: it ran the workspace suites' passes and nothing else of
the engine. So the fence suite's mutation unit, the passes about seventy engine suites run at
their own end, and every engine unit the gate cut at its cap ran nowhere. The lead's decision
on esc-20260930T223507Z-b12f0d6a: a nightly engine run of every unit, with its mutation passes,
as its OWN job, so it never gates or lengthens the app nightly's release (the CEO counts a
build's time from his order to a usable nightly); its failures reach the lead through
escalate.sh; then the merge gate switches the embedded passes off.

WHAT IT DOES
  1. Refuses to start (exit 75) while an app nightly holds its release lock
     (<app-state-dir>/release.lock, observed with a non-blocking flock that is released at
     once): the app nightly is never made to share the Mac with this. A lock file that does
     not exist means no app nightly ever ran from that state directory, so none is running.
     An unreadable one refuses (exit 2): unknown is not permission. The same observation is
     repeated every YIELD_POLL seconds for the whole run, because an app nightly can start
     after this one did and the app nightly never looks for this job: when one does, this run
     YIELDS (its runner's whole process group is stopped, exit 75, no escalation) so the
     release never queues behind engine work. A lock that becomes unreadable mid-run yields
     too (exit 2).
  2. Checks the commit out (default: main's tip) into a detached worktree of its own under
     the state directory, so a land moving main during the run changes nothing it reads, and
     the engine's leak canary watches a tree nobody else writes.
  3. Runs every unit `ci-units.sh units` lists through that commit's own proof-run.py, with
     RICHOS_MUTATION_PASSES=1 and RICHOS_FOURTEEN_MUTANTS=1 (every mutation pass on) and no
     --cap: each unit keeps the engine runner's own deadline, and the receipts check proves
     every unit ran. Admission is proof-run's (CEO ruling §77's CPU line), outside the main
     checkout, so a land's checks keep their priority.
  4. Anything that did not pass raises ONE escalation for the lead naming each check, its
     state and the logs. A run that could not produce a verdict raises one too.
  5. Removes its worktree however it ends (§54) and keeps the last KEEP_RUNS runs' logs.

It is started by hand or by a schedule, after the app nightly; nothing in nightly-local.py
calls it. Exit: 0 every unit passed; 1 something did not pass (escalation raised); 2 setup
refused or no verdict; 75 not started or yielded, an app nightly holds its release lock.

REFERENCE: adoption ledger §2.4 (release gating), COPY THE APPROACH: affected checks at the
merge, each short; the expensive passes run in a scheduled job of their own.
"""
import argparse
import datetime
import errno
import fcntl
import json
import os
from pathlib import Path
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
import machine_storage as M
import shutil
import signal
import subprocess
import sys
import tempfile
import time

HERE = Path(__file__).resolve().parent
DEFAULT_STATE = Path.home() / ".richos-nightly/nightly-engine"
DEFAULT_APP_STATE = Path.home() / ".richos-nightly"
KEEP_RUNS = 3
# Every mutation pass on: the engine suites' own harnesses run unless told 0 (the merge gate's
# switch), the workspace suites' and the fourteen-point pass only when told 1.
PASSES_ON = {"RICHOS_MUTATION_PASSES": "1", "RICHOS_FOURTEEN_MUTANTS": "1"}
NOT_STARTED = 75
YIELD_POLL = 15    # seconds between looks at the app nightly's lock while the units run
STOP_GRACE = 10    # seconds a yielding run's process group gets after SIGTERM before SIGKILL
# One selected unit, in the line shape proof-for.sh prints and proof-run.py plans.
UNIT_RUNNER = ["bash", "scripts/ci-shard.sh", "--only-units"]


def unit_line(unit):
    return "cd richos/engine && " + " ".join(UNIT_RUNNER + [unit]) + "\n"


def say(text):
    print("nightly-engine: " + text, flush=True)


def git(repo, *args):
    return subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True,
                          text=True).stdout.strip()


def app_nightly_running(app_state):
    """True while an app nightly holds <app_state>/release.lock; False when it is free or was
    never created. Raises OSError when the lock exists and cannot be inspected."""
    try:
        stream = open(app_state / "release.lock", "rb")
    except FileNotFoundError:
        return False
    with stream:
        try:
            fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError as exc:
            if exc.errno in (errno.EWOULDBLOCK, errno.EAGAIN):
                return True
            raise
        fcntl.flock(stream, fcntl.LOCK_UN)
        return False


def _group_present(proc):
    """True while any process of the runner's own group exists (the leader is reaped first)."""
    proc.poll()
    try:
        os.killpg(proc.pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def stop_group(proc, grace):
    """SIGTERM the runner's group (it was started as its own session), SIGKILL what remains after
    `grace` seconds. The leader exiting is not the group exiting."""
    for sig, wait in ((signal.SIGTERM, grace), (signal.SIGKILL, 5)):
        if not _group_present(proc):
            break
        try:
            os.killpg(proc.pid, sig)
        except (ProcessLookupError, PermissionError):
            pass
        end = time.monotonic() + wait
        while _group_present(proc) and time.monotonic() < end:
            time.sleep(0.05)
    proc.wait()


def run_yielding(argv, cwd, env, app_state, poll, grace):
    """Run `argv`; while it runs, look at the app nightly's lock every `poll` seconds. Returns
    (exit status, None) when it finished, or (None, "running" | OSError) after yielding."""
    proc = subprocess.Popen(argv, cwd=cwd, env=env, stdin=subprocess.DEVNULL, start_new_session=True)
    try:
        while True:
            try:
                return proc.wait(timeout=poll), None
            except subprocess.TimeoutExpired:
                pass
            try:
                if app_nightly_running(app_state):
                    return None, "running"
            except OSError as exc:
                return None, exc
    finally:
        if _group_present(proc):
            stop_group(proc, grace)


def engine_units(src):
    """Every unit id the engine's own inventory lists at this commit."""
    listed = subprocess.run(["bash", "scripts/ci-units.sh", "units"], cwd=src / "richos/engine",
                            capture_output=True, text=True)
    if listed.returncode:
        raise RuntimeError("ci-units.sh units exited %d: %s" % (listed.returncode, listed.stderr.strip()[-400:]))
    units = [row.split("\t", 1)[0] for row in listed.stdout.splitlines() if row.strip()]
    if not units:
        raise RuntimeError("ci-units.sh listed no units")
    return units


def not_passed(summary):
    """[(check, result)] of every check in proof-run's summary.json that did not pass."""
    with open(summary) as stream:
        rows = json.load(stream)["checks"]
    return [(row["check"], row["result"]) for row in rows if row.get("result") != "passed"]


def escalate(repo, src, title, question, tried):
    """One escalation for the lead, through the checked-out commit's escalate.sh (the installed
    engine's when that commit has none). Returns escalate.sh's exit status."""
    tool = src / "richos/engine/scripts/escalate.sh"
    if not tool.is_file():
        tool = Path.home() / ".claude/richos-engine/scripts/escalate.sh"
    fields = {"title": title, "state": "work-complete", "question": question, "for": "lead",
              "tried": tried, "meanwhile": "Nothing waits on this run: the app nightly and every merge are unaffected."}
    with tempfile.NamedTemporaryFile("w", prefix="nightly-engine-", suffix=".json", delete=False) as out:
        json.dump(fields, out)
        path = out.name
    try:
        done = subprocess.run(["bash", str(tool), "raise", "--fields", path, "--worktree", str(repo),
                               "--teammate", "nightly-engine", "--no-record"],
                              cwd=repo, stdin=subprocess.DEVNULL, capture_output=True, text=True)
    finally:
        os.unlink(path)
    print(done.stdout + done.stderr, end="", flush=True)
    if done.returncode:
        say("ESCALATION NOT DELIVERED: escalate.sh exited %d; the title was: %s" % (done.returncode, title))
    return done.returncode


def rotate(state):
    runs = sorted(p for p in state.iterdir() if p.is_dir() and p.name[:8].isdigit())
    for old in runs[:-KEEP_RUNS]:
        shutil.rmtree(old, ignore_errors=True)


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    p.add_argument("--commit", default="main", help="what to verify (default: main's tip)")
    p.add_argument("--state-dir", type=Path, help="run folders (default: %s)" % DEFAULT_STATE)
    p.add_argument("--app-state-dir", type=Path, default=DEFAULT_APP_STATE,
                   help="the app nightly's state directory, whose release lock this waits for")
    p.add_argument("--dry-run", action="store_true", help="print the plan and run nothing")
    p.add_argument("--yield-poll", type=float, default=YIELD_POLL, metavar="SECONDS",
                   help="seconds between looks at the app nightly's lock during the run (default %d)" % YIELD_POLL)
    p.add_argument("--stop-grace", type=float, default=STOP_GRACE, metavar="SECONDS",
                   help="seconds a yielding run's group gets after SIGTERM before SIGKILL (default %d)" % STOP_GRACE)
    args = p.parse_args(argv)
    repo = Path(git(HERE, "rev-parse", "--show-toplevel"))
    state = args.state_dir
    if state is None:
        state = M.state("nightly-engine", default=DEFAULT_STATE)
    state = M.validate(state)
    try:
        if app_nightly_running(args.app_state_dir):
            say("NOT STARTED: an app nightly holds %s; run this after it ends" % (args.app_state_dir / "release.lock"))
            return NOT_STARTED
    except OSError as exc:
        say("REFUSED: the app nightly's release lock could not be inspected (%s)" % exc)
        return 2
    commit = git(repo, "rev-parse", "--verify", args.commit + "^{commit}")
    run_dir = state / ("%s-%s-%d" % (datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ"),
                                      commit[:12], os.getpid()))
    src = run_dir / "src"
    run_dir.mkdir(parents=True)
    say("commit %s; run folder %s" % (commit, run_dir))
    git(repo, "worktree", "add", "--detach", str(src), commit)
    try:
        try:
            units = engine_units(src)
        except RuntimeError as exc:
            say("NO VERDICT: %s" % exc)
            escalate(repo, src, "Nightly engine run at %s produced no verdict" % commit[:12],
                     "Why could the engine's unit inventory not be read at this commit?", str(exc))
            return 2
        commands = run_dir / "commands.txt"
        commands.write_text("".join(unit_line(u) for u in units))
        summary = run_dir / "summary.json"
        argv_run = [sys.executable, str(src / "richos/app/scripts/proof-run.py"), "--commands", str(commands),
                    "--log-dir", str(run_dir / "logs"), "--summary-out", str(summary)]
        say("%d unit(s), every mutation pass on (%s)" % (
            len(units), " ".join("%s=%s" % kv for kv in sorted(PASSES_ON.items()))))
        if args.dry_run:
            say("dry run: " + " ".join(argv_run))
            return 0
        started = datetime.datetime.now(datetime.timezone.utc)
        code, yielded = run_yielding(argv_run, src, {**os.environ, **PASSES_ON}, args.app_state_dir,
                                     args.yield_poll, args.stop_grace)
        minutes = (datetime.datetime.now(datetime.timezone.utc) - started).total_seconds() / 60
        if code is None:
            if yielded == "running":
                say("YIELDED after %.1f min: an app nightly took %s; the engine run was stopped so the "
                    "release never shares the Mac with it; run this after it ends" % (
                        minutes, args.app_state_dir / "release.lock"))
                return NOT_STARTED
            say("REFUSED after %.1f min: the app nightly's release lock could not be inspected (%s); "
                "the engine run was stopped" % (minutes, yielded))
            return 2
        done = subprocess.CompletedProcess(argv_run, code)
        try:
            bad = not_passed(summary)
        except (OSError, ValueError, KeyError, TypeError) as exc:
            say("NO VERDICT after %.1f min: proof-run.py exited %d and its summary is unreadable (%s)" % (
                minutes, done.returncode, exc))
            escalate(repo, src, "Nightly engine run at %s produced no verdict" % commit[:12],
                     "Why did proof-run.py leave no readable summary? Logs: %s" % (run_dir / "logs"),
                     "proof-run.py exited %d after %.1f min." % (done.returncode, minutes))
            return 2
        if done.returncode == 0 and not bad:
            say("PASSED: %d unit(s) with every mutation pass, %.1f min" % (len(units), minutes))
            return 0
        named = ", ".join("%s (%s)" % row for row in bad) or "none named; proof-run.py exited %d" % done.returncode
        say("FAILED after %.1f min: %s" % (minutes, named))
        escalate(repo, src, "Nightly engine run at %s: %d check(s) did not pass" % (commit[:12], len(bad)),
                 "Fix or diagnose: %s. Logs: %s" % (named, run_dir / "logs"),
                 "Every engine unit with every mutation pass, %.1f min, through proof-run.py." % minutes)
        return 1
    finally:
        subprocess.run(["git", "-C", str(repo), "worktree", "remove", "--force", str(src)],
                       capture_output=True, text=True)
        subprocess.run(["git", "-C", str(repo), "worktree", "prune"], capture_output=True, text=True)
        if src.exists():
            say("WARNING: the run's worktree %s could not be removed" % src)
        rotate(state)


if __name__ == "__main__":
    sys.exit(main())

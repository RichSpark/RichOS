#!/usr/bin/env python3
"""The lock-side and registry-side rules of agent-liveness.py (hunt part 5, P5-18, P5-19).

Real git repository, real worktree locks, the real `ps` for the process start
time; only the workspace registry is stood in (it lives in the operator's
state). Nothing outside a scratch directory is read or written. Run directly:
    python3 scripts/lib/agent-liveness.test.py
"""
import importlib.util
import os
import shutil
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("agent_liveness", os.path.join(HERE, "agent-liveness.py"))
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)

PASS = FAIL = 0


def check(name, ok, detail=""):
    global PASS, FAIL
    if ok:
        PASS += 1
        print("  PASS  %s" % name)
    else:
        FAIL += 1
        print("  FAIL  %s  %s" % (name, detail))


def git(root, *args):
    r = subprocess.run(["git", "-C", root, "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null"]
                       + list(args), capture_output=True, text=True)
    if r.returncode != 0:
        raise SystemExit("git %s failed: %s" % (" ".join(args), (r.stderr or r.stdout).strip()))


scratch = tempfile.mkdtemp(prefix="agent-liveness-test-")
try:
    # Isolate every outside source: no roster, no worker-events, no registry.
    os.environ["RICHOS_LIVENESS_TEAMS_DIR"] = os.path.join(scratch, "no-teams")
    mod._registry_says = lambda _aid: None

    repo = os.path.join(scratch, "entity")
    os.makedirs(repo)
    git(repo, "init", "-q", "-b", "main")
    git(repo, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "x")

    def locked_worktree(agent_id, reason):
        path = os.path.join(scratch, "wt", "agent-" + agent_id)
        git(repo, "worktree", "add", "-q", "-b", "b-" + agent_id, path)
        if reason is None:
            git(repo, "worktree", "lock", path)
        else:
            git(repo, "worktree", "lock", "--reason", reason, path)
        return "agent-" + agent_id

    me = os.getpid()
    lstart = subprocess.run(["ps", "-o", "lstart=", "-p", str(me)], capture_output=True, text=True,
                            env=dict(os.environ, LC_ALL="C", TZ="UTC")).stdout.strip()   # UTC, as the lock records it

    # P5-18: a live pid whose start time is not the one the lock recorded is a different process.
    t = locked_worktree("reused", "claude agent agent-reused (pid %d start Mon Jan  6 10:00:00 2020)" % me)
    r = mod.resolve(repo, t)
    check("P5-18a  live pid with a DIFFERENT recorded start is a reused pid -> NOT-ALIVE",
          r["verdict"] == mod.NOT_ALIVE, r["verdict"] + ": " + r["reason"])

    t = locked_worktree("same", "claude agent agent-same (pid %d start %s)" % (me, lstart))
    r = mod.resolve(repo, t)
    check("P5-18b  live pid whose start matches the lock's record stays ALIVE",
          r["verdict"] == mod.ALIVE, r["verdict"] + ": " + r["reason"])

    t = locked_worktree("vague", "claude agent agent-vague (pid %d start now)" % me)
    r = mod.resolve(repo, t)
    check("P5-18c  an unreadable recorded start cannot prove reuse -> still ALIVE (the old rule)",
          r["verdict"] == mod.ALIVE, r["verdict"] + ": " + r["reason"])

    # P5-19: the authoritative registry settles a finished agent whose lock names nobody.
    t = locked_worktree("empty", None)
    r = mod.resolve(repo, t)
    check("P5-19a  (control) empty lock, no registry record -> INDETERMINATE",
          r["verdict"] == mod.INDETERMINATE, r["verdict"])

    mod._registry_says = lambda _aid: {"finished": True, "paused": False, "why": "run ended",
                                       "name": "zed", "key": "k"}
    r = mod.resolve(repo, t)
    check("P5-19b  empty lock + registry says FINISHED -> NOT-ALIVE",
          r["verdict"] == mod.NOT_ALIVE, r["verdict"] + ": " + r["reason"])

    mod._registry_says = lambda _aid: {"finished": False, "paused": False, "why": "running",
                                       "name": "zed", "key": "k"}
    r = mod.resolve(repo, t)
    check("P5-19c  empty lock + registry says RUNNING -> still INDETERMINATE",
          r["verdict"] == mod.INDETERMINATE, r["verdict"])

    # 2026-10-29 08:31Z: an app worker runs in a registered cc/ workspace, so the entity has no
    # .claude/worktrees/agent-<id> for it at all; it was NOT-ALIVE while writing its transcript.
    mod._registry_says = lambda _aid: None
    r = mod.resolve(repo, "a47f4a2eae95659d1")
    check("AW1  (control) no isolation worktree and no registry record -> NOT-ALIVE",
          r["verdict"] == mod.NOT_ALIVE, r["verdict"] + ": " + r["reason"])
    mod._registry_says = lambda _aid: {"finished": False, "paused": False, "why": "its run has not ended",
                                       "name": "echo-opus-ff00122b4923", "key": "k"}
    r = mod.resolve(repo, "a47f4a2eae95659d1")
    check("AW2  no isolation worktree + registry says RUNNING (an app worker) -> ALIVE",
          r["verdict"] == mod.ALIVE and "echo-opus-ff00122b4923" in r["reason"], r["verdict"] + ": " + r["reason"])
    mod._registry_says = lambda _aid: {"finished": True, "paused": False, "why": "run ended",
                                       "name": "echo-opus-ff00122b4923", "key": "k"}
    r = mod.resolve(repo, "a47f4a2eae95659d1")
    check("AW3  no isolation worktree + registry says FINISHED -> NOT-ALIVE",
          r["verdict"] == mod.NOT_ALIVE, r["verdict"] + ": " + r["reason"])
finally:
    shutil.rmtree(scratch, ignore_errors=True)

print("=== agent-liveness: %s ===" % ("%d FAILED, %d passed" % (FAIL, PASS) if FAIL else "all %d passed" % PASS))
sys.exit(1 if FAIL else 0)

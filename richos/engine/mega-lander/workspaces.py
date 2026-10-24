#!/usr/bin/env python3
"""workspaces.py — THE CEO'S WORKSPACE SPEC, AND NOTHING ELSE.

The only reference is docs/plans/worktree-spec-2026-09-11.md (fourteen
points). Every rule in this file names the point it implements. Nothing here
asks whether an agent is alive: an agent is FINISHED when a recorded fact says
so (point 11, point 12), and a workspace is deleted when its work is LANDED or
DISCARDED (points 4, 7) — by this file, and by nothing else in the engine.

THE TWO EVENTS (point 3)
    1. spawned   -> every workspace registered   (register_cc, register_spawn,
                                                   bind_agent, record_start)
    2. landed or discarded -> every workspace and branch deleted, as one
                                                  (land, discard; retry)

STATE — outside every repository and session, one JSON file per agent:
    $RICHOS_WORKSPACES_DIR, else $CLAUDE_CONFIG_DIR/state/workspaces,
    else ~/.claude/state/workspaces
      sessions/<session_id>.json   every session records itself (point 12)
      agents/<key>.json            one agent: its workspaces and its facts
      refs/<key>/<call>.json       the refs its repositories held at the START
                                   of ONE of its tool calls, keyed by that
                                   call's own id: its calls overlap, so there
                                   is one window per call and never one slot
                                   per agent (point 3)
      done/<key>.json              an agent whose deletion completed
      ids/<agent_id>               platform agent id -> key
      repos.json                   every repository a record has named
      integration.json             the branch each repository's work integrates
                                   on, RECORDED (point 14). Never inferred,
                                   never derived from what a checkout happens
                                   to be on, and never frozen onto an agent:
                                   the live record is the only answer, which is
                                   why a wrong one can always be corrected
      events.jsonl                 append-only history of every fact
      lock                         one flock for every mutation

WHAT IS RECORDED, AND BY WHOM (never inferred):
    registration   create-teammate-worktree.sh (cc/), PreToolUse[Agent],
                   PostToolUse[Agent] and SubagentStart (native)
    ref creation   the catch-all PreToolUse (barrier -> snapshot_refs) and the
                   catch-all PostToolUse (observe -> observe_created_refs):
                   a ref that appears BETWEEN the two halves of one of the
                   agent's own tool calls, and carries that agent's own
                   unlanded work, was created by it        (point 3, point 10)
    end of run     SubagentStop, and a successful TaskStop  (point 11)
                   a run the platform STOPPED gets neither: it is recorded in
                   the platform's own per-agent record beside the transcript
                   (`stoppedByUser`), which observe_platform_end reads (11)
    handed in      TaskCompleted                            (point 11, hole 7)
    pause/resume   a SendMessage carrying `pause-until: <what ends it>`, a later
                   message to a paused agent, or `workspaces.sh pause|resume`
    session end    SessionEnd, or the operating system saying the recorded
                   process is gone or is a different process  (point 12)
"""

import calendar
import errno
import fcntl
import glob
import hashlib
import json
import os
import re
import shlex
import shutil
import signal
import subprocess
import sys
import threading
import time

SPEC = "docs/plans/worktree-spec-2026-09-11.md"
CC_PREFIX = "cc/"
CODEX_PREFIX = "codex/"
NATIVE_BRANCH_PREFIX = "worktree-agent-"
NATIVE_DIR_MARK = os.sep + os.path.join(".claude", "worktrees", "agent-")
AGENT_ID_RE = re.compile(r"^[A-Za-z0-9_-]{6,80}$")
NAME_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,80}$")

# Point 13: the retry schedule. The first retry is a minute out and each wait
# doubles, capped at an hour. The CEO hears about it only if it keeps failing:
# after RETRY_TELL_CEO_AFTER failed attempts the notice names him.
RETRY_BASE_SECONDS = float(os.environ.get("RICHOS_WORKSPACES_RETRY_BASE", "60"))
RETRY_CAP_SECONDS = 3600.0
RETRY_TELL_CEO_AFTER = int(os.environ.get("RICHOS_WORKSPACES_RETRY_TELL_CEO", "5"))
PROCESS_STOP_GRACE = float(os.environ.get("RICHOS_WORKSPACES_STOP_GRACE", "3"))
# How long a SIGKILLed process may take to be gone before stop_processes calls
# it a survivor. Not a grace (nothing is waited for that a process decides):
# only an unkillable process, stuck in the kernel, ever reaches it.
PROCESS_KILL_WAIT = 10.0
# How long a spawn takes to register what it created (see _younger_than_its_spawn).
SPAWN_WINDOW = float(os.environ.get("RICHOS_WORKSPACES_SPAWN_WINDOW", "120"))

# THE POINT-5 GATE RUNS INSIDE SOMEBODY ELSE'S TIMEOUT. The Stop hook that
# carries it is registered with 60 s and the spawn gate with 10 s
# (hooks/hooks.json), while the work grows with the pending backlog: every
# pending item re-walks its workspace's ignored files against the main
# checkout. The platform CANCELS a hook that reaches its timeout and DISCARDS
# its output, so an overrun gate lets the turn end, or the spawn through,
# having decided NOTHING and announced NOTHING — the one outcome point 5 calls
# impossible ("a guarantee, not a habit").
#
# These are not performance targets and nothing here was made faster. They buy
# one property: the gate's ANSWER never depends on finishing an unbounded scan.
# The direction makes that free — work that cannot be checked cannot be
# auto-landed, so it stays pending, and the safe answer is also the cheap one.
GATE_STOP_BUDGET = 20.0
GATE_SPAWN_BUDGET = 4.0


class SpecError(Exception):
    """A refusal the page requires. The message is shown to the operator."""


class Deadline(SpecError):
    """A gate ran out of the budget it has inside somebody else's hook timeout.
    It is a SpecError so every `except SpecError` that means "this item cannot
    be landed right now" already handles it: an item that could not be checked
    is an item that stays pending, which is the answer the page wants anyway."""


# ---------------------------------------------------------------------------
# small utilities
# ---------------------------------------------------------------------------

def now():
    return time.time()


def iso(t=None):
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(now() if t is None else t))


def realpath(p):
    p = (p or "").strip()
    if not p:
        return ""
    return os.path.realpath(os.path.expanduser(p))


def state_dir():
    d = (os.environ.get("RICHOS_WORKSPACES_DIR") or "").strip()
    if d:
        return d
    base = (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() \
        or os.path.join(os.path.expanduser("~"), ".claude")
    return os.path.join(base, "state", "workspaces")


def _p(*parts):
    return os.path.join(state_dir(), *parts)


def _ensure_dirs():
    for sub in ("sessions", "agents", "done", "ids", "refs"):
        os.makedirs(_p(sub), exist_ok=True)


def _key_segment(s):
    s = re.sub(r"[^A-Za-z0-9._-]+", "_", s or "")
    return s[:120] or "_"


def write_json(path, obj):
    """Durable write: temp file, fsync, rename, fsync the directory."""
    d = os.path.dirname(path)
    os.makedirs(d, exist_ok=True)
    tmp = "%s.tmp.%d" % (path, os.getpid())
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(obj, f, indent=1, sort_keys=True)
        f.write("\n")
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, path)
    try:
        fd = os.open(d, os.O_RDONLY)
        try:
            os.fsync(fd)
        finally:
            os.close(fd)
    except OSError:
        pass


def read_json(path):
    try:
        with open(path, encoding="utf-8") as f:
            v = json.load(f)
        return v if isinstance(v, dict) else None
    except (OSError, ValueError):
        return None


class Lock(object):
    """One flock over every mutation. Kernel-released: never stranded.
    Re-entrant inside one process, so a nested caller never deadlocks on it."""

    _depth = 0
    _fd = None

    def __init__(self, timeout=30.0):
        self.timeout = timeout

    def __enter__(self):
        if Lock._depth:
            Lock._depth += 1
            return self
        _ensure_dirs()
        fd = os.open(_p("lock"), os.O_RDWR | os.O_CREAT, 0o600)
        deadline = now() + self.timeout
        while True:
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except OSError as e:
                if e.errno not in (errno.EAGAIN, errno.EACCES):
                    os.close(fd)
                    raise
                if now() >= deadline:
                    os.close(fd)
                    raise SpecError("the workspace registry lock %s was not free within %.0fs"
                                    % (_p("lock"), self.timeout))
                time.sleep(0.05)
        Lock._fd = fd
        Lock._depth = 1
        return self

    def __exit__(self, *a):
        Lock._depth -= 1
        if Lock._depth == 0:
            try:
                fcntl.flock(Lock._fd, fcntl.LOCK_UN)
            finally:
                os.close(Lock._fd)
                Lock._fd = None


def event(what, **fields):
    """Append one fact to events.jsonl. The history, never the authority."""
    rec = {"ts": iso(), "event": what}
    rec.update(fields)
    try:
        _ensure_dirs()
        with open(_p("events.jsonl"), "a", encoding="utf-8") as f:
            f.write(json.dumps(rec, sort_keys=True) + "\n")
    except OSError:
        pass


# ---------------------------------------------------------------------------
# git — read from git, never assembled from path arithmetic
# ---------------------------------------------------------------------------

def _git_env():
    env = dict(os.environ)
    for k in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR"):
        env.pop(k, None)
    env["LC_ALL"] = "C"
    return env


# The branch a land moves, told to the merge gate (richos autocheck.py reads it). A merge into a
# branch the main checkout is not on runs in a temporary checkout of that branch, where the gate
# saw no main and ran the commit check instead of the land check (review
# rv-20261013T095956Z-f4ae06df-2813, finding 1; 2026-10-18: two app suites broken on v1.2.1 were
# found only when v1.2.1 was merged into main). A land onto a release branch runs the check a
# land onto main runs.
LAND_TARGET_ENV = "RICHOS_AUTOCHECK_LAND_TARGET"
# The author's saved proof runs, offered to the merge gate as evidence to validate, never as a
# verdict (richos autocheck.py AUTHOR_RUNS; the runs come from the handover reports,
# attach_proof_reports).
AUTHOR_RUNS_ENV = "RICHOS_AUTOCHECK_AUTHOR_RUNS"


def _land_env(target, author_runs=()):
    env = _git_env()
    # THE GATE RUNS IN THE CALLER'S LOCALE (verification reliability PRD step 3, A10/R8).
    # _git_env's LC_ALL=C is for reading git's own output in this tool; this merge's output is
    # only shown, never parsed, and its pre-merge-commit hook runs every check of the gate in
    # the environment it gets. So a managed land ran its checks under LC_ALL=C while a plain-Git
    # land, the nightly (which passes the operator's locale through) and the author's handover
    # run did not: every UI suite binds LC_ALL, and no author pass could ever match the managed
    # land's identity. The merge passes the caller's locale on, as `git merge` from a shell does.
    if "LC_ALL" in os.environ:
        env["LC_ALL"] = os.environ["LC_ALL"]
    else:
        env.pop("LC_ALL", None)
    env[LAND_TARGET_ENV] = target
    if author_runs:
        env[AUTHOR_RUNS_ENV] = os.pathsep.join(author_runs)
    else:
        env.pop(AUTHOR_RUNS_ENV, None)
    return env


def _author_runs(chain, tip):
    """The saved proof runs this work's handover reports point at for exactly commit `tip`.

    Only pointers: the gate hands them to the runner, which re-reads every input identity on the
    tree being landed and reuses only what still matches (proof_evidence.reuse)."""
    runs = []
    for rec in chain:
        for w in live_workspaces(rec):
            report = w.get("proof_report") or {}
            if report.get("run") and report.get("head") == tip and report["run"] not in runs:
                runs.append(report["run"])
    return runs


def git(repo, *args, **kw):
    try:
        r = subprocess.run(["git", "-C", repo] + list(args), capture_output=True, text=True,
                           timeout=kw.get("timeout", 60), env=_git_env())
    except subprocess.TimeoutExpired as e:
        # 124, not 127: a caller running under a budget must be able to tell
        # "this did not finish in the time it had" from "git could not run".
        return 124, "", str(e)
    except OSError as e:
        return 127, "", str(e)
    return r.returncode, r.stdout, r.stderr


def main_checkout(path):
    """The repository's MAIN checkout: the first entry of git's own list."""
    path = realpath(path)
    if not path or not os.path.isdir(path):
        return ""
    rc, out, _ = git(path, "worktree", "list", "--porcelain")
    if rc != 0:
        return ""
    for line in out.splitlines():
        if line.startswith("worktree "):
            return realpath(line[len("worktree "):])
    return ""


def worktree_list(repo):
    """[{path, branch, head, detached, locked, prunable}] or None if unreadable."""
    rc, out, _ = git(repo, "worktree", "list", "--porcelain")
    if rc != 0:
        return None
    items, cur = [], None
    for line in out.splitlines() + [""]:
        if line.startswith("worktree "):
            cur = {"path": realpath(line[len("worktree "):]), "branch": "", "head": "",
                   "detached": False, "locked": False, "prunable": False}
        elif cur is None:
            continue
        elif line.startswith("branch refs/heads/"):
            cur["branch"] = line[len("branch refs/heads/"):]
        elif line.startswith("HEAD "):
            cur["head"] = line[5:].strip()
        elif line == "detached":
            cur["detached"] = True
        elif line.startswith("locked"):
            cur["locked"] = True
        elif line.startswith("prunable"):
            cur["prunable"] = True
        elif line == "" and cur is not None:
            items.append(cur)
            cur = None
    return items


def branch_tip(repo, branch):
    rc, out, _ = git(repo, "rev-parse", "--verify", "--quiet", "refs/heads/" + branch)
    return out.strip() if rc == 0 else ""


def branch_tip_read(repo, branch):
    """(tip, "") for a branch that exists, ("", "") for one git positively
    reports absent, and ("", why) when the answer could not be read.

    `branch_tip` answers "" for both of the last two. That is fine where a
    missing branch and an unreadable one lead to the same action, and wrong
    wherever the answer is used as PROOF: a failed read says nothing about
    whether a commit reached the integration branch, and nothing about whether
    a branch is gone (hunt part 4, finding 4). A missing branch is normal after
    a partial cleanup, so absence stays an answer; a read that failed is not
    one. `for-each-ref` is used because it separates the two where `rev-parse
    --verify --quiet` cannot: measured on this machine, both exit as if absent
    for a loose ref git cannot open, and only `for-each-ref` says so, on
    stderr ("ignoring broken ref")."""
    ref = "refs/heads/" + branch
    rc, out, err = git(repo, "for-each-ref", "--format=%(refname)%00%(objectname)", ref)
    if rc != 0:
        return "", "%s in %s could not be read (git exit %d: %s)" % (ref, repo, rc, err.strip()[:200])
    for line in out.splitlines():
        name, _sep, sha = line.partition("\0")
        if name == ref and sha.strip():
            return sha.strip(), ""
    if err.strip():
        return "", "%s in %s could not be read: %s" % (ref, repo, err.strip()[:200])
    return "", ""


def is_ancestor(repo, a, b):
    rc, _, _ = git(repo, "merge-base", "--is-ancestor", a, b)
    return rc == 0


def local_branches(repo, patterns):
    rc, out, _ = git(repo, "for-each-ref", "--format=%(refname:short)", *["refs/heads/" + p for p in patterns])
    if rc != 0:
        return None
    return [b for b in out.splitlines() if b.strip()]


def is_native_path(path):
    return NATIVE_DIR_MARK in (path or "")


def classify(path, branch):
    """'native', 'cc', 'codex' or '' (not the system's concern — point 1)."""
    if (branch or "").startswith(CODEX_PREFIX):
        return "codex"
    if is_native_path(path) or (branch or "").startswith(NATIVE_BRANCH_PREFIX):
        return "native"
    if (branch or "").startswith(CC_PREFIX):
        return "cc"
    return ""


# ---------------------------------------------------------------------------
# THE LAND LOCK'S HOME, AND THE APPEND-ONLY LAND RECORD BESIDE IT
# ---------------------------------------------------------------------------
# MOVED HERE FROM app.py ON 2026-09-17, AND THE LAYERING IS THE WHOLE REASON.
# `_restore_protected_refs` (far below) has to answer "whose land moved this
# ref?", and the answer is written by app.py's `integrate` — which imports THIS
# file as `W`. The import cannot go back the other way. So either the keying
# rule for the lock's path exists in two files, or it exists in the layer that
# already owns machine-wide state. Two copies of a keying rule is the defect
# class this subsystem keeps meeting: the copies agree until one is edited, and
# then two conversations take two locks while believing they share one — which
# is the exact before-state the land lock was written for (app.py, "TWO
# CONVERSATIONS, TWO LOCK FILES"). app.py now calls these four functions and
# declares no keying of its own.
#
# THE ONE CHANGE MADE WHILE MOVING: pathlib is gone. This module is imported by
# a PreToolUse and a PostToolUse hook on EVERY tool call of every agent, and it
# uses os.path throughout; these functions return strings for the same reason
# every other path in this file does.

def land_locks_dir():
    """The machine-wide home of the per-repository land locks and land records.

    Deliberately NOT `state_dir()` and NOT app.py's `state()`: those are the two
    partitions this lock exists to sit outside of. It is derived instead from
    `CLAUDE_CONFIG_DIR` (else `~/.claude`) — the same machine-wide home the
    workspace registry falls back to and the worktree ledger lives in — because
    the app neither sets nor removes that variable for the engine it launches:
    `EngineProfile::configure` strips every inherited `RICHOS_`/`LORO_`/`ECS_`/
    `GIT_` name and re-adds its own list, and `CLAUDE_CONFIG_DIR` is in neither
    (engine_profile.rs:156-210; the app reads it only to FIND the engine,
    setup.rs:146 and engine.rs:95). One value for every thread of one app, and
    for a terminal beside it.

    `RICHOS_LAND_LOCKS_DIR` overrides it for tests. That override cannot reach
    an app-launched engine at all, because the strip above removes it — and so
    that the property is CHECKED rather than argued from that, a home resolving
    inside either partition is REFUSED rather than used. A lock inside a
    partition is not a lock; it is the defect this function was written for,
    wearing the new name."""
    override = (os.environ.get("RICHOS_LAND_LOCKS_DIR") or "").strip()
    if override:
        base = realpath(override)
    else:
        home = (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() \
            or os.path.join(os.path.expanduser("~"), ".claude")
        base = os.path.join(realpath(home), "state", "land-locks")
    if not os.path.isabs(base):
        raise ValueError("the land lock home must be an absolute path")
    for name in ("RICHOS_APP_STATE", "RICHOS_WORKSPACES_DIR"):
        partition = realpath(os.environ.get(name) or "")
        if not partition:
            continue
        if base == partition or base.startswith(partition.rstrip(os.sep) + os.sep):
            raise ValueError("the land lock cannot live inside a per-conversation partition (%s); "
                             "a lock two conversations cannot share serializes nothing" % name)
    os.makedirs(base, mode=0o700, exist_ok=True)
    if os.path.islink(base):
        raise ValueError("the land lock home cannot be redirected")
    return base


def canonical_repository(repo):
    """The identity of the ref store a land mutates, not the string it was
    reached by.

    `--git-common-dir`, made absolute and then realpath'd: two symlinked paths
    to one repository resolve to one value, and so do a repository and a linked
    worktree of it — which share a ref store and would otherwise race on the
    same branch through two different lock files. If git cannot answer (the
    path is gone, it is not a repository) the realpath of the given path is the
    key, so this never turns into a refusal `integrate` did not already make."""
    rc, out, _err = git(str(repo), "rev-parse", "--path-format=absolute", "--git-common-dir")
    common = out.strip() if rc == 0 else ""
    return os.path.realpath(os.path.expanduser(common or str(repo)))


def land_lock_path(repo):
    """One file per repository. The digest is the discriminator; the readable
    prefix is there so that an operator listing the directory sees repositories
    rather than hashes."""
    canonical = canonical_repository(repo)
    name = os.path.basename(canonical.rstrip("/"))
    if name in (".git", ""):
        name = os.path.basename(os.path.dirname(canonical.rstrip("/")))
    label = re.sub(r"[^A-Za-z0-9._-]+", "-", name).strip("-")[:48] or "repository"
    return os.path.join(land_locks_dir(),
                        "%s-%s.lock" % (label, hashlib.sha256(canonical.encode()).hexdigest()[:16]))


def read_land_lock(path):
    """The holder's record out of a lock file, or None for anything that is not
    a complete JSON object — `_write_land_lock` writes in place and a reader can
    catch a partial line."""
    try:
        with open(path, encoding="utf-8") as handle:
            value = json.loads(handle.read(8192))
        return value if isinstance(value, dict) else None
    except (OSError, ValueError):
        return None


# A LAND RECORD IS APPENDED, NEVER OVERWRITTEN, and these are the bounds of the
# file that results. 500 lands into ONE repository is far beyond any window an
# agent's snapshot can still be open across (a snapshot belongs to a single tool
# call, or at the widest a single agent run), so the trim can only ever drop
# rows nothing will ask about. The line bound is the same 4000 the lock record
# carries, for the same reason: a bounded write cannot be half-read.
LAND_RECORD_KEEP = 500
LAND_RECORD_LINE = 4000


def land_record_path(repo):
    """The append-only history of lands in this repository, BESIDE its lock.

    A history and not a field on the lock, and that difference is the whole of
    attribution working versus saying "unknown". The lock file holds ONE
    record, rewritten in place by whoever takes the lock next
    (`_write_land_lock`), so of two lands in a row only the second survives —
    and an agent whose snapshot predates the FIRST one would see a move that
    nothing could name. Appending keeps both, and the match below is by COMMIT,
    so each land answers for exactly the move it made."""
    lock = land_lock_path(repo)
    return (lock[:-len(".lock")] if lock.endswith(".lock") else lock) + ".lands"


def append_land_record(repo, record):
    """Write one land into this repository's history. Append-only.

    `O_APPEND` and one bounded JSON line: every write lands at the end of the
    file, and the caller is holding this repository's land lock anyway, so the
    order is the order the lands happened in.

    A HISTORY IS NEVER WORTH FAILING A LAND FOR. Anything that goes wrong here
    returns "" and the land carries on: the cost of a missing row is that one
    later move is reported as unattributed, which is a worse message and never
    a wrong one."""
    try:
        path = land_record_path(repo)
        line = json.dumps(record, sort_keys=True)[:LAND_RECORD_LINE] + "\n"
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_APPEND | os.O_NOFOLLOW, 0o600)
        try:
            os.write(fd, line.encode("utf-8"))
        finally:
            os.close(fd)
        _trim_land_records(path)
        return path
    except (OSError, ValueError, TypeError):
        return ""


def _trim_land_records(path):
    """Keep the newest LAND_RECORD_KEEP rows. Runs under the land lock, and
    replaces the file rather than truncating it, so a reader that opened the
    old inode still reads a whole file rather than a half of one."""
    try:
        with open(path, encoding="utf-8") as handle:
            lines = handle.readlines()
        if len(lines) <= LAND_RECORD_KEEP * 2:
            return
        tmp = "%s.tmp.%d" % (path, os.getpid())
        with open(tmp, "w", encoding="utf-8") as handle:
            handle.writelines(lines[-LAND_RECORD_KEEP:])
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(tmp, path)
    except OSError:
        pass


def land_records(repo):
    """Every land recorded for this repository, oldest first.

    An empty list is NOT "no land happened": the terminal path lands with a
    hand-run `git merge` and writes nothing here. Telling those two apart is
    exactly what `land_by_another_conversation` needs, so absence is reported
    as absence and never as a land by nobody."""
    try:
        path = land_record_path(repo)
        with open(path, encoding="utf-8") as handle:
            raw = handle.readlines()[-(LAND_RECORD_KEEP * 2):]
    except (OSError, ValueError):
        return []
    out = []
    for line in raw:
        line = line.strip()
        if not line:
            continue
        try:
            value = json.loads(line)
        except ValueError:
            continue                    # a torn or foreign line names no land
        if isinstance(value, dict):
            out.append(value)
    return out


def this_conversation():
    """The conversation thread this engine was launched for, or "" in a
    terminal. Set by the app and stripped for anything else
    (engine_profile.rs:168-170), so it is the app's word rather than a guess."""
    return (os.environ.get("RICHOS_APP_THREAD") or "").strip()


def land_by_another_conversation(repo, branch, before, found):
    """WHOSE LAND MOVED THIS REF FORWARD? — the sentence to report, or None for
    silence. Consulted only where the caller has already established that a
    protected ref moved to a DESCENDANT: a fast-forward, nothing lost, the
    shape of an ordinary land.

    Until 2026-09-17 every one of those was silent. That is right for the
    lead's own land and wrong for another conversation's: the CEO runs two
    threads, their back ends share a repository, and the second one's land
    moves `main` under the first one's agents. Their snapshots, their bases and
    anything they recorded from the old tip are stale from that instant, with
    nothing saying so. The lock makes the two lands take turns; this makes the
    turn VISIBLE to the agent it happened under.

    SILENT WHEN NO RECORD NAMES THE MOVE. A land by another conversation is
    exactly the thing that writes a record (app.py `integrate`, under the land
    lock, right after the fast-forward), so a move that NO record names is the
    terminal path — Rich's lands there are a hand-run `git merge` that writes
    no record, and those teammates are already told by guard-inflight-notify.sh
    at the push. Calling every one of those "unattributed" would put an alarm on
    the single most common write a recorded branch ever receives, which is how a
    report becomes wallpaper. That was true of a repository with NO land file,
    and it is exactly as true of one whose file holds an older land: until
    2026-10-01 the first recorded land switched every later hand merge from
    silence to "moved forward by a land this repository's land records do not
    name", once per running agent per merge (hunt part 4, finding 24). The
    presence of the file says which conversation landed only when a record
    names the commit; it says nothing about a move it does not name.

    SILENT FOR THIS CONVERSATION'S OWN LAND. A record carrying this engine's
    own `RICHOS_APP_THREAD` is this conversation landing its own work; its
    agents are the one party that already knows.

    IT NEVER CLAIMS MORE THAN THE RECORD CARRIES. Only a record that names the
    branch and the commit produces a sentence, and it names that record's
    thread; a move without one produces none."""
    try:
        records = land_records(repo)
        mine = this_conversation()
        for record in reversed(records):
            if record.get("branch") != branch or record.get("commit") != found:
                continue
            thread = str(record.get("thread_id") or "")
            if thread == mine:
                return None             # this conversation's own land
            return "landed by conversation %s at %s, %s -> %s" % (
                thread or "an unnamed thread", record.get("at") or "an unrecorded time",
                (before or "")[:12], (found or "")[:12])
        return None                     # no record names this move: a hand merge at a terminal
    except (OSError, ValueError):
        return None


# ---------------------------------------------------------------------------
# point 12 — sessions record themselves; ended is read from the OS
# ---------------------------------------------------------------------------

def _ps_env():
    return dict(os.environ, LC_ALL="C", LANG="C", TZ="UTC0")


def process_start(pid, timeout=10):
    """('ok', '<lstart>') | ('gone', '') | ('unknown', why). Read from the OS.

    The start time is what makes the record more than a process number
    (point 12: "since those get reused")."""
    try:
        pid = int(pid)
    except (TypeError, ValueError):
        return "unknown", "no process number recorded"
    if pid <= 0:
        return "unknown", "no process number recorded"
    if not shutil.which("ps"):
        return "unknown", "ps is not available"
    try:
        r = subprocess.run(["ps", "-o", "lstart=", "-p", str(pid)], capture_output=True,
                           text=True, timeout=timeout, env=_ps_env())
    except (OSError, subprocess.TimeoutExpired) as e:
        return "unknown", "ps could not run: %s" % e
    text = " ".join(r.stdout.split())
    if r.returncode == 0 and text:
        return "ok", text
    if r.returncode in (0, 1) and not text:
        return "gone", ""
    return "unknown", "ps exited %d: %s" % (r.returncode, r.stderr.strip()[:200])


def _ps_parent_and_comm(pid, timeout=10):
    try:
        r = subprocess.run(["ps", "-o", "ppid=,comm=", "-p", str(pid)], capture_output=True,
                           text=True, timeout=timeout, env=_ps_env())
    except (OSError, subprocess.TimeoutExpired):
        return None, ""
    parts = r.stdout.split(None, 1)
    if len(parts) < 2:
        return None, ""
    try:
        return int(parts[0]), parts[1].strip()
    except ValueError:
        return None, ""


def _platform_sessions_dir():
    return (os.environ.get("RICHOS_SESSIONS_DIR") or "").strip() or os.path.join(
        (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() or os.path.join(os.path.expanduser("~"), ".claude"),
        "sessions")


def session_pid(session_id=""):
    """The pid of the claude process that owns this hook or command.

    RICHOS_SESSION_PID overrides (tests, and a harness that exports it).
    Otherwise walk the process ancestry, from the operating system: the first
    ancestor that the platform's own session file names as this session, else
    the first ancestor whose command is `claude`."""
    v = (os.environ.get("RICHOS_SESSION_PID") or "").strip()
    if v.isdigit():
        return int(v)
    pid = os.getppid()
    named = None
    for _ in range(16):
        if pid is None or pid <= 1:
            break
        f = os.path.join(_platform_sessions_dir(), "%d.json" % pid)
        rec = read_json(f)
        if rec:
            if not session_id or str(rec.get("sessionId") or "") == session_id:
                return pid
            # The platform says this process is ANOTHER session. The payload's
            # session is therefore not the one running this code, and its
            # identity is not this process's to lend.
            return None
        ppid, comm = _ps_parent_and_comm(pid)
        if named is None and os.path.basename(comm) == "claude":
            named = pid
        pid = ppid
    return named


def session_path(session_id):
    return _p("sessions", _key_segment(session_id) + ".json")


def load_session(session_id):
    return read_json(session_path(session_id)) if session_id else None


def identity_for(session_id):
    """{pid, pid_start} of the process running this session, read from the OS."""
    pid = session_pid(session_id)
    if not pid:
        return None
    st, text = process_start(pid)
    if st != "ok":
        return None
    return {"pid": pid, "pid_start": text}


def worktree_session_reason(cwd, pid):
    """Point 3: nobody starts a session in its own workspace. Returns why, or ''."""
    top = ""
    if cwd and os.path.isdir(cwd):
        rc, out, _ = git(cwd, "rev-parse", "--show-toplevel")
        top = realpath(out.strip()) if rc == 0 else ""
    if top and (os.sep + os.path.join(".claude", "worktrees") + os.sep) in top + os.sep:
        return "the session's directory %s is a Claude Code workspace (claude --worktree)" % top
    if pid:
        try:
            r = subprocess.run(["ps", "-o", "args=", "-p", str(pid)], capture_output=True, text=True,
                               timeout=10, env=_ps_env())
            args = " " + r.stdout.strip() + " "
        except (OSError, subprocess.TimeoutExpired):
            args = ""
        if re.search(r"\s(--worktree|-w)(\s|=)", args) and "claude" in args:
            return "the session was started with claude --worktree / -w"
    return ""


def platform_session(session_id, pid=None):
    """The platform's OWN record of the claude process running `session_id`
    (<config>/sessions/<pid>.json), or None. Read, never written.

    It is written once, when the process starts, and its `cwd` is the directory
    the session was LAUNCHED in (Claude Code 2.1.280 writes `cwd: originalCwd`).
    That is the one fact point 3 asks about, and no hook payload carries it
    reliably: see session_start."""
    if not session_id:
        return None
    pids = []
    for p in (pid, None):
        if p is None:
            p = session_pid(session_id)
        try:
            p = int(p)
        except (TypeError, ValueError):
            continue
        if p > 0 and p not in pids:
            pids.append(p)
    for p in pids:
        rec = read_json(os.path.join(_platform_sessions_dir(), "%d.json" % p))
        if rec and str(rec.get("sessionId") or "") == session_id:
            return rec
    return None


# The SessionStart sources that mean a PROCESS was launched. The others
# (compact, clear) happen inside a running session, and one of them, compact,
# also fires for a SUBAGENT's compaction carrying the lead's session_id.
LAUNCH_SOURCES = ("", "startup", "resume", "fork")


def session_start(session_id, cwd, source="", agent_id=""):
    """Record a SessionStart. Returns (record, foreign).

    `foreign` is True when this SessionStart is NOT the lead's own: it must
    change nothing about the lead and give it no context.

    WHY THE PAYLOAD'S cwd IS NOT THE LEAD'S DIRECTORY. On 2026-09-22 at 23:10Z a
    subagent (isaac-opus-n1) compacted its context. Claude Code fires
    SessionStart(source=compact) for a subagent's compaction too, and 2.1.280
    builds that input WITHOUT the agent context: the LEAD's session_id, the
    SUBAGENT's worktree as cwd, no agent_id. This function used to take every
    SessionStart as the lead's, so it wrote the subagent's worktree onto the
    lead's record, point 3 flagged it FORBIDDEN, and every lead call was refused
    for the 33 minutes until the CEO exited, with ten agents at 100% CPU and
    nothing able to stop them.

    So the lead's directory is, in order: the platform's own record of the
    process (platform_session), else the first record, else a launch event's
    cwd. A later in-session SessionStart never moves it, and a SessionStart
    carrying an agent_id (a later platform version may send one) is never the
    lead's at all."""
    if not session_id:
        raise SpecError("no session id in the payload")
    source = (source or "").strip()
    payload_cwd = realpath(cwd)
    prev = load_session(session_id) or {}
    if agent_id:
        event("session-start-not-the-lead", session_id=session_id, agent_id=agent_id, source=source or None,
              cwd=payload_cwd or None)
        return prev, True
    ident = identity_for(session_id)
    pid = ident["pid"] if ident else prev.get("pid")
    plat = platform_session(session_id, pid)
    if plat and plat.get("cwd"):
        lead_cwd, basis = realpath(str(plat["cwd"])), "platform"
    elif prev.get("cwd"):
        lead_cwd, basis = prev["cwd"], prev.get("cwd_basis") or "recorded"
    else:
        lead_cwd, basis = payload_cwd, "first-record"
    foreign = bool(payload_cwd and lead_cwd and payload_cwd != lead_cwd and source not in LAUNCH_SOURCES)
    if foreign:
        event("session-start-not-the-lead", session_id=session_id, source=source or None,
              cwd=payload_cwd, lead_cwd=lead_cwd)
        return prev, True
    # A compact or clear with nothing before it cannot say whose directory it
    # carries, so it is recorded and never made the reason for a refusal.
    may_forbid = basis != "first-record" or source in LAUNCH_SOURCES
    rec = dict(prev)
    rec.update({"session_id": session_id, "cwd": lead_cwd, "cwd_basis": basis,
                "started_at": rec.get("started_at") or iso(),
                "repo": main_checkout(lead_cwd) if lead_cwd else ""})
    if ident:
        rec.update(ident)
    rec.pop("ended_at", None)
    rec.pop("end_reason", None)
    forbidden = worktree_session_reason(lead_cwd, pid) if may_forbid else ""
    if forbidden:
        rec["forbidden"] = forbidden
    else:
        rec.pop("forbidden", None)
    with Lock():
        write_json(session_path(session_id), rec)
        if rec.get("repo"):
            _remember_repo(rec["repo"])
    event("session-start", session_id=session_id, pid=rec.get("pid"), pid_start=rec.get("pid_start"),
          source=source or None, basis=basis, forbidden=forbidden or None)
    return rec, False


def record_session_start(session_id, cwd, source="", agent_id=""):
    return session_start(session_id, cwd, source, agent_id)[0]


def forbidden_now(session_id, rec):
    """Point 3's verdict for the lead, RE-DERIVED rather than trusted.

    A stored flag is a claim with a date on it. The record 9cfd9fc9 was left
    with carried the wrong directory and FORBIDDEN, and nothing could ever
    correct it. So when a flag is stored, the platform's own launch record is
    asked again; where the two disagree the platform wins and the record is
    healed. Only a stored flag costs a lookup: the lead's ordinary call stays one
    file read."""
    stored = (rec or {}).get("forbidden") or ""
    if not stored:
        return ""
    plat = platform_session(session_id, rec.get("pid"))
    if not plat or not plat.get("cwd"):
        return stored
    lead_cwd = realpath(str(plat["cwd"]))
    reason = worktree_session_reason(lead_cwd, rec.get("pid"))
    if reason != stored:
        with Lock():
            cur = load_session(session_id) or dict(rec)
            cur["cwd"] = lead_cwd
            cur["cwd_basis"] = "platform"
            if reason:
                cur["forbidden"] = reason
            else:
                cur.pop("forbidden", None)
            write_json(session_path(session_id), cur)
        event("session-forbidden-rederived", session_id=session_id, was=stored, now=reason or None)
    return reason


def record_session_end(session_id, reason=""):
    with Lock():
        rec = load_session(session_id) or {"session_id": session_id}
        rec["ended_at"] = iso()
        rec["end_reason"] = reason or "SessionEnd"
        write_json(session_path(session_id), rec)
    event("session-end", session_id=session_id, reason=reason)


def session_state(session_id, fallback=None, cache=None):
    """('running'|'ended'|'unknown', why). Point 12, literally:
    ended when it recorded its end, or when its process no longer exists on
    this machine — read from the operating system, never guessed."""
    if cache is not None and session_id in cache:
        return cache[session_id]
    rec = load_session(session_id) if session_id else None
    ident = rec if (rec and rec.get("pid")) else (fallback or {})
    if rec and rec.get("ended_at"):
        out = ("ended", "session %s recorded its end at %s" % (session_id[:8], rec["ended_at"]))
    elif not ident.get("pid") or not ident.get("pid_start"):
        out = ("unknown", "session %s has no recorded process identity" % (session_id or "?")[:8])
    else:
        st, text = process_start(ident["pid"])
        if st == "gone":
            out = ("ended", "process %s of session %s no longer exists" % (ident["pid"], session_id[:8]))
        elif st == "ok" and text != ident["pid_start"]:
            out = ("ended", "process %s is no longer session %s's (started %s, recorded %s)"
                   % (ident["pid"], session_id[:8], text, ident["pid_start"]))
        elif st == "ok":
            out = ("running", "process %s of session %s runs" % (ident["pid"], session_id[:8]))
        else:
            out = ("unknown", text)
    if cache is not None:
        cache[session_id] = out
    return out


# ---------------------------------------------------------------------------
# records
# ---------------------------------------------------------------------------

def agent_path(key):
    return _p("agents", key + ".json")


def done_path(key):
    return _p("done", key + ".json")


def load_agent(key):
    return read_json(agent_path(key)) or read_json(done_path(key))


def save_agent(rec):
    write_json(agent_path(rec["key"]), rec)
    if rec.get("agent_id"):
        _write_id(rec["agent_id"], rec["key"])


def _write_id(agent_id, key):
    os.makedirs(_p("ids"), exist_ok=True)
    tmp = _p("ids", ".%s.%d" % (agent_id, os.getpid()))
    with open(tmp, "w", encoding="utf-8") as f:
        f.write(key + "\n")
    os.replace(tmp, _p("ids", _key_segment(agent_id)))


def key_for_id(agent_id):
    try:
        with open(_p("ids", _key_segment(agent_id)), encoding="utf-8") as f:
            return f.read().strip()
    except OSError:
        return ""


def all_agents(include_done=False):
    out = []
    dirs = ["agents"] + (["done"] if include_done else [])
    for sub in dirs:
        try:
            names = sorted(os.listdir(_p(sub)))
        except OSError:
            continue
        for n in names:
            if n.endswith(".json"):
                r = read_json(_p(sub, n))
                if r and r.get("key"):
                    out.append(r)
    return out


def damaged_records(include_done=True):
    """[(path, why)] for every registry record that EXISTS and cannot be read.

    A DAMAGED RECORD IS NOT A MISSING ONE (hunt part 4 v2, V2-04). read_json
    answers None for both, so all_agents() silently drops a record whose file
    is malformed, unreadable or not a record, and the sweep then saw that
    record's live workspace as one nobody registered: an orphan, "finished
    work of an ended session", whose processes the automatic land stopped and
    whose workspace it deleted. A record that cannot be read says nothing
    about whether its worker ended. A registry directory that cannot be
    listed is damaged too; one that does not exist yet is simply empty."""
    out = []
    for sub in ["agents"] + (["done"] if include_done else []):
        try:
            names = sorted(os.listdir(_p(sub)))
        except FileNotFoundError:
            continue
        except OSError as e:
            out.append((_p(sub), "the directory cannot be listed: %s" % e))
            continue
        for n in names:
            if not n.endswith(".json"):
                continue
            path = _p(sub, n)
            try:
                with open(path, encoding="utf-8") as f:
                    v = json.load(f)
            except FileNotFoundError:
                continue                      # removed between the listing and the read
            except (OSError, ValueError) as e:
                out.append((path, "%s: %s" % (e.__class__.__name__, e)))
                continue
            if not isinstance(v, dict) or not v.get("key"):
                out.append((path, "it is not a registry record (no key)"))
            # A KEYED RECORD WITHOUT ITS WORKSPACE LIST IS DAMAGED TOO (hunt
            # part 4 v3, V3-01). Every record new_record() writes carries the
            # list, so one without it has lost the very facts that say which
            # workspaces it owns; read as "none", the sweep made its live
            # workspace an orphan and the automatic land deleted it.
            elif not isinstance(v.get("workspaces"), list) or \
                    not all(isinstance(w, dict) for w in v["workspaces"]):
                out.append((path, "its workspace list is missing or not a list of workspaces"))
    return out


def named_key(session_id, name):
    return "%s--%s" % (_key_segment(session_id), _key_segment(name))


def new_record(key, **fields):
    rec = {"key": key, "name": "", "session_id": "", "agent_id": "", "workspaces": [],
           "registered_at": iso(), "end": None, "handed_in": None, "pause": None, "waiting": None,
           "disposition": None, "deletion": None, "orphan": False, "ceo_ordered": None,
           "continues": [], "lands_pending": [], "created_branches": [],
           "history": []}
    rec.update(fields)
    return rec


def _remember_repo(repo):
    repo = realpath(repo)
    if not repo:
        return
    path = _p("repos.json")
    cur = read_json(path) or {"repos": []}
    if repo not in cur["repos"]:
        cur["repos"].append(repo)
        write_json(path, cur)


def known_repos():
    return [r for r in (read_json(_p("repos.json")) or {}).get("repos", []) if os.path.isdir(r)]


# ---------------------------------------------------------------------------
# point 14 — the branch a body of work integrates on is RECORDED
# ---------------------------------------------------------------------------
#
# "Landed doesn't always mean landed on main. Landing means merged into the
#  branch this work integrates on. Usually that is main. When the work cannot
#  reach main yet — not reviewed, or not ready to install — it is that work's
#  dev branch. ... The branch a body of work integrates on is RECORDED when
#  that work starts, before its first agent is spawned. Nothing infers it and
#  nothing guesses it — without that record there is no fact to test a land
#  against, and 'landed' goes back to meaning whatever main happens to have."
#
# WHAT THIS REPLACED, AND WHY IT WAS WRONG. land() used to resolve each
# repository's target with `git rev-parse HEAD` in the main checkout, at the
# moment of the land. So "landed" meant "in whatever the main checkout has
# checked out just now" — which is not a fact about this work at all: it moves
# when Rich checks something else out, it is empty when the main checkout is
# detached, and it can NEVER answer yes for work that integrates on a dev
# branch, which is the case the page's fourteenth point exists for. Work merged
# onto its dev branch was reported "not landed yet" and its workspaces were left
# behind, against point 4.
#
# THE RECORD IS A FILE, PER REPOSITORY, AND THERE IS EXACTLY ONE WAY INTO IT:
#
#   `workspaces.sh integration --repo <r> --branch <b> --why '<this work>'`
#
# Rich records it when a body of work starts. It is the form the page asks for,
# and the only form that can name a dev branch: nothing in this file can know
# that this work is not allowed into main yet. It is read at land time, and a
# land in a repository with NO record REFUSES and names that command — refusing
# is the only answer that is not a guess.
#
# ===========================================================================
# THERE IS NO FLOOR, AND DELETING IT IS THE POINT — 2026-09-12
# ===========================================================================
# Until 2026-09-12 a "first-registration" floor read the main checkout's own
# current branch, wrote it down as though it were the recorded fact, and froze a
# copy of it onto every agent registered afterwards; `integration_target`
# preferred that frozen copy over the live record. It fired from
# `record_session_start` — earlier even than the comment above it claimed.
#
# IT WAS WORSE THAN THE REFUSAL IT REPLACED, and the comparison that shows it was
# reproduced independently by two reviewers:
#
#   with NO record at all      the land refuses, Rich records the branch, and
#                              the land then SUCCEEDS. The refusal HEALS.
#   with the floor's record    the wrong branch is frozen onto an agent already
#                              in flight. Recording the right branch afterwards
#                              cannot reach it, its land refuses FOREVER with
#                              "is not in main", its workspace is left behind,
#                              and point 5 is blocked — against point 14's own
#                              "'it cannot go to main yet' is never a reason for
#                              anything to be left behind".
#
# A derived value that cannot be corrected is not a floor; it is a wrong answer
# with the authority of a recorded one. "Nothing infers it and nothing guesses
# it" (point 14) is the whole sentence, so the derivation is gone, the
# `source: first-registration` record is gone, and the frozen per-agent copy is
# gone with them. THE LIVE RECORD IS THE ONLY ANSWER, and because it is the only
# answer it can always be corrected.
#
# ===========================================================================
# THE RECORD IS PER BODY OF WORK, NOT PER REPOSITORY — 2026-09-12
# ===========================================================================
# Point 14 says "the branch THIS WORK integrates on". The record had one slot
# per repository, read live at land time, and the code's own docstring called it
# "the branch each REPOSITORY's work integrates" — which is a different sentence
# and the wrong one.
#
# Point 5 permits a second body of work to start in a repository while the first
# body's work is running or pending integration. With one slot per repository,
# recording the second body's branch — which point 14 requires Rich to do — MOVED THE FIRST
# BODY'S RUNNING AGENTS ONTO IT, retroactively. Their work was merged onto the
# branch they were spawned for, their land was then measured against a branch
# they had never heard of, it refused forever, and their workspaces were
# stranded. The superseded value went into a `history` list that nothing read.
#
# So a recording now names a BODY OF WORK: an id, its repository, and the branch
# it integrates on. Each repository has a CURRENT body of work, which is what a
# new agent binds to when it is registered, and that binding is by ID and never
# by value.
#
# THE INDIRECTION IS THE WHOLE DESIGN, and it is what the deleted floor got
# wrong. Freezing the BRANCH onto an agent gives an answer that cannot be
# corrected. Binding the WORK ID gives one that can: the work's branch stays
# live and correctable — correcting it corrects every agent bound to that work,
# in flight — while starting a DIFFERENT body of work creates a different id and
# cannot reach backwards into the first one.
#
#   `workspaces.sh integration --repo <r> --branch <b> --why '<this work>'`
#       starts a body of work (or re-states the current one, if the branch is
#       unchanged: recording the same branch twice is the same work).
#   `workspaces.sh integration --repo <r> --branch <b> --correct --why '<...>'`
#       CORRECTS the branch of the body of work that is current, keeping its id,
#       so every agent already bound to it moves with it.
#
# ===========================================================================
# THE SPAWN IS REFUSED, NOT THE LAND — 2026-09-12, round 7
# ===========================================================================
# Until round 7 an agent registered before anything was recorded was "bound to
# nothing, heals later": `integration_target` fell back to the repository's
# CURRENT body of work at land time. Both round-6 reviewers proved that is not
# a heal but a guess about WHICH body of work the agent belongs to
# (certification-sage-round6-2026-09-12.md §2, S1–S4;
# certification-frank-round6-2026-09-12.md §5): an agent bound to nothing was
# judged against whichever body of work happened to be current when it landed,
# so a second, unrelated recording made in between MOVED its land verdict,
# while a properly bound control agent was unmoved. "Nothing infers it and
# nothing guesses it" (point 14) is the whole sentence, and "current at land
# time" is an inference.
#
# So the page's own ordering is enforced where it says: "RECORDED when that
# work starts, BEFORE ITS FIRST AGENT IS SPAWNED." `register_cc` and
# `register_spawn` REFUSE when any repository the agent will work in has no
# current body of work, naming the recording command — point 3's shape, "if
# registration fails, the spawn does not happen". Every spawned agent is
# therefore bound at its spawn, and `integration_target` has no fallback: a
# chain bound to nothing refuses and names the command.
#
# The one kind of record that is never spawned through the guard — an
# UNREGISTERED workspace the point-3 sweep found (an orphan), or a native start
# the platform reported for a spawn no guard registered (provisional) — has no
# spawn at which to be refused. It binds at the first sweep after a body of
# work exists for its repository (`scan_unregistered`), and that binding is
# recorded on it by id and never moves afterwards; until then its land refuses
# and names the command. That is the only path left that reads "current", it
# is written down as a recorded fact the moment it is read, and it covers no
# agent this system spawned.

def _integration_path():
    return _p("integration.json")


def _norm_repo(repo):
    return main_checkout(repo) or realpath(repo)


def _integration_file():
    return read_json(_integration_path()) or {}


def _work_by_id(work_id):
    """The body of work with this id — including one that is no longer current,
    because that is precisely the one an agent in flight is still bound to."""
    if not work_id:
        return None
    return (_integration_file().get("works") or {}).get(work_id)


def integration_record(repo):
    """The recorded integration branch of a repository's CURRENT body of work, or
    None. Never inferred.

    An agent asks `integration_target`, which asks the work IT is bound to. This
    is the answer for a caller that has no agent — a consumer asking about the
    repository, or Rich asking what he last recorded."""
    return _work_by_id((_integration_file().get("current") or {}).get(_norm_repo(repo)))


def all_integration_records():
    """{repository: its current body of work} — one row per repository, the shape
    every reader of this already expects."""
    cur = _integration_file().get("current") or {}
    return dict((r, _work_by_id(w)) for r, w in cur.items() if _work_by_id(w))


def all_bodies_of_work():
    """Every body of work ever recorded, current or superseded. A superseded one
    is not history: an agent spawned for it is still bound to it and still lands
    against it, which is the whole reason this is keyed by work and not by
    repository."""
    return dict(_integration_file().get("works") or {})


def _new_work_id(cur, main):
    n = 1
    base = _key_segment(os.path.basename(main.rstrip("/")) or "repo")
    while "%s-%03d" % (base, n) in (cur.get("works") or {}):
        n += 1
    return "%s-%03d" % (base, n)


def _write_integration(main, branch, source, why="", by_session="", correct=False):
    with Lock():
        cur = _integration_file()
        cur.setdefault("works", {})
        cur.setdefault("current", {})
        prior_id = cur["current"].get(main)
        prior = cur["works"].get(prior_id) if prior_id else None
        if prior and prior.get("branch") == branch:
            return prior                     # the same body of work, re-stated
        if correct:
            if not prior:
                raise SpecError("there is no body of work recorded for %s to correct. Record one: "
                                "workspaces.sh integration --repo %s --branch %s --why '<this work>'"
                                % (main, main, branch))
            prior.setdefault("corrections", []).append(
                {"from": prior.get("branch"), "at": iso(), "why": why or "",
                 "by_session": by_session or ""})
            prior["branch"] = branch
            prior["recorded_at"] = iso()
            cur["works"][prior_id] = prior
            write_json(_integration_path(), cur)
            rec = prior
        else:
            wid = _new_work_id(cur, main)
            rec = {"id": wid, "repo": main, "branch": branch, "recorded_at": iso(),
                   "source": source, "why": why or "", "by_session": by_session or "",
                   "corrections": []}
            cur["works"][wid] = rec
            cur["current"][main] = wid
            write_json(_integration_path(), cur)
    event("integration-recorded", repo=main, branch=branch, source=source,
          work=rec.get("id"), corrected=bool(correct), why=why or None)
    return rec


def record_integration(repo, branch, why="", by_session="", correct=False):
    """Point 14, the form the page asks for: Rich records the branch this body of
    work integrates on, before its first agent is spawned.

    Without `correct`, this STARTS a body of work: a new id, which becomes the
    repository's current one, and which every agent registered afterwards binds
    to. Agents already bound to an earlier body of work are untouched — that is
    the failure this shape exists to end.

    With `correct`, the branch of the CURRENT body of work is changed in place,
    keeping its id, so every agent bound to it moves with it. That is the other
    half of point 14: a recorded fact must stay correctable, or the refusal it
    causes can never heal."""
    main = main_checkout(repo)
    if not main:
        raise SpecError("the repository %s could not be resolved from git" % repo)
    branch = (branch or "").strip()
    if not branch:
        raise SpecError("name the branch this work integrates on (point 14)")
    if branch.startswith(CC_PREFIX) or branch.startswith(NATIVE_BRANCH_PREFIX):
        raise SpecError("%s is an agent's own workspace branch. Finished work never waits on the "
                        "agent's own branch (point 14); an integration branch is main or this work's "
                        "dev branch." % branch)
    if not branch_tip(main, branch):
        raise SpecError("there is no branch %s in %s. The branch a body of work integrates on is "
                        "recorded before its first agent is spawned, so it exists by then (point 14)."
                        % (branch, main))
    return _write_integration(main, branch, "recorded", why, by_session, correct)


def teammate_body_of_work(repo, branch, why="", by_session=""):
    """THE BODY OF WORK OF A TEAMMATE BOUND TO A BRANCH OF ITS OWN (2026-10-14).

    The CEO put the meeting and search work on the v1.3.0 release branch while
    fixes go to main, so two bodies of work run in one repository at once.
    `spawn.sh --integration richos=v1.3.0` used to CORRECT the repository's
    current body of work (richos-001) to v1.3.0, moving every running
    main-bound teammate with it; the lead found it 23 minutes later.

    This answers the teammate's own body of work instead, and never touches
    `current`: the repository's current one when it already integrates on
    `branch`, else the newest teammate body of work for (repository, branch),
    else a new one (source "teammate") that is not made current. The teammate
    is bound to its id like any other (register_cc), so a land is proved
    against `branch` and nobody else moves."""
    main = main_checkout(repo)
    if not main:
        raise SpecError("the repository %s could not be resolved from git" % repo)
    branch = (branch or "").strip()
    if not branch or branch.startswith(CC_PREFIX) or branch.startswith(NATIVE_BRANCH_PREFIX):
        raise SpecError("%r is not a branch a body of work integrates on (point 14)" % branch)
    if not branch_tip(main, branch):
        raise SpecError("there is no branch %s in %s to integrate on (point 14)" % (branch, main))
    with Lock():
        cur = _integration_file()
        works = cur.setdefault("works", {})
        current = works.get((cur.get("current") or {}).get(main) or "")
        if current and current.get("branch") == branch:
            return current
        mine = [w for w in works.values()
                if w.get("repo") == main and w.get("branch") == branch and w.get("source") == "teammate"]
        if mine:
            return max(mine, key=lambda w: w.get("recorded_at") or "")
        wid = _new_work_id(cur, main)
        rec = {"id": wid, "repo": main, "branch": branch, "recorded_at": iso(), "source": "teammate",
               "why": why or "", "by_session": by_session or "", "corrections": []}
        works[wid] = rec
        write_json(_integration_path(), cur)
    event("integration-recorded", repo=main, branch=branch, source="teammate", work=wid, corrected=False,
          why=why or None)
    return rec


def _add_workspace(rec, kind, repo, path, branch, source):
    path = realpath(path) if path else ""
    for w in rec["workspaces"]:
        if (path and w.get("path") == path) or (not path and branch and w.get("branch") == branch):
            if branch and not w.get("branch"):
                w["branch"] = branch
            return w
    w = {"kind": kind, "repo": realpath(repo), "path": path, "branch": branch or "",
         "registered_at": iso(), "source": source, "deleted_at": None}
    rec["workspaces"].append(w)
    _remember_repo(repo)
    _bind_body_of_work(rec, repo)
    return w


def _bind_body_of_work(rec, repo):
    """WHICH BODY OF WORK THIS AGENT BELONGS TO IN THIS REPOSITORY (point 14).

    The agent is bound to the work's ID, never to its branch. The id is the one
    thing about a body of work that does not change, so binding it keeps the
    branch live and correctable for an agent in flight while making a SECOND
    body of work in the same repository unable to reach backwards into this one.
    A frozen branch would do neither; that is what the deleted floor did."""
    main = _norm_repo(repo)
    wid = (_integration_file().get("current") or {}).get(main)
    if not wid:
        return False                 # nothing recorded: a spawn is refused before this (point 14)
    rec.setdefault("integration_work", {})
    rec["integration_work"].setdefault(main, wid)
    return True


def _unrecorded_repos(repos):
    """The repositories among `repos` with NO current body of work recorded —
    the ones point 14 forbids spawning into. Deduplicated, in order.

    A PATH THAT IS NOT A GIT REPOSITORY IS NEVER ONE OF THEM, and that is not a
    convenience — it is the same rule the deleted floor was deleted for. Point
    14 records A BRANCH, `record_integration` refuses any path git cannot
    resolve ("the repository %s could not be resolved from git"), and a
    directory with no branches has no land to measure and nothing to record. So
    until this line existed, an entity root that was not a repository was
    refused EVERY spawn, forever, and the remedy the refusal printed could not
    be run:

        spawn  -> exit 2 "no branch is recorded as the one this work integrates
                  on in <dir> ... Record it, then spawn: workspaces.sh
                  integration --repo <dir> --branch <main|dev/...>"
        remedy -> exit 2 "the repository <dir> could not be resolved from git"

    That is the shape this file already rejects by name: "with NO record at all
    the refusal HEALS"; a refusal that cannot heal "is not a floor, it is a
    wrong answer with the authority of a recorded one". Every repository still
    needs its record, including the repository of every cc/ workspace — nothing
    about a real repository is relaxed here."""
    out, seen = [], set()
    for r in repos:
        main = main_checkout(r) if r else ""
        if not main or main in seen:
            continue
        seen.add(main)
        if not integration_record(main):
            out.append(main)
    return out


def _refuse_unrecorded(repos, what):
    """Point 14, at the spawn: "RECORDED when that work starts, before its first
    agent is spawned. Nothing infers it and nothing guesses it." Raises,
    naming the recording command, when any repository in `repos` has no
    current body of work."""
    missing = _unrecorded_repos(repos)
    if missing:
        raise SpecError("no branch is recorded as the one this work integrates on in %s, so %s is "
                        "refused. The branch a body of work integrates on is RECORDED when that work "
                        "starts, before its first agent is spawned; nothing infers it and nothing "
                        "guesses it (point 14). Record it, then spawn:\n    workspaces.sh integration "
                        "--repo %s --branch <main|dev/...> --why '<this body of work>'"
                        % (", ".join(missing), what, missing[0]))


def _drop_orphans_for(path):
    """A registration arriving for a workspace a scan had called unregistered
    wins: the scan's record is withdrawn (call under the lock)."""
    for r in all_agents():
        if r.get("orphan") and not r.get("disposition") and any(
                w.get("path") == path for w in live_workspaces(r)):
            os.unlink(agent_path(r["key"]))
            event("unregistered-withdrawn", key=r["key"], path=path)


def live_workspaces(rec):
    return [w for w in rec.get("workspaces") or [] if not w.get("deleted_at")]


# ---------------------------------------------------------------------------
# point 11 — what "finished" means
# ---------------------------------------------------------------------------

def finished_state(rec, cache=None):
    """(finished, paused, why). Built only from recorded facts (point 11) and
    from the operating system for session end (point 12). Nothing else."""
    if rec.get("disposition"):
        return True, False, "its work was %s" % rec["disposition"].get("kind")
    if rec.get("orphan"):
        return True, False, "it has no registration: finished work of an ended session (point 3)"
    if rec.get("creation_failed"):
        return True, False, "its workspace creation failed, so it was never spawned (point 3)"
    if rec.get("session_id"):
        st, why = session_state(rec["session_id"], rec.get("session_identity"), cache)
        if st == "ended":
            return True, False, "its session has ended: %s (point 12)" % why
    end = rec.get("end")
    if end:
        if end.get("signal") == "stopped":
            return True, False, "it was stopped (%s)" % end.get("detail", "")
        if rec.get("handed_in"):
            return True, False, "it ended after handing in its work (point 11)"
        pause = rec.get("pause")
        if pause and pause.get("at", 0) <= end.get("at", 0):
            return False, True, "paused until: %s" % (pause.get("until") or "(nothing named)")
        return True, False, "the platform recorded the end of its run (%s at %s)" % (
            end.get("signal"), iso(end.get("at")))
    if rec.get("pause"):
        return False, True, "paused until: %s" % (rec["pause"].get("until") or "(nothing named)")
    return False, False, "its run has not ended"


# ---------------------------------------------------------------------------
# points 1, 2, 3, 6 — registration
# ---------------------------------------------------------------------------

def _check_workspace_name(path, branch):
    kind = classify(path, branch)
    if kind == "codex":
        raise SpecError("%s is a codex/ workspace. codex/ is never touched, and an agent never works "
                        "inside one; it works from a copy in its own cc/ workspace (point 2)." % path)
    if kind == "":
        raise SpecError("%s is on branch %r. A non-native workspace must be named cc/; creating one "
                        "that is not is refused (points 1, 3)." % (path, branch))
    return kind


def register_cc(session_id, name, repo, path, branch, identity=None, integration=""):
    """Point 3, event 1, for a cc/ workspace: registered BEFORE it exists.
    create-teammate-worktree.sh calls this first; if it raises, nothing is
    created and no spawn can name the workspace. `integration`: a branch this
    teammate lands on that is not the repository's current body of work's
    (teammate_body_of_work); the teammate alone is bound to it."""
    if not session_id:
        raise SpecError("no session id: a registration without one can never be matched to a spawn")
    if not NAME_RE.match(name or ""):
        raise SpecError("%r is not a usable agent name" % name)
    if not (branch or "").startswith(CC_PREFIX):
        raise SpecError("branch %r is not named cc/ (point 1)" % branch)
    repo = main_checkout(repo)
    if not repo:
        raise SpecError("the repository could not be resolved from git")
    ident = identity or identity_for(session_id)
    if not ident:
        raise SpecError("the session's process identity could not be read from the operating system, "
                        "so its end could never be told (point 12); registration refused")
    # Point 14: the workspace is the first half of the spawn (registered when it
    # is created, points 1 and 3), so the record has to exist before it too — a
    # registration made with nothing recorded would be a workspace bound to no
    # body of work, and its spawn would then be refused anyway, leaving a
    # registered, never-spawned workspace that point 5 cannot see until its
    # session ends. Refusing here leaves nothing behind at all.
    _refuse_unrecorded([repo], "creating %s's workspace" % name)
    own = teammate_body_of_work(repo, integration, "%s's own branch" % name, session_id) if integration else None
    key = named_key(session_id, name)
    with Lock():
        rec = load_agent(key) or new_record(key, name=name, session_id=session_id)
        # ONE WORKSPACE PER REPOSITORY PER NAME - AND THAT IS THE WHOLE RULE.
        #
        # This used to refuse ANY second registration under a name that had been
        # spawned, which reads as "names are used once" and is not the same
        # sentence: a name is used once for an AGENT, and point 10 says that one
        # agent's workspaces - "every workspace and branch it has" - go together.
        # On 2026-09-17 four teammates needed a workspace in a second repository
        # and this refusal sent every one of them to a SECOND NAME, which made
        # each a second agent landed separately; the CEO found the stray branch
        # of one (`cc/norm-opus-wireguide1`) himself.
        #
        # What must still be refused is unchanged, and is refused below by the
        # question it actually asks:
        #   - a second workspace in the SAME repository under one name (it would
        #     want the same cc/<name> branch; point 3's one registration);
        #   - a name whose agent is FINISHED (point 9: a finished agent never
        #     writes again, so it can have no use for a workspace) or whose work
        #     is already landed or discarded. That is where "names are used
        #     once" lives, and a reused name is still refused at the SPAWN by
        #     register_spawn.
        if rec.get("disposition"):
            raise SpecError("agent %s's work was already %s in this session; names are used once"
                            % (name, rec["disposition"].get("kind")))
        clash = [w for w in live_workspaces(rec)
                 if w.get("kind") == "cc" and realpath(w.get("repo") or "") == repo]
        if clash:
            raise SpecError("agent %s already has a workspace in %s (%s). One workspace per repository "
                            "per teammate: a second one in the same repository under the same name is "
                            "refused (point 3). Another REPOSITORY under this name is not - that is "
                            "what --repo <repo> --repo <repo> creates."
                            % (name, repo, clash[0].get("path") or clash[0].get("branch")))
        if rec.get("agent_id"):
            fin, _paused, why = finished_state(rec)
            if fin:
                raise SpecError("agent %s is finished (%s), so it gets no further workspace: a finished "
                                "agent never writes again (point 9), and its work is landed or "
                                "discarded (points 5, 7). For new work, spawn a fresh teammate - names "
                                "are used once." % (name, why))
        rec["session_identity"] = ident
        if own:
            rec.setdefault("integration_work", {})[_norm_repo(repo)] = own["id"]
        w = _add_workspace(rec, "cc", repo, path, branch, "create-teammate-worktree")
        w["created"] = False
        save_agent(rec)
    event("registered-cc", key=key, repo=repo, path=realpath(path), branch=branch)
    return rec


def confirm_cc(session_id, name, path, ok, why=""):
    key = named_key(session_id, name)
    # WHAT THE ENGINE ITSELF MADE (hunt part 4, finding 14). The creator
    # confirms only after it has seeded the workspace and run the repository's
    # own setup, so every ignored entry the workspace has now, and the main
    # checkout does not, is the engine's product and nobody's work: richos's
    # setup links each workspace to its own build cache, and that link held
    # every such land until somebody typed --ignored-not-needed. It is recorded
    # here, exactly, so the land can tell it from the agent's own ignored files.
    generated = _generated_manifest(realpath(path)) if ok else {}
    with Lock():
        rec = load_agent(key)
        if not rec:
            raise SpecError("no registration for %s" % name)
        for w in rec["workspaces"]:
            if w.get("path") == realpath(path):
                w["created"] = bool(ok)
                if generated:
                    w["generated"] = generated
        if not ok:
            rec["creation_failed"] = {"at": now(), "why": why}
        save_agent(rec)
    event("cc-created" if ok else "cc-creation-failed", key=key, path=realpath(path), why=why)
    return rec


def prompt_lines(prompt, marker):
    out = []
    for line in (prompt or "").splitlines():
        m = re.match(r"^\s*%s:\s*(.+?)\s*$" % re.escape(marker), line)
        if m:
            out.append(m.group(1))
    return out


def _planned_workspaces(payload):
    """The cc/ workspaces a DRY evaluation's caller is ABOUT to create, read from
    `richos_spawn_check.planned`. Never read for a live call."""
    chk = payload.get("richos_spawn_check")
    if not isinstance(chk, dict):
        return []
    out = []
    for p in chk.get("planned") or []:
        if isinstance(p, dict) and p.get("path"):
            out.append({"path": realpath(str(p["path"])), "repo": str(p.get("repo") or ""),
                        "branch": str(p.get("branch") or "")})
    return out


def is_spawn_check(payload):
    """Is this a DRY evaluation? Only under BOTH halves, and the second half is
    what makes it structural rather than a promise:

      1. the caller asked for one (`richos_spawn_check` is present), and
      2. the platform minted NO `tool_use_id`.

    A live PreToolUse[Agent] call always carries a tool_use_id: `register_spawn`
    has refused every payload without one since it was written, and spawns
    happen, so a payload that lacks one was never going to be registered anyway.
    The marker therefore cannot turn a live spawn into an unregistered one - it
    can only describe a call that has not been made yet."""
    return bool(isinstance(payload.get("richos_spawn_check"), dict)
                and not str(payload.get("tool_use_id") or ""))


def _check_planned_workspace(p, name):
    """The dry equivalent of clause 7a for a workspace that does not exist yet.

    "Is it registered, created, and on its registered branch?" has no answer
    before it is created, and a check with no answer must not be waved through.
    So the question that IS answerable now is asked instead, and it is the same
    question one step earlier: does its repository resolve, is its name the
    teammate's cc/ name, is its path absent, is its branch free, and can it be
    created? Every one of those is what create-teammate-worktree.sh refuses on,
    evaluated before it has made anything."""
    path, repo, branch = p["path"], p["repo"], p["branch"]
    main = main_checkout(repo) if repo else ""
    if not main:
        raise SpecError("the repository of the planned workspace %s could not be resolved from git "
                        "(repo %r)" % (path, repo))
    if branch != CC_PREFIX + name:
        raise SpecError("the planned workspace %s would be on %r; a teammate's workspace is named %s%s "
                        "(point 1)" % (path, branch, CC_PREFIX, name))
    if os.path.lexists(path):
        raise SpecError("%s already exists, so it cannot be created for %s. Pick a fresh identifier, or "
                        "land/discard whatever owns it." % (path, name))
    if branch_tip(main, branch):
        raise SpecError("branch %s already exists in %s - a teammate name is used once; pick a fresh "
                        "identifier" % (branch, main))
    probe = os.path.dirname(path)
    while probe and probe != os.path.dirname(probe) and not os.path.isdir(probe):
        probe = os.path.dirname(probe)
    if not probe or not os.access(probe, os.W_OK):
        raise SpecError("%s cannot be created: %s is not writable" % (path, probe or "/"))
    return main


def _pierce_before_registration(payload):
    # Native hook callbacks carry transcript_path. Standalone registry fixtures
    # and dry-run payloads do not. The separate Agent hook still refuses a real
    # dispatch whose original user context cannot be established.
    if not payload.get("transcript_path"):
        return
    here = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    inspector = _import_path("workspace_pierce", os.path.join(here, "scripts/lib/pierce.py"))
    try:
        result = inspector.inspect(payload)
        reason = inspector.refusal(result)
    except (OSError, ValueError) as error:
        raise SpecError("Pierce inspection unavailable: %s" % error)
    if reason:
        raise SpecError(reason)
    return result


def register_spawn(payload, entity, dry=False):
    """PreToolUse[Agent], point 3: the registration a spawn needs. Raises
    SpecError -> the spawn does not happen. Returns the record.

    A DRY EVALUATION (`dry=True`, reached through the `check-spawn` verb) asks
    the same question about a payload that HAS NOT BEEN SENT YET. Every refusal
    below is evaluated by the same code; the only difference is that nothing is
    written - no record, no binding to a body of work, no event.

    WHY IT EXISTS. Until 2026-09-13 the only way to find out whether a spawn
    would be refused was to make it: the guards run at PreToolUse, so a brief
    with two problems cost two dispatches, and each cost a round trip in front
    of the CEO. A pre-flight that could not run this function had to fake a
    session_id and a tool_use_id, which this function correctly refuses - so the
    pre-flight carried a known false positive on its most important check,
    which is a defense that reports "on" while protecting nothing.

    IT CANNOT BE USED TO SKIP A REGISTRATION. See `is_spawn_check`: the dry path
    is reached only when the platform minted no tool_use_id, and a live call
    always has one."""
    sid = str(payload.get("session_id") or "")
    tuid = str(payload.get("tool_use_id") or "")
    ti = payload.get("tool_input") if isinstance(payload.get("tool_input"), dict) else {}
    name = str(ti.get("name") or "")
    prompt = str(ti.get("prompt") or "")
    isolation = str(ti.get("isolation") or "")
    planned = _planned_workspaces(payload) if dry else []
    planned_by_path = dict((p["path"], p) for p in planned)
    if not sid or (not tuid and not dry):
        raise SpecError("the spawn payload carries no session_id/tool_use_id, so it cannot be registered")
    if not NAME_RE.match(name):
        raise SpecError("the spawn has no usable name, so it cannot be registered")
    inspection = None
    if not dry:
        # Command hooks can run concurrently. Inspection must finish BEFORE
        # this guard records any spawn evidence or acts on continuation state.
        inspection = _pierce_before_registration(payload)
    cc_paths = [realpath(p) for p in prompt_lines(prompt, "cross-repo-worktree") if p]
    if ti.get("cwd"):
        cc_paths.append(realpath(str(ti.get("cwd"))))
    continues = [p.split()[0] for p in prompt_lines(prompt, "continues") if p.split()]
    lands_pending = [p.split()[0] for p in prompt_lines(prompt, "lands-pending") if p.split()]
    ceo_ordered = prompt_lines(prompt, "ceo-ordered")
    stray = [p["path"] for p in planned if p["path"] not in cc_paths]
    if stray:
        raise SpecError("the planned workspace %s is on no 'cross-repo-worktree:' line of the prompt, so "
                        "the spawn would never name it" % stray[0])

    # Point 5, amended 2026-09-30: unrelated development does not depend on
    # pending integration. Only a continuation needs the pending list, because
    # it replaces finished work and must validate that work before cleanup.
    # Normal spawns must not scan or auto-land other agents' workspaces.
    # A DRY check asks the same question and acts on nothing (finding 29): it
    # used to auto-land and delete other eligible work, stopping its
    # processes, before reaching `if dry: return`. What WOULD land is left out
    # here exactly as the live call's land leaves it out.
    items = (pending(sid, entity, deadline=_gate_deadline(GATE_SPAWN_BUDGET), dry=dry)
             if continues else [])
    items = [i for i in items if not i.get("would_land") and not i.get("unregistered")]
    continuation_keys = []
    held_work = []
    for c in continues:
        match = [i for i in items if i["name"] == c or i["key"] == c]
        if not match and os.path.isfile(_hold_path(c)):
            held_work.append(c)          # held work with no teammate yet: this spawn runs it
            continue
        if not match:
            raise SpecError("continues: %s - there is no pending finished agent of that name to continue "
                            "(point 7)" % c)
        if len(match) != 1:
            raise SpecError("continues: %s is ambiguous; name its exact registered key" % c)
        continuation_keys.append(match[0]["key"])
        _require_clean(load_agent(match[0]["key"]), "continue it (its workspaces are deleted when the new "
                       "agent starts, point 7)", record=not dry)

    ident = identity_for(sid)
    if not ident:
        raise SpecError("the session's process identity could not be read from the operating system; "
                        "a registration that could never tell its session ended is refused (point 12)")
    key = named_key(sid, name)
    with Lock():
        rec = load_agent(key)
        if [p for p in cc_paths if p not in planned_by_path]:
            if not rec:
                raise SpecError("no registration exists for %s in this session. A cc/ workspace is "
                                "registered when it is created: use create-teammate-worktree.sh <repo> %s "
                                "(points 1, 3)." % (name, name))
        rec = rec or new_record(key, name=name, session_id=sid)
        if rec.get("agent_id") or rec.get("disposition"):
            raise SpecError("agent %s already ran in this session; names are used once" % name)
        known = dict((w.get("path"), w) for w in live_workspaces(rec) if w.get("kind") == "cc")
        planned_repos = []
        for p in cc_paths:
            if p in planned_by_path:
                planned_repos.append(_check_planned_workspace(planned_by_path[p], name))
                continue
            w = known.get(p)
            if not w:
                raise SpecError("%s was not registered for %s in this session (points 1, 3)" % (p, name))
            if not w.get("created") or not os.path.isdir(p):
                raise SpecError("%s is registered but was never created" % p)
            cur = ""
            for e in worktree_list(p) or []:
                if e["path"] == p:
                    cur = e["branch"]
            _check_workspace_name(p, cur)
            if cur != w.get("branch"):
                raise SpecError("%s is on %r, not the registered branch %r" % (p, cur, w.get("branch")))
        # Point 14: "RECORDED when that work starts, BEFORE ITS FIRST AGENT IS
        # SPAWNED. Nothing infers it and nothing guesses it." Every repository
        # this agent will work in - the entity, and the repository of every
        # cc/ workspace it names - must have a current body of work, and the
        # agent is BOUND to it here, by id, at the spawn. With no record the
        # spawn does not happen (point 3), and the refusal names the command.
        # This is what leaves `integration_target` no fallback to "current".
        # A PLANNED workspace's repository counts the same: its record has to
        # exist before the workspace is created (register_cc refuses without
        # one), so a dry evaluation that skipped it would report a pass for a
        # spawn whose very next step is refused.
        work_repos = ([main_checkout(entity) or realpath(entity)] if entity else []) \
            + [w.get("repo") for w in live_workspaces(rec) if w.get("repo")] \
            + [main_checkout(p) or realpath(p) for p in cc_paths if p not in planned_by_path] \
            + planned_repos
        _refuse_unrecorded(work_repos, "the spawn of %s" % name)
        recovery_assignment = _recovery_assignment(payload, rec, continuation_keys, lands_pending, inspection)
        recovery, host = _recovery_adapter()
        try:
            recovery_jobs = recovery.admitted(host, payload, rec, recovery_assignment, work_repos, entity, dry=dry)
        except (OSError, ValueError) as error:
            raise SpecError("recovery admission unavailable: %s" % error)
        if dry:
            return rec
        for r_ in work_repos:
            _bind_body_of_work(rec, r_)
        rec.update({"tool_use_id": tuid, "subagent_type": str(ti.get("subagent_type") or ""),
                    "recovery_assignment": recovery_assignment,
                    "recovery_jobs": [r["id"] for r in recovery_jobs],
                    "isolation": isolation, "session_identity": ident,
                    "ceo_ordered": (ceo_ordered[0] if ceo_ordered else rec.get("ceo_ordered")),
                    "continues": continuation_keys,
                    "lands_pending": lands_pending, "entity": realpath(entity),
                    "dispatch_transcript": str(payload.get("transcript_path") or ""),
                    "spawned_at": iso()})
        rec.pop("creation_failed", None)
        save_agent(rec)
        if entity:
            _remember_repo(entity)
    for c in held_work:
        _hold_runs(c, name)
    for recovery_job in recovery_jobs:
        scheduled = recovery.schedule(recovery_job, engine_root())
        if scheduled and scheduled.get("observation_gap"):
            event("recovery-observation-gap", key=key, job=recovery_job["id"], gap=scheduled["observation_gap"])
    event("registered-spawn", key=key, tool_use_id=tuid, cc=cc_paths, isolation=isolation,
          continues=continues, lands_pending=lands_pending)
    return rec


def withdraw_cc(session_id, name, why=""):
    """THE INVERSE OF `register_cc`, for a workspace whose spawn never happened.

    Point 3 is "if registration fails, the spawn does not happen", and its other
    half has never had a command: a workspace registered and created for a spawn
    that then could not be made is a registration for work that will not happen.
    Leaving it costs the name (names are used once) and leaves a directory and a
    branch behind for a land to puzzle over.

    IT REFUSES ANYTHING THAT EVER RAN. No agent_id, no tool_use_id, no
    spawned_at, no disposition - if any of those is set this is not an
    un-happened spawn, and `workspaces.sh land` or `discard` is the answer, not
    this. The record is then deleted rather than dispositioned: there is no work
    to account for, and the event log keeps the trace.

    THE DELETION IS `_delete`'s, NOT ITS OWN, and that is the whole design of
    this function rather than a detail. This first hand-rolled the sequence -
    remove_workspace, then delete_branch, in a loop - and it was correct on the
    day it was written. Hours later the container reaper landed, adding
    `stop_containers` inside `_delete` because an agent's container had outlived
    its agent and its worktree by six weeks. A hand-rolled copy would have
    quietly kept leaving those containers behind, and nothing would have said
    so. "Land, discard and the retry are the only code in the engine that
    deletes a workspace" is an invariant that holds only while every deleter
    goes through the same one; a fourth copy is how that stops being true.
    """
    key = named_key(session_id, name)
    rec = load_agent(key)
    if not rec:
        raise SpecError("there is no registration for %s in this session to withdraw" % name)
    for field, what in (("agent_id", "it ran"),
                        ("tool_use_id", "its spawn was registered"),
                        ("spawned_at", "its spawn was registered"),
                        ("disposition", "its work was already decided")):
        if rec.get(field):
            raise SpecError("%s is not an un-happened spawn (%s). Use `workspaces.sh land %s` or "
                            "`workspaces.sh discard %s` - this command withdraws only a registration "
                            "whose spawn never happened." % (name, what, name, name))
    ws = [w for w in live_workspaces(rec) if w.get("path")]
    paths = [w.get("path") for w in ws]
    _delete(rec, ws, branches=True, why="withdrawn: " + (why or "its spawn never happened"))
    fresh = load_agent(key) or rec
    if [w for w in live_workspaces(fresh) if w.get("path")] or fresh.get("deletion"):
        raise SpecError("%s could not be fully withdrawn: %s"
                        % (name, (fresh.get("deletion") or {}).get("last_error", "unknown")))
    with Lock():
        try:
            os.remove(agent_path(key))
        except OSError:
            pass
    event("withdrawn-cc", key=key, why=why, paths=paths)
    return {"withdrawn": paths}


def register_readonly(payload, entity):
    """Point 9: "The platform restarts finished agents (14 times observed). A
    restarted agent is refused every tool, so it cannot write anywhere,
    including after its workspace is gone."

    THE LOCK-OUT FINDS AN AGENT THROUGH ITS REGISTRATION, and a read-only type
    had none. READONLY_ALLOWLIST answers one question — does this agent need an
    isolated workspace? — and the spawn guard returned exit 0 on it before
    clause 7, so the type that needs no workspace also got no registration.
    barrier() then answered UNREGISTERED for it forever: it could never be
    found finished, so point 9 never fired for it once.

    That would be harmless if a read-only agent could not write. `Explore`'s
    allowlist is every tool EXCEPT Edit/Write/NotebookEdit, so IT CARRIES BASH:
    a restarted finished Explore can write anywhere a shell can, including
    after its workspace is gone. The exemption from isolation was never an
    exemption from the lock-out; it only looked like one.

    So a read-only spawn is registered here, with NO workspaces, and nothing
    else about it changes. It carries no name contract (a read-only spawn is
    not required to have a name, so the key is its tool_use_id), and it does
    is independent of pending integration and has never stopped an
    Explore spawn and widening it is not this fix. Because an agent that
    produced nothing counts as landed (point 7) and land() returns landed for
    an empty workspace list, it is landed the moment it finishes and never
    becomes pending work blocking the CEO's turn ends."""
    sid = str(payload.get("session_id") or "")
    tuid = str(payload.get("tool_use_id") or "")
    ti = payload.get("tool_input") if isinstance(payload.get("tool_input"), dict) else {}
    if not sid or not tuid:
        raise SpecError("the spawn payload carries no session_id/tool_use_id, so it cannot be registered")
    _pierce_before_registration(payload)
    name = str(ti.get("name") or "")
    if not NAME_RE.match(name):
        name = "readonly-" + _key_segment(tuid)[:40]
    ident = identity_for(sid)
    if not ident:
        raise SpecError("the session's process identity could not be read from the operating system; "
                        "a registration that could never tell its session ended is refused (point 12)")
    key = "%s--ro-%s" % (_key_segment(sid), _key_segment(tuid))
    with Lock():
        rec = load_agent(key)
        if rec and (rec.get("agent_id") or rec.get("disposition")):
            raise SpecError("this spawn is already registered as %s; a tool_use_id is used once" % key)
        rec = rec or new_record(key, name=name, session_id=sid)
        rec.update({"tool_use_id": tuid, "subagent_type": str(ti.get("subagent_type") or ""),
                    "isolation": str(ti.get("isolation") or ""), "session_identity": ident,
                    "readonly": True, "entity": realpath(entity), "spawned_at": iso()})
        save_agent(rec)
    event("registered-readonly", key=key, tool_use_id=tuid, subagent_type=rec.get("subagent_type"))
    return rec


def _is_key(s):
    return "--" in (s or "")


def _find_by_tool_use(session_id, tuid):
    for r in all_agents():
        if r.get("session_id") == session_id and r.get("tool_use_id") == tuid:
            return r
    return None


def _provisional_key(session_id, agent_id):
    return "%s--agent-%s" % (_key_segment(session_id), _key_segment(agent_id))


def _absorb_provisional(rec, prov_key):
    """The platform's own start and end for this agent — recorded against a
    PROVISIONAL record because nothing had bound its id to a registration yet —
    join the registration they belong to. Returns the provisional, or None.

    A provisional record that is still live has no facts of its own left after
    this and is removed, exactly as it always was. One that has already been
    DISPOSED OF (its native workspace landed on its own, which is what happens
    when the binding is missing for a whole run) is KEPT and stamped: it is the
    record of what the platform did, and deleting it to tidy up would delete the
    evidence that this reconciliation was needed."""
    prov = load_agent(prov_key)
    if not prov or prov.get("key") == rec.get("key"):
        return None
    for w in live_workspaces(prov):
        _add_workspace(rec, w["kind"], w["repo"], w["path"], w["branch"], w.get("source", ""))
    for f in ("started_at", "end", "handed_in"):
        if prov.get(f) and not rec.get(f):
            rec[f] = prov[f]
    if os.path.exists(agent_path(prov_key)):
        os.unlink(agent_path(prov_key))
    else:
        prov["absorbed_by"] = rec["key"]
        write_json(done_path(prov_key), prov)
    return prov


def bind_agent(session_id, tuid, agent_id, entity, name="", isolation=""):
    """PostToolUse[Agent]: the platform's agent id joins the registration, and
    the native workspace it created is registered (point 6).

    THE SPAWN WHOSE REGISTRATION WAS KILLED (2026-09-17). Point 3 says "if
    registration fails, the spawn does not happen", and the registration is
    written by the PreToolUse[Agent] guard — which the PLATFORM MAY KILL. On
    2026-09-17 at 10:57:48Z it did: `guard-worktree-isolation.sh` ran 10.027s
    against a 10s budget and the orchestrator's transcript records it as
    `hook_cancelled … timedOut: true`; two seconds later the same transcript
    records `Async agent launched successfully … agentId: a794a61d062e6d302`.
    A killed hook is not a refusal, so the spawn happened with no registration,
    and `sage-opus-nightly1`'s cc/ workspace could not be retired for hours —
    the ending point 11 requires "automatically and never by Rich noticing".
    The hook's budget is now larger than the longest wait inside it, but the
    platform decides when to kill a hook and no budget makes that impossible.

    So THIS call — the Post, which the platform delivers for a tool that
    actually ran, and which carries both the name the spawn used and the id it
    became — repairs the gap at the first moment it exists, seconds after the
    spawn rather than at the next land. It records what happened; it never
    certifies a spawn that was never checked, which is why the record says so."""
    if not AGENT_ID_RE.match(agent_id or ""):
        raise SpecError("not an agent id: %r" % agent_id)
    reconciled = ""
    with Lock():
        rec = _find_by_tool_use(session_id, tuid)
        if not rec and name and NAME_RE.match(name):
            cand = load_agent(named_key(session_id, name))
            if (cand and not cand.get("agent_id") and not cand.get("provisional")
                    and not cand.get("disposition") and not cand.get("orphan")):
                rec = cand
                reconciled = "PostToolUse[Agent]"
                rec["tool_use_id"] = rec.get("tool_use_id") or tuid
                rec["isolation"] = rec.get("isolation") or isolation
                rec["registration_reconciled"] = {
                    "at": now(), "agent_id": agent_id, "source": reconciled,
                    "why": "no registration carried this tool call, so the spawn's own "
                           "PreToolUse registration never ran; the platform's Post for the same "
                           "call names this session's registration for %r (point 3)" % name}
        if not rec:
            return None
        prov_key = _provisional_key(session_id, agent_id)
        rec["agent_id"] = agent_id
        if rec.get("isolation") == "worktree" and entity:
            main = main_checkout(entity) or realpath(entity)
            npath = os.path.join(main, ".claude", "worktrees", "agent-" + agent_id)
            _add_workspace(rec, "native", main, npath, NATIVE_BRANCH_PREFIX + agent_id, "PostToolUse[Agent]")
            _drop_orphans_for(realpath(npath))
        _absorb_provisional(rec, prov_key)
        save_agent(rec)
    event("bound", key=rec["key"], agent_id=agent_id, **({"reconciled": reconciled} if reconciled else {}))
    _on_start(rec)
    return rec


def record_start(session_id, agent_id, cwd, agent_type):
    """SubagentStart: the worker's own start. Registers its native workspace
    by the exact path the platform started it in (point 6)."""
    if not AGENT_ID_RE.match(agent_id or ""):
        return None
    top = ""
    if cwd and os.path.isdir(cwd):
        rc, out, _ = git(cwd, "rev-parse", "--show-toplevel")
        top = realpath(out.strip()) if rc == 0 else ""
    with Lock():
        key = key_for_id(agent_id)
        rec = load_agent(key) if key else None
        native = top if (top and is_native_path(top + os.sep) and top.endswith("agent-" + agent_id)) else ""
        if not rec:
            if not native:
                return None  # a helper with no workspace: nothing to register
            prov_key = "%s--agent-%s" % (_key_segment(session_id), _key_segment(agent_id))
            rec = read_json(agent_path(prov_key)) or new_record(prov_key, name="agent-" + agent_id,
                                                                  session_id=session_id, agent_id=agent_id,
                                                                  provisional=True)
            ident = identity_for(session_id)
            if ident:
                rec["session_identity"] = ident
        if native:
            branch = ""
            for e in worktree_list(native) or []:
                if e["path"] == native:
                    branch = e["branch"]
            _add_workspace(rec, "native", main_checkout(native), native, branch, "SubagentStart")
            _drop_orphans_for(native)
        rec["subagent_type"] = rec.get("subagent_type") or agent_type
        rec["started_at"] = rec.get("started_at") or iso()
        save_agent(rec)
    event("start", key=rec["key"], agent_id=agent_id, cwd=realpath(cwd))
    _on_start(rec)
    return rec


def _on_start(rec):
    """Point 7: when a continuing agent starts, the old agent's workspaces are
    deleted (its branches stay until the new agent's work lands)."""
    for old_key in rec.get("continues") or []:
        old = load_agent(old_key)
        if not old or (old.get("disposition") or {}).get("kind") == "continued":
            continue
        with Lock():
            old = load_agent(old_key)
            old["disposition"] = {"kind": "continued", "at": now(), "by": rec["key"]}
            save_agent(old)
        event("continued", key=old_key, by=rec["key"])
        _delete(old, [w for w in live_workspaces(old) if w.get("path")], branches=False,
                why="continued by %s (point 7)" % rec.get("name"))


# ---------------------------------------------------------------------------
# end of run, hand-in, pause (point 11)
# ---------------------------------------------------------------------------

def _record_for_agent(session_id, agent_id, name=""):
    key = key_for_id(agent_id) if agent_id else ""
    if key:
        return load_agent(key)
    if name and session_id:
        return load_agent(named_key(session_id, name))
    return None


def _running_calls(session_id, agent_id):
    """This agent's Bash calls whose shell is still running, from agent_hold's own record (the
    shell recorded itself at spawn: pid, parent, start time; a reused pid never matches), as
    [(tool call id, command)], plus the ended ones whose result the platform has still to
    deliver (_undelivered_calls). Its wait command is not work. Any failure to read is [] (the
    end is then recorded as it always was)."""
    try:
        ah = _agent_hold()
        if not ah._valid_ids(session_id, agent_id):
            return []
        table = ah.snapshot()
        running = [(c["tid"], c["command"]) for c in ah.calls(session_id, agent_id, table)
                   if c["mode"] != "exempt" and not table[c["pid"]]["stat"].startswith("Z")]
        return running + _undelivered_calls(session_id, agent_id, ah, {tid for tid, _c in running})
    except Exception:
        return []


def _agent_transcript(session_id, agent_id):
    """The platform's own transcript of this agent, beside its meta.json (platform_agent_record)."""
    if "/" in session_id:
        return ""
    hits = sorted(glob.glob(os.path.join(_platform_projects_dir(), "*", session_id, "subagents",
                                         "agent-%s.jsonl" % agent_id)))
    return hits[0] if hits else ""


def _undelivered_calls(session_id, agent_id, ah, running):
    """This agent's background calls whose shell has already ended but whose completion the
    platform has not yet delivered, as [(tool call id, command)]: its transcript holds the call's
    background receipt (a tool result carrying a backgroundTaskId) and no task notification naming
    it yet. The platform wakes the agent with that notification, so a stop before it arrives is not
    its end either (2026-10-08: echo-fable-dict5's test run ended at 16:55:45, its turn ended at
    16:55:46.18 with no shell left running, the end was recorded, and the notification woke it at
    16:55:46.76 into the lock-out). A call whose result came back directly has no receipt, and a
    delivered one names itself, so neither ever defers an end. No transcript is []."""
    directory = ah._shell_dir(session_id, agent_id)
    candidates = {}
    for name in os.listdir(directory) if os.path.isdir(directory) else []:
        tid = name[:-5]
        if name.endswith(".json") and tid not in running and ah._valid_ids(tid):
            meta = read_json(os.path.join(directory, name)) or {}
            if meta.get("mode") != "exempt":
                candidates[tid] = str(meta.get("command") or "")
    path = _agent_transcript(session_id, agent_id) if candidates else ""
    if not path:
        return []
    receipts, delivered = set(), set()
    with open(path, "rb") as f:
        for line in f:
            for m in re.finditer(rb"<tool-use-id>([A-Za-z0-9_.-]+)</tool-use-id>", line):
                delivered.add(m.group(1).decode())
            if b"backgroundTaskId" not in line:
                continue
            try:
                row = json.loads(line)
            except ValueError:
                continue
            result = row.get("toolUseResult") if isinstance(row, dict) else None
            content = (row.get("message") or {}).get("content") if isinstance(row, dict) else None
            if isinstance(result, dict) and result.get("backgroundTaskId") and isinstance(content, list):
                receipts.update(i.get("tool_use_id") for i in content
                                if isinstance(i, dict) and i.get("type") == "tool_result")
    return [(tid, cmd) for tid, cmd in sorted(candidates.items()) if tid in receipts and tid not in delivered]


# The history fact a deferred end leaves; stall_watch.py reads it to say why a
# silent agent has not ended and which one command ends its own commands.
END_DEFERRED_FACT = "SubagentStop while its own command(s) still run or still owe it their result: not its end"


def record_end(session_id, agent_id, signal_name, detail=""):
    """The platform's own end-of-run signal, recorded automatically.

    A SubagentStop WHILE A COMMAND THE AGENT STARTED IS STILL RUNNING IS NOT ITS
    END (2026-10-04: echo-opus-panel16 ended its turn at 22:39Z to wait on its own
    background timer, was recorded finished, and was sealed out of its workspace
    when the timer woke it; its evidence was left uncommitted). The platform
    wakes the agent again when that command ends, so nothing is recorded here
    and the end is taken from the SubagentStop that ends that next turn. The
    same holds for a background command that has just ended but whose completion
    notification has not reached the agent yet (2026-10-08, echo-fable-dict5):
    that notification is the wake. A stop
    (TaskStop, stoppedByUser) and its session's end still end it at once. A
    PAUSED agent's end is recorded as before: its commands are frozen, not
    running, and a pause already keeps it unsealed (finished_state)."""
    if signal_name == "SubagentStop":
        running = _running_calls(session_id, agent_id)
        if running:
            deferred = None
            with Lock():
                rec = _record_for_agent(session_id, agent_id)
                if rec and not (rec.get("disposition") or rec.get("end") or rec.get("pause")):
                    rec.setdefault("history", []).append({
                        "at": iso(), "fact": END_DEFERRED_FACT,
                        "calls": [tid for tid, _c in running]})
                    save_agent(rec)
                    deferred = rec
            if deferred:
                event("end-deferred", key=deferred["key"], signal=signal_name,
                      detail="; ".join("%s: %s" % (tid, cmd[:80]) for tid, cmd in running))
                return deferred
    if signal_name == "stopped":
        # A stopped agent's suspended processes would hold their termination forever.
        # Released by the ids the stop names, BEFORE any early return below: an agent
        # already disposed of, or one the registry never knew, is still released
        # (Sage's catch 1).
        release_held_ids(session_id, agent_id)
    # Its run has ended (the deferral above did not apply): no command of its own
    # stays running after it. A paused agent's work is never ended (a pause is
    # not a stop); end_owned() also refuses while agent_hold holds it.
    current = _record_for_agent(session_id, agent_id)
    if signal_name == "stopped" or not (current and current.get("pause")):
        end_owned_work(session_id, agent_id, (current or {}).get("name") or "",
                       (current or {}).get("key"), "its run ended (%s)" % signal_name)
    with Lock():
        rec = _record_for_agent(session_id, agent_id)
        if not rec:
            return None
        if rec.get("disposition"):
            rec.setdefault("history", []).append({"at": iso(), "fact": "end after disposition", "signal": signal_name})
            if os.path.exists(agent_path(rec["key"])) or not os.path.exists(done_path(rec["key"])):
                save_agent(rec)
            else:
                # Already filed as done: the late fact goes where the record is,
                # never back into live storage (V2-09).
                write_json(done_path(rec["key"]), rec)
            return rec
        prev = rec.get("end")
        if prev and prev.get("signal") == "stopped" and signal_name != "stopped":
            return rec
        rec["end"] = {"at": now(), "signal": signal_name, "detail": detail}
        if signal_name == "stopped":
            rec["pause"] = None
        save_agent(rec)
    # The end-of-run signal carries the agent's id too, so it is the last
    # observation: a ref created in its LAST call is still its own, even if that
    # call's PostToolUse never arrived. It CONSUMES EVERY window still open —
    # calls overlap, so there can be more than one — and nothing that happens
    # after the run has ended is ever compared against them.
    observe_created_refs(rec, all_open=True)
    event("end", key=rec["key"], signal=signal_name, detail=detail)
    if signal_name == "SubagentStop":
        # A normal end the deferral above permitted: the same pointer a handover gets.
        attach_proof_reports(rec["key"], "end")
    return rec


def _platform_projects_dir():
    return (os.environ.get("RICHOS_PROJECTS_DIR") or "").strip() or os.path.join(
        (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() or os.path.join(os.path.expanduser("~"), ".claude"),
        "projects")


def platform_agent_record(rec):
    """The platform's OWN per-agent record, which it writes beside the agent's
    transcript as <projects>/<project>/<session>/subagents/agent-<id>.meta.json
    and updates at the moment a run ends. Read, never written."""
    aid = (rec.get("agent_id") or "").strip()
    if not AGENT_ID_RE.match(aid):
        return None
    leaf = os.path.join("subagents", "agent-%s.meta.json" % aid)
    base = _platform_projects_dir()
    pats = []
    sid = (rec.get("session_id") or "").strip()
    if sid and "/" not in sid:
        pats.append(os.path.join(base, "*", sid, leaf))
    pats.append(os.path.join(base, "*", "*", leaf))
    for pat in pats:
        for p in sorted(glob.glob(pat)):
            d = read_json(p)
            if d is not None:
                return d
    return None


def platform_agent_records(session_id):
    """Every per-agent record the platform wrote for THIS session, as
    (agent_id, record). Read, never written. The platform writes one for each
    agent it actually launched, and it carries the NAME the spawn used and the
    TOOL CALL it came from — which is the binding the registry would otherwise
    have to guess at."""
    sid = (session_id or "").strip()
    if not sid or "/" in sid:
        return []
    out = []
    seen = set()
    for p in sorted(glob.glob(os.path.join(_platform_projects_dir(), "*", sid,
                                           "subagents", "agent-*.meta.json"))):
        aid = os.path.basename(p)[len("agent-"):-len(".meta.json")]
        if aid in seen or not AGENT_ID_RE.match(aid):
            continue
        d = read_json(p)
        if isinstance(d, dict):
            seen.add(aid)
            out.append((aid, d))
    return out


def platform_agent_transcript(rec):
    """The agent's own JSONL, beside the per-agent record the platform writes.

    <projects>/<project>/<session>/subagents/agent-<id>.jsonl — the same
    directory platform_agent_record reads, and the same file the session's
    scratch `tasks/<id>.output` is a symlink to. Read, never written."""
    aid = (rec.get("agent_id") or "").strip()
    if not AGENT_ID_RE.match(aid):
        return ""
    leaf = os.path.join("subagents", "agent-%s.jsonl" % aid)
    base = _platform_projects_dir()
    sid = (rec.get("session_id") or "").strip()
    pats = []
    if sid and "/" not in sid:
        pats.append(os.path.join(base, "*", sid, leaf))
    pats.append(os.path.join(base, "*", "*", leaf))
    for pat in pats:
        for p in sorted(glob.glob(pat)):
            if os.path.isfile(p):
                return p
    return ""


def qa_throwaway_lines(ref, me=""):
    """What a QA teammate's walk wrote from scratch, said at its land.

    The CEO, 2026-09-20: "what else must be done to ensure the QA toolkit
    actually gets used?" The spawn carries the toolkit's index in; this is the
    other end — the land says what the walk wrote instead of reaching for it.
    111 helper scripts across eight walks is the baseline it measures against,
    and a number nobody ever reads is the same as no number.

    INFORMATION, NEVER A REFUSAL. A walk that genuinely needed a one-off is not
    a defect, and a land that refused over a count would be waived the first
    time it was right — which is how a guard dies. It never raises either: a
    counter that could break the deleter it hangs off would be a worse defect
    than the one it reports.

    NEVER SILENT. A transcript it cannot find is said so, with where it looked.
    A count that is absent because nothing looked reads exactly like a clean
    walk, and those are opposite facts."""
    out = []
    try:
        here = os.path.join(os.path.dirname(os.path.dirname(
            os.path.abspath(__file__))), "scripts", "lib")
        # Both modules carry a hyphen in their filename, so they are loaded by
        # path rather than imported by name — the same reason spawn.py has its
        # own _load. ONE definition of "is this a QA type" (qa-toolkit.py, off
        # QA_TOOLKIT_AGENTS) and ONE definition of what counts as a throwaway
        # (qa-throwaways.py); this file restates neither.
        qa_toolkit = _import_path("qa_toolkit", os.path.join(here, "qa-toolkit.py"))
        throwaways = _import_path("qa_throwaways", os.path.join(here, "qa-throwaways.py"))
        rec = _resolve(ref, me)
        stype = (rec.get("subagent_type") or "").strip()
        if not qa_toolkit.is_qa_type(stype):
            return []
        path = platform_agent_transcript(rec)
        if not path:
            return ["qa toolkit:  no transcript found for %s under %s — the count of "
                    "helper scripts this walk wrote was NOT taken"
                    % (rec.get("name") or ref, _platform_projects_dir())]
        events, rows = throwaways.scan(path)
        scripts, toolkit = throwaways.classify(events)
        n = len(scripts)
        if n:
            out.append("qa toolkit:  %d helper script%s written from scratch in this walk "
                       "(%s):" % (n, "" if n == 1 else "s", os.path.basename(path)))
            for s in sorted(scripts, key=lambda s: s["rows"][0])[:12]:
                out.append("               %s" % os.path.basename(s["path"]))
            if n > 12:
                out.append("               ... and %d more — "
                           "scripts/qa-throwaways.sh %s" % (n - 12, path))
        else:
            out.append("qa toolkit:  0 helper scripts written from scratch in this walk")
        if toolkit:
            out.append("               %d added to the committed toolkit, which is the "
                       "wanted behavior" % len(toolkit))
    except Exception as e:                                   # never break a land
        out = ["qa toolkit:  the count could not be taken (%s)" % str(e)[:160]]
    return out


def _import_path(name, path):
    import importlib.util
    spec = importlib.util.spec_from_file_location(name, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


# POINT 11's UNTIDY ENDINGS. "That covers every ending: it handed in its work,
# crashed, was cut off by a limit, OR WAS STOPPED." The tidy ending arrives as
# a hook: the LAST SubagentStop of a run carries the agent's own bound id, and
# record_end takes it. A STOPPED agent's run never reaches that point, so no
# SubagentStop is ever delivered for it — measured on this machine on
# 2026-09-13: of 24 runs the platform flagged stoppedByUser in 14 days, 23 had
# no terminal SubagentStop at all, and the two the CEO stopped that day
# (reed-opus-fr1/fr2, killed 18:24:21Z and 18:26:28Z with
# "[Request interrupted by user]" as the last line of each transcript) have
# ZERO rows in worktree-ledger.jsonl, which worker-ended-handoff.sh appends to
# on every SubagentStop and on nothing else. They were marked finished BY HAND
# at 22:56:32Z, four and a half hours later, which is exactly what point 11
# forbids ("automatically and never by Rich noticing").
#
# The signal does exist. The platform records the stop ITSELF, in the agent's
# own record, within 0-5 s of the last transcript row (measured across all 20
# flagged runs that still have transcripts). Reading a recorded terminal fact
# is not a liveness question: nothing here asks whether a process is alive, and
# a running agent never carries the flag.
PLATFORM_ENDINGS = (
    ("stoppedByUser", "stopped", "the platform recorded stoppedByUser: the run was stopped (point 11)"),
)


def _platform_ending(rec):
    """The ending the platform recorded for this agent and the registry has not
    yet adopted, as (field, signal, why), or None. Reads only."""
    if not rec or rec.get("end") or rec.get("disposition") or rec.get("orphan"):
        return None
    d = platform_agent_record(rec)
    if not d:
        return None
    for field, signal_name, why in PLATFORM_ENDINGS:
        if d.get(field):
            return field, signal_name, why
    return None


def observe_platform_end(rec):
    """Point 11: an ending the platform recorded in its own per-agent record,
    for which it delivers no hook, makes the agent finished — automatically,
    at the next moment anything asks."""
    found = _platform_ending(rec)
    if found:
        field, signal_name, why = found
        with Lock():
            fresh = load_agent(rec["key"]) or rec
            if fresh.get("end") or fresh.get("disposition"):
                return fresh
            fresh["end"] = {"at": now(), "signal": signal_name, "detail": why, "source": field}
            fresh["pause"] = None
            save_agent(fresh)
        release_held_work(fresh)
        end_owned_work(str(fresh.get("session_id") or ""), str(fresh.get("agent_id") or ""),
                       fresh.get("name") or "", fresh.get("key"), "its run ended (%s)" % signal_name)
        observe_created_refs(fresh, all_open=True)
        event("end", key=fresh["key"], signal=signal_name, detail=why, source=field)
        return fresh
    return rec


# POINT 3'S REGISTRATION, WHEN THE HOOK THAT WRITES IT WAS KILLED. The spawn is
# registered at PreToolUse[Agent] and bound at PostToolUse[Agent]; both are
# hooks, and a hook the platform kills still lets the tool run. Measured
# 2026-09-17 on this machine: `guard-worktree-isolation.sh` was killed at
# 10.027s against its 10s budget (orchestrator transcript, `hook_cancelled …
# timedOut: true`, tool_use_id toolu_01C3dCBDwFksv5tBHramMDch) and the agent
# launched anyway. The registry was then left with a NAMED record holding a cc/
# workspace and no agent id — `started_at` None, `end` None — beside a
# PROVISIONAL record holding the platform's own SubagentStart (10:57:50Z) and
# SubagentStop (11:13:58Z) for the very same run. `land` refused the finished
# work with "its run has not ended", and the only ways left to retire it were
# the ones point 11 forbids ("automatically and never by Rich noticing").
#
# The facts were never missing — only the JOIN between them was, and the
# platform records that join itself, in the same per-agent file
# `observe_platform_end` already reads: `{"name": "sage-opus-nightly1",
# "toolUseId": "toolu_01C3…", "agentType": "sage"}`. So the registry adopts it,
# at the next moment anything asks, exactly as it adopts `stoppedByUser`.
#
# WHAT THIS IS NOT. It does not certify a spawn: the guard's checks never ran
# for that call and cannot be run after the fact, so the adoption is recorded on
# the record (`registration_reconciled`) and in the event log, where a reader
# sees that this registration was repaired rather than made. It never invents an
# ending either — a registration with no provisional twin adopts an id and stays
# unfinished, because nothing has recorded that its run ended.
def _platform_binding(rec):
    """The agent the platform recorded for this unbound registration, as
    (agent_id, platform record, provisional key), or None. Reads only."""
    if not rec or rec.get("agent_id") or rec.get("provisional") or rec.get("orphan"):
        return None
    if rec.get("disposition"):
        return None
    sid = str(rec.get("session_id") or "")
    name = str(rec.get("name") or "")
    if not sid or not NAME_RE.match(name):
        return None
    tuid = str(rec.get("tool_use_id") or "")
    hits = [(aid, d) for aid, d in platform_agent_records(sid)
            if (str(d.get("toolUseId") or "") == tuid if tuid
                else str(d.get("name") or "") == name)]
    # Nothing recorded, or more than one record answering to it: a reconciliation
    # is an adoption of a recorded fact, never a choice between candidates.
    if len(hits) != 1:
        return None
    aid, meta = hits[0]
    if not tuid and str(meta.get("name") or "") != name:
        return None
    prov_key = _provisional_key(sid, aid)
    bound_to = key_for_id(aid)
    if bound_to and bound_to not in (rec["key"], prov_key):
        return None             # that id already belongs to another registration
    return aid, meta, prov_key


def platform_view(rec):
    """WHAT THE PLATFORM HAS RECORDED, SEEN WITHOUT ADOPTING IT (hunt part 4 v2,
    V2-05). A COPY of `rec` with the binding and the ending the platform wrote
    in its own per-agent records and the registry has not taken yet, exactly as
    observe_platform_binding and observe_platform_end would take them, and
    nothing written: no save, no event, no released hold, no ref observation.

    The dry status (finding 29) rightly stopped adopting these facts, and then
    asked finished_state of the stale record instead, so a worker the user had
    stopped printed WORKING with "pending: none" until some live call adopted
    the stop. A question may READ what the platform recorded; only acting on
    it writes."""
    if not rec:
        return rec
    view = json.loads(json.dumps(rec))
    bound = _platform_binding(view)
    if bound:
        aid, meta, prov_key = bound
        view["agent_id"] = aid
        view["tool_use_id"] = view.get("tool_use_id") or str(meta.get("toolUseId") or "")
        view["subagent_type"] = view.get("subagent_type") or str(meta.get("agentType") or "")
        prov = load_agent(prov_key)
        if prov and prov.get("key") != view.get("key"):
            # _add_workspace remembers the repository and binds a body of
            # work, both writes, so the provisional's workspaces join the COPY
            # by hand.
            have = set(w.get("path") for w in view.get("workspaces") or [] if w.get("path"))
            for w in live_workspaces(prov):
                if not w.get("path") or w["path"] not in have:
                    view.setdefault("workspaces", []).append(dict(w))
            for f in ("started_at", "end", "handed_in"):
                if prov.get(f) and not view.get(f):
                    view[f] = prov[f]
    ending = _platform_ending(view)
    if ending:
        field, signal_name, why = ending
        view["end"] = {"at": now(), "signal": signal_name, "detail": why, "source": field}
        view["pause"] = None
    return view


def observe_platform_binding(rec):
    """Point 3 + point 11: the agent a registration became, taken from the
    platform's own record of the spawn when nothing bound it at the time."""
    bound = _platform_binding(rec)
    if not bound:
        return rec
    aid, meta, prov_key = bound
    sid = str(rec.get("session_id") or "")
    name = str(rec.get("name") or "")
    with Lock():
        fresh = load_agent(rec["key"]) or rec
        if fresh.get("agent_id") or fresh.get("disposition"):
            return fresh
        fresh["agent_id"] = aid
        fresh["tool_use_id"] = fresh.get("tool_use_id") or str(meta.get("toolUseId") or "")
        fresh["subagent_type"] = fresh.get("subagent_type") or str(meta.get("agentType") or "")
        prov = _absorb_provisional(fresh, prov_key)
        fresh["registration_reconciled"] = {
            "at": now(), "agent_id": aid, "source": "platform-record",
            "adopted": prov_key if prov else "",
            "why": "the spawn was never bound to an agent id; the platform's own record of this "
                   "session names %s as the agent spawned for %r (point 3)" % (aid, name)}
        save_agent(fresh)
    event("reconciled", key=fresh["key"], agent_id=aid, source="platform-record",
          adopted=prov_key if prov else "", tool_use_id=fresh.get("tool_use_id") or "")
    _on_start(fresh)
    return fresh


def record_handed_in(session_id, agent_id="", name=""):
    with Lock():
        rec = _record_for_agent(session_id, agent_id, name)
        if not rec:
            return None
        rec["handed_in"] = now()
        save_agent(rec)
    event("handed-in", key=rec["key"])
    attach_proof_reports(rec["key"], "handed-in")
    return rec


# THE HANDOVER'S PROOF REPORT (verification reliability PRD 2026-10-20, step 2).
#
# Nothing an agent handed over said which of its selected checks had run: the round 32 sidebar
# was handed over after its authors ran four suites of 78, and the land found the two stale
# checks one at a time. The runner already keeps every run's plan, outcomes and summary in this
# checkout's proof-run history, and the plan names the range it was selected from. So when the
# platform records a handover (TaskCompleted) or a normal end (SubagentStop), each workspace
# record gets a POINTER to the newest saved run of that checkout's current HEAD, with the
# runner's own counts and every check that did not pass, by name and state. It is a report,
# never proof: whoever integrates validates the saved evidence afresh (the runner's input
# identities, reuse()). Reading it is bounded (at most PROOF_REPORT_RUNS saved runs, no
# hashing, no history scan) and comes after the lifecycle fact is saved, so a missing,
# unreadable or slow report can never delay or prevent that fact.
PROOF_REPORT_RUNS = 5
PROOF_REPORT_SECONDS = 3.0
PROOF_REPORT_HELD_SCAN = 20
PROOF_RUN_NAME = re.compile(r"\d{8}T\d{6}Z(-[A-Za-z0-9_]+)?")


def _proof_history(path):
    """This checkout's proof-run history directory, as proof-run.py default_logdir names it."""
    here = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "scripts", "lib")
    if here not in sys.path:
        sys.path.insert(0, here)
    import machine_settings
    base = str(machine_settings.state("proof-runs", "RICHOS_PROOF_RUN_DIR"))
    return os.path.join(base, hashlib.sha256(path.encode()).hexdigest()[:12])


def proof_report(path, deadline=None):
    """The pointer for the checkout at `path` (above), or {"unavailable": why}.

    EVERY READ IS BOUNDED BY WHAT IS LEFT OF THE SHARED DEADLINE (review
    rv-20261020T044017Z-ebbb640b-fc3b #2): the HEAD read gets only the time left, and the time is
    asked again after each file read, so a slow read can spend the budget but never turn an
    overrun into a report. A report that ran out says so; the lifecycle fact was saved before."""
    deadline = deadline or (time.monotonic() + PROOF_REPORT_SECONDS)
    spent = "the report's time budget ran out"
    try:
        env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        if time.monotonic() >= deadline:
            return {"unavailable": spent}
        try:
            head = subprocess.run(["git", "-C", path, "rev-parse", "HEAD"], capture_output=True, text=True,
                                  timeout=max(0.05, deadline - time.monotonic()), stdin=subprocess.DEVNULL, env=env)
        except subprocess.TimeoutExpired:
            return {"unavailable": spent}
        if head.returncode != 0:
            return {"unavailable": "cannot read the workspace's HEAD"}
        head = head.stdout.strip()
        history = _proof_history(path)
        try:
            names = sorted((n for n in os.listdir(history) if PROOF_RUN_NAME.fullmatch(n)), reverse=True)
        except FileNotFoundError:
            return {"unavailable": "no saved proof run for this checkout", "head": head}
        if time.monotonic() > deadline:
            return {"unavailable": spent, "head": head}
        for name in names[:PROOF_REPORT_RUNS]:
            if time.monotonic() > deadline:
                return {"unavailable": spent, "head": head}
            run = os.path.join(history, name)
            try:
                with open(os.path.join(run, "plan.json")) as stream:
                    plan = json.load(stream)
            except (OSError, ValueError):
                continue
            if time.monotonic() > deadline:
                return {"unavailable": spent, "head": head}
            source = plan.get("source") or {}
            if plan.get("root") != path or source.get("commit") != head:
                continue
            selection = plan.get("selection") or {}
            report = {"run": run, "head": head, "candidate": selection.get("candidate"),
                      "base": selection.get("base"), "mode": selection.get("mode"),
                      "arguments": selection.get("arguments") or selection.get("commands"),
                      "planned": len(plan.get("items") or [])}
            try:
                with open(os.path.join(run, "summary.json")) as stream:
                    summary = json.load(stream)
            except (OSError, ValueError):
                report["unavailable"] = "the run has no readable summary (still running, or ended without one)"
                return report
            if time.monotonic() > deadline:
                report["unavailable"] = spent
                return report
            # Held for its consumer (proof-run.py HANDOVER_HOLD): rotation never removes it, and the
            # land that consumes it releases it. Any other held run of this checkout is named, so a
            # superseded handover is released by Rich once nothing still needs it.
            # The scan is bounded twice (review rv-20261020T073921Z-1e0461cc-0956 #3): at most
            # PROOF_REPORT_HELD_SCAN runs, and the deadline is asked before every read and after
            # the last one, so a slow disk can spend the budget but never turn it into a report.
            report["held"] = os.path.exists(os.path.join(run, "retain"))
            others = []
            for n in [n for n in names if n != name][:PROOF_REPORT_HELD_SCAN]:
                if time.monotonic() > deadline:
                    break
                if os.path.exists(os.path.join(history, n, "retain")):
                    others.append(os.path.join(history, n))
            if time.monotonic() > deadline:
                report["unavailable"] = spent
                return report
            if others:
                report["also_held"] = others[:PROOF_REPORT_RUNS]
            rows = summary.get("checks") or []
            counts = {}
            for row in rows:
                counts[row.get("result")] = counts.get(row.get("result"), 0) + 1
            reused = sum(1 for row in rows if row.get("reused_from"))
            # A KNOWN-RED unit is a declared exception the runner lets through (proof-run.py
            # known_red), never a pass: it is named with the checks that are not complete.
            unresolved = [{"check": row.get("check"), "result": row.get("result"),
                           "why": "known-red, a declared exception" if row.get("known_red")
                           else (row.get("not_run") or {}).get("why")}
                          for row in rows if row.get("result") != "passed" or row.get("known_red")]
            known_red = sum(1 for row in rows if row.get("known_red"))
            report.update(outcomes=counts, reused=reused, unresolved=unresolved,
                          crashed=summary.get("crashed"))
            report["line"] = ("%d planned, %d passed (%d of them reused)%s" % (
                report["planned"], counts.get("passed", 0) - known_red, reused,
                "; NOT complete: " + ", ".join("%s (%s)" % (u["check"], u["why"] or u["result"])
                                               for u in unresolved[:20])
                + (" and %d more" % (len(unresolved) - 20) if len(unresolved) > 20 else "")
                if unresolved else "; every planned check passed"))
            if selection.get("candidate") and selection["candidate"] != head:
                report["line"] += "; the plan's candidate %s is not HEAD" % selection["candidate"][:12]
            if report.get("also_held"):
                report["line"] += ("; %d earlier handover run(s) of this checkout still held (also_held): "
                                   "release each once nothing needs it, proof-run.py --release <run>"
                                   % len(others))
            return report
        return {"unavailable": "no saved proof run of HEAD %s" % head[:12], "head": head}
    except Exception as exc:  # noqa: BLE001  (a report never breaks the lifecycle fact it follows)
        return {"unavailable": "%s: %s" % (type(exc).__name__, exc)}


def attach_proof_reports(key, why):
    """Attach proof_report() to each live workspace of record `key`; never raises."""
    try:
        rec = load_agent(key)
        if not rec:
            return
        deadline = time.monotonic() + PROOF_REPORT_SECONDS
        reports = {w["path"]: dict(proof_report(w["path"], deadline), at=iso(), on=why)
                   for w in live_workspaces(rec) if w.get("path")}
        if not reports:
            return
        with Lock():
            rec = load_agent(key)
            if not rec:
                return
            for w in rec.get("workspaces") or []:
                if w.get("path") in reports:
                    w["proof_report"] = reports[w["path"]]
            save_agent(rec)
        event("proof-report", key=key, on=why,
              detail="; ".join("%s: %s" % (os.path.basename(p), r.get("line") or r.get("unavailable"))
                               for p, r in sorted(reports.items()))[:600])
    except Exception:  # noqa: BLE001
        return


def _resolve(ref, session_id=""):
    """A record by key, by agent id, or by name (this session first, then any
    pending item this session may handle), with any binding and any ending the
    platform has recorded for it since taken (points 3, 11)."""
    return observe_platform_end(observe_platform_binding(_resolve_record(ref, session_id)))


def _resolve_record(ref, session_id=""):
    if not ref:
        raise SpecError("name an agent")
    r = load_agent(ref)
    if r:
        return r
    k = key_for_id(ref)
    if k:
        return load_agent(k)
    if session_id:
        r = load_agent(named_key(session_id, ref))
        if r:
            return r
    hits = [a for a in all_agents() if a.get("name") == ref]
    if len(hits) == 1:
        return hits[0]
    if not hits:
        raise SpecError("no registered agent %r" % ref)
    raise SpecError("%r names %d agents; use a key: %s" % (ref, len(hits), ", ".join(h["key"] for h in hits)))


def _agent_hold():
    """scripts/lib/agent_hold.py, with its state beside a fixture registry's (tests)."""
    here = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "scripts", "lib")
    if here not in sys.path:
        sys.path.insert(0, here)
    if (os.environ.get("RICHOS_WORKSPACES_DIR") or "").strip() and not os.environ.get("RICHOS_AGENT_HOLD_DIR"):
        os.environ["RICHOS_AGENT_HOLD_DIR"] = _p("agent-hold")
    import agent_hold
    return agent_hold


def _generated(kind, text):
    """Is `text` the unchanged generated pause (kind "pause") or RESUME (kind "resume")?"""
    try:
        _agent_hold()                  # puts scripts/lib on the path
        import pause_protocol
        return (pause_protocol.is_generated_pause if kind == "pause" else pause_protocol.is_generated_resume)(text)
    except Exception:
        return False


def _generated_end(text):
    """A generated pause's checkable end (a quota reset), or ''."""
    try:
        _agent_hold()
        import pause_protocol
        value = pause_protocol.validate_text(text)
        return pause_protocol.end_of(value["reason"], value["reset"])
    except Exception:
        return ""


def hold_running_work(rec, notes=None):
    """A pause takes effect at once: the agent's running commands are suspended and its
    new ones wait (agent_hold.py). The message alone reaches it only at its next tool
    call. Never raises: a hold that fails is reported, and the pause stands."""
    try:
        ah = _agent_hold()
        try:
            spid = session_pid(rec.get("session_id") or "")
        except Exception:
            spid = None
        res = ah.hold(str(rec.get("session_id") or ""), str(rec.get("agent_id") or ""), rec.get("name") or "",
                      session_pid=spid)
        line = ah.describe_hold(res)
        event("hold", key=rec["key"], held=len(res.get("held") or {}), ok=res.get("ok"),
              stopped_seconds=res.get("stopped_seconds"), cpu_during_sample=res.get("cpu_during_sample"),
              excluded=res.get("excluded") or None, why=res.get("why"))
    except Exception as e:
        line = "HOLD %s: FAILED (%s); its running work was not suspended." % (rec.get("name") or "?", str(e)[:200])
        event("hold-failed", key=rec.get("key"), why=str(e)[:200])
    if notes is not None:
        notes.append(line)
    return line


def release_held_work(rec, notes=None):
    """RESUME, a stop and every other end of a pause continue what the hold suspended.
    Never raises; a release that fails is reported."""
    return release_held_ids(str(rec.get("session_id") or ""), str(rec.get("agent_id") or ""),
                            rec.get("name") or "", rec.get("key"), notes)


def release_held_ids(session_id, agent_id, name="", key=None, notes=None):
    """The same release, by the ids alone: a stop names an agent id whether or not the
    registry still holds a live record for it (Sage's catch 1)."""
    try:
        ah = _agent_hold()
        res = ah.release(str(session_id or ""), str(agent_id or ""))
        line = ah.describe_release(res)
        if line and res.get("ok") is False:
            event("release-failed", key=key, agent_id=agent_id, why=str(res.get("why"))[:200],
                  continued=len(res.get("continued") or []))
        elif line:
            event("release", key=key, agent_id=agent_id, continued=len(res.get("continued") or []),
                  waited=len(res.get("waited") or []), gone=len(res.get("gone") or []))
    except Exception as e:
        line = "RELEASE %s: FAILED (%s); its held work may still be suspended: agent_hold.py status." % (
            name or agent_id or "?", str(e)[:200])
        event("release-failed", key=key, agent_id=agent_id, why=str(e)[:200])
    if notes is not None and line:
        notes.append(line)
    return line


def end_owned_work(session_id, agent_id, name="", key=None, why=""):
    """An agent whose run has ended leaves no command of its own running
    (2026-10-11, echo-opus-walk49fix1's four endless wait loops): agent_hold's
    end_owned(), with the hold's exclusions, logged there and here. Never raises."""
    try:
        ah = _agent_hold()
        res = ah.end_owned(str(session_id or ""), str(agent_id or ""), name, why)
    except Exception as e:
        event("end-owned-failed", key=key, agent_id=agent_id, why=str(e)[:200])
        return None
    if res.get("ended") or res.get("survivors"):
        event("ended-owned-work", key=key, agent_id=agent_id, why=why,
              ended=["%d: %s" % (e["pid"], (e.get("command") or "")[:80]) for e in res["ended"]],
              survivors=res.get("survivors") or [], notes=res.get("notes") or [])
    return res


def release_session_holds(session_id, notes=None):
    """No work stays suspended after its session: at SessionEnd every hold of that session,
    and at SessionStart (session_id None) every hold whose session process is gone."""
    try:
        ah = _agent_hold()
        if not ah.records():
            return
        results = ah.release_session(session_id) if session_id else ah.release_orphans()
        for res in results:
            line = ah.describe_release(res)
            if line:
                event("release-failed" if res.get("ok") is False else "release",
                      continued=len(res.get("continued") or []), gone=len(res.get("gone") or []),
                      why=("session ended" if session_id else "session process gone")
                      + ("; " + str(res.get("why"))[:200] if res.get("ok") is False else ""))
                if notes is not None:
                    notes.append(line + " (its session ended)")
    except Exception as e:
        event("release-failed", why=str(e)[:200])
        if notes is not None:
            notes.append("RELEASE after a session ended FAILED (%s): agent_hold.py status." % str(e)[:200])


def pause(ref, until, session_id="", notes=None, control=False, end=""):
    """`control`: the pause came as the unchanged generated message (pause_protocol.py).
    Only the generated RESUME, a resume command, a stop or the session's end ends
    such a pause; any other message leaves it waiting (Sage's catch 3).
    A pause is a hold (HOLDS): `end` is its checkable end (a quota pause's
    at:<reset>); else an `until` written as one; else it has none."""
    end = hold_end(end) if end else hold_end(until, strict=False)
    with Lock():
        rec = _resolve(ref, session_id)
        fin, _p_, why = finished_state(rec)
        if fin:
            raise SpecError("%s is already finished (%s); a finished agent is not paused" % (rec["name"], why))
        rec["pause"] = {"at": now(), "until": (until or "").strip(), "control": bool(control), "end": end}
        save_agent(rec)
    event("pause", key=rec["key"], until=until, control=bool(control), end=end)
    hold_running_work(rec, notes)
    return rec


def resume(ref, session_id="", notes=None):
    with Lock():
        rec = _resolve(ref, session_id)
        fin, paused_, why = finished_state(rec)
        if fin:
            raise SpecError("%s is finished (%s); it cannot be resumed. Land or discard it, or continue "
                            "its work with a new agent (point 7)." % (rec["name"], why))
        if paused_:
            rec.setdefault("history", []).append({"at": iso(), "fact": "resumed", "pause": rec.get("pause"),
                                                  "end": rec.get("end")})
            rec["pause"] = None
            rec["end"] = None
            rec["handed_in"] = None
            save_agent(rec)
    if paused_:
        event("resume", key=rec["key"])
    # Also when the registry had no pause: a hold left by any path is released here.
    release_held_work(rec, notes)
    return rec


def stop(ref, why, session_id="", notes=None):
    """Point 11: a pause whose work is no longer wanted ends by stopping it."""
    with Lock():
        rec = _resolve(ref, session_id)
        rec["end"] = {"at": now(), "signal": "stopped", "detail": why or "stopped by Rich"}
        rec["pause"] = None
        save_agent(rec)
    # A suspended process holds a pending termination forever: continue it first.
    release_held_work(rec, notes)
    # Then end what it owns: the release just continued a paused agent's commands,
    # and a stop leaves none of them running (second review
    # rv-20261011T042129Z-6b0f0810-1c93: the app's Stop resumed them instead).
    end_owned_work(str(rec.get("session_id") or ""), str(rec.get("agent_id") or ""),
                   rec.get("name") or "", rec.get("key"), "its run ended (stopped)")
    observe_created_refs(rec, all_open=True)
    event("end", key=rec["key"], signal="stopped", detail=why)
    return rec


def wait(ref, kind, on, todo="", session_id="", until=""):
    """Point 5's allowances for ending a turn, recorded by Rich. Every wait is a
    hold (see HOLDS below): `until` is its checkable end, and without one the
    free text keeps nothing quiet."""
    if kind not in ("started", "outside", "ceo-discard", "held"):
        raise SpecError("unknown wait kind %r" % kind)
    if not (on or "").strip():
        raise SpecError("say what it is waiting on")
    if kind in ("outside", "ceo-discard") and not (todo or "").strip():
        raise SpecError("an item waiting on something outside Rich's reach goes on the CEO's TODO list; "
                        "pass --todo with where it is (point 5)")
    end = hold_end(until)
    with Lock():
        rec = _resolve(ref, session_id)
        if kind == "ceo-discard" and not rec.get("ceo_ordered"):
            raise SpecError("%s was not recorded as ordered by the CEO, so its discard does not need his word"
                            % rec["name"])
        # A REPEATED WAIT KEEPS ITS AGE (second review rv-20261022T002958Z-2ac46e09-fa4e,
        # finding 2): the work has been held since the first one, until the hold ends.
        held_since = (rec.get("waiting") or {}).get("at") or now()
        rec["waiting"] = {"kind": kind, "on": on.strip(), "todo": (todo or "").strip(), "at": held_since,
                          "end": end}
        save_agent(rec)
    event("wait", key=rec["key"], kind=kind, on=on, todo=todo, end=end)
    return rec


# ---------------------------------------------------------------------------
# HOLDS: a hold on his work stays in sight until it runs, lands or he decides
# ---------------------------------------------------------------------------
# The CEO, 2026-10-22, after his 2026-09-27 order sat held for weeks: "WHEN WILL
# [THIS] STOP ONCE AND FOR ALL???" Plan: richos-hq docs/plans/2026-10-22-no-
# silent-holds.md. Every hold counts as a hold on his work (nothing marks his
# orders). A hold is:
#   - a finished agent's `waiting` (workspaces.sh wait), or a live one's
#     `waiting` of kind "held" (workspaces.sh hold <agent>; legacy records also
#     came from a message's words, which since 2026-10-24 record nothing);
#   - an agent's `pause`;
#   - holds/<name>.json, for held work with no teammate yet (workspaces.sh
#     hold). A reply's words never open one (the deferral check only asks the
#     lead; legacy `by: deferral` files are read like any other).
# Only an END the engine can check keeps it quiet, from a fixed list:
#   agent:<name>   a live teammate does it      review:<id>  a second review runs
#   at:<UTC time>  e.g. a quota reset            ceo:<item>   his decision, on CEO-TODOs
# Free text is the WHY and keeps nothing quiet: a hold with no end is one line
# at every turn end (gate_stop) once it is 30 min old, and a stall-watch wake
# every 30 min; an end that comes true wakes Rich once (RELEASE DUE).
# Three ways it ends: the work runs (a live `agent:`, a continuation spawn,
# the agent resumed), it is done (landed or discarded), or his words
# (`release --ceo-word`). `release --not-a-hold` takes back a false catch and
# is shown to him once at the next turn end.
HOLD_END_KINDS = ("agent", "review", "at", "ceo")
HOLD_REPORT_SECONDS = 30 * 60
HOLD_LINES = 6
REVIEW_RUN_SECONDS = 90 * 60


def _hold_go(text):
    """The go that ends a legacy `held` wait (scripts/lib/hold_words.py)."""
    _agent_hold()                        # puts scripts/lib on the path
    import hold_words
    return hold_words.is_go(text)


def _holds_dir():
    return _p("holds")


def _hold_path(name):
    return os.path.join(_holds_dir(), _key_segment(name) + ".json")


def hold_end(text, strict=True):
    """The normalized end ('kind:value') of `text`, or '' for none. Strict (the
    commands): anything given that is not on the list is refused. Lenient (a
    pause's free `until` text): what is not on the list is no end."""
    t = (text or "").strip()
    if not t:
        return ""
    kind, _, value = t.partition(":")
    kind, value = kind.strip().lower(), value.strip()
    ok = kind in HOLD_END_KINDS and value
    if ok and kind == "at":
        value = _hold_time(value)
        ok = bool(value)
    if ok:
        return "%s:%s" % (kind, value)
    if strict:
        raise SpecError("a hold's end is one of agent:<name>, review:<id>, at:<UTC time, e.g. "
                        "2026-10-22T23:00:00Z or 23:00Z>, ceo:<CEO-TODOs item>; %r is none of them. Say why it "
                        "is held with --why/--started; free text keeps nothing quiet" % t)
    return ""


def _hold_time(value):
    """An ISO UTC time, or HH:MMZ taken as its occurrence nearest to now; '' if neither."""
    v = value.strip().upper()
    m = re.fullmatch(r"(\d{4}-\d{2}-\d{2})T(\d{2}):(\d{2})(?::(\d{2}))?Z", v)
    if m:
        try:
            t = calendar.timegm(time.strptime("%sT%s:%s:%s" % (m.group(1), m.group(2), m.group(3),
                                                                 m.group(4) or "00"), "%Y-%m-%dT%H:%M:%S"))
        except ValueError:
            return ""
        return iso(t)
    m = re.fullmatch(r"([01]\d|2[0-3]):([0-5]\d)Z", v)
    if not m:
        return ""
    t0 = now()
    day = calendar.timegm(time.gmtime(t0)[:3] + (0, 0, 0, 0, 0, 0))
    t = day + int(m.group(1)) * 3600 + int(m.group(2)) * 60
    t = min((t - 86400, t, t + 86400), key=lambda c: abs(c - t0))
    return iso(t)


def _review_ended(review_id):
    """None while review <id> runs; else why it counts as ended."""
    base = (os.environ.get("SECOND_REVIEW_STATE_DIR") or "").strip() or os.path.join(
        (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() or os.path.join(os.path.expanduser("~"), ".claude"),
        "state")
    try:
        with open(os.path.join(base, "reviews.jsonl"), encoding="utf-8") as f:
            for line in f:
                if review_id in line and ('"%s"' % review_id) in line:
                    return "review %s has its verdict" % review_id
    except OSError:
        pass
    d = os.path.join(base, "reviews", _key_segment(review_id))
    try:
        newest = max([os.path.getmtime(os.path.join(d, n)) for n in os.listdir(d)] + [os.path.getmtime(d)])
    except OSError:
        return "no review %s is running" % review_id
    if now() - newest > REVIEW_RUN_SECONDS:
        return "review %s wrote nothing for %d min and has no verdict" % (review_id, (now() - newest) // 60)
    return None


def hold_end_state(end, at=None, cache=None):
    """('none' | 'pending' | 'due' | 'ceo', why) of a hold's end, at time `at`."""
    if not end:
        return "none", "nothing checkable ends it"
    kind, _, value = end.partition(":")
    if kind == "ceo":
        # HIS ONLY WHEN IT IS ON HIS LIST (second review rv-20261022T002958Z-2ac46e09-fa4e,
        # finding 3): the reference must name an item the CEO-TODOs page shows as waiting
        # on him. Any other reference keeps nothing quiet.
        item = _ceo_item(value)
        if item:
            return "ceo", "his decision (CEO-TODOs %s)" % item
        return "none", "%s names no item waiting on him in CEO-TODOs" % value
    if kind == "at":
        t = calendar.timegm(time.strptime(value, "%Y-%m-%dT%H:%M:%SZ"))
        if (now() if at is None else at) >= t:
            return "due", "its time %s has passed" % value
        return "pending", "until %s" % value
    if kind == "review":
        why = _review_ended(value)
        return ("due", why) if why else ("pending", "review %s is running" % value)
    live = [r for r in all_agents() if r.get("name") == value and not r.get("disposition")]
    if any(not finished_state(r, cache)[0] for r in live):
        return "pending", "agent %s is working on it" % value
    return "due", ("agent %s has finished" % value) if live else ("no agent %s is registered" % value)


def _ceo_items():
    """The ids of the items the CEO-TODOs page shows waiting on him (`### <id> - <title>`),
    from every known repository whose `.ceo-todos` declares a TODO_VIEW, and
    RICHOS_CEO_TODOS_VIEW (a page path) where set. Only ids: the page is generated from
    the record with the CEO's own sections only (scripts/lib/ceo-todos.py)."""
    pages = [p for p in (os.environ.get("RICHOS_CEO_TODOS_VIEW") or "").split(os.pathsep) if p]
    for repo in known_repos():
        try:
            with open(os.path.join(repo, ".ceo-todos"), encoding="utf-8") as f:
                decl = f.read()
        except OSError:
            continue
        m = re.search(r'(?m)^TODO_VIEW="?([^"\n]+)"?\s*$', decl)
        if m:
            pages.append(os.path.join(repo, m.group(1).strip()))
    ids = set()
    for page in pages:
        try:
            with open(page, encoding="utf-8") as f:
                ids.update(re.findall(r"(?m)^###\s+(\S+)\s+[\u2014-]", f.read()))
        except OSError:
            continue
    return ids


def _ceo_item(ref):
    """The CEO-TODOs item a reference names ('CEO-TODOs 2.6', '2.6'), when it is on his page; else ''."""
    ids = _ceo_items()
    for token in re.findall(r"\d+(?:\.\d+)*", ref or ""):
        if token in ids:
            return token
    return ""


def _in_scope(rec_session, claimed, me, cache):
    """A hold is this session's to report when it was made here, claimed here,
    or its session is not running (ended or unknown: told, never dropped)."""
    if not me or rec_session == me or claimed == me or not rec_session:
        return True
    return session_state(rec_session, None, cache)[0] != "running"


def _continued_by(rec, cache):
    """A live agent doing this record's work (a continuation), or ''."""
    for a in all_agents():
        if a.get("key") == rec.get("key"):
            continue
        if rec["key"] in (a.get("continues") or []) or rec.get("name") in (a.get("lands_pending") or []):
            if not finished_state(a, cache)[0]:
                return a.get("name") or a["key"]
    return ""


def holds(me="", at=None, cache=None):
    """Every hold this session must keep in sight: a list of dicts with name,
    what, why, since (epoch), end, state, detail, source, key."""
    cache = {} if cache is None else cache
    if (os.environ.get("RICHOS_APP_ENTITY") or "").strip() and (os.environ.get("RICHOS_APP_THREAD") or "").strip():
        # THE APP'S REGISTRY IS ONE CONVERSATION'S OWN PARTITION (engine_profile.rs
        # workspace_state), so every hold in it is this conversation's, Rich's lease's
        # and Stu's alike: Stu's watcher must not drop a hold of Rich's running lease
        # (second review rv-20261022T013056Z-12c77c90-1291, finding 4).
        me = ""
    out = []
    try:
        names = sorted(os.listdir(_holds_dir()))
    except OSError:
        names = []
    for n in names:
        h = read_json(os.path.join(_holds_dir(), n)) if n.endswith(".json") else None
        if not h or h.get("released") or not _in_scope(h.get("session_id"), "", me, cache):
            continue
        st, why = hold_end_state(h.get("end") or "", at, cache)
        out.append({"name": h.get("name"), "what": h.get("what") or h.get("name"), "why": h.get("why") or "",
                    "since": float(h.get("at") or now()), "end": h.get("end") or "", "state": st,
                    "detail": why, "source": "hold", "key": ""})
    reviews = None
    for rec in all_agents():
        if rec.get("disposition") or not _in_scope(rec.get("session_id"), rec.get("claimed_by"), me, cache):
            continue
        fin, paused_, _why = finished_state(rec, cache)
        w, p = rec.get("waiting") or {}, rec.get("pause") or {}
        name = rec.get("name") or rec["key"]
        found = []                       # (source, what, why, since, end)
        # A PAUSE OUTLIVES ITS SESSION (second review rv-20261022T013056Z-12c77c90-1291,
        # finding 5): once the session ends the agent counts as finished, but the work
        # it was paused on never ran; only a hand-in after it ends the pause.
        if p and (paused_ or (fin and not rec.get("handed_in"))):
            found.append(("pause" if paused_ else "pause-ended", "the work of %s" % name,
                          p.get("until") or "(nothing named)", p.get("at"),
                          p.get("end") or hold_end(p.get("until"), strict=False)))
        elif w and (fin or w.get("kind") == "held"):
            found.append(("wait", "the work of %s" % name, w.get("on") or "", w.get("at"),
                          w.get("end") or ("ceo:" + w["todo"] if w.get("todo") else "")))
        if fin:
            tips = [(repo, t) for repo, b in sorted(_branch_targets([rec])) for t in [branch_tip(repo, b)] if t]
            # A REVIEW ASKING FOR CHANGES ON THE TIP IT HANDED OVER (a mid-job or handover
            # CHANGES-REQUESTED verdict, then a hand-over without the fix).
            if tips and reviews is None:
                reviews = _reviews_by_tip()
            for _repo, t in tips:
                row = (reviews or {}).get(t)
                if row and row.get("verdict") == "changes-requested" \
                        and row.get("id") not in (rec.get("reviews_released") or []):
                    found.append(("review", "the fixes review %s asked of %s" % (row.get("id"), name),
                                  "it handed over %s without them (%s finding(s))" % (t[:10], row.get("findings")),
                                  parse_iso_epoch(row.get("finished_at")) or now(), "", ))
                    found[-1] = found[-1] + (row.get("id"),)
        if not found or _continued_by(rec, cache):
            continue                     # nothing held, or the work runs: a continuation does it
        for f in found:
            src, what, why, since, end = f[:5]
            st, detail = hold_end_state(end, at, cache)
            out.append({"name": name, "what": what, "why": why, "since": float(since or now()), "end": end,
                        "state": st, "detail": detail, "source": src, "key": rec["key"],
                        "review": f[5] if len(f) > 5 else ""})
    return out


def _commit_time(repo, tip):
    rc, out, _e = git(repo, "log", "-1", "--format=%ct", tip)
    try:
        return float(out.strip()) if rc == 0 else 0.0
    except ValueError:
        return 0.0


def parse_iso_epoch(text):
    try:
        return float(calendar.timegm(time.strptime(str(text), "%Y-%m-%dT%H:%M:%SZ")))
    except (TypeError, ValueError):
        return None


def _reviews_by_tip():
    """{tip: the newest second-review verdict row for it} from second-review's ledger."""
    base = (os.environ.get("SECOND_REVIEW_STATE_DIR") or "").strip() or os.path.join(
        (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() or os.path.join(os.path.expanduser("~"), ".claude"),
        "state")
    out = {}
    try:
        with open(os.path.join(base, "reviews.jsonl"), encoding="utf-8") as f:
            for line in f:
                try:
                    r = json.loads(line)
                except ValueError:
                    continue
                if isinstance(r, dict) and r.get("tip") and r.get("verdict"):
                    prev = out.get(r["tip"])
                    if prev is None or str(r.get("finished_at") or "") >= str(prev.get("finished_at") or ""):
                        out[r["tip"]] = r
    except OSError:
        pass
    return out


def hold_command(h):
    """What makes held work run: the one command for Rich."""
    if h["source"] == "pause":
        return "workspaces.sh resume %s" % h["name"]
    # Anything else, a pause whose session ended included: its run cannot resume.
    return "spawn the teammate that does it with the line  continues: %s" % h["name"]


def _span(seconds):
    m = int(max(0, seconds) // 60)
    return "%d h %d min" % (m // 60, m % 60) if m >= 60 else "%d min" % m


def hold_lines(me="", at=None):
    """The turn-end lines: one per hold with no checkable end older than 30 min,
    at most six and then '+N', plus each `--not-a-hold` release once."""
    t = now() if at is None else at
    quiet = float(os.environ.get("RICHOS_HOLD_REPORT_SECONDS") or HOLD_REPORT_SECONDS)
    loud = [h for h in holds(me, t) if h["state"] == "none" and t - h["since"] >= quiet]
    loud.sort(key=lambda h: h["since"])
    lines = ["HELD BY RICH, NOT BY YOU: %s since %s (%s). Why: %s. Your call: go, or hold until a date or event."
             % (h["what"], time.strftime("%H:%MZ", time.gmtime(h["since"])), _span(t - h["since"]),
                " ".join((h["why"] or "not said").split())[:200].rstrip(".")) for h in loud[:HOLD_LINES]]
    if len(loud) > HOLD_LINES:
        lines.append("+%d more held: workspaces.sh holds" % (len(loud) - HOLD_LINES))
    try:
        names = sorted(os.listdir(_holds_dir()))
    except OSError:
        names = []
    for n in names:
        p = os.path.join(_holds_dir(), n)
        h = read_json(p) if n.endswith(".json") else None
        r = (h or {}).get("released") or {}
        if not r.get("not_a_hold") or not _in_scope(h.get("session_id"), "", me, {}):
            continue
        lines.append("RELEASED AS NOT A HOLD: %s. Rich's reason: %s." % (h.get("what") or h.get("name"),
                                                                          r["not_a_hold"]))
        try:
            os.unlink(p)                 # shown once
        except OSError:
            pass
    return lines


def hold(name, what, until="", why="", session_id="", by="rich"):
    """`workspaces.sh hold`: held work with no teammate yet. A name that is a live
    registered agent records the hold on that agent instead (kind held)."""
    if not NAME_RE.match(name or ""):
        raise SpecError("a hold needs a name (letters, digits, . _ -): %r" % name)
    if not (what or "").strip():
        raise SpecError("say what work is held (--what)")
    end = hold_end(until)
    try:
        rec = _resolve_record(name, session_id)
    except SpecError:
        rec = None
    if rec and not rec.get("disposition"):
        return wait(rec["key"], "held", (why or what).strip(), "", session_id, end)
    with Lock():
        os.makedirs(_holds_dir(), exist_ok=True)
        prev = read_json(_hold_path(name)) or {}
        h = {"name": name, "what": what.strip(), "why": (why or "").strip(), "end": end,
             "at": prev.get("at") if prev and not prev.get("released") else now(),
             "session_id": session_id, "by": by}
        write_json(_hold_path(name), h)
    event("hold-opened", name=name, work=what, end=end, why=why, by=by)
    return h


def release(name, ceo_word="", not_a_hold="", session_id=""):
    """The three ends of a hold that a command makes: his words, a false catch
    taken back (shown to him once), or the work running. Rich saying so is none."""
    found = [h for h in holds("", None) if h["name"] == name]
    if not found:
        raise SpecError("no hold named %r (workspaces.sh holds lists them)" % name)
    for h in found[1:]:
        _release_one(h, name, ceo_word, not_a_hold, session_id)
    return _release_one(found[0], name, ceo_word, not_a_hold, session_id)


def _release_one(h, name, ceo_word, not_a_hold, session_id):
    runs = h["state"] == "pending" and h["end"].startswith(("agent:", "review:"))
    if not (ceo_word or "").strip() and len((not_a_hold or "").strip()) < 10 and not runs:
        raise SpecError("%s is held work he ordered, and Rich cannot end a hold by saying so. It ends when "
                        "the work runs (%s), when it lands or is discarded, or by his words: --ceo-word "
                        "'<his words>'. A false catch is --not-a-hold '<why it is not a hold>'."
                        % (name, hold_command(h)))
    how = {"at": now(), "ceo_word": (ceo_word or "").strip() or None,
           "not_a_hold": (not_a_hold or "").strip() or None, "work_runs": h["detail"] if runs else None,
           "by_session": session_id}
    with Lock():
        if h["source"] == "hold":
            rec = read_json(_hold_path(name)) or {"name": name}
            rec["released"] = how
            if how["not_a_hold"]:
                write_json(_hold_path(name), rec)   # shown once at the next turn end, then gone
            else:
                try:
                    os.unlink(_hold_path(name))
                except OSError:
                    pass
        elif h["source"] == "review":
            rec = load_agent(h["key"])
            rec["reviews_released"] = (rec.get("reviews_released") or []) + [h["review"]]
            rec.setdefault("history", []).append({"at": iso(), "fact": "hold released", "review": h["review"],
                                                  "how": how})
            save_agent(rec)
        else:
            rec = load_agent(h["key"])
            field = {"pause": "pause", "pause-ended": "pause"}.get(h["source"], "waiting")
            rec.setdefault("history", []).append({"at": iso(), "fact": "hold released", field: rec.get(field),
                                                  "how": how})
            rec[field] = None
            save_agent(rec)
    event("hold-released", name=name, **{k: v for k, v in how.items() if k != "at"})
    if h["source"] == "pause":
        release_held_work(load_agent(h["key"]) or {"name": name})
    return how


def _hold_runs(name, by):
    """A spawn carrying `continues: <name>` for held work with no teammate: the work runs."""
    with Lock():
        h = read_json(_hold_path(name))
        if not h or h.get("released"):
            return False
        try:
            os.unlink(_hold_path(name))
        except OSError:
            pass
    event("hold-released", name=name, work_runs="spawn %s continues it" % by)
    return True


# ---------------------------------------------------------------------------
# point 5 — the pending list, and who handles which item (point 12)
# ---------------------------------------------------------------------------

def scan_unregistered(repos, record=True):
    """Point 3, hole 6: a cc/ or native workspace with no registration, and any
    such branch, is finished work of an ended session. codex/ and every other
    name is not the system's concern (points 1, 2) and is never listed.

    `record=False` only looks (finding 29): nothing is bound and no record is
    made, and what was found is returned as (repo, path, branch, kind).

    WHILE ANY RECORD IS DAMAGED, NOTHING IS UNREGISTERED (V2-04). "No
    registration" is proved by reading every registration; a record that
    cannot be read may be the one that holds this workspace, so the sweep
    finds nothing and records nothing until it is repaired. pending() names
    the damaged files instead."""
    damaged = damaged_records()
    if damaged:
        if record:
            event("registry-damaged", files=[p for p, _w in damaged])
        return []
    paths, branches = set(), set()
    # Point 14: a record that was never spawned through the guard — an orphan
    # this sweep made, or a provisional native start no spawn registered — had
    # no spawn at which to be refused for a missing record. It binds to its
    # repository's body of work at the first sweep after one exists, recorded
    # on it by id, once; until then its land refuses and names the command.
    for r in (all_agents() if record else []):
        if not (r.get("orphan") or r.get("provisional")) or r.get("disposition"):
            continue
        # ITS OWN name, never `repos`: reusing the parameter's name narrowed
        # the discovery below to the last such record's repositories, so one
        # known stray hid a new one in every other repository it was asked to
        # scan (hunt part 4, finding 7).
        its_repos = sorted(set(w.get("repo") for w in live_workspaces(r) if w.get("repo")))
        bound = r.get("integration_work") or {}
        if all(_norm_repo(x) in bound for x in its_repos):
            continue
        with Lock():
            fresh = load_agent(r["key"])
            if not fresh or fresh.get("disposition"):
                continue
            changed = False
            for x in its_repos:
                if _norm_repo(x) not in (fresh.get("integration_work") or {}) and _bind_body_of_work(fresh, x):
                    changed = True
            if changed:
                save_agent(fresh)
                event("bound-at-sweep", key=fresh["key"], work=fresh.get("integration_work"))
    for r in all_agents(include_done=True):
        for w in r.get("workspaces") or []:
            if w.get("path") and not w.get("deleted_at"):
                paths.add(w["path"])
            if w.get("branch") and not w.get("branch_deleted_at"):
                branches.add((w.get("repo"), w["branch"]))
        # A ref already attributed to an agent (point 3) is that agent's work,
        # not unregistered work of an ended session: it is landed or discarded
        # with the agent that made it.
        for pair in r.get("created_branches") or []:
            if len(pair) >= 2:
                branches.add((pair[0], pair[1]))
    found = []
    for repo in repos:
        repo = main_checkout(repo)
        if not repo:
            continue
        wl = worktree_list(repo)
        if wl is None:
            continue
        attached = set()
        for e in wl[1:]:
            kind = classify(e["path"], e["branch"])
            if e["branch"]:
                attached.add(e["branch"])
            if kind not in ("cc", "native"):
                continue
            if e["path"] in paths or (repo, e["branch"]) in branches:
                continue
            if _younger_than_its_spawn(repo, e["path"]):
                continue
            found.append((repo, e["path"], e["branch"], kind))
        for b in local_branches(repo, ["cc/", NATIVE_BRANCH_PREFIX + "*"]) or []:
            if b in attached or (repo, b) in branches or b.startswith(CODEX_PREFIX):
                continue
            found.append((repo, "", b, classify("", b)))
    if not record:
        return found
    made = []
    for repo, path, branch, kind in found:
        key = "orphan--" + hashlib.sha1(("%s\0%s\0%s" % (repo, path, branch)).encode()).hexdigest()[:16]
        with Lock():
            prior = load_agent(key)
            if prior and not prior.get("disposition"):
                continue
            if prior:
                key = "%s-%d" % (key, int(now()))
            rec = new_record(key, name="orphan-" + os.path.basename(path or branch.replace("/", "-")),
                             orphan=True)
            _add_workspace(rec, kind, repo, path, branch, "unregistered")
            save_agent(rec)
        event("unregistered", key=key, repo=repo, path=path, branch=branch)
        made.append(key)
    return made


def _younger_than_its_spawn(repo, path):
    """A workspace git registered less than SPAWN_WINDOW seconds ago is inside
    the spawn that created it: points 3 and 6 register it *at* that spawn
    (PreToolUse, SubagentStart, PostToolUse), and a scan that raced them would
    call a workspace unregistered while its registration is being written.
    This reads the age of git's own registration of it — it says nothing
    about whether any agent is alive, and after the window the rule is the
    page's: no registration means finished work of an ended session."""
    admin = _admin_dir_for(repo, path)
    try:
        # `commondir` is written once, when git creates the workspace, and never
        # again — unlike the directory itself, whose time moves with every commit.
        return bool(admin) and (now() - os.stat(os.path.join(admin, "commondir")).st_mtime) < SPAWN_WINDOW
    except OSError:
        return False


def _claimable(rec, me, cache):
    """Point 12: each running session handles only the agents it started; the
    next session handles an ended session's. An unregistered workspace is an
    ended session's (point 3)."""
    owner = rec.get("session_id")
    if owner == me:
        return True
    if owner and not rec.get("orphan"):
        st, _ = session_state(owner, rec.get("session_identity"), cache)
        if st != "ended":
            return False
    claim = rec.get("claimed_by")
    if claim and claim != me:
        st, _ = session_state(claim, None, cache)
        if st != "ended":
            return False
    return True


def pending(me, entity="", scan=False, auto=True, deadline=None, report=None, dry=False):
    """The finished work this session must land or discard (point 5), after
    landing automatically everything that already is landed (point 4).

    With a deadline, the AUTO-LAND is what gets dropped when the budget runs
    out — never the list. An item that could not be checked stays pending and
    keeps blocking, which is the safe answer; the names of the items that were
    not checked go into `report` so the gate can say so rather than go quiet.

    `dry=True` IS THE SAME QUESTION ASKED WITHOUT ACTING ON THE ANSWER (hunt
    part 4, finding 29). `workspaces.sh status` and the dry spawn check ask
    what WOULD happen, and both used to take this function's automatic land on
    the way: work that was eligible was landed and deleted, its processes
    stopped, and other sessions' records claimed. Dry, nothing is written:
    no land, no claim, no adoption of the platform's records, no orphan record
    for an unregistered workspace. Eligibility is still proved, by the same
    read-only proof the land uses after shutdown, and an item that WOULD land
    is returned marked `would_land` rather than left out, so the caller can
    say so. An unregistered workspace the scan finds is returned marked
    `unregistered`. The operations that are asked to act (the Stop gate, a
    live spawn, `land`) still act."""
    cache = {}
    items = []
    landed_here = False
    if scan:
        repos = set(known_repos())
        if entity:
            repos.add(main_checkout(entity) or realpath(entity))
        found = scan_unregistered(sorted(r for r in repos if r), record=not dry)
        # A DAMAGED RECORD IS NAMED, NEVER SWEPT (V2-04). The scan above found
        # nothing while one exists; this says why, at every gate and status,
        # without holding the turn: the repair is a person's, outside the run.
        for path, why_bad in damaged_records():
            items.append({"key": "", "name": "damaged-record-" + os.path.basename(path),
                          "why": "its registry record cannot be read (%s: %s), and an unreadable "
                                 "record says nothing about whether its worker ended; no workspace "
                                 "is treated as unregistered until it is repaired" % (path, why_bad),
                          "waiting": "outside", "waiting_on": "repair of %s" % path,
                          "blocks_new_work": False, "blocks_turn_end": False, "damaged": True,
                          "workspaces": []})
        if dry:
            for repo, path, branch, kind in found:
                items.append({"key": "", "name": "orphan-" + os.path.basename(path or branch.replace("/", "-")),
                              "why": "it has no registration: finished work of an ended session (point 3)",
                              "waiting": "", "waiting_on": "", "blocks_new_work": False,
                              "blocks_turn_end": True, "unregistered": True,
                              "workspaces": [(path or "(branch only)", branch)]})
    for rec in all_agents():
        if rec.get("disposition"):
            continue
        if dry:
            # The platform's recorded binding and ending are READ here, never
            # adopted (V2-05): the live branch below adopts them; a question
            # must still see them.
            rec = platform_view(rec)
            fin, paused_, why = finished_state(rec, cache)
            if not fin:
                if paused_ and not (rec.get("pause") or {}).get("until") and rec.get("session_id") == me:
                    items.append(_item(rec, "paused with nothing named that ends it (point 11)", cache, me))
                continue
            if not _claimable(rec, me, cache):
                continue
            it = _item(rec, why, cache, me)
            if auto and not _past(deadline):
                try:
                    _require_landed(rec, _chain(rec), "", deadline, record=False)
                    it["would_land"] = True
                except Deadline:
                    _deferred(report, rec)
                except SpecError:
                    pass
            elif auto:
                _deferred(report, rec)
            items.append(it)
            continue
        # Point 11: an ending the platform recorded itself and gave no hook for
        # (a stopped agent) becomes finished HERE, before anything asks whether
        # it is, so the list below is the list the page describes. Point 3's
        # half of the same sentence comes first: a registration whose spawn was
        # never bound to an agent — because the hook that binds it was killed —
        # takes the platform's own binding here too, or the ending below would
        # be looked for on a record that can never have one.
        rec = observe_platform_binding(rec)
        rec = observe_platform_end(rec)
        fin, paused_, why = finished_state(rec, cache)
        if not fin:
            if paused_ and rec.get("session_id") == me and not (rec.get("pause") or {}).get("until"):
                items.append(_item(rec, "paused with nothing named that ends it (point 11)", cache, me))
            continue
        if not _claimable(rec, me, cache):
            continue
        if rec.get("session_id") != me and rec.get("claimed_by") != me:
            with Lock():
                fresh = load_agent(rec["key"])
                if fresh and not fresh.get("disposition"):
                    fresh["claimed_by"] = me
                    save_agent(fresh)
                    rec = fresh
        refused = ""
        if auto and not _past(deadline):
            try:
                res = land(rec["key"], me, auto=True, deadline=deadline)
                if res.get("landed"):
                    landed_here = True
                    continue
            except Deadline:
                _deferred(report, rec)
            except SpecError as e:
                # SAID, NEVER SWALLOWED (2026-10-04): four merged, ended
                # workspaces sat 1-2 days while this refusal was dropped here
                # and their wait note kept them off every gate's list.
                refused = " ".join(str(e).split())[:400]
        elif auto:
            _deferred(report, rec)
        items.append(_item(rec, why + ("; it did not land on its own: " + refused if refused else ""),
                           cache, me))
    if landed_here:
        # A LAND IN THIS PASS CHANGES OTHER RECORDS (review
        # rv-20261011T230453Z-7b7f2a33-fec0). Landing a carrier ends the wait
        # of the work it did not fully carry, or lands that work with it, but
        # the items above were built from copies read before that land. The
        # gate reading them let the turn end on the ended wait; only the next
        # gate blocked. So every item is rebuilt from its record as it is now.
        fresh_items = []
        for it in items:
            fresh = load_agent(it["key"]) if it.get("key") else None
            if fresh and fresh.get("disposition"):
                continue
            fresh_items.append(_item(fresh, it["why"], cache, me) if fresh else it)
        items = fresh_items
    return items


def _deferred(report, rec):
    if report is not None:
        report.setdefault("deferred", []).append(rec.get("name") or rec["key"])


def _item(rec, why, cache, me):
    waiting = rec.get("waiting") or {}
    helpers = [a for a in all_agents() if rec["key"] in (
               (a.get("recovery_assignment") or {}).get("correction_of") or a.get("continues") or [])
               or rec["name"] in (a.get("lands_pending") or [])]
    # A helper the user stopped is not "working to land it" (V2-05): its
    # platform-recorded ending is read, without adopting it, before asking.
    started = [a for a in helpers if not finished_state(platform_view(a), cache)[0]]
    kind = waiting.get("kind", "")
    if not kind and started:
        kind, waiting = "started", {"on": "agent %s is working to land it" % started[0].get("name")}
    if not kind:
        merging = _active_merge(rec, cache)
        if merging:
            kind, waiting = "started", {"on": merging}
    # Pending work remains an integration obligation, including failed merges.
    # It never blocks unrelated spawns. Waits and live merges govern turn end.
    return {"key": rec["key"], "name": rec.get("name") or rec["key"], "why": why,
            "waiting": kind, "waiting_on": waiting.get("on", ""),
            "blocks_new_work": False,
            "blocks_turn_end": kind not in ("ceo-discard", "started", "outside", "held"),
            "workspaces": [(w.get("path") or "(branch only)", w.get("branch")) for w in live_workspaces(rec)]}


def _active_merge(rec, cache):
    """Read-only evidence of this work's live merge into its recorded target.

    MERGE_HEAD is not written until the pre-merge check returns and survives
    a failed check. Require a live Git merge naming the workspace branch.
    """
    if "merge-processes" not in cache:
        merges = []
        for pid, command in _process_args().items():
            try:
                args = shlex.split(command)
            except ValueError:
                continue
            if not args or os.path.basename(args[0]) != "git" or "merge" not in args[1:]:
                continue
            i = 1
            while i < len(args) and args[i].startswith("-"):
                if args[i] in ("-C", "-c", "--git-dir", "--work-tree", "--namespace"):
                    i += 2
                else:
                    i += 1
            if i >= len(args) or args[i] != "merge":
                continue
            merges.append((pid, args[i + 1:]))
        cache["merge-processes"] = merges
    for workspace in live_workspaces(rec):
        branch = workspace.get("branch") or ""
        candidates = [(pid, arguments) for pid, arguments in cache["merge-processes"]
                      if branch and (branch in arguments or "refs/heads/" + branch in arguments
                                     or any(re.fullmatch(r"[0-9a-f]{40}", arg) for arg in arguments))]
        if not candidates:
            continue
        main = main_checkout(workspace.get("repo") or workspace.get("path") or "")
        if not main:
            continue
        target, _tip, why = integration_target([rec], main)
        rc, checked_out, _err = git(main, "symbolic-ref", "--quiet", "--short", "HEAD")
        if why or rc or checked_out.strip() != target:
            continue
        tip, unread = branch_tip_read(main, branch)
        if unread or not tip:
            continue
        for pid, arguments in candidates:
            if not any(ref in arguments for ref in (branch, "refs/heads/" + branch, tip)):
                continue
            _parent, executable = _ps_parent_and_comm(pid)
            if os.path.basename(executable) != "git":
                continue
            # Read cwd only for a Git merge naming this work. Git's -C changes
            # its actual OS cwd, including when the path contains spaces.
            try:
                cwd = os.readlink("/proc/%s/cwd" % pid) if os.path.isdir("/proc/self") else ""
                if not cwd:
                    found = subprocess.run(["lsof", "-a", "-p", str(pid), "-d", "cwd", "-Fn", "-w"],
                                           capture_output=True, text=True, timeout=2, env=_ps_env())
                    cwd = next(line[1:] for line in found.stdout.splitlines() if line.startswith("n"))
                if realpath(cwd) == main and _alive(pid):
                    return "Git is merging %s into %s in %s" % (branch, target, main)
            except (OSError, StopIteration, subprocess.TimeoutExpired):
                continue
    return ""


def gate_message(items, what):
    lines = ["=== Finished work is not landed or discarded: you cannot %s (point 5) ===" % what,
             "  Spec: %s. Land or discard every item first:" % SPEC]
    for i in items:
        w = ("  [waiting: %s — %s]" % (i["waiting"], i["waiting_on"])) if i["waiting"] else ""
        lines.append("   - %s: %s%s" % (i["name"], i["why"], w))
        for p, b in i["workspaces"][:4]:
            lines.append("       %s  %s" % (p, b or ""))
    lines += ["  How: merge its branch, then  workspaces.sh land <name>   (it also lands on its own",
              "       once every branch is in the branch this work integrates on and nothing is",
              "       uncommitted — `workspaces.sh integration` says which branch that is (point 14))",
              "       or  workspaces.sh discard <name> --reason '...' (--ceo-word '...' | --not-ceo-ordered '...')",
              "       or spawn the work that lands it, naming it:  lands-pending: <name>  /  continues: <name>"]
    lines += ["  Independent agents may start while this work remains pending.",
              "  Allowed while it waits (point 5): answer the CEO naming this work, or record",
              "       workspaces.sh wait <name> --started '...'  |  --outside '...' --todo '<CEO TODO ref>'",
              "       |  --ceo '<the question you asked him>' --todo '<CEO TODO ref>'"]
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# point 8 — nothing uncommitted lands
# ---------------------------------------------------------------------------

def _past(deadline):
    return deadline is not None and now() >= deadline


def _bounded(deadline, cap):
    """A subprocess timeout that does not outlive the caller's deadline (hunt
    part 4 v2, V2-03): the smaller of `cap` and what is left, and at least one
    second, so a call already admitted is never handed zero. A caller that
    must not START a step past its deadline checks _past first."""
    if deadline is None:
        return cap
    return max(1.0, min(float(cap), deadline - now()))


def _within(until, fn, *args, **kw):
    """(finished, result) of `fn(*args, **kw)`, waited for no longer than the
    caller's deadline `until` (hunt part 4 v3, V3-02).

    The container reaper and the test-instance collector run on their own
    clocks (Docker and app timeouts this module does not set), so checking
    the time only BEFORE calling them let one call keep a gate waiting long
    past its budget. Here the caller stops waiting at the deadline and gets
    (False, None): the cleanup is deferred, exactly as a stage that would
    start late is. The helper finishes on its own in the background; both
    are idempotent and never delete a workspace. No deadline: called inline."""
    if until is None:
        return True, fn(*args, **kw)
    box = {}

    def run():
        try:
            box["result"] = fn(*args, **kw)
        except BaseException as e:            # re-raised in the caller below
            box["error"] = e
    t = threading.Thread(target=run, name="workspaces-within-deadline", daemon=True)
    t.start()
    t.join(max(0.0, until - now()))
    if t.is_alive():
        return False, None
    if "error" in box:
        raise box["error"]
    return True, box.get("result")


def _gate_deadline(default_seconds):
    v = (os.environ.get("RICHOS_WORKSPACES_GATE_BUDGET") or "").strip()
    try:
        secs = float(v) if v else float(default_seconds)
    except ValueError:
        secs = float(default_seconds)
    return now() + max(0.0, secs)


_UNOPENED_DIR = re.compile(r"warning: could not open directory '(.+)': (.+)$")


def uncommitted(path, deadline=None):
    """([uncommitted entries], [ignored entries the main checkout does not have]).

    THIS IS THE UNBOUNDED PART, and it is unbounded by nature: it walks every
    ignored entry of a workspace and compares it against the main checkout. Run
    with a deadline it raises Deadline rather than overrunning it — including
    inside `git status` itself, which is one subprocess that cannot be
    interrupted, so it is given only the time that is left."""
    if _past(deadline):
        raise Deadline("the gate's budget ran out before %s could be checked for uncommitted work" % path)
    kw = {}
    if deadline is not None:
        kw["timeout"] = max(1.0, min(60.0, deadline - now()))
    rc, out, err = git(path, "status", "--porcelain=v1", "-z", "--untracked-files=normal", "--ignored", **kw)
    if rc == 124:
        raise Deadline("`git status --ignored` in %s did not finish inside the gate's budget" % path)
    if rc != 0:
        raise SpecError("git status failed in %s: %s" % (path, err.strip()[:200]))
    dirty, ignored = [], []
    # GIT ITSELF SKIPS A DIRECTORY IT CANNOT OPEN, exits 0 and says so only on
    # stderr (measured here: `warning: could not open directory 'build/':
    # Permission denied`, and no `!!` entry at all). What is in it is unknown,
    # so it is named (hunt part 4, finding 5): with the ignored entries when
    # git's own rules ignore that path, so `--ignored-not-needed` can still
    # waive it; otherwise with the uncommitted ones, which nothing waives.
    for line in err.splitlines():
        m = _UNOPENED_DIR.match(line.strip())
        if not m:
            continue
        rel, why = m.group(1), m.group(2)
        entry = "%s/ (unreadable: %s)" % (rel.rstrip("/"), why)
        ic, _o, _e = git(path, "check-ignore", "-q", "--no-index", "--", rel)
        (ignored if ic == 0 else dirty).append(entry)
    main = main_checkout(path)
    for n, ent in enumerate([e for e in out.split("\0") if e]):
        if deadline is not None and (n & 63) == 0 and _past(deadline):
            raise Deadline("the gate's budget ran out while walking the ignored entries of %s" % path)
        code, rel = ent[:2], ent[3:]
        if code == "!!":
            other = os.path.join(main, rel.rstrip("/")) if main else ""
            mine = os.path.join(path, rel.rstrip("/"))
            if other and os.path.isdir(mine) and not os.path.islink(mine) and os.path.isdir(other):
                # AN IGNORED DIRECTORY IS COMPARED BY CONTENT, NEVER SKIPPED BY
                # NAME. `git status --ignored` reports a wholly ignored directory
                # as ONE entry, so until round 7 a directory the main checkout
                # also had was `continue`d here with nothing inside it looked
                # at — and every main checkout and most cc/ workspaces on this
                # machine carry `.claude/`, so that was the ordinary case. A
                # nested repository with unlanded commits under an ignored
                # `vendor/`, and a `.claude/notes/needed.txt`, were neither
                # refused nor preserved: the land proceeded and deleted them
                # (lifecycle-failure-record-2026-09-10.md §3b.2, the one real
                # way to lose work; certification-frank-round6-2026-09-12.md
                # §2.1). Point 8: "Deletion therefore never loses anything that
                # was meant to land." So every file under it is compared, and
                # the ones the main checkout does not have, or has differently,
                # are named by path.
                for sub in _ignored_dir_diff(mine, other, rel.rstrip("/"), deadline):
                    ignored.append(sub)
                continue
            if other and os.path.islink(mine):
                # A link is compared by where it points, never by what it
                # points at (V2-08), exactly as nested links are.
                try:
                    if os.path.islink(other) and os.readlink(mine) == os.readlink(other):
                        continue
                except OSError:
                    pass
            elif (other and os.path.isfile(mine) and os.path.isfile(other)
                    and not os.path.islink(other) and _same_file(mine, other, deadline)):
                continue
            ignored.append(rel)
        else:
            dirty.append(ent)
    # git's warning and the walk below can both name one unreadable directory
    return dirty, list(dict.fromkeys(ignored))


def _ignored_dir_diff(mine, other, rel, deadline=None):
    """Every entry under the ignored directory `mine` that the main checkout's
    `other` lacks or has with different bytes, as paths relative to the
    workspace. Symlinks are compared by target and never followed; a nested
    repository's `.git` is walked like anything else, so a commit that exists
    only in the workspace's copy shows up as an object file the main checkout
    does not have. Bounded by the gate's deadline like the rest of the walk.

    A DIRECTORY THAT CANNOT BE LISTED IS NAMED, NEVER PASSED OVER (hunt part
    4, finding 5). `os.walk` drops a directory it cannot read unless it is
    given `onerror`, and nothing inside was then compared: an ignored
    directory made unreadable came back "no differences" while it held the
    only copy of a file. What is in it is unknown, so it is reported as a
    difference, by path, with the reason; the land then refuses it like any
    other, and `--ignored-not-needed` is still the way to say it is not needed.
    The partial-cleanup check (`_landed_residue`) already refuses the same
    error; this is the same rule for the normal comparison."""
    out = []
    n = 0

    def unreadable(error):
        where = getattr(error, "filename", None) or mine
        sub = os.path.normpath(os.path.join(rel, os.path.relpath(where, mine)))
        out.append("%s/ (unreadable: %s)" % (sub, error.strerror or error))

    for root, dirs, files in os.walk(mine, followlinks=False, onerror=unreadable):
        dirs.sort()
        for name in sorted(files) + [d for d in dirs if os.path.islink(os.path.join(root, d))]:
            n += 1
            # Every entry, not every 64th: one comparison can be slow (a large
            # file, slow storage), and the clock is only a clock read.
            if _past(deadline):
                raise Deadline("the gate's budget ran out while comparing the ignored directory %s" % rel)
            a = os.path.join(root, name)
            b = os.path.join(other, os.path.relpath(a, mine))
            sub = os.path.join(rel, os.path.relpath(a, mine))
            if os.path.islink(a):
                try:
                    same = os.path.islink(b) and os.readlink(a) == os.readlink(b)
                except OSError:
                    same = False
            else:
                same = os.path.isfile(b) and not os.path.islink(b) and _same_file(a, b, deadline)
            if not same:
                out.append(sub)
        # a symlinked directory is compared above as a link and never descended
        dirs[:] = [d for d in dirs if not os.path.islink(os.path.join(root, d))]
    return out


def _same_file(a, b, deadline=None):
    """Byte-identical? Read in pieces with the gate's clock checked between
    them (hunt part 4, finding 15): one whole-file read of a large file used
    to be a step no deadline could interrupt."""
    try:
        if os.path.getsize(a) != os.path.getsize(b):
            return False
        return _file_digest(a, deadline) == _file_digest(b, deadline)
    except OSError:
        return False


def _landed_residue(rec, w, deadline=None):
    """Accept partial-removal leftovers only when their content is still preserved."""
    path = w["path"]
    main = main_checkout(w.get("repo") or "")
    entries = worktree_list(main) if main else None
    if entries is None:
        return False
    if any(e["path"] == path for e in entries):
        return None  # Still a worktree: use Git's normal cleanliness check.
    _branch, tip, why = integration_target([rec], w.get("repo"))
    if why:
        return False
    rc, tree, _err = git(main, "ls-tree", "-rz", tip)
    if rc:
        return False
    tracked = {}
    for item in tree.split("\0"):
        if item:
            info, name = item.split("\t", 1)
            mode, kind, oid = info.split()
            if kind == "blob":
                tracked[name] = (mode, oid)
    pointer = os.path.join(path, ".git")
    try:
        if os.path.lexists(pointer):
            if not os.path.isfile(pointer) or os.path.islink(pointer):
                return False
            with open(pointer, encoding="utf-8") as f:
                text = f.read().strip()
            common = _common_dir(main)
            if not text.startswith("gitdir:") or not common or not realpath(text[7:].strip()).startswith(
                    os.path.join(common, "worktrees") + os.sep):
                return False
        def unreadable(error):
            raise error
        for root, dirs, files in os.walk(path, followlinks=False, onerror=unreadable):
            for name in files + [d for d in dirs if os.path.islink(os.path.join(root, d))]:
                if _past(deadline):
                    raise Deadline("the gate's budget ran out while checking partial cleanup")
                source = os.path.join(root, name)
                if source == pointer:
                    continue
                relative = os.path.relpath(source, path)
                target = os.path.join(main, relative)
                mode, oid = tracked.get(relative, ("", ""))
                if os.path.islink(source):
                    if os.path.islink(target) and os.readlink(source) == os.readlink(target):
                        continue
                    rc, content, _err = git(main, "cat-file", "blob", oid) if mode == "120000" else (1, "", "")
                    if rc or os.readlink(source) != content:
                        return False
                elif os.path.isfile(source):
                    executable = bool(os.stat(source).st_mode & 0o111)
                    if (os.path.isfile(target) and not os.path.islink(target) and _same_file(source, target, deadline)
                            and executable == bool(os.stat(target).st_mode & 0o111)):
                        continue
                    if mode != ("100755" if executable else "100644"):
                        return False
                    rc, actual, _err = git(main, "hash-object", "--no-filters", "--", source)
                    if rc or actual.strip() != oid:
                        return False
                else:
                    return False
            dirs[:] = [d for d in dirs if not os.path.islink(os.path.join(root, d))]
    except (OSError, UnicodeError):
        return False
    return True


GENERATED_TREE_CAP = 2000


def _file_digest(p, deadline=None):
    """SHA-1 of a file, read in pieces with the clock checked between them."""
    h = hashlib.sha1()
    with open(p, "rb") as f:
        while True:
            if _past(deadline):
                raise Deadline("the gate's budget ran out while reading %s" % p)
            chunk = f.read(1 << 20)
            if not chunk:
                return h.hexdigest()
            h.update(chunk)


def _fingerprint(p, deadline=None):
    """What an ignored entry IS, precisely enough to tell it changed: a link by
    its target, a file by size and content, a directory by every entry under
    it (links by target, never followed). None when it cannot be read, or a
    directory is larger than GENERATED_TREE_CAP entries: an entry with no
    fingerprint is never waived, so the answer is the safe one."""
    try:
        if os.path.islink(p):
            return ["link", os.readlink(p)]
        if os.path.isfile(p):
            return ["file", os.path.getsize(p), _file_digest(p, deadline)]
        if not os.path.isdir(p):
            return None
        rows = []

        def unreadable(error):
            raise error
        for root, dirs, files in os.walk(p, followlinks=False, onerror=unreadable):
            dirs.sort()
            for name in sorted(files) + [d for d in dirs if os.path.islink(os.path.join(root, d))]:
                if len(rows) >= GENERATED_TREE_CAP:
                    return None
                a = os.path.join(root, name)
                rows.append([os.path.relpath(a, p), _fingerprint(a, deadline)])
            dirs[:] = [d for d in dirs if not os.path.islink(os.path.join(root, d))]
        return ["dir", len(rows), hashlib.sha1(json.dumps(rows).encode()).hexdigest()]
    except OSError:
        return None


def _generated_manifest(path):
    """{relative ignored entry: fingerprint} for every ignored entry of a
    just-created workspace that the main checkout does not have (finding 14).
    Never raises: a workspace whose products could not be recorded is one
    whose ignored entries all need the waiver, exactly as before."""
    try:
        _dirty, ignored = uncommitted(path)
    except SpecError:
        return {}
    out = {}
    for rel in ignored:
        if " (unreadable" in rel:
            continue
        fp = _fingerprint(os.path.join(path, rel.rstrip("/")))
        if fp is not None:
            out[rel] = fp
    return out


def _minus_generated(w, ignored, deadline=None):
    """`ignored` without the entries the engine made at creation and nobody has
    changed since (finding 14). Anything else — the agent's notes, a nested
    repository, a setup product the agent then altered — is still named."""
    made = w.get("generated") or {}
    if not made:
        return ignored
    out = []
    for rel in ignored:
        if rel in made and _fingerprint(os.path.join(w["path"], rel.rstrip("/")), deadline) == made[rel]:
            continue
        out.append(rel)
    return out


# OUTPUT THE REPOSITORY DECLARES REGENERABLE (hunt part 4 v2, V2-02). Finding
# 14 exempted what the ENGINE made at creation, fingerprinted then; build output
# a test or a build writes LATER still held the land until somebody typed
# --ignored-not-needed, every time. Nothing in an ignored file says whether it
# is the only copy of something, so the engine still never decides that on its
# own: the REPOSITORY says it, in its own committed root .gitignore, one line
# per path, as a comment git itself ignores:
#
#     #regenerable: build/              a directory and everything under it
#     #regenerable: **/__pycache__/     the same name at any depth
#     #regenerable: dist/*.map          a glob over the path
#
# The declaration is read from the integration branch's tip (point 14), never
# from the workspace: a line the agent adds in its own work is not landed and
# widens nothing. An entry the walk could not read is never waived.
_REGENERABLE_RE = re.compile(r"^#\s*regenerable:\s*(\S+)\s*$")


def _regenerable_patterns(rec, repo):
    """The `#regenerable:` lines of the root .gitignore at the tip of the
    branch this work integrates on; [] when nothing is recorded or readable."""
    try:
        _branch, tip, why = integration_target([rec], repo)
    except Exception:
        return []
    if why or not tip:
        return []
    main = main_checkout(repo) or repo
    rc, out, _err = git(main, "show", "%s:.gitignore" % tip)
    if rc != 0:
        return []
    return [m.group(1) for m in (_REGENERABLE_RE.match(l.strip()) for l in out.splitlines()) if m]


def _is_regenerable(rel, patterns):
    """Does the ignored entry `rel` (relative; a trailing "/" marks a
    directory) fall under one of `patterns`?"""
    import fnmatch
    if " (unreadable" in rel:
        return False
    is_dir = rel.endswith("/")
    parts = [p for p in rel.rstrip("/").split("/") if p]
    for pat in patterns:
        anywhere = pat.startswith("**/")
        body = pat[3:] if anywhere else pat.lstrip("/")
        dir_only = body.endswith("/")
        want = [p for p in body.rstrip("/").split("/") if p]
        if not want:
            continue
        starts = range(len(parts)) if anywhere else [0]
        for s in starts:
            seg = parts[s:s + len(want)]
            if len(seg) < len(want) or not all(fnmatch.fnmatchcase(a, b) for a, b in zip(seg, want)):
                continue
            under = s + len(want) < len(parts)          # rel is INSIDE the matched path
            if not dir_only or under or is_dir:
                return True
    return False


def _minus_regenerable(rec, w, ignored, record=True):
    """`ignored` without what the repository declares regenerable (V2-02).

    `record=False` for a question (hunt part 4 v3, V3-06): a dry status or a
    dry spawn check proves eligibility through this same helper, and its
    `regenerable-not-preserved` event made every status query write cleanup
    history although nothing was cleaned up."""
    if not ignored:
        return ignored
    pats = _regenerable_patterns(rec, w.get("repo") or "")
    if not pats:
        return ignored
    kept = [rel for rel in ignored if not _is_regenerable(rel, pats)]
    if record and len(kept) != len(ignored):
        event("regenerable-not-preserved", key=rec.get("key"), path=w.get("path"),
              entries=[rel for rel in ignored if rel not in kept][:50])
    return kept


def _require_clean(rec, doing, ignored_ok="", deadline=None, record=True):
    """Refuses uncommitted work. Returns the paths it proved to be partial-
    cleanup residue whose every file is preserved (`_landed_residue`): the one
    kind of workspace directory that legitimately has no readable HEAD."""
    problems = []
    preserved = []
    for w in live_workspaces(rec):
        if w.get("path") and os.path.isdir(w["path"]):
            residue = None
            if rec.get("landing_cleanup_started") or (
                    (rec.get("disposition") or {}).get("kind") == "landed" and rec.get("deletion")):
                residue = _landed_residue(rec, w, deadline)
                if residue is False:
                    raise SpecError("partial cleanup at %s cannot verify all remaining files are preserved; kept" % w["path"])
                if residue:
                    preserved.append(w["path"])
            dirty, ignored = ([], []) if residue else uncommitted(w["path"], deadline)
            ignored = _minus_generated(w, ignored, deadline)
            ignored = _minus_regenerable(rec, w, ignored, record)
            if dirty:
                problems.append("%s has %d uncommitted entr%s (%s)" % (
                    w["path"], len(dirty), "y" if len(dirty) == 1 else "ies", ", ".join(dirty[:5])))
            if ignored and not ignored_ok:
                problems.append("%s has %d ignored entr%s the main checkout does not have (%s)" % (
                    w["path"], len(ignored), "y" if len(ignored) == 1 else "ies", ", ".join(ignored[:5])))
    if problems:
        raise SpecError("cannot %s — nothing uncommitted is ever landed (point 8):\n    %s\n  Commit what it "
                        "left to its branch, or discard it. If the ignored files are not needed, say so: "
                        "--ignored-not-needed '<why>'. Output the repository always regenerates is "
                        "declared once in its committed root .gitignore (`#regenerable: <path>`) and then "
                        "needs no waiver." % (doing, "\n    ".join(problems)))
    return preserved


# ---------------------------------------------------------------------------
# point 3 — "any branch an agent created": ATTRIBUTION BY RECORDED CREATION
# ---------------------------------------------------------------------------
#
# A ref an agent created is the agent's, however it was made and whether or not
# it is checked out. A ref that already existed is never the agent's, however
# long the agent holds it. So the fact that decides is CREATION, and it is
# recorded at the moment it happens, against the agent's id.
#
# THREE EARLIER ANSWERS WERE WRONG, EACH IN ITS OWN DIRECTION, AND ALL THREE ARE
# WORTH KEEPING WRITTEN DOWN.
#
# THE REFLOG. git writes `branch: Created from <X>` identically whoever ran the
# command and wherever they ran it, and writes NOTHING to a workspace's HEAD
# reflog for a plain `git branch`. It missed the agent's own branch and handed it
# Rich's rescue copy: wrong in both directions at once.
#
# A WINDOW OVER THE WHOLE REPOSITORY, BETWEEN TWO OF THE AGENT'S TOOL CALLS.
# Every ref that appeared anywhere in the repository in that window was called
# that agent's. TWO AGENTS IN ONE REPOSITORY IS THE NORMAL CASE, and reproduced
# on this machine agent A's record held `cc/zach-opus-b1` and
# `worktree-agent-azachopusb10000` — B's, both of them. A's land was refused
# because B's branch was not in main, and `discard(A)` DELETED B's branch.
#
# POSSESSION: a branch git reports checked out at one of the agent's own
# registered workspace paths, sampled at its tool calls. That fixed the window
# and left three holes, which are one defect — it answers WHERE a ref is, and
# the question is WHO MADE IT:
#   the stray        `git branch spare` checks nothing out, so it was never
#                    attributed and was left behind (against point 10, "none is
#                    left behind").
#   the side branch  an agent that commits on a branch and switches away leaves
#                    work that nothing attributes: land() reported SUCCESS,
#                    pending() listed nothing, and the work was in no
#                    integration branch (against points 5 and 8).
#   the borrowed     a PRE-EXISTING branch the agent merely checked out became
#     branch         the agent's, and a discard deleted it with `git branch -D`
#                    (against point 8, in the destructive direction).
#
# ===========================================================================
# THE PAIR OF EVENTS, WHICH IS WHAT MAKES CREATION OBSERVABLE
# ===========================================================================
# The platform gives a tool call two halves, and both carry the agent's id:
# PreToolUse and PostToolUse. A ref that is absent at the first and present at
# the second appeared DURING that agent's own tool call. There is a catch-all
# PreToolUse (the lock-out, which calls snapshot_refs); the catch-all PostToolUse
# in hooks/hooks.json is the other half, and until 2026-09-12 it did not exist —
# which is why creation could not be recorded and possession was the only fact
# left to read.
#
# The pair is per tool call, not per agent-lifetime, and that matters: the
# window is the duration of ONE call, so it is measured in seconds rather than
# in the minutes between two calls.
#
# ===========================================================================
# ONE WINDOW PER TOOL CALL, KEYED BY THE CALL — NOT ONE SLOT PER AGENT
# ===========================================================================
# Until 2026-09-12 the snapshot was written to ONE path per agent,
# `refs/<key>.json`, and the first PostToolUse to arrive consumed it. AN AGENT'S
# OWN CALLS OVERLAP: this project's own agent instructions require independent
# calls to be issued in one block, and a backgrounded Bash call was measured
# returning 4.0 s before its process finished. Pre(A) Pre(B) Post(A) Post(B)
# therefore lost a whole window — B's snapshot overwrote A's, Post(A) consumed
# what was left, and Post(B) had nothing to compare against. A ref created in
# call B was attributed to NOBODY, and a side branch created that way took real
# commits with it: land() reported LANDED, pending() listed nothing, and the
# commit was in no integration branch (reproduced by two reviewers at 84e12d32,
# against points 5, 8 and 10).
#
# THE PLATFORM GIVES EACH CALL ITS OWN ID and both halves carry it: `tool_use_id`
# is the same string at PreToolUse and at PostToolUse of one call — the spawn
# registration has relied on exactly that pairing since it was written
# (register_spawn stores it at the Pre; bind_agent looks it up at the Post). So
# the snapshot is keyed by the CALL, in a directory per agent, and a Post
# consumes ITS OWN call's window and no other. Overlapping calls of one agent can
# no longer clobber each other, and a Post whose Pre never ran (a guard refused
# it) still attributes nothing, exactly as before.
#
# WHEN NO CALL ID IS PRESENT — a caller driving these entry points directly —
# the only pairing available is arrival order, so an unkeyed snapshot is written
# under its own name and a Post consumes the OLDEST unkeyed one. That is still
# one window per call; it is merely paired by order rather than by name. A Pre
# and a Post that DISAGREE about whether there is an id find nothing to consume
# and attribute nothing, which is the safe direction: under-attribution leaves a
# stray, and the land refuses while that stray carries anything unlanded.
#
# The open windows of one agent are bounded (_MAX_OPEN_CALLS): a PreToolUse that
# a guard then REFUSES leaves a snapshot no Post will ever consume, so the oldest
# are evicted rather than accumulating. Every open window is consumed at the
# agent's end of run, which is the last observation (record_end, stop).
#
# ===========================================================================
# TIME ALONE IS STILL A GUESS, SO IT IS NEVER THE WHOLE TEST
# ===========================================================================
# Other things happen while an agent's tool call runs: Rich cuts a branch, a
# second agent is spawned, a human commits in a worktree of his own. "It
# appeared while this agent was working" is co-occurrence, and the closing
# section of the page allows no guessing. So a candidate must ALSO carry THIS
# AGENT'S OWN UNLANDED WORK, which is a fact about the agent and not about the
# clock. Four filters, each one load-bearing and each one with its own mutant:
#
#   1. NEW      absent from the snapshot this agent's own PreToolUse took, and
#               present now. codex/ is never a candidate at all (point 2).
#   2. NOBODY   not a ref recorded on another agent's record — its workspace
#      ELSE'S   branch or a ref it is already recorded as having created — and
#               not the branch the main checkout itself holds.
#   3. AT       its tip is NOT already contained in the branch this work
#      STAKE    integrates on (point 14). A ref that is entirely landed carries
#               nothing, so attributing it decides nothing; leaving it is a
#               stray, and the page's own bound on the cost of that is below.
#   4. THIS     its tip is on one line of history with a commit of this agent's
#      AGENT'S  OWN that is itself not yet in the integration branch — equal to
#      WORK     it, or one is an ancestor of the other. "Its own" means every
#               commit its own workspaces have pointed at: their branch tips,
#               their current HEADs and their own private HEAD reflogs. An agent
#               that has produced no unlanded commit of its own in that
#               repository is attributed NOTHING, which is what keeps a second
#               agent's refs, and a human's, out of its record.
#
# WHEN, still: only at a moment the platform vouches for with this agent's id,
# and only while the agent is not finished. A finished agent's restarted call is
# refused before it gets here (point 9), and its open snapshot is thrown away, so
# a branch Rich cuts in the workspace afterwards to rescue the work stays his.
#
# THE ONE THING THIS STILL CANNOT TELL APART, named rather than papered over: a
# ref Rich cuts, DURING one of the agent's own tool calls, AT that agent's own
# unlanded tip. git keeps no record of where a ref was created, so `git branch
# spare` inside the workspace and `git branch rich/copy <the agent's tip>` in the
# main checkout are the same event seen from outside. It is decided for the
# agent, because the agent's stray is the case the page names ("any branch an
# agent created") and because the cost is bounded in both directions: a land
# refuses while such a ref carries anything not in the integration branch, so the
# only one a land can delete is one already fully landed, and a discard records
# every tip it deletes. Rich does not rescue a running agent's work — he rescues
# a finished one's, which is outside every window by construction.
#
# Cost, per tool call: one `git for-each-ref` per repository the agent has a
# workspace in (one or two) at each half, plus one small write. The registry lock
# and the scan of other agents' records are touched only when a ref that is not
# already accounted for is actually sitting there, which is rare.

# A REFUSED PreToolUse LEAKS A WINDOW, AND THAT IS THE WHOLE OF ITEM 2.
# -------------------------------------------------------------------------
# The barrier runs at PreToolUse and writes this call's window. If ANOTHER
# PreToolUse hook then refuses the call, the tool never runs and no PostToolUse
# ever arrives: the window stays open for the rest of the agent's run. Two
# things used to go wrong with it, and they need different answers.
#
#   THE WIDENING (fixed in `_before_set`). The end-of-run pass consumed every
#   open window and INTERSECTED their before-sets, so "the widest one decides".
#   One window leaked at minute one turned the last comparison of the run into
#   "everything that appeared during this agent's entire run" — and a branch
#   RICH cut between two of the agent's calls was then attributed to the agent
#   and destroyed by its discard. The answer is the opposite operation: the
#   before-sets are UNIONED, and the most recent snapshot this agent ever took
#   is always part of that union even when its own call already consumed it.
#   A leaked window can then only ever NARROW what is attributed, never widen
#   it, which is the safe direction: an unattributed stray is a branch left
#   behind and named by the point-3 sweep, while an over-attributed one is
#   somebody else's work deleted.
#
#   THE EVICTION, MEASURED AND LEFT ALONE. The cap evicts the OLDEST window by
#   position, so a burst of refused calls opened while a live call is in flight
#   evicts the LIVE one. That looks like a second bug and it was going to be
#   fixed here. It was measured instead, and it is not one: once the
#   before-sets are UNIONED, a window opened before the ref still decides, and
#   after a burst of refusals there is always one of those still open. Both an
#   age-based cap and the unpaired-Post fallback below rescue the case
#   independently, and neither can be told from the other by any scenario --
#   so the age-based cap was an unproven claim and is not here. What actually
#   rescues it is the fallback, which has its own case and its own mutant.
_MAX_OPEN_CALLS = 64
# What the last observation of this process FOUND about the protected refs (see
# _restore_protected_refs): rows of (action, repo, branch, snapshot_tip, found,
# why), action "RESTORED" (a deleted ref put back), "MOVED" (seen, reported,
# left exactly where it was) or "LANDED" (another conversation's land, named
# from the land record and announced to the thread it moved under). The CLI
# prints them after the CREATED rows so the observe hook can announce them.
PROTECTED_REF_FINDINGS = []
# What each action is called ON THE AGENT'S OWN RECORD. Three actions, three
# sentences, and the third is not a variation of the second: "another
# conversation landed" is news about somebody else's finished work, while
# "moved" is an unexplained write on a protected ref.
_PROTECTED_REF_FACTS = {
    "RESTORED": "protected ref restored",
    "MOVED": "protected ref moved (reported, not restored)",
    "LANDED": "another conversation landed on this branch",
}


def _refs_dir(key):
    return _p("refs", _key_segment(key))


def _latest_path(key):
    """The most recent snapshot this agent took, kept AFTER its own window is
    consumed. It is never attributed from on its own; it only joins the union
    that decides what counts as new (see `_before_set`)."""
    return os.path.join(_refs_dir(key), "latest.json")


def _slot_path(key, call):
    """The window of ONE tool call. Keyed by the platform's own call id when the
    payload carries one; otherwise a unique, time-ordered unkeyed slot."""
    if call:
        return os.path.join(_refs_dir(key), "c." + _key_segment(call) + ".json")
    return os.path.join(_refs_dir(key), "u.%020d.%s.json" % (time.time_ns(), os.urandom(4).hex()))


def _open_slots(key):
    """Every open window of this agent, oldest first. Unkeyed slots sort
    chronologically by construction; keyed ones are looked up by name.

    `latest.json` is NOT a window: it is the running record of the last
    snapshot taken, and consuming it would attribute a call twice."""
    try:
        names = sorted(n for n in os.listdir(_refs_dir(key))
                       if n.endswith(".json") and n != "latest.json")
    except OSError:
        return []
    return [os.path.join(_refs_dir(key), n) for n in names]


def _evict_old_slots(key):
    """A refused PreToolUse leaves a window no Post will consume. Keep the cap."""
    slots = _open_slots(key)
    if len(slots) <= _MAX_OPEN_CALLS:
        return
    try:
        slots.sort(key=os.path.getmtime)
    except OSError:
        pass
    for p in slots[:len(slots) - _MAX_OPEN_CALLS]:
        try:
            os.unlink(p)
        except OSError:
            pass


def _local_refs(repo):
    """{branch: tip} for every local branch, read from git. None if unreadable."""
    rc, out, _ = git(repo, "for-each-ref", "--format=%(objectname) %(refname:short)", "refs/heads")
    if rc != 0:
        return None
    refs = {}
    for line in out.splitlines():
        parts = line.split(None, 1)
        if len(parts) == 2 and parts[1].strip():
            refs[parts[1].strip()] = parts[0].strip()
    return refs


def _repos_of(rec):
    """The repositories this agent has a live workspace in, from its record."""
    out = []
    for w in live_workspaces(rec):
        repo = w.get("repo")
        if repo and repo not in out and os.path.isdir(repo):
            out.append(repo)
    return out


def snapshot_refs(rec, call=""):
    """THE FIRST HALF OF THE PAIR (point 3): what its repositories held when
    THIS tool call started. Called from barrier(), so it runs at a moment the
    platform vouches for with this agent's id, and never for a finished one.

    `call` is the platform's own id for this tool call, and it KEYS the window:
    two of the agent's own calls open at once keep two windows, and neither can
    clobber the other.

    A snapshot lost to a crash costs one window of attribution, and
    under-attribution loses nothing — so this never fails a tool call.

    A BACKGROUND WINDOW WHOSE SHELL HAS ENDED IS SETTLED HERE FIRST, at the
    earliest moment the engine sees the agent again (`_settle_ended_background`):
    judged one last time and closed before this call can do anything, so the
    window it no longer needs cannot claim a ref cut after it."""
    try:
        _settle_ended_background(rec)
    except (OSError, ValueError, SpecError):
        pass
    try:
        snap = {}
        tips = {}
        for repo in _repos_of(rec):
            refs = _local_refs(repo)
            if refs is None:
                continue                 # unreadable: no snapshot, so no candidates
            snap[repo] = sorted(refs)
            # THE PROTECTED REFS' TIPS (round 8, items 2 and 3): a RECORDED
            # integration branch and every codex/ ref. What is compared at the
            # other half of the pair is not only which refs EXIST but where
            # these point, so a move by any verb — named or not, from the
            # agent's own worktree or the main checkout — can be restored.
            tips[repo] = _protected_tips(repo, refs)
        row = {"key": rec["key"], "call": call or "", "at": now(), "repos": snap, "tips": tips}
        write_json(_slot_path(rec["key"], call), row)
        # The same fact, kept where consuming a window cannot remove it. It is
        # what stops a window leaked by a refused call from widening the
        # end-of-run comparison to the whole run (see `_before_set`).
        write_json(_latest_path(rec["key"]), row)
        _evict_old_slots(rec["key"])
        return snap
    except (OSError, ValueError):
        return {}


def _protected_names(repo):
    """The branch names protected IN THIS REPOSITORY: a RECORDED integration
    branch of a body of work recorded FOR THIS REPOSITORY (any body of work,
    superseded ones included). A codex/ ref is protected by its prefix in every
    repository and is not in here.

    KEYED BY (REPOSITORY, BRANCH), NEVER BY NAME ALONE. Until 2026-09-14 this
    gathered every recorded branch of every body of work and matched on the NAME,
    so `main` recorded for femcboost made `refs/heads/main` protected — and
    restorable — in richos and richos-hq too (three bodies of work on this
    machine, all three integrating on `main`). The repository a body of work was
    recorded for is written on the record; nothing has to be inferred."""
    names = set()
    try:
        key = _norm_repo(repo)
        for w in all_bodies_of_work().values():
            if not (w or {}).get("branch"):
                continue
            if realpath(w.get("repo") or "") != key:
                continue
            names.add(w["branch"])
    except (OSError, ValueError):
        pass
    return names


def _is_protected(repo_names, b):
    return b.startswith(CODEX_PREFIX) or b in repo_names


def _protected_tips(repo, refs):
    """{branch: tip} for every ref of `refs` that is protected in THIS repository:
    a recorded integration branch of a body of work recorded for it, or a codex/
    ref."""
    recorded = _protected_names(repo)
    return {b: sha for b, sha in refs.items() if _is_protected(recorded, b)}


def _recreate_deleted_ref(repo, branch, tip, who):
    """THE ONLY REF WRITE THIS CHECK MAKES, and it is create-only.

    The empty old value means "the ref must not exist at the time of the update"
    (git refuses with `cannot lock ref ... reference already exists`, measured on
    git 2.52.0), so this can re-create a protected ref that was DELETED and can
    do nothing else: it cannot move a ref, cannot clobber one somebody re-created
    between the deletion and this call, and cannot oscillate with another agent's
    check, because repeating it is refused rather than repeated. It is therefore
    incapable of dropping a commit.

    The `-m` is not decoration. Until 2026-09-14 this call site passed none, and
    the three writes it made to refs/heads/main landed in the reflog with an
    EMPTY message — which is why a day was spent working out who had moved main
    (docs/verification/ref-write-forensics-2026-09-14.md). An automated system
    writing a ref anonymously is indefensible and the message is free.

    UNDER THE OPERATOR FENCE (spec r3 e2, Frank G3). Git deletes a branch's
    reflog together with the branch, so the fence cannot recognize this restore
    from the reflog. When the repository's fence is on, a one-line RESTORE INTENT
    is written first, into the lease home the fence reads (baked into its
    launcher, never this process's environment): the ref, the value, and this
    process's pid and start time. The fence passes the creation of a missing
    `main` only when the new value equals the intent's and the intent's process
    is an ancestor of the Git call. It is removed afterwards. With the fence off,
    or absent, nothing is written and this function is what it always was.

    Returns git's own (rc, out, err)."""
    msg = "richos engine: protected ref restored after it was deleted during %s's tool call" % who
    intent = _fence_restore_intent(repo, branch, tip)
    try:
        return git(repo, "update-ref", "-m", msg, "--no-deref", "refs/heads/" + branch, tip, "")
    finally:
        if intent:
            try:
                os.unlink(intent)
            except OSError:
                pass


def _fence_restore_intent(repo, branch, tip):
    """Write the G3 restore intent when `repo`'s operator fence is on; return its
    path, or '' when nothing was written. Never raises."""
    try:
        rc, out, _ = git(repo, "rev-parse", "--path-format=absolute", "--git-common-dir")
        if rc != 0:
            return ""
        launcher = os.path.join(out.strip(), "hooks", "reference-transaction")
        with open(launcher, encoding="utf-8") as fh:
            text = fh.read(65536)
        if "richos-operator-fence-launcher" not in text:
            return ""
        conf = dict(re.findall(r'(?m)^OPERATOR_FENCES_([A-Z_]+)="([^"]*)"\s*$', text))
        if conf.get("STATE") != "on" or not conf.get("HOME") or not conf.get("KEY"):
            return ""
        st, started = process_start(os.getpid())
        if st != "ok":
            return ""
        epoch = calendar.timegm(time.strptime(" ".join(started.split()), "%a %b %d %H:%M:%S %Y"))
        path = os.path.join(conf["HOME"], "restore-intents", conf["KEY"] + ".json")
        os.makedirs(os.path.dirname(path), mode=0o700, exist_ok=True)
        tmp = "%s.%d.tmp" % (path, os.getpid())
        with open(tmp, "w", encoding="utf-8") as fh:
            fh.write(json.dumps({"ref": "refs/heads/" + branch, "value": tip, "pid": os.getpid(),
                                 "start": epoch, "at": iso()}) + "\n")
        os.replace(tmp, path)
        return path
    except (OSError, ValueError, OverflowError):
        return ""


def _restore_protected_refs(rec, priors, latest):
    """ITEMS 2 AND 3 OF ROUND 8, THE EFFECTS CHECK. A protected ref — a RECORDED
    integration branch of THIS repository, or any codex/ ref — that is gone or
    has moved since the snapshot this agent's call started with is REPORTED,
    whatever verb moved it: a named one the Bash guard missed, an unnamed one
    (`checkout <it>` then `commit`/`reset`/`merge`/`rebase`, the doorway class
    no verb list can close — brief-audit-frank-round8 §2), from the agent's own
    worktree or from the main checkout, or a non-git write.

    A MOVE IS NEVER PUT BACK. THE CHECK REPORTS IT AND LEAVES THE REF ALONE.
    From 2026-09-14, and the reason is measured, not argued. Until then a move
    was "restored" with `git update-ref --no-deref refs/heads/<b> <snapshot
    tip>`, and on 2026-09-13/14 that line moved `refs/heads/main` in
    /Users/alex/ab/richos three times, twice within twelve seconds in opposite
    directions, while Rich was landing (docs/verification/
    ref-write-forensics-2026-09-14.md, events 3, 4, 5; reproduced from nothing by
    docs/verification/protected-ref-oscillation-2026-09-14-logs/repro.py).

      * THE SHAPE CANNOT IDENTIFY THE WRITER. This check infers who moved the
        ref from the shape of the result, and Rich's ordinary land has the same
        shape as the abuse it hunts — a land is the most common write the
        recorded branch ever receives. The old docstring conceded this for "Rich
        rewinding the branch"; the reproduction shows it fires on his plain
        `git merge` of the very agent whose window is open.
      * ITS OWN WRITE WAS AN INPUT TO ITSELF. Every restore manufactured the
        non-descendant condition that made the NEXT agent's check fire, so two
        agents holding different snapshots alternated for as long as both kept
        making tool calls. A report is not an input to the check, so the loop
        cannot exist.
      * A "RESTORE" OF A MOVE IS AN ASSIGNMENT IN EITHER DIRECTION, and it drops
        every commit made between the snapshot and the write. `--mode
        destruction` of the reproduction loses a merge Rich had just made.
      * THE SNAPSHOT IT WOULD RESTORE TO IS NOBODY'S DECISION: the oldest still-
        open window of ONE agent, which can be minutes stale, and which a second
        running agent does not share.

    A DELETION IS STILL RESTORED, AND ONLY A DELETION. A deleted protected ref
    is unambiguous — point 2 says a codex/ branch is never deleted without the
    CEO's express word, and nothing in the spec deletes the branch a body of
    work integrates on while an agent is running — and re-creating it is
    information-preserving: the objects are still there and the ref goes back to
    the tip it held. The write is CREATE-ONLY and carries a reflog message:

        git update-ref -m <msg> --no-deref refs/heads/<b> <snapshot tip> ""

    The empty old value means "the ref must not exist" (git refuses with
    `reference already exists` otherwise, measured on git 2.52.0), so it can
    never clobber a ref somebody re-created in between, can never move one, and
    cannot oscillate: re-creating a ref that is already there is refused rather
    than repeated. `-m` is free, and its absence is what cost a day of
    forensics.

    NOT REPORTED: the lead's legitimate move. Rich lands FINISHED agents' work
    onto the recorded branch while other agents run, so a move to a DESCENDANT
    of the snapshot tip carrying none of THIS agent's own unlanded work is his
    and is passed over in silence. Reported: the ref is DELETED (and restored);
    or it moved to a tip that is not a descendant (a rewind, a force-move, a
    symref); or it moved to a descendant carrying this agent's own unlanded
    commits (it committed or merged onto it — which is ALSO what Rich's land of
    that same agent looks like, and is now reported rather than undone).

    AND, FROM 2026-09-17, THE ONE DESCENDANT THAT IS NOT HIS. The CEO runs two
    conversation threads whose back ends share a repository; their lands take
    one machine-wide lock per repository, so they never collide — but the
    second one's land moves the recorded branch UNDER the first one's running
    agents, and until now that was the silent case above, indistinguishable
    from the lead's own land. It is no longer inferred from the shape: app.py's
    `integrate` appends a land record naming the conversation that landed, and
    `land_by_another_conversation` reads it. A land by THIS conversation stays
    silent; one by another conversation is reported, with its thread, its time
    and its two tips. A fast-forward that no record names is the terminal path
    whether or not the repository holds older records, and it stays silent —
    Rich's hand-run `git merge` writes no record, and those teammates are told
    by guard-inflight-notify.sh at the push instead.

    Returns [(action, repo, branch, snapshot_tip, found_tip_or_"", why)], with
    action "RESTORED" (a deletion put back), "MOVED" (seen, reported, left
    alone) or "LANDED" (another conversation's land, announced to this one).

    THE OWN-WORK RULE APPLIES ONLY INSIDE A WINDOW. A descendant that carries
    the agent's own unlanded commits is evidence of the AGENT's move only while
    one of its calls is open — the doorway commit happens inside a call. With no
    window open (the end of the run, compared against the last snapshot, which
    may be minutes old) the same shape is what Rich's fast-forward of the
    agent's OWN branch onto the recorded one looks like, made after the agent's
    last call and before its end signal; measured undoing exactly that in
    certification-sage-runner-round case R8 on 2026-09-13, and the land then
    refused. So outside a window only a deletion or a non-descendant move is
    reported."""
    findings = []
    try:
        ordered = sorted([p for p in priors if isinstance(p.get("tips"), dict)], key=lambda p: p.get("at", 0))
        for repo in _repos_of(rec):
            before = {}
            windowed = set()
            for prior in ordered:                       # oldest first: the earliest tip is the reference
                for b, sha in (prior["tips"].get(repo) or {}).items():
                    before.setdefault(b, sha)
                    windowed.add(b)
            if latest and isinstance(latest.get("tips"), dict):
                for b, sha in (latest["tips"].get(repo) or {}).items():
                    before.setdefault(b, sha)
            if not before:
                continue
            refs = _local_refs(repo)
            if refs is None:
                continue
            # (REPOSITORY, BRANCH) KEYING IS APPLIED HERE TOO, not only where the
            # snapshot is taken: a window opened before 2026-09-14 recorded the
            # name `main` for every repository at once, and those windows are
            # still on disk when this lands.
            protected = _protected_names(repo)
            own = None
            for b, old in sorted(before.items()):
                if not _is_protected(protected, b):
                    continue
                cur = refs.get(b)
                if cur == old:
                    continue
                action = "MOVED"
                if cur is None:
                    why = "deleted"
                elif not is_ancestor(repo, old, cur):
                    why = "moved to %s, which does not descend from %s (a rewind, a force-move or a symref)" % (cur[:12], old[:12])
                elif b not in windowed:
                    landed = land_by_another_conversation(repo, b, old, cur)
                    if not landed:
                        continue                        # no call open: a descendant is the lead's land, whatever it carries
                    action, why = "LANDED", landed      # except when the land record names another conversation
                else:
                    if own is None:
                        # The agent's own unlanded tips are measured against the
                        # recorded branch's SNAPSHOT tip, never the tip read now:
                        # after a doorway commit the branch's current tip IS the
                        # agent's commit, and subtracting it would call the move
                        # the lead's.
                        _branch, target, _why_not = integration_target([rec], repo)
                        base = before.get(_branch, target) if _branch else target
                        own = _own_unlanded_tips(rec, repo, refs, base)
                    if any(t == cur or is_ancestor(repo, t, cur) for t in own):
                        why = "moved to %s, which carries this agent's own unlanded work (it committed or merged onto it)" % cur[:12]
                    else:
                        landed = land_by_another_conversation(repo, b, old, cur)
                        if not landed:
                            continue                    # a descendant carrying none of the agent's work: the lead's land
                        action, why = "LANDED", landed  # except when the land record names another conversation
                if cur is not None:
                    if action == "LANDED":
                        # ANOTHER CONVERSATION'S LAND, REPORTED TO THE THREAD IT
                        # MOVED UNDER. Nothing was lost and nothing is changed
                        # here either: a fast-forward is announced, not judged.
                        # It is deliberately NOT one of the classes
                        # scripts/lib/protected-ref-moves.py reads — that
                        # reader's whole question is "were commits lost", and
                        # the answer here is no.
                        findings.append((action, repo, b, old, cur, why))
                        PROTECTED_REF_FINDINGS.append((action, repo, b, old, cur, why))
                        event("protected-ref-landed-elsewhere", key=rec["key"], repo=repo, branch=b,
                              tip=old, found=cur, why=why,
                              action="reported: another conversation's land, nothing was changed")
                        continue
                    # A MOVE IS REPORTED AND LEFT ALONE. See the docstring: the
                    # shape cannot tell Rich's land from an agent's doorway, and
                    # the write this used to make was destructive, self-
                    # triggering, and aimed at a tip no one decided on.
                    findings.append(("MOVED", repo, b, old, cur, why))
                    PROTECTED_REF_FINDINGS.append(("MOVED", repo, b, old, cur, why))
                    event("protected-ref-moved", key=rec["key"], repo=repo, branch=b, tip=old,
                          found=cur, why=why, action="reported: the engine does not move a ref back")
                    continue
                # A DELETION, AND ONLY A DELETION, IS PUT BACK — by the
                # create-only, attributed write above, which cannot clobber, move
                # or oscillate.
                rc, _o, err = _recreate_deleted_ref(repo, b, old, rec.get("name") or rec["key"])
                if rc != 0:
                    event("protected-ref-restore-failed", key=rec["key"], repo=repo, branch=b, tip=old, err=err.strip()[:300])
                    continue
                findings.append(("RESTORED", repo, b, old, "", why))
                PROTECTED_REF_FINDINGS.append(("RESTORED", repo, b, old, "", why))
                event("protected-ref-restored", key=rec["key"], repo=repo, branch=b, tip=old, found=cur, why=why)
        if findings:
            with Lock():
                fresh = load_agent(rec["key"])
                if fresh:
                    fresh.setdefault("history", []).extend(
                        {"at": iso(), "fact": _PROTECTED_REF_FACTS[a],
                         "repo": r, "branch": b, "tip": o, "found": c, "why": w}
                        for a, r, b, o, c, w in findings)
                    save_agent(fresh)
    except (OSError, ValueError, SpecError):
        pass
    return findings


def _take_snapshots(key, call="", all_open=False, background=False, rec=None):
    """CONSUME this call's window — or, at the end of the run, every open one —
    and return what was in it. Consumed whatever the outcome: one creation is
    attributed once, and a Post whose own Pre never ran attributes nothing.

    A BACKGROUNDED CALL'S WINDOW STAYS OPEN (round 8, item 8). When the Post
    says the call was issued with `run_in_background` — the platform's own
    stamped field, never the text of the command — the call's process is, by
    the platform's own word, still running after this Post. Its window is
    read and compared now, then written BACK marked `background`, and every
    later observation of this agent judges it again, until the process is
    CONFIRMED ended (`_background_windows`) or the run ends. That is what lets
    a ref the backgrounded process creates AFTER its call's Post — measured on
    this machine: a Bash call returned 3 s before its process finished
    (certification-frank-recorded-attribution-2026-09-12-probe.py) — be judged
    against the window the process actually belongs to, however many calls
    the agent makes in between (hunt part 4, finding 6: the window used to be
    consumed at the next observation, and a ref the process made after a
    second, whole call was attributed to nobody).
    Returns (priors, background_priors): the two are judged apart, because a
    background window must NOT be unioned with a later snapshot that already
    contains what its process created (see `observe_created_refs`)."""
    own = []
    if all_open:
        own = _open_slots(key)
    elif call:
        p = _slot_path(key, call)
        # A POST WHOSE OWN WINDOW IS NOT THERE STILL ENDS SOME CALL OF THIS
        # AGENT. The platform does not always carry `tool_use_id` on both
        # halves, and a Pre keyed / Post unkeyed pair (or the reverse) used to
        # find nothing at either end: the ref created inside that call was
        # attributed to nobody until the end of the run, and after the run
        # ended, to nobody at all. So an unpaired Post falls back to the OLDEST
        # open window, which is the one least likely to have another Post
        # coming.
        own = [p] if os.path.exists(p) else [q for q in _open_slots(key) if not _is_background(q)][:1]
    else:
        own = [p for p in _open_slots(key) if os.path.basename(p).startswith("u.") and not _is_background(p)][:1] \
            or [q for q in _open_slots(key) if not _is_background(q)][:1]
    rec = rec or {"key": key}
    cache = {}
    priors, bg_priors = [], []
    for p in own:
        prior = read_json(p)
        if prior and isinstance(prior.get("repos"), dict):
            if prior.get("background"):
                bg_priors.append(prior)
            else:
                priors.append(prior)
        if background and not all_open and prior and isinstance(prior.get("repos"), dict):
            state = _background_call_state(rec, prior.get("call") or call, cache)
            if state != "ended":
                prior["background"] = True              # the process outlives the call: keep the window
                # agent_hold's record of this call's shell was READ here, so
                # its later disappearance is agent_hold pruning an ended call
                prior["hold_seen"] = state == "running"
                write_json(p, prior)
                continue
            # its shell already ended before this Post: nothing outlives the call
        try:
            os.unlink(p)
        except OSError:
            pass
    bg_priors += _background_windows(rec, all_open=all_open, exclude=own, cache=cache)
    return priors, bg_priors


def _is_background(slot_path):
    prior = read_json(slot_path)
    return bool(prior and prior.get("background"))


def _background_call_state(rec, call, cache=None):
    """What agent_hold's own record of this background call's shell says:
    "running", "ended", "absent" (no record) or "unknown".

    Every Bash call of a subagent is recorded by agent_hold (its PreToolUse
    rewrite, then the shell records ITSELF: pid, parent and start time). A
    call is "ended" only when that record exists with its shell's pid and the
    shell is gone, a zombie, or the pid now belongs to another process; it is
    the one fact the engine has that says a background command FINISHED.
    Anything unreadable is "unknown", never "ended"."""
    if not call:
        return "unknown"
    cache = {} if cache is None else cache
    try:
        ah = _agent_hold()
        session, agent = str(rec.get("session_id") or ""), str(rec.get("agent_id") or "")
        if not ah._valid_ids(session, agent, call):
            return "unknown"
        stem = os.path.join(ah._shell_dir(session, agent), call)
        if not os.path.exists(stem + ".json"):
            return "absent"
        if ah._read_pid(stem + ".pid") is None:
            return "unknown"                 # recorded, but its shell has not recorded itself
        if "table" not in cache:
            cache["table"] = ah.snapshot()
        table = cache["table"]
        live = [c for c in ah.calls(session, agent, table)
                if c["tid"] == call and not table[c["pid"]]["stat"].startswith("Z")]
        return "running" if live else "ended"
    except Exception:
        return "unknown"


def _background_windows(rec, all_open=False, exclude=(), cache=None, ended_only=False):
    """The agent's open background windows, for the observation that is now
    judging them. A window whose call is CONFIRMED ended — agent_hold's record
    shows its shell gone, or that record, once read with a shell in it, has
    been pruned (agent_hold prunes only a call whose shell and tagged children
    are gone) — or every window at the end of the run, is returned for its
    last judgment and consumed. Any other is returned and KEPT: its process
    may still be running, and a ref it makes later is still its (finding 6).

    Each returned row carries `judged` as it was BEFORE this observation, so
    the protected-ref check sees a window once, as before; attribution judges
    it every time, and attribution never records one ref twice.

    `ended_only` (the start of a call) leaves still-running windows untouched."""
    key = rec["key"]
    cache = {} if cache is None else cache
    out = []
    for p in _open_slots(key):
        if p in exclude or not _is_background(p):
            continue
        prior = read_json(p)
        if not (prior and isinstance(prior.get("repos"), dict)):
            try:
                os.unlink(p)
            except OSError:
                pass
            continue
        ended = all_open
        if not ended:
            state = _background_call_state(rec, prior.get("call"), cache)
            ended = state == "ended" or (state == "absent" and bool(prior.get("hold_seen")))
        if ended_only and not ended:
            continue
        if ended:
            try:
                os.unlink(p)
            except OSError:
                pass
        elif not prior.get("judged"):
            write_json(p, dict(prior, judged=True))
        out.append(prior)
    return out


def _background_before(bg_priors):
    """ONE before-set for every background window judged together: per
    repository, the INTERSECTION of theirs. A ref is new when ANY background
    call whose process may still be running started without it, and the
    intersection is exactly that. (The union `_before_set` takes is for
    ordinary windows, where a window leaked by a refused call must not widen
    the comparison; a background window is not leaked, its Post stamped it.)"""
    repos = {}
    for prior in bg_priors:
        for repo, names in (prior.get("repos") or {}).items():
            s = set(names or [])
            repos[repo] = s if repo not in repos else (repos[repo] & s)
    return {"key": (bg_priors[0] or {}).get("key", ""), "call": "", "background": True,
            "repos": {r: sorted(s) for r, s in repos.items()}}


def _settle_ended_background(rec):
    """At the START of a call: every background window whose call is
    confirmed ended is judged one last time and closed, before the call can
    do anything. Still-running windows are left for the Post to judge."""
    settled = _background_windows(rec, ended_only=True)
    if not settled:
        return []
    latest = read_json(_latest_path(rec["key"]))
    _restore_protected_refs(rec, [p for p in settled if not p.get("judged")], latest)
    return _attribute_new_refs(rec, [_background_before(settled)], None)


def _before_set(priors, latest, repo):
    """WHAT ALREADY EXISTED WHEN ANY OF THIS AGENT'S OPEN CALLS STARTED — the
    UNION of their before-sets, plus the most recent snapshot it ever took.

    It used to be the INTERSECTION, described as "the widest one decides". With
    one window leaked by a refused call, the widest one was the start of the
    run, so the end-of-run comparison asked "what appeared while this agent
    existed" instead of "what changed during this call", and a branch Rich cut
    between two of the agent's calls came back as the agent's.

    The union is the other direction and it is the safe one. A ref that already
    existed when ANY call of this agent started was not created by that call,
    and with windows overlapping there is no way to say which call made it. The
    cost is under-attribution in one narrow shape — a ref created during a call
    that is still open when an even later call starts, and whose own Post never
    arrives — and under-attribution leaves a branch behind for the point-3
    sweep to name, while over-attribution deletes somebody else's work.

    `latest` joins the union even after its own window was consumed, because
    that is exactly the fact a leaked window is missing: the run got as far as
    THAT call, so anything already present then is not the leaked call's doing.
    It never adds attribution of its own — it only ever removes some."""
    before = None
    for prior in priors:
        if repo in (prior.get("repos") or {}):
            b = set(prior["repos"][repo] or [])
            before = b if before is None else (before | b)
    if latest and repo in (latest.get("repos") or {}):
        b = set(latest["repos"][repo] or [])
        before = b if before is None else (before | b)
    return before


def _drop_snapshots(key):
    """Every open window of this agent, thrown away unconsumed."""
    for p in _open_slots(key):
        try:
            os.unlink(p)
        except OSError:
            pass
    try:
        os.unlink(_latest_path(key))
    except OSError:
        pass
    try:
        os.rmdir(_refs_dir(key))
    except OSError:
        pass
    try:
        os.unlink(_p("refs", key + ".json"))      # the pre-2026-09-12 single slot
    except OSError:
        pass


def _refs_recorded_elsewhere(exclude_key):
    """Every (repo, branch) recorded on some OTHER agent's record: its
    workspaces' own branches and the refs it is recorded as having created.
    Filter 2, and the hard guarantee for the shape that broke on 2026-09-11."""
    out = set()
    for r in all_agents(include_done=True):
        if r.get("key") == exclude_key:
            continue
        for w in r.get("workspaces") or []:
            if w.get("branch"):
                out.add((w.get("repo"), w["branch"]))
        for pair in r.get("created_branches") or []:
            if len(pair) >= 2:
                out.add((pair[0], pair[1]))
    return out


# A reflog subject says whether this workspace MADE the commit its HEAD moved
# to, or merely visited one that already existed. "checkout: moving from X to Y"
# is a visit; "commit: ...", "merge ...", "rebase ...", "cherry-pick ...",
# "revert ...", "am ..." and "pull ..." all produce the commit they land on.
# Counting visits is how a ref Rich cut at a commit the agent only BORROWED
# became the agent's: the agent checked that commit out for one call and its
# reflog then swore the commit was its own work.
_MADE_HERE = ("commit", "merge", "rebase", "cherry-pick", "revert", "am", "pull")


def _made_here(subject):
    head = (subject or "").split(":")[0].strip().lower()
    return head.split(" ")[0] in _MADE_HERE if head else False


def _own_unlanded_tips(rec, repo, refs, target):
    """EVERY COMMIT THIS AGENT'S OWN WORKSPACES IN `repo` HAVE POINTED AT, less
    the ones already in the branch this work integrates on. Filter 4's other
    half: an agent with none of these has produced nothing of its own here, and
    is attributed nothing.

    Four sources, all facts about the agent's own workspaces or its own record:
    its workspace branches' tips, its workspaces' current HEADs, its workspaces'
    own HEAD REFLOGS, and the refs it is already recorded as having created.

    THE REFLOG HERE IS THE PER-WORKSPACE ONE, AND IT IS NOT THE REFLOG THAT WAS
    REJECTED. What was rejected is `logs/refs/heads/<name>`, the BRANCH's reflog:
    git writes `branch: Created from <X>` into it identically whoever ran the
    command and wherever they ran it, so it says nothing about who made the ref.
    `$GIT_DIR/logs/HEAD` of a workspace is a different file with a different
    property — it is PRIVATE TO THAT WORKSPACE, it is written when that
    workspace's HEAD moves, and it is deleted with it.

    IT IS WHAT CLOSES THE SIDE BRANCH INSIDE ONE TOOL CALL. An agent that runs
    `git checkout -b sidework && <commit> && git checkout <its own branch>` in a
    single call is back on its own empty branch by the time anything looks: its
    branch tip and its current HEAD are both the integration tip, so without the
    reflog it has no unlanded work of its own and the commit on `sidework` is
    attributed to nobody — which is exactly the hole where land() reported
    success and the work was in no integration branch (points 5, 8). Its HEAD
    reflog still carries the commit it made, so the side branch is on one line of
    history with something of its own, and the land is held until that branch is
    merged or discarded."""
    tips = set()
    bases = set()
    for w in live_workspaces(rec):
        if w.get("repo") != repo:
            continue
        if w.get("branch") and refs.get(w["branch"]):
            tips.add(refs[w["branch"]])
        if not w.get("path") or not os.path.isdir(w["path"]):
            continue
        rc, out, _ = git(w["path"], "rev-parse", "HEAD")
        if rc == 0 and out.strip():
            tips.add(out.strip())
        rc, out, _ = git(w["path"], "reflog", "show", "--format=%H%x09%gs", "HEAD")
        if rc == 0:
            lines = [l for l in out.splitlines() if l.strip()]
            for line in lines:
                sha, _tab, subject = line.partition("\t")
                sha = sha.strip()
                if not sha:
                    continue
                if _made_here(subject):
                    tips.add(sha)
            if lines:
                # The OLDEST entry is where this workspace started — the commit
                # git put its HEAD on when the workspace was created. It is a
                # fact about the agent's own workspace, recorded by git, and it
                # is the floor used when point 14's record does not exist yet.
                oldest = lines[-1].partition("\t")[0].strip()
                if oldest:
                    bases.add(oldest)
    for pair in rec.get("created_branches") or []:
        if len(pair) >= 2 and pair[0] == repo and refs.get(pair[1]):
            tips.add(refs[pair[1]])
    if not target:
        # NOTHING IS RECORDED FOR THIS REPOSITORY YET (point 14 not yet done),
        # so "less the ones already in the integration branch" has no branch to
        # subtract. The floor is then each workspace's OWN starting commit: work
        # that was already there when the agent's workspace was created is not
        # the agent's, whatever any record says. Without this floor the base
        # commit itself counts as the agent's work, every ref in the repository
        # descends from it, and every ref is "on this agent's line of work" —
        # which is how a ref cut at a tip the agent only BORROWED became the
        # agent's the moment attribution stopped waiting for the record.
        if not bases:
            return set(t for t in tips if t)
        return set(t for t in tips
                   if t and not any(is_ancestor(repo, t, b) for b in bases))
    return set(t for t in tips if t and not is_ancestor(repo, t, target))


def observe_created_refs(rec, call="", all_open=False, background=False):
    """THE SECOND HALF OF THE PAIR (point 3): the refs this agent created during
    the tool call that is now ending, recorded against it. Returns what it added.

    `call` names WHICH of the agent's open windows this is the far end of, so
    two of its own calls open at once no longer clobber each other. `all_open`
    is the end of the run: every window still open is the last observation.
    `background` says the platform stamped this call `run_in_background`: its
    window is compared now and kept open for the process that outlives it
    (`_take_snapshots`).

    The window is CONSUMED here, whatever the outcome: one creation is
    attributed once. A background window is the one exception: it is judged
    at every later observation of this agent against ITS OWN before-set — a
    later snapshot already holds what its process created, so the union rule
    would call that ref old, and it is not — and consumed only once its
    process is confirmed ended or the run ends (`_background_windows`).

    IT DOES NOT NEED AN INTEGRATION BRANCH TO BE RECORDED. The record is a
    filter on what is at stake, never a gate on whether the observation happens
    at all — attribution is made once and never again, so a missing record used
    to mean "attributed to nobody, permanently"."""
    # READ BEFORE CONSUMING. The last snapshot has to be in hand before the
    # windows are thrown away, or the end-of-run pass is judged against the
    # leaked window alone -- which is the widening this whole change ends.
    latest = read_json(_latest_path(rec["key"]))
    priors, bg_priors = _take_snapshots(rec["key"], call, all_open, background, rec=rec)
    if all_open:
        _drop_snapshots(rec["key"])
    # The effects check runs FIRST, on the snapshots as taken: a protected ref
    # the call DELETED is put back, and one it MOVED is reported and left where
    # it is (it never moves a ref back — see _restore_protected_refs), before
    # anything is attributed. A background window kept open across several
    # observations takes part the first time only, as it did when the next
    # observation consumed it, so a move is reported once.
    _restore_protected_refs(rec, priors + [p for p in bg_priors if not p.get("judged")], latest)
    added = []
    if bg_priors:
        # THE BACKGROUND WINDOWS, JUDGED APART: against their own before-sets,
        # never unioned with `latest` — the next call's snapshot was taken
        # while the backgrounded process was still running, so it already
        # holds what that process created, and the union would call it old.
        # Several open at once are judged as one, against the intersection of
        # their before-sets (`_background_before`).
        # The four filters still apply (own unlanded work, not somebody
        # else's, at stake), which is what keeps a second agent's refs out.
        bg_priors = [_background_before(bg_priors)]
        added += _attribute_new_refs(rec, bg_priors, None)
    # THE END OF THE RUN COMPARES ONCE MORE AGAINST THE LAST SNAPSHOT, WHETHER
    # OR NOT A WINDOW IS STILL OPEN (round 8, item 8). A backgrounded process
    # outlives its tool call — measured on this machine: a Bash call returned
    # 3 s before its process finished (certification-frank-recorded-attribution
    # -2026-09-12-probe.py, cases outside-stray / outside-side) — so a ref it
    # creates appears AFTER that call's PostToolUse consumed the window. The
    # end signal used to observe only windows still open, so with the last
    # window consumed `priors` was empty and the ref was compared against
    # nothing: attributed to nobody, land() reported success, the ref was left
    # behind (esc-20260912T225456Z-d34bf4e6, the one RED probe of round 7).
    # `latest` is the snapshot the run got as far as; anything present in it
    # was not created after it, and the four filters below still apply, so
    # this widens nothing but the moment of the last comparison. Points 3, 9.
    if all_open and latest and isinstance(latest.get("repos"), dict):
        priors.append(latest)
    if not priors:
        return added
    return added + _attribute_new_refs(rec, priors, latest)


def _attribute_new_refs(rec, priors, latest):
    """THE FOUR FILTERS, over the union of the before-sets in `priors` (plus
    `latest`, see `_before_set`): what is new, not somebody else's, at stake,
    and on this agent's own line of work is recorded against it. Returns what
    it added. Shared by the end of a call (observe_created_refs), the end of
    the run, and the start of a call with nothing in flight (snapshot_refs)."""
    try:
        mine = []
        repos = _repos_of(rec)
        seen_repos = sorted(set(r for prior in priors for r in prior["repos"]))
        for repo in seen_repos:
            if repo not in repos:
                continue
            # A ref is NEW when it did not exist at the start of ANY of this
            # agent's open calls (`_before_set` says why that is a union and not
            # an intersection).
            before = _before_set(priors, latest, repo)
            if before is None:
                continue
            refs = _local_refs(repo)
            if refs is None:
                continue                              # unreadable: observe nothing
            fresh_refs = [b for b in sorted(refs) if b not in before
                          and not b.startswith(CODEX_PREFIX)]
            if not fresh_refs:
                continue
            # ATTRIBUTION DOES NOT WAIT FOR THE RECORD, AND THIS IS ITEM 1.
            # It used to: with no integration branch recorded, this gave up on
            # the whole observation and wrote an `attribution-skipped` event
            # that had one producer and NO CONSUMER. Attribution happens once,
            # at the end of a tool call, so "skipped" meant attributed to NOBODY
            # PERMANENTLY -- recording the branch afterwards unblocked the land
            # and never went back for what was skipped, and `land()` then
            # reported success over a commit that had reached no integration
            # branch. The control that recorded the branch first held, so the
            # cause was the ORDER, and the fix is to take the order out: the
            # target is now only ever a FILTER here, never a precondition.
            #
            # With no target the judgment is deliberately wider -- the two tests
            # it can make no longer run, so a ref that is already fully merged
            # is attributed where it would otherwise have been skipped. That is
            # the right way round. An over-attributed ref is still measured at
            # land time, where a ref already in the integration branch simply
            # passes and a ref that is not REFUSES the land by name. A ref
            # attributed to nobody is measured nowhere, ever.
            _branch, target, _why_not = integration_target([rec], repo)
            wl = worktree_list(repo) or []
            held_by_main = (wl[0].get("branch") or "") if wl else ""
            elsewhere = _refs_recorded_elsewhere(rec["key"])
            own_branches = set(w.get("branch") for w in live_workspaces(rec)
                               if w.get("repo") == repo)
            own = _own_unlanded_tips(rec, repo, refs, target)
            if not own:
                continue     # it has produced nothing of its own here (filter 4)
            for b in fresh_refs:
                if b == held_by_main or b in own_branches or (repo, b) in elsewhere:
                    continue
                tip = refs[b]
                if target and is_ancestor(repo, tip, target):
                    continue                          # entirely landed: nothing at stake
                if not any(t == tip or is_ancestor(repo, t, tip) or is_ancestor(repo, tip, t)
                           for t in own):
                    continue                          # not on this agent's line of work
                if (repo, b) not in mine:
                    mine.append((repo, b))
        if not mine:
            return []
        with Lock():
            fresh = load_agent(rec["key"])
            if not fresh:
                return []
            have = [tuple(x) for x in (fresh.get("created_branches") or [])]
            own_ws = set((w.get("repo"), w.get("branch")) for w in fresh.get("workspaces") or [])
            add = [x for x in mine if x not in have and x not in own_ws]
            if not add:
                return []
            fresh["created_branches"] = [list(x) for x in have + add]
            save_agent(fresh)
        event("branch-created", key=rec["key"], branches=[list(x) for x in add])
        return add
    except (OSError, ValueError, SpecError):
        return []


def observe(payload):
    """The catch-all PostToolUse's half of the pair (point 3). A payload with no
    agent_id is the lead's own call and can never be an agent's creation; a
    finished agent observes nothing and its open snapshot is thrown away (point
    9), so a ref Rich cuts to rescue the work afterwards stays his."""
    recovery, host = _recovery_adapter()
    recovery.finish_tool(host, payload)
    aid = str(payload.get("agent_id") or "")
    if not aid:
        return []
    key = key_for_id(aid)
    rec = load_agent(key) if key else None
    if not rec:
        return []
    if finished_state(rec)[0]:
        _drop_snapshots(rec["key"])
        return []
    ti = payload.get("tool_input") if isinstance(payload.get("tool_input"), dict) else {}
    background = str(payload.get("tool_name") or "") == "Bash" and bool(ti.get("run_in_background"))
    return observe_created_refs(rec, str(payload.get("tool_use_id") or ""), background=background)


def _chain(rec):
    """The agent plus every agent whose work it continues (point 7)."""
    out, seen, todo = [], set(), [rec]
    while todo:
        r = todo.pop()
        if r["key"] in seen:
            continue
        seen.add(r["key"])
        out.append(r)
        for k in r.get("continues") or []:
            o = load_agent(k)
            if o:
                todo.append(o)
    return out


def recovery_chain(rec, observation=False):
    """Correction identity without changing merge/discard/cleanup ownership.

    New registrations carry exact host-bound correction keys. Legacy
    lands-pending links remain candidates, not permission to combine budgets.
    They need source-qualified reconciliation before recovery enforcement.
    """
    out, seen, todo = [], set(), [rec]
    while todo:
        current = todo.pop()
        if current["key"] in seen:
            continue
        seen.add(current["key"])
        out.append(current)
        # Observation follows every predecessor's own observation projection,
        # so a chain of replacements keeps one job identity, not only one hop.
        assignment = (current is not rec and observation and current.get("recovery_observation_assignment")) or \
            current.get("recovery_assignment") or {}
        refs = assignment.get("correction_of", []) if assignment else current.get("continues", [])
        if assignment and assignment.get("kind") != "implementation":
            refs = []
        for reference in refs:
            previous = load_agent(reference)
            if not previous:
                raise SpecError("recovery predecessor %s is missing; reconcile its source record" % reference)
            projected = (observation and previous.get("recovery_observation_assignment")) or previous.get("recovery_assignment") or {}
            if assignment and not (observation and _recovery_unclassified(previous)) and projected.get("outcome") not in (
                    None, assignment.get("outcome")):
                raise SpecError("recovery relationship joins different authorized outcomes")
            todo.append(previous)
    return sorted(out, key=lambda r: (str(r.get("registered_at") or ""), r["key"]))


def _recovery_adapter():
    import types
    here = os.path.join(engine_root(), "scripts", "lib")
    if here not in sys.path:
        sys.path.insert(0, here)
    import recovery_controls
    return recovery_controls, types.SimpleNamespace(**globals())


def recovery_stop(key, record, control, tip):
    recovery, host = _recovery_adapter()
    return recovery.stop(host, key, record, control, tip)


def _baseline_review_identity(rec, observation=False):
    assignment = rec.get("recovery_assignment") or {}
    authority = assignment.get("authority") or {}
    return not observation and (authority.get("mode") in ("off", "observe", "questions-only") or
        (not authority.get("host") and _recovery_unclassified(rec)))


def review_chain(rec, observation=False):
    """Review identity and history use the same baseline or attested resolver."""
    if _baseline_review_identity(rec, observation):
        return sorted(_chain(rec), key=lambda r:str(r.get("registered_at") or ""))
    return recovery_chain(rec, observation=observation)


def recovery_work_key(rec, observation=False):
    if _baseline_review_identity(rec, observation):
        # Baseline review identity follows existing lifecycle continuations.
        # Shadow/enforced correction identity is a separate attested projection.
        predecessor = review_chain(rec)[0]
        return "teammate:"+predecessor["key"]
    outcome = (rec.get("recovery_assignment") or {}).get("outcome") or ""
    if outcome.startswith("app:") and (rec.get("recovery_assignment") or {}).get("kind") == "implementation":
        return outcome
    return "teammate:%s" % recovery_chain(rec, observation=observation)[0]["key"]


def _recovery_unclassified(rec):
    assignment = rec.get("recovery_assignment") or {}
    if assignment.get("kind", "implementation") != "implementation":
        return False
    authority = assignment.get("authority") or {}
    return not (authority.get("host") or (authority.get("inspection") and not assignment.get("observation_only")))


def _baseline_recovery_assignment(payload, rec, mode):
    prompt = str((payload.get("tool_input") or {}).get("prompt") or "")
    host = payload.get("richos_recovery_assignment") or {}
    roles = prompt_lines(prompt, "recovery-role")
    kind = host.get("kind") or (roles[0] if roles else "implementation")
    return dict(kind=kind if kind in ("implementation", "review", "helper") else "implementation",
        outcome=host.get("outcome") or "teammate:"+rec["key"], correction_of=[],
        acceptance=host.get("acceptance") or hashlib.sha256(prompt.encode()).hexdigest(),
        observation_only=mode in ("observe", "questions-only"), authority=dict(host=host, mode=mode,
            session=str(payload.get("session_id") or ""), tool_use_id=str(payload.get("tool_use_id") or ""),
            transcript=str(payload.get("transcript_path") or ""), brief_sha256=hashlib.sha256(prompt.encode()).hexdigest()))


def _recovery_assignment(payload, rec, continues, lands_pending, inspection=None):
    recovery, _host = _recovery_adapter()
    declaration = recovery.R.mode(rec.get("entity") or os.environ.get("CLAUDE_PROJECT_DIR") or payload.get("cwd"))
    if declaration["mode"] == "off":
        return _baseline_recovery_assignment(payload, rec, "off")
    if declaration["mode"] == "enforce":
        return _classified_recovery_assignment(payload, rec, continues, lands_pending, inspection)
    # Observation has its own projection. Never rewrite live review identity
    # or predecessor records because a would-be classification changed.
    # questions-only takes the same projection: identity gaps never refuse.
    baseline = _baseline_recovery_assignment(payload, rec, declaration["mode"])
    rec.pop("recovery_observation_assignment", None)
    rec.pop("recovery_observation_gap", None)
    try:
        proposal = _classified_recovery_assignment(payload, rec, continues, lands_pending, inspection, observing=True)
        proposal["observation_only"] = True
        rec["recovery_observation_assignment"] = proposal
    except (SpecError, OSError, ValueError) as error:
        rec["recovery_observation_gap"] = str(error)
        baseline["authority"]["observation_gap"] = str(error)
    return baseline


def _classified_recovery_assignment(payload, rec, continues, lands_pending, inspection=None, observing=False):
    library = os.path.join(os.path.dirname(os.path.dirname(os.path.realpath(__file__))), "scripts", "lib")
    if library not in sys.path:
        sys.path.insert(0, library)
    import recovery
    virtual = {}
    lookup = lambda key: virtual.get(key) or load_agent(key)
    prompt = str((payload.get("tool_input") or {}).get("prompt") or "")
    # Top-level host metadata is supplied by the app adapter after it verifies
    # the exact prepared Agent payload. It is never read from the model's tool
    # input or an invented helper name.
    host = payload.get("richos_recovery_assignment") or {}
    roles = prompt_lines(prompt, "recovery-role")
    classified = (inspection or {}).get("assignment_classification") or {}
    if not host and not classified:
        import recovery
        declaration = recovery.mode(rec.get("entity") or os.environ.get("CLAUDE_PROJECT_DIR") or payload.get("cwd"))
        if declaration["mode"]=="enforce":
            raise SpecError("enforced native dispatch requires the bound job classification from its existing inspection")
    kind = host.get("kind") or classified.get("kind") or (roles[0] if roles else "implementation")
    if kind not in ("implementation", "review", "helper"):
        raise SpecError("recovery-role must name implementation, review or helper")
    refs = list(continues)
    if classified:
        refs = classified.get("correction_of", []) if classified.get("relation")=="corrective" else []
        if classified.get("relation")=="unavailable":
            import recovery
            declaration = recovery.mode(rec.get("entity") or os.environ.get("CLAUDE_PROJECT_DIR") or payload.get("cwd"))
            if declaration["mode"]=="enforce":
                raise SpecError("job identity unavailable: "+classified["evidence"])
        legacy = [lookup(ref) for ref in refs]
        if legacy and all(r and _recovery_unclassified(r) for r in legacy):
            import recovery
            legacy.sort(key=lambda r:(str(r.get("registered_at") or ""),r["key"]))
            origin = legacy[0]["key"]
            for old in legacy:
                for space in old.get("workspaces") or []:
                    if space.get("repo") and recovery.read(recovery.key(space["repo"],"teammate:"+old["key"])):
                        raise SpecError("legacy job already has tracked recovery; explicit identity reconciliation required")
            for old in legacy:
                old = dict(old)
                old["recovery_assignment"] = dict(kind="implementation", outcome="teammate:"+origin,
                    correction_of=[] if old["key"]==origin else [origin], acceptance="legacy:"+origin,
                    authority=dict(bootstrap=True, inspection=(inspection or {}).get("id"), evidence=classified["evidence"],
                        transcript=str(payload.get("transcript_path") or "")))
                virtual[old["key"]] = old
                if not observing:
                    save_agent(old)
    elif kind == "implementation":
        # `lands-pending` describes merge ownership, not semantic identity.
        # Explicit corrective links or the bound inspection supply identity.
        for reference in [v.split()[0] for v in prompt_lines(prompt, "correction-of") if v.split()]:
            previous = _resolve_record(reference, str(payload.get("session_id") or ""))
            refs.append(previous["key"])
    else:
        refs = []
    inherited = []
    # An observing projection inherits from the predecessor's projection too.
    projection = lambda r: (observing and r.get("recovery_observation_assignment")) or r.get("recovery_assignment") or {}
    for reference in refs:
        previous = lookup(reference)
        if not previous:
            raise SpecError("recovery predecessor %s is missing" % reference)
        if projection(previous).get("kind", "implementation") != "implementation":
            raise SpecError("a recovery correction cannot inherit a helper or review budget")
        inherited.append(projection(previous).get("outcome") or
                         "teammate:%s" % recovery_chain(previous, observation=observing)[0]["key"])
    if len(set(inherited)) > 1:
        raise SpecError("one correction cannot silently combine different jobs")
    source = {"session": str(payload.get("session_id") or ""),
              "tool_use_id": str(payload.get("tool_use_id") or ""),
              "transcript": str(payload.get("transcript_path") or ""),
              "brief_sha256": hashlib.sha256(prompt.encode()).hexdigest()}
    if host:
        source["host"] = host
    if classified:
        source["inspection"] = (inspection or {}).get("id")
        source["classification"] = classified
        source["classification_index"] = (inspection or {}).get("classification_index")
    return {"kind": kind,
            "outcome": inherited[0] if inherited else (host.get("outcome") or "teammate:%s" % rec["key"]),
            "correction_of": sorted(set(refs)), "authority": source,
            "acceptance": host.get("acceptance") or (projection(lookup(refs[0])).get("acceptance")
                          if refs else None) or hashlib.sha256(prompt.encode()).hexdigest()}


def _branch_targets(chain):
    """Every branch this work has: each workspace's own, plus every branch this
    agent is RECORDED as having created (created_branches). The record is what
    survives the workspace directory being deleted.

    A WORKSPACE CONTRIBUTES EXACTLY ONE BRANCH — ITS OWN — AND `branch_deleted_at`
    IS WHY THAT MATTERS. Until 2026-09-12 a workspace record could also carry an
    `extra_branches` list, which `_workspace_branches` folded in here. NOTHING
    EVER WROTE THAT FIELD: every reader (this function, the prune in `_delete`,
    the done-check) read a list that was always empty, so all three were dead
    code wearing the shape of a rule. It was also the one path in the file where
    a failed deletion could never be retried (point 13): the `branch_deleted_at`
    skip is per WORKSPACE, so the moment the workspace's own branch was deleted
    its extras dropped out of the target list — a retry could not name what it
    had failed to delete. Deleting the field deleted that path with it; nothing
    is skipped here now except a branch whose deletion has already succeeded."""
    out = []
    for r in chain:
        for w in r.get("workspaces") or []:
            if w.get("branch_deleted_at") or not w.get("branch"):
                continue
            if (w.get("repo"), w["branch"]) not in out:
                out.append((w.get("repo"), w["branch"]))
        for pair in r.get("created_branches") or []:
            t = (pair[0], pair[1])
            if t not in out:
                out.append(t)
    return out


# ---------------------------------------------------------------------------
# points 4, 7, 9, 10, 13 — land, discard, delete, retry
# ---------------------------------------------------------------------------

def integration_target(chain, repo):
    """(branch, tip, why_not) — the ref a land is proved against (point 14).

    THE ONE ANSWER. "Every part of the system that needs to know whether work
    has landed asks the same question: is it in the branch recorded for this
    work? None of them is allowed its own answer, and none of them assumes
    main." This function is that question. A consumer with no agent in hand asks
    `integration_for(repo)`, which is this with an empty chain.

    The branch comes from the body of work THIS CHAIN IS BOUND TO, and nowhere
    else. Nothing is inferred, no branch is frozen onto an agent, and nothing
    falls back to whatever the main checkout happens to have checked out: a land
    that cannot name its target REFUSES and names the command that records it.

    `chain` is the unit of work, and it is now a real input rather than a
    courtesy: a second body of work started in the same repository — which point
    5 permits while this one's agents are still running — must not move this
    chain's target. Each agent carries the work ID it was registered under; the
    branch is read live from that work, so a CORRECTION still reaches an agent
    in flight and a DIFFERENT body of work never does.

    THERE IS NO FALLBACK TO "CURRENT" FOR A CHAIN. A spawned agent is bound at
    its spawn or the spawn is refused (register_spawn, point 14), and an
    unregistered or provisional record binds at the first sweep after a body
    of work exists (scan_unregistered). A chain that is still bound to nothing
    REFUSES and names the command — reading the repository's current body of
    work at land time would be a guess about which work the agent belongs to,
    and both round-6 reviewers reproduced that guess moving a land verdict.
    Only a caller with NO chain (`integration_for`, a consumer asking about the
    repository itself) reads the current record, because that is the question
    it is asking."""
    repo = realpath(repo or "")
    if not repo or not os.path.isdir(repo):
        return "", "", "its repository %s cannot be read" % repo
    main_key = _norm_repo(repo)
    work = None
    for r in chain or []:
        wid = (r.get("integration_work") or {}).get(main_key)
        if wid:
            work = _work_by_id(wid)
            if work:
                break
    if work is None and chain:
        return "", "", ("%s is bound to no body of work in %s: it was registered while none was "
                        "recorded, and nothing guesses which one it belongs to (point 14). A spawned "
                        "agent is refused before this can happen; an unregistered workspace binds at "
                        "the next sweep once a branch is recorded: workspaces.sh integration --repo %s "
                        "--branch <main|dev/...> --why '<this body of work>'. Otherwise discard it "
                        "(point 7)." % (", ".join(r.get("name") or r.get("key") for r in chain), repo, repo))
    if work is None:
        work = integration_record(repo)
    branch = (work or {}).get("branch") or ""
    if not branch:
        return "", "", ("no branch is recorded for %s as the one this work integrates on. The branch a "
                        "body of work integrates on is RECORDED when that work starts (point 14); "
                        "nothing infers it. Record it: workspaces.sh integration --repo %s --branch "
                        "<main|dev/...> --why '<this body of work>'" % (repo, repo))
    main = main_checkout(repo) or repo
    tip = branch_tip(main, branch)
    if not tip:
        return branch, "", ("the recorded integration branch %s (body of work %s) does not exist in %s "
                            "any more. Correct the branch this work integrates on (point 14): "
                            "workspaces.sh integration --repo %s --branch <main|dev/...> --correct"
                            % (branch, (work or {}).get("id") or "?", repo, repo))
    return branch, tip, ""


def integration_for(repo):
    """(branch, tip, why_not) for a caller that has no agent in hand — the single
    answer every consumer asks instead of keeping its own.

    A SECOND CORRECT COPY IS STILL A SECOND ANSWER. Consumers that need to know
    whether work has landed — `completion-proof.py`, `land-completeness.py`,
    `land-residue-gate.py`, `unlanded-branches.py`, `guard-ci-red-lands.sh`,
    `guard-unresolved-claims.py`, `guard-idle-land.py`, `inflight.py` — call
    this. Where nothing is recorded it returns a `why_not` that NAMES THE
    RECORDING COMMAND, and the consumer abstains on it. None of them assumes
    main (point 14)."""
    return integration_target([], repo)


def land(ref, me="", auto=False, ignored_ok="", deadline=None, keep_ignored=None):
    """Point 4: landed means every workspace and branch is deleted. Landed is
    proved from git: every branch tip (and every workspace HEAD) is already in
    the branch this work INTEGRATES ON, which is the branch recorded when the
    work started — usually main, and its dev branch when it cannot reach main
    yet (point 14). An agent that produced nothing is landed (point 7).

    THE AUTOMATIC LAND KEEPS IGNORED FILES INSTEAD OF WAITING FOR A WAIVER
    (2026-10-04). Until then any ignored file the main checkout lacked (an
    in-flight ack file, build output nobody declared regenerable) held a
    merged, ended workspace pending until a person typed
    --ignored-not-needed: 418 of the 559 lands since 2026-09-21 were done by
    hand with that waiver and 51 landed on their own, and four merged
    workspaces sat 1-2 days. The refusal was swallowed in pending(), so
    nothing said why. Now, once every branch is proved to be in the
    integration branch, those entries are MOVED to <state>/kept/ and the land
    goes on: point 8 still holds (nothing is lost), and nobody has to
    remember. Uncommitted work and unreadable directories still refuse.
    `keep_ignored` defaults to `auto`; `merge_and_land` passes it too."""
    keep = auto if keep_ignored is None else keep_ignored
    rec = _resolve(ref, me)
    fin, _pz, why = finished_state(rec)
    if not fin:
        raise SpecError("%s is not finished (%s); only finished work is landed" % (rec["name"], why))
    sweep_scopes = land_sweep_scopes(ref, me)
    if rec.get("disposition") and rec["disposition"].get("kind") != "continued":
        kind = rec["disposition"]["kind"]
        if kind == "landed":
            # An explicit retry must finish failed deletion, not certify the
            # earlier eligibility receipt as completed cleanup. The canonical
            # deletion path rechecks landing after writers stop.
            # With the caller's deadline (V3-02): this retry used to drop it
            # and spend whatever its stages took.
            chain = _chain(rec)
            clean = _delete_chain(chain, "landed", deadline=deadline)
            fresh = load_agent(rec["key"]) or {}
            # A RETRY THAT REOPENED THE LANDING IS NOT A LAND (hunt part 4 v3,
            # V3-04). The reproof found new work, withdrew the disposition and
            # dropped the retry; answering a dict here let the CLI print
            # "landed ... it is retried" and exit 0. It is refused exactly as
            # the first land refuses the same change below.
            if (fresh.get("disposition") or {}).get("kind") != "landed":
                raise SpecError("landing eligibility changed during cleanup; the work was preserved")
            sweep_scratch_after_land(sweep_scopes, deadline=deadline)
            return {"landed": True, "already": kind, "cleanup_pending": not clean}
        return {"landed": True, "already": kind}
    chain = _chain(rec)
    # NOTHING IS STOPPED FOR WORK THAT PLAINLY CANNOT LAND (hunt part 4,
    # finding 13). An automatic attempt (a status, a spawn check, a turn end)
    # used to stop a finished worker's tests and servers and only then find
    # its commits were not merged, so it stayed pending with its useful work
    # killed. Shutdown can only ADD commits, never merge them, so a HEAD or a
    # branch already read to be outside the integration branch cannot become
    # landed by stopping anything: that is refused first. Only a definite
    # answer refuses here; whatever could not be read is left to the full
    # proof below.
    early = _not_in_integration(rec, chain, deadline=deadline, definite_only=True)
    if early:
        raise SpecError(_not_landed_message(rec, early))
    # Shutdown can flush files or create commits. Prove landing only after it.
    # No stage starts past the caller's deadline (V2-03): out of time is a
    # Deadline, which leaves the work pending, never a stage begun late.
    paths = [w["path"] for r in chain for w in live_workspaces(r) if w.get("path")]
    if _past(deadline):
        raise Deadline("the budget ran out before %s's processes were stopped" % rec["name"])
    stopped = stop_processes(paths, deadline=deadline)
    if stopped.get("survivors"):
        if _past(deadline):
            raise Deadline("the budget ran out while %s's processes were still stopping" % rec["name"])
        raise SpecError("cannot land %s: workspace processes are still running: %s" %
                        (rec["name"], stopped["survivors"]))
    if stopped.get("unknown"):
        if _past(deadline):
            raise Deadline("the budget ran out before %s's processes could be listed" % rec["name"])
        raise SpecError("cannot land %s: %s" % (rec["name"], stopped["unknown"]))
    if _past(deadline):
        raise Deadline("the budget ran out before %s's containers were stopped" % rec["name"])
    finished, containers = _within(deadline, stop_containers, paths)
    if not finished:
        raise Deadline("the budget ran out while %s's containers were being stopped" % rec["name"])
    if containers.get("failed"):
        raise SpecError("cannot land %s: workspace containers could not be stopped" % rec["name"])
    kept = ""
    if keep and not ignored_ok and not _not_in_integration(rec, chain, deadline=deadline):
        kept = _keep_ignored(rec, chain, deadline)
    _require_landed(rec, chain, ignored_ok, deadline)
    for r in chain:
        with Lock():
            fresh = load_agent(r["key"])
            fresh["disposition"] = {"kind": "landed", "at": now(), "auto": bool(auto), "by_session": me,
                                    "ignored_not_needed": ignored_ok or None, "as_part_of": rec["key"],
                                    **({"kept_ignored": kept} if kept else {})}
            save_agent(fresh)
        event("landed", key=r["key"], auto=bool(auto), as_part_of=rec["key"])
    _release_carried_waits(chain, deadline)
    clean = _delete_chain(chain, "landed", processes=stopped, deadline=deadline)
    if not clean:
        if not (load_agent(rec["key"]) or {}).get("disposition"):
            raise SpecError("landing eligibility changed during cleanup; the work was preserved")
    sweep_scratch_after_land(sweep_scopes, deadline=deadline)
    return {"landed": True, **({"cleanup_pending": True} if not clean else {}),
            **({"kept_ignored": kept} if kept else {})}


def _release_carried_waits(chain, deadline=None):
    """A WAIT ON AN AGENT ENDS WHEN THAT AGENT LANDS (2026-10-11).

    zach-opus-pierce3 had workspaces in two repositories and a wait "carried
    by zach-opus-pierce4, lands with it". pierce4 carried only the first
    repository's branch and landed; the wait outlived it, and the second
    repository's branch sat unlanded for ~40 h behind that note on every gate.

    So when `chain` lands, every other agent whose recorded wait names one of
    its agents, or that one of them carried (lands-pending / continues), loses
    that wait while any of its own work is not in its integration branch. The
    gate then lists it as unlanded again and its automatic land names the
    branch still out. Work that could not be checked loses the wait too:
    showing it is the safe answer, and nothing is deleted here."""
    keys = {r["key"] for r in chain}
    names = [r["name"] for r in chain if r.get("name")]
    carried = {c for r in chain for c in (r.get("lands_pending") or []) + (r.get("continues") or [])}
    for other in all_agents():
        waiting = other.get("waiting")
        if not waiting or other["key"] in keys or other.get("disposition"):
            continue
        on = waiting.get("on") or ""
        if not (other.get("name") in carried or other["key"] in carried
                or any(re.search(r"(?<![\w-])%s(?![\w-])" % re.escape(n), on) for n in names)):
            continue
        try:
            out = _not_in_integration(other, _chain(other), deadline=deadline)
        except (SpecError, OSError) as e:
            out = ["it could not be checked (%s)" % e]
        if not out:
            continue
        with Lock():
            fresh = load_agent(other["key"])
            if not fresh or fresh.get("disposition") or fresh.get("waiting") != waiting:
                continue
            fresh.setdefault("history", []).append({"at": iso(), "fact": "wait ended: %s landed without "
                                                    "all of this work" % ", ".join(names),
                                                    "wait": waiting, "not_landed": out})
            fresh["waiting"] = None
            save_agent(fresh)
        event("wait-ended", key=other["key"], landed=names, not_landed=out)


def merge_and_land(ref, me="", message=""):
    """`workspaces.sh merge <name>`: THE MERGE AND THE LAND ARE ONE COMMAND
    (2026-10-04). The lead merged with `git merge --no-ff` and then had to
    remember a separate `workspaces.sh land`; for four teammates he did not,
    and their workspaces stayed 1-2 days. This merges every branch of a
    FINISHED teammate that is not yet in its integration branch, in that
    repository's main checkout (which must be on that branch), with git's own
    hooks and checks, then lands it exactly as the automatic land does
    (ignored files are kept, never a reason to stay). A merge git refuses
    stops here with git's words; nothing is landed. In a repository listed in
    SECOND_REVIEW_REPOS nothing is merged unless the newest handover verdict
    on exactly each branch tip says passed (_review_check, CEO §113), and the
    merge message names that review. Returns (merged, land result)."""
    rec = _resolve(ref, me)
    fin, _pz, why = finished_state(rec)
    if not fin:
        raise SpecError("%s is not finished (%s); merge it when its run has ended" % (rec["name"], why))
    chain = _chain(rec)
    merged = []
    todo = []
    bases = {}                  # {tip: the integration branch's tip it merges into}
    for repo, b in _branch_targets(chain):
        main = main_checkout(repo)
        target, tip, why_not = integration_target(chain, repo)
        if why_not or not main:
            raise SpecError("cannot merge %s of %s: %s" % (b, repo, why_not or "its main checkout cannot be read"))
        t, unread = branch_tip_read(main, b)
        if unread:
            raise SpecError("cannot merge %s of %s: %s" % (b, repo, unread))
        if not t or is_ancestor(main, t, tip):
            continue
        todo.append((repo, b, main, t))
        bases[t] = tip
    # A branch whose tip another branch of this work already contains comes in
    # with that one, so it is neither merged nor reviewed on its own.
    todo = [x for x in todo
            if not any(y[2] == x[2] and y[3] != x[3] and is_ancestor(x[2], x[3], y[3]) for y in todo)]
    reviews = _review_check(todo, _governing_entities(chain), bases)
    for main in sorted({x[2] for x in todo}):
        _bring_fence_current(main)
    taken = []                  # [(main, holder args)]: the leases THIS command took, released at its end
    try:
        for repo, b, main, t in todo:
            target, tip, why_not = integration_target(chain, repo)
            if why_not:
                raise SpecError("cannot merge %s of %s: %s" % (b, repo, why_not))
            if is_ancestor(main, t, tip):
                continue
            in_main = _merges_in_main_checkout(main, target, b)
            # Git merges the commit read above, never the branch name: a writer
            # that advances the branch after that read (even while an earlier
            # repository's merge gate runs) would otherwise land a newer commit
            # no review saw (review rv-20261009T025426Z-15dae5ca-3cc1, finding 2).
            # A commit merged by its ID gets Git's default message for its branch.
            msg = ["-m", message or "Merge branch '%s'%s" % (b, "" if target in ("main", "master")
                                                             else " into %s" % target)]
            if reviews.get(t):
                # The merge commit names the review that let it in (plan §2.4).
                msg += ["-m", reviews[t]]
            args = ["merge", "--no-ff", "--no-edit"] + msg + [t]
            if main not in [m for m, _h in taken]:
                holder = _take_land_lease(main)
                if holder is not None:
                    taken.append((main, holder))
            # The lease this land runs under names this command, so the holder's
            # turn end keeps it until the command ends (2026-10-11: two merges in
            # a background script were refused after the lead's turn end released
            # the lease between their steps; operator_fences.land_process).
            _fence_program().register_land(main)
            # The author's own saved runs of exactly this tip, offered to the gate for validation.
            offered = _author_runs(chain, t)
            if in_main:
                before = git(main, "rev-parse", "HEAD")[1].strip()
                # git's own checks (pre-merge-commit) run here and may take minutes.
                r = subprocess.run(["git", "-C", main] + args, capture_output=True, text=True,
                                   env=_land_env(target, offered))
                if r.returncode != 0:
                    raise SpecError("git merge of %s into %s of %s failed (exit %d); nothing was landed:\n%s\n%s"
                                    % (b, target, main, r.returncode, (r.stdout + r.stderr).strip()[-3000:],
                                       _abort_own_merge(main, before, t)))
            else:
                _merge_in_temporary_checkout(main, target, args, b, offered)
            merged.append((repo, b))
            event("merged", key=rec["key"], repo=repo, branch=b, into=target)
        return merged, land(rec["key"], me, keep_ignored=True)
    finally:
        for main, holder in taken:
            _release_land_lease(main, holder)


def _merges_in_main_checkout(main, target, b):
    """True: the merge into `target` runs in the main checkout, which has it
    checked out. False: it runs in a temporary checkout of `target`
    (_merge_in_temporary_checkout), because nothing has `target` checked out.

    A RELEASE BRANCH IS NEVER CHECKED OUT IN THE MAIN CHECKOUT (2026-10-14):
    the main checkout stays on main while teammates land on v1.3.0, and this
    refused every such merge, so each was done by hand. main (or master) itself
    is still merged only in the main checkout, as before; a target another
    worktree has checked out is refused, because that tree is somebody's."""
    wl = worktree_list(main)
    if wl is None:
        raise SpecError("cannot merge %s: the worktrees of %s cannot be read" % (b, main))
    if wl and wl[0]["branch"] == target:
        return True
    other = [w["path"] for w in wl[1:] if w["branch"] == target]
    if other:
        raise SpecError("cannot merge %s into %s: %s has %s checked out, and a merge never runs in somebody "
                        "else's tree. Free it there, or check %s out in the main checkout %s."
                        % (b, target, other[0], target, target, main))
    if target in ("main", "master"):
        head = wl[0]["branch"] if wl else ""
        raise SpecError("cannot merge %s: the main checkout %s is on %s, not on %s, the branch this work "
                        "integrates on" % (b, main, head or "a detached HEAD", target))
    return False


def _merge_in_temporary_checkout(main, target, args, b, author_runs=()):
    """Runs `git merge` (with git's own hooks and checks) in a temporary
    checkout of `target` under this tool's state directory, then removes that
    checkout. A refused merge commits nothing, so `target` has not moved; the
    checkout is removed either way. `author_runs` are offered to the gate
    (_land_env)."""
    _ensure_dirs()
    parent = _p("merge-checkouts")
    os.makedirs(parent, exist_ok=True)
    path = os.path.join(os.path.realpath(parent), "%s-%d" % (_key_segment(target), os.getpid()))
    _remove_temporary_checkout(main, path)
    rc, out, err = git(main, "worktree", "add", "--quiet", path, target, timeout=600)
    if rc != 0:
        _remove_temporary_checkout(main, path)
        raise SpecError("cannot merge %s: a temporary checkout of %s could not be made (exit %d); nothing was "
                        "landed:\n%s" % (b, target, rc, (out + err).strip()[-2000:]))
    try:
        # git's own checks (pre-merge-commit) run here and may take minutes.
        r = subprocess.run(["git", "-C", path] + args, capture_output=True, text=True,
                           env=_land_env(target, author_runs))
    finally:
        left = _remove_temporary_checkout(main, path)
    if r.returncode != 0:
        raise SpecError("git merge of %s into %s of %s failed (exit %d); nothing was landed and %s did not move:"
                        "\n%s%s" % (b, target, main, r.returncode, target, (r.stdout + r.stderr).strip()[-3000:],
                                    "\n" + left if left else ""))
    if left:
        sys.stderr.write(left + "\n")


def _remove_temporary_checkout(main, path):
    """"" once `path` is gone and git no longer lists it, else what is left.
    Only this path is removed: no `git worktree prune`, which would touch
    every other worktree's record too."""
    if os.path.lexists(path):
        git(main, "worktree", "remove", "--force", path, timeout=300)
    listed = path in [w["path"] for w in worktree_list(main) or []]
    if os.path.lexists(path) or listed:
        return ("the temporary checkout %s could not be removed; git worktree remove --force %s removes it"
                % (path, path))
    return ""


def _lease_holder_args(F, conf):
    """`--holder <kind> --holder-pid <pid>` for a declared non-Claude holder
    (Codex) behind this command, else []: acquire then finds the Claude session
    itself, or refuses (review rv-20261011T205852Z-54ae8c6a-eec1)."""
    try:
        who = F.caller_identity(conf)
    except Exception:           # noqa: BLE001  acquire then identifies the caller itself
        who = None
    if who and who.get("kind") and who.get("kind") != "claude":
        return ["--holder", str(who["kind"]), "--holder-pid", str(who["pid"])]
    return []


def _take_land_lease(main):
    """THE MERGE TAKES THE LAND LEASE ITSELF (2026-10-14). `workspaces.sh merge
    echo-opus-hiresnag55b` was refused by the operator fence ("this process
    does not hold the land lease. The land lease for this repository is
    free."), and the lead then ran `land-lease.sh acquire` by hand before every
    merge. Called before the first write into `main`:

      * the fence is off or not installed: nothing to take, returns None;
      * this command's caller already holds the lease: it stays the caller's
        (it may be holding it for a push or a deploy), returns None;
      * otherwise the lease is taken through `land-lease.sh acquire`, with
        every rule of an acquire. A lease another holder has is waited for as
        acquire waits (LAND_LEASE_WAIT, default 90 s), and that is said first.
        Returns the holder arguments, so the caller releases it when done.

    Still not held after that: SpecError with acquire's own words, and nothing
    is merged."""
    F = _fence_program()
    # THIS PROCESS COUNTS AS ITS OWN CALLER: a lease held by the process that
    # runs this merge in-process (the app's library call, a test that is the
    # session) is held, and git's fence hooks, its descendants, agree. Without
    # it, acquire RENEWED that lease and this still said "could not take it".
    me = F.proc(os.getpid())
    chain = ([(me["pid"], me["start"])] if me else []) + F.ancestors()
    try:
        conf, _paths = F.launcher_for(main)
        if not F.fenced(conf):
            return None
        files = F.Files(conf)
        held, lease = F.authorized(files, chain)
    except Exception as exc:    # noqa: BLE001  a check that cannot decide refuses, and says so
        raise SpecError("cannot merge into %s: whether this command holds the land lease could not be read (%s)"
                        % (main, exc))
    if held:
        return None
    lease_cmd = os.path.join(engine_root(), "scripts", "land-lease.sh")
    holder = _lease_holder_args(F, conf)
    if F.lease_state(lease) in ("live", "expired"):
        sys.stderr.write("merge: waiting for the land lease of %s: %s holds it. Waiting as land-lease.sh "
                         "acquire waits.\n" % (main, F.holder_label(lease.get("holder"))))
    r = subprocess.run(["bash", lease_cmd, "acquire", "--repo", main] + holder, capture_output=True, text=True)
    said = (r.stdout + r.stderr).strip()
    if said:
        sys.stderr.write(said + "\n")
    if r.returncode == 0 and F.authorized(F.Files(conf), chain)[0]:
        return holder
    raise SpecError("nothing was merged into %s: this command could not take the land lease (exit %d). %s\n"
                    "Run the same command again once it is free."
                    % (main, r.returncode, said[-800:] or "land-lease.sh acquire said nothing"))


def _release_land_lease(main, holder):
    """Releases a lease `_take_land_lease` took. A release that fails is said,
    never raised: the land has already happened or been refused by then, and the
    holder's turn end releases a lease left behind."""
    lease_cmd = os.path.join(engine_root(), "scripts", "land-lease.sh")
    r = subprocess.run(["bash", lease_cmd, "release", "--repo", main] + list(holder),
                       capture_output=True, text=True)
    said = (r.stdout + r.stderr).strip()
    if said:
        sys.stderr.write(said + "\n")
    if r.returncode != 0:
        sys.stderr.write("merge: the land lease of %s this command took could not be released (exit %d); "
                         "%s release --repo %s releases it.\n" % (main, r.returncode, lease_cmd, main))


def _fence_program():
    """The engine's operator_fences.py, where the second review's land rule
    lives (the fence applies the same rule to every other move of main)."""
    import importlib.util
    path = os.path.join(engine_root(), "scripts", "lib", "operator_fences.py")
    spec = importlib.util.spec_from_file_location("workspaces_operator_fences", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _bring_fence_current(main):
    """Before anything is merged into `main`, its installed operator fence (the
    copy git's reference-transaction hook runs) is made the engine's
    (operator_fences_admin.bring_current), so a land is never refused by an
    out-of-date copy after its whole land check has run (2026-10-19). A copy
    that cannot be written refuses here, before that check, naming the fix."""
    here = os.path.join(engine_root(), "scripts", "lib")
    if here not in sys.path:
        sys.path.insert(0, here)
    import operator_fences_admin
    try:
        replaced = operator_fences_admin.bring_current(main)
    except OSError as exc:
        raise SpecError("nothing was merged into %s: its installed operator fence differs from the engine's and "
                        "could not be brought current (%s). Run %s install --repo %s, then the same command again."
                        % (main, exc, os.path.join(engine_root(), "scripts", "operator-fences.sh"), main))
    if replaced:
        sys.stderr.write("merge: the installed operator fence of %s was older than the engine's; brought current "
                         "(%s).\n" % (main, ", ".join(replaced)))


def _review_check(todo, entities=(), bases=None):
    """NO WORK LANDS WITHOUT A PASSING REVIEW OF EXACTLY ITS TIP (CEO §113;
    richos-hq plan 2026-10-09 §2.5 and §4 row 3). For every branch about to be
    merged into a reviewed repository (_review_ledgers: one the SECOND_REVIEW_REPOS
    of a governing declaration lists, or whose fence launcher carries a review
    ledger), the newest handover verdict on exactly its tip must say passed, in
    every ledger that applies. Otherwise NOTHING is merged, and the refusal
    names each tip and its findings; a listed repository whose review setup
    cannot be established is refused as well. Asked here before git's merge
    gate spends its minutes, with the fence on, off or not installed; the fence
    asks the same question of a plain `git merge`. `entities`: the governing
    entities (_governing_entities). `bases`: {tip: the integration branch's
    tip}; a branch whose changes over it are all release notes is asked
    nothing (F.notes_only), against the main checkout's HEAD when absent.
    Returns {tip: the merge message's review paragraph}."""
    if not todo:
        return {}
    F = _fence_program()
    registry = _registry_entity_map(F)
    out, refusals = {}, []
    for _repo, b, main, t in todo:
        if F.notes_only(main, (bases or {}).get(t) or "HEAD", t):
            continue                     # release notes only: no review is asked (F.RELEASE_NOTES)
        listed, unknown, recorded = _declared_review_listing(F, entities, main, registry)
        if unknown:
            refusals.append("=== SECOND REVIEW: %s was not merged into %s ===\n  Whether %s is reviewed cannot "
                            "be established: %s." % (b, main, main, "; ".join(unknown)))
            continue
        ledgers, broken = _review_ledgers(F, main, listed, recorded)
        if broken:
            refusals.append("=== SECOND REVIEW: %s was not merged into %s ===\n  %s" % (b, main, broken))
            continue
        gaps = next((g for g in (F.review_gaps(ledger, [t], main) for ledger in ledgers) if g), None)
        if gaps:
            for ledger in ledgers:
                try:
                    F.route_refused_review(ledger, gaps, main)
                except (OSError, ValueError):
                    pass
            refusals.append(F.review_refusal_text(main, gaps, engine_root()).replace(
                "=== SECOND REVIEW: refused in %s ===" % main,
                "=== SECOND REVIEW: %s was not merged into %s ===" % (b, main), 1))
            continue
        if not ledgers:
            # No install recorded a review, every governing declaration was read
            # and lists nothing, and no launcher carries a ledger: positive
            # evidence, never a fallback.
            continue
        row, _mid = F.review_of(F.read_reviews(ledgers[0]), t, main, F.read_deferrals(ledgers[0]))
        para = "Second review: %s, %s by %s (%s), %s finding(s)." % (
            row.get("verdict"), row.get("id"), row.get("reviewer"), row.get("reviewer_model"), row.get("findings"))
        notes = ["P%s %s (%s)" % f for f in F.review_findings(row, limit=20)]
        notes += ["Filed as a follow-up: %s %s. Why: %s" % (fid, d.get("title") or "", d.get("why") or "")
                  for fid, d in sorted((row.get("deferred") or {}).items())]
        out[t] = para + ("\n" + "\n".join(notes) if notes else "")
    if refusals:
        raise SpecError("nothing was merged.\n" + "\n".join(refusals))
    return out


def _governing_entities(chain):
    """The entities whose orchestration.config governs this work: each record's
    own (written when it was registered), and this run's resolved entity."""
    out = []
    for e in [r.get("entity") or "" for r in chain] + [os.environ.get("RICHOS_ENTITY_ROOT_RESOLVED", "")]:
        e = os.path.realpath(e) if e.strip() else ""
        if e and e not in out:
            out.append(e)
    return out


def _registry_key(F, main):
    """(the fence registry's key for the main checkout `main`, why unknown or
    ""): the key is the path operator_fences_admin.cmd_install records
    (F.repo_paths()["main"]). A repository Git cannot read has no key that can
    be trusted, so the registry entry governing it cannot be found: unknown,
    never "no entry" (review rv-20261009T042243Z-42ecc969-5f85)."""
    paths = F.repo_paths(main)
    if not paths:
        return os.path.realpath(main), ("the repository %s cannot be read, so the fence registry entry that "
                                        "governs it cannot be found" % os.path.realpath(main))
    return paths.get("main") or os.path.realpath(main), ""


def _registry_entity_map(F):
    """({main checkout: entity, or "" when unknown}, [why unknown], {main
    checkout whose install recorded that it requires review}) from the
    fence registry `operator-fences.sh install` writes
    (operator_fences_admin.registry_path(): REGISTRY_NAME in land_locks_dir()),
    read through F.registry_entities (an entry with no recorded entity is
    unknown, never inferred). THE
    ENTITY THAT GOVERNS A REPOSITORY'S FENCE IS READ FROM HERE, NEVER ONLY
    FROM THE LAUNCHER (review rv-20261009T033652Z-f3bfe22f-9372, finding 2):
    it can differ from the spawning entity, and with the launcher removed
    nothing else names it. IT IS KEPT PER REPOSITORY (review
    rv-20261009T035450Z-882c7e75-0eab, findings 1 and 2): an entry's entity
    governs that repository and no other, and an entry whose entity cannot be
    established is unknown, never skipped. An absent registry (nothing ever
    installed) names nothing; one that exists but cannot be read or parsed,
    or holds no `repositories` table (the shape install always writes; such a
    file was read as naming nothing, review rv-20261009T042243Z-42ecc969-5f85),
    leaves every repository's governing entity unknown. So does a registry
    that is a link to nothing: something was installed there (review
    rv-20261009T044823Z-fc8c7569-5919, finding 2).

    THE INSTALL IS THE ONE DURABLE RECORD OF WHETHER A REPOSITORY IS REVIEWED
    (F.registry_reviewed): what its declaration said at `operator-fences.sh
    install`, so removing or breaking a declaration afterwards never switches
    the review off; only the next install does."""
    path = os.path.join(land_locks_dir(), "operator-fences.json")
    try:
        with open(path, encoding="utf-8") as f:
            reg = json.loads(f.read())
    except FileNotFoundError:
        if os.path.lexists(path):
            return {}, ["the fence registry %s is a link to nothing" % path], set()
        return {}, [], set()
    except (OSError, ValueError) as exc:
        return {}, ["the fence registry %s cannot be read (%s)" % (path, getattr(exc, "strerror", None) or exc)], set()
    if not isinstance(reg, dict) or not isinstance(reg.get("repositories"), dict):
        return {}, ["the fence registry %s cannot be read (not the registry's shape)" % path], set()
    return F.registry_entities(reg), [], F.registry_reviewed(reg)


def _review_declaration(F, entity, recorded, key):
    """(the SECOND_REVIEW_REPOS names or None, why unknown or "") of the
    declaration of one governing `entity` of the main checkout `key`.
    `recorded`: the fence registry records `entity` as the one `key`'s fence
    was installed from.

    "NOT LISTED" NEEDS POSITIVE EVIDENCE (review
    rv-20261009T042243Z-42ecc969-5f85, the fourth review in a row to find the
    answer falling back to "not reviewed" with the launcher gone). There are
    exactly two kinds: the declaration was read and does not list it, or the
    entity is a spawning one (a record's, or this run's) whose directory exists
    with no orchestration.config in it, an entity that never had a declaration
    (review rv-20261009T033652Z-f3bfe22f-9372). Everything else is unknown:
      * a recorded entity whose declaration is missing, or whose directory is
        gone or is not a directory (a retired worktree): install read that
        declaration, so its absence now says nothing (the review's finding 1);
      * a spawning entity whose directory is gone or is not a directory;
      * a declaration that is a link to nothing (something was there);
      * a declaration that exists and cannot be read;
      * a declaration F.review_repos, its only reader, cannot read: bash
        does not source it cleanly, or the value bash sets is not names
        (reviews rv-20261009T050654Z-e21238ff-7685 and
        rv-20261009T052655Z-daf09e19-0bc6): install refuses it as well."""
    config = os.path.join(entity, "orchestration.config")
    try:
        with open(config, encoding="utf-8", errors="replace") as f:
            f.read()
    except OSError as exc:
        if exc.errno not in (errno.ENOENT, errno.ENOTDIR):
            return None, "the governing declaration %s cannot be read (%s)" % (config, exc.strerror or exc)
        here = os.path.isdir(entity)
        if recorded:
            return None, ("the fence registry records that %s's fence was installed from %s, but its declaration "
                          "%s %s; restore it, or record the entity that governs %s now with operator-fences.sh "
                          "install --repo %s --entity <that entity>"
                          % (key, entity, config, "is missing" if here else "is gone with its directory", key, key))
        if not here:
            return None, ("the entity %s that governs this work is gone (%s), so whether its declaration lists %s "
                          "cannot be established; restore it (as an empty directory if it never had an "
                          "orchestration.config)" % (entity, "not a directory" if os.path.lexists(entity)
                                                     else "nothing is there", key))
        if os.path.lexists(config):
            return None, "the governing declaration %s is a link to nothing" % config
        # A spawning entity that never had a declaration. One whose declaration
        # was deleted reads the same, and nothing records which it was: for an
        # installed repository the install's record (F.registry_reviewed) keeps
        # the review on; a repository never installed has no fence for a plain
        # `git merge` either, so the land asks no more of it than that (review
        # rv-20261009T044823Z-fc8c7569-5919, finding 1).
        return None, ""
    names, why = F.review_repos(config)
    if why:
        return None, ("SECOND_REVIEW_REPOS in %s cannot be read (%s); it must source cleanly in bash and set "
                      "SECOND_REVIEW_REPOS=\"<names>\"" % (config, why))
    return names, ""


def _declared_review_listing(F, entities, main, registry):
    """([SECOND_REVIEW_REPOS names], [why unknown], the install recorded that
    `main` requires review) for every declaration
    governing the main checkout `main`: those of `entities`
    (_governing_entities: the work's own and this run's) and the entity the
    fence registry records for `main` (`registry`, _registry_entity_map), the
    value bash sets in each, as F.review_repos (the one reader) reads it.
    WHETHER A REPOSITORY IS REVIEWED IS DECIDED FROM THESE (review
    rv-20261009T031207Z-ba444a8a-607a, finding 1): the fence launcher is only
    the copy `operator-fences.sh install` makes of the declaration, and reading
    the launcher alone let a missing launcher switch the review off.
    ABSENT IS NOT UNREADABLE (review rv-20261009T033652Z-f3bfe22f-9372, finding
    1): an entity with no orchestration.config lists nothing, but one whose
    orchestration.config exists and cannot be read leaves applicability
    unknown, and _review_check refuses rather than read it as an empty list.
    ONLY WHAT GOVERNS `main` IS READ (review rv-20261009T035450Z-882c7e75-0eab,
    finding 2): an entity the registry records for another repository is
    never read here, so its unreadable declaration cannot block this land; and
    a registry entry for `main` whose entity is unknown (finding 1) refuses.
    "NOT REVIEWED" NEEDS POSITIVE EVIDENCE FOR EVERY GOVERNING ENTITY (review
    rv-20261009T042243Z-42ecc969-5f85, the fourth in a row to find this answer
    falling back to "not reviewed" with the launcher gone): each entity's
    declaration is read by _review_declaration, which accepts only a
    declaration read that does not list it, or a spawning entity that never
    had one; an entity the registry records whose declaration is gone, and
    every other way the answer could not be read, is unknown. So is work that
    no declaration is known to govern at all: nothing was read.
    THE INSTALL'S RECORD COMES FIRST (review rv-20261009T044823Z-fc8c7569-5919):
    a repository whose install recorded that it requires review requires it
    whatever these declarations say now, so a declaration removed or broken
    after the install never switches the review off."""
    mapping, unknown, reviewed = registry
    unknown = list(unknown)
    key, unread = _registry_key(F, main)
    if unread:
        unknown.append(unread)
    governing = {}                              # entity -> the registry records it for `main`
    for e in entities:
        governing.setdefault(os.path.realpath(os.path.expanduser(e)), False)
    if key in mapping:
        if mapping[key]:
            governing[os.path.realpath(os.path.expanduser(mapping[key]))] = True
        else:
            unknown.append("the fence registry records a fence for %s, but the entity its fence was installed "
                           "from is not recorded there and cannot be established; run operator-fences.sh "
                           "install --repo %s --entity <the entity whose orchestration.config governs it>"
                           % (key, key))
    if not governing and not unknown:
        unknown.append("no declaration is known to govern this work: its records name no entity, this run "
                       "resolved none (RICHOS_ENTITY_ROOT_RESOLVED) and the fence registry records none for %s"
                       % key)
    values = []
    for e, recorded in governing.items():
        value, why = _review_declaration(F, e, recorded, key)
        if why:
            unknown.append(why)
        elif value is not None:
            values.append(value)
    return values, unknown, key in reviewed


def _review_ledgers(F, main, listed, recorded=False):
    """([review ledger], why refused) for the main checkout `main`.

    Recorded by its install as requiring review (`recorded`), or listed in a
    governing declaration (`listed`; both from _declared_review_listing):
    the ledger the declaration names, which is where second-review writes its
    verdicts (F.review_ledger_default(), the path `operator-fences.sh install`
    bakes into the launcher), plus the launcher's own when that differs. With
    no launcher at all it is still reviewed. A listed repository whose
    repository or existing launcher cannot be read has no established review
    setup and is refused with that reason. Not listed: the launcher's ledger
    when it carries one (the fence would ask it anyway), else ([], ""); a
    repository or existing launcher that cannot be read is refused here too,
    because whether that ledger applies is unknown, never "none" (review
    rv-20261009T042243Z-42ecc969-5f85). An existing launcher that is a link
    to nothing cannot be read either (review rv-20261009T044823Z-fc8c7569-5919,
    finding 2)."""
    declared = recorded or any(F.review_listed(v, main) for v in listed)
    ledgers = [F.review_ledger_default()] if declared else []
    rc, common, _e = git(main, "rev-parse", "--path-format=absolute", "--git-common-dir")
    if rc != 0 or not common.strip():
        if declared:
            return [], ("%s is listed in SECOND_REVIEW_REPOS but its repository cannot be read, so its "
                        "review setup cannot be established." % main)
        return [], ("the repository %s cannot be read, so whether its fence launcher carries a review ledger "
                    "cannot be established." % main)
    path = os.path.join(common.strip(), "hooks", "reference-transaction")
    try:
        with open(path, encoding="utf-8", errors="replace") as f:
            text = f.read(65536)
    except FileNotFoundError:
        text = ""                               # no launcher: the declarations alone decide
        if os.path.lexists(path):
            return [], ("%s's fence launcher %s is a link to nothing, so its review setup cannot be "
                        "established." % (main, path))
    except OSError as exc:
        if declared:
            return [], ("%s is listed in SECOND_REVIEW_REPOS but its fence launcher %s cannot be read (%s), "
                        "so its review setup cannot be established." % (main, path, exc.strerror or exc))
        return [], ("%s's fence launcher %s cannot be read (%s), so whether the review ledger it carries "
                    "applies cannot be established." % (main, path, exc.strerror or exc))
    m = re.search(r'(?m)^OPERATOR_FENCES_REVIEWS="([^"]*)"\s*$', text) \
        if "richos-operator-fence-launcher" in text else None
    if m and m.group(1) and m.group(1) not in ledgers:
        ledgers.append(m.group(1))
    return ledgers, ""


def _abort_own_merge(main, before, tip):
    """A REFUSED MERGE LEAVES NO MERGE BEHIND (2026-10-07). When git's own
    pre-merge-commit check (the merge gate) refuses, git stops with "Not
    committing merge" and leaves MERGE_HEAD set and the branch's files staged.
    Three of Rich's lands ended that way and his hand-typed `git merge --abort`
    then met the operator fence without the land lease, which refused it after
    git had already rewritten the index.

    The abort runs HERE, as a child of this command. Under the fence, the merge
    above wrote ORIG_HEAD in the main checkout, which the fence passes only for
    a process whose ancestor holds the land lease (operator-fences.test.sh F1,
    F6); the abort writes the same ref from the same ancestry, so the fence
    accepts it exactly as it accepted the merge. With the fence off nothing is
    fenced. Only the merge this command started is aborted: MERGE_HEAD must be
    the branch tip it merged and HEAD the commit it merged into. Returns one
    line saying what was done, for the refusal it is appended to."""
    rc, mh, _e = git(main, "rev-parse", "-q", "--verify", "MERGE_HEAD")
    if rc != 0:
        return "No merge was left in progress in %s." % main
    now_head = git(main, "rev-parse", "HEAD")[1].strip()
    if mh.strip() != tip or not before or now_head != before:
        return ("A merge is in progress in %s that is not the one this command started (MERGE_HEAD %s, HEAD %s); "
                "it was left alone." % (main, mh.strip()[:12], now_head[:12]))
    blocked = _lease_for_abort(main)
    if blocked:
        return ("The merge it started was NOT aborted and is still in progress in %s: %s"
                % (main, blocked))
    rc, out, err = git(main, "merge", "--abort", timeout=300)
    left = git(main, "rev-parse", "-q", "--verify", "MERGE_HEAD")[0] == 0
    if rc == 0 and not left:
        return "The merge it started was aborted: %s is back at %s with no merge in progress." % (main, before[:12])
    return ("The merge it started could NOT be aborted (exit %d) and is still in progress in %s:\n%s"
            % (rc, main, (out + err).strip()[-2000:]))


def _lease_for_abort(main):
    """An empty string when the abort may run: the fence is off or not installed, or this
    process holds the land lease, or the lease was free and this command has
    just taken it again. Otherwise the reason the abort must not run.

    2026-10-11 (echo-sonnet-quotaflake3): the lease was released while the
    merge's gate ran, the fence refused the move of main, and the abort below
    met the same fence without the lease; `git merge --abort` rewrote the index
    before the fence refused its ORIG_HEAD and left MERGE_HEAD behind. A lease
    that is FREE is retaken here, through land-lease.sh acquire and so with
    every rule of an acquire, by the same holder that started the merge. A
    lease another holder has is never taken, and then the abort is not run at
    all, because run without the lease it rewrites the tree and is refused."""
    try:
        F = _fence_program()
        conf, _paths = F.launcher_for(main)
        if not F.fenced(conf) or F.authorized(F.Files(conf))[0]:
            return ""
    except Exception as exc:    # noqa: BLE001  a check that cannot decide says so
        return "whether this command holds the land lease could not be read (%s)" % exc
    lease_cmd = os.path.join(engine_root(), "scripts", "land-lease.sh")
    # The retake is by the holder that started the merge: a declared non-Claude
    # holder (Codex) is named, or acquire finds no Claude session and refuses
    # (review rv-20261011T205852Z-54ae8c6a-eec1).
    holder = _lease_holder_args(F, conf)
    r = subprocess.run(["bash", lease_cmd, "acquire", "--repo", main, "--wait", "30"] + holder,
                       capture_output=True, text=True)
    said = (r.stdout + r.stderr).strip()
    if r.returncode == 0 and F.authorized(F.Files(conf))[0]:
        return ""
    return ("the land lease is not this command's and could not be taken (%s), and an abort without it "
            "rewrites the tree and is then refused by the fence. Once the lease is free: "
            "%s acquire --repo %s, then %s abort-orphan --repo %s."
            % (said[-600:] or "exit %d" % r.returncode, lease_cmd, main, lease_cmd, main))


def _require_landed(rec, chain, ignored_ok="", deadline=None, record=True):
    """Read current work after writers stop, including on a deletion retry.
    `record=False`: asked by a dry question, which writes nothing (V3-06)."""
    preserved = set()
    for r in chain:
        preserved.update(_require_clean(r, "land %s" % r["name"], ignored_ok, deadline, record) or [])
    missing = _not_in_integration(rec, chain, preserved, deadline)
    if missing:
        raise SpecError(_not_landed_message(rec, missing))


def _keep_ignored(rec, chain, deadline=None):
    """Move every ignored entry the land would refuse (one the main checkout
    lacks, not generated at creation, not declared regenerable) out of the
    work's workspaces into <state>/kept/<key>-<time>/<n>-<workspace>/<entry>,
    and return that directory ("" when nothing was kept). In-flight ack files
    are deleted instead, never kept. Called only once every branch and HEAD is
    proved to be in the integration branch.

    Nothing moves while any workspace has uncommitted work or an entry that
    could not be read: _require_clean then refuses, naming them, exactly as
    before. Partial-cleanup residue is left to _require_clean as well."""
    if any(r.get("landing_cleanup_started") or r.get("deletion") for r in chain):
        return ""
    plan = []
    for r in chain:
        for n, w in enumerate(live_workspaces(r)):
            p = w.get("path")
            if not p or not os.path.isdir(p):
                continue
            dirty, ignored = uncommitted(p, deadline)
            ignored = _minus_regenerable(r, w, _minus_generated(w, ignored, deadline))
            if dirty or any(" (unreadable" in rel for rel in ignored):
                return ""
            label = "%d-%s" % (n, _key_segment(os.path.basename(p.rstrip("/"))))
            plan += [(r["key"], label, p, rel.rstrip("/")) for rel in ignored]
    # IN-FLIGHT ACK FILES ARE NOT KEPT AT ALL (2026-10-04). They are the
    # agent's receipts that it saw main move, and their durable copy is the
    # ack ledger, so once the work is proved landed they are deleted, never
    # moved. Everything else kept goes to <state>/kept/, which the scratch
    # reaper empties after SCRATCH_KEPT_RETENTION_DAYS (§54).
    acks = [x for x in plan if _is_ack(x[3])]
    plan = [x for x in plan if not _is_ack(x[3])]
    if not plan and not acks:
        return ""
    # A MOVE, NEVER A COPY: inside a gate's budget only a rename is bounded.
    # A workspace on another volume than the registry keeps the old answer
    # (the land waits, and pending() now says why).
    _ensure_dirs()
    try:
        here = os.stat(state_dir()).st_dev
        if any(os.stat(p).st_dev != here for _k, _l, p, _r in plan):
            return ""
    except OSError:
        return ""
    root = _p("kept", "%s-%d" % (_key_segment(rec["key"]), int(now())))
    moved, dropped = [], []
    # An entry that holds the ack directory (a `.claude/` the main checkout
    # lacks) loses the acks first, so they are not kept inside it.
    holders = [(p, _ACK_DIR) for _k, _l, p, rel in plan if _ACK_DIR.startswith(rel.rstrip("/") + "/")]
    for p, rel in [(p, rel) for _k, _l, p, rel in acks] + holders:
        src = os.path.join(p, rel)
        if not os.path.lexists(src):
            continue
        try:
            if os.path.isdir(src) and not os.path.islink(src):
                shutil.rmtree(src)
            else:
                os.unlink(src)
        except OSError as e:
            raise SpecError("cannot land %s: the in-flight ack %s could not be deleted (%s)"
                            % (rec["name"], src, e))
        dropped.append(src)
    for key, label, p, rel in plan:
        where = label if key == rec["key"] else _key_segment(key) + "-" + label
        src, dst = os.path.join(p, rel), os.path.join(root, where, rel)
        if not os.path.lexists(src):
            continue
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        try:
            shutil.move(src, dst)
        except (OSError, shutil.Error) as e:
            event("kept-ignored", key=rec["key"], kept=root, entries=moved[:50], failed=rel, why=str(e)[:200])
            raise SpecError("cannot land %s: the ignored entry %s could not be kept in %s (%s); "
                            "what was moved is there" % (rec["name"], os.path.join(p, rel), root, e))
        moved.append(os.path.join(p, rel))
    event("kept-ignored", key=rec["key"], kept=root if moved else "", entries=moved[:50], count=len(moved),
          acks_deleted=len(dropped))
    return root if moved else ""


# Where scripts/inflight-ack.sh writes an agent's ack files, relative to its
# workspace: <workspace>/.claude/inflight-acks/<first-12-of-sha>.<name>.ack
_ACK_DIR = ".claude/inflight-acks"


def _is_ack(rel):
    rel = rel.rstrip("/")
    return rel == _ACK_DIR or rel.startswith(_ACK_DIR + "/")


def _not_landed_message(rec, missing):
    return ("%s is not landed yet: %s. Merge it onto the branch this work integrates on, then land it; or "
            "discard it (point 7)." % (rec["name"], "; ".join(missing)))


def _not_in_integration(rec, chain, preserved=(), deadline=None, definite_only=False):
    """What of this work is not shown to be in the branch it integrates on.

    `definite_only` keeps only what was READ and found outside it (a HEAD or a
    branch tip that is not an ancestor of the integration tip) and drops every
    unknown: a read that failed, a target that is not recorded. That is the
    cheap question land() asks before it stops anything (finding 13); the full
    question, after shutdown, still counts every unknown as not landed."""
    missing = []
    definite = []
    targets = {}

    def _target(repo):
        """The RECORDED integration ref of this repository (point 14) — resolved
        once per land, never from a HEAD read at land time."""
        if repo not in targets:
            targets[repo] = integration_target(chain, repo)
        return targets[repo]

    for r in chain:
        for w in r.get("workspaces") or []:
            repo = w.get("repo")
            branch, tip, why_not = _target(repo)
            if why_not:
                missing.append("%s: %s" % (w.get("path") or w.get("branch"), why_not))
                continue
            if not w.get("deleted_at") and w.get("path") and os.path.isdir(w["path"]):
                # Every git read, not only the branch loop, looks at the
                # clock (V2-03).
                if _past(deadline):
                    raise Deadline("the budget ran out before %s's workspaces could be proved to be in "
                                   "the branch this work integrates on" % rec["name"])
                rc, out, err = git(w["path"], "rev-parse", "HEAD", timeout=_bounded(deadline, 60))
                # A HEAD that could not be READ is not a HEAD that is in the
                # integration branch: nothing was proved, so the land waits
                # (hunt part 4, finding 4). The one directory that has no HEAD
                # to read and still proves its work landed is partial-cleanup
                # residue whose Git metadata is already gone and whose every
                # file `_require_clean` just proved preserved in the
                # integration tip; its branch is still checked below.
                if rc != 0 and w["path"] in preserved:
                    pass
                elif rc != 0:
                    missing.append("HEAD of %s could not be read (git exit %d: %s), so it is not shown to be in "
                                   "%s of %s" % (w["path"], rc, err.strip()[:200], branch, repo))
                elif not is_ancestor(repo, out.strip(), tip):
                    missing.append("HEAD of %s (%s) is not in %s of %s at %s"
                                   % (w["path"], out.strip()[:12], branch, repo, tip[:12]))
                    definite.append(missing[-1])
    for repo, b in _branch_targets(chain):
        if _past(deadline):
            raise Deadline("the gate's budget ran out before %s's branches could be proved to be in the "
                           "branch this work integrates on" % rec["name"])
        branch, tip, why_not = _target(repo)
        if why_not:
            missing.append("branch %s: %s" % (b, why_not))
            continue
        # A branch that is GONE has nothing left to land (partial cleanup
        # deletes branches first); a branch whose tip could not be READ may
        # still carry the only copy of a commit, so it holds the land.
        t, unread = branch_tip_read(repo, b)
        if unread:
            missing.append("branch %s: %s" % (b, unread))
        elif t and not is_ancestor(repo, t, tip):
            missing.append("%s (%s) is not in %s of %s at %s" % (b, t[:12], branch, repo, tip[:12]))
            definite.append(missing[-1])
    return definite if definite_only else missing


def discard(ref, reason, ceo_word="", not_ceo_ordered="", me=""):
    """Point 7: work that must not go into main is discarded — every workspace
    and branch deleted, with the reason recorded. Work the CEO ordered is never
    discarded without his word."""
    if len((reason or "").strip()) < 10:
        raise SpecError("a discard records its reason (point 7): give one")
    rec = _resolve(ref, me)
    fin, _pz, why = finished_state(rec)
    if not fin and not never_ran(rec):
        raise SpecError("%s is not finished (%s). Stop it first; a running agent is not discarded."
                        % (rec["name"], why))
    refused = "" if fin else dispatch_refusal(rec)
    chain = _chain(rec)
    ordered = [r for r in chain if r.get("ceo_ordered")]
    if ordered and not (ceo_word or "").strip():
        raise SpecError("%s is work the CEO ordered (%s). It is never discarded without his word: ask him, "
                        "then pass --ceo-word '<his words>' (point 7)."
                        % (rec["name"], ordered[0]["ceo_ordered"]))
    if not ordered and not (ceo_word or "").strip() and len((not_ceo_ordered or "").strip()) < 10:
        raise SpecError("say whether the CEO ordered this work: --ceo-word '<his words>' or "
                        "--not-ceo-ordered '<why it was not his order>' (point 7)")
    tips = {}
    for repo, b in _branch_targets(chain):
        t = branch_tip(repo, b)
        if t:
            tips["%s:%s" % (repo, b)] = t
    for r in chain:
        with Lock():
            fresh = load_agent(r["key"])
            fresh["disposition"] = {"kind": "discarded", "at": now(), "reason": reason.strip(),
                                    "ceo_word": ceo_word or None, "not_ceo_ordered": not_ceo_ordered or None,
                                    "tips": tips, "by_session": me, "as_part_of": rec["key"]}
            if refused and r["key"] == rec["key"]:
                fresh["disposition"]["dispatch_refused"] = refused
            save_agent(fresh)
        event("discarded", key=r["key"], reason=reason, tips=tips, as_part_of=rec["key"])
    _delete_chain(chain, "discarded")
    return {"discarded": True, "tips": tips}


def never_ran(rec):
    """A registration whose spawn never happened: its workspace was created and
    registered, and its spawn was refused before it was registered
    (2026-10-14: Pierce refused echo-opus-tap22's spawn, and `discard` answered
    "is not finished (its run has not ended)" until it was stopped first).
    Exactly withdraw_cc's test, plus no start or end recorded.

    A spawn that WAS registered (tool_use_id, spawned_at) may have an agent
    running in the foreground, where the platform reports its id only at the
    end, so it never ran only when the platform itself says it refused that
    call (dispatch_refusal): the registration guard writes the spawn before
    the other Agent guards answer, and one of them can still refuse it (review
    rv-20261014T105153Z-f22eff3f-da5a)."""
    if any(rec.get(f) for f in ("agent_id", "started_at", "end", "provisional", "disposition")):
        return False
    if not (rec.get("tool_use_id") or rec.get("spawned_at")):
        return True
    return bool(dispatch_refusal(rec))


def dispatch_refusal(rec):
    """The platform's own answer to this registered Agent call when a guard
    refused it, else "". It is read from the lead's transcript (the payload's
    transcript_path, kept at registration): the tool result for this
    tool_use_id, an error that starts "PreToolUse:Agent hook error". An Agent
    call a hook refused never started an agent. No transcript, no result yet,
    or any other result is "": the agent may be running."""
    tuid = str(rec.get("tool_use_id") or "")
    path = str(rec.get("dispatch_transcript") or "")
    if not tuid or not path:
        return ""
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            for line in f:
                if tuid not in line or "tool_result" not in line:
                    continue
                try:
                    row = json.loads(line)
                except ValueError:
                    continue
                content = ((row.get("message") or {}).get("content")) if isinstance(row, dict) else None
                for c in content if isinstance(content, list) else []:
                    if not (isinstance(c, dict) and c.get("type") == "tool_result"
                            and c.get("tool_use_id") == tuid):
                        continue
                    body = c.get("content")
                    if isinstance(body, list):
                        body = "\n".join(str(b.get("text") or "") for b in body if isinstance(b, dict))
                    text = str(body or "")
                    if c.get("is_error") and text.startswith("PreToolUse:Agent hook error"):
                        return text.splitlines()[0][:500]
                    return ""
    except OSError:
        return ""
    return ""


def _delete_chain(chain, why, processes=None, deadline=None):
    allw = [(r, w) for r in chain for w in live_workspaces(r) if w.get("path")]
    # Past the deadline nothing is stopped here (V2-03): each _delete below is
    # handed no shutdown result, sees the deadline and records a deferral.
    stopped = processes if processes is not None else (
        None if _past(deadline) else stop_processes([w["path"] for _r, w in allw], deadline=deadline))
    complete = True
    for r in chain:
        if not _delete(r, [w for w in live_workspaces(r) if w.get("path")], branches=True, why=why,
                       processes=stopped, deadline=deadline):
            complete = False
            if why == "landed" and not (load_agent(r["key"]) or {}).get("disposition"):
                return False
    return complete


def _delete(rec, workspaces, branches, why, processes=None, deadline=None):
    """Points 9, 10, 13: stop every process, then delete every workspace (and
    branch) as one; whatever fails is retried automatically.

    `deadline` is the caller's budget (hunt part 4, finding 15). The second
    landing proof below used to run without it, so a gate or a retry with a
    stated budget could spend any amount of time comparing files. Running out
    is not a failure and not a change of eligibility: the deletion is simply
    deferred to the next retry, with nothing deleted and no attempt counted.

    THE BUDGET BOUNDS EVERY STAGE, NOT ONLY THE PROOF (hunt part 4 v2, V2-03).
    Process shutdown, container cleanup, test-instance collection, each
    workspace removal and each branch deletion used to run on their own
    clocks once the proof had passed, so a gate with a 20 s budget could
    start a 300 s `git worktree remove` after its budget was gone. Now no
    stage STARTS past the deadline (the rest is deferred, exactly as a proof
    that ran out is), and the subprocesses a stage runs are bounded by what
    is left of it."""
    paths = [w["path"] for w in workspaces]
    failures = []
    deferred = ""
    held = False

    def out_of_time(before):
        nonlocal deferred, held
        if _past(deadline):
            deferred = "the budget ran out before %s" % before
            held = True
        return held

    if processes is None and not out_of_time("its workspaces' processes were stopped"):
        processes = stop_processes(paths, deadline=deadline)
    processes = processes or {}
    if held:
        pass
    elif processes.get("survivors"):
        # Still dying when the budget ran out (V3-02: the kill wait no longer
        # outlives the deadline) is a deferral, not a failure counted toward
        # the CEO's notice.
        if not out_of_time("its workspaces' processes had stopped"):
            failures.append("processes still running in its workspaces: %s" % processes["survivors"])
            held = True
    elif processes.get("unknown"):
        # Not listed is not "none running" (V2-03): out of time, it is a
        # deferral like any other; otherwise a failure, retried.
        if not out_of_time("its workspaces' processes could be listed"):
            failures.append(processes["unknown"])
            held = True
    elif not out_of_time("its containers were stopped"):
        # Containers first, directories second: a workspace's containers are
        # part of it, and stop_containers never raises. See stop_containers.
        # Waited for only inside the budget (V3-02): the reaper runs on
        # Docker's clocks, not the caller's.
        finished, containers = _within(deadline, stop_containers, paths)
        if not finished:
            deferred = "the budget ran out while its containers were being stopped"
            held = True
        # §54 addendum 4, and it sits here rather than beside stop_processes
        # for the same reason containers do: it never raises and it never
        # blocks the deletion, so it cannot cost a land that would otherwise
        # have worked. A window that will not close is recorded for the alert,
        # not made into a reason to keep a landed worktree on disk.
        elif not out_of_time("its test instances were collected"):
            if not _within(deadline, stop_test_instances, paths, deadline=deadline)[0]:
                deferred = "the budget ran out while its test instances were being collected"
                held = True
        # A CONTAINER THAT COULD NOT BE STOPPED IS A SURVIVOR (hunt part 4,
        # finding 11). It used to be checked for a LANDED disposition only, so
        # a discard deleted the workspace, filed the record as done and left
        # the containers running with no retry at all, and a reaper that
        # crashed was indistinguishable from one that found nothing. Whatever
        # the disposition, it is now what a surviving process is: nothing of
        # the workspace is deleted, the failure is recorded and retried (point
        # 13), and after RETRY_TELL_CEO_AFTER attempts the CEO is told. The
        # deleter still never raises: tidying up cannot break it, and it can
        # no longer be forgotten either.
        unstopped = _containers_unstopped(containers)
        if unstopped:
            failures.append(unstopped)
            held = True
    if not held:
        current = load_agent(rec["key"]) or rec
        disposition = current.get("disposition") or {}
        if disposition.get("kind") == "landed":
            owner = load_agent(disposition.get("as_part_of") or rec["key"]) or current
            chain = _chain(owner)
            try:
                _require_landed(owner, chain, disposition.get("ignored_not_needed") or "", deadline)
            except Deadline as e:
                deferred = str(e)
                held = True
            except SpecError as e:
                # Eligibility expired. Return surviving work to the pending gate,
                # instead of retrying a forced deletion under an old verdict.
                with Lock():
                    for member in chain:
                        if not os.path.exists(agent_path(member["key"])):
                            continue
                        fresh = load_agent(member["key"])
                        if (fresh.get("disposition") or {}).get("kind") == "landed":
                            if fresh.get("deletion"):
                                fresh["landing_cleanup_started"] = True
                            fresh["disposition"] = None
                            fresh["deletion"] = None
                            fresh.setdefault("history", []).append({"at": iso(),
                                "fact": "landing eligibility changed", "why": str(e)})
                            save_agent(fresh)
                event("landing-reopened", key=rec["key"], why=str(e))
                return False

        for w in workspaces:
            # Point 3: the branches the agent created are its branches too, and
            # they are recorded (observe_created_refs) rather than read back out of
            # the directory here — the record survives the directory.
            if held:
                break
            if _past(deadline):
                deferred = "the budget ran out before %s was deleted" % w.get("path")
                held = True
                break
            caches = workspace_build_caches(w.get("path"))
            ok, err = remove_workspace(w, deadline=deadline)
            if ok:
                w["deleted_at"] = iso()
                discard_build_caches(rec["key"], w.get("path"), caches)
            else:
                failures.append(err)
    untouched = []
    if branches and not held:
        for repo, b in _branch_targets([rec]):
            if out_of_time("branch %s was deleted" % b):
                break
            ok, err = delete_branch(repo, b, deadline=deadline)
            if ok is None:
                # POINT 2, AT THE DELETER: a codex/ ref on this agent's record —
                # it can only get there by hand or by a defect, since
                # observe_created_refs never attributes one — is aimed at and
                # REFUSED by delete_branch, and the refusal is not a failed
                # deletion to retry until the CEO is told: it is the page's own
                # answer. The ref is dropped from the record with the fact
                # written down, and the agent's own deletion completes.
                untouched.append({"at": iso(), "fact": "codex/ untouched", "repo": repo, "branch": b, "why": err})
                rec["created_branches"] = [p for p in (rec.get("created_branches") or [])
                                           if (p[0], p[1]) != (repo, b)]
                for w in rec["workspaces"]:
                    if w.get("repo") == repo and w.get("branch") == b and not w.get("path"):
                        w["deleted_at"] = iso()
                event("codex-untouched", key=rec["key"], repo=repo, branch=b)
                continue
            if ok:
                for w in rec["workspaces"]:
                    if w.get("repo") == repo and w.get("branch") == b:
                        w["branch_deleted_at"] = iso()
                        if not w.get("path"):
                            w["deleted_at"] = iso()
                rec["created_branches"] = [p for p in (rec.get("created_branches") or [])
                                           if (p[0], p[1]) != (repo, b)]
            else:
                failures.append(err)
    if deferred:
        failures.append(deferred)
    with Lock():
        fresh = load_agent(rec["key"]) or rec
        fresh["workspaces"] = rec["workspaces"]
        fresh["created_branches"] = rec.get("created_branches") or []
        if untouched:
            fresh.setdefault("history", []).extend(untouched)
        if failures and failures == [deferred]:
            # Out of budget only: due again at once, and no attempt counted,
            # so a busy gate never walks a healthy deletion toward the CEO's
            # "keeps failing" notice.
            d = fresh.get("deletion") or {"attempts": 0}
            d["deferred"] = deferred[:400]
            d["next_at"] = now()
            d["branches"] = bool(branches)
            fresh["deletion"] = d
        elif failures:
            d = fresh.get("deletion") or {"attempts": 0, "first_failed_at": now()}
            d["attempts"] = d.get("attempts", 0) + 1
            d["last_error"] = "; ".join(failures)[:2000]
            d["next_at"] = now() + min(RETRY_CAP_SECONDS, RETRY_BASE_SECONDS * (2 ** (d["attempts"] - 1)))
            d["branches"] = bool(branches)
            fresh["deletion"] = d
        else:
            fresh["deletion"] = None
        save_agent(fresh)
        done = not failures and fresh.get("disposition") and fresh["disposition"].get("kind") != "continued"
        if done and not fresh["created_branches"] \
                and all(w.get("deleted_at") and (w.get("branch_deleted_at") or not w.get("branch"))
                        for w in fresh["workspaces"]):
            write_json(done_path(fresh["key"]), fresh)
            os.unlink(agent_path(fresh["key"]))
            _drop_snapshots(fresh["key"])
    event("deleted" if not failures else "deletion-failed", key=rec["key"], why=why,
          failures=failures or None, stopped=processes.get("stopped") or None)
    return not failures


def retry_due(budget=5.0, deadline=None):
    """Point 13: a failed deletion is retried with no one's involvement.

    The budget bounds the attempts, not only the gaps between them (hunt part
    4, finding 15): each attempt is given what is left of it, and a caller
    with its own deadline (the Stop gate) passes that, so the retries spend
    the gate's budget instead of adding to it."""
    t0 = now()
    stop_at = t0 + budget if deadline is None else min(t0 + budget, deadline)
    out = []
    for rec in all_agents():
        d = rec.get("deletion")
        if not d or d.get("next_at", 0) > now():
            continue
        if now() - t0 > budget or _past(stop_at):
            break
        targets = [w for w in live_workspaces(rec) if w.get("path")]
        ok = _delete(rec, targets, branches=d.get("branches", True), why="retry %d" % (d.get("attempts", 0) + 1),
                     deadline=stop_at)
        out.append((rec["key"], ok))
    return out


def keeps_failing():
    return [r for r in all_agents() if (r.get("deletion") or {}).get("attempts", 0) >= RETRY_TELL_CEO_AFTER]


_BUILD_CACHES = []


def _build_caches():
    """scripts/lib/build_caches.py, the same file the scheduled sweep loads, so the land and
    the sweep can never disagree about which cache folder belongs to which checkout."""
    if not _BUILD_CACHES:
        import importlib.util
        path = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
                            "scripts", "lib", "build_caches.py")
        spec = importlib.util.spec_from_file_location("richos_build_caches", path)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        _BUILD_CACHES.append(mod)
    return _BUILD_CACHES[0]


def workspace_build_caches(path):
    """The build output keyed to this workspace on the external drive (its Cargo folder,
    its iOS and Android build folders), read BEFORE the workspace is deleted, because the
    Cargo keys come from the Cargo.toml files in its tree. Never raises: a land is never
    refused over a cache."""
    try:
        return _build_caches().checkout_caches(path)
    except Exception as exc:                               # noqa: BLE001
        event("build-caches-unread", path=path, why=str(exc)[:300])
        return []


def discard_build_caches(key, path, caches):
    """THE MOMENT THE CACHE BECOMES GARBAGE IS THE MOMENT ITS WORKSPACE IS DELETED (CEO,
    2026-10-08: "Doesn't that cache clean-up belong to the Mega Lander then?"). Before this,
    nothing removed a landed workspace's build folders, and 1,103 Cargo folders plus the
    native ones filled the external drive. Each folder is moved aside in one rename and
    deleted in the background, so the land waits for nothing; the scheduled sweep takes
    whatever that deletion did not finish. Never raises."""
    if not caches:
        return
    try:
        bc = _build_caches()
        moved, bad = bc.discard(caches, bc.roots()[0])
    except Exception as exc:                               # noqa: BLE001
        moved, bad = [], [str(exc)]
    event("build-caches-discarded", key=key, path=path, moved=len(moved),
          folders=caches, failures=bad or None)


def remove_workspace(w, deadline=None):
    """Deletes one workspace. With a `deadline` (V2-03) its `git worktree
    remove` is bounded by what is left of it rather than by its own 300 s; a
    removal cut short leaves the directory, which is reported and retried."""
    path, repo = w.get("path"), w.get("repo")
    main = main_checkout(repo) if repo and os.path.isdir(repo) else ""
    if not main:
        return False, "%s: its repository %s cannot be read" % (path, repo)
    wl = worktree_list(main) or []
    entry = None
    for e in wl:
        if e["path"] == path:
            entry = e
    if entry and wl and entry is wl[0]:
        return False, "%s is the main checkout of %s and is never deleted" % (path, main)
    if entry and entry["branch"].startswith(CODEX_PREFIX):
        return False, "%s is now on %s; codex/ is never touched (point 2)" % (path, entry["branch"])
    if os.path.lexists(path):
        if entry is None:
            # Git no longer lists it. Delete it only if it is provably this
            # repository's workspace (its .git file points into this
            # repository's worktrees/), or the residue of a deletion that git
            # began (no .git left at all) — never a repository of its own.
            gitfile = os.path.join(path, ".git")
            if os.path.isdir(gitfile):
                return False, "%s is a repository of its own, not a workspace of %s; not deleted" % (path, main)
            if os.path.isfile(gitfile):
                with open(gitfile, encoding="utf-8", errors="replace") as f:
                    pointer = f.read()
                common = _common_dir(main)
                if not common or (os.path.join(common, "worktrees") + os.sep) not in realpath(
                        pointer.replace("gitdir:", "").strip()) + os.sep:
                    return False, "%s is not a workspace of %s; not deleted" % (path, main)
            shutil.rmtree(path, ignore_errors=True)
        else:
            rc, _o, err = git(main, "worktree", "remove", "--force", "--force", path, timeout=_bounded(deadline, 300))
            if rc != 0 and os.path.lexists(path):
                return False, "git worktree remove %s failed: %s" % (path, err.strip()[:300])
    # A registration whose directory is gone: remove exactly its own admin entry.
    wl = worktree_list(main) or []
    if any(e["path"] == path for e in wl):
        admin = _admin_dir_for(main, path)
        if admin:
            shutil.rmtree(admin, ignore_errors=True)
    if os.path.lexists(path):
        return False, "%s still exists after deletion" % path
    if any(e["path"] == path for e in (worktree_list(main) or [])):
        return False, "%s is still registered with git after deletion" % path
    return True, ""


def _common_dir(main):
    rc, out, _ = git(main, "rev-parse", "--path-format=absolute", "--git-common-dir")
    return realpath(out.strip()) if rc == 0 else ""


def _admin_dir_for(main, path):
    common = _common_dir(main)
    base = os.path.join(common, "worktrees") if common else ""
    if not base or not os.path.isdir(base):
        return ""
    for n in os.listdir(base):
        g = os.path.join(base, n, "gitdir")
        try:
            with open(g, encoding="utf-8") as f:
                target = f.read().strip()
        except OSError:
            continue
        if realpath(os.path.dirname(target)) == path or realpath(target) == os.path.join(path, ".git"):
            return os.path.join(base, n)
    return ""


def delete_branch(repo, b, deadline=None):
    """(True, "") deleted or already gone; (False, why) failed, retried later
    (point 13); (None, why) REFUSED BY THE PAGE — a codex/ branch is never
    touched (point 2), and that is an answer, not a failure to retry. With a
    `deadline` (V2-03) the deleting git call is bounded by what is left."""
    if b.startswith(CODEX_PREFIX):
        return None, "branch %s is codex/; never touched (point 2)" % b
    main = main_checkout(repo)
    wl = worktree_list(main) or []
    if not wl:
        return False, "%s: repository %s cannot be read" % (b, repo)
    if wl[0]["branch"] == b:
        return False, "branch %s is the main checkout's branch; never deleted" % b
    # "Already gone" is written down as deleted, so it must be git's answer,
    # never a read that failed (hunt part 4, finding 4): that is a failure,
    # retried later (point 13).
    tip, unread = branch_tip_read(main, b)
    if unread:
        return False, "branch %s: %s" % (b, unread)
    if not tip:
        return True, ""
    holders = [e["path"] for e in wl if e["branch"] == b]
    if holders:
        return False, "branch %s is still checked out at %s" % (b, holders[0])
    rc, _o, err = git(main, "branch", "-D", b, timeout=_bounded(deadline, 60))
    if rc != 0 or branch_tip(main, b):
        return False, "git branch -D %s failed: %s" % (b, err.strip()[:300])
    return True, ""


# ---------------------------------------------------------------------------
# point 9 — every process it started is stopped before deletion
# ---------------------------------------------------------------------------

def _process_cwds(timeout=60, strict=False):
    """{pid: cwd} for every process the OS will show us.

    `strict=True` answers None when the listing could not be made (lsof did not
    finish in `timeout`, could not run, is missing or failed) instead of {}: an empty answer from
    a listing that never happened would read as "nothing works there" and let
    a deletion go ahead over running writers (V2-03 bounds this call by the
    caller's deadline, which makes "did not finish" a real answer)."""
    out = {}
    if os.path.isdir("/proc/self"):
        for n in os.listdir("/proc"):
            if n.isdigit():
                try:
                    out[int(n)] = os.readlink("/proc/%s/cwd" % n)
                except OSError:
                    continue
        return out
    # A LISTING THAT FAILED IS NOT AN EMPTY ONE (hunt part 4 v3, V3-05). A
    # missing lsof, or one that exited non-zero (it names no file here, so a
    # non-zero exit is an error, never "nothing found"), used to answer {} in
    # strict mode too, and a deletion went ahead over a running process.
    if not shutil.which("lsof"):
        return None if strict else out
    try:
        r = subprocess.run(["lsof", "-a", "-d", "cwd", "-F", "pn", "-w"], capture_output=True, text=True,
                           timeout=timeout, env=_ps_env())
    except (OSError, subprocess.TimeoutExpired):
        return None if strict else out
    if r.returncode != 0 and strict:
        return None
    pid = None
    for line in r.stdout.splitlines():
        if line.startswith("p"):
            try:
                pid = int(line[1:])
            except ValueError:
                pid = None
        elif line.startswith("n") and pid:
            out[pid] = line[1:]
    return out


def process_table(timeout=30, strict=False):
    """{pid: {"ppid": int, "args": str}} for every process the OS shows, in one
    `ps` call. `strict=True` answers None, not {}, when `ps` did not finish or
    could not run (see _process_cwds)."""
    try:
        r = subprocess.run(["ps", "-axww", "-o", "pid=,ppid=,args="], capture_output=True, text=True,
                           timeout=timeout, env=_ps_env())
    except (OSError, subprocess.TimeoutExpired):
        return None if strict else {}
    if r.returncode != 0 and strict:
        return None                       # failed, not empty (V3-05)
    out = {}
    for line in r.stdout.splitlines():
        parts = line.strip().split(None, 2)
        if len(parts) >= 2 and parts[0].isdigit() and parts[1].isdigit():
            out[int(parts[0])] = {"ppid": int(parts[1]), "args": parts[2] if len(parts) > 2 else ""}
    return out


def _process_args(timeout=30):
    return dict((pid, p["args"]) for pid, p in process_table(timeout=timeout).items())


def _names_a_path(args, paths):
    return any((" " + p + os.sep) in (" " + args + os.sep) or (" " + p + " ") in (" " + args + " ")
               or args.endswith(" " + p) for p in paths)


def _protected_pids(deadline=None):
    """This process, its ancestors and every claude process: never stopped.
    Its `ps` reads are bounded by the caller's deadline (V3-02)."""
    keep = set()
    pid = os.getpid()
    for _ in range(64):
        if not pid or pid <= 1:
            break
        keep.add(pid)
        pid, _c = _ps_parent_and_comm(pid, timeout=_bounded(deadline, 10))
    for p, a in _process_args(timeout=_bounded(deadline, 30)).items():
        if os.path.basename((a.split() or [""])[0]) == "claude":
            keep.add(p)
    return keep


def processes_in(paths, deadline=None):
    """The processes that are the workspace's own: every process WORKING in one
    of `paths` (its directory is inside), and every descendant of one, wherever
    that descendant works. Never this process, its ancestors or claude.

    A COMMAND LINE THAT MERELY NAMES THE PATH IS NOT OWNERSHIP (hunt part 4,
    finding 12). A second loop used to add every process whose arguments named
    the workspace to the same stop list, so a reviewer, a log reader or an
    editor working from somewhere else lost its running work to a land it had
    nothing to do with. The reason for that loop holds, and is kept where it
    holds: a command the workspace's own process starts can work on the
    workspace from elsewhere (`cd / && cargo --manifest-path ...`), and that
    one is owned by its ANCESTRY, which is a fact the OS reports. A process
    whose only tie is its arguments is left running and named in the record
    (`_named_only`), never signaled.

    With a `deadline` (V2-03) the two listings are bounded by it. A listing
    that did not finish, could not run or FAILED makes the answer None --
    unknown, never "none" -- with or without a deadline (V3-05): a failed `ps`
    or `lsof` answered {} and certified a workspace empty while it ran.

    A GRADLE DAEMON A NATIVE BUILD KEPT WARM FOR THE WORKSPACE IS THE WORKSPACE'S OWN too
    (2026-10-02, scripts/lib/gradle_daemons.py). It works from Gradle's registry, not from
    the workspace, and the build that started it has ended, so neither test above sees it;
    its record (PID and birth generation, re-read here) does, and its children follow it by
    ancestry like any other descendant."""
    paths = [realpath(p) for p in paths if p]
    if not paths:
        return []
    keep = _protected_pids(deadline)
    hits = set(_gradle_daemons(paths))
    cwds = _process_cwds(timeout=_bounded(deadline, 60), strict=True)
    if cwds is None:
        return None
    for pid, cwd in cwds.items():
        c = realpath(cwd)
        if any(c == p or c.startswith(p + os.sep) for p in paths):
            hits.add(pid)
    hits -= keep
    table = process_table(timeout=_bounded(deadline, 30), strict=True)
    if table is None:
        return None
    grew = bool(hits)
    while grew:
        grew = False
        for pid, info in table.items():
            if pid not in hits and pid not in keep and info["ppid"] in hits:
                hits.add(pid)
                grew = True
    return sorted(hits)


def _named_only(paths, owned, deadline=None):
    """Processes whose command line names one of `paths` and that are not the
    workspace's own: reported, never stopped (finding 12). Bounded by the
    caller's deadline (V3-02)."""
    paths = [realpath(p) for p in paths if p]
    keep = _protected_pids(deadline)
    out = []
    for pid, args in _process_args(timeout=_bounded(deadline, 30)).items():
        if pid in owned or pid in keep:
            continue
        if _names_a_path(args, paths):
            out.append((pid, args[:200]))
    return sorted(out)


def _gradle_daemons(paths):
    """Live Gradle daemons native builds kept warm for workspaces at or inside `paths`
    (scripts/lib/gradle_daemons.py). Never raises: tidying up cannot break the deleter."""
    try:
        here = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "scripts", "lib")
        if here not in sys.path:
            sys.path.insert(0, here)
        import gradle_daemons
        return gradle_daemons.owned_pids(paths)
    except Exception as e:
        event("gradle-daemons-unread", why=str(e)[:200], paths=paths or None)
        return []


def _forget_gradle_daemons(paths):
    try:
        import gradle_daemons
        gradle_daemons.forget(paths)
    except Exception as e:
        event("gradle-daemons-unforgotten", why=str(e)[:200], paths=paths or None)


def stop_processes(paths, deadline=None):
    """Point 9. With a `deadline` (V2-03) the process listings are bounded by
    it, and a listing that could not be made is answered as UNKNOWN (never as
    "nothing works there"): {"stopped": [], "survivors": [], "unknown": why},
    which the deleter holds on exactly as it holds on a survivor."""
    pids = processes_in(paths, deadline)
    if pids is None:
        why = "the processes working in its workspaces could not be listed (the listing failed or ran out of time)"
        event("processes-unknown", paths=paths or None, why=why)
        return {"stopped": [], "survivors": [], "unknown": why}
    spared = _named_only(paths, set(pids), deadline)
    if spared:
        event("processes-named-only", pids=[p for p, _a in spared], args=[a for _p, a in spared],
              why="their command line names the workspace, but nothing shows the workspace started them; "
                  "left running (hunt part 4, finding 12)")
    if not pids:
        _forget_gradle_daemons(paths)
        return {"stopped": [], "survivors": []}
    # PIDs are reused, so the history keeps each one's start time (V2-11).
    process_start_before = {}
    for p in pids:
        st, when = process_start(p, timeout=_bounded(deadline, 10))
        if st == "ok":
            process_start_before[p] = when
    for p in pids:
        try:
            os.kill(p, signal.SIGTERM)
        except OSError:
            pass
    # Neither wait outlives the caller's deadline (V3-02): a process still
    # dying then is a survivor, which the caller defers rather than fails.
    grace_until = now() + PROCESS_STOP_GRACE
    if deadline is not None:
        grace_until = min(grace_until, deadline)
    alive = list(pids)
    while alive and now() < grace_until:
        time.sleep(0.1)
        alive = [p for p in alive if _alive(p)]
    for p in alive:
        try:
            os.kill(p, signal.SIGKILL)
        except OSError:
            pass
    # A SIGKILL cannot be refused, but the kernel still has to tear the process
    # down, and on a loaded Mac that takes no fixed time. This used to sleep
    # 0.2 s and record whatever was still there as a SURVIVOR, which fails the
    # land (`processes still running in its workspaces`) for a process that
    # was already dying (2026-09-29, the load-sensitive checks audit). So poll
    # until each is gone. The bound is only for a process the kernel cannot
    # finish (an uninterruptible wait): that one IS a survivor, and it is
    # reported as one instead of hanging the land.
    survivors = [p for p in alive if _alive(p)]
    # Inside the caller's budget too (V2-03): a process still dying when it
    # runs out is a survivor, which holds the deletion for a retry.
    kill_deadline = time.monotonic() + (PROCESS_KILL_WAIT if deadline is None
                                        else min(PROCESS_KILL_WAIT, max(0.0, deadline - now())))
    while survivors and time.monotonic() < kill_deadline:
        time.sleep(0.05)
        survivors = [p for p in survivors if _alive(p)]
    event("processes-stopped", pids=pids, survivors=survivors or None,
          starts={str(p): s for p, s in ((p, process_start_before.get(p)) for p in pids) if s})
    if not survivors:
        _forget_gradle_daemons(paths)
    return {"stopped": pids, "survivors": survivors}


def stop_test_instances(paths, deadline=None):
    """§54 ADDENDUM 4: a test instance of the app still running after its agent
    has finished is uncollected garbage, and the land step collects it.

    CEO, 2026-09-18, verbatim: "Ray had left the test app window open. Test app
    windows must always close/quit when testing is finished. Same hygiene as
    with any other garbage." The candidate-.7 walk ended with the QA instance
    still on his screen after the handoff.

    WHY stop_processes DOES NOT ALREADY DO THIS, measured rather than assumed.
    stop_processes matches a process by its cwd (and, since hunt part 4
    finding 12, by descent from such a process; its workspace path appearing
    in argv, which it used to match too, is no longer ownership). A test
    instance launched the way this engine launches one has NEITHER: scripts/
    lib/gui-launch.sh runs it `cd /` with `env -i`, so its cwd is `/`, and the
    candidate-.7 instance's argv was the relative
    `./RichOS.app/Contents/MacOS/richos-tauri`, which names no absolute path at
    all. Both tests miss it, which is exactly how it survived its own land.

    WHAT IS IN SCOPE HERE, AND WHAT IS DELIBERATELY NOT. This collects
    instances rooted under the paths being deleted -- the landing agent's own
    workspaces -- and nothing else. It does NOT sweep every scratch root on the
    machine, because a live peer agent's test instance is rooted under a scratch
    root too, and quitting a running colleague's app mid-test would be a new
    defect wearing this fix's clothes. The machine-wide arm belongs to the
    reaper, which knows which sessions are still alive; here we only know about
    this agent.

    A SURVIVOR DOES NOT BLOCK THE WORKSPACE DELETION, and that is on purpose. A
    window that will not close is not a reason to keep a landed worktree on
    disk; the two failures are unrelated. It is made DURABLE instead, so the
    §54 alert names it at session start and every turn end until it is gone.
    Never raises: tidying up must not be able to break the deleter it hangs off.
    """
    try:
        here = os.path.join(os.path.dirname(os.path.dirname(
            os.path.abspath(__file__))), "scripts", "lib")
        if here not in sys.path:
            sys.path.insert(0, here)
        import appinstances
        own = [p for p in paths if p]
        if not own:
            return {}
        # include_declared=False: ONLY this agent's workspaces. See the RootSet
        # docstring -- with the declared roots in scope this call would quit a
        # live peer's test instance.
        res = appinstances.collect_and_record(
            roots=appinstances.RootSet(extra=own, include_declared=False))
    except Exception as e:
        event("test-instances-uncollected", why=str(e)[:200], paths=paths or None)
        return {}
    if res.get("collected") or res.get("survivors") or res.get("undecided"):
        event("test-instances-collected",
              collected=[d["pid"] for d in res.get("collected") or []] or None,
              survivors=[d["pid"] for d in res.get("survivors") or []] or None,
              undecided=[d["pid"] for d in res.get("undecided") or []] or None)
    # The device collector takes a budget; with the caller's deadline (V2-03)
    # it is given what is left of it, and nothing is started once it is gone.
    if not _past(deadline):
        collect_test_devices(departing=paths,
                             budget=None if deadline is None else max(0.5, deadline - now()))
    return res


def collect_test_devices(departing=(), budget=None):
    """§54 FOR SIMULATORS AND EMULATORS (scripts/lib/testdevices.py). On
    2026-09-22 three iOS simulators booted by killed test runs outlived their
    agents and the session. Collected here machine-wide, because each device is
    removed only when its owner is PROVEN gone (a registered pid, the pid its
    name carries, or a recorded checkout that no longer exists), so a live
    peer's device is never touched. `departing` are the workspaces this land is
    deleting: a device keyed to one of them goes with it. Never raises."""
    try:
        here = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "scripts", "lib")
        if here not in sys.path:
            sys.path.insert(0, here)
        import testdevices
        res = testdevices.collect(apply=True, departing=[p for p in departing or () if p],
                                  deadline=(now() + budget) if budget else None)
    except Exception as e:
        event("test-devices-uncollected", why=str(e)[:200])
        try:
            from testdevice_alerts import record_failure
            record_failure("collector unavailable: %s" % e)
        except Exception as alert_error:
            sys.stderr.write("TEST DEVICE CLEANUP FAILED; alert state could not be written: %s\n" % alert_error)
        return {}
    # A shut-down device nothing proves the owner of is reported by the
    # collector itself; repeating it in the history at every land is noise.
    running = [d["id"] for d in res.get("undecided") or [] if d.get("state") != "Shutdown"]
    if res.get("notes"):
        event("test-devices-uncollected", why="; ".join(res["notes"])[:400])
    if res.get("collected") or res.get("survivors") or running or res.get("deferred"):
        event("test-devices-collected",
              collected=[d["id"] for d in res.get("collected") or []] or None,
              survivors=[d["id"] for d in res.get("survivors") or []] or None,
              undecided=running or None,
              deferred=[d["id"] for d in res.get("deferred") or []] or None)
    return res


def stop_containers(paths):
    """POINT 9'S OTHER HALF: the containers a workspace declared go with it.

    "reap PROCESSES, not just directories" was written after a detached child
    outlived both its agent and its worktree. A container is that same defect
    wearing a different hat, and nothing was reaping them:
    `agent-af36c9abcc76937ab-redis-1` — an agent id — sat on the founder's
    machine for six weeks after its agent ended.

    The whole mechanism, and the argument for every refusal in it, is in
    scripts/lib/containers.py. Two properties matter to THIS caller, and both
    are that file's job to keep: it NEVER raises, and it never removes a
    container that is not DECLARED by one of these paths. A machine with no
    Docker, or a daemon that is down, lands work exactly as it did before this
    function existed.
    """
    try:
        here = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "scripts", "lib")
        if here not in sys.path:
            sys.path.insert(0, here)
        import containers
        # ending=True: these workspaces are being deleted right now, so their
        # own liveness must not protect them from their own deletion. Every
        # other live workspace keeps its protection. See reap_for_workspaces.
        res = containers.reap_for_workspaces(paths, ending=True)
    except Exception as e:
        # Tidying up must never be able to break the deleter it is attached to,
        # so this still does not raise. But it is not a success either: an
        # empty result here used to be indistinguishable from "no containers",
        # so the deleter went on and nothing ever retried (hunt part 4, finding
        # 11). The error is returned, and _delete holds the workspace and
        # retries it like a process that survived its stop.
        event("containers-unreaped", why=str(e)[:200], paths=paths or None)
        return {"error": "container cleanup could not run: %s" % (str(e)[:200] or type(e).__name__)}
    if res.get("removed") or res.get("failed") or res.get("kept"):
        event("containers-reaped", removed=[r["name"] for r in res.get("removed") or []] or None,
              failed=[r["name"] for r in res.get("failed") or []] or None,
              kept=[r["name"] for r in res.get("kept") or []] or None)
    return res


def _containers_unstopped(res):
    """"" when stop_containers left nothing behind that it knows of, else the
    failure to record: named containers it could not remove, or the reason the
    reaper could not run at all. A machine without Docker is neither: the
    reaper reports it unavailable, and there is nothing to stop."""
    res = res or {}
    names = ["%s (%s)" % (r.get("name") or r.get("id") or "?", r.get("why") or "no reason given")
             for r in res.get("failed") or []]
    if names:
        return "workspace containers could not be stopped: %s" % ", ".join(names)
    if res.get("error"):
        return str(res["error"])
    return ""


def _alive(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    try:
        r = subprocess.run(["ps", "-o", "stat=", "-p", str(pid)], capture_output=True, text=True, timeout=5)
        return r.returncode == 0 and "Z" not in r.stdout
    except (OSError, subprocess.TimeoutExpired):
        return True


# ---------------------------------------------------------------------------
# point 9 — the lock-out, and point 3 — no worktree sessions
# ---------------------------------------------------------------------------

RECOVERY_SCRIPTS = ("stop.sh", "stop-work-ack.sh")


def engine_root():
    return os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def stop_command():
    return os.path.join(engine_root(), "scripts", "stop.sh")


def _one_plain_command(cmd):
    """True when `cmd` is ONE simple command: no operator, redirection,
    substitution or expansion anywhere a shell would act on it. Single quotes
    make everything literal; inside double quotes `$` and a backquote still
    act, so they are refused there too."""
    q = None
    i = 0
    while i < len(cmd):
        c = cmd[i]
        if q == "'":
            if c == "'":
                q = None
        elif q == '"':
            if c == '"':
                q = None
            elif c == "\\":
                i += 1
            elif c in "$`":
                return False
        elif c in "'\"":
            q = c
        elif c == "\\":
            i += 1
        elif c in ";&|<>()$`\n\r":
            return False
        i += 1
    return q is None


def lead_recovery_call(payload):
    """The name of the stop this lead call is, or "".

    POINT 3 NEVER TAKES AWAY THE STOP. A refused lead that cannot stop its
    agents is how 2026-09-22 went: ten agents at 100% CPU for 33 minutes, the
    lead refused TaskStop, SendMessage and every command, and the CEO stopping
    them from his own screen. So a refused lead may still stop agents: TaskStop,
    and the engine's own stop.sh / stop-work-ack.sh (which write only the ack
    TaskStop's guard asks for), called as ONE plain command resolving to THIS
    engine's copy. Nothing may ride along with it."""
    tool = str(payload.get("tool_name") or "")
    if tool == "TaskStop":
        return "TaskStop"
    if tool != "Bash":
        return ""
    ti = payload.get("tool_input") if isinstance(payload.get("tool_input"), dict) else {}
    cmd = str(ti.get("command") or "").strip()
    if not cmd or not _one_plain_command(cmd):
        return ""
    try:
        toks = shlex.split(cmd)
    except ValueError:
        return ""
    if toks and toks[0] in ("bash", "/bin/bash"):
        toks = toks[1:]
    if not toks:
        return ""
    p = os.path.expanduser(toks[0])
    if not os.path.isabs(p):
        base = str(payload.get("cwd") or "")
        if not base:
            return ""
        p = os.path.join(base, p)
    rp = realpath(p)
    for name in RECOVERY_SCRIPTS:
        if rp == realpath(os.path.join(engine_root(), "scripts", name)):
            return name
    return ""


def _hands_back_after_its_run(rec):
    """Is this a finished agent whose ONLY ending is its own run's end, so its
    report may still be handed back?

    THE REPORT IS NOT A WRITE (walk 6, 2026-10-05, walk-10608166d53f,
    esc-20261005T151403Z-2f5c3737). Point 9 refuses a restarted agent every tool
    "so it cannot write anywhere". The platform restarts an agent whose run ended
    without handing its report back, and the one thing that restart is for is
    SubagentHandback: the reviewer's verdict, written to nothing in any workspace.
    Refusing it left the back end with "changes-requested" and no reason, three
    times over, while its reviewer's own last words said the handback had been
    refused. So that one tool passes when the agent's end is its own SubagentStop
    and nothing else: not stopped, not disposed of, not an orphan, not of a
    session that has ended, and not one that already handed in its work. Every
    other tool stays refused, exactly as before."""
    end = rec.get("end") or {}
    if end.get("signal") != "SubagentStop" or rec.get("handed_in"):
        return False
    if rec.get("disposition") or rec.get("orphan") or rec.get("creation_failed"):
        return False
    if rec.get("session_id"):
        state, _why = session_state(rec["session_id"], rec.get("session_identity"))
        if state == "ended":
            return False
    return True


def barrier(payload):
    """Verdict for guard-sealed-worktree.sh: (kind, detail).
    FINISHED        a finished agent: refused every tool (point 9)
    HANDBACK        a finished agent's SubagentHandback, when its only ending is
                    its own run's end: its report is not a write (point 9's reason)
    FORBIDDEN       the lead of a claude --worktree session (point 3)
    RECOVERY        that lead stopping its agents: never refused
    REGISTERED      a registered, unfinished worker (paused included)
    UNREGISTERED    a worker with no registration (point 3)
    LEAD            the lead's own call"""
    sid = str(payload.get("session_id") or "")
    aid = str(payload.get("agent_id") or "")
    if not aid:
        s = load_session(sid) if sid else None
        if s and s.get("forbidden"):
            why = forbidden_now(sid, s)
            if why:
                stop = lead_recovery_call(payload)
                if stop:
                    return "RECOVERY", "%s stops agents, which a refused lead may always do (%s)" % (stop, why)
                return "FORBIDDEN", why
        recovery, host = _recovery_adapter()
        return "LEAD", recovery.cancellation_notice(host, sid)
    key = key_for_id(aid)
    rec = load_agent(key) if key else None
    if not rec:
        return "UNREGISTERED", "agent %s has no registration" % aid
    # POINT 9 MEETS POINT 11'S FOURTH ENDING. "The platform restarts finished
    # agents. A restarted agent is refused every tool." A STOPPED agent's
    # restart can be the FIRST thing that happens after the kill — before any
    # gate or command has looked at it — so the lock-out asks the platform's
    # own record here too, rather than waiting to be told.
    rec = observe_platform_end(rec)
    if rec.get("recovery_control"):
        return "CONTROLLED", "recovery %s disabled this worker's tools" % rec["recovery_control"]["job"]
    fin, _paused, why = finished_state(rec)
    if fin and str(payload.get("tool_name") or "") == "SubagentHandback" and _hands_back_after_its_run(rec):
        return "HANDBACK", "agent %s (%s) hands its report back after its run ended: %s" % (
            aid, rec.get("name"), why)
    if fin:
        return "FINISHED", "agent %s (%s) is finished: %s" % (aid, rec.get("name"), why)
    # App consults retain their existing research/media capability policy.
    # Axel/Wes use a separate restricted provider process; ordinary consults
    # are not those advisors merely because neither owns implementation.
    if (rec.get("recovery_assignment") or {}).get("kind") in ("review", "helper") and not (rec.get("recovery_assignment") or {}).get("observation_only") and not (
            ((rec.get("recovery_assignment") or {}).get("authority") or {}).get("host")) and str(
            payload.get("tool_name") or "") not in ("Read", "Glob", "Grep", "SubagentHandback"):
        return "READONLY", "agent %s is a read-only review/helper; request bounded host evidence instead" % aid
    # POINT 2, AT THE TOOL: "An agent never works inside a codex/ workspace."
    # The Bash guard refuses a command run there; the only hook that sees a
    # Write, Edit, MultiEdit or NotebookEdit is this one, and until round 8 an
    # agent's Edit with a file_path inside a codex/ workspace passed
    # (brief-audit-sage-round8 §4, measured with a registered agent). Whether
    # the path IS inside a codex/ workspace is read from git on disk — the
    # branch its worktree has checked out — never from the path's spelling.
    tool = str(payload.get("tool_name") or "")
    if tool in ("Write", "Edit", "MultiEdit", "NotebookEdit"):
        ti = payload.get("tool_input") if isinstance(payload.get("tool_input"), dict) else {}
        fp = str(ti.get("file_path") or ti.get("notebook_path") or "")
        cx = _codex_workspace_of(fp, str(payload.get("cwd") or ""))
        if cx:
            return "CODEX", "agent %s (%s) is writing %s inside the codex/ workspace %s" % (
                aid, rec.get("name"), fp, cx)
    # This call carries the agent's id, so it is a moment the platform vouches
    # for. Record what its repositories hold NOW; the catch-all PostToolUse
    # compares, and a ref that appeared in between was created by this agent
    # (point 3, "any branch an agent created"). It runs only for an UNFINISHED
    # agent: a ref Rich cuts after the run has ended, to rescue the work, is his
    # and is never a candidate at all.
    snapshot_refs(rec, str(payload.get("tool_use_id") or ""))
    recovery, host = _recovery_adapter()
    allowed, reason = recovery.begin_tool(host, payload, rec)
    if not allowed:
        return "CONTROLLED", reason
    return "REGISTERED", rec.get("name") or ""


def _codex_workspace_of(path, cwd=""):
    """The top level of the codex/ workspace `path` lies in, or "". Read from
    git: the nearest existing ancestor's toplevel, and the branch git lists
    for that worktree. A relative `path` is the path the tool will edit from
    the payload's working folder `cwd` (the hook process's own folder says
    nothing about it)."""
    path = os.path.expanduser((path or "").strip())
    if path and not os.path.isabs(path) and (cwd or "").strip():
        path = os.path.join(os.path.expanduser(cwd.strip()), path)
    p = realpath(path)
    if not p:
        return ""
    d = p if os.path.isdir(p) else os.path.dirname(p)
    while d and not os.path.isdir(d):
        d = os.path.dirname(d)
    if not d or d == os.sep:
        return ""
    rc, out, _ = git(d, "rev-parse", "--show-toplevel")
    if rc != 0:
        return ""
    top = realpath(out.strip())
    for e in worktree_list(top) or []:
        if e["path"] == top and (e.get("branch") or "").startswith(CODEX_PREFIX):
            return top
    return ""


def _recipient_record(session_id, to):
    """The record a SendMessage `to` names: the teammate's name, or the agent
    ID the platform's other lifecycle events use (TaskStop, SubagentStop)."""
    if not session_id or not to:
        return None
    rec = load_agent(named_key(session_id, to)) if NAME_RE.match(to) else None
    if not rec:
        key = key_for_id(to)
        rec = load_agent(key) if key else None
        if rec and rec.get("session_id") not in ("", None, session_id):
            rec = None                   # another session's agent is not this session's recipient
    return rec


def recipient_state(session_id, name):
    rec = _recipient_record(session_id, name)
    if not rec:
        return "unregistered", ""
    # A STOPPED agent is finished (point 11), so a message to it would restart
    # an agent that can do nothing — which is what guard-resume-isolation.sh
    # refuses. It asks this function, so this function asks the platform's own
    # record rather than waiting for a signal a stopped run never sends.
    rec = observe_platform_end(rec)
    fin, paused_, why = finished_state(rec)
    return ("finished" if fin else ("paused" if paused_ else "active")), why


# ---------------------------------------------------------------------------
# the Stop gate (point 5)
# ---------------------------------------------------------------------------

def _turn_started_by_person(transcript):
    """True when the turn now ending began with a message from a person — the
    CEO — rather than a platform notification. Read from the transcript."""
    # Read BACKWARD in bounded chunks until a row decides, so memory stays one
    # chunk wide (the reason the read was bounded) while a long tool-heavy turn,
    # whose initiating row lies earlier than any fixed window, still finds it.
    chunk = 4 * 1024 * 1024
    try:
        with open(transcript, "rb") as f:
            pos = os.fstat(f.fileno()).st_size
            tail = b""                   # the unfinished first line of the chunk after this one
            while pos > 0:
                start = max(0, pos - chunk)
                f.seek(start)
                data = f.read(pos - start) + tail
                pos = start
                parts = data.split(b"\n")
                if pos > 0:
                    tail, parts = parts[0], parts[1:]      # its start lies in an earlier chunk
                else:
                    tail = b""
                for raw in reversed(parts):
                    try:
                        d = json.loads(raw.decode("utf-8", "replace"))
                    except ValueError:
                        continue
                    if not isinstance(d, dict) or d.get("type") != "user":
                        continue
                    verdict = _row_is_a_persons(d)
                    if verdict is None:
                        continue         # this row starts no turn: the row before it does
                    return verdict
    except (OSError, TypeError):
        return False
    return False


def _row_is_a_persons(d):
    """None — this row starts no turn (a tool result, hook feedback, a
    compaction summary, image metadata: skip to the row before it);
    True — a person's turn, the CEO's; False — the platform's, a peer's, a
    notification's.

    DECIDED ON THE FIELDS THE PLATFORM STAMPS, NEVER ON THE SHAPE OF THE TEXT
    (round 8, item 4). Until round 8 this was a deny-list of five tag names over
    the text (`<task-notification`, `<teammate-message`, ...), so any shape the
    platform had not been listed for read as a person — and the transcripts on
    this machine already held one: 68 non-meta peer-message rows beginning
    `Another Claude session sent a message:` (v2.1.229–2.1.267, origin absent)
    passed the deny-list and spent the CEO's allowance. A textual ALLOW-list is
    wrong the other way: the CEO's own `[Image #5] …` turns (11 rows, origin
    human, promptSource typed/queued) begin with a bracket.

    Measured over every persisted transcript on this machine on 2026-09-13
    (2,591 files; 4,675 main-session user text rows; the census script and its
    output are in docs/verification/round8-fixes-2026-09-13-logs/): a row a
    person typed carries `origin.kind == "human"` (1,861 typed + 52 queued + 24
    suggestion_accepted + 92 through the SDK, all versions 2.1.217–2.1.269); the
    RichOS app's prompts through the `sdk-cli` entrypoint carry NO origin and
    `promptSource: sdk` (670 rows, v2.1.250–2.1.267 — an origin-only rule
    rejects every one of them); a notification carries `origin.kind ==
    "task-notification"` and `promptSource: system` (1,320 rows); a peer's
    message carries `origin.kind == "peer"` with `isMeta` (53 rows); hook
    feedback, image metadata and local-command caveats are `isMeta` with no
    origin (283 rows, none starts a turn); a compaction summary is
    `isCompactSummary` (8 rows, none starts a turn); and every main-session row
    with NO origin and NO promptSource is the platform's — `<command-name>`,
    `<local-command-stdout>`, `[Request interrupted by user]`, the 68 old-shape
    peer rows — 212 rows, not one of them a person's words. Both sides are
    asserted from those shapes in the fourteen (C5.15, C5.16)."""
    msg = d.get("message") or {}
    content = msg.get("content")
    texts = []
    if isinstance(content, str):
        texts = [content]
    elif isinstance(content, list):
        if any(isinstance(c, dict) and c.get("type") == "tool_result" for c in content):
            return None
        texts = [c.get("text", "") for c in content if isinstance(c, dict) and c.get("type") == "text"]
    text = "\n".join(texts).strip()
    origin = d.get("origin")
    kind = origin.get("kind") if isinstance(origin, dict) else None
    if d.get("isMeta") or d.get("isCompactSummary"):
        # A meta row with a stamped non-human origin (a peer's message, the
        # coordinator's) starts a turn that is not his. A meta row with no
        # origin (hook feedback, image metadata, a caveat, a summary) is part of
        # another row's turn: the row before it decides.
        if kind and kind != "human":
            return False
        return None
    if not text and not isinstance(origin, dict):
        return None
    if d.get("queueSkipAttachments") or d.get("promptSource") == "system":
        return False
    if isinstance(origin, dict):
        return kind == "human"
    return d.get("promptSource") in ("typed", "queued", "sdk", "suggestion_accepted")


def _allowance_state(rec, item):
    """WHAT THE ONE ALLOWANCE WAS SPENT ON — the item's decision-relevant state.

    Point 5 allows a reply that names the pending work, and the page binds that
    allowance in its own parenthesis: "(the reply names the pending work, WHICH
    IS HANDLED RIGHT AFTER)". Naming it is not handling it, so the allowance is
    spent per item and returns only when the item has MOVED. This is the
    fingerprint of "moved": the reason it is pending, what it is recorded as
    waiting on, the workspaces it still has, and the tip of every branch it
    still has. Rich committing what the agent left to its branch, or merging
    it, or recording what it waits on, all change it; a second turn that only
    says the name again does not.

    It is bounded by the ITEM's own branches — one rev-parse each — never by a
    scan of anything (see the gate's budget)."""
    tips = []
    for repo, b in sorted(_branch_targets([rec])):
        tips.append("%s\t%s\t%s" % (repo, b, branch_tip(repo, b)))
    return hashlib.sha1(json.dumps([
        item["why"], item["waiting"], item["waiting_on"],
        sorted((p or "", b or "") for p, b in item["workspaces"]),
        (rec.get("disposition") or {}).get("kind", ""),
        sorted(rec.get("lands_pending") or []),
        sorted(rec.get("continues") or []),
        tips,
    ], sort_keys=True).encode("utf-8")).hexdigest()


def _allowance_spent_message(spent):
    lines = ["=== That reply has already used its one allowance (point 5) ===",
             "  Spec: %s — \"the reply names the pending work, which is HANDLED RIGHT AFTER\"." % SPEC,
             "  Naming it a second time is not handling it. These items were named in an earlier",
             "  reply and have not moved since — same reason, same workspaces, same branch tips:"]
    for i, prev in spent:
        lines.append("   - %s: named at %s; %s" % (i["name"], iso(prev.get("at")), i["why"]))
    lines += ["  The allowance returns the moment the item MOVES: commit what it left to its branch,",
              "  merge it, record what it waits on, start the work that lands it, or discard it.",
              "  Handling it is the only way past this."]
    return "\n".join(lines)


def refused_session_text(why):
    return ("THIS SESSION IS NOT ALLOWED: %s. Nobody starts a session in its own workspace in RichOS "
            "(point 3); every tool but reading and stopping agents is refused, and retrying cannot succeed. "
            "To stop its agents, first prepare the acknowledgement with "
            "%s <name> [<name> ...] --ceo-word '<his words>', then make the printed TaskStop calls. Tell the CEO once that the session "
            "must be restarted from the repository's main checkout, and end the turn."
            % (why, stop_command()))


def _refused_session_notice(sid, why):
    """The Stop gate's answer for a refused session: said ONCE, never a hold.

    2026-09-22, 23:31-23:43Z: this gate refused the lead's turn end 42 times,
    each time demanding a `workspaces.sh wait` that the lock-out refused, and
    each refusal produced one more "still blocked" reply. A session that can do
    nothing but stop agents cannot land anything, so holding its turn only
    makes it spin; the finished work waits for the next session, whose start
    names it first (point 5)."""
    with Lock():
        rec = load_session(sid) or {}
        if rec.get("refused_notice") == why:
            return ""
        rec["refused_notice"] = why
        write_json(session_path(sid), rec)
    event("refused-session-told", session_id=sid)
    return refused_session_text(why)


def gate_stop(payload, entity):
    """(allowed, message). Point 5: Rich cannot end his turn while finished
    work is neither landed nor discarded, except as the page allows."""
    sid = str(payload.get("session_id") or "")
    if not sid:
        return True, ""
    s = load_session(sid)
    if s and s.get("forbidden"):
        why = forbidden_now(sid, s)
        if why:
            return True, _refused_session_notice(sid, why)
    # ONE BUDGET FOR THE WHOLE GATE (hunt part 4, finding 15): the retries used
    # to run first, on their own clock, before the gate's deadline existed.
    deadline = _gate_deadline(GATE_STOP_BUDGET)
    retry_due(deadline=deadline)
    report = {}
    items = pending(sid, entity, scan=True, deadline=deadline, report=report)
    loud = keeps_failing()
    notes = []
    if report.get("deferred"):
        notes.append("NOTE: the gate answered inside its budget rather than overrunning the hook's "
                     "timeout, which the platform would have canceled, discarding this answer. "
                     "%d item(s) were not checked for an automatic land this turn and STAY PENDING: %s. "
                     "That is the safe answer, not a missing one. `workspaces.sh land <name>` run by "
                     "hand has no budget."
                     % (len(report["deferred"]), ", ".join(sorted(set(report["deferred"])))))
    for r in loud:
        d = r["deletion"]
        notes.append("TELL THE CEO: deleting %s has failed %d times since %s: %s" % (
            r.get("name"), d.get("attempts"), iso(d.get("first_failed_at")), d.get("last_error")))
    # THE HOLDS, AT EVERY TURN END (HOLDS above): never shown just once.
    try:
        notes = hold_lines(sid) + notes
    except Exception as e:  # noqa: BLE001: an unread hold list is said, never silence
        notes.insert(0, "HOLDS NOT READ this turn (%s: %s): held work may be out of sight; "
                        "workspaces.sh holds" % (e.__class__.__name__, str(e)[:160]))
    blocking = [i for i in items if i["blocks_turn_end"]]
    if not blocking:
        msg = "\n".join(notes)
        if items:
            msg = (msg + "\n" if msg else "") + "Pending finished work (waiting): " + \
                ", ".join("%s [%s: %s]%s" % (i["name"], i["waiting"], i["waiting_on"],
                                             " (%s)" % i["why"] if "did not land on its own" in i["why"] else "")
                          for i in items)
        return True, msg
    last = str(payload.get("last_assistant_message") or "")
    if last and _turn_started_by_person(str(payload.get("transcript_path") or "")) \
            and all(i["name"] in last for i in blocking):
        # THE ALLOWANCE IS CONSUMED. It used to be unlimited: ten consecutive
        # turns whose reply merely contained each item's name all ended, and
        # nothing anywhere made the work get handled afterwards. The page's own
        # parenthesis binds it, so it is spent per item and returns only when
        # that item's state has changed (_allowance_state).
        spent, state = [], {}
        for i in blocking:
            rec = load_agent(i["key"])
            if not rec:
                continue
            state[i["key"]] = _allowance_state(rec, i)
            prev = rec.get("answer_allowance") or {}
            if prev.get("state") == state[i["key"]]:
                spent.append((i, prev))
        if not spent:
            for i in blocking:
                if i["key"] not in state:
                    continue
                with Lock():
                    rec = load_agent(i["key"])
                    if not rec:
                        continue
                    rec["answer_allowance"] = {"at": now(), "state": state[i["key"]], "session": sid}
                    save_agent(rec)
                event("answer-allowance-used", key=i["key"])
            return True, "\n".join(notes)
        return False, _allowance_spent_message(spent) + "\n" + \
            gate_message(blocking, "end your turn") + ("\n" + "\n".join(notes) if notes else "")
    return False, gate_message(blocking, "end your turn") + ("\n" + "\n".join(notes) if notes else "")


# ---------------------------------------------------------------------------
# the lifecycle hook: facts, never a refusal
# ---------------------------------------------------------------------------

def _taskstop_id(resp, depth=0):
    if depth > 3:
        return ""
    if isinstance(resp, str):
        try:
            return _taskstop_id(json.loads(resp), depth + 1)
        except ValueError:
            return ""
    if isinstance(resp, list):
        for c in resp:
            v = _taskstop_id(c, depth + 1)
            if v:
                return v
        return ""
    if isinstance(resp, dict):
        if resp.get("is_error") or resp.get("error") or resp.get("success") is False:
            return ""
        tid = resp.get("task_id")
        if isinstance(tid, str) and AGENT_ID_RE.match(tid):
            return tid
        if resp.get("type") == "text" and isinstance(resp.get("text"), str):
            return _taskstop_id(resp["text"], depth + 1)
    return ""


def _agent_id_from_response(resp):
    if isinstance(resp, dict):
        v = str(resp.get("agentId") or "").strip()
        if AGENT_ID_RE.match(v):
            return v
    text = json.dumps(resp) if not isinstance(resp, str) else resp
    m = re.search(r"agentId:\s*([A-Za-z0-9_-]+)", text or "")
    return m.group(1) if m else ""


def lifecycle(payload, entity):
    """Records the facts the page names. Returns (context_text, notices)."""
    ev = str(payload.get("hook_event_name") or "")
    sid = str(payload.get("session_id") or "")
    notices = []
    ctx = ""
    if ev in ("PostToolUse", "PostToolUseFailure"):
        recovery, host = _recovery_adapter()
        recovery.finish_tool(host, payload)
    if ev == "SessionStart":
        rec, foreign = session_start(sid, str(payload.get("cwd") or entity or ""),
                                     str(payload.get("source") or ""), str(payload.get("agent_id") or ""))
        if foreign:
            # A subagent's compaction: nothing about the lead changes, and the
            # lead's gate is no business of an agent that cannot land anything.
            return "", []
        if rec.get("forbidden"):
            notices.append(refused_session_text(rec["forbidden"]))
        if str(payload.get("source") or "") in LAUNCH_SOURCES:
            # A session that STARTS is the first moment after a killed run
            # that anything is listening: 2026-09-22's three simulators were
            # still booted when the next session began. Bounded, because this
            # runs inside the hook's timeout; what is not reached is collected
            # by the next land or sweep.
            collect_test_devices(budget=8.0)
        release_session_holds(None, notices)
        retry_due()
        items = pending(sid, entity, scan=True)
        if items:
            ctx = ("RECONCILE FINISHED WORK FIRST (point 5); independent agents may start while it is pending.\n"
                   + gate_message(items, "declare this work complete"))
    elif ev == "SessionEnd":
        record_session_end(sid, str(payload.get("reason") or "SessionEnd"))
        release_session_holds(sid)
        collect_test_devices(budget=8.0)
    elif ev == "SubagentStart":
        record_start(sid, str(payload.get("agent_id") or ""), str(payload.get("cwd") or ""),
                     str(payload.get("agent_type") or ""))
    elif ev == "SubagentStop":
        record_end(sid, str(payload.get("agent_id") or ""), "SubagentStop")
        recovery, host = _recovery_adapter()
        recovery.acknowledgement(host, sid, str(payload.get("agent_id") or ""), "SubagentStop")
        collect_test_devices(budget=8.0)
    elif ev == "TaskCompleted":
        record_handed_in(sid, str(payload.get("agent_id") or payload.get("agentId") or ""),
                         str(payload.get("teammate_name") or payload.get("teammateName") or ""))
    elif ev == "PostToolUse":
        tool = str(payload.get("tool_name") or "")
        ti = payload.get("tool_input") if isinstance(payload.get("tool_input"), dict) else {}
        if tool == "Agent":
            aid = _agent_id_from_response(payload.get("tool_response"))
            if aid:
                # The name and isolation of the call the platform actually ran:
                # the repair path in bind_agent needs them when no registration
                # carries this tool call (its PreToolUse was killed).
                bind_agent(sid, str(payload.get("tool_use_id") or ""), aid, entity,
                           name=str(ti.get("name") or ""), isolation=str(ti.get("isolation") or ""))
        elif tool == "TaskStop":
            aid = _taskstop_id(payload.get("tool_response"))
            if aid:
                record_end(sid, aid, "stopped", "TaskStop")
                recovery, host = _recovery_adapter()
                recovery.acknowledgement(host, sid, aid, "stopped")
                collect_test_devices(budget=8.0)
        elif tool == "SendMessage":
            to = str(ti.get("to") or "")
            msg = ti.get("message")
            text = msg if isinstance(msg, str) else ""
            if to:
                rec = _recipient_record(sid, to)
                if rec:
                    until = prompt_lines(text, "pause-until")
                    if prompt_lines(text, "pause-until") or re.search(r"(?m)^\s*pause-until:\s*$", text):
                        try:
                            pause(rec["key"], until[0] if until else "", sid, notices,
                                  control=_generated("pause", text), end=_generated_end(text))
                        except SpecError as e:
                            notices.append(str(e))
                    else:
                        # A FREE-TEXT MESSAGE CREATES NO OBLIGATION (orchestration reliability R1,
                        # richos-hq docs/prds/2026-10-23-orchestration-reliability.md): "Yes, stop
                        # it now" and "Hand over now" were kept as corrections a later commit had
                        # to fulfil, and "wait for my go" as a hold. Only the structured pause
                        # above, the generated RESUME and a stop change the lifecycle; a real
                        # correction goes in the teammate's TASK or a corrected continuation brief.
                        # A LEGACY "held" WAIT (recorded from a message before 2026-10-24) still
                        # ends at the go (second review rv-20261022T002958Z-2ac46e09-fa4e,
                        # finding 1): a status question or a note leaves the teammate held.
                        if (rec.get("waiting") or {}).get("kind") == "held" and _hold_go(text):
                            with Lock():
                                fresh = load_agent(rec["key"])
                                fresh.setdefault("history", []).append(
                                    {"at": iso(), "fact": "hold released", "waiting": fresh.get("waiting"),
                                     "how": {"work_runs": "a later message to it"}})
                                fresh["waiting"] = None
                                save_agent(fresh)
                            event("hold-released", key=rec["key"], work_runs="a later message to it")
                        st = finished_state(rec)
                        if st[1] and (rec.get("pause") or {}).get("control") and not _generated("resume", text):
                            # Sage's catch 3: a land's in-flight notice or any other message
                            # must not thaw a generated pause. The agent reads it when it wakes.
                            event("pause-kept", key=rec["key"])
                            notices.append(
                                "%s is WAITING under the generated pause: this message did not resume it. Its "
                                "work stays frozen and its new commands wait until you send the generated RESUME: "
                                "pause_protocol.py --resume --to %s" % (rec["name"], rec["name"]))
                        elif st[1]:
                            resume(rec["key"], sid, notices)
        elif tool == "Bash":
            cmd = str(ti.get("command") or "")
            if re.search(r"\bgit\b.*\b(merge|pull|rebase|cherry-pick)\b", cmd):
                retry_due()
                pending(sid, entity, scan=False)
    if ev in ("SubagentStop", "SubagentStart", "TaskCompleted"):
        retry_due(budget=2.0)
    return ctx, notices


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def current_session():
    """The session a CLI call runs in: the recorded session whose process is
    this command's ancestor (point 12's record, read against the OS)."""
    v = (os.environ.get("RICHOS_SESSION_ID") or "").strip()
    if v:
        return v
    pid = session_pid()
    if not pid:
        return ""
    st, text = process_start(pid)
    try:
        names = os.listdir(_p("sessions"))
    except OSError:
        names = []
    for n in names:
        r = read_json(_p("sessions", n))
        if r and r.get("pid") == pid and r.get("pid_start") == text:
            return r.get("session_id") or ""
    rec = read_json(os.path.join(_platform_sessions_dir(), "%d.json" % pid))
    return str((rec or {}).get("sessionId") or "")


def _print_status(me, entity):
    # A status is a question: it lands nothing, deletes nothing, stops nothing
    # and claims nothing (hunt part 4, finding 29). What the next gate or land
    # WOULD land is shown as LANDABLE instead.
    items = pending(me, entity, scan=True, dry=True)
    print("session: %s" % (me or "(none)"))
    for repo in sorted(all_integration_records()):
        r = all_integration_records()[repo]
        print("integrates on: %s  %s  (%s)" % (r.get("branch"), repo, r.get("source")))
    if not items:
        print("pending: none")
    for i in items:
        label = "LANDABLE" if i.get("would_land") else ("UNREGISTERED" if i.get("unregistered") else
                                                       ("DAMAGED " if i.get("damaged") else "PENDING "))
        note = ("  [lands at the next gate, or: workspaces.sh land %s]" % i["name"]) if i.get("would_land") else ""
        print("%s %s  %s%s%s%s" % (label, i["name"], i["why"],
                                    ("  [waiting %s: %s]" % (i["waiting"], i["waiting_on"])) if i["waiting"] else "",
                                    "  [independent spawns allowed]", note))
        for p, b in i["workspaces"]:
            print("           %s  %s" % (p, b or ""))
    for r in all_agents():
        if r.get("disposition") or any(i["key"] == r["key"] for i in items):
            if r.get("deletion"):
                d = r["deletion"]
                print("RETRYING %s  deletion attempt %d failed: %s" % (r.get("name"), d.get("attempts"),
                                                                     d.get("last_error")))
            continue
        # The same read-only view the dry pending used (V2-05): a stopped
        # worker of another session prints FINISHED, not WORKING.
        fin, paused_, why = finished_state(platform_view(r))
        print("%-8s %s  %s" % ("PAUSED" if paused_ else ("FINISHED" if fin else "WORKING"), r.get("name"), why))
    return 0


def land_sweep_scopes(ref, me=""):
    """What a land may sweep: for the agent and every agent its work continues,
    its name, its session and every workspace it ever had. Read BEFORE the land,
    because the land deletes the workspaces. [] when the record cannot be read,
    and then the land sweeps nothing (never the whole machine instead).

    `hold_named` is set when another agent with the same name is not finished:
    a directory named for this agent could then be that one's, so it is kept."""
    try:
        rec = _resolve_record(ref, me)
    except Exception:
        return []
    if not rec:
        return []
    others = []
    try:
        others = all_agents()
    except Exception:
        others = []
    scopes = []
    for r in _chain(rec):
        name = r.get("name") or ""
        if not name:
            continue
        hold = False
        for o in others:
            if o.get("key") == r.get("key") or o.get("name") != name:
                continue
            try:
                fin, _paused, _why = finished_state(o)
            except Exception:
                fin = False
            if not fin:
                hold = True
                break
        scopes.append({"name": name, "session": r.get("session_id") or "",
                       "workspaces": [w["path"] for w in r.get("workspaces") or [] if w.get("path")],
                       "hold_named": hold})
    return scopes


def sweep_scratch_after_land(scopes=None, deadline=None):
    """Reclaim the landed agent's scratch, right here, while somebody is looking.

    ONLY THE LANDED AGENT'S (2026-10-01). This used to run the whole machine's
    sweep, so landing one agent took every other agent's and session's eligible
    scratch with it: two lands that night deleted 141 entries, none of them the
    landed agent's. Now it passes the agent's name, session and workspaces
    (land_sweep_scopes) and the reaper plans only what provably belongs to it:
    allocations made from its workspaces and directories named for it
    (scratch-reaper.py scan_agent). Everything else it looks at is counted as
    unattributed and left to the scheduled sweep, and the printed line says how
    many. No scope, no sweep.

    A LAND IS THE MOMENT THE GARBAGE BECOMES GARBAGE. The agent has finished, its
    workspace is gone, and every temporary directory its harnesses made is now
    owned by nothing. Waiting up to six hours for the scheduled sweep means the
    next agent starts on a disk carrying the last one's leavings — and on
    2026-09-17 one agent's leavings were 105.3 GB.

    NEVER FATAL, AND NEVER NOISY ON SUCCESS. The land has already happened and
    printed its result by the time this runs; a sweeper that raised here would
    turn a completed land into a traceback and leave the operator unsure whether
    the land took. So every failure is one line on stderr and nothing else, and a
    sweep that reclaims nothing says nothing at all.

    It calls scripts/scratch-sweep.sh — the app-facing entry point, whose whole
    contract is "safe to call constantly, safe to call concurrently, one line of
    JSON" — rather than the reaper directly. That gets the flock for free, so a
    land that overlaps the launchd job or an app-triggered sweep collapses into
    one instead of two runs measuring a tree the other is deleting.
    """
    sweep = os.path.join(os.path.dirname(os.path.dirname(
        os.path.abspath(__file__))), "scripts", "scratch-sweep.sh")
    if not scopes or not os.access(sweep, os.X_OK):
        return
    for scope in scopes:
        if _past(deadline):
            print("scratch sweep after land deferred: landing deadline reached", file=sys.stderr)
            return
        args = ["bash", sweep, "--agent", scope["name"]]
        if scope.get("session"):
            args += ["--session", scope["session"]]
        for w in scope.get("workspaces") or []:
            args += ["--workspace", w]
        if scope.get("hold_named"):
            args.append("--hold-named")
        try:
            r = subprocess.run(args, capture_output=True, text=True, timeout=_bounded(deadline, 600))
        except (OSError, subprocess.TimeoutExpired) as exc:
            print("scratch sweep after land could not run: %s" % exc, file=sys.stderr)
            continue
        line = (r.stdout or "").strip().splitlines()
        if not line:
            continue
        try:
            rep = json.loads(line[-1])
        except ValueError:
            continue
        for out in _land_sweep_lines(scope["name"], rep):
            print(out[1], file=sys.stderr)


def _land_sweep_lines(name, rep):
    """[(to_stderr, text)] for one scoped sweep's JSON. SPEAK ONLY WHEN THERE IS
    SOMETHING TO SAY: a land already prints a lot. Something is: a failure, a
    skip, bytes reclaimed, or entries left alone because they are not provably
    this agent's (the count the land owes, whatever was reclaimed)."""
    def entries(n):
        return "%d entr%s" % (n, "y" if n == 1 else "ies")
    if rep.get("failures"):
        return [(True, "SCRATCH SWEEP: %d deletion(s) FAILED after this land — %s"
                 % (rep["failures"], rep.get("reason") or "")),
                (True, "  The CEO's rule (ceo-decisions §54): if the clean-up fails, it is"
                 " deleted BY HAND. scripts/disk-watchdog.sh --status names the paths.")]
    if rep.get("skipped"):
        return [(False, "scratch of %s NOT swept: %s; the scheduled sweep will not take its "
                 "named directories" % (name, rep.get("reason") or "skipped"))]
    swept = int(rep.get("swept") or 0)
    unattr = int(rep.get("unattributed") or 0)
    undec = int(rep.get("undecidable") or 0)
    if not swept and not unattr and not undec:
        return []
    text = "scratch swept for %s: %s reclaimed (%s of its own)" % (
        name, rep.get("freed_human") or "0 B", entries(swept))
    if undec:
        text += "; %s of its own kept undecided (scratch-reaper.log says why)" % entries(undec)
    text += "; %s not provably its own, left alone" % entries(unattr)
    return [(False, text)]


def main(argv):
    import argparse
    ap = argparse.ArgumentParser(prog="workspaces.sh", description="the CEO's workspace spec (%s)" % SPEC)
    ap.add_argument("--session", default="")
    ap.add_argument("--entity", default="")
    sub = ap.add_subparsers(dest="cmd")
    sub.add_parser("status")
    x = sub.add_parser("land")
    x.add_argument("agent")
    x.add_argument("--ignored-not-needed", default="")
    x = sub.add_parser("merge", help="merge a finished teammate's branches into the branch its work "
                                     "integrates on, then land it: its workspaces and branches are deleted")
    x.add_argument("agent")
    x.add_argument("-m", "--message", default="")
    x = sub.add_parser("discard")
    x.add_argument("agent")
    x.add_argument("--reason", required=True)
    x.add_argument("--ceo-word", default="")
    x.add_argument("--not-ceo-ordered", default="")
    x = sub.add_parser("pause")
    x.add_argument("agent")
    x.add_argument("--until", required=True)
    x = sub.add_parser("resume")
    x.add_argument("agent")
    x = sub.add_parser("stop")
    x.add_argument("agent")
    x.add_argument("--why", default="its work is no longer wanted")
    x = sub.add_parser("wait")
    x.add_argument("agent")
    g = x.add_mutually_exclusive_group(required=True)
    g.add_argument("--started")
    g.add_argument("--outside")
    g.add_argument("--ceo")
    x.add_argument("--todo", default="")
    x.add_argument("--until", default="", help="its checkable end: agent:<name> | review:<id> | at:<UTC> | "
                                              "ceo:<CEO-TODOs item>; without one the hold is reported")
    x = sub.add_parser("hold", help="held work with no teammate yet; reported until it runs, lands or he decides")
    x.add_argument("name")
    x.add_argument("--what", required=True)
    x.add_argument("--until", default="")
    x.add_argument("--why", default="")
    x.add_argument("--by", default="rich")
    x = sub.add_parser("release", help="end a hold: his words, or a false catch taken back")
    x.add_argument("name")
    g = x.add_mutually_exclusive_group()
    g.add_argument("--ceo-word", default="")
    g.add_argument("--not-a-hold", default="")
    sub.add_parser("holds")
    sub.add_parser("retry")
    x = sub.add_parser("integration-branch")
    x.add_argument("--repo", default="")
    x = sub.add_parser("integration")
    x.add_argument("--repo", default="")
    x.add_argument("--branch", default="")
    x.add_argument("--why", default="")
    x.add_argument("--correct", action="store_true",
                   help="change the branch of the body of work that is CURRENT, keeping its "
                        "id, so every agent already bound to it moves with it. Without this, "
                        "a recording STARTS a new body of work and reaches no running agent.")
    x.add_argument("--all", action="store_true",
                   help="list every body of work, superseded ones included — a superseded "
                        "one is what an agent spawned for it still lands against")
    x = sub.add_parser("register-cc")
    for f in ("--name", "--repo", "--path", "--branch"):
        x.add_argument(f, required=True)
    x.add_argument("--integration", default="",
                   help="a branch this teammate alone lands on (teammate_body_of_work)")
    x = sub.add_parser("confirm-cc")
    x.add_argument("--name", required=True)
    x.add_argument("--path", required=True)
    x.add_argument("--failed", default="")
    x = sub.add_parser("withdraw-cc",
                       help="withdraw a registration whose spawn NEVER HAPPENED: delete its cc/ "
                            "workspaces and branches and remove the record, so the name is free "
                            "again. Refuses anything that ran (point 3's other half).")
    x.add_argument("--name", required=True)
    x.add_argument("--why", default="")
    for n in ("hook", "gate-stop", "register-spawn", "register-readonly", "barrier",
              "observe-refs", "check-spawn"):
        sub.add_parser(n)
    x = sub.add_parser("recipient")
    x.add_argument("--name", required=True)
    a = ap.parse_args(argv)
    if not a.cmd:
        ap.print_help()
        return 2
    entity = a.entity or os.environ.get("RICHOS_ENTITY_ROOT_RESOLVED", "")
    try:
        if a.cmd in ("hook", "gate-stop", "register-spawn", "register-readonly", "barrier",
                     "observe-refs", "check-spawn"):
            raw = sys.stdin.read()
            try:
                payload = json.loads(raw)
            except ValueError:
                payload = None
            if not isinstance(payload, dict):
                if a.cmd == "barrier":
                    print("ERROR\tthe payload is unparseable")
                    return 0
                if a.cmd == "observe-refs":
                    return 0
                if a.cmd in ("register-spawn", "register-readonly", "check-spawn"):
                    sys.stderr.write("the spawn payload is unparseable; it cannot be registered\n")
                    return 2
                return 0
            if not entity and payload.get("cwd"):
                entity = main_checkout(str(payload["cwd"]))
            if a.cmd == "observe-refs":
                for repo, b in observe(payload):
                    print("CREATED\t%s\t%s" % (repo, b))
                for r in PROTECTED_REF_FINDINGS:
                    print("%s\t%s\t%s\t%s\t%s\t%s" % r)
                return 0
            if a.cmd == "barrier":
                k, d = barrier(payload)
                print("%s\t%s" % (k, d.replace("\t", " ").replace("\n", " ")))
                return 0
            if a.cmd == "register-spawn":
                rec = register_spawn(payload, entity)
                print("REGISTERED\t%s" % rec["key"])
                return 0
            if a.cmd == "check-spawn":
                # THE DRY EVALUATION. Same refusals, nothing written. It refuses
                # a payload that is NOT a dry one rather than evaluating it,
                # because a caller that reached here with a live tool_use_id
                # wanted `register-spawn` and would otherwise be told its spawn
                # was fine while no registration was written for it.
                if not is_spawn_check(payload):
                    sys.stderr.write("check-spawn evaluates a spawn that has not been made: the payload "
                                     "must carry richos_spawn_check and no tool_use_id. This one carries "
                                     "a tool_use_id, so it is a live call - use register-spawn.\n")
                    return 2
                register_spawn(payload, entity, dry=True)
                print("WOULD-REGISTER\t%s"
                      % named_key(str(payload.get("session_id") or ""),
                                  str((payload.get("tool_input") or {}).get("name") or "")))
                return 0
            if a.cmd == "register-readonly":
                rec = register_readonly(payload, entity)
                print("REGISTERED\t%s" % rec["key"])
                return 0
            if a.cmd == "gate-stop":
                ok, msg = gate_stop(payload, entity)
                if not ok:
                    sys.stderr.write(msg + "\n")
                    return 2
                if msg:
                    print(json.dumps({"systemMessage": msg}))
                return 0
            ctx, notices = lifecycle(payload, entity)
            out = {}
            if ctx and payload.get("hook_event_name") == "SessionStart":
                out["hookSpecificOutput"] = {"hookEventName": "SessionStart", "additionalContext": ctx}
            if notices:
                out["systemMessage"] = "\n".join(notices)
            # The measured hold/release result is also context for the lead, who reports it.
            held = [n for n in notices if n.startswith(("HOLD ", "RELEASE ")) or "is WAITING under the generated pause" in n]
            if held and payload.get("hook_event_name") == "PostToolUse":
                out["hookSpecificOutput"] = {"hookEventName": "PostToolUse", "additionalContext": "\n".join(held)}
            if out:
                print(json.dumps(out))
            return 0
        me = a.session or current_session()
        if a.cmd == "status":
            return _print_status(me, entity)
        if a.cmd in ("land", "merge"):
            # Taken BEFORE the land: the record is read for the agent's type and
            # its transcript, and a land that ends in a deletion retry must not
            # cost the count. The transcript itself lives in the platform's
            # projects directory and no workspace deletion touches it.
            qa_lines = qa_throwaway_lines(a.agent, me)
            if a.cmd == "merge":
                merged, res = merge_and_land(a.agent, me, a.message)
                for repo, b in merged:
                    print("merged: %s into the integration branch of %s" % (b, repo))
                res = res or {}
                if res.get("kept_ignored"):
                    print("kept: ignored files it left are in %s" % res["kept_ignored"])
            else:
                res = land(a.agent, me, ignored_ok=a.ignored_not_needed) or {}
            if res.get("cleanup_pending"):
                print("landed: %s — deletion of its workspaces and branches is NOT finished; it is retried" % a.agent)
            else:
                print("landed: %s — every workspace and branch deleted" % a.agent)
            for _l in qa_lines:
                print(_l)
        elif a.cmd == "discard":
            r = discard(a.agent, a.reason, a.ceo_word, a.not_ceo_ordered, me)
            print("discarded: %s — tips recorded: %s" % (a.agent, json.dumps(r["tips"])))
        elif a.cmd == "pause":
            notes = []
            pause(a.agent, a.until, me, notes)
            print("paused: %s until %s" % (a.agent, a.until))
            for _l in notes:
                print(_l)
        elif a.cmd == "resume":
            notes = []
            resume(a.agent, me, notes)
            print("resumed: %s" % a.agent)
            for _l in notes:
                print(_l)
        elif a.cmd == "stop":
            notes = []
            stop(a.agent, a.why, me, notes)
            print("stopped: %s (finished)" % a.agent)
            for _l in notes:
                print(_l)
        elif a.cmd == "wait":
            kind, on = ("started", a.started) if a.started else (("outside", a.outside) if a.outside else ("ceo-discard", a.ceo))
            wait(a.agent, kind, on, a.todo, me, a.until)
            print("recorded: %s waits (%s) on %s%s" % (a.agent, kind, on, (" until " + a.until) if a.until else
                                                     "; nothing checkable ends it, so it is reported until it runs"))
        elif a.cmd == "hold":
            h = hold(a.name, a.what, a.until, a.why, me, a.by)
            print("held: %s%s" % (a.name, (" until " + (h.get("end") or (h.get("waiting") or {}).get("end") or ""))
                                  if a.until else "; nothing checkable ends it, so it is reported until it runs"))
        elif a.cmd == "release":
            release(a.name, a.ceo_word, a.not_a_hold, me)
            print("released: %s" % a.name)
        elif a.cmd == "holds":
            for h in holds(me):
                print("%-8s %-10s %s  since %s  end=%s (%s)  why: %s" % (
                    h["state"].upper(), h["source"], h["name"], iso(h["since"]), h["end"] or "-", h["detail"],
                    " ".join(h["why"].split())[:160]))
        elif a.cmd == "integration-branch":
            # THE ONE ANSWER, FOR A SHELL CONSUMER. "Every part of the system
            # that needs to know whether work has landed asks the same
            # question... None of them is allowed its own answer, and none of
            # them assumes main" (point 14). Exit 0 and the branch on stdout, or
            # exit 3 and the reason on stderr -- 3 means ABSTAIN, not failure:
            # the caller must not fall back to main, it must say it cannot
            # answer and name the command below.
            branch, tip, why_not = integration_for(a.repo or entity)
            if why_not:
                print(why_not, file=sys.stderr)
                return 3
            print("%s\t%s" % (branch, tip))
        elif a.cmd == "integration":
            if a.branch:
                r = record_integration(a.repo or entity, a.branch, a.why, me, a.correct)
                print("%s: %s integrates on %s (body of work %s)"
                      % ("corrected" if a.correct else "recorded", r["repo"], r["branch"], r["id"]))
            elif a.all:
                works = all_bodies_of_work()
                cur = set((_integration_file().get("current") or {}).values())
                if not works:
                    print("no body of work has an integration branch recorded (point 14)")
                for wid in sorted(works):
                    w = works[wid]
                    print("%s\t%s\t%s\t%s\t%s\t%s"
                          % (wid, "current" if wid in cur else ("teammate" if w.get("source") == "teammate"
                                                                else "superseded"), w.get("repo"),
                             w.get("branch"), w.get("recorded_at"), w.get("why") or ""))
                    for c in w.get("corrections") or []:
                        print("\t  corrected from %s at %s: %s"
                              % (c.get("from"), c.get("at"), c.get("why") or ""))
            else:
                recs = all_integration_records()
                if a.repo:
                    one = integration_record(a.repo)
                    recs = {one["repo"]: one} if one else {}
                if not recs:
                    print("no integration branch is recorded (point 14)")
                for repo in sorted(recs):
                    r = recs[repo]
                    print("%s\t%s\t%s\t%s\t%s" % (repo, r.get("branch"), r.get("source"),
                                                    r.get("recorded_at"), r.get("id") or ""))
        elif a.cmd == "retry":
            for k, ok in retry_due(budget=60.0):
                print("%s %s" % ("deleted" if ok else "still failing", k))
        elif a.cmd == "register-cc":
            register_cc(me, a.name, a.repo, a.path, a.branch, integration=a.integration)
            print("registered")
        elif a.cmd == "withdraw-cc":
            r = withdraw_cc(me, a.name, a.why)
            print("withdrawn: %s - %s" % (a.name, ", ".join(r["withdrawn"]) or "nothing was created"))
        elif a.cmd == "confirm-cc":
            confirm_cc(me, a.name, a.path, not a.failed, a.failed)
            print("confirmed" if not a.failed else "recorded failure")
        elif a.cmd == "recipient":
            st, why = recipient_state(me, a.name)
            print("%s\t%s" % (st, why))
        return 0
    except SpecError as e:
        sys.stderr.write("workspaces: REFUSED — %s\n" % e)
        # The Stop gate's exit 2 means "block the turn"; a refusal inside it
        # is an evaluation failure, which the gate reports and never blocks on.
        return 4 if a.cmd == "gate-stop" else 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

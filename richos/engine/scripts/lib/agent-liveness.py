#!/usr/bin/env python3
"""agent-liveness.py — THE ONE IMPLEMENTATION OF "IS THIS AGENT ALIVE?".

===========================================================================
WHY THIS FILE EXISTS
===========================================================================
On 2026-08-31 at ~22:45 the orchestrator told the CEO that `zach-opus-g1` was
COMPLETED. It was not. The CEO's own screen showed it working, and its
isolation-worktree lock was held the entire time.

The correct signal was present and was DISCARDED. `git worktree list` said
`locked`. `remove-agent-worktree.sh` REFUSED the removal, for exactly the right
reason, and printed that a live agent's worktree is always locked. The lead
called the lock "residue" and quoted the `ListAgents` roster instead, which said
`completed`. The roster was stale. The lock was right. The CEO had to correct
his own assistant from a screenshot.

The project doctrine already said *liveness = the worktree lock* and *never
infer an agent is dead from filesystem inactivity; require a positive
termination signal*. What it did not anticipate is the INVERSE: a
positive-looking signal that is WRONG. A stale `completed` is more dangerous
than an absent status, because absence prompts a check and a false positive
does not.

And the correct logic already existed — inside remove-agent-worktree.sh, which
got today's case right. Nothing needed inventing. What was missing is that
nothing checked the lead's STATEMENTS against it. So the logic is extracted
here, and the remover CONSUMES this file rather than carrying a second copy:
two implementations of "alive" is how one of them ends up being the stale one.

===========================================================================
THE RULE, UNCHANGED FROM THE REMOVER (this is an extraction, not a redesign)
===========================================================================
An agent is ALIVE iff its NATIVE isolation worktree, registered in the ENTITY's
main checkout, is locked with a LIVE pid.

    absent / unregistered worktree ......... NOT-ALIVE
    registered but UNLOCKED ................ NOT-ALIVE
    locked, lock line carries no pid ....... INDETERMINATE  (see below)
    locked, pid parsed, pid is dead ........ NOT-ALIVE (stale lock)
    locked, pid parsed, pid is running ..... ALIVE
    git could not be queried ............... INDETERMINATE

The third row said NOT-ALIVE until 2026-09-10, when it was measured answering
that for three RUNNING agents at once -- Claude Code's lock file is sometimes
EMPTY, and an empty lock names nobody. The row is now INDETERMINATE, which is
this file's own honest outcome and the one it promises never to collapse.

INDETERMINATE IS A REAL OUTCOME AND IS NEVER COLLAPSED. The remover treats it
as "refuse" and the claim guard treats it as "say nothing", and those are
different responses to the same honest answer — which is the point of keeping
it. A resolver that only ever says yes or no has to guess, and a guess is what
produced the defect.

===========================================================================
WHAT THE PID IN THE LOCK ACTUALLY IS — MEASURED, NOT ASSUMED
===========================================================================
Read on this machine, 2026-08-31, from four concurrent agent worktrees in
/Users/alex/ab/femcboost/.git/worktrees/*/locked:

    claude agent agent-a86f3ce81f7d36390 (pid 94086 start Mon Aug 31 19:39:29 2026)
    claude agent agent-ac6e9e5cf51a66b24 (pid 94086 start Mon Aug 31 19:39:29 2026)

The pid is the HOST SESSION's pid, and it is THE SAME for every agent of that
session. So `kill -0 <pid>` does not distinguish one agent from another; it
distinguishes "the session that owns this lock is still running" from "this
lock was left behind by a session that is gone".

That is stated here rather than glossed because it bounds what this file can
prove FROM THE LOCK. The LOCK'S PRESENCE is the per-agent signal; the pid check
is the stale-lock filter. A worktree whose agent finished but which has not
yet been reaped still reads ALIVE from the lock alone — which is why, since
round 8 (2026-09-13), `resolve()` asks the WORKSPACE REGISTRY whether the agent
is finished (point 11) before it answers, and a finished record decides
NOT-ALIVE. The claim guard built on this still REPORTS rather than BLOCKS — see
guard-agent-state-claims.py, which carries the measurement.

`pid_shared_with` is emitted in the JSON for exactly this reason: an operator
reading the evidence can see for himself that the pid is a session pid rather
than an agent pid, instead of taking this docstring's word for it.

===========================================================================
NAMING THE DISAGREEMENT IS HALF THE JOB
===========================================================================
The defect was not a missing answer. It was TWO answers and the wrong one
believed. So every verdict carries the other sources and says which of them
disagree:

  worktree-lock   Decides whether the SESSION that took the lock is still
                  running: the pid on the lock line is the session's, shared by
                  every agent of that session (measured above). It does NOT
                  decide whether an AGENT is finished. Until round 8
                  (2026-09-13) this line read "AUTHORITATIVE. The only source
                  that decides", which over-claimed: a terminal agent inside a
                  living session read ALIVE from its session's lock (round 7
                  §10, brief-audit-frank-round8 §6).
  workspace-registry
                  DECIDES "finished": mega-lander/workspaces.py finished_state,
                  the CEO's own definition (docs/plans/worktree-spec-2026-09-11.md
                  point 11 — the platform's recorded end-of-run signal, not
                  paused; or its session has ended, point 12). A record that
                  reads finished makes the verdict NOT-ALIVE whatever the lock
                  says, and the lock's answer is kept as evidence beside it.
                  An agent the registry does not know (spawned before the spec
                  build, or under another entity) falls back to the lock.
  roster          ~/.claude/teams/session-*/config.json members[].status.
                  This is the surface `ListAgents` reads. Advisory ONLY.
  worker-events   worker-events.jsonl, the SubagentStart/SubagentStop log.
                  MEASURED TRAP: `WorkerRunEnded` fires at the end of EVERY
                  subagent turn, not only at termination. On this machine one
                  session held 337 WorkerRunEnded records for 6 agents. A
                  reader who takes the last event as terminal will call a live
                  agent dead most of the time. Advisory ONLY, and labelled.

A source that says "terminal" over a held lock is not noise to be averaged in.
It is the exact signal that mattered on 2026-08-31, so it is reported by name.
"""

import argparse
import calendar
import json
import os
import re
import subprocess
import sys
import time

ALIVE = "ALIVE"
NOT_ALIVE = "NOT-ALIVE"
INDETERMINATE = "INDETERMINATE"

# `agent-<id>` as the native isolation worktree names it.
AGENT_DIR_RE = re.compile(r"^agent-[A-Za-z0-9_]+$")
LOCK_PID_RE = re.compile(r"\(pid\s+(\d+)")
LOCK_AGENT_RE = re.compile(r"claude agent (agent-[A-Za-z0-9_]+)")

# Roster status values that CLAIM the agent is finished. Kept identical in
# spirit to guard-unresolved-claims.py's TERMINAL_STATUS: this is the set whose
# disagreement with a held lock is the 2026-08-31 defect.
TERMINAL_STATUS = {
    "shutdown", "shutdown_request", "shutdown_approved", "completed",
    "complete", "done", "terminated", "dead", "exited", "killed", "removed",
    "gone",
}


# --------------------------------------------------------------------------
# git
# --------------------------------------------------------------------------

def _worktree_entries(entity_root):
    """[(path, lock_line_or_None)] or raises RuntimeError.

    The parse is deliberately the same shape as the one remove-agent-worktree.sh
    used to carry inline, because this file replaced that copy and a behavioral
    difference here would be a silent change to the removal gate.
    """
    try:
        res = subprocess.run(
            ["git", "-C", entity_root, "worktree", "list", "--porcelain"],
            capture_output=True, text=True, timeout=20,
        )
    except Exception as e:
        raise RuntimeError("git worktree list failed: %s" % e)
    if res.returncode != 0:
        raise RuntimeError("git worktree list exited %d: %s"
                           % (res.returncode, (res.stderr or "").strip()[:200]))

    entries = []
    cur_path = None
    cur_locked = None
    for line in res.stdout.splitlines():
        if line.startswith("worktree "):
            if cur_path is not None:
                entries.append((cur_path, cur_locked))
            cur_path = line[len("worktree "):]
            cur_locked = None
        elif line.startswith("locked"):
            cur_locked = line
    if cur_path is not None:
        entries.append((cur_path, cur_locked))
    return entries


def _pid_alive(pid):
    try:
        os.kill(int(pid), 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        # Running, owned by somebody else. Alive.
        return True
    except Exception:
        return None


# The lock reason records when the holder started: `(pid 94086 start Mon Aug 31
# 19:39:29 2026)`. A running pid whose REAL start time is not that one is another
# process that was handed a recycled pid (hunt part 5, P5-18), so the lock is stale.
LOCK_START_RE = re.compile(r"\(pid\s+\d+\s+start\s+([^)]+)\)")
START_TOLERANCE_SECONDS = 10


def _parse_start(text):
    """Epoch for a `Mon Aug 31 19:39:29 2026` start string, read as UTC, or None.

    UTC because the platform writes the lock's start with `TZ=UTC ps -o lstart`
    (measured 2026-09-30 on a live lock: recorded 22:12:51, local `ps` 23:12:51,
    one hour apart in BST). Reading it in local time called every live session's
    pid "reused"."""
    try:
        return float(calendar.timegm(time.strptime(text.strip(), "%a %b %d %H:%M:%S %Y")))
    except (ValueError, OverflowError):
        return None


def _pid_start_epoch(pid):
    """When the running process `pid` started (UTC, like the lock), or None."""
    try:
        res = subprocess.run(["ps", "-o", "lstart=", "-p", str(int(pid))],
                             capture_output=True, text=True, timeout=10,
                             env=dict(os.environ, LC_ALL="C", TZ="UTC"))
    except Exception:
        return None
    if res.returncode != 0:
        return None
    return _parse_start(res.stdout)


def _pid_reused(pid, lock_line):
    """True only when BOTH start times are readable and differ: reuse is then
    proven. Anything unreadable on either side is None -- it proves nothing, and
    the caller keeps the plain running-pid answer."""
    m = LOCK_START_RE.search(lock_line or "")
    recorded = _parse_start(m.group(1)) if m else None
    actual = _pid_start_epoch(pid)
    if recorded is None or actual is None:
        return None
    return abs(recorded - actual) > START_TOLERANCE_SECONDS


# --------------------------------------------------------------------------
# other sources -- advisory, reported, never decisive
# --------------------------------------------------------------------------

def teams_dir():
    return (os.environ.get("RICHOS_LIVENESS_TEAMS_DIR")
            or os.path.expanduser("~/.claude/teams"))


def _roster_says(agent_id):
    """What every session roster on this machine says about this agent id.

    Returns [] when no roster mentions it -- which is itself worth knowing, and
    is reported as such rather than silently read as "not alive".
    """
    out = []
    td = teams_dir()
    if not os.path.isdir(td):
        return out
    try:
        sessions = sorted(os.listdir(td))
    except OSError:
        return out
    for s in sessions:
        cfg = os.path.join(td, s, "config.json")
        try:
            with open(cfg, encoding="utf-8") as f:
                members = json.load(f).get("members") or []
        except Exception:
            continue
        for m in members:
            if not isinstance(m, dict):
                continue
            aid = str(m.get("agentId") or "")
            cwd = str(m.get("cwd") or "")
            if agent_id not in aid and not cwd.rstrip("/").endswith("/" + agent_id):
                continue
            out.append({
                "session": s,
                "name": m.get("name"),
                "agentType": m.get("agentType"),
                "status": str(m.get("status") or "") or None,
            })
    return out


def _worker_events_say(agent_id):
    """Last lifecycle event for this agent id, plus the count, plus the trap.

    `WorkerRunEnded` fires on EVERY SubagentStop, which is every turn the agent
    takes -- not its termination. So the last event being `run_ended` means
    almost nothing on its own, and this dict says so in `terminal_looking`
    rather than letting a caller infer it.
    """
    td = teams_dir()
    last = None
    n = 0
    if not os.path.isdir(td):
        return None
    try:
        sessions = sorted(os.listdir(td))
    except OSError:
        return None
    for s in sessions:
        p = os.path.join(td, s, "worker-events.jsonl")
        try:
            with open(p, encoding="utf-8") as f:
                for line in f:
                    if agent_id not in line:
                        continue
                    try:
                        d = json.loads(line)
                    except Exception:
                        continue
                    if str(d.get("agent_id") or "") != agent_id:
                        continue
                    n += 1
                    last = d
        except Exception:
            continue
    if last is None:
        return None
    ev = str(last.get("event") or "")
    return {
        "last_event": ev,
        "last_timestamp": last.get("timestamp"),
        "events_seen": n,
        "agent_type": last.get("agent_type") or None,
        "terminal_looking": ev in ("WorkerRunEnded", "WorkerEnded"),
        "caveat": ("WorkerRunEnded fires at the end of EVERY subagent turn, "
                   "not only at termination -- it is not a termination signal"),
    }


# --------------------------------------------------------------------------
# name -> agent id, from the orchestrator's own transcript
# --------------------------------------------------------------------------

def agent_spawns(transcript_path, limit_bytes=64 * 1024 * 1024):
    """[(tool_use_id, name_or_"", agent_id)] — every Agent call in a session
    transcript that the harness answered with an agent id.

    THE MAPPING IS NOT ON DISK ANYWHERE ELSE, and this is why the guard cannot
    work from `spawned-names.log` alone. Verified on a real transcript
    (session 042f3850, 2026-08-31):

      * the assistant record carries a tool_use named `Agent` whose input has
        the teammate's `name` ("zach-opus-g1") and a tool_use `id`
      * the following user record carries a tool_result with the SAME
        tool_use_id, and a `toolUseResult` object holding `agentId`

    So the join is on tool_use_id and it is exact. Nothing here guesses. This
    is THE ONE parser of that join; names_to_ids() and tool_use_ids_to_agent_ids()
    are two views of its output, and a third consumer must be a third view,
    never a third parser.
    """
    out = []
    if not transcript_path or not os.path.isfile(transcript_path):
        return out
    pending = {}     # tool_use_id -> name ("" when the spawn carried none)
    try:
        size = os.path.getsize(transcript_path)
    except OSError:
        return out
    try:
        with open(transcript_path, encoding="utf-8", errors="replace") as f:
            if size > limit_bytes:
                f.seek(size - limit_bytes)
                f.readline()
            for line in f:
                if '"Agent"' not in line and '"agentId"' not in line:
                    continue
                try:
                    d = json.loads(line)
                except Exception:
                    continue
                msg = d.get("message") or {}
                content = msg.get("content")
                if isinstance(content, list):
                    for c in content:
                        if not isinstance(c, dict):
                            continue
                        if c.get("type") == "tool_use" and c.get("name") == "Agent":
                            inp = c.get("input") or {}
                            nm = inp.get("name")
                            if c.get("id"):
                                pending[c["id"]] = nm.strip() if isinstance(nm, str) else ""
                        elif c.get("type") == "tool_result" and c.get("tool_use_id"):
                            tur = d.get("toolUseResult")
                            if isinstance(tur, dict) and tur.get("agentId"):
                                tuid = c["tool_use_id"]
                                if tuid in pending:
                                    out.append((tuid, pending[tuid], str(tur["agentId"])))
    except Exception:
        return out
    return out


def names_to_ids(transcript_path, limit_bytes=64 * 1024 * 1024):
    """{'zach-opus-g1': 'a86f3ce81f7d36390', ...} from a session transcript.

    A name that cannot be joined is simply absent from the result -- the caller
    then has no agent to check, which is the honest answer rather than a
    role-prefix match that would be ambiguous the moment two teammates of the
    same role run at once (three did, on the day of the defect).
    """
    out = {}
    for _tuid, nm, aid in agent_spawns(transcript_path, limit_bytes):
        if nm:
            out[nm] = aid
    return out


def tool_use_ids_to_agent_ids(transcript_path, limit_bytes=64 * 1024 * 1024):
    """{'toolu_01...': 'a86f3ce81f7d36390', ...} — the join keyed the way the
    worktree lifecycle keys it: by the exact tool_use_id the PreToolUse[Agent]
    spawn-intent was written under (worktree-transactions.py). Exact, and
    name-free: a spawn with no name still binds."""
    out = {}
    for tuid, _nm, aid in agent_spawns(transcript_path, limit_bytes):
        out[tuid] = aid
    return out


# --------------------------------------------------------------------------
# the resolution
# --------------------------------------------------------------------------

def _normalize(target):
    """A bare id, an `agent-<id>`, or a worktree path -> (agent_dirname, path_hint)."""
    t = (target or "").strip().rstrip("/")
    if not t:
        return None, None
    if "/" in t:
        return os.path.basename(t), t
    if t.startswith("agent-"):
        return t, None
    return "agent-" + t, None


def _registry_says(agent_id):
    """What the workspace registry (mega-lander/workspaces.py, the spec's own
    store) records for this agent id: {"finished", "paused", "why", "name"},
    or None when it has no record of the id or cannot be read. Read-only.

    This is the reader item 6 of round 8 added: the lock proves the SESSION is
    alive, the registry proves the AGENT is finished (point 11), and the two
    are different questions."""
    if not agent_id:
        return None
    try:
        import importlib.util
        lib = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "mega-lander", "workspaces.py")
        if not os.path.isfile(lib):
            return None
        spec = importlib.util.spec_from_file_location("al_workspaces", lib)
        ws = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(ws)
        key = ws.key_for_id(agent_id)
        if not key:
            return None
        rec = ws.load_agent(key) or ws.read_json(ws.done_path(key))
        if not rec:
            return None
        fin, paused, why = ws.finished_state(rec)
        return {"finished": bool(fin), "paused": bool(paused), "why": why,
                "name": rec.get("name") or "", "key": key}
    except Exception:
        return None


def resolve(entity_root, target):
    """One verdict, with its evidence and its disagreements.

    Two sources, two questions. The LOCK answers "is the session that took this
    lock running?"; the WORKSPACE REGISTRY answers "is this agent finished?"
    (point 11). A registry record that reads finished decides NOT-ALIVE, and the
    lock's own answer is kept in the evidence so an operator can see both."""
    rec = _lock_resolve(entity_root, target)
    reg = _registry_says(rec.get("agent_id") or "")
    rec.setdefault("sources", {})
    rec["sources"]["workspace-registry"] = {
        "authoritative": bool(reg),
        "decides": "whether the AGENT is finished (docs/plans/worktree-spec-2026-09-11.md, point 11)",
        "says": (("FINISHED" if reg["finished"] else ("PAUSED" if reg["paused"] else "RUNNING"))
                 if reg else "no record of this agent"),
        "detail": (reg["why"] if reg else "mega-lander/workspaces.py has no record for this id; "
                                          "the lock alone answers"),
    }
    if reg:
        rec["evidence"]["registry_finished"] = reg["finished"]
        rec["evidence"]["registry_why"] = reg["why"]
    # A lock that names nobody (held, no pid) leaves the verdict INDETERMINATE; the
    # registry is authoritative for "finished", so it settles that case too (hunt
    # part 5, P5-19). An empty lock ALONE still proves nothing -- live agents have had
    # empty locks -- which is why only a FINISHED record changes the answer.
    lock_names_nobody = (rec["verdict"] == INDETERMINATE and rec["evidence"].get("locked")
                         and "pid" not in rec["evidence"])
    if reg and reg["finished"] and (rec["verdict"] == ALIVE or lock_names_nobody):
        was = rec["verdict"]
        rec["evidence"]["lock_verdict"] = was
        rec["evidence"]["lock_reason"] = rec["reason"]
        rec["verdict"] = NOT_ALIVE
        rec["reason"] = ("the workspace registry records %s as FINISHED — %s (point 11). Its "
                         "worktree is still locked because the lock is the SESSION's (pid %s), not "
                         "the agent's; the lock proves the session runs, not that this agent does"
                         % (reg["name"] or rec.get("agent_id"), reg["why"],
                            rec["evidence"].get("pid", "unrecorded")))
    elif reg and not reg["finished"] and rec["verdict"] == NOT_ALIVE \
            and not rec["evidence"].get("registered"):
        # A WORKER WITH NO ISOLATION WORKTREE AT ALL (2026-10-29 08:31Z). An app worker, and any
        # teammate given a cc/ workspace, runs in a registered workspace, never in the entity's
        # .claude/worktrees/agent-<id>, so there is no lock to read: "no registered entity
        # worktree" is the absence of a lock, not the end of a run. The registry is the one
        # source that knows such an agent, and it decides "finished" (point 11), its session's
        # liveness included (point 12): not finished is a running agent. (echo-opus-ff00122b4923
        # was reported NOT-ALIVE while it wrote 3.79 MB of transcript that second.)
        rec["evidence"]["lock_verdict"] = rec["verdict"]
        rec["evidence"]["lock_reason"] = rec["reason"]
        rec["verdict"] = ALIVE
        rec["reason"] = ("the workspace registry records %s as not finished (%s)%s; it has no isolation "
                         "worktree to lock (a registered cc/ workspace), so the registry alone decides"
                         % (reg["name"] or rec.get("agent_id"), reg["why"],
                            ", paused by its lead" if reg["paused"] else ""))
    elif reg and not reg["finished"] and rec["verdict"] == NOT_ALIVE:
        rec.setdefault("disagreements", []).append(
            "the workspace registry records %s as not finished (%s) while the lock reads NOT-ALIVE "
            "(%s). The registry decides 'finished'; the lock decides whether the session runs -- "
            "an unfinished agent whose session is gone is finished by point 12, so ask the registry "
            "again after its session's end is recorded." % (reg["name"] or rec.get("agent_id"), reg["why"], rec["reason"]))
    return rec


def _lock_resolve(entity_root, target):
    """The LOCK's half: what the isolation-worktree lock says about the session."""
    agent_dir, path_hint = _normalize(target)
    rec = {
        "target": target,
        "agent_dir": agent_dir,
        "agent_id": agent_dir[len("agent-"):] if agent_dir else None,
        "entity_root": entity_root,
        "verdict": INDETERMINATE,
        "reason": "",
        "evidence": {},
        "sources": {},
        "disagreements": [],
    }
    if not agent_dir:
        rec["reason"] = "no agent id or worktree path given"
        return rec

    try:
        entries = _worktree_entries(entity_root)
    except RuntimeError as e:
        rec["verdict"] = INDETERMINATE
        rec["reason"] = str(e)
        rec["evidence"] = {"git_query": "failed"}
        return rec

    match = None
    for path, locked in entries:
        base = path.rstrip("/").split("/")[-1]
        if base == agent_dir or (locked and agent_dir in locked) or (
                path_hint and path.rstrip("/") == path_hint):
            match = (path, locked)
            break

    ev = rec["evidence"]
    ev["registered"] = match is not None
    if match is None:
        ev["worktree_path"] = path_hint
        ev["present_on_disk"] = bool(path_hint and os.path.isdir(path_hint))
        ev["locked"] = False
        rec["verdict"] = NOT_ALIVE
        rec["reason"] = ("no registered entity worktree for %s in %s "
                         "(absent/unregistered)" % (agent_dir, entity_root))
        _attach_sources(rec)
        return rec

    path, locked = match
    ev["worktree_path"] = path
    ev["present_on_disk"] = os.path.isdir(path)
    ev["locked"] = bool(locked)
    ev["lock_line"] = locked

    if not locked:
        rec["verdict"] = NOT_ALIVE
        rec["reason"] = ("entity worktree %s is present but UNLOCKED -- a live "
                         "agent isolation worktree is always locked" % path)
        _attach_sources(rec)
        return rec

    m = LOCK_PID_RE.search(locked)
    if not m:
        # MEASURED 2026-09-10, and it INVERTED this verdict. At 11:25 that day
        # every RUNNING agent on this machine reached exactly this branch and
        # was answered NOT-ALIVE: three live isolation worktrees, present,
        # registered and locked, whose lock line was the bare word `locked`.
        #
        # The parser is not at fault -- git 2.52.0 reports a lock reason
        # faithfully in --porcelain and in --porcelain -z, for a lock taken by
        # `git worktree lock --reason` and for a lock file written directly
        # (both measured in a sandbox the same hour). Nor was the platform:
        # the orchestrator had unlocked all three at 11:24:02 and re-locked
        # them at 11:24:11 (a 20ms poller caught both edges), and
        # `git worktree lock` WITHOUT --reason writes an EMPTY reason file,
        # destroying the platform's own
        #     claude agent agent-a42c90096292136f0 (pid 8799 start Thu Sep 10 ...)
        #
        # THE CAUSE DOES NOT MATTER TO THIS BRANCH, and that is the point. Any
        # hand, any tool, any future platform revision can leave a lock with no
        # pid on it, and this verdict is what decides whether a workspace may
        # be destroyed.
        #
        # A HELD LOCK IS THE PLATFORM SAYING THE WORKSPACE MAY STILL BE IN USE.
        # With no pid on it, this file cannot tell whether the holder runs --
        # and INDETERMINATE is the honest answer this module already keeps for
        # exactly that, "never collapsed" by its own rule. Answering NOT-ALIVE
        # made the one implementation of "is this agent alive?" PERMISSIVE for
        # every live agent, which is worse than absent: absence prompts a
        # check and a false negative does not. The remover refuses on
        # INDETERMINATE, the claim guard says nothing, and the reclaim lane has
        # its own positive-evidence route for a lock the platform never
        # releases (daily-workspace-cleanup.py, the agent ground).
        rec["verdict"] = INDETERMINATE
        rec["reason"] = ("entity worktree %s is LOCKED and the lock line carries no pid "
                         "(%r) -- the platform is holding it and this cannot say who; "
                         "an empty lock reason has been observed on RUNNING agents "
                         "(2026-09-10)" % (path, locked))
        _attach_sources(rec)
        return rec

    pid = int(m.group(1))
    ev["pid"] = pid
    shared = sum(1 for _p, _l in entries
                 if _l and LOCK_PID_RE.search(_l)
                 and int(LOCK_PID_RE.search(_l).group(1)) == pid)
    # See the module docstring: the lock pid is the HOST SESSION pid, so this is
    # routinely > 1 and that is not an anomaly. It is emitted so an operator can
    # see the pid is not per-agent instead of assuming it is.
    ev["pid_shared_with"] = shared - 1
    alive = _pid_alive(pid)
    ev["pid_alive"] = alive
    if alive:
        reused = _pid_reused(pid, locked)
        ev["pid_reused"] = reused
        if reused:
            alive = False

    if alive is None:
        rec["verdict"] = INDETERMINATE
        rec["reason"] = ("worktree %s is locked by pid %d and that pid could not "
                         "be probed" % (path, pid))
    elif alive:
        rec["verdict"] = ALIVE
        rec["reason"] = ("isolation worktree %s is LOCKED and the locking pid %d "
                         "is running" % (path, pid))
    else:
        rec["verdict"] = NOT_ALIVE
        if ev.get("pid_reused"):
            rec["reason"] = ("entity worktree %s carries a STALE lock (pid %d is running, but it "
                             "started at a different time than the lock recorded, so that pid was "
                             "reused by another process)" % (path, pid))
        else:
            rec["reason"] = ("entity worktree %s carries a STALE lock (pid %d is "
                             "dead)" % (path, pid))
    _attach_sources(rec)
    return rec


def _attach_sources(rec):
    """The advisory surfaces, and every place they contradict the lock."""
    aid = rec.get("agent_id") or ""
    if not aid:
        return
    roster = _roster_says(aid)
    events = _worker_events_say(aid)
    rec["sources"] = {
        "worktree-lock": {
            "authoritative": True,
            "decides": "whether the SESSION that took the lock is running (the pid is the session's, "
                       "shared by every agent of that session) -- never whether an agent is finished",
            "says": rec["verdict"],
            "detail": rec["reason"],
        },
        "roster": {
            "authoritative": False,
            "entries": roster,
            "detail": ("~/.claude/teams/*/config.json -- the surface ListAgents "
                       "reads. Advisory: it is not refreshed from the lock."),
        },
        "worker-events": {
            "authoritative": False,
            "entry": events,
            "detail": ("worker-events.jsonl. Advisory: WorkerRunEnded fires at "
                       "every subagent turn end, not only at termination."),
        },
    }

    dis = rec["disagreements"]
    if rec["verdict"] == ALIVE:
        for r in roster:
            st = (r.get("status") or "").lower()
            if st in TERMINAL_STATUS:
                dis.append(
                    "roster (session %s) says status=%r for %s while the lock is "
                    "HELD -- this is the 2026-08-31 shape: believe the lock."
                    % (r["session"], r.get("status"), r.get("name") or aid))
        if events and events.get("terminal_looking"):
            dis.append(
                "worker-events last record is %s (%s) while the lock is HELD. "
                "That event fires at every turn end, so it is not a termination "
                "signal -- believe the lock."
                % (events["last_event"], events.get("last_timestamp")))
    if rec["verdict"] == NOT_ALIVE and roster:
        for r in roster:
            st = (r.get("status") or "").lower()
            if st and st not in TERMINAL_STATUS:
                dis.append(
                    "roster (session %s) says status=%r for %s while the lock is "
                    "NOT held. The lock decides; the roster is stale in the other "
                    "direction." % (r["session"], r.get("status"), r.get("name") or aid))


def enumerate_all(entity_root):
    """Every agent worktree registered in the entity -- the sweep the lead owes
    himself before saying anything about an agent's state."""
    try:
        entries = _worktree_entries(entity_root)
    except RuntimeError as e:
        return [{"target": None, "verdict": INDETERMINATE, "reason": str(e),
                 "entity_root": entity_root, "evidence": {}, "sources": {},
                 "disagreements": []}]
    out = []
    for path, locked in entries:
        base = path.rstrip("/").split("/")[-1]
        if not AGENT_DIR_RE.match(base):
            if not (locked and LOCK_AGENT_RE.search(locked)):
                continue
            base = LOCK_AGENT_RE.search(locked).group(1)
        out.append(resolve(entity_root, path if base == path.rstrip("/").split("/")[-1] else base))
    return out


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------

def _triple(rec):
    """The line format remove-agent-worktree.sh consumes.

    THREE FIELDS, TAB SEPARATED, one line:
        ALIVE\t<pid>\t<worktree-path>
        NOT-ALIVE\t<reason>\t<worktree-path or empty>
        INDETERMINATE\t<reason>\t
    """
    v = rec["verdict"]
    if v == ALIVE:
        return "%s\t%s\t%s" % (ALIVE, rec["evidence"].get("pid", ""),
                               rec["evidence"].get("worktree_path", ""))
    reason = (rec.get("reason") or "").replace("\t", " ").replace("\n", " ")
    if v == NOT_ALIVE:
        return "%s\t%s\t%s" % (NOT_ALIVE, reason,
                               rec["evidence"].get("worktree_path") or "")
    return "%s\t%s\t" % (INDETERMINATE, reason)


def main(argv=None):
    ap = argparse.ArgumentParser(add_help=True)
    ap.add_argument("--entity", required=True)
    ap.add_argument("--owner", default=None,
                    help="agent id, agent-<id>, or a worktree path")
    ap.add_argument("--all", action="store_true")
    ap.add_argument("--transcript", default=None,
                    help="a session transcript; attaches teammate NAMES to ids")
    ap.add_argument("--format", choices=("triple", "json"), default="json")
    args = ap.parse_args(argv)

    if args.all or not args.owner:
        recs = enumerate_all(args.entity)
    else:
        recs = [resolve(args.entity, args.owner)]

    if args.transcript:
        n2i = names_to_ids(args.transcript)
        i2n = {}
        for n, i in n2i.items():
            i2n.setdefault(i, []).append(n)
        for r in recs:
            r["names"] = sorted(i2n.get(r.get("agent_id") or "", []))

    if args.format == "triple":
        for r in recs:
            print(_triple(r))
        return 0
    print(json.dumps(recs if (args.all or not args.owner) else recs[0], indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())

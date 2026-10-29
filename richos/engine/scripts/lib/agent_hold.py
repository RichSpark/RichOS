#!/usr/bin/env python3
"""Hold an agent's running commands when it is paused; continue them on RESUME.

A pause is a message, and a message reaches an agent only at its next tool call,
so the command it is already running keeps the CPU until it ends. This module
makes the pause take effect at once: it suspends (SIGSTOP) exactly the processes
the paused agent owns and continues (SIGCONT) exactly those on RESUME. Nothing is
terminated, so nothing is lost. Measured on Claude Code 2.1.283: a suspended
foreground command that passes its Bash timeout is moved to the background, not
signaled, and finishes normally once continued.

OWNERSHIP IS CAPTURED AT SPAWN, BY THE PROCESS ITSELF. Every Bash call made by a
subagent is rewritten (shell-evidence.py calls rewrite()) to record its shell
through a literal helper call keyed by session, agent and tool call, and to export an
inherited owner tag. A process is owned only when it is alive, still has its
recorded parent and started between the hook and the record's write. A reused PID
fails that test. Names, paths and working directories never choose a process.

THE OWNED TREE is those shells, their descendants, the members of process groups
an owned process leads, and processes carrying this agent's own inherited tag
(detached and reparented children). Never pid 1, never the session process, never
this process or its ancestors. A proc_tree.py supervisor inside the tree IS held
with its check (it registers as a cpu_guard "session" root, which protects it from
cpu_guard's CPU kills, not from a pause): left running, it watched a frozen check
and its bounds expired (Sage's catch 2).

SHARED DAEMONS ARE LEFT RUNNING. A tagged process that was reparented to launchd
and leads its own session is a daemon the agent started for everyone (an adb
server, a Gradle daemon): other agents' work talks to it, so freezing it would
hold their work too. It is named in the report. (Sage's catch 5.)

THE TEST VM GUEST IS LEFT RUNNING. A walk that holds a guest slot (its holder
record is validated against the holder's start time) keeps running with
everything under it, and so does every recorded VM process and the run's login
keeper, which run.sh detaches (so neither is under the walk by parent link).
Freezing them would keep a shared slot locked for the whole pause and fail the
walk's own cleanup. The walk ends by itself and releases its slot; its boot,
push and cleanup steps are bounded (480, 120 and 120 s) but its scenario step is
not, and during a quota pause the guest keeps using the quota until it ends.
(Sage's catches 4 and 7.)

NOTHING STAYS FROZEN AFTER ITS LEAD. Every hold starts a watchdog, a detached
process outside every agent's tree. It releases the hold on its own when every
recorded session process is gone (a lead that crashed without SessionEnd: each
Bash shell is a session leader under `claude`, so the kernel never continues its
stopped group) or when the registry records the agent finished (a stop made from
the CEO's screen, which runs no hook). (Sage's catches 6 and 1.)

A PAUSED AGENT WAITS INSIDE ITS RUN. A background subagent that ends its turn
ends its run, and a later message starts a new run (Rich, brief addition 1). The
generated WAIT therefore tells it to run `agent_hold.py wait`, which returns
RESUMED only once the hold is released.

A HELD AGENT MAKES NO MODEL CALL WHILE IT WAITS (2026-10-04). Every return of a
tool call is a model call that reads the agent's whole context. The wait used to
return STILL WAITING every 270 s to keep the prompt cache warm, and each held
agent re-ran it: on 2026-09-29 fifteen held Fable workers made 44 such calls in
17 minutes, reading 22.0 million context tokens, after the five-hour quota had
switched to the API. Now gate(), run first by the PreToolUse[Bash] rewriter
(shell-evidence.py), holds a held agent's wait call before it starts, for as
long as the hold stands, once that agent has been handed this hold's WAIT
notice; the model is not called until the release lets the call run and print
RESUMED. Measured on Claude Code 2.1.288: a PreToolUse hook held a subagent's
tool call for 700 s with no model call, and a test agent held 5 minutes made
none until its release. The bound is that hook's timeout in hooks.json (86400 s;
not measured beyond 700 s): when it expires the call runs, returns STILL WAITING
once and the next wait is held again.

NATIVE TASKS KEEP THE TOOL ROUND FREE (2026-09-28). The previous foreground
wrapper used trap, $$ and a subshell job. Claude Code 2.1.283 rejects those
constructs for worktree-isolated agents. New calls retain the original command
at top level after a literal helper invocation and owner exports. The helper
records its own parent; it never reads or executes the user's command.

Ordinary CLI subagent Bash calls get six seconds to deliver their result in the
foreground by default, then use the harness's automatic native background
handoff. Installed acceptance passed on Claude Code 2.1.289. SDK callers and
explicit background calls keep their existing path. RICHOS_AGENT_BASH_FOREGROUND=0
restores forced background execution. This is a handoff grace, not a process
execution deadline; the host's own background limits still apply. A directly
delivered result needs
no collector call and is marked consumed at the next tool boundary.
The agent waits with this module's foreground `wait` command, which blocks in the
kernel until a hold appears, a running shell exits or the host appends a result
(2026-10-28: it used to look every half second, at about 75% of a core). A hold freezes the native task's entire owned tree;
the wait returns promptly so the queued WAIT message reaches the agent. The next
wait stays inside the same run until release, returning at most every 270 s to
retain the prompt cache. Native task completion retains the original output and
exit status. The wait binds the exact tool result in this agent's transcript to
the host's output file and returns its output/status, so no extra Read is needed
for ordinary results. A finished call's result is returned as soon as it is
ready, even while another of the agent's calls is still running; that call is
named as not finished and a later wait returns it (2026-10-04: one long test
run held every later wait at its bound, about 4.5 min per step).
A nonzero task makes that wait nonzero too. A missing
native result is explicitly unavailable, never inferred to be successful.

A held agent's new Bash calls are denied by the hook. The mark helper checks the
hold again at execution to close the hook-to-spawn race. Wait calls are exempt
and forced into the foreground. Legacy wrappers already in flight still use
_detach/_collect and keep their frozen results until collected.

AN ENDED RUN LEAVES NOTHING OF ITS OWN RUNNING (2026-10-11). A pause never ends
anything; the end of an agent's run does. end_owned(), called wherever the
registry records an agent's end, ends the commands it still owns through this
same ownership record and owned tree, with the same exclusions, and logs them to
ended.log. `agent_hold.py end` does it by hand (see the section below).

KNOWN LIMITS: held work keeps its file locks (the hold report lists the lock
files it has open: a Cargo target lock, a worker token, a git index lock), and a
command started before capture was installed has no record and is not held (and
one started under the previous capture is frozen whole, round included). Timers
inside held work keep counting unless they use proc_tree.HeldClock. A wrapped
foreground command's output reaches the harness when it ends, not line by line,
and stdout and stderr arrive as one stream. Non-Bash tools are not refused while
held; the WAIT reaches the agent at their return anyway.
"""
import argparse
import calendar
import ctypes
import json
import os
import re
import shlex
import signal
import stat
import struct
import subprocess
import sys
import time

TAG = "RICHOS_AGENT_OWNER"
SESSION_TAG = "RICHOS_AGENT_SESSION"
ID = re.compile(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,127}\Z")
BIRTH_SLACK = 1.0          # `ps -o lstart` has one-second resolution
PRUNE_AFTER = 60.0         # a finished call's record is removed after this long
UNSTARTED_AFTER = 3600.0   # a call that never wrote its PID (refused, sandboxed)
STOP_ROUNDS = 10
SETTLE_SECONDS = 2.0
RELEASE_SCANS = 4
RELEASE_SCAN_GAP = 0.25
SLOT_FILES = ("guest.lock", "guest-2.lock")
DETACH_GRACE = 1.5         # a foreground call caught between its hold check and its job's start
DETACHED_KEEP = 86400.0    # an uncollected frozen command's output is kept this long after it ends
REFUSED_EXIT = 75          # EX_TEMPFAIL: the command did not run; run it again after RESUME

# The one command a held agent runs. Same text as the generated WAIT (pause_protocol.py).
WAIT_COMMAND = "python3 " + shlex.quote(os.path.realpath(__file__)) + " wait"
TOOL_MAX_TIMEOUT_MS = 600000   # the Bash tool's ceiling unless BASH_MAX_TIMEOUT_MS says otherwise
WAIT_MARGIN_SECONDS = 15       # the wait returns this long before its call's timeout
# Each STILL WAITING / STILL RUNNING is one model turn. Measured 2026-09-28: after a 25 s gap the
# turn read the agent's whole context from the prompt cache; after 585 s the conversation part had
# expired and was written again. Returning inside the cache's lifetime keeps each turn a cache read.
# This bound now applies only to an agent that is NOT held (waiting for its own running task): a
# held agent's wait is held by gate() before it starts and makes no model call until its release.
WAIT_CACHE_SECONDS = 270
FOREGROUND_GRACE_MS = 6000
FOREGROUND_DELIVERED = object()  # Host already delivered the result; no invented exit code.
# Every wait below blocks in the kernel on what it waits for (quiet_wait.py: a process's exit,
# a change in the hold or call-record directory, an append to the host's transcript or output
# file) and looks again only when one happens. This is the longest it goes without looking
# when nothing woke it: a safety net for an event no watch covers, never the way it notices.
# 2026-10-28: the wait's 0.05 s and 0.5 s loops, each re-reading up to 4 MB of transcript per
# call record, held about 4.6 of the Mac's 10 cores while ten agents only waited.
WAIT_RECHECK_SECONDS = 30.0
GATE_POLL_SECONDS = WAIT_RECHECK_SECONDS
HOW_TO_WAIT = ("To wait, run this command with the Bash tool's timeout input set to 600000 (do not type a shell timeout prefix), and run it again each time it prints "
               "STILL WAITING: " + WAIT_COMMAND)
REFUSED_TEXT = ("WAIT: the orchestrator has told you to wait, so this command did not run. Run it again after "
                "you are resumed. " + HOW_TO_WAIT)
DETACHED_TEXT = ("WAIT: the orchestrator has told you to wait while this command was running. The command is "
                 "frozen, not ended: it continues from the same point when you are resumed. " + HOW_TO_WAIT
                 + ". When you are resumed, the wait prints this command's output and exit status.")


class _SleepWatch:
    """quiet_wait.Watch's interface when quiet_wait.py is not beside this file (a lone copy):
    it notices nothing and sleeps at most a second at a time, still about 0% CPU."""

    def __init__(self, paths=(), pids=()):
        pass

    def __enter__(self):
        return self

    def __exit__(self, *_exc):
        return False

    def add_paths(self, paths):
        pass

    def add_pids(self, pids):
        pass

    def wait(self, seconds):
        time.sleep(max(0.0, min(float(seconds), 1.0)))
        return False


_QUIET = []


def _watch(paths=(), pids=()):
    """A quiet_wait.Watch, loaded by fixed path from beside this file (realpath)."""
    if not _QUIET:
        here = os.path.join(os.path.dirname(os.path.realpath(__file__)), "quiet_wait.py")
        try:
            import importlib.util
            spec = importlib.util.spec_from_file_location("richos_quiet_wait", here)
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            _QUIET.append(module.Watch)
        except (OSError, ImportError, AttributeError):
            _QUIET.append(_SleepWatch)
    return _QUIET[0](paths, pids)


# ---------------------------------------------------------------------------
# state
# ---------------------------------------------------------------------------

def state_dir():
    d = (os.environ.get("RICHOS_AGENT_HOLD_DIR") or "").strip()
    if d:
        return d
    base = (os.environ.get("CLAUDE_CONFIG_DIR") or "").strip() or os.path.join(os.path.expanduser("~"), ".claude")
    return os.path.join(base, "state", "agent-hold")


def _shell_dir(session_id, agent_id):
    return os.path.join(state_dir(), "shells", session_id, agent_id)


def _held_path(session_id, agent_id):
    return os.path.join(state_dir(), "held", "%s__%s.json" % (session_id, agent_id))


def _write_json(path, value):
    os.makedirs(os.path.dirname(path), mode=0o700, exist_ok=True)
    tmp = "%s.%d.new" % (path, os.getpid())
    with open(tmp, "w") as out:
        json.dump(value, out, sort_keys=True)
    os.replace(tmp, path)


def _read_json(path):
    try:
        with open(path) as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def _valid_ids(*values):
    return all(isinstance(v, str) and ID.fullmatch(v) for v in values)


# ---------------------------------------------------------------------------
# capture: called by the PreToolUse[Bash] rewriter for a subagent's call
# ---------------------------------------------------------------------------

def _alive(pid):
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


CALL_FILES = (".json", ".pid", ".job", ".out", ".status", ".detached")


def _unlink_call(stem):
    for suffix in CALL_FILES:
        try:
            os.unlink(stem + suffix)
        except OSError:
            pass


def _prune(directory, now):
    """Remove records of calls that ended; retain ownership of tagged descendants.

    A frozen command's output is kept until the agent's wait collects it, or for
    DETACHED_KEEP after its job is gone when nobody ever does."""
    try:
        names = os.listdir(directory)
    except OSError:
        return
    prune_table = None
    for name in names:
        if not name.endswith(".json"):
            continue
        stem = os.path.join(directory, name[:-5])
        try:
            age = now - os.stat(stem + ".json").st_mtime
        except OSError:
            continue
        detached = _read_json(stem + ".detached")
        if detached is not None:
            job = detached.get("job")
            if age > DETACHED_KEEP and not (isinstance(job, int) and _alive(job)):
                _unlink_call(stem)
            continue
        pid = _read_pid(stem + ".pid")
        if pid is None:
            dead = age > UNSTARTED_AFTER
        else:
            dead = age > PRUNE_AFTER and not _alive(pid[0])
        if dead:
            meta = _read_json(stem + ".json") or {}
            if meta.get("result_source") and not meta.get("result_collected") and age < DETACHED_KEEP:
                continue
            if meta.get("identity"):
                if prune_table is None:
                    prune_table = snapshot()
                session, agent = os.path.basename(os.path.dirname(directory)), os.path.basename(directory)
                if tagged(agent, prune_table, meta["at"], session):
                    continue
            _unlink_call(stem)


def _wait_bound_seconds():
    """How long `wait` may block: just under the Bash timeout its call is given."""
    ceiling = TOOL_MAX_TIMEOUT_MS
    try:
        declared = int(os.environ.get("BASH_MAX_TIMEOUT_MS") or 0)
        if 0 < declared < ceiling:
            ceiling = declared
    except ValueError:
        pass
    bound = max(5, min(WAIT_CACHE_SECONDS, ceiling // 1000 - WAIT_MARGIN_SECONDS))
    try:
        # A declared seam: a shorter bound, so a test can watch a STILL WAITING cycle.
        asked = float(os.environ.get("RICHOS_AGENT_HOLD_WAIT_SECONDS") or 0)
        if 0 < asked < bound:
            bound = asked
    except ValueError:
        pass
    return ceiling, bound


def _wait_args(command):
    if not isinstance(command, str):
        return None
    try:
        args = shlex.split(command)
    except ValueError:
        return None
    if args[:1] == ["python3"]:
        args = args[1:]
    if not args or os.path.basename(args.pop(0)) != "agent_hold.py":
        return None
    if args[-1:] == ["2>&1"]:
        args = args[:-1]
    if args == ["wait"] or (len(args) == 3 and args[:2] == ["wait", "--max-seconds"]
                            and re.fullmatch(r"\d+(?:\.\d+)?", args[2])):
        return args
    return None


def is_wait_call(command):
    return _wait_args(command) is not None


def _record(payload, mode, command, foreground=False):
    """Writes the call's record; returns (pid path, hold path) or None when it is not a subagent's call."""
    agent_id = payload.get("agent_id")
    session_id = payload.get("session_id")
    tool_use_id = payload.get("tool_use_id")
    if not _valid_ids(agent_id, session_id, tool_use_id):
        return None
    directory = _shell_dir(session_id, agent_id)
    now = time.time()
    try:
        os.makedirs(directory, mode=0o700, exist_ok=True)
        consume_foreground(session_id, agent_id)
        _prune(directory, now)
        meta = {"at": now, "tool_use_id": tool_use_id, "cwd": str(payload.get("cwd") or ""),
                "mode": mode, "command": (command or "")[:300]}
        if foreground:
            meta["foreground_grace_ms"] = FOREGROUND_GRACE_MS
        transcript = payload.get("transcript_path")
        if mode == "native" and isinstance(transcript, str) and os.path.isabs(transcript):
            if os.path.basename(transcript) == session_id + ".jsonl":
                transcript = os.path.join(os.path.dirname(transcript), session_id, "subagents", "agent-" + agent_id + ".jsonl")
            if os.path.basename(transcript) == "agent-" + agent_id + ".jsonl":
                try:
                    offset = os.path.getsize(transcript)
                except OSError:
                    offset = 0
                meta["result_source"] = {"transcript": transcript, "offset": offset,
                                         "session": session_id, "agent": agent_id}
        _write_json(os.path.join(directory, tool_use_id + ".json"), meta)
    except OSError:
        return None
    return os.path.join(directory, tool_use_id), _held_path(session_id, agent_id)


def _head(stem, payload):
    """Only literal helper arguments and exports precede the visible command.

    Reading $$/$PPID in shell text makes Claude Code 2.1.283 reject otherwise
    ordinary isolated git calls. The helper records its own parent instead.
    """
    return ("python3 %s mark --state %s %s\nexport %s=%s %s=%s\n"
            % (shlex.quote(os.path.realpath(__file__)), shlex.quote(state_dir()), shlex.quote(stem),
               TAG, payload["agent_id"], SESSION_TAG, payload["session_id"]))


def mark(stem):
    """Record only the calling shell, then close the hook-to-spawn hold race.

    A failed capture fails the call before its body runs. No user command is
    passed to or executed by this helper.
    """
    root = os.path.realpath(os.path.join(state_dir(), "shells"))
    stem = os.path.realpath(stem)
    try:
        session, agent, tid = os.path.relpath(stem, root).split(os.sep)
    except ValueError:
        return 2
    if not _valid_ids(session, agent, tid):
        return 2
    meta = _read_json(stem + ".json")
    if not isinstance(meta, dict) or meta.get("tool_use_id") != tid:
        return 2
    pid = os.getppid()
    table = snapshot()
    row = table.get(pid)
    if not row or not (int(meta["at"]) - BIRTH_SLACK <= row["birth"] <= time.time() + BIRTH_SLACK):
        return 2
    meta["identity"] = {"pid": pid, "birth": row["birth"], "ppid": row["ppid"],
                        "parent_birth": table.get(row["ppid"], {}).get("birth")}
    _write_json(stem + ".json", meta)
    tmp = stem + ".pid.%d.new" % os.getpid()
    with open(tmp, "w") as out:
        out.write("%d %d\n" % (pid, row["ppid"]))
    os.replace(tmp, stem + ".pid")
    if meta.get("mode") != "exempt" and os.path.exists(_held_path(session, agent)):
        print(REFUSED_TEXT)
        return REFUSED_EXIT
    return 0


def _refuse(held):
    """A held agent's new command does not run and its call returns at once, carrying the WAIT."""
    return ("if [ -e %s ]; then printf '%%s\\n' %s; exit %d; fi\n"
            % (shlex.quote(held), shlex.quote(REFUSED_TEXT), REFUSED_EXIT))


def _wrap(stem, command):
    """The foreground command as a job of its shell, detachable by SIGUSR1 (see the module doc).

    The job keeps the shell's option state (errexit and pipefail from the caller's
    prefix) inside an inner subshell; the outer one only records the exit status.
    Unheld, the shell prints the job's output and exits with its status.
    """
    q = {s: shlex.quote(stem + s) for s in (".out", ".status", ".job")}
    return "\n".join([
        "__richos_detached=%s" % shlex.quote(DETACHED_TEXT),
        "trap 'printf \"%s\\n\" \"$__richos_detached\"; exit 0' USR1",
        "__richos_flags=$-",
        "(",
        "set +e",
        "(",
        "case $__richos_flags in *e*) set -e ;; esac",
        command,
        ")",
        "printf '%%s\\n' \"$?\" > %s" % q[".status"],
        ") > %s 2>&1 &" % q[".out"],
        "__richos_job=$!",
        "printf '%%s\\n' \"$__richos_job\" > %s" % q[".job"],
        "__richos_w=0",
        "wait \"$__richos_job\" || __richos_w=$?",
        "trap '' USR1",
        "__richos_s=$__richos_w",
        "read -r __richos_s < %s 2>/dev/null || :" % q[".status"],
        "cat %s 2>/dev/null || :" % q[".out"],
        "rm -f %s %s %s" % (q[".out"], q[".status"], q[".job"]),
        "[ \"$__richos_s\" = 0 ] || exit \"$__richos_s\"",
    ])


def rewrite(payload):
    """{"command": ..., "input": {...}} for a subagent's Bash call, or None to leave it alone.

    The command goes after the caller's own prefix (shell-evidence.py). "input"
    carries tool-input changes: the wait command gets the tool's longest timeout.
    """
    if not isinstance(payload, dict) or payload.get("tool_name") != "Bash":
        return None
    ti = payload.get("tool_input")
    if not isinstance(ti, dict) or not isinstance(ti.get("command"), str):
        return None
    command = ti["command"]
    mode = "exempt" if is_wait_call(command) else "bg" if ti.get("run_in_background") else "native"
    # Only the CLI transcript has authoritative structured handoff metadata.
    # Keep SDK/other hosts on their established background path.
    foreground = (mode == "native" and os.environ.get("RICHOS_AGENT_BASH_FOREGROUND", "1") == "1"
                  and os.environ.get("CLAUDE_CODE_ENTRYPOINT", "cli") == "cli")
    rec = _record(payload, mode, command, foreground=foreground)
    if rec is None:
        return None
    stem, held = rec
    head = _head(stem, payload)
    if mode == "exempt":
        # The wait runs THIS file, whatever path the agent typed: the code that wrapped its
        # calls is the code that collects them.
        ceiling, bound = _wait_bound_seconds()
        args = _wait_args(command)
        return {"command": head + "export RICHOS_AGENT_HOLD_WAIT_SECONDS=%d\n" % bound
                + " ".join(["python3", shlex.quote(os.path.realpath(__file__))] + [shlex.quote(a) for a in args]),
                "input": {"timeout": ceiling, "run_in_background": False}}
    if os.path.exists(held):
        _unlink_call(stem)  # the hook refuses this call, so no native task can start
        return {"command": command, "input": {}, "deny": REFUSED_TEXT}
    if foreground:
        return {"command": head + command,
                "input": {"run_in_background": False, "timeout": FOREGROUND_GRACE_MS},
                "context": ("This command has a six-second foreground grace. Use a directly delivered result "
                "without an extra wait. A command exceeding the grace continues as a native background task; "
                "do not restart it. Only after a background handoff, before dependent work or your final reply, run "
                + WAIT_COMMAND + " with Bash timeout 600000 to collect its output and exit status. "
                "Repeat on STILL RUNNING or STILL WAITING. A hold freezes owned work; follow its WAIT instructions. "
                "A missing result is never a task pass.")}
    return {"command": head + command, "input": {"run_in_background": True},
            "context": ("This command runs as a native background task so WAIT can freeze it immediately. "
            "Before dependent work or your final reply, run " + WAIT_COMMAND +
            " and set the Bash tool's timeout input to 600000; do not add a shell timeout prefix. "
            "That wait returns this command's output and exit status; no separate Read is needed for ordinary output. "
            "Repeat on STILL RUNNING or STILL WAITING. "
            "The wait returns promptly when a new hold starts; follow its WAIT instructions. "
            "The wait labels each task's exit status; a missing result is never a task pass." if mode == "native" else "")}


def capture(payload):
    """The ownership and hold-check lines alone (a background call's prefix), or ""."""
    if not isinstance(payload, dict) or payload.get("tool_name") != "Bash":
        return ""
    rec = _record(payload, "bg", str((payload.get("tool_input") or {}).get("command") or ""))
    if rec is None:
        return ""
    return _head(rec[0], payload)


# ---------------------------------------------------------------------------
# the process table
# ---------------------------------------------------------------------------

def _cpu_seconds(value):
    days, _, rest = value.partition("-")
    total = int(days) * 86400 if rest else 0
    for i, part in enumerate(reversed((rest or days).split(":"))):
        total += float(part) * 60 ** i
    return total


def snapshot():
    """{pid: row} for this user's processes: ppid, pgid, stat, cpu seconds, birth (epoch seconds)."""
    out = subprocess.run(["ps", "-ax", "-o", "pid=,ppid=,pgid=,uid=,stat=,time=,lstart="],
                         capture_output=True, text=True, timeout=10, check=True,
                         env={**os.environ, "LC_ALL": "C", "TZ": "UTC0"}).stdout
    uid = os.getuid()
    table = {}
    for line in out.splitlines():
        f = line.split(None, 6)
        if len(f) != 7 or not f[0].isdigit() or int(f[3]) != uid:
            continue
        try:
            birth = calendar.timegm(time.strptime(f[6].strip(), "%a %b %d %H:%M:%S %Y"))
            cpu = _cpu_seconds(f[5])
        except ValueError:
            continue
        table[int(f[0])] = {"ppid": int(f[1]), "pgid": int(f[2]), "stat": f[4], "cpu": cpu,
                            "birth": birth, "lstart": f[6].strip()}
    return table


def _read_pid(path):
    try:
        with open(path) as f:
            parts = f.read().split()
        return int(parts[0]), int(parts[1])
    except (OSError, ValueError, IndexError):
        return None


def calls(session_id, agent_id, table):
    """[{tid, pid, ppid, at, mode, stem}] of this agent's recorded calls whose shell is still that shell.

    mode is "fg" (a wrapped foreground call), "bg", "exempt" (its wait command) or
    "" (recorded by the previous capture, which did not wrap)."""
    directory = _shell_dir(session_id, agent_id)
    try:
        names = sorted(os.listdir(directory))
    except OSError:
        return []
    out = []
    for name in names:
        if not name.endswith(".pid"):
            continue
        stem = os.path.join(directory, name[:-4])
        got = _read_pid(stem + ".pid")
        meta = _read_json(stem + ".json") or {}
        try:
            written = os.stat(stem + ".pid").st_mtime
        except OSError:
            continue
        if not got or not isinstance(meta.get("at"), (int, float)):
            continue
        pid, ppid = got
        row = table.get(pid)
        if not row or row["ppid"] != ppid:
            continue
        # Started after the hook ran and before it wrote its own record: that shell, not a reused PID.
        if not (int(meta["at"]) - BIRTH_SLACK <= row["birth"] <= written + BIRTH_SLACK):
            continue
        out.append({"tid": name[:-4], "pid": pid, "ppid": ppid, "at": float(meta["at"]),
                    "mode": str(meta.get("mode") or ""), "stem": stem, "command": str(meta.get("command") or "")})
    return out


def owned_shells(session_id, agent_id, table):
    """[(pid, ppid, registered_at)] of this agent's recorded calls whose shell is still that shell."""
    return [(c["pid"], c["ppid"], c["at"]) for c in calls(session_id, agent_id, table)]


def _job_of(call, table):
    """The wrapped call's job pid, when it is alive and still that shell's child."""
    try:
        with open(call["stem"] + ".job") as f:
            job = int(f.read().split()[0])
    except (OSError, ValueError, IndexError):
        return None
    row = table.get(job)
    return job if row and row["ppid"] == call["pid"] else None


def _environment(pid, _buf={}):
    """This process's environment strings, or None when the OS will not say. Never printed."""
    if sys.platform == "darwin":
        try:
            libc = _buf.get("libc") or ctypes.CDLL(None, use_errno=True)
            _buf["libc"] = libc
            if "buf" not in _buf:
                argmax = ctypes.c_int(0)
                size = ctypes.c_size_t(ctypes.sizeof(argmax))
                if libc.sysctl((ctypes.c_int * 2)(1, 8), 2, ctypes.byref(argmax), ctypes.byref(size), None, 0):
                    return None
                _buf["buf"] = ctypes.create_string_buffer(argmax.value)
            buf = _buf["buf"]
            size = ctypes.c_size_t(len(buf))
            # KERN_PROCARGS2: argc, exec path, padding, argv, then the environment.
            if libc.sysctl((ctypes.c_int * 3)(1, 49, pid), 3, buf, ctypes.byref(size), None, 0):
                return None
            raw = buf.raw[:size.value]
            argc = int.from_bytes(raw[:4], "little")
            rest = raw[4:]
            i = rest.index(b"\0")
            while i < len(rest) and rest[i] == 0:
                i += 1
            parts = rest[i:].split(b"\0")
            env = []
            for item in parts[argc:]:
                if not item:
                    break
                env.append(item)
            return env
        except (OSError, ValueError, AttributeError):
            return None
    try:
        with open("/proc/%d/environ" % pid, "rb") as f:
            return f.read().split(b"\0")
    except OSError:
        return None


def tagged(agent_id, table, since, session_id=None):
    """Processes carrying this agent's inherited tag, started no earlier than its first recorded call."""
    want = ("%s=%s" % (TAG, agent_id)).encode()
    hits = set()
    for pid, row in table.items():
        if row["birth"] < since - BIRTH_SLACK:
            continue
        env = _environment(pid)
        if env and want in env and (session_id is None or
                ("%s=%s" % (SESSION_TAG, session_id)).encode() in env):
            hits.add(pid)
    return hits


def foreign(session_id, agent_id, pids, table):
    """Those of pids whose readable environment carries ANOTHER agent's owner or session tag,
    with their trees: provably not this agent's, whatever their ancestry. A process whose
    environment the OS will not show (macOS shows none for its own binaries: /bin/sleep,
    /bin/zsh) or that carries no tag is not foreign."""
    own = {TAG.encode(): ("%s=%s" % (TAG, agent_id)).encode(),
           SESSION_TAG.encode(): ("%s=%s" % (SESSION_TAG, session_id)).encode()}
    hits = set()
    for pid in pids:
        for item in _environment(pid) or []:
            name = item.split(b"=", 1)[0]
            if name in own and item != own[name]:
                hits.add(pid)
                break
    return subtree(hits, table) & set(pids)


def subtree(roots, table):
    """roots, their descendants and the members of every group one of them leads."""
    found = set(p for p in roots if p in table)
    while True:
        more = {p for p, r in table.items() if p not in found and (r["ppid"] in found or r["pgid"] in found)}
        if not more:
            return found
        found |= more


def _ancestors(table):
    """This process and its ancestors: the caller (a hook, the lead's shell) is never suspended."""
    keep, pid = set(), os.getpid()
    for _ in range(64):
        if pid <= 1 or pid in keep:
            break
        keep.add(pid)
        pid = table.get(pid, {}).get("ppid", os.getppid() if pid == os.getpid() else 0)
    return keep


def daemons(owned, shells, table):
    """Owned processes that daemonized: reparented to launchd and leading their own session.

    An adb server or a Gradle daemon started by this agent serves every agent's
    work; freezing it would hold theirs too. The agent's own recorded shells are
    never daemons, even after their session process died.
    """
    return {p for p in owned if p not in shells and table[p]["ppid"] == 1 and "s" in table[p]["stat"]}


def _recorded_pid(path, table, slack=5.0):
    """A pid a launcher wrote right after starting it (`echo $! > file`), still that process."""
    try:
        with open(path) as f:
            pid = int(f.read().strip())
        written = os.stat(path).st_mtime
    except (OSError, ValueError):
        return None
    return pid if pid in table and abs(table[pid]["birth"] - written) <= slack else None


def testvm_exclusions(table):
    """Roots the hold never freezes: validated guest-slot holders, recorded VM processes
    and each run's login keeper (both detached by run.sh, so not under the walk)."""
    root = os.environ.get("TESTVM_ROOT") or os.path.join(os.path.expanduser("~"), ".richos-testvm")
    roots, notes = set(), []
    for name in SLOT_FILES:
        rec = _read_json(os.path.join(root, name)) or {}
        pid, since = rec.get("pid"), rec.get("since")
        # The holder wrote its record after it started; a reused PID started later.
        if isinstance(pid, int) and isinstance(since, (int, float)) and pid in table \
                and table[pid]["birth"] <= since + BIRTH_SLACK:
            roots.add(pid)
            notes.append("test VM walk pid %d (slot %s) left running to its own end (its scenario step "
                         "has no time bound; a guest keeps using the quota until it ends)" % (pid, name))
    run = os.path.join(root, "run")
    try:
        guests = sorted(os.listdir(run))
    except OSError:
        guests = []
    for guest in guests:
        # run.sh writes `echo $! > vm.pid` and `> claude-keep.pid` right after each launch.
        for leaf, what in (("vm.pid", "test VM guest"), ("claude-keep.pid", "test VM login keeper")):
            pid = _recorded_pid(os.path.join(run, guest, leaf), table)
            if pid is not None:
                roots.add(pid)
                notes.append("%s pid %d (%s) left running" % (what, pid, guest))
    return roots, notes


LOCK_NAME = re.compile(r"(?:\.lock|-lock|/token-\d+)\Z")


def lock_files(pids):
    """{path: [pid, ...]}: lock files the held processes have open. Reported, never acted on.

    Held work keeps every lock it holds (Sage's catch 5): a Cargo target lock, a
    machine worker token, a git index lock. Nothing but ending the work can drop
    another process's lock, so the lead is told which ones a pause is keeping.
    Read through libproc (no lsof); any failure reports nothing.
    """
    if sys.platform != "darwin":
        return {}
    try:
        libproc = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
    except OSError:
        return {}
    found = {}
    for pid in sorted(pids):
        try:
            size = libproc.proc_pidinfo(pid, 1, 0, None, 0)          # PROC_PIDLISTFDS
            if size <= 0:
                continue
            buf = ctypes.create_string_buffer(size)
            size = libproc.proc_pidinfo(pid, 1, 0, buf, size)
            for i in range(max(0, size) // 8):                       # struct proc_fdinfo
                fd, kind = struct.unpack_from("iI", buf.raw, i * 8)
                if kind != 1:                                        # PROX_FDTYPE_VNODE
                    continue
                info = ctypes.create_string_buffer(1200)             # vnode_fdinfowithpath
                if libproc.proc_pidfdinfo(pid, fd, 2, info, 1200) <= 0:
                    continue
                path = info.raw[176:1200].split(b"\0", 1)[0].decode("utf-8", "replace")
                if path and LOCK_NAME.search(path):
                    found.setdefault(path, []).append(pid)
        except (OSError, ValueError, struct.error):
            continue
    return found


def owned_tree(session_id, agent_id, table):
    """(owned pids, excluded pids, protected pids, notes, shell parents)."""
    shells = owned_shells(session_id, agent_id, table)
    parents = {ppid: table[ppid]["birth"] for _p, ppid, _a in shells if ppid in table}
    # A shell can exit while its tagged descendants remain. Retain only records
    # whose helper actually captured a process identity, never an unstarted hook.
    identities = []
    directory = _shell_dir(session_id, agent_id)
    try:
        for name in os.listdir(directory):
            if name.endswith(".json"):
                meta = _read_json(os.path.join(directory, name)) or {}
                identity = meta.get("identity")
                if isinstance(identity, dict) and isinstance(meta.get("at"), (int, float)):
                    identities.append(meta)
                    pp = identity.get("ppid")
                    if pp in table and table[pp]["birth"] == identity.get("parent_birth"):
                        parents[pp] = table[pp]["birth"]
    except OSError:
        pass
    starts = [at for _p, _pp, at in shells] + [m["at"] for m in identities]
    if not starts:
        return set(), set(), set(), [], {}
    since = min(starts)
    owned = subtree([p for p, _pp, _a in shells] + sorted(tagged(agent_id, table, since, session_id)), table)
    protected = {1} | set(parents) | _ancestors(table)
    owned -= protected
    vm_roots, vm_notes = testvm_exclusions(table)
    excluded = subtree(vm_roots, table) & owned
    notes = vm_notes if excluded else []
    shared = daemons(owned - excluded, {p for p, _pp, _a in shells}, table)
    if shared:
        excluded |= subtree(shared, table) & owned
        notes.append("shared daemon(s) it started left running for other agents' work: pid %s"
                     % ", ".join(map(str, sorted(shared))))
    return owned, excluded, protected, notes, parents


def work_tree(session_id, agent_id, table):
    """The processes whose CPU says whether this agent's commands still work: its owned
    tree, test VM work included (a walk is work, never idleness), the shared daemons it
    started left out (other agents' work keeps them busy, not this agent's)."""
    owned = owned_tree(session_id, agent_id, table)[0]
    if not owned:
        return set()
    vm = subtree(testvm_exclusions(table)[0], table) & owned
    shells = {p for p, _pp, _a in owned_shells(session_id, agent_id, table)}
    shared = subtree(daemons(owned - vm, shells, table), table) & owned
    return owned - (shared - vm)


_TIMEBASE = []


def cpu_used(pid):
    """CPU seconds this process has used, user and system time, its exited and reaped
    children's included (so a loop of short commands counts all of them), or None when
    the OS will not say. ps's TIME column leaves the children out (measured 2026-10-14:
    a shell that ran 100 short python3 processes read 0:00.04 with and without -S)."""
    if sys.platform == "darwin":
        try:
            libproc = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
            if not _TIMEBASE:
                class _Timebase(ctypes.Structure):
                    _fields_ = [("numer", ctypes.c_uint32), ("denom", ctypes.c_uint32)]
                tb = _Timebase()
                if ctypes.CDLL(None).mach_timebase_info(ctypes.byref(tb)) or not tb.denom:
                    return None
                _TIMEBASE.append(tb.numer / tb.denom / 1e9)
            buf = ctypes.create_string_buffer(256)
            if libproc.proc_pid_rusage(int(pid), 2, buf):                   # RUSAGE_INFO_V2
                return None
            user, system = struct.unpack_from("QQ", buf.raw, 16)            # ri_user_time, ri_system_time
            cuser, csystem = struct.unpack_from("QQ", buf.raw, 96)          # ri_child_user/system_time
        except (OSError, ValueError, AttributeError, struct.error):
            return None
        return (user + system + cuser + csystem) * _TIMEBASE[0]
    try:
        with open("/proc/%d/stat" % int(pid)) as f:
            fields = f.read().rsplit(")", 1)[1].split()
        return sum(int(v) for v in fields[11:15]) / float(os.sysconf("SC_CLK_TCK"))
    except (OSError, ValueError, IndexError):
        return None


# ---------------------------------------------------------------------------
# hold and release
# ---------------------------------------------------------------------------

def _detach(call, job, table):
    """The job's tree is already stopped: record it, then make its shell print the WAIT and exit."""
    try:
        _write_json(call["stem"] + ".detached", {"at": time.time(), "job": job, "job_birth": table[job]["birth"],
                                                 "command": call["command"], "tool_use_id": call["tid"]})
        os.kill(call["pid"], signal.SIGUSR1)
        return True
    except (OSError, KeyError):
        return False


RELEASING = ".releasing"


def _save_hold(path, session_id, agent_id, name, at, held, parents, detached):
    _write_json(path, {"session_id": session_id, "agent_id": agent_id, "name": name, "at": at,
                       "held": {str(p): b for p, b in held.items()}, "parents": parents, "detached": detached})


def hold(session_id, agent_id, name="", sample=0.5, session_pid=None):
    """Suspend the agent's owned processes, end its foreground round, and refuse its new commands.

    The record is written FIRST, even for an idle agent: from that moment every new
    Bash call of this agent is refused with the WAIT text. Then its running processes
    are suspended, except the shell of a wrapped foreground call, which is told to
    return (its job stays frozen) so the queued WAIT reaches the agent, and its wait
    command, which is never frozen. `session_pid` (the lead's session process) lets
    release_orphans() tell a dead session from a live one when no shell recorded it.
    Returns the measured result; never raises on process races.
    """
    if not _valid_ids(session_id, agent_id):
        return {"ok": False, "why": "no valid session and agent id recorded", "held": {}}
    t0 = time.monotonic()
    path = _held_path(session_id, agent_id)
    previous = _read_json(path) or {}
    held = {int(k): v for k, v in (previous.get("held") or {}).items()}
    # A release that was cut short leaves its record beside the hold; the processes
    # it still names are held again here, so this hold's release continues them.
    leftover = _read_json(path + RELEASING)
    if leftover:
        held.update({int(k): v for k, v in (leftover.get("held") or {}).items()})
    parents = dict(previous.get("parents") or {})
    detached = dict(previous.get("detached") or {})
    excluded, notes, undetached = set(), [], []
    table = snapshot()
    if isinstance(session_pid, int) and session_pid in table:
        parents[str(session_pid)] = table[session_pid]["birth"]
    at = previous.get("at") or time.time()
    _save_hold(path, session_id, agent_id, name, at, held, parents, detached)
    if leftover:
        try:
            os.unlink(path + RELEASING)
        except OSError:
            pass
    grace = time.monotonic() + DETACH_GRACE
    rounds = 0
    while True:
        rounds += 1
        owned, excluded, _protected, notes, found_parents = owned_tree(session_id, agent_id, table)
        parents.update({str(k): v for k, v in found_parents.items()})
        mine = calls(session_id, agent_id, table)
        # Its wait command runs on; a wrapped foreground shell is told to return, not stopped.
        running_on = subtree([c["pid"] for c in mine if c["mode"] == "exempt"], table)
        foreground = [c for c in mine if c["mode"] == "fg" and c["tid"] not in detached]
        running_on |= {c["pid"] for c in foreground}
        # A shell already told to return is exiting: stopping it now would keep its round open.
        running_on |= {d["shell"] for d in detached.values()
                       if d.get("shell") in table and table[d["shell"]]["birth"] == d.get("shell_birth")}
        targets = sorted((owned - excluded - running_on)
                         - {p for p in held if held[p] == table.get(p, {}).get("birth")})
        # Recorded BEFORE the signal: a hold cut short between the stop and the final
        # save would otherwise leave a stopped process no release knows about. A
        # SIGCONT to a process that never stopped is harmless.
        for pid in targets:
            held[pid] = table[pid]["birth"]
        if targets:
            _save_hold(path, session_id, agent_id, name, at, held, parents, detached)
        for pid in targets:
            try:
                os.kill(pid, signal.SIGSTOP)
            except (ProcessLookupError, PermissionError):
                held.pop(pid, None)
        pending = []
        for c in foreground:
            if "s" not in table[c["pid"]]["stat"]:
                # Only a session leader's job survives its exit while stopped: otherwise its
                # group becomes orphaned with a stopped member and the kernel sends it SIGHUP.
                # Claude Code's Bash shells lead their own session (measured, 2.1.283); a
                # shell that does not is frozen whole, as before, and said so.
                pending.append(c)
                continue
            job = _job_of(c, table)
            if job is not None and all(p in held or p in excluded for p in subtree([job], table)):
                if _detach(c, job, table):
                    detached[c["tid"]] = {"shell": c["pid"], "shell_birth": table[c["pid"]]["birth"],
                                          "job": job, "at": time.time()}
            else:
                # Between its hold check and its job's start (milliseconds), or its job's tree
                # not yet stopped: look again after the next snapshot.
                pending.append(c)
        if not targets and not pending:
            break
        if pending and time.monotonic() >= grace:
            # Never started its job: stop it as the previous capture did, and say so.
            for c in pending:
                try:
                    held[c["pid"]] = table[c["pid"]]["birth"]
                    _save_hold(path, session_id, agent_id, name, at, held, parents, detached)
                    os.kill(c["pid"], signal.SIGSTOP)
                    undetached.append(c["pid"])
                except (ProcessLookupError, PermissionError, KeyError):
                    held.pop(c["pid"], None)
            table = snapshot()
            break
        if rounds >= STOP_ROUNDS and not pending:
            break
        if pending:
            time.sleep(0.05)   # short on purpose: bounded by DETACH_GRACE (1.5 s) and STOP_ROUNDS, never a long wait
        # A process forked between the snapshot and the signal is caught by the next round.
        table = snapshot()
    held = {p: b for p, b in held.items() if table.get(p, {}).get("birth") == b}
    deadline = time.monotonic() + SETTLE_SECONDS
    running = sorted(p for p in held if not table[p]["stat"].startswith("T"))
    while running and time.monotonic() < deadline:
        time.sleep(0.05)   # short on purpose: bounded by SETTLE_SECONDS (2 s); a stop has no kernel event to block on
        table = snapshot()
        running = sorted(p for p in held if p in table and not table[p]["stat"].startswith("T"))
    stopped_seconds = time.monotonic() - t0
    before = {p: table[p]["cpu"] for p in held if p in table}
    cpu_advance = None
    if held and sample > 0:
        time.sleep(sample)
        after = snapshot()
        cpu_advance = round(sum(max(0.0, after[p]["cpu"] - c) for p, c in before.items() if p in after), 2)
    locks = lock_files(held)
    result = {"ok": not running, "session_id": session_id, "agent_id": agent_id, "name": name,
              "at": at, "held": {str(p): b for p, b in sorted(held.items())},
              "not_stopped": running, "excluded": sorted(excluded), "notes": notes, "parents": parents,
              "stopped_seconds": round(stopped_seconds, 3), "sample_seconds": sample,
              "cpu_during_sample": cpu_advance, "locks": sorted(locks),
              "detached": detached, "undetached": undetached,
              "watchdog": previous.get("watchdog")}
    _write_json(path, result)
    # The record exists before the watchdog starts, so its first look finds it.
    result["watchdog"] = ensure_watchdog(session_id, agent_id, result.get("watchdog"))
    _write_json(path, result)
    return result


# ---------------------------------------------------------------------------
# the watchdog: no hold outlives its lead (Sage's catch 6) or its agent (catch 1)
# ---------------------------------------------------------------------------

def _watch_seconds():
    try:
        return max(0.2, float(os.environ.get("RICHOS_AGENT_HOLD_WATCH_SECONDS") or 5))
    except ValueError:
        return 5.0


def ensure_watchdog(session_id, agent_id, current=None):
    """{pid, birth} of this hold's watchdog, started unless the recorded one still runs.

    Detached (its own session, reparented to launchd once the hook exits) and
    without this agent's tags, so no hold ever freezes it.
    """
    if isinstance(current, dict) and isinstance(current.get("pid"), int):
        row = snapshot().get(current["pid"])
        if row and row["birth"] == current.get("birth"):
            return current
    env = {k: v for k, v in os.environ.items() if k not in (TAG, SESSION_TAG)}
    env["RICHOS_AGENT_HOLD_DIR"] = state_dir()
    try:
        p = subprocess.Popen([sys.executable, os.path.abspath(__file__), "watch",
                              "--session", session_id, "--agent", agent_id],
                             env=env, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                             stderr=subprocess.DEVNULL, start_new_session=True, close_fds=True)
    except OSError:
        return None
    row = None
    for _ in range(20):
        row = snapshot().get(p.pid)
        if row:
            break
        time.sleep(0.05)   # short on purpose: at most 20 looks (1 s) for a process just started
    return {"pid": p.pid, "birth": row["birth"] if row else None}


def _registry_finished(rec):
    """True when the workspace registry records this agent finished (stopped, disposed of,
    its session ended). A stop made from the CEO's screen runs no hook; this sees it."""
    name = rec.get("name") or ""
    if not ID.fullmatch(name or "-"):
        return False
    # RICHOS_AGENT_HOLD_REGISTRY: a declared test seam naming a workspaces.py stand-in.
    ws = (os.environ.get("RICHOS_AGENT_HOLD_REGISTRY") or "").strip() or os.path.join(
        os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))), "mega-lander", "workspaces.py")
    if not os.path.isfile(ws):
        return False
    try:
        out = subprocess.run([sys.executable, ws, "--session", rec["session_id"], "recipient", "--name", name],
                             capture_output=True, text=True, timeout=30).stdout
    except (OSError, subprocess.SubprocessError):
        return False
    return out.split("\t", 1)[0].strip() == "finished"


def watch(session_id, agent_id, registry_every=6):
    """Wait until this hold is released by anyone; release it ourselves when every
    recorded session process is gone or the registry records the agent finished.

    It blocks on the hold directory (a release moves the record) and on the exit of each
    recorded session process; only the registry, which has no event, is asked on a clock
    (every registry_every x RICHOS_AGENT_HOLD_WATCH_SECONDS, 30 s by default, as before)."""
    path = _held_path(session_id, agent_id)
    every = _watch_seconds() * registry_every
    next_registry = time.monotonic() + every
    while True:
        with _watch(paths=[os.path.dirname(path)]) as w:
            rec = _read_json(path)
            if not rec:
                return 0
            retry = False
            parents = rec.get("parents") or {}
            if parents:
                table = snapshot()
                live = [int(p) for p, b in parents.items() if table.get(int(p), {}).get("birth") == b]
                if not live:
                    if _watch_release(session_id, agent_id, rec, "its session process is gone"):
                        return 0
                    retry = True
                w.add_pids(live)
            if time.monotonic() >= next_registry:
                next_registry = time.monotonic() + every
                if _registry_finished(rec):
                    if _watch_release(session_id, agent_id, rec, "the registry records it finished"):
                        return 0
                    retry = True
            left = max(0.0, next_registry - time.monotonic())
            # A release that failed is tried again after the old poll gap, not at the next registry look.
            w.wait(min(left, _watch_seconds()) if retry else left)


def _watch_release(session_id, agent_id, rec, why):
    """The watchdog's release. A failed one is logged and the watchdog keeps watching, so
    the next poll tries again instead of leaving the hold standing with nobody on it."""
    name = rec.get("name") or agent_id
    result = release(session_id, agent_id)
    if result.get("ok"):
        _log("watchdog released %s: %s" % (name, why))
        return True
    _log("watchdog release of %s FAILED (%s): %s; trying again" % (name, why, result.get("why")))
    return not os.path.lexists(_held_path(session_id, agent_id))


def _log(line):
    try:
        os.makedirs(state_dir(), mode=0o700, exist_ok=True)
        with open(os.path.join(state_dir(), "watchdog.log"), "a") as out:
            out.write("%s %s\n" % (time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), line))
    except OSError:
        pass


# ---------------------------------------------------------------------------
# wait: a paused agent stays inside its run (Rich, brief addition 1)
# ---------------------------------------------------------------------------

def _clock():
    return time.strftime("%H:%M:%SZ", time.gmtime())


def _detached_calls(session, agent):
    """[(stem, record)] of this agent's frozen foreground commands not yet collected, oldest first."""
    directory = _shell_dir(session, agent)
    try:
        names = os.listdir(directory)
    except OSError:
        return []
    found = []
    for name in names:
        if name.endswith(".detached"):
            rec = _read_json(os.path.join(directory, name))
            if isinstance(rec, dict):
                found.append((os.path.join(directory, name[:-len(".detached")]), rec))
    return sorted(found, key=lambda sr: sr[1].get("at") or 0)


def _collect(stem, rec, deadline, poll, out):
    """Print one frozen command's result once it ends; True when it was collected."""
    job, birth = rec.get("job"), rec.get("job_birth")
    what = (rec.get("command") or "").strip().splitlines()[0][:120] if (rec.get("command") or "").strip() else ""
    label = "The command that was frozen%s" % (" (%s)" % what if what else "")
    status = None
    while True:
        # Blocks on the job's exit and on its status file's write (the call directory), never a clock.
        with _watch(paths=[os.path.dirname(stem)]) as w:
            try:
                with open(stem + ".status") as f:
                    status = f.read().strip()
            except OSError:
                status = None
            if status:
                break
            row = snapshot().get(job) if isinstance(job, int) else None
            if not row or row["birth"] != birth or row["stat"].startswith("Z"):
                # Ended without recording a status (killed); one last look for a late status.
                try:
                    with open(stem + ".status") as f:
                        status = f.read().strip() or None
                except OSError:
                    status = None
                break
            if time.monotonic() >= deadline:
                out.write("STILL RUNNING at %s: %s is running again and has not finished yet. Run this same "
                          "command again, with the Bash timeout 600000, to get its output and exit status.\n"
                          % (_clock(), label))
                return False
            w.add_pids([job])
            w.wait(min(poll, deadline - time.monotonic()))
    try:
        with open(stem + ".out", "rb") as f:
            body = f.read()
    except OSError:
        body = b""
    if status is not None:
        out.write("%s has finished with exit status %s. Its output, exactly as it would have returned:\n"
                  % (label, status))
    else:
        out.write("%s ended without recording an exit status (it was ended from outside). Its output up to "
                  "then:\n" % label)
    out.flush()
    if body:
        text = body.decode("utf-8", "replace")
        out.write(text if text.endswith("\n") else text + "\n")
    _unlink_call(stem)
    return True


def native_pending_ids(session, agent, found=None):
    """Tool ids of this agent's native calls whose shell is still running or still starting.

    Only mode "native" counts: the wait's own call is "exempt" and never waits on itself.
    `found`, a dict, also receives what a wait blocks on: "pids", the running shells, and
    "starting_until", the epoch time the earliest startup grace below runs out."""
    table = snapshot()
    running = [c for c in calls(session, agent, table)
               if c["mode"] == "native" and not table[c["pid"]]["stat"].startswith("Z")]
    pending = {c["tid"] for c in running}
    starting = []
    # The native tool can return its task id before the shell has started.
    # Do not let a subsequent wait race through that startup interval. A refused
    # call never writes its PID, so this grace is bounded rather than indefinite.
    directory = _shell_dir(session, agent)
    try:
        for name in os.listdir(directory):
            if name.endswith(".json"):
                stem = os.path.join(directory, name[:-5])
                meta = _read_json(stem + ".json") or {}
                if (meta.get("mode") == "native" and not os.path.exists(stem + ".pid")
                        and 0 <= time.time() - float(meta.get("at", 0)) < 10):
                    pending.add(name[:-5])
                    starting.append(float(meta.get("at", 0)) + 10)
    except OSError:
        pass
    if isinstance(found, dict):
        found["pids"] = [c["pid"] for c in running]
        found["starting_until"] = min(starting) if starting else None
    return pending


def native_pending(session, agent):
    return bool(native_pending_ids(session, agent))


def _native_uncollected(session, agent):
    """[(path, meta)] of this agent's native calls whose result is not delivered yet, oldest first."""
    directory = _shell_dir(session, agent)
    try:
        records = [(os.path.join(directory, n), _read_json(os.path.join(directory, n)) or {})
                   for n in os.listdir(directory) if n.endswith(".json")]
    except OSError:
        return []
    return sorted([(p, m) for p, m in records if m.get("mode") == "native" and not m.get("result_collected")],
                  key=lambda row: row[1].get("at", 0))


def _native_ready(session, agent, pending):
    """True when a finished, uncollected native call's result can be delivered now.

    A long call still running never keeps a finished call's result waiting."""
    for _path, meta in _native_uncollected(session, agent):
        if meta.get("tool_use_id") in pending or not meta.get("result_source"):
            continue
        try:
            if native_result(meta) is not None:
                return True
        except (OSError, ValueError, TypeError, KeyError):
            continue
    return False


NATIVE_OUTPUT_LIMIT = 64000
NATIVE_TRANSCRIPT_LIMIT = 4 * 1024 * 1024
NATIVE_FOOTER = re.compile(rb"\n\[(?:exited with code ([0-9]+)|killed)\]\n\Z")
NATIVE_HANDOFF = re.compile(
    r"(?:Command running in background with ID: (?P<explicit>[A-Za-z0-9_-]+)\. "
    r"|Command did not complete within its [0-9]+(?:\.[0-9]+)?s timeout and was moved to the background "
    r"\(ID: (?P<timed>[A-Za-z0-9_-]+)\)\. "
    r"|Command was moved to the background \(ID: (?P<message>[A-Za-z0-9_-]+)\) "
    r"so that a message that arrived while it was running can reach you; it was not interrupted\. "
    r"|Command was manually backgrounded by user with ID: (?P<manual>[A-Za-z0-9_-]+)\. )"
    r"Output is being written to: (?P<path>.+?\.output)(?:\. |\n|$)")

# THE HOST'S TRANSCRIPT IS READ ONCE PER PROCESS, NOT ONCE PER CALL RECORD PER LOOK (2026-10-28).
# native_result used to read up to 4 MB of the agent's transcript from each record's offset and
# parse every line, for every uncollected record, at every look of a wait (every 0.5 s, and every
# 0.05 s for up to 2 s per record once nothing ran). In the app's SDK sessions no record ever
# binds (the host writes task output under a different session directory than the transcript's
# sessionId), so they stayed uncollected: one agent had 222 records over an 8.5 MB transcript,
# about 0.7 GB of JSON parsed per look. Now each transcript is indexed once: only a complete line
# that names a tool_result is parsed (the api-request rows that fill most of it never are), and
# a later look reads only what the host appended since. A transcript that shrank, was replaced or
# no longer ends with the bytes this index last read is indexed again from the start.
_TRANSCRIPTS = {}
_BOUND = {}       # tool_use_id -> the bound host output file, so a wait can watch it change
_TAIL = 64


def _tool_result_rows(path, tool_use_id, start, end):
    """Rows of the transcript at `path` carrying a tool_result for tool_use_id whose whole line
    (newline included) lies in [start, end), in file order: exactly the lines a read of that
    window would parse, less those that cannot match. Raises OSError as that read would."""
    st = os.stat(path)
    ix = _TRANSCRIPTS.get(path)
    if ix is not None and (ix["ino"] != st.st_ino or st.st_size < ix["pos"] or start < ix["base"]):
        ix = None
    if ix is not None and ix["tail"]:
        with open(path, "rb") as stream:
            stream.seek(ix["pos"] - len(ix["tail"]))
            if stream.read(len(ix["tail"])) != ix["tail"]:
                ix = None
    if ix is None:
        ix = _TRANSCRIPTS[path] = {"ino": st.st_ino, "base": start, "pos": start, "tail": b"", "rows": {}}
    if st.st_size > ix["pos"]:
        with open(path, "rb") as stream:
            stream.seek(ix["pos"])
            raw = stream.read(st.st_size - ix["pos"])
        complete = raw[:raw.rfind(b"\n") + 1]
        at = ix["pos"]
        for line in complete.split(b"\n")[:-1]:
            line_end = at + len(line) + 1
            if b"tool_result" in line:
                try:
                    row = json.loads(line)
                except ValueError:
                    row = None
                content = row.get("message") if isinstance(row, dict) else None
                content = content.get("content") if isinstance(content, dict) else None
                ids = []
                for item in content if isinstance(content, list) else ():
                    tid = item.get("tool_use_id") if isinstance(item, dict) else None
                    if isinstance(tid, str) and tid not in ids:
                        ids.append(tid)
                for tid in ids:
                    ix["rows"].setdefault(tid, []).append((at, line_end, row))
            at = line_end
        if complete:
            ix["tail"] = complete[-_TAIL:] if len(complete) >= _TAIL else (ix["tail"] + complete)[-_TAIL:]
            ix["pos"] = at
    return [row for s, e, row in ix["rows"].get(tool_use_id, ()) if s >= start and e <= end]


def _result_paths(session, agent, pending):
    """The host files whose change can make a finished, uncollected call's result ready."""
    paths = []
    for _path, meta in _native_uncollected(session, agent):
        source = meta.get("result_source")
        if meta.get("tool_use_id") in pending or not isinstance(source, dict):
            continue
        for p in (source.get("transcript"), (meta.get("native_result") or {}).get("path"),
                  _BOUND.get(meta.get("tool_use_id"))):
            if isinstance(p, str) and p not in paths:
                paths.append(p)
    return paths


def consume_foreground(session, agent):
    """A later tool boundary consumes direct delivery without another model/tool round.

    Never read background output here or consume a live/starting command.
    """
    records = [(p, m) for p, m in _native_uncollected(session, agent)
               if m.get("foreground_grace_ms") and m.get("result_source") and not m.get("native_result")]
    if not records:
        return
    pending = native_pending_ids(session, agent)
    for path, meta in records:
        if meta.get("tool_use_id") in pending:
            continue
        try:
            if native_result(meta, binding_only=True) is FOREGROUND_DELIVERED:
                meta["result_collected"] = True
                _write_json(path, meta)
            elif meta.get("native_result"):
                _write_json(path, meta)  # Cache the handoff binding, without reading task output.
        except (OSError, ValueError, TypeError, KeyError):
            continue  # Unknown results remain available to the ordinary collector.


def native_result(meta, binding_only=False):
    """Read this call's host-owned result, never a guessed task directory.

    Claude Code 2.1.283 records the task id and output path in the subagent's
    tool result and appends a terminal status to that file. Bind only the exact
    session/agent/tool id captured before the call. The caller first waits for
    the owned shell to end: command output resembling a footer is not completion
    while that shell is running. Missing or changed host formats are not passes.
    """
    source = meta["result_source"]
    binding = meta.get("native_result")
    if not binding:
        # Only complete lines inside [offset, offset + NATIVE_TRANSCRIPT_LIMIT), as a read of
        # that window would see them: a trailing partial line may still be being appended.
        for row in _tool_result_rows(source["transcript"], meta["tool_use_id"], source["offset"],
                                     source["offset"] + NATIVE_TRANSCRIPT_LIMIT):
            if (not isinstance(row, dict) or row.get("type") != "user" or
                    row.get("agentId") != source["agent"] or row.get("sessionId") != source["session"]):
                continue
            content = row.get("message", {}).get("content")
            if not isinstance(content, list):
                continue
            if (row.get("toolUseResult") is not None and row.get("tool_use_result") is not None
                    and row["toolUseResult"] != row["tool_use_result"]):
                continue
            response = row.get("toolUseResult", row.get("tool_use_result")) or {}
            structured_task = response.get("backgroundTaskId") if isinstance(response, dict) else None
            for item in content:
                if not isinstance(item, dict) or item.get("type") != "tool_result" or item.get("tool_use_id") != meta["tool_use_id"]:
                    continue
                text = item.get("content")
                if not isinstance(text, str):
                    continue
                # The original host tool result already reached the model. Do not
                # replay its output or infer a task exit code from arbitrary prose.
                direct = (isinstance(response, dict) and
                          isinstance(response.get("stdout"), str) and
                          isinstance(response.get("stderr"), str) and
                          isinstance(response.get("interrupted"), bool) and
                          not structured_task and "timedOutAfterMs" not in response)
                match = NATIVE_HANDOFF.match(text)
                if (meta.get("foreground_grace_ms") and isinstance(item.get("is_error"), bool)
                        and direct):
                    meta["foreground_delivery"] = {"is_error": item["is_error"] or response.get("interrupted") is True}
                    return FOREGROUND_DELIVERED
                if item.get("is_error"):
                    return (2, "Host refused or failed this call:\n" + text, None)
                # Foreground stdout can imitate the entire host handoff receipt,
                # including an owned output file. Require a structured host task
                # id. The established forced-background SDK path is unambiguous:
                # its initial tool result is always a host background receipt.
                if meta.get("foreground_grace_ms") and not structured_task:
                    continue
                if not match:
                    continue
                task = next(match[k] for k in ("explicit", "timed", "message", "manual") if match[k])
                path = match["path"]
                if not _valid_ids(task) or (structured_task is not None and structured_task != task):
                    continue
                if (not os.path.isabs(path) or os.path.basename(path) != task + ".output" or
                        os.path.basename(os.path.dirname(path)) != "tasks" or
                        os.path.basename(os.path.dirname(os.path.dirname(path))) != source["session"]):
                    continue
                binding = meta["native_result"] = {"task": task, "path": path}
        if not binding:
            return None
    if isinstance(binding, dict) and isinstance(binding.get("path"), str):
        _BOUND[meta["tool_use_id"]] = binding["path"]   # only so a wait can watch this output file
    if binding_only:
        return None
    fd = os.open(binding["path"], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, "rb") as stream:
        before = os.fstat(stream.fileno())
        if not stat.S_ISREG(before.st_mode) or before.st_uid != os.getuid():
            raise ValueError("native output is not a regular file owned by this user")
        stream.seek(max(0, before.st_size - 256))
        tail = stream.read(256)
        footer = NATIVE_FOOTER.search(tail)
        if not footer:
            return None
        body_size = before.st_size - len(tail) + footer.start()
        stream.seek(0)
        body = stream.read(min(body_size, NATIVE_OUTPUT_LIMIT)).decode("utf-8", "replace")
        after = os.fstat(stream.fileno())
        if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
            return None
    code = int(footer[1]) if footer[1] is not None else 137
    if not 0 <= code <= 255:
        raise ValueError("unknown native exit status")
    if body_size > NATIVE_OUTPUT_LIMIT:
        body += "\n[output truncated; full native output: %s]\n" % binding["path"]
    if footer[1] is None:
        body += "\n[host reports task killed; collector returns 137]\n"
    return code, body, binding["task"]


def collect_native(session, agent, deadline, out, pending=frozenset()):
    """Return native results in the wait call, retaining ownership after delivery.

    A call in `pending` is still running: it is named, never collected, and a later wait returns it."""
    result_code = 0
    for path, meta in _native_uncollected(session, agent):
        if meta.get("tool_use_id") in pending:
            what = (meta.get("command") or "").strip().splitlines()
            out.write("NOT FINISHED YET: tool %s%s is still running; a later wait returns its output and "
                      "exit status.\n" % (meta.get("tool_use_id"), " (%s)" % what[0][:120] if what else ""))
            continue
        if not meta.get("result_source"):
            out.write("NATIVE RESULT UNAVAILABLE for %s: this older or non-host call has no transcript binding. "
                      "Use its native completion notification and output file.\n" % meta["tool_use_id"])
            result_code = result_code or 2
            continue
        result = None
        bound = min(deadline, time.monotonic() + 2)
        held = _held_path(session, agent)
        while True:
            # Blocks on the hold directory and on the host's transcript and output file, where
            # the result appears; it used to look again every 0.05 s, re-reading the transcript.
            source = meta["result_source"] if isinstance(meta["result_source"], dict) else {}
            with _watch(paths=[os.path.dirname(held), source.get("transcript")]) as w:
                if os.path.exists(held):
                    notice_hold(session, agent)
                    out.write("WAIT: running work is held. " + HOW_TO_WAIT + "\n")
                    return result_code
                try:
                    result = native_result(meta)
                except (OSError, ValueError, TypeError) as error:
                    reason = type(error).__name__
                else:
                    reason = "host task binding or terminal output is not available"
                if result is not None or time.monotonic() >= bound:
                    break
                w.add_paths([_BOUND.get(meta.get("tool_use_id"))])
                w.wait(bound - time.monotonic())
        if result is None:
            out.write("NATIVE RESULT UNAVAILABLE for %s: %s. No task success is established; "
                      "check its native completion notification.\n" % (meta["tool_use_id"], reason))
            result_code = result_code or 2
            continue
        if result is FOREGROUND_DELIVERED:
            failed = meta["foreground_delivery"]["is_error"]
            out.write("ALREADY DELIVERED: tool %s foreground result%s; output is not repeated and "
                      "no task exit code is inferred.\n" % (meta["tool_use_id"], " reported failure" if failed else ""))
            meta["result_collected"] = True
            _write_json(path, meta)
            result_code = result_code or (2 if failed else 0)
            continue
        code, body, task = result
        out.write("TASK %s (tool %s) EXIT STATUS %d\n" % (task or "refused", meta["tool_use_id"], code))
        if body:
            out.write(body if body.endswith("\n") else body + "\n")
        out.flush()
        meta["result_collected"] = True
        _write_json(path, meta)
        result_code = result_code or code
    return result_code


def notice_hold(session, agent):
    """Return once for each hold before waiting, including a hold that predates
    this call. That tool boundary lets the harness deliver the queued message.
    """
    held = _read_json(_held_path(session, agent))
    if not isinstance(held, dict) or not isinstance(held.get("at"), (int, float)):
        return False
    path = os.path.join(state_dir(), "wait-notices", "%s__%s.json" % (session, agent))
    if (_read_json(path) or {}).get("at") == held["at"]:
        return False
    _write_json(path, {"at": held["at"]})
    return True


def gate(payload, poll=GATE_POLL_SECONDS):
    """A held agent's wait call starts only once its hold is released (run first by shell-evidence.py).

    The call is held here, before it runs, so the model is not called while the
    agent waits. Only a subagent's own wait call, only while its hold stands and
    only after that agent was handed this hold's WAIT notice (the first wait after
    a hold returns at once, so the queued WAIT message reaches the agent). Always
    allows the call in the end and never prints a decision: the wait itself then
    reports RESUMED. Ends early when the session that ran this hook is gone.
    """
    try:
        if not isinstance(payload, dict) or payload.get("tool_name") != "Bash":
            return 0
        if not is_wait_call((payload.get("tool_input") or {}).get("command")):
            return 0
        session, agent = payload.get("session_id"), payload.get("agent_id")
        if not _valid_ids(session, agent):
            return 0
        path = _held_path(session, agent)
        notice = os.path.join(state_dir(), "wait-notices", "%s__%s.json" % (session, agent))
        parent = os.getppid()
        while True:
            # Blocks on the hold and notice directories (a release moves the record) and on the
            # exit of the process that ran this hook; `poll` is only the longest gap between looks.
            with _watch(paths=[os.path.dirname(path), os.path.dirname(notice)], pids=[parent]) as w:
                held = _read_json(path)
                if not isinstance(held, dict):
                    return 0
                if (_read_json(notice) or {}).get("at") != held.get("at"):
                    return 0
                if os.getppid() == 1:
                    return 0
                w.wait(poll)
    except Exception:  # a broken gate must never keep a call from running
        return 0


def wait_resume(max_seconds=None, poll=WAIT_RECHECK_SECONDS, out=sys.stdout):
    """Returns once this agent's hold is released, printing RESUMED and then the result of
    any foreground command the hold froze; at the bound it prints STILL WAITING (or STILL
    RUNNING) so the agent runs it again. Native task output and exit status are
    delivered here even when no pause happened. Never ends anything.

    Every wait in it blocks on what it waits for (see WAIT_RECHECK_SECONDS): the hold
    directory, the call-record directory, the exit of each running shell, the host's
    transcript and output files. `poll` is only the longest it goes without looking."""
    agent, session = os.environ.get(TAG, ""), os.environ.get(SESSION_TAG, "")
    if not _valid_ids(session, agent):
        out.write("PAUSE-WAIT: this shell carries no agent identity (%s, %s), so there is no hold to wait "
                  "for. Only a subagent's Bash call carries it.\n" % (TAG, SESSION_TAG))
        return 2
    if max_seconds is None:
        max_seconds = _wait_bound_seconds()[1]
    deadline = time.monotonic() + max_seconds
    path = _held_path(session, agent)
    held_dir, shells = os.path.dirname(path), _shell_dir(session, agent)
    while True:
        with _watch(paths=[held_dir]) as w:
            if not os.path.exists(path):
                break
            if notice_hold(session, agent):
                out.write("WAIT: running work is held. " + HOW_TO_WAIT + "\n")
                return 0
            if time.monotonic() >= deadline:
                out.write("STILL WAITING at %s: run this same command again, with the Bash timeout 600000.\n"
                          % _clock())
                return 0
            w.wait(min(poll, deadline - time.monotonic()))
    notice_path = os.path.join(state_dir(), "wait-notices", "%s__%s.json" % (session, agent))
    notice = _read_json(notice_path)
    if notice and not notice.get("resumed"):
        out.write("RESUMED at %s: carry on from where you were.\n" % _clock())
        _write_json(notice_path, dict(notice, resumed=True))
    # Native tasks keep their output/status in the harness. Keep this agent's
    # run active while it waits, but return as soon as a new hold appears so a
    # queued SendMessage can be delivered at this tool boundary, and as soon as
    # any finished call's result is ready, even while another call still runs.
    while True:
        with _watch(paths=[held_dir, shells]) as w:
            found = {}
            pending = native_pending_ids(session, agent, found)
            if not pending or _native_ready(session, agent, pending):
                break
            if os.path.exists(path):
                notice_hold(session, agent)
                out.write("WAIT: running work is held. " + HOW_TO_WAIT + "\n")
                return 0
            if time.monotonic() >= deadline:
                out.write("STILL RUNNING at %s: repeat this wait to collect the native task's completion.\n" % _clock())
                return 0
            # A running shell's exit, a new record or .pid, a hold, or the host appending a result.
            w.add_pids(found.get("pids") or ())
            w.add_paths(_result_paths(session, agent, pending))
            gap = min(poll, deadline - time.monotonic())
            if found.get("starting_until"):
                gap = min(gap, max(0.0, found["starting_until"] - time.time()) + 0.05)
            w.wait(gap)
    for stem, rec in _detached_calls(session, agent):
        if not _collect(stem, rec, deadline, poll, out):
            break
    return collect_native(session, agent, deadline, out, pending)


def release(session_id, agent_id):
    """Continue exactly the processes this agent's hold suspended, each re-checked by start time."""
    if not _valid_ids(session_id, agent_id):
        return {"ok": False, "why": "no valid session and agent id recorded", "continued": [], "gone": []}
    path = _held_path(session_id, agent_id)
    rec = _read_json(path)
    # A release cut short after the hold record moved aside is finished from the
    # record it left, so the processes it still names are continued.
    if not rec:
        rec = _read_json(path + RELEASING)
    if not rec:
        return {"ok": True, "continued": [], "gone": [], "why": "nothing was held"}
    t0 = time.monotonic()
    # Moved aside FIRST, so no new command suspends itself after the scans below,
    # yet the record of what was stopped survives until every SIGCONT is sent.
    # Only an ABSENT active record is the retry of a release cut short; an active
    # record that is still there after a failed move means the hold stands: its new
    # commands are still refused and its wait never sees a release, so nothing is
    # continued and the release reports that it failed (hunt P5-76).
    try:
        os.replace(path, path + RELEASING)
    except OSError as error:
        if os.path.lexists(path):
            return {"ok": False, "continued": [], "gone": [], "waited": [], "name": rec.get("name", ""),
                    "why": "its hold record could not be moved aside (%s: %s), so the hold still stands and "
                           "nothing was continued; release it again once %s is writable"
                           % (type(error).__name__, error, os.path.dirname(path))}
    table = snapshot()
    continued, gone, waited = [], [], []
    for key, birth in sorted((rec.get("held") or {}).items(), key=lambda kv: int(kv[0])):
        pid = int(key)
        if table.get(pid, {}).get("birth") != birth:
            gone.append(pid)
            continue
        try:
            os.kill(pid, signal.SIGCONT)
            continued.append(pid)
        except ProcessLookupError:
            gone.append(pid)
    # New commands that suspended themselves while held: this agent's own recorded
    # shells, registered after the hold began, in state T. Scanned more than once
    # so a shell that read the record just before its removal is not left behind.
    since = float(rec.get("at") or 0) - BIRTH_SLACK
    for scan in range(RELEASE_SCANS):
        if scan:
            time.sleep(RELEASE_SCAN_GAP)   # bounded: RELEASE_SCANS (4) looks, 0.75 s in all, never a wait
            table = snapshot()
        for pid, _ppid, at in owned_shells(rec["session_id"], rec["agent_id"], table):
            if at >= since and pid not in continued and table[pid]["stat"].startswith("T"):
                try:
                    os.kill(pid, signal.SIGCONT)
                    continued.append(pid)
                    waited.append(pid)
                except ProcessLookupError:
                    pass
    released = time.time()
    _remember_hold(rec, released)
    result = {"ok": True, "continued": continued, "gone": gone, "waited": waited, "name": rec.get("name", ""),
              "held_seconds": round(released - float(rec.get("at") or released), 1),
              "continued_seconds": round(time.monotonic() - t0, 3)}
    try:
        os.unlink(path + RELEASING)
    except FileNotFoundError:
        pass
    except OSError as error:
        # The processes are continued, but the record left behind is read by the next
        # hold as "already held", so that hold would skip suspending them: not a success.
        result["ok"] = False
        result["why"] = ("its processes were continued, but the release record %s could not be removed "
                         "(%s: %s); the next hold of this agent would treat them as already suspended"
                         % (path + RELEASING, type(error).__name__, error))
    return result


HISTORY_FILE = "hold-history.jsonl"
HISTORY_KEEP = 14 * 86400      # a released hold is remembered this long
HISTORY_PRUNE_BYTES = 256 * 1024


def _remember_hold(rec, released):
    """One line per released hold: who was held, from when to when, and which processes.

    resource_waits.py reads it so a wait recorded by a held agent does not count
    the held time once the agent is resumed. Best effort: a line that cannot be
    written costs only that subtraction, never the release."""
    path = os.path.join(state_dir(), HISTORY_FILE)
    line = json.dumps({"session_id": rec.get("session_id"), "agent_id": rec.get("agent_id"),
                       "name": rec.get("name") or "", "at": rec.get("at"), "released": released,
                       "held": rec.get("held") or {}}, sort_keys=True) + "\n"
    try:
        os.makedirs(state_dir(), mode=0o700, exist_ok=True)
        with open(path, "a") as out:
            out.write(line)
        if os.path.getsize(path) > HISTORY_PRUNE_BYTES:
            keep = [r for r in history() if released - float(r.get("released") or 0) < HISTORY_KEEP]
            tmp = "%s.%d.new" % (path, os.getpid())
            with open(tmp, "w") as out:
                out.writelines(json.dumps(r, sort_keys=True) + "\n" for r in keep)
            os.replace(tmp, path)
    except (OSError, ValueError, TypeError):
        pass


def history():
    """Released holds, oldest first: [{session_id, agent_id, name, at, released, held}]."""
    try:
        with open(os.path.join(state_dir(), HISTORY_FILE)) as f:
            lines = f.read().splitlines()
    except OSError:
        return []
    out = []
    for line in lines:
        try:
            r = json.loads(line)
        except ValueError:
            continue
        if isinstance(r, dict) and isinstance(r.get("at"), (int, float)) \
                and isinstance(r.get("released"), (int, float)):
            out.append(r)
    return out


# ---------------------------------------------------------------------------
# end: an agent whose run has ended leaves no command of its own running
# ---------------------------------------------------------------------------
# 2026-10-11, echo-opus-walk49fix1: it handed over its fixes, but four of its own
# background loops (`until grep -q ... ; do sleep 3; done` on a file in a scratch
# folder it had already deleted) could never finish. The registry rightly does not
# take a SubagentStop as the end while the agent's own command still runs (the
# platform wakes it when that command ends: workspaces.py record_end), so the land
# was refused until Rich found and stopped them by hand. Now, whenever the registry
# records an agent's end, the commands it still owns are ended through the same
# ownership record and owned-tree computation as a pause, with the same exclusions
# (shared daemons it started and the test VM guest stay running), and what was
# ended is logged. `agent_hold.py end` is the one command that does this by hand
# for an agent whose end waits on its own commands: once they are gone the platform
# wakes it, its turn ends with nothing running, and that end is recorded.
END_GRACE = 2.0      # seconds between SIGTERM and SIGKILL


def _commands(pids):
    """{pid: command line} for the log, read once; a process ps cannot see is ''."""
    if not pids:
        return {}
    try:
        out = subprocess.run(["ps", "-o", "pid=,command=", "-p", ",".join(map(str, sorted(pids)))],
                             capture_output=True, text=True, timeout=10).stdout
    except (OSError, subprocess.SubprocessError):
        return {}
    got = {}
    for line in out.splitlines():
        pid, _, cmd = line.strip().partition(" ")
        if pid.isdigit():
            got[int(pid)] = cmd.strip()
    return got


def _gone(pid, birth, table):
    row = table.get(pid)
    return not row or row["birth"] != birth or row["stat"].startswith("Z")


def _birth_now(pid):
    """This process's start time (epoch seconds, the resolution snapshot() records) read
    at this instant, or None when it is gone, a zombie or unreadable. On macOS one libproc
    call (struct proc_bsdinfo, 136 bytes: status at 4, start seconds at 120); elsewhere a
    one-process ps read with snapshot()'s parsing."""
    if sys.platform == "darwin":
        try:
            libproc = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
            buf = ctypes.create_string_buffer(136)
            if libproc.proc_pidinfo(int(pid), 3, ctypes.c_uint64(0), buf, 136) != 136:  # PROC_PIDTBSDINFO
                return None
            status, = struct.unpack_from("I", buf.raw, 4)
            start, = struct.unpack_from("Q", buf.raw, 120)
        except (OSError, ValueError, struct.error):
            return None
        return None if status == 5 else int(start)                                      # SZOMB
    try:
        out = subprocess.run(["ps", "-o", "stat=,lstart=", "-p", str(int(pid))], capture_output=True,
                             text=True, timeout=10, env={**os.environ, "LC_ALL": "C", "TZ": "UTC0"}).stdout
        st, _, lstart = out.strip().partition(" ")
        if not st or st.startswith("Z"):
            return None
        return calendar.timegm(time.strptime(lstart.strip(), "%a %b %d %H:%M:%S %Y"))
    except (OSError, subprocess.SubprocessError, ValueError):
        return None


def _signal_same(pid, birth, sig):
    """Send `sig` only if `pid` is still the process born at `birth`, re-read from the kernel
    immediately before (second review rv-20261011T042129Z-6b0f0810-1c93: a snapshot can be
    older than the signal, and an id freed in between can belong to a process the agent
    does not own). False when it is no longer that process; never signals another."""
    if _birth_now(pid) != birth:
        return False
    os.kill(pid, sig)
    return True


def end_owned(session_id, agent_id, name="", why="", spare_foreign=False):
    """End the commands this agent still owns; never a held (paused) agent's work.

    The owned tree and its exclusions are owned_tree()'s, exactly as for a hold.
    spare_foreign (the stall watcher's automatic end): a process whose readable
    environment carries another agent's owner or session tag is left running, its tree too.
    Each process is signaled only while it is still the process recorded (its start
    time re-read from the kernel immediately before EACH signal): SIGTERM and SIGCONT first, SIGKILL after END_GRACE for any
    that remain. Rounds catch a process forked between the snapshot and the
    signal. Every ended process is logged to ended.log. Never raises on races."""
    if not _valid_ids(session_id, agent_id):
        return {"ok": False, "why": "no valid session and agent id recorded", "ended": []}
    path = _held_path(session_id, agent_id)
    if os.path.lexists(path) or os.path.lexists(path + RELEASING):
        # A pause never ends anything: its frozen work continues on RESUME.
        return {"ok": True, "ended": [], "skipped": "its work is held by a pause, which never ends anything"}
    t0 = time.monotonic()
    table = snapshot()
    shell_cmds = {c["pid"]: c["command"] for c in calls(session_id, agent_id, table)}
    signaled, excluded, notes, described, unconfirmed = {}, set(), [], {}, {}
    for _ in range(STOP_ROUNDS):
        owned, excl, _protected, found_notes, _parents = owned_tree(session_id, agent_id, table)
        other = foreign(session_id, agent_id, owned - excl, table) if spare_foreign else set()
        excl = excl | other
        # What is left running keeps its owned parents too: ending a parent can end its tree
        # (a proc_tree supervisor's cleanup ends the walk it runs; second review
        # rv-20261011T055437Z-792eda49-35e8). Freezing a parent is harmless, so a hold has no
        # such step. They end by themselves when what they wait on ends.
        parents_of = set()
        for pid in excl:
            up = table.get(pid, {}).get("ppid")
            while up in owned and up not in excl and up not in parents_of:
                parents_of.add(up)
                up = table[up]["ppid"]
        excl = excl | parents_of
        excluded |= excl
        notes = list(found_notes or notes)
        if other:
            line = ("process(es) carrying another agent's owner or session tag left running, with "
                    "their own: pid %s" % ", ".join(map(str, sorted(other))))
            if line not in notes:
                notes.append(line)
        if parents_of:
            notes.append("its parent process(es) of what is left running also left running, so ending "
                         "them cannot end it: pid %s" % ", ".join(map(str, sorted(parents_of))))
        targets = sorted(p for p in owned - excl
                         if signaled.get(p) != table[p]["birth"] and not table[p]["stat"].startswith("Z"))
        if not targets:
            break
        described.update(_commands(set(targets) - set(described)))
        for pid in targets:
            birth = table[pid]["birth"]
            try:
                # Identity re-read from the kernel before each signal, never from the snapshot.
                if not _signal_same(pid, birth, signal.SIGTERM):
                    unconfirmed[pid] = birth
                    continue
                signaled[pid] = birth
                unconfirmed.pop(pid, None)
                _signal_same(pid, birth, signal.SIGCONT)   # a stopped process takes its SIGTERM only once continued
            except ProcessLookupError:
                pass
            except PermissionError:
                signaled.pop(pid, None)
        time.sleep(0.05)   # bounded: STOP_ROUNDS (10) rounds, 0.5 s in all; a fork has no event to block on
        table = snapshot()
    deadline = time.monotonic() + END_GRACE
    remaining = [p for p, b in signaled.items() if not _gone(p, b, table)]
    while remaining and time.monotonic() < deadline:
        time.sleep(0.05)   # bounded: END_GRACE (2 s) between SIGTERM and SIGKILL, never a long wait
        table = snapshot()
        remaining = [p for p, b in signaled.items() if not _gone(p, b, table)]
    killed = []
    for pid in remaining:
        try:
            if _signal_same(pid, signaled[pid], signal.SIGKILL):
                killed.append(pid)
        except (ProcessLookupError, PermissionError):
            pass
    if killed:
        time.sleep(0.1)
        table = snapshot()
    # A target whose identity could not be confirmed was never signaled; if the snapshot
    # still shows it as the same process, it is still running and reported, never hidden.
    survivors = sorted({p for p, b in signaled.items() if not _gone(p, b, table)}
                       | {p for p, b in unconfirmed.items() if not _gone(p, b, table)})
    ended = [{"pid": p, "command": shell_cmds.get(p) or described.get(p, ""), "killed": p in killed}
             for p in sorted(signaled) if p not in survivors]
    result = {"ok": not survivors, "session_id": session_id, "agent_id": agent_id, "name": name,
              "why": why, "ended": ended, "survivors": survivors, "excluded": sorted(excluded),
              "notes": notes, "seconds": round(time.monotonic() - t0, 3)}
    if ended or survivors:
        _log_end(result)
    return result


def _log_end(result):
    try:
        os.makedirs(state_dir(), mode=0o700, exist_ok=True)
        at = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        who = "%s (%s/%s)" % (result.get("name") or result["agent_id"], result["session_id"], result["agent_id"])
        with open(os.path.join(state_dir(), "ended.log"), "a") as out:
            for e in result["ended"]:
                out.write("%s ended %s pid %d%s: %s [%s]\n" % (at, who, e["pid"], " (SIGKILL)" if e["killed"] else "",
                                                              " ; ".join(e["command"].split("\n"))[:300], result.get("why") or "its run ended"))
            for pid in result["survivors"]:
                out.write("%s NOT ENDED %s pid %d [%s]\n" % (at, who, pid, result.get("why") or "its run ended"))
            for note in result.get("notes") or []:
                out.write("%s left running for %s: %s\n" % (at, who, note))
    except OSError:
        pass


def describe_end(result):
    name = result.get("name") or result.get("agent_id") or "the agent"
    if result.get("skipped"):
        return "END %s: nothing ended (%s)." % (name, result["skipped"])
    if result.get("ok") is False and not result.get("survivors"):
        return "END %s: NOT done (%s)." % (name, result.get("why"))
    line = "END %s: %d process(es) of its own ended" % (name, len(result.get("ended") or []))
    if result.get("survivors"):
        line += "; NOT ended: pid %s" % ", ".join(map(str, result["survivors"]))
    for note in result.get("notes") or []:
        line += "; " + note
    return line + "."


def end_command(session_id, agent_id, name=""):
    """The one command that ends this agent's own commands by hand (stall-watch names it)."""
    return "python3 %s end --session %s --agent %s%s" % (
        shlex.quote(os.path.realpath(__file__)), shlex.quote(session_id), shlex.quote(agent_id),
        (" --name " + shlex.quote(name)) if name else "")


def records():
    directory = os.path.join(state_dir(), "held")
    try:
        names = sorted(os.listdir(directory))
    except OSError:
        return []
    return [r for r in (_read_json(os.path.join(directory, n)) for n in names if n.endswith(".json")) if r]


def release_session(session_id):
    """Every hold of this session, released: a session that ends never leaves work frozen."""
    return [release(r["session_id"], r["agent_id"]) for r in records() if r.get("session_id") == session_id]


def release_orphans():
    """Holds whose session process is gone (recorded parent absent, or a different process now)."""
    table = snapshot()
    out = []
    for r in records():
        parents = r.get("parents") or {}
        if parents and not any(table.get(int(p), {}).get("birth") == b for p, b in parents.items()):
            out.append(release(r["session_id"], r["agent_id"]))
    return out


def describe_hold(result):
    name = result.get("name") or result.get("agent_id") or "the agent"
    if result.get("ok") is False and result.get("why"):
        return "HOLD %s: NOT held (%s); its running work was not suspended." % (name, result["why"])
    if not result.get("held"):
        return ("HOLD %s: no running command of it is recorded, so nothing was suspended (idle, or started "
                "before ownership capture was installed). Its next command returns the WAIT at once, and the "
                "message reaches it with that result." % name)
    line = "HOLD %s: %d process(es) suspended in %.2f s" % (name, len(result["held"]), result["stopped_seconds"])
    if result.get("detached"):
        line += ("; its foreground command is frozen and its tool call returned, so the WAIT reaches it now "
                 "(%d command(s))" % len(result["detached"]))
    if result.get("undetached"):
        line += ("; foreground shell(s) %s could not be told to return (no job started, or not a session "
                 "leader) and were frozen whole, so the WAIT reaches it only at that call's Bash timeout"
                 % ", ".join(map(str, result["undetached"])))
    if result.get("cpu_during_sample") is not None:
        line += "; CPU used by them over the next %.1f s: %.2f s" % (result["sample_seconds"], result["cpu_during_sample"])
    if result.get("not_stopped"):
        line += "; NOT confirmed stopped: %s" % ", ".join(map(str, result["not_stopped"]))
    for note in result.get("notes") or []:
        line += "; " + note
    if result.get("locks"):
        line += ("; they keep these locks until RESUME, so work elsewhere that needs one waits: %s"
                 % ", ".join(result["locks"][:6]) + (" and %d more" % (len(result["locks"]) - 6)
                                                     if len(result["locks"]) > 6 else ""))
    if not result.get("watchdog"):
        line += "; its watchdog did NOT start, so only a SessionStart releases it if the lead dies"
    return line + ". They continue on RESUME; until then its new commands are refused with the WAIT."


def describe_release(result):
    name = result.get("name") or "the agent"
    if result.get("why") == "nothing was held":
        return ""
    if result.get("ok") is False:
        return "RELEASE %s: FAILED (%s); %d process(es) continued: agent_hold.py status." % (
            name, result.get("why") or "no reason recorded", len(result.get("continued") or []))
    line = "RELEASE %s: %d process(es) continued after %.0f s held" % (
        name, len(result["continued"]), result.get("held_seconds") or 0)
    if result.get("waited"):
        line += ", including %d new command(s) that waited" % len(result["waited"])
    if result.get("gone"):
        line += "; %d had already ended" % len(result["gone"])
    return line + "."


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    for cmd in ("hold", "release", "watch", "end"):
        s = sub.add_parser(cmd)
        s.add_argument("--session", required=True)
        s.add_argument("--agent", required=True, help="the agent id recorded by the registry")
        s.add_argument("--name", default="")
    m = sub.add_parser("mark", help="record the calling shell without shell PID expansion")
    m.add_argument("--state", required=True)
    m.add_argument("stem")
    sub.add_parser("status")
    sub.add_parser("capture", help="print the ownership lines for a hook payload on stdin")
    w = sub.add_parser("wait", help="a paused agent's own call: returns RESUMED once its hold is released")
    w.add_argument("--max-seconds", type=float, default=None,
                   help="default: just under the Bash tool's timeout ceiling")
    a = p.parse_args(argv)
    if a.cmd == "mark":
        os.environ["RICHOS_AGENT_HOLD_DIR"] = a.state
        return mark(a.stem)
    if a.cmd == "wait":
        return wait_resume(a.max_seconds)
    if a.cmd == "watch":
        return watch(a.session, a.agent)
    if a.cmd == "hold":
        r = hold(a.session, a.agent, a.name)
        print(json.dumps(r, sort_keys=True))
        return 0 if r.get("ok") else 1
    if a.cmd == "end":
        r = end_owned(a.session, a.agent, a.name, "ended by hand (agent_hold.py end)")
        print(describe_end(r))
        return 0 if r.get("ok") else 1
    if a.cmd == "release":
        r = release(a.session, a.agent)
        print(json.dumps(r, sort_keys=True))
        return 0 if r.get("ok") else 1
    if a.cmd == "status":
        for r in records():
            print("%s (%s): %d held since %s" % (r.get("name") or r.get("agent_id"), r.get("session_id"),
                                                 len(r.get("held") or {}),
                                                 time.strftime("%H:%M:%SZ", time.gmtime(r.get("at") or 0))))
        return 0
    try:
        payload = json.load(sys.stdin)
    except ValueError:
        return 2
    sys.stdout.write(capture(payload))
    return 0


if __name__ == "__main__":
    sys.exit(main())

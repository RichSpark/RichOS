#!/usr/bin/env python3
"""stall_watch.py: THE LEAD IS WOKEN THE MOMENT TEAMMATE WORK STALLS.

===========================================================================
WHAT THIS EXISTS FOR
===========================================================================
The CEO, 2026-09-28, after three teammates made no commit for 70 to 85 minutes
while he was away: "everyone is fucking WAITING ALL THE FUCKING TIME AND
NOTHING EVER MOVES FORWARD", "WHY THE FUCK DO I ALWAYS HAVE TO MANAGE ALL THIS
FUCKSHIT??? IS THAT MY FUCKING JOB???" and "WHERE THE FUCK WAS THAT FUCKING
WATCHER BEFORE???". His build points (2026-09-25): "He never has to ask; every
step and failure reaches him unprompted." "A free Mac is used whole; nothing
waits in a line."

What happened: the three were queued behind the Mac's single proof-run slot
(limit 1, one holder, three waiting 32 to 41 minutes) while the Mac sat at 26
to 38% CPU, and one of them (buildguard1) was queued behind its OWN proof run.
Nothing woke the lead: no teammate finished, guard-resource-waits.sh fires only
when the lead ends a turn, and the lead had no turn. A watcher that must be
started by hand is the same defect one step later, so this one is started by
Claude Code itself with every interactive session: the engine's plugin monitor
(monitors/monitors.json), exactly as quota-watch is.

===========================================================================
WHAT IS A STALL (every threshold, and why)
===========================================================================
1. A RESOURCE WAIT OVER 10 MINUTES (RICHOS_RESOURCE_WAIT_MINUTES, the same
   number and the same variable as the turn-end gate, resource_waits.py).
   Read from the records waiters write themselves (resource_waits.py: CPU
   admission, the test VM slot, a go-file, and any other resource a waiter
   names, such as a device), from the escalation ledger's stated waits, and
   from the proof-run slot line itself (app/scripts/lib/proof_slots.py
   status), which says who holds each slot and where each waiter stands.
   A wait is recorded, never inferred from process names. A wait whose
   teammate the lead holds (agent_hold.py) is not a stall while the hold
   stands, and the held time is never counted (resource_waits.py, A HELD
   AGENT IS NOT WAITING IN LINE).

2. A SILENT TEAMMATE: a live teammate of THIS session (the workspace registry
   says its run has not ended and it is not paused) with no new commit on any
   of its workspaces AND no new write to its transcript for 20 minutes
   (STALL_WATCH_SILENT_MINUTES). Why 20: a working teammate writes its
   transcript at every tool result. The longest honest gap between two is one
   tool call: the Bash tool's ceiling is 600 s (a longer foreground command is
   moved to the background at 600 s and the move is a result), and a terminal
   teammate waiting on a background command runs `agent_hold.py wait`, which returns at
   most every 270 s. (An app teammate does not run that wait since 2026-10-30: it keeps
   working or ends its turn to await the host's notice, and an ended turn with its own
   command still running is the deferred end below.) So a long honest test run keeps writing the transcript at
   least every ten minutes however long the run itself takes, and twenty is
   twice the longest honest silence. A commit is not required: a teammate that
   is working but has not committed for an hour is not silent.
   When its turn ended but the registry deferred its end because its own
   command still runs (workspaces.py record_end; 2026-10-11, four endless wait
   loops of echo-opus-walk49fix1), the notice says so and names the one
   command that ends its own commands (agent_hold.py end), never a pid hunt.
   Once those commands have sat idle for 10 minutes the watcher runs that end
   itself (IDLE END below).

3. A SELF-WAIT: a teammate process waiting for a proof-run slot or a test VM
   slot that another process of the SAME teammate holds, reported after 2
   minutes (STALL_WATCH_SELF_WAIT_MINUTES): the Mac getting freer never ends
   it, only its own other run ending does. Ownership is read, in order, from
   the process's inherited owner tag (agent_hold.py's RICHOS_AGENT_OWNER,
   captured at spawn), from the registered workspace its recorded working
   directory is in, and from the waiter name the slot library recorded. When
   the holder is the waiter's own ANCESTOR it is a DEADLOCK: a nested run that
   did not borrow its caller's slot, which only the 3-hour timeout ends.

4. A TEAMMATE ESCALATION (2026-10-01; its own section below, ESCALATION). Not a
   stall: a `proceeding` or `work-complete` escalation is a record, and the
   ledger's design depends on never calling it one. It is told here because
   this monitor is the one thing that can start a lead turn: on 2026-10-01 a
   teammate's question that needed the CEO at the test iPhone waited 82
   minutes for a turn that no hook could start. It prints its own
   ESCALATION-WATCH block, first in the look's output.

5. DUE WORK (orchestration reliability R2, richos-hq docs/prds/2026-10-23-
   orchestration-reliability.md): finished work with no wait, and a hold or wait
   with no checkable end or whose end came true (workspaces.due_set, the same
   calculation the Stop fallback and `workspaces.sh holds` use). Told at the
   first look while the lead is idle, one line per item and at most eight, then
   a pointer to the complete list; again after the 30-minute repeat while it
   stays due; said nothing when it stops being due. An item the SessionStart
   notice already told this session (record_told_due) is not told again at the
   first look unless its state changed. A busy lead is told at its
   next idle look. Idle is the lead's recorded turn end; under the app host it is
   the host's word that the lead's turn ended (host_child.idle). A pending end (a
   live agent, a running review, a time not reached) and a CEO-TODOs item are
   quiet.

===========================================================================
EACH FINDING GOES ONLY TO THE CHAT WHOSE WORK IT IS (2026-10-28)
===========================================================================
Silent teammates, slow steps and due work were always read from this session's
own records. The waits (1 and 3) were not: they are read machine-wide, so in the
app, where every conversation runs its own watcher, a SELF-WAIT of a nightly
test-VM walk (agent-abbf962a73586890c) was told in the RichSpark avatar chat,
whose lead then carried another chat's problem. A wait is now told only in the
session that owns the waiting process (Registry.owning_session): its session tag
(agent_hold.py exports RICHOS_AGENT_SESSION with RICHOS_AGENT_OWNER on every
subagent Bash call), else its owner tag through this registry (in the app, one
conversation's partition, so an agent it does not hold is another chat's), else
the lead's own RICHOS_SESSION_ID, else the registered workspace it runs in, else
a waiter name that is exactly one record's. A wait whose owner nothing names is
still told, as before: unattributed is never silently dropped.

===========================================================================
HOW OFTEN IT SPEAKS (nothing spams)
===========================================================================
It looks every 60 s (STALL_WATCH_POLL_SECONDS). A stall is announced ONCE when
it is first seen, again only when it has lasted another 30 minutes since the
last notice (STALL_WATCH_REPEAT_MINUTES), and once more, in one line, when it
has been gone for two consecutive looks (one look that misses it is not an end:
a source can fail to read once). Everything due in one look is ONE notification
block. What was announced is kept in its session's state directory, so a
restarted monitor does not repeat itself.

===========================================================================
IT REPORTS, AND ENDS ONE THING
===========================================================================
It never stops, pauses or messages a teammate, and it takes no lock that
anything else waits for (the slot library's own status probe is a shared,
non-blocking lock dropped at once). Each notice names the teammate, what it
waits on, who holds it, since when, and what the lead can do. The lead acts.
The one exception (2026-10-14, IDLE END below): a teammate whose turn ended
but whose end its own commands defer, when those commands have used no CPU to
speak of for 10 minutes (STALL_WATCH_IDLE_END_MINUTES), has them ended through
agent_hold.py's own ownership record, and one line says what was ended.

===========================================================================
COMMANDS (stall-watch.sh passes --engine-root, --config and --command)
===========================================================================
  --monitor  the plugin monitor's body: runs for the whole session and prints
             each notification block; ends with its session
  --alive    one line: is this session watched, and when did it last look.
             0 watched; 1 nothing is watching, the watcher stopped looking,
             its last look failed, or it has not yet evaluated the source on
             disk; 2 no session here to check
  --watch    the fallback where plugin monitors do not run: the same loop, run
             as a BACKGROUND command; it exits after the first notification,
             so start it again after each one
  --once     every stall right now, whatever was announced; 0 none, 1 some,
             2 could not look at all. Keeps no state
  --notice   the SessionStart notice body (session-start-stall.sh wraps it)
  --tick     one look against the session's state, printing what the monitor
             would print (the test suite's clock is STALL_WATCH_NOW). The
             monitor runs every look this way, as a fresh child process
             (EVERY LOOK IS FRESH CODE below)
  --host-json  with --monitor: the RichOS app host's own child (richos-core
             team_watch.rs), from the app's launch to its quit. Each look is
             for the session of his team's lead, which the host names
             (host_child.py), and nothing is looked at while no lead runs;
             each block is one JSON line the host sends to that lead
"""

import argparse
import calendar
import datetime as _dt
import fcntl
import glob
import hashlib
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import host_child  # noqa: E402  (sibling in scripts/lib: the app host's child)
import resource_waits  # noqa: E402  (sibling in scripts/lib)
import escalations  # noqa: E402  (sibling in scripts/lib)

POLL_SECONDS = 60
SILENT_MINUTES = 20
SELF_WAIT_MINUTES = 2
REPEAT_MINUTES = 30
CLEAR_AFTER_MISSES = 2
DUE_SHOWN = 8              # due items shown one per line; the rest are one pointer line
STATE_KEEP_SECONDS = 2 * 86400
PROOF_SLOT_REASON = "proof-run: waiting for a proof-run slot"
OWNER_TAG = "RICHOS_AGENT_OWNER"
SESSION_TAG = "RICHOS_AGENT_SESSION"   # agent_hold.py exports both with every subagent Bash call
LEAD_SESSION_ENV = "RICHOS_SESSION_ID"  # the app sets it for its lead's whole lease; children inherit it
OTHER_CHAT = "\0another chat"           # an owner that is provably not any session this registry names
GIT_SECONDS = 5
CPU_LINE = 80
IDLE_END_MINUTES = 10      # a deferred end's own commands idle this long are ended (IDLE END below)
IDLE_CPU_PERCENT = 1.0     # "idle": under this share of one core between two looks
IDLE_CPU_FLOOR = 0.05      # CPU seconds always counted as idle between two looks (clock resolution)
IDLE_LOOK_GAP = 3 * POLL_SECONDS  # a look later than this after the previous one starts the idle count again


# ---------------------------------------------------------------------------
# small utilities
# ---------------------------------------------------------------------------

def _env_float(name, default):
    raw = (os.environ.get(name) or "").strip()
    try:
        value = float(raw) if raw else float(default)
    except ValueError:
        value = float(default)
    return value if value > 0 else float(default)


def clock():
    """Now, as stalls are judged. STALL_WATCH_NOW (epoch seconds) exists for
    stall-watch.test.sh, which cannot make a real process wait ten minutes;
    nothing else sets it. Process identities are always read against the real
    clock, so moving this cannot bring a dead wait back."""
    raw = (os.environ.get("STALL_WATCH_NOW") or "").strip()
    try:
        return float(raw) if raw else time.time()
    except ValueError:
        return time.time()


def hhmm(epoch):
    return _dt.datetime.fromtimestamp(epoch, _dt.timezone.utc).strftime("%H:%MZ")


def hms(epoch):
    return _dt.datetime.fromtimestamp(epoch, _dt.timezone.utc).strftime("%H:%M:%SZ")


def span(seconds):
    return resource_waits.minutes(max(0, seconds))


def parse_iso(text):
    try:
        return _dt.datetime.strptime(str(text), "%Y-%m-%dT%H:%M:%SZ").replace(
            tzinfo=_dt.timezone.utc).timestamp()
    except (TypeError, ValueError):
        return None


def _short(command, limit=100):
    return resource_waits.short(command or "", limit)


def _read_json(path):
    try:
        with open(path, encoding="utf-8") as fh:
            v = json.load(fh)
        return v if isinstance(v, dict) else {}
    except (OSError, ValueError):
        return {}


def _write_json(path, data):
    tmp = "%s.%d.tmp" % (path, os.getpid())
    try:
        with open(tmp, "w", encoding="utf-8") as fh:
            json.dump(data, fh)
        os.replace(tmp, path)
    except OSError:
        try:
            os.unlink(tmp)
        except OSError:
            pass


def _load(name, path):
    """A module from a file path, or None. Never raises."""
    if not path or not os.path.isfile(path):
        return None
    try:
        spec = importlib.util.spec_from_file_location(name, path)
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        return mod
    except Exception:  # noqa: BLE001: a module that cannot load is "not read", reported
        return None


# ---------------------------------------------------------------------------
# the sources: registry, proof-run slots, the admission sampler, ownership
# ---------------------------------------------------------------------------

class Sources(object):
    """Everything a look reads, loaded once per process."""

    def __init__(self, engine_root):
        self.engine_root = engine_root or os.path.dirname(os.path.dirname(HERE))
        self.ws = _load("stall_watch_workspaces", os.path.join(self.engine_root, "mega-lander", "workspaces.py"))
        app = os.path.join(os.path.dirname(os.path.realpath(self.engine_root)), "app", "scripts")
        self.proof_slots_path = os.path.join(app, "lib", "proof_slots.py")
        self.proof_slots = _load("stall_watch_proof_slots", self.proof_slots_path)
        self._reserve_path = os.path.join(app, "testvm", "reserve.py")
        self._reserve = None
        try:
            import agent_hold  # noqa: WPS433 (sibling in scripts/lib)
            self.agent_hold = agent_hold
        except Exception:  # noqa: BLE001
            self.agent_hold = None
        self.transcripts = {}

    def cpu_busy(self):
        """(percent, how) from the admission rule's own one-second sample, or
        (None, why). STALL_WATCH_CPU_BUSY pins it for the test suite."""
        pinned = (os.environ.get("STALL_WATCH_CPU_BUSY") or "").strip()
        if pinned:
            try:
                return float(pinned), "pinned"
            except ValueError:
                pass
        if self._reserve is None:
            self._reserve = _load("stall_watch_reserve", self._reserve_path) or False
        if not self._reserve:
            return None, "the admission sampler (app/scripts/testvm/reserve.py) is not beside this engine"
        try:
            s = self._reserve.host_sample(1.0)
            return s["cpu_user_percent"] + s["cpu_system_percent"], "sampled"
        except Exception as exc:  # noqa: BLE001: unmeasured is said, never guessed
            return None, "the CPU could not be sampled (%s)" % exc

    def session(self):
        """(session id, claude pid) of the session this runs in, or ('', None)."""
        if self.ws is None:
            return "", None
        try:
            sid = self.ws.current_session() or ""
            pid = self.ws.session_pid(sid) if sid else self.ws.session_pid()
        except Exception:  # noqa: BLE001
            return "", None
        return sid, pid

    def tags(self, pid):
        """{name: value} of the ownership tags in the process's readable environment
        (OWNER_TAG, SESSION_TAG, the lead's RICHOS_SESSION_ID); {} when it shows none."""
        if self.agent_hold is None or not pid:
            return {}
        try:
            env = self.agent_hold._environment(int(pid)) or []
        except Exception:  # noqa: BLE001
            return {}
        out = {}
        for name in (OWNER_TAG, SESSION_TAG, LEAD_SESSION_ENV):
            want = (name + "=").encode()
            for item in env:
                if item.startswith(want):
                    out[name] = item[len(want):].decode("utf-8", "replace")
                    break
        return out

    def owner_tag(self, pid):
        """The agent id in the process's inherited owner tag, or ''."""
        return self.tags(pid).get(OWNER_TAG, "")


class Registry(object):
    """The workspace registry, read once per look: names by agent id and by path."""

    def __init__(self, src):
        self.src = src
        self.recs = []
        self.problem = ""
        self.by_agent = {}
        self.paths = []
        if src.ws is None:
            self.problem = "the workspace registry library (mega-lander/workspaces.py) could not be loaded"
            return
        try:
            self.recs = src.ws.all_agents()
        except Exception as exc:  # noqa: BLE001
            self.problem = "the workspace registry could not be read (%s)" % exc.__class__.__name__
        for r in sorted(self.recs, key=lambda r: str(r.get("registered_at") or "")):
            if r.get("agent_id"):
                self.by_agent[str(r["agent_id"])] = r
            for w in r.get("workspaces") or []:
                if w.get("path") and not w.get("deleted_at"):
                    self.paths.append((os.path.realpath(w["path"]), r))
        self.paths.sort(key=lambda p: len(p[0]), reverse=True)
        self.names = set(str(r.get("name") or "") for r in self.recs)

    def name_for_path(self, cwd):
        if not cwd:
            return ""
        real = os.path.realpath(cwd)
        for path, r in self.paths:
            if real == path or real.startswith(path + os.sep):
                return str(r.get("name") or r.get("key") or "")
        return ""

    def name_for_waiter(self, waiter):
        """A recorded waiter name as a teammate name: a registry name as is,
        `agent-<id>` through the registry, anything path-like is no name."""
        waiter = str(waiter or "")
        if not waiter or waiter.startswith(os.sep) or waiter.startswith("pid "):
            return ""
        if waiter.startswith("agent-") and waiter[len("agent-"):] in self.by_agent:
            return str(self.by_agent[waiter[len("agent-"):]].get("name") or waiter)
        return waiter

    def owner(self, pid, cwd="", waiter=""):
        """(teammate name, how it is known) for a process, or ('', '')."""
        tag = self.src.owner_tag(pid)
        if tag:
            rec = self.by_agent.get(tag)
            return (str(rec.get("name")) if rec and rec.get("name") else "agent-" + tag), "its owner tag"
        name = self.name_for_path(cwd)
        if name:
            return name, "its working directory"
        name = self.name_for_waiter(waiter)
        if name:
            return name, "the name its slot record carries"
        return "", ""

    def owning_session(self, pid=None, cwd="", waiter="", worktree="", session=""):
        """The session (the chat, in the app) whose work a waiting process or a ledger wait
        is: '' when nothing says, OTHER_CHAT for an agent this registry does not hold.

        Read in order: the session tag every subagent Bash call carries (agent_hold.py), the
        owner tag through this registry to its record's session, the lead's own session in
        its environment, then the registered workspace its working directory (or the
        ledger row's worktree) is in, then the session the ledger row itself was raised in
        (`session`: another chat's registry holds neither its workspace nor its name, so
        without it an unrelated chat took the wait as unattributed and was woken by it),
        then a waiter name that is exactly one record's."""
        tags = self.src.tags(pid) if pid else {}
        if tags.get(SESSION_TAG):
            return tags[SESSION_TAG]
        if tags.get(OWNER_TAG):
            rec = self.by_agent.get(tags[OWNER_TAG])
            # In the app the registry is one conversation's partition: an agent it does not
            # hold is another conversation's (2026-10-28, a nightly walk told in the avatar chat).
            return str(rec.get("session_id") or OTHER_CHAT) if rec else OTHER_CHAT
        if tags.get(LEAD_SESSION_ENV):
            return tags[LEAD_SESSION_ENV]
        for where in (cwd, worktree):
            if where:
                real = os.path.realpath(where)
                for path, rec in self.paths:
                    if real == path or real.startswith(path + os.sep):
                        return str(rec.get("session_id") or "")
        if session:
            return str(session)
        name = self.name_for_waiter(waiter)
        named = [r for r in self.recs if name and str(r.get("name") or "") == name]
        if len(named) == 1:
            return str(named[0].get("session_id") or "")
        return ""


# ---------------------------------------------------------------------------
# one stall
# ---------------------------------------------------------------------------

class Stall(object):
    def __init__(self, key, kind, who, since, what, lines, action):
        self.key, self.kind, self.who, self.since = key, kind, who, since
        self.what, self.lines, self.action = what, list(lines), list(action)

    def render(self, now, notice_no=1, first_at=None):
        head = "  [%s] %s" % (self.kind, self.headline(now))
        if notice_no > 1:
            head += "  (still, notice %d; first announced %s)" % (notice_no, hhmm(first_at or now))
        out = [head]
        out += ["      " + ln for ln in self.lines]
        for i, a in enumerate(self.action):
            out.append(("      You can: " if i == 0 else "               ") + a)
        return out

    def headline(self, now):
        return "%s: %s, for %s (since %s)" % (self.who, self.what, span(now - self.since), hhmm(self.since))


# ---------------------------------------------------------------------------
# the look
# ---------------------------------------------------------------------------

def thresholds():
    return {"wait": resource_waits.wait_minutes() * 60,
            "silent": _env_float("STALL_WATCH_SILENT_MINUTES", SILENT_MINUTES) * 60,
            "self": _env_float("STALL_WATCH_SELF_WAIT_MINUTES", SELF_WAIT_MINUTES) * 60,
            "repeat": _env_float("STALL_WATCH_REPEAT_MINUTES", REPEAT_MINUTES) * 60}


def _ancestors(pid, table):
    out, seen = [], set()
    p = table.get(pid) if table else None
    while p is not None and p.ppid not in seen and p.ppid > 1:
        seen.add(p.ppid)
        out.append(p.ppid)
        p = table.get(p.ppid)
    return out


def _proc_words(pid, table):
    p = table.get(pid) if table else None
    return "`%s`" % _short(p.command, 90) if p is not None else "(command not read)"


def _slot_lines(src, now):
    """(status dict or None, problem)."""
    if src.proof_slots is None:
        if not os.path.isfile(src.proof_slots_path):
            return None, ""  # Optional capability not shipped with a standalone engine.
        return None, "the proof-run slot library could not be loaded"
    try:
        return src.proof_slots.status(), ""
    except Exception as exc:  # noqa: BLE001
        return None, "the proof-run slots could not be read (%s)" % exc


def look(src, now, session_id, th=None, due=True):
    """(stalls, problems): every stall right now. Never raises for a source it
    cannot read; that source is named in problems and the rest is still read.
    `due` False leaves the due work out (the lead is busy: told at its next idle look)."""
    th = th or thresholds()
    reg = Registry(src)
    problems = [reg.problem] if reg.problem else []
    stalls = []

    def ours(pid=None, cwd="", waiter="", worktree="", session=""):
        """A wait is told only to the session (in the app, the chat) whose work it is
        (Registry.owning_session); one whose owner nothing names is told, never lost."""
        owner = reg.owning_session(pid, cwd, waiter, worktree, session)
        return not owner or not session_id or owner == session_id
    deadline = time.time() + resource_waits.BUDGET_SECONDS
    table = None

    # -- proof-run slots: the line, its holders, and whose each one is --------
    slots, why = _slot_lines(src, now)
    if why:
        problems.append(why)
    # -- every recorded wait (and the ledger's); a held agent's is skipped ----
    reading = resource_waits.collect(now=now, threshold_seconds=min(th["self"], th["wait"]))
    problems += reading["problems"]

    slot_waiter_pids = set()
    watched = []  # (kind, waiter dict, holders)
    if slots:
        for w in slots.get("waiting") or []:
            try:
                slot_waiter_pids.add(int(w.get("pid")))
            except (TypeError, ValueError):
                pass
            since = w.get("since") if isinstance(w.get("since"), (int, float)) else now
            # A teammate the lead holds is not waiting in line (resource_waits.py,
            # A HELD AGENT IS NOT WAITING IN LINE); its clock resumes on RESUME.
            pw = {"pid": w.get("pid"), "waiter": w.get("waiter"), "since": since,
                  "names": [reg.owner(w.get("pid"), w.get("cwd") or "", w.get("waiter") or "")[0]]
                  if reading["holds"] else []}
            resource_waits.apply_holds([pw], reading["holds"], now)
            if pw["held_by"]:
                continue
            if now - pw["since"] >= min(th["self"], th["wait"]) and ours(
                    w.get("pid"), w.get("cwd") or "", w.get("waiter") or ""):
                watched.append(("proof", dict(w, since=pw["since"], began=since), slots.get("holders") or []))
    # A proof-run slot waiter also records itself as a CPU-admission wait; with
    # the slot line read, that record is the same wait and is told once.
    others = [w for w in reading["waits"]
              if not (w.get("pid") in slot_waiter_pids or
                      (slots is not None and w["source"] == "record" and w["reason"].startswith(PROOF_SLOT_REASON)))]

    if watched or any(now - w["since"] >= min(th["self"], th["wait"]) for w in others):
        table, why = resource_waits.ps_table(deadline)
        if table is None:
            problems.append(why)

    for _kind, w, holders in watched:
        stalls += _proof_stall(src, reg, w, holders, slots, now, th, table)

    vm_slots = None
    for w in others:
        age = now - w["since"]
        if age < min(th["self"], th["wait"]) or not ours(w.get("pid"), "", w.get("waiter") or "",
                                                         w.get("worktree") or "", w.get("session") or ""):
            continue
        if w["resource"] == resource_waits.VM and w["source"] == "record":
            if vm_slots is None:
                _lines, vm_slots, _g = resource_waits.vm_facts(resource_waits.testvm_root(), table, reading["waits"])
            own = _vm_self_wait(src, reg, w, vm_slots, table)
            if own and age >= th["self"]:
                stalls.append(own)
                continue
        if age < th["wait"]:
            continue
        stalls.append(_wait_stall(reg, w, now, table, reading["waits"]))

    # -- silent teammates of this session ------------------------------------
    s, why = _silent(src, reg, now, session_id, th, reading["waits"])
    stalls += s
    if why:
        problems.append(why)
    stalls += _slow_steps(src, reg, now, session_id)
    if due:
        s, why = _due(src, reg, now, session_id)
        stalls += s
        if why:
            problems.append(why)
    return stalls, [p for p in problems if p]


# ---------------------------------------------------------------------------
# DUE WORK (orchestration reliability R2; point 5 of the module doc)
# ---------------------------------------------------------------------------

def _due(src, reg, now, session_id):
    """([DUE stalls], problem). One stall per due item, keyed by the work and its
    material state, so an unchanged item is told once and again only at the repeat."""
    if src.ws is None or reg.problem or not hasattr(src.ws, "due_set"):
        return [], ""
    try:
        items = src.ws.pending(session_id, auto=False, dry=True)
        due = src.ws.due_set(session_id, items, now)
    except Exception as exc:  # noqa: BLE001: an unread list is a source not read, never "nothing due"
        return [], "the due work could not be read (%s: %s)" % (exc.__class__.__name__, str(exc)[:160])
    out = []
    for d in due[:DUE_SHOWN]:
        why = " ".join(str(d.get("why") or "not said").split())[:200]
        out.append(Stall(due_key(d), "DUE",
                         d["name"], d["since"], d["detail"], ["why: " + why], ["next: " + d["command"]]))
    if len(due) > DUE_SHOWN:
        out.append(Stall(more_key(due), "DUE", "%d more" % (len(due) - DUE_SHOWN), now, "more due work", [],
                         [src.ws.due_list_pointer() if hasattr(src.ws, "due_list_pointer")
                          else "the complete list: workspaces.sh holds"]))
    # One damaged record never hides the readable due work above: it is one compact problem.
    bad = src.ws.unreadable_line() if hasattr(src.ws, "unreadable_line") else ""
    return out, bad


def due_key(d):
    """A due item's announce key: its work, its condition (source and review) and its state
    and end (its material state). Finished work that a changes-requested review then
    reaches is the same work under another condition, so it is told."""
    return "due:%s:%s:%s:%s:%s" % (d.get("key") or d["name"], d.get("source") or "", d.get("review") or "",
                                   d["state"], d.get("end") or "")


def more_key(due):
    """The "more due" pointer's announce key: a digest of the hidden items' own keys, so a
    changed set behind the pointer (another item due, a different one) is told at the next
    look and an unchanged set stays quiet until the repeat."""
    hidden = "\n".join(due_key(d) for d in due[DUE_SHOWN:])
    return "due:more:" + hashlib.sha1(hidden.encode("utf-8")).hexdigest()[:12]


def record_told_due(sid, due, now=None):
    """The SessionStart notice (workspaces.lifecycle) told these due items: they are
    recorded in this session's announced.json under the key the first look makes, so
    that look does not tell them again unchanged. A changed item has another key. Past
    DUE_SHOWN items the notice showed the "more due" pointer, recorded under more_key."""
    if not sid or not due:
        return
    now = clock() if now is None else now
    told = [(due_key(d), d["since"], d["name"], d["detail"]) for d in due[:DUE_SHOWN]]
    if len(due) > DUE_SHOWN:
        told.append((more_key(due), now, "%d more" % (len(due) - DUE_SHOWN), "more due work"))
    sd = session_dir(sid)
    path = os.path.join(sd, "announced.json")
    with announced_lock(sd):
        state = _read_json(path)
        opened = dict(state.get("open") or {})
        for key, since, who, what in told:
            prev = opened.get(key) or {"first": now, "count": 1, "since": since, "who": who, "what": what}
            prev.update(last=now, missed=0)
            opened[key] = prev
        state["open"] = opened
        _write_json(path, state)


class announced_lock(object):
    """A short exclusive lock around one read-modify-write of a session's announced.json.
    Its two writers (the tick's announce decision and write, record_told_due) each read the
    file inside it, so neither decides from or writes back a copy older than the other's
    write. Never held across a look. A lock that cannot be opened is not taken."""

    def __init__(self, sd):
        self.path, self.fd = os.path.join(sd, "announced.lock"), None

    def __enter__(self):
        try:
            self.fd = os.open(self.path, os.O_RDWR | os.O_CREAT, 0o600)
            fcntl.flock(self.fd, fcntl.LOCK_EX)
        except OSError:
            pass
        return self

    def __exit__(self, *exc):
        if self.fd is not None:
            os.close(self.fd)
        return False


def _proof_stall(src, reg, w, holders, slots, now, th, table):
    pid = w.get("pid")
    since = w.get("since") if isinstance(w.get("since"), (int, float)) else now
    who, how = reg.owner(pid, w.get("cwd") or "", w.get("waiter") or "")
    who = who or str(w.get("waiter") or "pid %s" % pid)
    limit = slots.get("limit")
    held = []
    mine = []
    for h in holders:
        hname, hhow = reg.owner(h.get("pid"), h.get("cwd") or "", h.get("waiter") or "")
        hsince = h.get("since") if isinstance(h.get("since"), (int, float)) else now
        desc = "%s (pid %s, %s) for %s since %s" % (hname or h.get("waiter") or "?", h.get("pid"),
                                                    _proc_words(h.get("pid"), table), span(now - hsince), hhmm(hsince))
        held.append(desc)
        if hname and hname == who:
            mine.append((h, hhow))
        elif table and isinstance(h.get("pid"), int) and h["pid"] in _ancestors(pid, table):
            mine.append((h, "its process ancestry"))
    ps = src.proof_slots_path
    place = "#%s in line, this Mac runs %s at once" % (w.get("position", "?"), limit)
    key = "wait:proof:%s:%d" % (pid, int(w.get("began", since)))
    if mine and now - since >= th["self"]:
        h, hhow = mine[0]
        deadlock = bool(table) and isinstance(h.get("pid"), int) and h["pid"] in _ancestors(pid, table)
        lines = ["waiting: pid %s %s (%s)" % (pid, _proc_words(pid, table), place),
                 "held by: pid %s %s, which is ALSO %s's (known from %s)" % (
                     h.get("pid"), _proc_words(h.get("pid"), table), who, hhow)]
        if deadlock:
            lines.append("DEADLOCK: the holder is the waiter's own ancestor, so the waiter is never admitted while "
                         "it waits: a nested run that did not borrow its caller's slot (RICHOS_PROOF_RUN_SLOT_HELD). "
                         "Only its 3-hour timeout ends it.")
        else:
            lines.append("It waits on itself: the Mac getting freer never ends this wait, only its own run ending does.")
        action = ["tell %s to run it inside its proof run, or after that run ends" % who]
        if deadlock:
            action.append("or have %s end the nested wait (pid %s) and rerun it through its caller's slot" % (who, pid))
        elif isinstance(limit, int):
            action.append("or, if the Mac has headroom: python3 %s set %d  (`set default` puts it back)" % (ps, limit + 1))
        return [Stall(key, "SELF-WAIT", who, since, "waits for a proof-run slot its own process holds", lines, action)]
    if now - since < th["wait"]:
        return []
    lines = ["waiting: pid %s %s (%s)" % (pid, _proc_words(pid, table), place),
             "held by: " + ("; ".join(held) if held else "nobody holds a slot right now (it should be admitted at its next poll)")]
    busy, how = src.cpu_busy()
    action = []
    if busy is None:
        lines.append("Mac CPU: unknown (%s)" % how)
        action.append("check the CPU, then if there is headroom: python3 %s set %s" % (
            ps, (limit + 1) if isinstance(limit, int) else "N"))
    elif busy < CPU_LINE:
        lines.append("Mac CPU now: %.0f%% busy, under the %d%% admission line: there is headroom" % (busy, CPU_LINE))
        if isinstance(limit, int) and limit < (os.cpu_count() or 1):
            action.append("python3 %s set %d  (lets the next run in now; `set default` puts it back)" % (ps, limit + 1))
        action.append("or reorder: ask the holder's owner whether its run can end sooner")
    else:
        lines.append("Mac CPU now: %.0f%% busy, at or over the %d%% line: another slot would not run faster" % (
            busy, CPU_LINE))
        action.append("reorder: defer or end the holder's run (%s), or tell %s to do other work meanwhile" % (
            "; ".join("pid %s" % h.get("pid") for h in holders) or "no holder", who))
    action.append("the whole line: python3 %s status" % ps)
    return [Stall(key, "WAIT", who, since, "waits for a proof-run slot", lines, action)]


def _vm_self_wait(src, reg, w, vm_slots, table):
    who, _how = reg.owner(w["pid"], "", w["waiter"])
    if not who:
        return None
    for s in vm_slots or []:
        if not s.get("pid"):
            continue
        hname, hhow = reg.owner(s["pid"], "", "")
        anc = bool(table) and s["pid"] in _ancestors(w["pid"], table)
        if (hname and hname == who) or anc:
            lines = ["waiting: pid %s %s" % (w["pid"], _proc_words(w["pid"], table)),
                     "held by: %s held by pid %s %s, which is ALSO %s's (known from %s)" % (
                         s["name"], s["pid"], _proc_words(s["pid"], table), who,
                         hhow or "its process ancestry")]
            return Stall("wait:rec:%d:%d" % (w["pid"], int(w.get("began", w["since"]))), "SELF-WAIT", who, w["since"],
                         "waits for the test VM that its own process holds", lines,
                         ["tell %s to let its own walk finish first, or to end the walk it no longer needs" % who,
                          "every current wait: python3 %s list" % os.path.join(HERE, "resource_waits.py")])
    return None


def _wait_stall(reg, w, now, table, waits):
    label = resource_waits.LABELS.get(w["resource"], w["resource"])
    name = reg.name_for_waiter(w["waiter"]) or w["waiter"]
    rw = os.path.join(HERE, "resource_waits.py")
    lines = []
    if w["source"] == "record":
        lines.append("waiting: pid %d %s" % (w["pid"], _proc_words(w["pid"], table)))
        key = "wait:rec:%d:%d" % (w["pid"], int(w.get("began", w["since"])))
    else:
        lines.append("from the escalation ledger: %s" % ", ".join(w.get("escalations") or []))
        key = "wait:esc:%s:%s" % (w["waiter"], w["resource"])
    if w.get("reason"):
        lines.append("last refusal: %s" % w["reason"][:220])
    action = []
    if w["resource"] == resource_waits.VM:
        facts = resource_waits.vm_facts(resource_waits.testvm_root(), table, waits)[0]
        lines += ["held by: " + f for f in facts]
        action.append("reclaim a slot whose holder is not using it, or stop or defer the holder")
    elif w["resource"] == resource_waits.CPU:
        facts = resource_waits.cpu_facts(table)
        lines += ["CPU: " + f for f in facts]
        action.append("stop or defer the largest CPU user above, or tell %s to do other work meanwhile" % name)
    else:
        lines.append("held by: not recorded by the waiter")
        action.append("find the holder of %s, or tell %s to do other work meanwhile" % (label, name))
    action.append("every current wait: python3 %s list" % rw)
    return Stall(key, "WAIT", name, w["since"], "waits for %s" % label, lines, action)


def _git_last_commit(path):
    """(committer epoch, short sha, branch) of the workspace's HEAD, or None."""
    if not path or not os.path.isdir(path):
        return None
    try:
        r = subprocess.run(["git", "-C", path, "log", "-1", "--format=%ct %h", "HEAD"],
                           capture_output=True, text=True, timeout=GIT_SECONDS)
    except (OSError, subprocess.TimeoutExpired):
        return None
    parts = r.stdout.strip().split(" ")
    if r.returncode != 0 or len(parts) < 2 or not parts[0].isdigit():
        return None
    return int(parts[0]), parts[1]


def _transcript(src, rec):
    aid = str(rec.get("agent_id") or "")
    if not aid:
        return ""
    # Resolve again: an account move can leave the cached file intact but stale.
    try:
        path = src.ws.platform_agent_transcript(rec) or ""
    except Exception:  # noqa: BLE001
        path = ""
    if path:
        src.transcripts[aid] = path
    return path


def _running(src, rec, now):
    """Up to three of the teammate's own recorded commands still running."""
    ah = src.agent_hold
    if ah is None or not rec.get("agent_id") or not rec.get("session_id"):
        return []
    try:
        table = ah.snapshot()
        calls = ah.calls(str(rec["session_id"]), str(rec["agent_id"]), table)
    except Exception:  # noqa: BLE001
        return []
    out = []
    for c in sorted(calls, key=lambda c: c["at"])[:3]:
        out.append("running: `%s` (pid %d, for %s)" % (_short(c["command"], 90), c["pid"], span(time.time() - c["at"])))
    return out


def _deferred_end(src, rec, running, marks):
    """Epoch of the SubagentStop the registry did not take as this agent's end because
    its own commands still ran, when that is still why it has not ended: it is the
    latest thing the agent did (nothing written or committed after it) and a command
    of its own still runs. Else None."""
    fact = getattr(src.ws, "END_DEFERRED_FACT", None)
    if not running or not fact or src.agent_hold is None:
        return None
    at = None
    for h in rec.get("history") or []:
        if isinstance(h, dict) and h.get("fact") == fact:
            t = parse_iso(h.get("at"))
            if t is not None and (at is None or t > at):
                at = t
    if at is None or max(marks) > at + 60:
        return None
    return at


def _silent(src, reg, now, session_id, th, waits):
    if src.ws is None or reg.problem:
        return [], ""
    if not session_id:
        return [], "no session here, so no teammate of it can be named"
    out = []
    unread = []
    cache = {}
    for rec in reg.recs:
        if rec.get("session_id") != session_id or rec.get("disposition"):
            continue
        if not (rec.get("agent_id") or rec.get("started_at")):
            continue                       # preparation or a refused call is not a silent worker
        try:
            fin, paused, _why = src.ws.finished_state(rec, cache)
        except Exception:  # noqa: BLE001
            continue
        if fin or paused:
            continue
        name = str(rec.get("name") or rec.get("key") or "?")
        start = None
        for k in ("started_at", "spawned_at", "registered_at"):
            start = parse_iso(rec.get(k))
            if start is not None:
                break
        marks = [start or now]
        commit_line = "no commit since it started"
        best = None
        for w in rec.get("workspaces") or []:
            if w.get("deleted_at"):
                continue
            c = _git_last_commit(w.get("path"))
            if c and (best is None or c[0] > best[0][0]):
                best = (c, w.get("branch") or "?")
        if best and start is not None and best[0][0] > start:
            marks.append(best[0][0])
            commit_line = "last commit %s %s on %s" % (hhmm(best[0][0]), best[0][1], best[1])
        path = _transcript(src, rec)
        try:
            mt = os.path.getmtime(path) if path else None
        except OSError:
            mt = None
        if mt is None:
            if now - max(marks) >= th["silent"]:
                running = _running(src, rec, now)
                # With NO transcript to read, a worker whose turn has not ended and whose own
                # command still runs is taken as working (2026-10-29 08:28Z: three live workers,
                # one of them in a 4-minute `git commit`, called SILENT with "no transcript
                # found"). A transcript that WAS read is the stronger evidence: a foreground call
                # returns within 600 s and a background one is waited on in steps of at most
                # 270 s, so a working agent writes it well inside SILENT_MINUTES, and one
                # unchanged that long is silent however long a command it left behind (a dev
                # server, a watcher) keeps running.
                if not (running and not _deferred_end(src, rec, running, marks)):
                    unread.append(name)
            continue  # missing evidence cannot prove inactivity
        marks.append(mt)
        tline = "last transcript write %s (%s)" % (hhmm(mt), path)
        last = max(marks)
        if now - last < th["silent"]:
            continue
        lines = [commit_line, tline]
        own_waits = [w for w in waits if reg.name_for_waiter(w["waiter"]) == name]
        for w in own_waits[:2]:
            lines.append("recorded wait: %s for %s" % (resource_waits.LABELS.get(w["resource"], w["resource"]),
                                                       span(now - w["since"])))
        running = _running(src, rec, now)
        lines += running
        deferred = _deferred_end(src, rec, running, marks)
        if deferred:
            # 2026-10-11, echo-opus-walk49fix1: its turn ended with its handover, but its
            # own endless wait loops kept its end deferred, and Rich looked up their pids.
            lines.insert(0, "its turn ended at %s, but its end waits on its own command(s) still "
                            "running (below); they end only by themselves or by the command below" % hhmm(deferred))
            what = "its turn ended; its end waits on its own still-running command(s)"
            action = ["leave it: once they have used no CPU to speak of for %.0f min this watcher ends them "
                      "itself and says so in one line; a command still working is never ended that way" % (
                          _env_float("STALL_WATCH_IDLE_END_MINUTES", IDLE_END_MINUTES)),
                      "end its own commands now (its run then ends by itself; shared daemons and the test VM "
                      "guest are left running): %s" % src.agent_hold.end_command(
                          str(rec["session_id"]), str(rec["agent_id"]), name)]
            out.append(Stall("silent:%s" % rec.get("key"), "SILENT", name, last, what, lines, action))
            continue
        action = ["ask %s where it is (SendMessage to %s)" % (name, name),
                  "read the end of its transcript (above)",
                  "check it: %s %s" % (os.path.join(src.engine_root, "scripts", "agent-liveness.sh"),
                                       rec.get("agent_id") or name)]
        out.append(Stall("silent:%s" % rec.get("key"), "SILENT", name, last,
                         "no commit and no transcript activity" if not own_waits else
                         "no commit and no transcript activity (it has a recorded wait)", lines, action))
    return out, ("worker activity not checked: no readable transcript for " + ", ".join(sorted(unread))) if unread else ""


# ---------------------------------------------------------------------------
# IDLE END (2026-10-14): a deferred end's own idle commands are ended here
# ---------------------------------------------------------------------------
# THE FAILURE: echo-opus-tap22b's turn ended at 10:14:43Z, but three of its own
# background `until grep ...; do sleep 20; done` loops kept its end deferred
# (workspaces.py record_end), so it was never finished, never landable, and its
# loops ran until the lead looked up their pids. The same class on 2026-10-11
# (echo-opus-walk49fix1) and twice on 2026-10-14 (echo-opus-pocketvoice1,
# ray-opus-test55), each ended by hand with `agent_hold.py end`. The CEO: "how
# many more times will this type of shit keep happening?"
#
# THE RULE: an agent of this session whose last SubagentStop was deferred by its
# own commands, which has done nothing since (no commit, no transcript write),
# and whose own commands (agent_hold.work_tree: its owned tree, test VM work
# included, shared daemons left out) have together used under IDLE_CPU_PERCENT
# of one core between every two looks for IDLE_END_MINUTES, has those commands
# ended here through agent_hold.end_owned(), with every exclusion it keeps (the
# test VM guest, shared daemons, a held agent's work) and one more: a process whose
# readable environment carries ANOTHER agent's owner or session tag is left
# running, with its own tree. What is ended is the agent's recorded shells (pid,
# start time and parent captured at spawn, start time re-read from the kernel
# before each signal), their descendants and the processes carrying its own tag.
# The tag alone cannot be the rule (measured 2026-10-14 on macOS 15): the shell
# exports the tag after its own exec, and macOS shows no environment at all for
# /bin/sleep, /bin/bash, /bin/zsh or /usr/bin/grep, so an `until grep; do sleep`
# loop has no process whose environment shows the tag. Only looks that follow
# each other count: a look more than IDLE_LOOK_GAP after the previous one (the
# Mac asleep, a slow look) starts the ten minutes again, because nobody measured
# the time in between. Their completion wakes the
# agent and its next SubagentStop records its end, which runs end_owned_work().
# One line says what was ended. CPU includes exited children (agent_hold.cpu_used),
# so a loop of short commands is work; a process whose CPU cannot be read makes
# the whole look busy. A build, a test run or a walk that uses CPU in any look
# starts the ten minutes again, so it is never ended this way however long it runs.
#
# HOST CALLS ARE NEVER ENDED HERE (2026-10-30). An app worker's ordinary command now runs in
# the foreground exactly as in the terminal (agent_hold.py, HOST CALLS); the host moves a long
# one to the background at its Bash timeout and the worker may end its turn to await the
# host's notification. That is the normal path now, and such a command can sit idle for a long
# time on purpose (a notarization's `--wait`, a download). So an agent with a live call recorded
# as mode "host" is skipped: its end stays deferred until the command ends, and the SILENT
# notice still names the deferred end and the one command that ends it by hand. Explicit
# background calls (mode "bg") and the terminal's handed-off calls keep the rule above.

def _last_activity(src, rec):
    """Epoch of the agent's latest commit or transcript write, or None."""
    marks = []
    for w in rec.get("workspaces") or []:
        if not w.get("deleted_at"):
            c = _git_last_commit(w.get("path"))
            if c:
                marks.append(c[0])
    path = _transcript(src, rec)
    try:
        if not path:
            return None
        marks.append(os.path.getmtime(path))
    except OSError:
        return None
    return max(marks) if marks else None


def _deferred_at(src, rec):
    fact = getattr(src.ws, "END_DEFERRED_FACT", None)
    at = None
    for h in (rec.get("history") or []) if fact else []:
        if isinstance(h, dict) and h.get("fact") == fact:
            t = parse_iso(h.get("at"))
            if t is not None and (at is None or t > at):
                at = t
    return at


def idle_end(src, now, session_id, prev):
    """(lines, new idle state): ends the idle own commands of agents whose end they defer.
    The state keeps, per agent, when its commands were first seen idle and the last CPU
    reading of each of them (pid -> [start time, CPU seconds]) at a real-clock time."""
    ah = src.agent_hold
    if src.ws is None or ah is None or not session_id:
        return [], {}
    prev = prev if isinstance(prev, dict) else {}
    state, lines, cache = {}, [], {}
    end_after = _env_float("STALL_WATCH_IDLE_END_MINUTES", IDLE_END_MINUTES) * 60
    # A command waiting in line for a shared resource (a proof-run slot, CPU admission,
    # the test VM, a device) uses no CPU and is still work: its waiter record names its pid.
    records, _bad = resource_waits.read_records()
    if records is None:
        return [], {}
    in_line = set()
    for _path, w in records:
        try:
            in_line.add(int(w.get("pid")))
        except (TypeError, ValueError):
            pass
    for rec in src.ws.all_agents():
        sid, aid = str(rec.get("session_id") or ""), str(rec.get("agent_id") or "")
        if sid != session_id or not aid or rec.get("disposition"):
            continue
        at = _deferred_at(src, rec)
        if at is None:
            continue
        fin, paused, _why = src.ws.finished_state(rec, cache)
        last = _last_activity(src, rec)
        if fin or paused or last is None or last > at + 60 or os.path.lexists(ah._held_path(sid, aid)):
            continue
        table = ah.snapshot()
        if any(c["mode"] == "host" for c in ah.calls(sid, aid, table)):
            # An ordinary command the agent ran in the foreground, which the host moved to the
            # background at its timeout: the agent ended its turn to await its notification,
            # as in the terminal. Idle is not forgotten (a notarization's --wait, a slow
            # download), so it is never ended here; SILENT still names it (HOST CALLS, above).
            continue
        work = ah.work_tree(sid, aid, table)
        if not work or work & in_line:
            continue
        real = time.time()
        reading = {str(p): [table[p]["birth"], ah.cpu_used(p)] for p in sorted(work)}
        key = str(rec.get("key"))
        old = prev.get(key) if isinstance(prev.get(key), dict) else None
        if any(v[1] is None for v in reading.values()):
            # A child can exit between the snapshot and its CPU read. Keep the last
            # valid sample: the next cumulative reading covers this interval. Its
            # timestamp stays unchanged, so the existing maximum-gap reset still applies.
            if old:
                state[key] = old
            continue  # unmeasured is never idle
        before = old.get("at") if old else None
        if not isinstance(before, (int, float)) or not 0 <= now - before <= IDLE_LOOK_GAP:
            # The first look, or one after time nobody measured: the count starts here.
            state[key] = {"since": now, "at": now, "real": real, "cpu": reading}
            continue
        used = 0.0
        for pid, (birth, cpu) in reading.items():
            before = (old.get("cpu") or {}).get(pid)
            used += cpu - before[1] if before and before[0] == birth and cpu >= before[1] else cpu
        limit = max(IDLE_CPU_FLOOR, IDLE_CPU_PERCENT / 100.0 * max(0.0, real - float(old.get("real") or real)))
        since = float(old.get("since") or now) if used <= limit else now
        if now - since < end_after:
            state[key] = {"since": since, "at": now, "real": real, "cpu": reading}
            continue
        name = str(rec.get("name") or key)
        cmds = []
        for c in sorted(ah.calls(sid, aid, table), key=lambda c: c["at"]):
            if c["command"] and _short(c["command"], 70) not in cmds:
                cmds.append(_short(c["command"], 70))
        r = ah.end_owned(sid, aid, name, "ended by stall-watch: its end waited only on its own commands, "
                                         "idle %s" % span(now - since), spare_foreign=True)
        lines.append("STALL-WATCH %s: %s's turn ended at %s and its end waited only on its own command(s), idle "
                     "(no CPU to speak of) for %s, so they were ended (its run now ends by itself)%s: %s" % (
                         hhmm(now), name, hhmm(at), span(now - since),
                         (" [" + "; ".join("`%s`" % c for c in cmds[:3]) + "]") if cmds else "",
                         ah.describe_end(r)))
    return lines, state


# ---------------------------------------------------------------------------
# SLOW STEPS (2026-10-04)
# ---------------------------------------------------------------------------
# THE FAILURE: isaac-opus-logo1 spent 1.5 hours on a small fix because every
# step cost about 4.5 minutes (a background Bash call, then `agent_hold.py wait`
# returning only at its bound). It wrote its transcript every 4.5 minutes, so the
# SILENT rule (20 minutes) never saw it, and the CEO had to ask.
#
# THE RULE: a live teammate whose last SLOW_STEPS_N tool results each came at
# least SLOW_STEP_SECONDS after the call they answer. The step time is the
# timestamp of the tool_result row minus the timestamp of its tool_use row in
# the teammate's own transcript. A step whose command really ran that long (a
# test run, a build, a phone measurement) is exempt: its command is a
# reserve.py, run-tests.sh, perf, xcodebuild or gradle command, or its output
# says it was still working. A "STILL RUNNING" or "STILL WAITING" from
# `agent_hold.py wait` is never exempt: it is the defect itself.
SLOW_STEPS_N = 3
SLOW_STEP_SECONDS = 180
SLOW_TAIL_BYTES = 4 * 1024 * 1024
SLOW_EXEMPT_WORDS = ("reserve.py", "run-tests.sh", "perf", "xcodebuild", "gradle")


def _row_epoch(text):
    try:
        t = str(text)
        if t.endswith("Z"):
            t = t[:-1] + "+00:00"
        return _dt.datetime.fromisoformat(t).timestamp()
    except (TypeError, ValueError):
        return None


def _result_text(content):
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return " ".join(str(b.get("text", "")) for b in content if isinstance(b, dict))
    return ""


def transcript_steps(path):
    """[(call epoch, step seconds, command, result text)] oldest first, for every
    tool call in the transcript's tail that has its result. Never raises."""
    try:
        size = os.path.getsize(path)
        with open(path, "rb") as fh:
            if size > SLOW_TAIL_BYTES:
                fh.seek(size - SLOW_TAIL_BYTES)
                fh.readline()  # a partial first row
            raw = fh.read().decode("utf-8", "replace")
    except OSError:
        return []
    calls, steps = {}, []
    for line in raw.splitlines():
        try:
            row = json.loads(line)
        except ValueError:
            continue
        msg = row.get("message") if isinstance(row, dict) else None
        content = msg.get("content") if isinstance(msg, dict) else None
        at = _row_epoch(row.get("timestamp")) if isinstance(row, dict) else None
        if at is None or not isinstance(content, list):
            continue
        for b in content:
            if not isinstance(b, dict):
                continue
            if b.get("type") == "tool_use" and b.get("id"):
                inp = b.get("input") if isinstance(b.get("input"), dict) else {}
                calls[b["id"]] = (at, str(inp.get("command") or b.get("name") or ""))
            elif b.get("type") == "tool_result" and b.get("tool_use_id") in calls:
                t0, cmd = calls.pop(b["tool_use_id"])
                steps.append((t0, at - t0, cmd, _result_text(b.get("content"))))
    steps.sort(key=lambda s: s[0] + s[1])
    return steps


def _step_is_slow(step):
    _t0, secs, cmd, text = step
    if secs < SLOW_STEP_SECONDS:
        return False
    if "agent_hold.py wait" in cmd or "STILL RUNNING" in text or "STILL WAITING" in text:
        return True
    if any(w in cmd for w in SLOW_EXEMPT_WORDS) or "still working" in text.lower():
        return False
    return True


def _median(values):
    v = sorted(values)
    n = len(v)
    return v[n // 2] if n % 2 else (v[n // 2 - 1] + v[n // 2]) / 2.0


def _slow_steps(src, reg, now, session_id):
    if src.ws is None or reg.problem or not session_id:
        return []
    out = []
    cache = {}
    for rec in reg.recs:
        if rec.get("session_id") != session_id or rec.get("disposition"):
            continue
        try:
            fin, paused, _why = src.ws.finished_state(rec, cache)
        except Exception:  # noqa: BLE001
            continue
        if fin or paused:
            continue
        path = _transcript(src, rec)
        if not path:
            continue
        last = transcript_steps(path)[-SLOW_STEPS_N:]
        if len(last) < SLOW_STEPS_N or not all(_step_is_slow(s) for s in last):
            continue
        name = str(rec.get("name") or rec.get("key") or "?")
        worst = max(last, key=lambda s: s[1])
        since = last[0][0]
        lines = ["median step time %.1f min over its last %d tool calls (steps: %s min)" % (
                     _median([s[1] for s in last]) / 60.0, SLOW_STEPS_N, ", ".join("%.1f" % (s[1] / 60.0) for s in last)),
                 "slowest step (%.1f min): `%s`" % (worst[1] / 60.0, (worst[2] or "")[:100]),
                 "transcript: " + path]
        action = ["read the end of its transcript, then tell %s what is slow (SendMessage to %s)" % (name, name),
                  "check it: %s %s" % (os.path.join(src.engine_root, "scripts", "agent-liveness.sh"),
                                       rec.get("agent_id") or name)]
        out.append(Stall("slow:%s" % rec.get("key"), "SLOW-STEPS", name, since,
                         "every step takes minutes, though it is not silent", lines, action))
    return out


# ---------------------------------------------------------------------------
# what is announced, and when
# ---------------------------------------------------------------------------

def announce(state, stalls, problems, now, th, keep=""):
    """(lines to print, new state). A stall is announced when first seen, again
    after th['repeat'] since its last notice, and cleared (one line) after
    CLEAR_AFTER_MISSES looks without it. A source that cannot be read is
    announced the same way, as a stall of the watcher itself. Keys starting with
    `keep` were not looked at this time and are carried unchanged; a due item
    that stops being due ends without a line (the lead handled it)."""
    opened = dict(state.get("open") or {})
    current = {s.key: s for s in stalls}
    for p in problems:
        key = "blind:" + p[:80]
        current[key] = Stall(key, "NOT READ", "stall-watch", now, "cannot read a source", [p],
                             ["stalls that depend on it are not seen until it reads again"])
    new, again, cleared = [], [], []
    for key, s in current.items():
        prev = opened.get(key)
        if prev is None:
            opened[key] = {"first": now, "last": now, "count": 1, "since": s.since, "who": s.who,
                           "what": s.what, "missed": 0}
            new.append((s, 1, now))
            continue
        prev["missed"] = 0
        if now - prev["last"] >= th["repeat"]:
            prev["last"] = now
            prev["count"] = int(prev.get("count") or 1) + 1
            again.append((s, prev["count"], prev["first"]))
    for key in list(opened):
        if key in current or (keep and key.startswith(keep)):
            continue
        prev = opened[key]
        prev["missed"] = int(prev.get("missed") or 0) + 1
        if prev["missed"] >= CLEAR_AFTER_MISSES:
            gone = opened.pop(key)
            if not key.startswith("due:"):
                cleared.append(gone)
    lines = []
    if new or again:
        n = len(new) + len(again)
        lines.append("STALL-WATCH %s: %d stall%s%s (reported only: nothing of these was paused, stopped or killed)" % (
            hhmm(now), n, "" if n == 1 else "s",
            "" if not again else ", %d still going since an earlier notice" % len(again)))
        for s, no, first in new + again:
            lines += s.render(now, no, first)
    for c in cleared:
        lines.append("STALL-CLEARED %s: %s: %s, over after about %s" % (
            hhmm(now), c.get("who"), c.get("what"), span(now - float(c.get("since") or now))))
    return lines, {"open": opened, "at": now}


# ---------------------------------------------------------------------------
# ESCALATION: a teammate's escalation wakes an idle lead within a minute
# ---------------------------------------------------------------------------
# THE FAILURE (2026-10-01, richos-hq docs/operations/2026-10-01-escalation-
# wakes-the-lead.md). isaac-opus-d3c raised esc-20261001T113833Z-c9ac3ed0 at
# 11:38:33Z: "Can the CEO be at the test iPhone SE for the UI automation
# approval". The lead had ended its turn at 11:33:57Z. Escalations reached the
# lead only through hooks (SessionStart and Stop), a hook needs a turn, and this
# monitor, the one thing that can start a turn, read the ledger only for prose
# claiming a resource wait. The first lead turn came at 13:00:59Z, woken by an
# unrelated teammate finishing: 82 minutes. d3c gave up waiting at 13:07Z.
#
# WHAT IT DOES. Each look reads the ledger's outstanding escalations (the one
# predicate, escalations.outstanding) by STATE, never by wording, so no phrasing
# slips past, and prints the ones due in an ESCALATION-WATCH block FIRST in the
# look's output. The printed block is what starts a turn for an idle lead.
#
# WHAT IS DUE, every rule from the design and its review (Sage, richos-hq
# 8ba32b71), and why:
#   FIRST WAKE, once per delivery key, when all of these hold:
#     - raised (or reopened by an expired ack) AFTER this session started.
#       The SessionStart block and the Stop hook own the backlog; a block over
#       the host's 10,000-character cap would otherwise wake the lead at once
#       and then every minute until it drained (Sage 6a).
#     - it has not already reached this session's model
#       (escalations.reached_scan / has_reached over the host transcript: the
#       SessionStart block, a Stop delivery, an earlier monitor event).
#     - it is not owned by ANOTHER live session (twin Rich): suppressed only on
#       exactly one registry record with the row's worktree and teammate name
#       whose session is positively running. Zero matches, several, a dead or
#       unknown owner: told (Sage 4). Host cleanup instead selects one live app
#       lead from the host's existing lead files and recorded process identities.
#   TOLD AGAIN while unacknowledged, once it has reached this session's model
#   (by this monitor or by the Stop hook):
#     - needs=ceo-hands: every 10 min (STALL_WATCH_CEO_HANDS_REPEAT_MINUTES).
#       Each minute burns a waiting teammate; d3c gave up after 89.
#     - state=stopped: every 30 min (the stall repeat, STALL_WATCH_REPEAT_MINUTES).
#       A stopped teammate is a stall by the ledger's own definition.
#     - any other: once at each age bucket the ledger defines (1h, 24h, 72h),
#       never on a clock. A `proceeding` escalation is not a stall.
#   An ACKNOWLEDGEMENT ends every repeat SILENTLY: the lead made the ack, so a
#   line about it would be a wake about nothing.
#
# THE BLOCK STAYS UNDER 2,000 CHARACTERS (ESC_BLOCK_CHARS). The largest monitor
# event ever delivered on this Mac was 2,352 characters and nobody has measured
# whether the host cuts a longer one; if it records the text before a cut, the
# transcript would claim a delivery the model never saw (Sage 5). So one line
# per escalation (key, title, from, state, for, needs, the question's start)
# and a pointer to the full text. What does not fit goes in the next look.
#
# A SOURCE THAT CANNOT BE READ changes nothing: the ledger or the transcript
# unreadable is one NOT READ line through the stall machinery, and every
# escalation key is carried forward unchanged, so the next good read does not
# re-announce everything (Sage 6b). Its state is its own (`esc` in this
# session's announced.json); it never reads or writes the Stop hook's memory.
ESC_BLOCK_CHARS = 2000
ESC_TITLE_CHARS = 120
ESC_QUESTION_CHARS = 200
CEO_HANDS_REPEAT_MINUTES = 10
ESC_LOUD_BUCKETS = ("1h", "24h", "72h")


def _esc_iso_epoch(text):
    d = escalations.parse_iso(text)
    return d.timestamp() if d is not None else None


def lead_transcript(src, sid):
    """The lead's own transcript: STALL_WATCH_LEAD_TRANSCRIPT (the test suite's
    fixture; nothing else sets it), else <projects>/*/<session id>.jsonl, the
    file the host writes every row of the lead's conversation to.

    Globbed by session id (a UUID), in the projects directory workspaces.py
    already resolves for teammates' transcripts, rather than by recomputing
    the project directory's name from a cwd, which the host derives and nothing
    here should guess. The newest wins if the id appears under two project
    directories. "" when there is none yet: the host creates it with the
    session's first recorded row."""
    pinned = (os.environ.get("STALL_WATCH_LEAD_TRANSCRIPT") or "").strip()
    if pinned:
        return pinned
    if src.ws is None or not sid or "/" in sid or sid in (".", ".."):
        return ""
    try:
        base = src.ws._platform_projects_dir()
        found = [p for p in glob.glob(os.path.join(base, "*", sid + ".jsonl")) if os.path.isfile(p)]
    except Exception:  # noqa: BLE001: no transcript found is "nothing reached yet"
        return ""
    return max(found, key=lambda p: (os.path.getmtime(p), p)) if found else ""


def session_started(src, sid):
    """Epoch seconds this session's claude process started, or None.

    STALL_WATCH_SESSION_START pins it for the test suite. Under the app host
    (--host-json), the time the host started that lead (host_child.started):
    the lead is this watcher's sibling there, so the lookup below, which goes
    through this process's ancestry, finds nothing. Else the platform's own
    record of the process (<config>/sessions/<pid>.json, startedAt in ms),
    else the OS's start time for that pid. A resumed session is a new process,
    so its backlog stays the SessionStart block's and the Stop hook's."""
    pinned = (os.environ.get("STALL_WATCH_SESSION_START") or "").strip()
    if pinned:
        try:
            return float(pinned)
        except ValueError:
            pass
    hosted = host_child.started(sid)
    if hosted is not None:
        return hosted
    if src.ws is None:
        return None
    try:
        _sid, pid = src.session()
        rec = src.ws.platform_session(sid, pid) if sid else None
        if rec and isinstance(rec.get("startedAt"), (int, float)):
            return float(rec["startedAt"]) / 1000.0
        if pid:
            st, text = src.ws.process_start(pid)
            if st == "ok" and text:
                # workspaces.process_start runs ps with TZ=UTC0: the text is UTC.
                return float(calendar.timegm(time.strptime(" ".join(text.split()), "%a %b %d %H:%M:%S %Y")))
    except Exception:  # noqa: BLE001
        return None
    return None


def _session_state_anywhere(src, owner, cache):
    """'running', 'ended' or 'unknown' for a session this registry may not hold: its own
    record first, then its identity in every other registry on this machine."""
    st = src.ws.session_state(owner, None, cache)[0]
    others = getattr(src.ws, "other_registries", None)
    if st != "unknown" or others is None:
        return st
    for registry in others():
        identity = _read_json(os.path.join(registry, "sessions", owner + ".json"))
        if identity:
            st = "ended" if identity.get("ended_at") else src.ws.session_state(owner, identity)[0]
            if st != "unknown":
                return st
    return "unknown"


def _esc_owner(src, reg, e, sid, cache):
    """'ours', 'other' or 'unknown': one live app lead handles host cleanup;
    teammates otherwise belong to an exactly matching live registration."""
    wt, name = e.get("worktree") or "", e.get("teammate") or ""
    if not wt or not name or src.ws is None:
        return "unknown"
    lead = os.environ.get(host_child.LEAD_ENV, "")
    if name == "scratch-reaper" and lead:
        # Host cleanup has no teammate registration. Reuse the host's lead
        # records to select one live handler, with takeover when that lead ends.
        candidates = []
        try:
            for filename in os.listdir(os.path.dirname(lead)):
                if not filename.endswith(".json"):
                    continue
                row = _read_json(os.path.join(os.path.dirname(lead), filename))
                owner, started = row.get("session"), row.get("started")
                if (isinstance(owner, str) and owner and os.sep not in owner
                        and isinstance(started, (int, float)) and not isinstance(started, bool) and started > 0):
                    status = src.ws.session_state(owner, None, cache)[0]
                    if status == "unknown":
                        # A peer belongs to another conversation registry, and
                        # may use another Claude account. Its recorded pid AND
                        # start identity still prove liveness on this machine.
                        registries = os.path.join(os.path.dirname(os.path.dirname(lead)), "workspaces")
                        for partition in os.listdir(registries):
                            identity = _read_json(os.path.join(registries, partition, "sessions", owner + ".json"))
                            if identity:
                                status = "ended" if identity.get("ended_at") else src.ws.session_state(owner, identity)[0]
                                if status != "unknown":
                                    break
                    if status == "running":
                        candidates.append((started, owner))
        except Exception:  # ownership unreadable: report rather than silently lose it
            return "unknown"
        if candidates and any(owner == sid for _, owner in candidates):
            return "ours" if min(candidates)[1] == sid else "other"
    real = os.path.realpath(wt)
    matches = []
    for r in reg.recs:
        if str(r.get("name") or "") != name:
            continue
        if any(w.get("path") and os.path.realpath(w["path"]) == real for w in r.get("workspaces") or []):
            matches.append(r)
    if len(matches) != 1:
        # In the app another chat's registry partition holds neither the worktree nor the name:
        # the session the row was raised in decides, while it runs (2026-10-30, second review
        # of 5881747e5: chat B was told chat A's wait). An ended owner's row is still told here.
        owner = str(e.get("session_id") or "")
        if not owner or os.sep in owner:
            return "unknown"
        if owner == sid:
            return "ours"
        try:
            return "other" if _session_state_anywhere(src, owner, cache) == "running" else "unknown"
        except Exception:  # noqa: BLE001
            return "unknown"
    owner = str(matches[0].get("session_id") or "")
    if owner and owner == sid:
        return "ours"
    if not owner:
        return "unknown"
    try:
        st, _why = src.ws.session_state(owner, matches[0].get("session_identity"), cache)
    except Exception:  # noqa: BLE001
        return "unknown"
    return "other" if st == "running" else "unknown"


def _esc_since(e):
    """When this delivery key began: raised, or reopened by an expired ack."""
    reo = e.get("reopened_by_expiry")
    if reo:
        return _esc_iso_epoch(reo.get("until")) or _esc_iso_epoch(e.get("raised"))
    return _esc_iso_epoch(e.get("raised"))


def _esc_repeat_seconds(e, th):
    if e.get("needs") == "ceo-hands":
        return _env_float("STALL_WATCH_CEO_HANDS_REPEAT_MINUTES", CEO_HANDS_REPEAT_MINUTES) * 60
    if e.get("state") == "stopped":
        return th["repeat"]
    return None


def _esc_line(e, again=None):
    key = escalations.delivery_key(e)
    title = " ".join(str(e.get("title") or "").split())
    if len(title) > ESC_TITLE_CHARS:
        title = title[:ESC_TITLE_CHARS - 3] + "..."
    q = " ".join(str(e.get("question") or "").split())
    if len(q) > ESC_QUESTION_CHARS:
        q = q[:ESC_QUESTION_CHARS - 3] + "..."
    who = "from %s, state=%s, for=%s%s, %s" % (
        e.get("teammate") or "<unnamed>", e.get("state", ""), e.get("for", "lead"),
        ", needs=%s" % e["needs"] if e.get("needs") else "", escalations.age_phrase(e.get("age_min")))
    parts = ["[%s] %s" % (key, title), who]
    reo = e.get("reopened_by_expiry")
    if reo:
        parts.append("REOPENED: its ack held only until %s" % (reo.get("until") or "?"))
    parts += ["Q: " + q, "full text: escalate.sh show %s" % e["id"]]
    line = "  " + " | ".join(parts)
    if again:
        line += "  (told again, notice %d; first told %s)" % again
    return line


def _esc_render(now, first, again):
    """(lines, keys included). ceo-hands first under its own heading."""
    def order(item):
        e = item[0]
        return (0 if e.get("needs") == "ceo-hands" else 1 if e.get("state") == "stopped" else 2,
                str(e.get("raised") or ""))
    items = sorted([(e, None) for e in first] + list(again), key=order)
    head_fmt = ("ESCALATION-WATCH %s: %d teammate escalation%s for you%s (in the ledger until acknowledged; "
                "nothing was paused, stopped or killed)")
    foot = ["  You can: answer it, or put it to the CEO; then escalate.sh ack <id> --disposition \"<what you "
            "decided or did>\" (routed to him: add --until <when to look again>). An ack ends every repeat."]
    hands = [it for it in items if it[0].get("needs") == "ceo-hands"]
    body, used, keys, n_first, n_again = [], 0, [], 0, 0
    budget = ESC_BLOCK_CHARS - len(head_fmt) - 40 - sum(len(f) + 1 for f in foot)
    for e, again_info in items:
        text = _esc_line(e, again_info)
        heading = []
        if hands and not body:
            heading = ["  NEEDS THE CEO AT A DEVICE (told again every %d min until acknowledged):"
                       % int(_env_float("STALL_WATCH_CEO_HANDS_REPEAT_MINUTES", CEO_HANDS_REPEAT_MINUTES))]
        elif hands and e.get("needs") != "ceo-hands" and not any(
                b.startswith("  THE REST") for b in body):
            heading = ["  THE REST:"]
        cost = sum(len(h) + 1 for h in heading) + len(text) + 1
        if body and used + cost > budget:
            continue                       # the next look tells it; nothing is marked
        body += heading + [text]
        used += cost
        keys.append(escalations.delivery_key(e))
        if again_info:
            n_again += 1
        else:
            n_first += 1
    if not keys:
        return [], []
    n = n_first + n_again
    head = head_fmt % (hhmm(now), n, "" if n == 1 else "s",
                       "" if not n_again else ", %d told again" % n_again)
    return [head] + body + foot, keys


def escalation_watch(src, now, sid, prev, th):
    """(lines to print, new `esc` state, problems). Never raises for a source
    it cannot read; that source is a problem and the state is carried forward."""
    prev = dict(prev or {})
    if not sid:
        return [], prev, []
    rows, _bad = escalations.read_rows()
    if rows is None:
        return [], prev, ["the escalation ledger (%s) could not be read, so no teammate escalation can "
                          "wake you until it reads again" % escalations.ledger_path()]
    start = prev.get("session_start")
    if not isinstance(start, (int, float)):
        start = session_started(src, sid)
        if start is None:
            start = prev.get("first_look") if isinstance(prev.get("first_look"), (int, float)) else now
    transcript = lead_transcript(src, sid)
    scan = dict(prev.get("scan") or {})
    if scan.get("transcript") != transcript:
        scan = {"transcript": transcript, "offset": 0, "session_start": [], "stop": [], "monitor": []}
    found, offset, readable = escalations.reached_scan(transcript, int(scan.get("offset") or 0))
    if not readable:
        return [], prev, ["the lead's transcript (%s) could not be read, so what already reached you is "
                          "unknown and no escalation is told until it reads again" % transcript]
    for k in ("session_start", "stop", "monitor"):
        scan[k] = sorted(set(scan.get(k) or []) | found[k])
    scan["offset"] = offset
    baseline = set(scan["session_start"])
    delivered = set(scan["stop"]) | set(scan["monitor"])

    live = escalations.outstanding(rows, _dt.datetime.fromtimestamp(now, _dt.timezone.utc))
    by_key = dict((escalations.delivery_key(e), e) for e in live)
    woken = dict(prev.get("woken") or {})
    for k in list(woken):
        if k not in by_key:
            del woken[k]                   # acknowledged: silent (and nothing more about it)
    reg = None
    cache = {}
    first, again = [], []
    for k, e in by_key.items():
        if e.get("teammate") == "scratch-reaper":
            if reg is None:
                reg = Registry(src)
            if _esc_owner(src, reg, e, sid, cache) == "other":
                continue                   # applies to old duplicate reminders as well as new issues
        w = woken.get(k)
        if w is None:
            since = _esc_since(e)
            if since is None or since < start:
                continue                   # the backlog: SessionStart's and the Stop hook's
            if escalations.has_reached(e, baseline, delivered):
                woken[k] = {"first": now, "last": now, "count": 0, "bucket": e.get("bucket"),
                            "by": "reached the model before this monitor told it"}
                continue
            if reg is None:
                reg = Registry(src)
            if e.get("teammate") != "scratch-reaper" and _esc_owner(src, reg, e, sid, cache) == "other":
                continue                   # that session's own monitor tells it
            first.append(e)
            continue
        every = _esc_repeat_seconds(e, th)
        due = (every is not None and now - float(w.get("last") or now) >= every) or (
            e.get("bucket") != w.get("bucket") and e.get("bucket") in ESC_LOUD_BUCKETS)
        if due:
            again.append((e, (int(w.get("count") or 0) + 1, hhmm(float(w.get("first") or now)))))
    lines, told = _esc_render(now, first, again)
    for k in told:
        e = by_key[k]
        w = woken.get(k)
        if w is None:
            woken[k] = {"first": now, "last": now, "count": 1, "bucket": e.get("bucket"), "by": "this monitor"}
        else:
            w.update({"last": now, "count": int(w.get("count") or 0) + 1, "bucket": e.get("bucket")})
    # Anything due that did not fit is untouched, so it is still due next look.
    state ={"session_start": start, "first_look": prev.get("first_look") or now,
             "scan": scan, "woken": woken}
    return lines, state, []


# ---------------------------------------------------------------------------
# the lead's transcript and start (escalations; whether the lead is idle)
# ---------------------------------------------------------------------------

def lead_is_idle(src, sid):
    """Use the recorded turn end, never a quiet-but-still-running model call.

    Read only a bounded tail. An unreadable or incomplete transcript leaves
    the reminder due until a later look can establish that the lead is idle.

    Under the app host (--host-json) the host's word is used instead
    (host_child.idle): its lead runs with --print, whose transcript never
    records a turn's end, and the host sees every turn end and begin.
    """
    hosted = host_child.idle(sid)
    if hosted is not None:
        return hosted
    path = lead_transcript(src, sid)
    try:
        with open(path, "rb") as fh:
            size = fh.seek(0, os.SEEK_END)
            start = max(0, size - 256 * 1024)
            fh.seek(start)
            tail = fh.read()
        if not tail.endswith(b"\n"):
            return False
        rows = tail.splitlines()[1 if start else 0:]
        for raw in reversed(rows):
            row = json.loads(raw)
            if not isinstance(row, dict):
                return False
            if row.get("isSidechain"):
                continue
            if row.get("type") in ("user", "assistant"):
                return False
            if row.get("type") == "system" and row.get("subtype") == "turn_duration":
                return True
            if (row.get("attachment") or {}).get("type") == "queued_command":
                return False
    except (OSError, ValueError, TypeError):
        pass
    return False


# ---------------------------------------------------------------------------
# state, the session, and the loop
# ---------------------------------------------------------------------------

def state_root():
    return (os.environ.get("STALL_WATCH_STATE_DIR") or "").strip() or os.path.join(
        os.path.expanduser("~"), ".claude", "state", "stall-watch")


def session_dir(sid):
    d = os.path.join(state_root(), sid or "no-session")
    os.makedirs(d, exist_ok=True)
    return d


def _try_lock(path):
    try:
        fd = os.open(path, os.O_RDWR | os.O_CREAT, 0o600)
    except OSError:
        return None
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        return fd
    except OSError:
        os.close(fd)
        return None


def lock_held(path):
    """True when a live process holds the lock at path. Takes nothing that lasts."""
    if not os.path.exists(path):
        return False
    fd = _try_lock(path)
    if fd is None:
        return True
    try:
        fcntl.flock(fd, fcntl.LOCK_UN)
    finally:
        os.close(fd)
    return False


def prune_state(keep):
    """Section 54: a session's directory goes once nothing holds its lock and it
    has not been written for two days. Never the one in use."""
    root = state_root()
    try:
        names = os.listdir(root)
    except OSError:
        return
    now = time.time()
    for n in names:
        d = os.path.join(root, n)
        if d == keep or not os.path.isdir(d) or lock_held(os.path.join(d, "monitor.lock")):
            continue
        try:
            newest = max([os.path.getmtime(os.path.join(d, f)) for f in os.listdir(d)] + [os.path.getmtime(d)])
        except OSError:
            continue
        if now - newest > STATE_KEEP_SECONDS:
            shutil.rmtree(d, ignore_errors=True)


def session_alive_check(pid):
    """Is the session's claude process still the one this started for? Its pid
    every call, and its start time once a minute (a reused pid is not it)."""
    def _start(p):
        try:
            r = subprocess.run(["ps", "-o", "lstart=", "-p", str(int(p))], capture_output=True, text=True, timeout=10)
        except (OSError, ValueError, subprocess.TimeoutExpired):
            return ""
        return r.stdout.strip() if r.returncode == 0 else ""
    first = _start(pid)
    calls = [0]

    def alive():
        try:
            os.kill(int(pid), 0)
        except PermissionError:
            pass
        except (OSError, ValueError, TypeError):
            return False
        calls[0] += 1
        if first and calls[0] % 12 == 0:
            now_start = _start(pid)
            if now_start and now_start != first:
                return False
        return True
    return alive


def tick(src, sd, sid, now=None, out=None, json_lines=False):
    """One look against the session's state: prints what is due, returns the
    number of lines printed. A look that raises is announced once as a stall
    of the watcher and changes nothing else."""
    out = out or sys.stdout
    now = clock() if now is None else now
    th = thresholds()
    path = os.path.join(sd, "announced.json")
    state = _read_json(path)
    esc_prev = state.get("esc")
    idle_prev = state.get("idle")
    lead_idle = lead_is_idle(src, sid)
    try:
        # Before the look, so a stall it ends is never also announced as one.
        idle_lines, idle_state = idle_end(src, now, sid, idle_prev)
    except Exception as exc:  # noqa: BLE001: a failed idle look is reported, never the end of watching
        idle_lines, idle_state = ["STALL-WATCH %s: the idle-command look failed (%s: %s); nothing was ended" % (
            hhmm(now), exc.__class__.__name__, str(exc)[:160])], {}
    try:
        esc_lines, esc_state, esc_problems = escalation_watch(src, now, sid, esc_prev, th)
    except Exception as exc:  # noqa: BLE001: a failed escalation look is reported, never the end of watching
        esc_lines, esc_state = [], esc_prev
        esc_problems = ["the escalation watch failed (%s: %s)" % (exc.__class__.__name__, str(exc)[:160])]
    failed = ""
    try:
        stalls, problems = look(src, now, sid, th, due=lead_idle)
        problems = problems + esc_problems
    except Exception as exc:  # noqa: BLE001: one failed look is reported, never the end of watching
        failed = "a look failed (%s: %s)" % (exc.__class__.__name__, str(exc)[:160])
    # The look ran outside the lock; what to announce is decided inside it, against the
    # state as it is now. The SessionStart notice (record_told_due) may have recorded an
    # item during the look, and an item it told is not told again by this tick.
    with announced_lock(sd):
        state = _read_json(path)
        if failed:
            problems = [failed]
            keep = dict(state.get("open") or {})
            lines, _new = announce({"open": {}}, [], problems, now, th)
            key = "blind:" + problems[0][:80]
            if key in keep:
                lines = []
            else:
                keep[key] = {"first": now, "last": now, "count": 1, "since": now, "who": "stall-watch",
                             "what": "a look failed", "missed": 0}
            state = {"open": keep, "at": now}
        else:
            lines, state = announce(state, stalls, problems, now, th, keep="" if lead_idle else "due:")
        if esc_state is not None:
            state["esc"] = esc_state
        state["idle"] = idle_state
        # The ESCALATION-WATCH block goes FIRST: a monitor event counts as having
        # told the lead only when its <event> body starts with ESCALATION-WATCH
        # (escalations.monitor_event_keys).
        lines = esc_lines + idle_lines + lines
        if lines:
            text = "\n".join(lines)
            out.write((json.dumps({"text": text}) if json_lines else text) + "\n")
            out.flush()
        _write_json(path, state)
    return len(lines), len(state.get("open") or {})


# ---------------------------------------------------------------------------
# EVERY LOOK IS FRESH CODE (orchestration reliability R5)
# ---------------------------------------------------------------------------
# 2026-10-22/23: the holds feature landed at 06:44:57Z and the turn-end hooks used it
# at once, but the session's watcher (PID 71067, started 2026-10-21 14:01Z) had loaded
# workspaces.py once and never saw it: a healthy heartbeat over code that could not
# see a hold. So the monitor keeps only its lock, its session's liveness and its
# output; every look is `--tick` in a fresh child, which loads the engine from disk.
# The child records the source identity it evaluated (evaluated.json); the parent
# copies it beside the beat (monitor.json). A child that fails or outlives its
# timeout is a watcher failure, told once per failing streak, and the beat's
# last_look does not move, so --alive and the Stop fallback see it.
EVALUATOR_SOURCES = ("scripts/lib/stall_watch.py", "mega-lander/workspaces.py", "scripts/lib/resource_waits.py",
                     "scripts/lib/escalations.py", "scripts/lib/host_child.py")


def source_identity(engine_root):
    """(sha256 of the evaluator's source files as they are on disk, newest mtime)."""
    import hashlib
    h, newest = hashlib.sha256(), 0.0
    for rel in EVALUATOR_SOURCES:
        path = os.path.join(engine_root, rel)
        try:
            with open(path, "rb") as fh:
                h.update(rel.encode() + b"\0" + fh.read() + b"\0")
            newest = max(newest, os.path.getmtime(path))
        except OSError:
            h.update(rel.encode() + b"\0missing\0")
    return h.hexdigest()[:16], newest


def _child_look(a, src, sid, sd, poll):
    """(ok, lines printed, why). One look in a fresh child of this same script."""
    timeout = max(5.0, min(poll - 5.0, 0.75 * poll))
    cmd = [sys.executable, "-B", os.path.abspath(__file__), "--tick", "--engine-root", src.engine_root,
           "--config", a.config, "--command", a.command] + (["--jsonl"] if a.jsonl else [])
    started = time.time()
    # The source on disk when the child STARTS. The child hashes it again after its look; a
    # difference means it changed while the child was loading or looking, so which version
    # it ran is unknown and the look is recorded as no version (R5).
    before = source_identity(src.engine_root)[0]
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout,
                           env=dict(os.environ, RICHOS_SESSION_ID=sid or ""))
    except subprocess.TimeoutExpired:
        return False, 0, "the look did not finish within %.0f s" % timeout
    except OSError as exc:
        return False, 0, "the look could not start (%s)" % exc
    if r.stdout:
        sys.stdout.write(r.stdout)
        sys.stdout.flush()
    if r.returncode != 0:
        tail = " ".join((r.stderr or "").strip().splitlines()[-2:])[:200]
        return False, 0, "the look exited %d%s" % (r.returncode, (": " + tail) if tail else "")
    ev = _read_json(os.path.join(sd, "evaluated.json"))
    if not isinstance(ev.get("at"), (int, float)) or ev["at"] < started:
        return False, 0, "the look recorded no evaluated source"
    if ev.get("identity") != before:
        _write_json(os.path.join(sd, "evaluated.json"), dict(ev, identity="changed-during-the-look"))
    return True, len([ln for ln in r.stdout.splitlines() if ln.strip()]), ""


def _failure_block(a, why, now):
    text = ("STALL-WATCH %s: the look failed (%s). Nothing is seen until a look succeeds; nothing was paused, "
            "stopped or killed. Check: %s --alive" % (hhmm(now), why, a.command))
    return (json.dumps({"text": text}) if a.jsonl else text) + "\n"


def _nap(seconds, alive):
    left = seconds
    while left > 0:
        step = min(5.0, left)
        time.sleep(step)
        left -= step
        if alive is not None and not alive():
            return


def run_loop(a, src, exit_on_notice=False):
    sid, spid = src.session()
    sd = session_dir(sid)
    lock = os.path.join(sd, "monitor.lock")
    fd = _try_lock(lock)
    if fd is None:
        if exit_on_notice:
            mon = _read_json(os.path.join(sd, "monitor.json"))
            print("stall-watch: this session is already watched (pid %s); nothing to start." % mon.get("pid", "?"))
        return 0
    prune_state(sd)
    # The holder names itself in the lock, so --alive can tell the monitor that holds it from
    # a beat another process left behind (R5: the owner, not just "some lock is held").
    try:
        os.ftruncate(fd, 0)
        os.pwrite(fd, str(os.getpid()).encode(), 0)
    except OSError:
        pass
    alive = session_alive_check(spid) if spid else None
    poll = _env_float("STALL_WATCH_POLL_SECONDS", POLL_SECONDS)
    beat = {"pid": os.getpid(), "started": int(time.time()), "poll": poll, "session": sid,
            # The registry partition this monitor reads (in the app, one conversation's):
            # another conversation's reader may count this monitor only for the same one.
            "scope": os.path.realpath(os.environ["RICHOS_WORKSPACES_DIR"])
                     if (os.environ.get("RICHOS_WORKSPACES_DIR") or "").strip() else "",
            "mode": "watch" if exit_on_notice else "monitor", "last_look": None, "open": 0}
    failing = ""
    try:
        while alive is None or alive():
            ok, printed, why = _child_look(a, src, sid, sd, poll)
            if ok:
                beat["last_look"] = time.time()
                beat["open"] = len(_read_json(os.path.join(sd, "announced.json")).get("open") or {})
                beat["evaluated"] = _read_json(os.path.join(sd, "evaluated.json"))
                beat.pop("failed", None)
                failing = ""
            else:
                beat["failed"] = {"at": time.time(), "why": why}
                if why != failing:            # once per failing streak and reason
                    sys.stdout.write(_failure_block(a, why, clock()))
                    sys.stdout.flush()
                    printed = 1
                failing = why
            _write_json(os.path.join(sd, "monitor.json"), beat)
            if printed and exit_on_notice:
                print("  To be told of the next one, start this again as a background command: %s --watch" % a.command)
                return 0
            _nap(poll, alive)
    finally:
        os.close(fd)
    return 0


def run_host_loop(src):
    """--monitor --host-json: the app host's child. Every look is for the lead the host names, in
    that session's own state directory, exactly as a terminal session's monitor looks for its
    own; with no lead running there is no session of whose teammates to speak, so it waits.
    One child per app: it holds the host's own directory's lock for its life."""
    alive = host_child.start()
    if not alive():
        return 0                           # its host is gone already
    hd = session_dir("operator-host")
    fd = _try_lock(os.path.join(hd, "monitor.lock"))
    if fd is None:
        return 0                           # another copy of the app already watches
    prune_state(hd)
    poll = _env_float("STALL_WATCH_POLL_SECONDS", POLL_SECONDS)
    try:
        while alive():
            sid = host_child.session()
            if sid:
                tick(src, session_dir(sid), sid, json_lines=True)
            _nap(poll, alive)
    finally:
        os.close(fd)
    return 0


def _in_app():
    """Launched by the app for one conversation (engine_profile.rs configure sets both)."""
    return bool((os.environ.get("RICHOS_APP_ENTITY") or "").strip() and (os.environ.get("RICHOS_APP_THREAD") or "").strip())


def alive_status(sid, engine_root, command="stall-watch.sh"):
    """(code, line): is `sid` watched by a live monitor that evaluated the engine's
    current source? 0 yes; 1 no, with why; 2 no session. The one reading --alive and the
    Stop/startup fallback (workspaces.py) share. In the app the registry is one
    conversation's partition and its one monitor runs under the Work lease's session, so
    there a healthy monitor of another session counts only when its beat's `scope` is this
    caller's registry partition (RICHOS_WORKSPACES_DIR)."""
    if not sid:
        return 2, "stall-watch: no session here; nothing to check"
    names = [sid]
    if _in_app():
        try:
            names += sorted(n for n in os.listdir(state_root()) if n != sid)
        except OSError:
            pass
    mine = os.path.realpath(os.environ["RICHOS_WORKSPACES_DIR"]) \
        if (os.environ.get("RICHOS_WORKSPACES_DIR") or "").strip() else ""
    first = None
    for name in names:
        if name != sid and (not mine or _read_json(os.path.join(state_root(), name, "monitor.json"))
                            .get("scope") != mine):
            continue          # another conversation's monitor watches another partition
        code, line = _alive_one(name, sid, engine_root, command)
        if code == 0:
            return code, line
        first = first or (code, line)
    return first


def _alive_one(name, sid, engine_root, command):
    sd = os.path.join(state_root(), name)
    beat = _read_json(os.path.join(sd, "monitor.json"))
    if not lock_held(os.path.join(sd, "monitor.lock")):
        if _in_app():
            # The app owns its monitor (native.rs StallWatch::start, with Stu's lease); the
            # lease's PATH carries no stall-watch.sh, so there is no command to give (CEO §138).
            return 1, ("stall-watch: NOT WATCHED: nothing looks for stalls in this conversation; "
                       "RichOS starts its watcher with Stu's session")
        return 1, ("stall-watch: NOT WATCHED: nothing looks for stalls in this session. Start as a background "
                   "command (Bash with run_in_background: true): %s --watch" % command)
    poll = float(beat.get("poll") or POLL_SECONDS)
    try:
        with open(os.path.join(sd, "monitor.lock"), encoding="utf-8") as fh:
            holder = fh.read().strip()
    except OSError:
        holder = ""
    if holder and holder != str(beat.get("pid")):
        return 1, ("stall-watch: NOT ITS OWNER: the lock is held by pid %s, but the last beat was written by pid %s"
                   % (holder, beat.get("pid")))
    if not session_alive_check(beat.get("pid"))():
        return 1, "stall-watch: NOT WATCHED: the pid that wrote the last beat (%s) is gone" % beat.get("pid")
    if name == sid and beat.get("session") not in (None, "", sid):
        return 1, "stall-watch: the monitor here watches session %s, not %s" % (beat.get("session"), sid)
    last = beat.get("last_look")
    failed = beat.get("failed") or {}
    if failed and float(failed.get("at") or 0) >= float(last or 0):
        return 1, "stall-watch: LOOKS FAILING: %s pid %s: %s" % (beat.get("mode", "monitor"), beat.get("pid"),
                                                               failed.get("why"))
    if not last:
        return 1, "stall-watch: %s pid %s started, no look finished yet" % (beat.get("mode", "monitor"), beat.get("pid"))
    age = time.time() - float(last)
    if age > 3 * poll + 30:
        return 1, "stall-watch: STOPPED LOOKING: %s pid %s last looked %s, %s ago (it looks every %.0f s)" % (
            beat.get("mode", "monitor"), beat.get("pid"), hms(last), span(age), poll)
    ident, newest = source_identity(engine_root)
    ev = beat.get("evaluated") or {}
    if not ev.get("identity"):
        return 1, ("stall-watch: OLD MONITOR: %s pid %s looks in-process with the code it started with; it is "
                   "replaced when its session restarts" % (beat.get("mode", "monitor"), beat.get("pid")))
    if ev["identity"] != ident:
        # Until a look has evaluated the CURRENT source, nothing says the change is watched:
        # the ordinary old/new interval is not healthy, so the Stop/startup fallback stays on.
        return 1, ("stall-watch: NOT YET ON THE CURRENT SOURCE: its last look evaluated %s, the engine's is %s; "
                   "the next look uses it" % (ev["identity"], ident))
    return 0, "stall-watch: watching (%s pid %s, every %.0f s; last look %s, %.0f s ago; %d open stall%s; source %s)" % (
        beat.get("mode", "monitor"), beat.get("pid"), poll, hms(last), age, int(beat.get("open") or 0),
        "" if int(beat.get("open") or 0) == 1 else "s", ev["identity"])


def mode_alive(a, src):
    sid, spid = src.session()
    if not spid and not sid:
        print("stall-watch: no session here; nothing to check")
        return 2
    code, line = alive_status(sid or "no-session", src.engine_root, a.command)
    print(line)
    return code


def mode_once(a, src):
    sid, _spid = src.session()
    now = clock()
    try:
        stalls, problems = look(src, now, sid)
    except Exception as exc:  # noqa: BLE001
        print("stall-watch: could not look (%s: %s)" % (exc.__class__.__name__, exc))
        return 2
    if not stalls:
        print("stall-watch %s: no stalls" % hhmm(now))
    for s in stalls:
        print("\n".join(s.render(now)))
    for p in problems:
        print("  not read: " + p)
    return 1 if stalls else 0


def mode_notice(a):
    if not a.config:
        return 0
    th = thresholds()
    lines = [
        "=== STALL WATCH runs with this session (the CEO's build point: \"He never has to ask; every step and failure reaches him unprompted.\") ===",
        "  THE WATCHER STARTS BY ITSELF: the engine's plugin monitor `stall-watch` runs",
        "    %s --monitor" % a.command,
        "  for the whole session and looks every %.0f s for: a resource wait over %.0f min (proof-run slot, CPU admission," % (
            _env_float("STALL_WATCH_POLL_SECONDS", POLL_SECONDS), th["wait"] / 60),
        "  test VM, a device, any recorded wait); a live teammate with no commit AND no transcript write for %.0f min;" % (
            th["silent"] / 60),
        "  a teammate waiting %.0f min on a slot its own other process holds. Each stall reaches you ONCE as a" % (
            th["self"] / 60),
        "  notification naming the teammate, what it waits on, who holds it, since when and what you can do; again only",
        "  after another %.0f min; and in one line when it clears. It never pauses or stops a teammate. The one thing it" % (
            th["repeat"] / 60),
        "  ends: a teammate's own commands that keep its end waiting after its turn ended and have used no CPU to speak",
        "  of for %.0f min; one line says what it ended." % _env_float("STALL_WATCH_IDLE_END_MINUTES", IDLE_END_MINUTES),
        "  It also wakes you within a minute when a teammate raises an escalation in this session (an",
        "  ESCALATION-WATCH block, by state, never by wording), again every 10 min while one that needs the CEO",
        "  at a device (needs=ceo-hands) or every 30 min while a stopped one is unacknowledged; an ack ends it.",
        "  Do not start it yourself. Check it (one line): %s --alive" % a.command,
        "  Only if --alive says NOT WATCHED (plugin monitors do not run here), start `%s --watch` as a background" % a.command,
        "  command; it exits at each notification, so start it again after each one.",
    ]
    print("\n".join(lines))
    return 0


def main(argv):
    ap = argparse.ArgumentParser(prog="stall-watch.sh")
    g = ap.add_mutually_exclusive_group(required=True)
    for flag in ("--monitor", "--watch", "--alive", "--once", "--notice", "--tick"):
        g.add_argument(flag, action="store_true")
    ap.add_argument("--jsonl", action="store_true", help="frame monitor/watch/tick output as one JSON line per complete block")
    ap.add_argument("--host-json", action="store_true",
                    help="with --monitor: the app host's child, for the lead it names; JSON lines")
    ap.add_argument("--config", default="")
    ap.add_argument("--engine-root", default="")
    ap.add_argument("--command", default="stall-watch.sh")
    a = ap.parse_args(argv)
    if a.notice:
        return mode_notice(a)
    if a.monitor and not a.config:
        return 0  # a repository that never adopted the engine: nothing to watch, and nothing said
    src = Sources(a.engine_root)
    if a.alive:
        return mode_alive(a, src)
    if a.once:
        return mode_once(a, src)
    if a.tick:
        sid, _spid = src.session()
        sd = session_dir(sid)
        tick(src, sd, sid, json_lines=a.jsonl)
        # The source on disk AFTER the look; the parent compares it with the source when the
        # child started (_child_look) and records no version when they differ.
        ident, _newest = source_identity(src.engine_root)
        _write_json(os.path.join(sd, "evaluated.json"), {"identity": ident, "at": time.time(), "pid": os.getpid()})
        return 0
    if a.monitor and a.host_json:
        return run_host_loop(src)
    return run_loop(a, src, exit_on_notice=a.watch)


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except KeyboardInterrupt:
        sys.exit(130)

#!/usr/bin/env python3
"""resource_waits.py — NO JOB WAITS ON A SHARED RESOURCE FOR MORE THAN TEN MINUTES
                       WITHOUT THE LEAD BEING STOPPED AT TURN END.

===========================================================================
WHAT THIS EXISTS FOR
===========================================================================
The CEO, 2026-09-25: "A free Mac is used whole; nothing waits in a line."
The CEO, 2026-09-27, after a day on which jobs waited about six and a half
hours in total for the single test VM: "do I want the work to be BLOCKED AND
PISSED AWAY like this because the current worker might need the VM for a
5-second-long fart? Do I want HOURS OF DEVELOPMENT TIME TO BE PISSED AWAY FOR
EVERY 5-SECOND FART???" and then "how many more times will this exact
fuckshit keep repeating itself?"

The rule was written down on the 25th and it had no mechanism. On the 27th
three escalations, each raised after a 90-minute wait for the VM, were each
answered "keep waiting" within a minute (esc-20260927T135514Z-79cdca76,
esc-20260927T161022Z-59234edf, esc-20260927T170831Z-b7177ad3 in the ledger).
A disposition closed each demand and the wait went on. A rule enforced by
attention lasts exactly as long as the attention.

So this is a Stop gate on the lead: while any job has waited more than
RESOURCE_WAIT_MINUTES (10) on a shared resource, the lead's turn does not end.
The refusal names the waiter, what it waits for, who holds the resource, and
whether the holder is actually using it right now. IT CLEARS ONLY WHEN THE WAIT
ENDS. There is no acknowledgement, no disposition and no marker line that
clears it: "keep waiting" is exactly the answer the CEO asked to make
impossible.

===========================================================================
THE TWO SOURCES OF A WAIT, AND WHAT ENDS EACH
===========================================================================
A WAIT IS RECORDED, NEVER INFERRED. An earlier draft also read slot-queue
callers out of the process table by their script names; the lead ruled it out
(esc-20260927T192047Z-fb31d4e2): guessing waiters from process names is the
matched-process pattern the working rules forbid, and it breaks on any rename.
The waiter says it is waiting, through `waiting()`, or it is not counted.

1. WAIT RECORDS — `~/.richos-waits/<pid>-<token>.json` (RICHOS_WAITS_DIR).
   A process that waits on a shared resource writes one while it waits, with
   `waiting()` below, and removes it when the wait is over. Written by the CPU
   admission loop (app/scripts/testvm/reserve.py `cpu_admission`, which
   run-walk.py and every `reserve.py --wait` go through), by proof-run.py when
   it has nothing of its own running and admission refuses the next check, and
   by the test VM's slot queue (app/scripts/testvm/slots.py `guest_slot`, which
   `slots.py run --wait` and run-walk.py go through).
   `resource_waits.py wait --until-exists PATH` records a hand-rolled wait for
   a file (the lead's `vm-queue/<name>-go` protocol) the same way.
   ENDS: the record is removed, or its process is gone (a dead pid, or a pid
   reused by a process that started after the wait did).

2. THE ESCALATION LEDGER — an Escalation whose title, question or meanwhile
   says the teammate is WAITING for a shared resource (the VM, a guest, a
   go-file, a slot, CPU admission). Its wait began when it was raised, minus
   the minutes it says it has already waited ("has waited 90 min", "waiting
   since 12:23Z"). Repeated escalations for one wait are one wait.
   An EscalationAck NEVER ends it. That is the whole point.
   ENDS: the go-file it names exists; its worktree no longer exists; or the
   waiter records the end itself with `resource_waits.py wait-over <id> --note
   "<what ended the wait>"`, which refuses a note that says to keep waiting.

===========================================================================
A HELD AGENT IS NOT WAITING IN LINE (2026-10-11)
===========================================================================
When the lead freezes a teammate (agent_hold.py, through a generated pause),
a wait that teammate had recorded is not a job waiting in line: the lead chose
to hold it. On 2026-10-11 such a wait blocked the lead at 04:26Z, during the
CEO's account-switch hold, and at 06:19Z forced three teammates frozen for
Codex's final proof to be resumed early. So a wait whose agent has a hold
record standing (agent_hold.py's held/ directory) is HELD, not waiting: `list`
shows it as held, the Stop gate and stall-watch skip it, and its clock stops.
On RESUME it counts again from where it was: every hold that covered part of
the wait (agent_hold.py's hold-history.jsonl, written at each release) is
subtracted from its age. A wait belongs to a hold when its waiter is the held
agent's name (or `agent-<id>`), or when its process is one the hold froze
(pid and start time from the hold record).

===========================================================================
WHO HOLDS IT, AND IS THE HOLDER USING IT
===========================================================================
TEST VM. Holders are read from the slot files' own holder records
(`<TESTVM_ROOT>/guest.lock`, `guest-2.lock`: `{"pid", "since", "purpose"}`
while held) and from any wait record whose `holding` names a slot file (a
walk that holds guest.lock while it waits for CPU is a holder that is NOT
using the VM). Whether a run executes is read from `<TESTVM_ROOT>/run/<vm>/`:
a live `vm.pid` is a booted guest, its process CPU is how busy the guest is,
and a `hold-walk.py` among the holder's own descendants (found from the
holder's pid, never by searching for the name) is a guest held open for
hand-driven steps rather than a scripted run. That only changes the WORDS of
the refusal; whether a turn is refused never depends on a process name. The lock files are NEVER probed with flock:
a probe that took the lock for a microsecond could refuse a real walk
(run-walk.py takes the lock without waiting). An older checkout that holds
guest.lock without writing a holder record is reported as such.

CPU ADMISSION. There is no single holder; the refusal names the processes
using the most CPU right now (`ps` %CPU) and the other waits, so the lead can
decide what to stop or reorder.

===========================================================================
COST
===========================================================================
Per turn end: one directory listing, one read of the escalation ledger and a
few stat calls. One `ps` call (37 ms median, measured) is added only when a
wait has reached the threshold: to confirm its pid was not reused and to say
who holds the resource. BUDGET_SECONDS bounds it. The whole hook's cost,
measured, is resource-waits.test.sh's RW36 line.

Exit codes of `stop`:
    0  the turn may end; stdout is one `RW<TAB>kind<TAB>detail` line
    2  REFUSED: a wait is over the threshold; the refusal is on stderr
    3  this file could not evaluate
"""

import argparse
import json
import os
import re
import subprocess
import sys
import time
import uuid

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

SCHEMA = "richos-wait/1"
DEFAULT_WAIT_MINUTES = 10
BUDGET_SECONDS = 2.0
SLOT_FILES = ("guest.lock", "guest-2.lock")
MIN_WAIT_OVER_NOTE = 30
HOOK_TAG = "(hook: scripts/hooks/guard-resource-waits.sh)"

# Resource ids a writer uses, and how the refusal names them.
VM = "testvm-slot"
CPU = "cpu-admission"
LABELS = {VM: "the test VM", CPU: "CPU admission"}

# A note that says the wait goes on is not the end of the wait.
KEEP_WAITING = re.compile(
    r"keep\s+waiting|still\s+waiting|continue\s+waiting|wait\s+(?:for|until|on)\b|"
    r"\bis\s+next\b|\bnext\s+(?:on|in|for)\b|\bin\s+line\b|\bqueued?\b|"
    r"\bwill\s+(?:get|have|receive)\b|\bafter\s+.{0,60}\b(?:release|releases|walk|finishes)\b",
    re.IGNORECASE)

WAIT_WORD = re.compile(r"\bwait(?:s|ed|ing)?\b", re.IGNORECASE)
# A CLAIM that the teammate itself is waiting, not the word "waiting" used about
# waiting in general ("two slots will not end the waiting" is a design argument,
# not a waiter). Calibrated on the real ledger, 2026-09-27: resource-waits.test.sh
# carries the true and false cases it was calibrated on.
_DUR = r"\d+(?:\.\d+)?\+?\s*(?:min|mins|minutes?|h|hrs?|hours?)\b"
WAIT_CLAIM = re.compile(
    r"\b(?:has|have|had)\s+(?:now\s+)?waited\b"
    r"|\bwaiting\s+(?:in\s+the\s+foreground\s+)?(?:for|on)\s+(?:the\s+|a\s+)?"
    r"(?:test\s+)?(?:VM|guest|go[- ]?file|slot|CPU|admission)\b"
    r"|\b" + _DUR + r"\s+(?:of\s+)?(?:waiting|without)\b"
    r"|\bwaiting\s+since\b"
    r"|\bqueued\s+behind\b"
    r"|\b(?:not\s+admitted|refused|refuses|refusing)\b[^.;]{0,60}\bfor\s+" + _DUR +
    r"|\b(?:cannot|can't|could\s+not|never)\s+(?:be\s+)?admitted\b",
    re.IGNORECASE)
NEGATED = re.compile(r"\b(?:no\s+longer|not|never|stopped|done|finished|without\s+any)\s*$", re.IGNORECASE)
RESOURCE_WORDS = (
    (CPU, re.compile(r"\b(?:CPU|admission|admitted)\b", re.IGNORECASE)),
    (VM, re.compile(r"\b(?:VM|guest|go[- ]?file|vm-queue|guest\.lock|slot)\b", re.IGNORECASE)),
)
GO_FILE = re.compile(r"(~?/[\w.@/-]*vm-queue/[\w.@-]+-go)\b")
WAITED_FOR = re.compile(
    r"\b(\d+(?:\.\d+)?)\s*(h|hr|hrs|hours?|m|min|mins|minutes?)\b(?:[^.;]{0,20}\bwait|\s+(?:waiting|of\s+waiting|without))",
    re.IGNORECASE)
WAITED_FOR_2 = re.compile(
    r"\bwait(?:ed|ing)\s+(?:for\s+)?(\d+(?:\.\d+)?)\s*(h|hr|hrs|hours?|m|min|mins|minutes?)\b",
    re.IGNORECASE)
WAITING_SINCE = re.compile(r"\bsince\s+(\d{1,2}):(\d{2})\s*Z\b", re.IGNORECASE)


# ---------------------------------------------------------------------------
# PATHS
# ---------------------------------------------------------------------------
def waits_dir():
    return os.path.abspath(os.path.expanduser(
        os.environ.get("RICHOS_WAITS_DIR") or os.path.join("~", ".richos-waits")))


def testvm_root():
    return os.path.abspath(os.path.expanduser(
        os.environ.get("TESTVM_ROOT") or os.path.join("~", ".richos-testvm")))


def clock():
    """Now, as the gate judges ages. RICHOS_RESOURCE_WAITS_NOW (epoch seconds)
    exists for resource-waits.test.sh, which cannot make a real process wait
    eleven minutes; nothing else sets it. Process start times are always read
    against the real clock, so moving this cannot resurrect a dead wait."""
    raw = os.environ.get("RICHOS_RESOURCE_WAITS_NOW", "")
    try:
        return float(raw) if raw.strip() else time.time()
    except ValueError:
        return time.time()


def wait_minutes():
    raw = os.environ.get("RICHOS_RESOURCE_WAIT_MINUTES", "")
    try:
        value = float(raw) if raw.strip() else DEFAULT_WAIT_MINUTES
    except ValueError:
        value = DEFAULT_WAIT_MINUTES
    return value if value > 0 else DEFAULT_WAIT_MINUTES


# ---------------------------------------------------------------------------
# THE WRITER — a process waiting on a shared resource says so while it waits
# ---------------------------------------------------------------------------
def _waiter_from_cwd(cwd):
    """The teammate's name when the process runs in a teammate workspace
    (`.../<repo>-wt/<name>/...` or `.claude/worktrees/<name>`), else the cwd."""
    parts = cwd.split(os.sep)
    for i, part in enumerate(parts[:-1]):
        if part.endswith("-wt") or part == "worktrees":
            return parts[i + 1]
    return cwd


class Wait(object):
    """One wait, recorded while it lasts. Never raises into the caller: a wait
    that cannot be recorded is announced on stderr once and the caller goes on
    waiting, because breaking the admission rule to report on it would be worse.
    """

    def __init__(self, resource, reason="", holding=(), waiter=None, since=None, directory=None):
        self.resource = resource
        self.reason = reason
        self.holding = [str(h) for h in holding]
        cwd = os.getcwd()
        self.record = {
            "schema": SCHEMA,
            "pid": os.getpid(),
            "since": time.time() if since is None else float(since),
            "resource": resource,
            "waiter": waiter or os.environ.get("RICHOS_WAITER") or _waiter_from_cwd(cwd),
            "reason": reason,
            "holding": self.holding,
            "cwd": cwd,
            "command": " ".join(sys.argv)[:300],
        }
        self.directory = directory or waits_dir()
        self.path = os.path.join(self.directory, "%d-%s.json" % (os.getpid(), uuid.uuid4().hex[:8]))
        self.warned = False
        self.open = False

    def _write(self):
        self.record["updated"] = time.time()
        tmp = self.path + ".tmp"
        try:
            os.makedirs(self.directory, mode=0o700, exist_ok=True)
            with open(tmp, "w", encoding="utf-8") as fh:
                fh.write(json.dumps(self.record) + "\n")
            os.replace(tmp, self.path)
            self.open = True
        except OSError as exc:
            if not self.warned:
                self.warned = True
                sys.stderr.write("resource wait NOT RECORDED at %s (%s): the lead's turn-end "
                                 "gate cannot see this wait\n" % (self.directory, exc))

    def start(self):
        prune(self.directory)
        self._write()
        return self

    def update(self, reason):
        if reason != self.record.get("reason"):
            self.record["reason"] = reason
            self._write()

    def close(self):
        try:
            os.unlink(self.path)
        except OSError:
            pass
        self.open = False

    def __enter__(self):
        return self.start()

    def __exit__(self, *exc):
        self.close()
        return False


def waiting(resource, reason="", holding=(), waiter=None, since=None, directory=None):
    """`with waiting(CPU, reason): ...` — recorded for exactly the body."""
    return Wait(resource, reason, holding, waiter, since, directory)


def prune(directory=None):
    """Remove records whose process is gone. Best effort; the reader ignores
    them anyway, this only keeps the directory from growing."""
    directory = directory or waits_dir()
    try:
        names = os.listdir(directory)
    except OSError:
        return
    for name in names:
        if not name.endswith(".json"):
            continue
        try:
            pid = int(name.split("-", 1)[0])
        except ValueError:
            continue
        if not pid_alive(pid):
            try:
                os.unlink(os.path.join(directory, name))
            except OSError:
                pass


# ---------------------------------------------------------------------------
# PROCESS FACTS
# ---------------------------------------------------------------------------
def pid_alive(pid):
    try:
        pid = int(pid)
    except (TypeError, ValueError):
        return False
    if pid <= 0:
        return False
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    except OSError:
        return False
    return not _is_zombie(pid)


def _is_zombie(pid):
    """kill(pid, 0) succeeds on a zombie (exited, not yet reaped by its parent,
    which under load can take seconds). A zombie waits for nothing."""
    try:
        out = subprocess.run(["ps", "-o", "stat=", "-p", str(pid)],
                             capture_output=True, text=True, timeout=5).stdout
    except Exception:
        return False
    return out.strip().startswith("Z")


def parse_etime(text):
    """`ps` elapsed time: [[dd-]hh:]mm:ss -> seconds, or None."""
    text = (text or "").strip()
    days = 0
    if "-" in text:
        d, text = text.split("-", 1)
        if not d.isdigit():
            return None
        days = int(d)
    parts = text.split(":")
    if not parts or not all(p.isdigit() for p in parts) or len(parts) > 3:
        return None
    secs = 0
    for p in parts:
        secs = secs * 60 + int(p)
    return days * 86400 + secs


class Proc(object):
    __slots__ = ("pid", "ppid", "age", "cpu", "command")

    def __init__(self, pid, ppid, age, cpu, command):
        self.pid, self.ppid, self.age, self.cpu, self.command = pid, ppid, age, cpu, command

    def argv(self):
        return self.command.split()


def ps_table(deadline):
    """{pid: Proc} from ONE `ps` call bounded by the deadline, or (None, why)."""
    left = deadline - time.time()
    if left <= 0.05:
        return None, "the %.1fs budget was spent before the process table was read" % BUDGET_SECONDS
    try:
        out = subprocess.run(["ps", "-A", "-o", "pid=,ppid=,etime=,pcpu=,command="],
                             capture_output=True, text=True, timeout=left).stdout
    except subprocess.TimeoutExpired:
        return None, "`ps` did not answer inside the %.1fs budget" % BUDGET_SECONDS
    except OSError as exc:
        return None, "`ps` could not be run (%s)" % exc
    table = {}
    for line in out.splitlines():
        parts = line.split(None, 4)
        if len(parts) < 5 or not parts[0].isdigit() or not parts[1].isdigit():
            continue
        try:
            cpu = float(parts[3])
        except ValueError:
            cpu = 0.0
        table[int(parts[0])] = Proc(int(parts[0]), int(parts[1]), parse_etime(parts[2]), cpu, parts[4])
    return table, ""


def children_of(table):
    kids = {}
    for p in table.values():
        kids.setdefault(p.ppid, []).append(p.pid)
    return kids


def descendants(pid, table, kids=None):
    kids = kids if kids is not None else children_of(table)
    out, stack, seen = [], list(kids.get(pid, [])), set()
    while stack:
        c = stack.pop()
        if c in seen:
            continue
        seen.add(c)
        out.append(c)
        stack.extend(kids.get(c, []))
    return out


def short(command, limit=90):
    """A command, shortened for a refusal: script basenames kept, paths trimmed."""
    words = []
    for w in (command or "").split():
        if "/" in w and not w.startswith("-"):
            w = os.path.basename(w.rstrip("/")) or w
        words.append(w)
    s = " ".join(words)
    return s if len(s) <= limit else s[:limit - 3] + "..."


def minutes(seconds):
    if seconds is None:
        return "an unknown time"
    m = int(seconds // 60)
    if m < 120:
        return "%d min" % m
    return "%dh %02dm" % (m // 60, m % 60)


# ---------------------------------------------------------------------------
# SOURCE 1 — WAIT RECORDS
# ---------------------------------------------------------------------------
def read_records(directory=None):
    """[(path, record)] for every parseable record; unparseable ones are counted."""
    directory = directory or waits_dir()
    out, bad = [], 0
    try:
        names = sorted(os.listdir(directory))
    except FileNotFoundError:
        return out, bad
    except OSError:
        return None, 0
    for name in names:
        if not name.endswith(".json"):
            continue
        path = os.path.join(directory, name)
        try:
            with open(path, encoding="utf-8") as fh:
                rec = json.load(fh)
        except (OSError, ValueError):
            bad += 1
            continue
        if isinstance(rec, dict) and rec.get("schema") == SCHEMA:
            out.append((path, rec))
        else:
            bad += 1
    return out, bad


def record_waits(records, now):
    """Live waits from records. A record whose pid is dead is over."""
    waits = []
    for path, rec in records:
        pid = rec.get("pid")
        if not pid_alive(pid):
            continue
        try:
            since = float(rec.get("since"))
        except (TypeError, ValueError):
            since = now
        waits.append({
            "source": "record", "path": path, "pid": int(pid), "since": since,
            "resource": str(rec.get("resource") or "unknown"),
            "waiter": str(rec.get("waiter") or "pid %s" % pid),
            "reason": str(rec.get("reason") or ""),
            "holding": [str(h) for h in (rec.get("holding") or [])],
            "command": str(rec.get("command") or ""),
            "until": str(rec.get("until") or ""),
        })
    return waits


def drop_reused_pids(waits, table):
    """A record whose pid now belongs to a process that started AFTER the wait
    began is a dead wait whose pid was reused."""
    if table is None:
        return waits
    now = time.time()
    kept = []
    for w in waits:
        if w["source"] != "record":
            kept.append(w)
            continue
        p = table.get(w["pid"])
        if p is None:
            continue
        if p.age is not None and (now - p.age) > w.get("began", w["since"]) + 5:
            continue
        kept.append(w)
    return kept


# ---------------------------------------------------------------------------
# HELD AGENTS — a wait the lead froze is not a wait in line
# ---------------------------------------------------------------------------
HOLD_BIRTH_SLACK = 2.0  # agent_hold.py reads start times from `ps -o lstart` (one-second resolution)


def holds():
    """([{name, agent_id, at, until, held}], problem): every hold standing now
    (until None) and every released one (agent_hold.py's records and history)."""
    try:
        import agent_hold  # noqa: WPS433 (sibling module in scripts/lib)
    except Exception as exc:  # noqa: BLE001
        return [], "the agent hold records could not be loaded (%s)" % exc.__class__.__name__
    out = []
    for rec, until in ([(r, None) for r in agent_hold.records()] +
                       [(r, r.get("released")) for r in agent_hold.history()]):
        try:
            at = float(rec.get("at"))
            until = None if until is None else float(until)
        except (TypeError, ValueError):
            continue
        held = rec.get("held") if isinstance(rec.get("held"), dict) else {}
        out.append({"name": str(rec.get("name") or ""), "agent_id": str(rec.get("agent_id") or ""),
                    "at": at, "until": until, "held": held})
    return out, ""


def _hold_covers(w, h):
    """True when this hold is of the wait's agent: by name, or by a process it froze."""
    names = {h["name"], h["agent_id"], "agent-" + h["agent_id"] if h["agent_id"] else ""} - {""}
    if any(n in names for n in [w.get("waiter")] + list(w.get("names") or [])):
        return True
    birth = h["held"].get(str(w.get("pid")))
    # The frozen process started before its wait did; a reused pid started later.
    return isinstance(birth, (int, float)) and birth <= w.get("began", w["since"]) + HOLD_BIRTH_SLACK


def apply_holds(waits, hold_list, now):
    """Mark each wait held or not and take the held time out of its age.

    Sets `began` (when the wait really began), `held_by` (the standing hold, or
    None), `held_seconds` (time inside [began, now] some hold of its agent
    covered) and moves `since` forward by that, so every age read from `since`
    stops while held and resumes from where it was."""
    for w in waits:
        began = w.get("began", w["since"])
        w["began"] = began
        spans, standing = [], None
        for h in hold_list:
            if not _hold_covers(w, h):
                continue
            if h["until"] is None:
                standing = h
            start, end = max(h["at"], began), min(now if h["until"] is None else h["until"], now)
            if end > start:
                spans.append((start, end))
        total, reach = 0.0, began  # overlapping holds are counted once
        for start, end in sorted(spans):
            start = max(start, reach)
            if end > start:
                total += end - start
                reach = end
        w["held_by"] = standing
        w["held_seconds"] = total
        w["since"] = began + total
    return waits


# ---------------------------------------------------------------------------
# SOURCE 2 — THE ESCALATION LEDGER
# ---------------------------------------------------------------------------
def _escalations():
    try:
        import escalations  # noqa: WPS433 (sibling module in scripts/lib)
        return escalations
    except Exception:
        return None


def classify(title, text=""):
    """The resource an escalation says its teammate is waiting for, or None.

    The claim must be made (WAIT_CLAIM) in the title or the body. The resource
    is read from the sentence that makes the claim first, then from the title,
    then from the whole text, so "CPU admission has refused the verifier for 34
    minutes ... the VM re-run" is a CPU wait."""
    whole = " ".join(t for t in (title, text) if t)
    m = None
    for cand in WAIT_CLAIM.finditer(whole):
        # "no longer waiting for the VM" is the end of a wait, not a wait.
        if NEGATED.search(whole[max(0, cand.start() - 24):cand.start()]):
            continue
        m = cand
        break
    if m is None:
        return None
    start = max(whole.rfind(".", 0, m.start()), whole.rfind(";", 0, m.start())) + 1
    end_candidates = [i for i in (whole.find(".", m.end()), whole.find(";", m.end())) if i >= 0]
    sentence = whole[start:min(end_candidates) if end_candidates else len(whole)]
    for scope in (sentence, title or "", whole):
        for resource, pattern in RESOURCE_WORDS:
            if pattern.search(scope):
                return resource
    return None


def _stated_wait_seconds(text):
    best = 0.0
    for pattern in (WAITED_FOR, WAITED_FOR_2):
        for m in pattern.finditer(text or ""):
            n = float(m.group(1))
            unit = m.group(2).lower()
            secs = n * 3600 if unit.startswith("h") else n * 60
            best = max(best, secs)
    return best


def _since_from_text(text, raised_epoch):
    """`waiting since 12:23Z` on the day it was raised (the day before when that
    time is later than the raise)."""
    m = WAITING_SINCE.search(text or "")
    if not m:
        return None
    hh, mm = int(m.group(1)), int(m.group(2))
    if hh > 23 or mm > 59:
        return None
    day = raised_epoch - (raised_epoch % 86400)
    t = day + hh * 3600 + mm * 60
    if t > raised_epoch:
        t -= 86400
    return t


def escalation_waits(rows, now):
    """One wait per (teammate, resource), from the ledger. Acks are IGNORED."""
    esc = _escalations()
    parse_iso = esc.parse_iso if esc else (lambda s: None)
    over = {}
    for r in rows:
        if r.get("event") == "WaitOver" and r.get("id"):
            t = parse_iso(r.get("at"))
            over.setdefault(str(r["id"]), []).append(t.timestamp() if t else now)
    groups = {}
    for r in rows:
        if r.get("event") != "Escalation":
            continue
        text = " ".join(str(r.get(k) or "") for k in ("title", "question", "meanwhile"))
        resource = classify(str(r.get("title") or ""),
                            " ".join(str(r.get(k) or "") for k in ("question", "meanwhile")))
        if not resource:
            continue
        raised = parse_iso(r.get("raised"))
        raised_epoch = raised.timestamp() if raised else now
        since = _since_from_text(text, raised_epoch)
        if since is None:
            since = raised_epoch - _stated_wait_seconds(r.get("title") or "")
        go = GO_FILE.search(text)
        # The session that raised it is part of whose wait it is: in the app every chat
        # has its own teammates, and two chats' "echo-opus-wait" are two waits.
        key = (str(r.get("session_id") or ""),
               str(r.get("teammate") or r.get("worktree") or r.get("id")), resource)
        groups.setdefault(key, []).append({
            "id": str(r.get("id") or ""), "raised": raised_epoch, "since": since,
            "worktree": str(r.get("worktree") or ""), "go": go.group(1) if go else "",
            "title": str(r.get("title") or ""),
        })
    # A row that names no session joins every session's wait for the same teammate and
    # resource (one wait each, as before), and is its own wait only when none names one.
    for (session, teammate, resource) in [k for k in groups if not k[0]]:
        named = [k for k in groups if k[0] and k[1:] == (teammate, resource)]
        for k in named:
            groups[k] = groups[k] + groups[("", teammate, resource)]
        if named:
            del groups[("", teammate, resource)]
    waits = []
    for (session, teammate, resource), items in groups.items():
        items = sorted(items, key=lambda i: i["raised"])
        # A WaitOver for any of them ends every one raised before it; a later
        # escalation that still says "waiting" is a new wait from its own time.
        ends = [t for i in items for t in over.get(i["id"], [])]
        last_end = max(ends) if ends else None
        live = [i for i in items if last_end is None or i["raised"] > last_end]
        if not live:
            continue
        latest = live[-1]
        go = next((i["go"] for i in reversed(live) if i["go"]), "")
        if go and os.path.exists(os.path.expanduser(go)):
            continue
        worktree = latest["worktree"]
        if worktree and not os.path.isdir(worktree):
            continue
        waits.append({
            "source": "escalation", "pid": None,
            "since": min(i["since"] for i in live), "resource": resource,
            "waiter": teammate,
            "reason": "escalation %s: %s" % (latest["id"], latest["title"][:160]),
            "holding": [], "command": "", "until": go,
            "escalations": [i["id"] for i in live], "worktree": worktree,
            "session": session,
        })
    return waits


# ---------------------------------------------------------------------------
# HOLDERS
# ---------------------------------------------------------------------------
def vm_facts(root, table, waits):
    """What holds the test VM and whether a run is executing in a guest."""
    now = time.time()
    kids = children_of(table) if table else {}
    slots = []
    for name in SLOT_FILES:
        path = os.path.join(root, name)
        if not os.path.exists(path):
            continue
        try:
            with open(path, encoding="utf-8") as fh:
                raw = fh.read().strip()
            rec = json.loads(raw) if raw else {}
        except (OSError, ValueError):
            rec = {}
        pid = rec.get("pid") if isinstance(rec, dict) else None
        slots.append({"name": name, "path": path,
                      "pid": int(pid) if pid and pid_alive(pid) else None,
                      "since": rec.get("since") if isinstance(rec, dict) else None,
                      "purpose": str(rec.get("purpose") or "") if isinstance(rec, dict) else "",
                      "state": str(rec.get("state") or "") if isinstance(rec, dict) else ""})
    guests = []
    run = os.path.join(root, "run")
    try:
        states = sorted(os.listdir(run))
    except OSError:
        states = []
    for vm in states:
        state = os.path.join(run, vm)
        try:
            with open(os.path.join(state, "vm.pid"), encoding="utf-8") as fh:
                vpid = int(fh.read().strip() or 0)
            booted = os.path.getmtime(os.path.join(state, "vm.pid"))
        except (OSError, ValueError):
            continue
        if not pid_alive(vpid):
            continue
        try:
            with open(os.path.join(state, "slot"), encoding="utf-8") as fh:
                slot = fh.read().strip()
        except OSError:
            slot = ""
        cpu = None
        if table is not None:
            tree = [vpid] + descendants(vpid, table, kids)
            cpu = sum(table[p].cpu for p in tree if p in table)
        guests.append({"vm": vm, "pid": vpid, "booted": booted, "slot": slot, "cpu": cpu})
    # A wait record that HOLDS a slot file names a holder the lock file cannot.
    for w in waits:
        for h in w.get("holding") or []:
            for s in slots:
                if os.path.realpath(h) == os.path.realpath(s["path"]) and s["pid"] is None:
                    s["pid"] = w["pid"]
                    s["since"] = w["since"]
                    s["purpose"] = s["purpose"] or short(w["command"], 80)
                    s["holder_waiting"] = w
    lines = []
    for s in slots:
        if s["pid"] is None:
            continue
        mine = [g for g in guests if g["slot"] and os.path.realpath(g["slot"]) == os.path.realpath(s["path"])]
        if not mine and len([x for x in slots if x["pid"]]) == 1:
            mine = [g for g in guests if not g["slot"]]
        held_for = minutes(now - s["since"]) if isinstance(s.get("since"), (int, float)) else "an unknown time"
        who = "%s held by pid %d for %s (%s)" % (s["name"], s["pid"], held_for, s["purpose"] or "no purpose recorded")
        if s.get("holder_waiting"):
            hw = s["holder_waiting"]
            lines.append("%s. IN USE RIGHT NOW: NO. No guest is booted; the holder is itself waiting "
                         "for %s (%s) and has been for %s." % (
                             who, LABELS.get(hw["resource"], hw["resource"]), hw["reason"] or "no reason recorded",
                             minutes(now - hw["since"])))
            continue
        if not mine and s.get("state") == "admitting":
            lines.append("%s. IN USE RIGHT NOW: NO. The holder is still being admitted (slots.py's "
                         "\"admitting\" state): it holds the slot while it checks CPU and memory, and "
                         "no run has started." % who)
            continue
        if not mine:
            lines.append("%s. IN USE RIGHT NOW: NO. The slot is held but no guest is booted." % who)
            continue
        scripts = []
        hand_held = False
        if table is not None:
            for d in descendants(s["pid"], table, kids):
                for a in table[d].argv()[:3]:
                    b = os.path.basename(a)
                    if b == "hold-walk.py":
                        hand_held = True
                    if (b.endswith(".py") or b.endswith(".sh")) and b not in scripts and b != "run-walk.py":
                        scripts.append(b)
        for g in mine:
            guest = "guest %s booted %s ago, guest CPU %s" % (
                g["vm"], minutes(now - g["booted"]), "%.0f%%" % g["cpu"] if g["cpu"] is not None else "unknown")
            if hand_held:
                lines.append("%s. IN USE RIGHT NOW: HELD OPEN, NOT RUNNING. hold-walk.py keeps the guest up for "
                             "hand-driven steps; no scripted run is executing (%s)." % (who, guest))
            else:
                lines.append("%s. IN USE RIGHT NOW: YES, a run is executing (%s%s)." % (
                    who, guest, "; running " + ", ".join(scripts[:4]) if scripts else ""))
    unowned = [g for g in guests
               if not any(g["slot"] and os.path.realpath(g["slot"]) == os.path.realpath(s["path"]) and s["pid"]
                          for s in slots)
               and not (len([x for x in slots if x["pid"]]) == 1 and not g["slot"])]
    for g in unowned:
        lines.append("guest %s is booted (%s ago, guest CPU %s) with no holder record: an older checkout's "
                     "run-walk.py holds guest.lock without writing one, or it was booted outside a slot." % (
                         g["vm"], minutes(now - g["booted"]),
                         "%.0f%%" % g["cpu"] if g["cpu"] is not None else "unknown"))
    if not lines:
        lines.append("NO HOLDER IS RECORDED AND NO GUEST IS RUNNING: as far as this gate can read, the VM "
                     "is FREE and the waiter could be running now.")
    return lines, slots, guests


def cpu_facts(table, top=5):
    if table is None:
        return ["the process table could not be read, so who is using the CPU is unknown"]
    me = os.getpid()
    rows = sorted((p for p in table.values() if p.pid != me and p.cpu > 0),
                  key=lambda p: p.cpu, reverse=True)[:top]
    if not rows:
        return ["no process is using measurable CPU right now: admission should open at the next sample"]
    total = sum(p.cpu for p in table.values())
    cores = os.cpu_count() or 1
    head = "the Mac is using about %.0f%% of %d cores (ps %%CPU, summed); the largest users right now:" % (
        total / cores, cores)
    return [head] + ["pid %d at %.0f%% CPU for %s: %s" % (p.pid, p.cpu, minutes(p.age), short(p.command))
                     for p in rows]


# ---------------------------------------------------------------------------
# THE WHOLE READING
# ---------------------------------------------------------------------------
def collect(now=None, threshold_seconds=None, ledger_rows=None):
    """Every current wait, with holder facts for the resources the overdue ones
    wait on. Returns a dict; never raises for an unreadable source, it reports it."""
    now = now or clock()
    threshold = threshold_seconds if threshold_seconds is not None else wait_minutes() * 60
    deadline = time.time() + BUDGET_SECONDS
    problems = []
    records, bad = read_records()
    if records is None:
        problems.append("the wait records at %s could not be read" % waits_dir())
        records = []
    if bad:
        problems.append("%d unreadable file(s) in %s" % (bad, waits_dir()))
    waits = record_waits(records, now)
    if ledger_rows is None:
        esc = _escalations()
        rows = None
        if esc is not None:
            rows, _bad = esc.read_rows()
        if rows is None:
            problems.append("the escalation ledger could not be read")
            rows = []
    else:
        rows = ledger_rows
    waits += escalation_waits(rows, now)
    hold_list, why = holds()
    if why:
        problems.append(why)
    apply_holds(waits, hold_list, now)
    held = sorted((w for w in waits if w["held_by"]), key=lambda w: w["since"])
    waits = [w for w in waits if not w["held_by"]]

    root = testvm_root()
    table = None
    if any(now - w["since"] >= threshold for w in waits):
        # Only now is the process table worth its 37 ms: to drop a record whose
        # pid was reused, and to say who holds what the overdue wait waits for.
        table, why = ps_table(deadline)
        if table is None:
            problems.append(why)
        waits = drop_reused_pids(waits, table)
    waits.sort(key=lambda w: w["since"])
    overdue = [w for w in waits if now - w["since"] >= threshold]
    facts = {}
    if any(w["resource"] == VM for w in overdue):
        facts[VM] = vm_facts(root, table, waits + held)[0]
    if any(w["resource"] == CPU for w in overdue):
        facts[CPU] = cpu_facts(table)
    return {"now": now, "threshold": threshold, "waits": waits, "overdue": overdue, "held": held,
            "holds": hold_list, "facts": facts, "problems": problems, "ps_read": table is not None}


def _hhmm(epoch):
    return time.strftime("%H:%MZ", time.gmtime(epoch))


def describe_held(w, now):
    label = LABELS.get(w["resource"], w["resource"])
    h = w["held_by"]
    where = ("pid %d" % w["pid"]) if w["source"] == "record" else \
        "from the escalation ledger: %s" % ", ".join(w.get("escalations") or [])
    return ("%s is HELD by the lead (hold of %s since %s), so its wait for %s is not counted: stopped at %s, "
            "it resumes from there on RESUME (%s)." % (
                w["waiter"], h["name"] or h["agent_id"], _hhmm(h["at"]), label,
                minutes(now - w["since"]), where))


def describe_wait(w, now):
    label = LABELS.get(w["resource"], w["resource"])
    line = "%s has waited %s for %s" % (w["waiter"], minutes(now - w["since"]), label)
    bits = []
    if w["source"] == "record":
        bits.append("pid %d, recorded in %s" % (w["pid"], os.path.basename(w["path"])))
    else:
        bits.append("from the escalation ledger: %s" % ", ".join(w.get("escalations") or []))
    if w.get("held_seconds", 0) >= 60:
        bits.append("%s of it held by the lead, not counted" % minutes(w["held_seconds"]))
    if w.get("until"):
        bits.append("waiting for %s to exist" % w["until"])
    if w.get("reason"):
        bits.append("last refusal: %s" % w["reason"][:220])
    return line + " (" + "; ".join(bits) + ")."


def refusal(reading):
    now = reading["now"]
    n = len(reading["overdue"])
    out = ["RESOURCE WAIT OVER %d MIN: THIS TURN DOES NOT END WHILE A JOB WAITS IN LINE. %s" % (
               int(reading["threshold"] // 60), HOOK_TAG),
           "",
           "The CEO, 2026-09-25: \"A free Mac is used whole; nothing waits in a line.\" "
           "On 2026-09-27 jobs waited hours for the single test VM while each 90-minute escalation was "
           "answered \"keep waiting\". This gate exists so that answer is no longer available.",
           "",
           "%d wait(s) over %d minutes:" % (n, int(reading["threshold"] // 60))]
    for i, w in enumerate(reading["overdue"], 1):
        out.append("  %d. %s" % (i, describe_wait(w, now)))
    for resource in (VM, CPU):
        lines = reading["facts"].get(resource)
        if not lines:
            continue
        out.append("")
        out.append("Who holds %s, and whether it is being used right now:" % LABELS[resource])
        for line in lines:
            out.append("  - " + line)
    if reading["problems"]:
        out.append("")
        out.append("Not read this turn (the refusal stands on what WAS read): " + "; ".join(reading["problems"]))
    out += ["",
            "THIS CLEARS ONLY WHEN THE WAIT ENDS: the waiter gets the resource, stops waiting, or ends. "
            "An acknowledgement, a disposition, a marker line or \"keep waiting\" does not clear it.",
            "Decide now, one of: give the waiter the resource (reclaim a slot whose holder is not using it, "
            "or create the go-file it is polling for); reorder (stop or defer the holder, or stop the job "
            "hogging the CPU); or tell the waiter to stop waiting and do other work.",
            "A teammate whose wait ended some other way records it from its own worktree: "
            "resource_waits.py wait-over <escalation-id> --note \"<what ended the wait>\" "
            "(scripts/lib/resource_waits.py in the engine).",
            "Every current wait, any time: python3 <engine>/scripts/lib/resource_waits.py list"]
    return "\n".join(out)


# ---------------------------------------------------------------------------
# COMMANDS
# ---------------------------------------------------------------------------
def cmd_stop():
    try:
        payload = json.load(sys.stdin)
    except Exception:
        print("RW\tcannot\tthe Stop payload could not be parsed")
        return 0
    if not isinstance(payload, dict):
        print("RW\tcannot\tthe Stop payload is not an object")
        return 0
    if payload.get("agent_id"):
        # A teammate is never refused; the lead is the one who can reorder.
        print("RW\tnone\tteammate")
        return 0
    reading = collect()
    if not reading["overdue"]:
        waits = reading["waits"]
        detail = "%d wait(s), none over %d min" % (len(waits), int(reading["threshold"] // 60))
        if reading["problems"]:
            print("RW\tcannot\tnot every wait source could be read (%s); %s" % (
                "; ".join(reading["problems"]), detail))
        else:
            print("RW\tnone\t" + detail)
        return 0
    sys.stderr.write(refusal(reading) + "\n")
    print("RW\tblocked\t%d wait(s) over the threshold" % len(reading["overdue"]))
    return 2


def cmd_list(args):
    reading = collect(threshold_seconds=0 if args.all else None)
    now = reading["now"]
    if not reading["waits"]:
        print("no job is waiting on a shared resource")
    for w in reading["waits"]:
        flag = "OVER" if now - w["since"] >= wait_minutes() * 60 else "    "
        print("%s %s" % (flag, describe_wait(w, now)))
    for w in reading["held"]:
        print("HELD %s" % describe_held(w, now))
    for resource, lines in reading["facts"].items():
        print("%s:" % LABELS.get(resource, resource))
        for line in lines:
            print("  - " + line)
    for p in reading["problems"]:
        print("not read: " + p)
    return 1 if reading["overdue"] and not args.all else 0


def cmd_wait(args):
    """Poll for a file while RECORDED as a wait. Exit 0 when it exists, 75 on timeout."""
    if args.every < 1:
        sys.stderr.write("resource_waits.py wait: --every must be at least 1 second\n")
        return 2
    path = os.path.expanduser(args.until_exists)
    started = time.time()
    resource = {"vm": VM, "cpu": CPU}.get(args.resource, args.resource)
    w = Wait(resource, reason="polling for %s" % path, waiter=args.waiter)
    w.record["until"] = path
    with w:
        while not os.path.exists(path):
            if args.timeout and time.time() - started >= args.timeout:
                sys.stderr.write("resource_waits.py wait: %s did not appear in %.0f s\n" % (path, args.timeout))
                return 75
            time.sleep(args.every)
    print("%s exists after %.0f s" % (path, time.time() - started))
    return 0


def cmd_wait_over(args):
    esc = _escalations()
    if esc is None:
        sys.stderr.write("resource_waits.py: the escalation module could not be loaded\n")
        return 2
    note = (args.note or "").strip()
    if len(note) < MIN_WAIT_OVER_NOTE:
        sys.stderr.write("wait-over refused: --note must say what ended the wait (at least %d characters)\n"
                         % MIN_WAIT_OVER_NOTE)
        return 2
    if KEEP_WAITING.search(note):
        sys.stderr.write("wait-over refused: the note says the wait goes on (\"%s\"). A wait that goes on is "
                         "not over, and saying so does not end it.\n" % KEEP_WAITING.search(note).group(0))
        return 2
    rows, _bad = esc.read_rows()
    if rows is None:
        sys.stderr.write("wait-over refused: the escalation ledger could not be read\n")
        return 2
    subject = [r for r in rows if r.get("event") == "Escalation" and str(r.get("id")) == args.id]
    if not subject:
        sys.stderr.write("wait-over refused: no escalation %s in %s\n" % (args.id, esc.ledger_path()))
        return 2
    row = {"event": "WaitOver", "id": args.id, "at": esc.iso(esc.utcnow()), "note": note,
           "cwd": os.getcwd(), "actor": esc._actor()}
    try:
        esc.append_row(row)
    except Exception as exc:
        sys.stderr.write("wait-over NOT RECORDED: %s\n" % exc)
        return 2
    print("recorded: the wait behind %s is over (%s)" % (args.id, note))
    return 0


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="verb", required=True)
    sub.add_parser("stop", help="the Stop analyzer: payload on stdin")
    lp = sub.add_parser("list", help="every current wait, with holders for the overdue ones")
    lp.add_argument("--all", action="store_true", help="holder facts for every wait, not only overdue ones")
    wp = sub.add_parser("wait", help="poll for a file while recorded as a wait")
    wp.add_argument("--resource", required=True, help="vm, cpu, or a free-form resource name")
    wp.add_argument("--until-exists", required=True, metavar="PATH")
    wp.add_argument("--timeout", type=float, default=0, help="seconds; 0 waits until the file exists")
    wp.add_argument("--every", type=float, default=30)
    wp.add_argument("--waiter", default=None, help="the name to show (default: the teammate from the cwd)")
    op = sub.add_parser("wait-over", help="record, from the waiter, that an escalated wait is over")
    op.add_argument("id")
    op.add_argument("--note", required=True)
    args = p.parse_args(argv)
    if args.verb == "stop":
        return cmd_stop()
    if args.verb == "list":
        return cmd_list(args)
    if args.verb == "wait":
        return cmd_wait(args)
    return cmd_wait_over(args)


if __name__ == "__main__":
    try:
        sys.exit(main())
    except SystemExit:
        raise
    except Exception as exc:  # pragma: no cover
        sys.stderr.write("resource_waits.py: %s: %s\n" % (type(exc).__name__, exc))
        sys.exit(3)

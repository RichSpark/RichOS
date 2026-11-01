#!/usr/bin/env python3
"""proof_slots.py — how many proof runs share this Mac at once, host-wide.

    proof_slots.py [status] [--json]   who holds a slot and who waits, in line order
    proof_slots.py set N               the host's limit: N proof runs at once (Rich sets it)
    proof_slots.py set default         back to the measured default (below)

    import: acquire(meta, wait_seconds, say) -> Slot (its .fds are passed to the run's checks);
            Slot.release(); status(); limit()

WHY (2026-09-27). proof-run.py admitted each CHECK by one CPU sample and a machine budget of
checks, and neither counted RUNS. Several agents each saw a free Mac within the same minute,
each started a full selection, and the Mac fell to a few percent idle; a proof overlapped
Codex's CPU test window and ended it early. The CEO: "how many more times will this fucking
CPU shit keep happening?" and, of builds, "A free Mac is used whole; nothing waits in a line."
and "Rich sets how much runs at once."

THE LIMIT, FROM MEASUREMENT. proof-run.py's own worker budget is 80% of the logical cores (the
CEO ruling §77 line, in cores): ONE run is built to fill the admission line by itself. Measured
from the proof-run logs of 2026-09-26/27 (/Volumes/E1TB/state/richos/proof-runs/*/*/summary.json,
host samples every 10 s, 10 logical cores): 36 runs with no other proof run beside them peaked
at a median 99% total CPU and spent 62% of their samples at or over 80%; 25 runs overlapping
another averaged 85% and spent 86% of their samples over the line; 13 overlapping two or more
averaged 93% and 92%. The samples are the whole Mac, so these are upper bounds for one run. So
the default is floor(line cores / cores one run peaks at) = floor(0.8 x cores / (1.0 x cores)),
at least 1: ONE run at a time. `set N` is how the lead changes it for the whole Mac; an agent's
own flag cannot, because a limit each caller picks for itself is no limit.

HOW IT HOLDS. A slot is a file locked with flock(2) (slot-NNN); a run that holds one also locks
its holder record (slot-NNN.holder), which says who holds it. The lock FDs are passed to the
supervisors of the run's checks, so a runner killed with SIGKILL keeps its slot only until the
supervisors have stopped its checks: the kernel drops every lock with its last descriptor.
Nothing is a pid file, nothing is reclaimed by guessing.

THE LINE. A run that finds no slot free waits in first-come order: it holds a lock on its own
`wait-<ns>-<pid>-<hex>` file (the name orders the line) and only the first live waiter tries
for a slot. While it waits it is RECORDED as a CPU-admission wait (engine resource_waits.py),
naming the run it waits for, so the lead's turn end is refused once it passes ten minutes.
`status` reads the holder and waiter files and probes their locks with a shared, non-blocking
lock that it drops at once; it never touches a slot file, so looking cannot delay admission.

A RUN WHOSE CHECKS STILL WAIT GOES FIRST (2026-10-29, work-6ec9b429; reshaped 2026-11-01). With
the limit set to 6, six runs started together; one of them ran 46 min at 95% CPU and 30 of its 37
checks were never admitted, so it proved almost nothing. Each holder publishes how many of its
checks wait for the Mac (below) in its `slot-NNN.waiting` file (written by the run, removed when
none wait or the slot is released, believed only while that holder's lock is held by the pid it
names), and a run behind it starts none of its own checks while that count is above zero
(`ahead_of`, below). Until 2026-11-01 that count ALSO kept a free slot from being taken: the next
run waited in line, unstarted, recorded as a CPU wait. That second hold protected nothing
`ahead_of` does not already protect, and it kept whole runs out while slots were free: on
2026-11-01 one run waited 10 min behind another's waiting checks while the stall watcher sampled
the Mac at 19% busy, and the nightly's own checks waited 10 min the same way. So a free slot is
taken at once; the run in it gives way check by check, and uses only what the runs ahead of it
cannot.

ONLY CHECKS THE MAC COULD TAKE NOW COUNT, AND A NEWER RUN GIVES WAY (2026-10-31). The count used
to be every check not yet started, and most of those wait for the run's own work, not for the
Mac: the engine units of one shard run one after another in its lane, and `engine receipts`
waits for every shard, so any run with an engine unit published a count above zero from its
first second to its last. With the limit set to 4, agents' final proof runs waited 40 to 87
minutes in line while the stall watcher sampled the Mac at 13-17% busy. So proof-run.py now
publishes only the checks it would start the moment the Mac (the CPU line, the memory rule, the
worker budget) lets it, never one waiting for its own lane or prerequisite, the engine slot, an
identical check or the integration plan. A run that starts beside a holder this way can no
longer starve it: before each admission a run looks at the holders that took their slot before
it (`ahead_of`), and while one of them still has checks waiting to be admitted, its own checks
give way, a wait that is not the Mac refusing and never spends --admission-wait. So the older
run is always admitted first, and the newer one uses only what the older one cannot. The one
exception keeps a single priority order: a land in the primary checkout holding integration
priority (engine_pass.Admission) never gives way, because that priority is what keeps an older
background run's checks waiting; giving way to them left both waiting until the 600 s turn
expired (review rv-20261031T223851Z-0d748dd7-f9f7). Once its turn is over it gives way like any.

ONLY CHECKS THE MAC REFUSED COUNT (2026-11-01, amending the paragraph above). The count still took
in checks waiting for their own run's worker budget or ramp, and checks of a run that was itself
giving way, and a run behind gave way on it without taking a sample: at 22:52-22:55Z three runs
gave way to echo-opus-build51 ("11 of its checks still wait to be admitted") for 11 minutes while
the Mac sampled 30-56% busy, and one of them, giving way, published its own 75 checks to the runs
behind it. CEO ruling §77 admits a test by the CPU rule alone, and a free Mac is used whole (the
header). So proof-run.py now publishes only the checks the Mac refused at its last admission
attempt (`refused_by_the_mac`: a host sample over the CPU line or its memory rule, the pressure
controller, the resource envelope). A check the CPU rule admits is admitted, whichever run it
belongs to; the order between runs decides only who goes first when there is not room for both:
while the Mac refuses a run's checks, the runs behind it give way, so the run ahead takes the
first sample under the line. Every run loads this file from its own checkout, so a run started
from an older one still publishes the old count; its `.waiting` file does not say `counts:
mac-refused` (`COUNTS`), and no run gives way to it: each check of either run is still admitted
by the CPU rule.

A NIGHTLY OR RELEASE COMMAND'S RUN GOES FIRST (2026-11-01). On 2026-11-01 08:47-08:57Z the
nightly's mutation batch (nightly-local.py `mutation_units`) stood #1 in line for 10 min behind a
teammate's run, and went on only after the lead asked the teammate to stop. The CEO's first point
on builds: he says go and walks away, and the right app is built. A run is the build's own when
the nightly names its state folder for that run (`--release-state`, passed by nightly-local.py
`mutation_units` on its own proof-run command line and on no other), its checkout is that
folder's `source` tree, and that folder's `release.lock` is held (nightly-local.py
`Runner.source` and `exclusive()`, which every nightly-local command holds for its whole run).
The checkout and the lock alone are not enough (run 20261101T104504Z-3ecb70df): the
nightly's script-suites gate runs proof-run.test.py and nightly-local.test.py from that same
`source` tree while the lock is held, so every fixture run those suites start counted as the
build's own, never waited, and nine queue tests failed. A command-line flag is never inherited:
a check, or a test, started from inside the build's tree carries none.
Such a run never waits in the line: it takes a free slot, or one past the limit when none is free.
In the order above it is ahead of every other run whenever it took its slot, so each teammate run
gives way check by check while the Mac refuses the build's run's checks; nothing of the teammate's
is stopped, its running checks finish and its waiting ones start once the Mac refuses none of the
build's. A teammate cannot claim it from its own worktree: the checkout must be the build's
source tree. The engine nightly (nightly-engine.py) is not one: it yields to the app nightly.
A run that gives way also withdraws its worker request (proof-run.py `schedule`): left live, a
refused request ages past engine_pass BACKGROUND_PRIORITY_AGE and worker admission then favors it
over the run ahead, which it waits for, so neither moved with every worker free (review
rv-20261101T094016Z-2c1ea15a-8a00).

NESTED RUNS. A suite that runs the real proof-run.py (proof-run.test.py, test-results.test.sh)
inside a check of a proof run works inside its caller's slot: the caller exports its holder
record's path as RICHOS_PROOF_RUN_SLOT_HELD, and the nested run borrows only if that record is
held right now AND its pid is one of the nested run's own ancestors. Naming it from anywhere
else does not skip the line.
"""
import datetime
import fcntl
import json
import math
import os
import signal
import subprocess
import sys
import time
import uuid

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "..", "..", "engine", "scripts", "lib"))
try:
    from cpu_policy import DEFAULT_MAX_CPU
except ImportError:  # pragma: no cover - an engine older than cpu_policy
    DEFAULT_MAX_CPU = 80
try:
    import resource_waits
except ImportError:  # pragma: no cover - an engine older than the recorder
    resource_waits = None

HELD_ENV = "RICHOS_PROOF_RUN_SLOT_HELD"
DIR_ENV = "RICHOS_PROOF_RUN_SLOTS_DIR"
# The share of the Mac one proof run reaches at peak, measured (header): the median peak of 36
# runs with no other proof run beside them was 99% of all cores.
MEASURED_PEAK_SHARE = 1.0
POLL_SECONDS = 1.0
# A waiting run gives up after three hours: the longest single run measured on 2026-09-27 took
# 3615 s alone, so this is the line behind two full runs. The lead hears of it at ten minutes.
DEFAULT_WAIT_SECONDS = 3 * 3600
STALE_WAITER_SECONDS = 5.0
SAY_EVERY_SECONDS = 120.0
# What a `.waiting` file counts (header, ONLY CHECKS THE MAC REFUSED COUNT). A file without it was
# written by a runner from an older checkout, whose count includes checks waiting for that run's
# own work, so it never holds a run back.
COUNTS = "mac-refused"


def directory():
    d = os.path.abspath(os.path.expanduser(os.environ.get(DIR_ENV) or
                                           os.path.join("~", ".richos-nightly", "proof-run-slots-v1")))
    os.makedirs(d, mode=0o700, exist_ok=True)
    return d


def cores():
    return os.cpu_count() or 1


def measured_default():
    line_cores = cores() * DEFAULT_MAX_CPU / 100.0
    return max(1, int(math.floor(line_cores / (cores() * MEASURED_PEAK_SHARE))))


def limit(d=None):
    """(n, source): the host's setting when one is set and valid, else the measured default."""
    d = d or directory()
    try:
        with open(os.path.join(d, "limit")) as fh:
            n = int(fh.read().strip())
        if 1 <= n <= cores():
            return n, "set on this Mac"
    except (OSError, ValueError):
        pass
    return measured_default(), "measured default"


def set_limit(value, d=None):
    d = d or directory()
    path = os.path.join(d, "limit")
    if value == "default":
        try:
            os.unlink(path)
        except FileNotFoundError:
            pass
        return measured_default()
    n = int(value)
    if not 1 <= n <= cores():
        raise ValueError("the limit is a whole number of runs from 1 to %d (this Mac's logical cores)" % cores())
    with open(path + ".new", "w") as fh:
        fh.write("%d\n" % n)
    os.replace(path + ".new", path)
    return n


def _utc(epoch):
    return datetime.datetime.fromtimestamp(epoch, datetime.timezone.utc).strftime("%H:%M:%SZ")


def _minutes(seconds):
    return "%.0f s" % seconds if seconds < 120 else "%.0f min" % (seconds / 60)


def _waiter_name():
    if os.environ.get("RICHOS_WAITER"):
        return os.environ["RICHOS_WAITER"]
    if resource_waits is not None:
        return resource_waits._waiter_from_cwd(os.getcwd())
    return os.getcwd()


def _locked(path):
    """True when another process holds a lock on `path` right now; None when it is missing.
    The probe is shared and non-blocking and is dropped at once."""
    try:
        fd = os.open(path, os.O_RDONLY)
    except OSError:
        return None
    try:
        fcntl.flock(fd, fcntl.LOCK_SH | fcntl.LOCK_NB)
    except BlockingIOError:
        return True
    else:
        fcntl.flock(fd, fcntl.LOCK_UN)
        return False
    finally:
        os.close(fd)


def _read(path):
    try:
        with open(path) as fh:
            return json.load(fh)
    except (OSError, ValueError):
        return {}


def release_owned(checkout, state):
    """True when `state` is the state folder a nightly or release command named for this run and
    `checkout` is that folder's source tree while the command runs (header): `<state>/source`, and
    `<state>/release.lock` held right now. Without `state` never: a run started from inside the
    build's tree (a test's fixture run) is not the build's own."""
    if not checkout or not state:
        return False
    state = os.path.realpath(state)
    return (os.path.realpath(checkout) == os.path.join(state, "source")
            and _locked(os.path.join(state, "release.lock")) is True)


def _rank(record):
    """The order runs are admitted in (header): a build's run before any other, then by slot taken."""
    return (0 if record.get("first") else 1, record.get("since") or 0,
            record.get("slot") if isinstance(record.get("slot"), int) else -1)


class Slot(object):
    def __init__(self, index, fds, holder_path, borrowed=False):
        self.index, self._fds, self.holder_path, self.borrowed = index, list(fds), holder_path, borrowed

    @property
    def fds(self):
        return tuple(self._fds)

    def env(self):
        """What a check of this run inherits, so a proof run inside it can borrow this slot."""
        return {HELD_ENV: self.holder_path} if self.holder_path else {}

    def publish_waiting(self, count):
        """How many of this run's checks wait because the Mac refused them (header); 0 removes the file.
        Written only when the count changes; a borrowed slot is its caller's to describe."""
        if self.borrowed or not self._fds or count == getattr(self, "_published", None):
            return
        _publish(self.holder_path, count)
        self._published = count

    def release(self):
        if not self.borrowed and self._fds:
            _publish(self.holder_path, 0)
        for fd in reversed(self._fds):
            try:
                fcntl.flock(fd, fcntl.LOCK_UN)
            except OSError:
                pass
            try:
                os.close(fd)
            except OSError:
                pass
        self._fds = []


def _try(d, n, meta, checks=0):
    for i in range(n):
        path = os.path.join(d, "slot-%03d" % i)
        fd = os.open(path, os.O_RDWR | os.O_CREAT, 0o600)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            os.close(fd)
            continue
        holder = path + ".holder"
        hfd = os.open(holder, os.O_RDWR | os.O_CREAT, 0o600)
        # Blocking: only a status probe can hold it, and only for an instant.
        fcntl.flock(hfd, fcntl.LOCK_EX)
        os.ftruncate(hfd, 0)
        record = dict(meta, pid=os.getpid(), since=time.time(), slot=i)
        os.write(hfd, (json.dumps(record) + "\n").encode())
        slot = Slot(i, [fd, hfd], holder)
        slot.since, slot.first = record["since"], bool(record.get("first"))
        # What the caller says waits for the Mac from the start (proof-run.py says none: nothing of
        # it has been refused yet), said before the next run looks.
        _publish(holder, checks)
        slot._published = checks
        return slot
    return None


def _publish(holder, count):
    path = holder[:-len(".holder")] + ".waiting"
    if count <= 0:
        try:
            os.unlink(path)
        except FileNotFoundError:
            pass
        return
    with open(path + ".new", "w") as fh:
        json.dump({"pid": os.getpid(), "waiting": count, "since": time.time(), "counts": COUNTS}, fh)
    os.replace(path + ".new", path)


def waiting_holders(d=None):
    """Holders with checks the Mac refused (header): their record plus `waiting`, the count.
    A `.waiting` file counts only while its slot's holder lock is held by the pid it names, and
    only when it says it counts what the Mac refused (`COUNTS`)."""
    rows = []
    for rec in holders(d):
        sidecar = _read(rec["holder_file"][:-len(".holder")] + ".waiting")
        if (sidecar.get("pid") == rec.get("pid") and sidecar.get("counts") == COUNTS
                and isinstance(sidecar.get("waiting"), int) and sidecar["waiting"] > 0):
            rows.append(dict(rec, waiting=sidecar["waiting"]))
    return rows


def ahead_of(slot):
    """The holders ahead of `slot` in the admission order (`_rank`: a nightly or release command's
    run first, then the order slots were taken) whose checks the Mac refused at their last attempt
    (header): there is not room for both, so while there is one, `slot`'s run starts none of its
    own. Empty for a borrowed slot (a proof run inside a check of its caller is part of that
    caller's run) and for no slot at all."""
    if slot is None or slot.borrowed or not slot.fds or not slot.holder_path:
        return []
    record = _read(slot.holder_path)
    mine = _rank({"first": getattr(slot, "first", record.get("first")),
                  "since": getattr(slot, "since", None) or record.get("since") or time.time(), "slot": slot.index})
    own = os.path.realpath(slot.holder_path)
    return [r for r in waiting_holders(os.path.dirname(slot.holder_path))
            if os.path.realpath(r["holder_file"]) != own and _rank(r) < mine]


def describe_behind(rows):
    return "; ".join("%s (pid %s%s): the Mac refused %d of its checks, so it goes first" % (
        r.get("waiter", "?"), r.get("pid", "?"), ", a nightly or release command's run" if r.get("first") else "",
        r["waiting"]) for r in rows)


def _ancestors(pid):
    out = subprocess.run(["ps", "-A", "-o", "pid=,ppid="], capture_output=True, text=True).stdout
    parent = {}
    for line in out.split("\n"):
        parts = line.split()
        if len(parts) == 2 and all(p.isdigit() for p in parts):
            parent[int(parts[0])] = int(parts[1])
    seen = set()
    while pid in parent and pid not in seen and pid > 1:
        seen.add(pid)
        pid = parent[pid]
        yield pid


def _borrowed(d):
    """The caller's slot, when this run was started by a check of the run that holds it."""
    held = os.environ.get(HELD_ENV, "")
    if not held or os.path.realpath(os.path.dirname(held)) != os.path.realpath(d):
        return None
    if _locked(held) is not True:
        return None
    pid = _read(held).get("pid")
    if not isinstance(pid, int) or pid not in set(_ancestors(os.getpid())):
        return None
    return Slot(_read(held).get("slot"), [], held, borrowed=True)


def _waiters(d):
    """Live waiters in line order; stale files (no lock, older than a few seconds) are removed."""
    live = []
    for name in sorted(os.listdir(d)):
        if not (name.startswith("wait-") and name.endswith(".json")):
            continue
        path = os.path.join(d, name)
        state = _locked(path)
        if state:
            live.append(path)
        elif state is False:
            try:
                if time.time() - os.path.getmtime(path) > STALE_WAITER_SECONDS:
                    os.unlink(path)
            except OSError:
                pass
    return live


def holders(d=None):
    d = d or directory()
    rows = []
    for name in sorted(os.listdir(d)):
        if name.startswith("slot-") and name.endswith(".holder"):
            path = os.path.join(d, name)
            if _locked(path):
                rec = _read(path)
                rec["holder_file"] = path
                rows.append(rec)
    return rows


def status(d=None):
    d = d or directory()
    n, source = limit(d)
    waiting = []
    for k, path in enumerate(_waiters(d), 1):
        rec = _read(path)
        rec["position"] = k
        waiting.append(rec)
    behind = {rec["holder_file"]: rec["waiting"] for rec in waiting_holders(d)}
    held = [dict(rec, checks_waiting=behind.get(rec["holder_file"], 0)) for rec in holders(d)]
    return {"dir": d, "limit": n, "limit_source": source, "measured_default": measured_default(),
            "holders": held, "waiting": waiting}


def describe_holders(rows, now=None):
    now = now or time.time()
    if not rows:
        return "no run holds a slot right now"
    return "; ".join("%s (pid %s) for %s since %s" % (r.get("waiter", "?"), r.get("pid", "?"),
                                                       _minutes(now - r.get("since", now)), _utc(r.get("since", now)))
                     for r in rows)


def acquire(meta, wait_seconds=DEFAULT_WAIT_SECONDS, say=None, checks=0):
    """A slot for this run: at once when one is free and nobody waits, else in line; a nightly or
    release command's run (`release_owned(meta["checkout"], meta["release_state"])`) never
    waits, taking a slot past the limit when none is free (header). `checks`: how many this run
    plans, published as waiting until it says otherwise. Returns (slot, seconds waited). Raises
    TimeoutError, naming the holders, after `wait_seconds`."""
    say = say or (lambda line: None)
    d = directory()
    meta = dict(meta, waiter=_waiter_name(), cwd=os.getcwd())
    borrowed = _borrowed(d)
    if borrowed is not None:
        return borrowed, 0.0
    n, _source = limit(d)
    if release_owned(meta.get("checkout"), meta.get("release_state")):
        meta["first"] = True
        slot = _try(d, n + cores(), meta, checks)
        if slot is not None:
            return slot, 0.0
    if not _waiters(d):
        slot = _try(d, n, meta, checks)
        if slot is not None:
            return slot, 0.0
    started = time.time()
    name = "wait-%019d-%d-%s.json" % (time.time_ns(), os.getpid(), uuid.uuid4().hex[:8])
    mine = os.path.join(d, name)
    wfd = os.open(mine, os.O_RDWR | os.O_CREAT | os.O_EXCL, 0o600)
    fcntl.flock(wfd, fcntl.LOCK_EX)
    os.write(wfd, (json.dumps(dict(meta, pid=os.getpid(), since=started)) + "\n").encode())
    record = None
    said, said_at = None, 0.0
    previous = {}

    def interrupted(signum, _frame):
        raise SystemExit(128 + signum)

    for sig in (signal.SIGTERM, signal.SIGHUP):
        previous[sig] = signal.signal(sig, interrupted)
    try:
        while True:
            n, _source = limit(d)
            line = _waiters(d)
            if meta.get("first") or (line and line[0] == mine):
                slot = _try(d, n + cores() if meta.get("first") else n, meta, checks)
                if slot is not None:
                    return slot, time.time() - started
            position = line.index(mine) + 1 if mine in line else 1
            held = holders(d)
            why = ("proof-run: waiting for a proof-run slot (this Mac runs %d at once): held by %s; #%d in line"
                   % (n, describe_holders(held), position))
            # What the line is SAID again for: the limit, who holds, the place in line. Not
            # elapsed times or counts, which would make every poll a new line.
            changed = (n, tuple((h.get("pid"), h.get("since")) for h in held), position)
            if record is None and resource_waits is not None:
                try:
                    record = resource_waits.waiting(resource_waits.CPU, why).start()
                except Exception as exc:  # the record never breaks the wait
                    say("proof-run: slot wait not recorded: %s" % exc)
            elif record is not None:
                record.update(why)
            now = time.time()
            if changed != said or now - said_at >= SAY_EVERY_SECONDS:
                say("%s. Who holds and who waits: python3 %s status" % (why, os.path.abspath(__file__)))
                said, said_at = changed, now
            if now - started >= wait_seconds:
                raise TimeoutError("waited %.0f s for a proof-run slot (this Mac runs %d at once); held by %s"
                                   % (now - started, n, describe_holders(holders(d))))
            time.sleep(POLL_SECONDS)
    finally:
        for sig, handler in previous.items():
            signal.signal(sig, handler)
        if record is not None:
            record.close()
        try:
            os.unlink(mine)
        except OSError:
            pass
        fcntl.flock(wfd, fcntl.LOCK_UN)
        os.close(wfd)


def print_status(s):
    now = time.time()
    print("proof-run slots on this Mac: %d at once (%s; the measured default is %d) in %s"
          % (s["limit"], s["limit_source"], s["measured_default"], s["dir"]))
    if not s["holders"]:
        print("  holding: nobody")
    for h in s["holders"]:
        print("  holding: slot %s  %s (pid %s) for %s since %s" % (
            h.get("slot", "?"), h.get("waiter", "?"), h.get("pid", "?"), _minutes(now - h.get("since", now)),
            _utc(h.get("since", now))))
        if h.get("first"):
            print("           a nightly or release command's run: it goes first, past the limit if it must")
        for key in ("checkout", "logs", "selection"):
            if h.get(key):
                print("           %s: %s" % (key, h[key]))
        if h.get("checks_waiting"):
            print("           checks the Mac refused: %d (runs behind it start no check until the Mac "
                  "takes them)" % h["checks_waiting"])
    if not s["waiting"]:
        print("  waiting: nobody")
    for w in s["waiting"]:
        print("  waiting: #%d  %s (pid %s) for %s since %s%s" % (
            w["position"], w.get("waiter", "?"), w.get("pid", "?"), _minutes(now - w.get("since", now)),
            _utc(w.get("since", now)), ("  checkout " + w["checkout"]) if w.get("checkout") else ""))


def main(argv):
    if argv[:1] == ["set"] and len(argv) == 2:
        try:
            n = set_limit(argv[1])
        except ValueError as exc:
            print("proof_slots.py: %s" % (exc if "whole number" in str(exc) else
                  "the limit is a whole number of runs from 1 to %d, or `default`" % cores()), file=sys.stderr)
            return 2
        print("proof-run slots on this Mac: %d at once from the next admission" % n)
        return 0
    rest = argv[1:] if argv[:1] == ["status"] else argv
    if rest in ([], ["--json"]):
        s = status()
        if rest:
            print(json.dumps(s, indent=1))
        else:
            print_status(s)
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

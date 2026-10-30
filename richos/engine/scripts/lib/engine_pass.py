#!/usr/bin/env python3
"""Bounded large-plan serialization with main-checkout admission priority.

The threshold is 20 units, not the full inventory. The inherited kernel-held
slot serializes large plans without changing their coverage obligations.
This helper alone is neither a CPU quota nor universal execution enforcement.
"""
import argparse
import fcntl
import json
import io
import os
import signal
import subprocess
import sys
import time
import uuid
import hashlib
import machine_settings as M

FULL_PASS_UNITS = 20
# No default bound on the wait for the slot (CEO, 2026-10-30): a queued run waits its turn for as
# long as the queue really takes. On 2026-10-30 the old 1800 s teammate bound failed P1-B's 76
# engine units as "not-admitted" while they were only queued. The slot is a kernel-held flock that
# frees when its holder dies, so a wait cannot outlive a dead holder. An explicit wait (--wait,
# proof-run.py --engine-slot-wait) is still honored as a bound.
SLOT_WAIT = None
POLL_SECONDS = float(os.environ.get("RICHOS_ENGINE_PASS_POLL") or 2.0)   # a suite seam; never tuned
REPORT_EVERY = 60
REFUSED = 75
NEEDED = 10
BACKGROUND_PRIORITY_AGE = 120
INTEGRATION_PLAN_SECONDS = 600


class IntegrationPlan:
    """One immutable integration episode, with a finite turn between plans.

    Only the owner of the kernel lease may advertise per-check priority. A new
    request cannot join or extend the owner's plan. At its boundary, already-aged
    background waiters receive one compatible admission before another plan can
    claim priority. The set is frozen, so later arrivals cannot extend that turn.
    """
    def __init__(self, root, plan, evidence, limit=INTEGRATION_PLAN_SECONDS):
        self.root = os.path.join(root, "admission")
        os.makedirs(self.root, mode=0o700, exist_ok=True)
        self.path = os.path.join(self.root, "integration-plan.json")
        frozen = json.dumps(plan, sort_keys=True)
        self.plan = json.loads(frozen)
        self.identity = hashlib.sha256(frozen.encode()).hexdigest()
        self.evidence = evidence
        self.requested = time.monotonic()
        self.deadline = self.requested + limit
        self.fd = None
        self.record = None

    def expired(self):
        return time.monotonic() >= self.deadline

    def _waiters(self):
        result = {}
        for name in os.listdir(self.root):
            if not name.startswith("wait-") or not name.endswith(".json"):
                continue
            try:
                with open(os.path.join(self.root, name), "r+") as source:
                    try:
                        fcntl.flock(source, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    except BlockingIOError:
                        record = json.load(source)
                    else:
                        continue
                if not record["main"]:
                    result[name] = record
            except FileNotFoundError:
                continue
        return result

    def _publish(self, record):
        temporary = self.path + ".%s.new" % os.getpid()
        with open(temporary, "w") as stream:
            json.dump(record, stream, sort_keys=True)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, self.path)

    def enter(self):
        if self.expired():
            return False
        if self.fd is not None:
            return True
        fd = os.open(os.path.join(self.root, "integration-plan.lock"), os.O_RDWR | os.O_CREAT, 0o600)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            os.close(fd)
            return False
        try:
            try:
                with open(self.path) as stream:
                    previous = json.load(stream)
            except FileNotFoundError:
                previous = None
            waiting = self._waiters()
            if previous:
                if previous.get("schema") != 1 or not isinstance(previous.get("yield_to"), list):
                    raise ValueError("invalid integration episode record: " + self.path)
                if previous["status"] == "active":
                    # The kernel lease is free: the prior owner died. Preserve its
                    # episode and give its aged background waiters their turn.
                    previous.update(status="owner-lost", yield_to=[name for name, row in waiting.items()
                        if time.monotonic() - row["since"] >= BACKGROUND_PRIORITY_AGE])
                    self._publish(previous)
                if any(name in waiting for name in previous["yield_to"]):
                    return False
            self.record = {"schema": 1, "pid": os.getpid(), "birth": birth(os.getpid()),
                "identity": self.identity, "plan": self.plan, "evidence": self.evidence,
                "requested": self.requested, "deadline": self.deadline,
                "status": "active", "yield_to": []}
            self._publish(self.record)
            self.fd, fd = fd, None
            return True
        finally:
            if fd is not None:
                os.close(fd)

    def close(self, status="finished"):
        if self.fd is None:
            return
        try:
            waiting = self._waiters()
            self.record.update(status=status, ended=time.monotonic(),
                yield_to=[name for name, row in waiting.items()
                          if time.monotonic() - row["since"] >= BACKGROUND_PRIORITY_AGE])
            self._publish(self.record)
        finally:
            os.close(self.fd)
            self.fd = None


def directory():
    """The machine's one slot. RICHOS_ENGINE_PASS_DIR is for suites, never for a real run."""
    path = str(M.state("engine-pass-v1", "RICHOS_ENGINE_PASS_DIR"))
    if os.path.isfile(os.path.join(os.path.dirname(__file__), 'cpu_guard.py')):
        import cpu_guard
        path = str(cpu_guard.governed_directory(path, cpu_guard.CANONICAL_ENGINE_PASS))
    os.makedirs(path, mode=0o700, exist_ok=True)
    return path


def needs_slot(count):
    return int(count) >= FULL_PASS_UNITS


def _ps_rows():
    from proc_tree import process_rows
    return {pid: (parent, generation) for pid, (parent, _group, generation) in process_rows().items()
            if generation is not None}


def birth(pid, rows=None):
    rows = rows if rows is not None else _ps_rows()
    row = rows.get(int(pid))
    return row[1] if row else None


def _locked(path):
    """True when another open file description holds `path` exclusively. Takes nothing."""
    try:
        fd = os.open(path, os.O_RDWR | os.O_CREAT, 0o600)
    except OSError:
        return False
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        return True
    else:
        fcntl.flock(fd, fcntl.LOCK_UN)
        return False
    finally:
        os.close(fd)


def holder():
    """The recorded holder, only while the slot is actually held."""
    d = directory()
    if not _locked(os.path.join(d, "slot.lock")):
        return None
    try:
        with open(os.path.join(d, "holder.json")) as fh:
            return json.load(fh)
    except (OSError, ValueError):
        return {"label": "unknown (the slot is held and no holder record was readable)"}


def held_by_ancestor(pid=None):
    """True when the slot is held by this process or one of its ancestors (same PID AND the same
    start time, so a reused PID never counts)."""
    rec = holder()
    if not rec or "pid" not in rec:
        return False
    rows = _ps_rows()
    if birth(rec["pid"], rows) != rec.get("birth"):
        return False
    pid = os.getpid() if pid is None else int(pid)
    seen = set()
    while pid and pid not in seen:
        if pid == rec["pid"]:
            return True
        seen.add(pid)
        row = rows.get(pid)
        if not row:
            return False
        pid = row[0]
    return False


def is_main_checkout(path):
    try:
        out = subprocess.run(["git", "-C", path, "rev-parse", "--path-format=absolute", "--git-dir",
                              "--git-common-dir"], capture_output=True, text=True, timeout=10)
    except (OSError, subprocess.TimeoutExpired):
        return False
    lines = out.stdout.split()
    return out.returncode == 0 and len(lines) == 2 and os.path.realpath(lines[0]) == os.path.realpath(lines[1])


def _branch(path):
    try:
        out = subprocess.run(["git", "-C", path, "rev-parse", "--abbrev-ref", "HEAD"],
                             capture_output=True, text=True, timeout=10)
        return out.stdout.strip() or "?"
    except (OSError, subprocess.TimeoutExpired):
        return "?"


def describe(rec):
    if not rec:
        return "nobody"
    if "pid" not in rec:
        return rec.get("label", "unknown")
    age = time.time() - rec.get("started", time.time())
    return "%s (pid %s, %s, %s, %s units, for %.0f s)" % (
        rec.get("label"), rec.get("pid"), "LAND RUN, main checkout" if rec.get("main") else
        "checkout %s" % rec.get("checkout"), "branch %s" % rec.get("branch"), rec.get("count"), age)


class Refused(Exception):
    def __init__(self, waited, rec, count, units, main):
        self.waited, self.rec, self.count, self.units, self.main = waited, rec, count, list(units), main
        super().__init__(self.message())

    def message(self):
        lines = ["engine-pass: REFUSED after %.0f s. The machine's one full-engine-pass slot is held by %s."
                 % (self.waited, describe(self.rec)),
                 "  This run selected %d engine unit(s); %d or more is a full pass, and only one runs on "
                 "this Mac at a time. NOTHING WAS RUN." % (self.count, FULL_PASS_UNITS)]
        if self.main:
            lines.append("  Run it again when that pass ends (engine_pass.py status shows it).")
        else:
            lines.append("  From a teammate's worktree the full pass is the lead's, at land. Verify what you wrote "
                         "with `bash scripts/ci-shard.sh --only-units <unit>` on the units covering it.")
        if self.units:
            lines.append("  The units this run selected:")
            lines.extend("    " + u for u in self.units)
        return "\n".join(lines)


class Slot:
    def __init__(self, fd, waited):
        self.fd, self.waited = fd, waited

    def release(self):
        # Inherited descriptors share the lock. Only the last close may release
        # it; LOCK_UN would also release a still-running descendant's protection.
        if self.fd is not None:
            os.close(self.fd)
            self.fd = None


class PlanGate:
    """Nonblocking engine-only slot admission with draining at unit boundaries."""
    def __init__(self, count, label, checkout, units=(), wait=None):
        self.count, self.label, self.checkout, self.units = count, label, checkout, tuple(units)
        self.main = is_main_checkout(checkout)
        self.wait = SLOT_WAIT if wait is None else wait
        self.root = directory()
        self.slot = None
        self.priority_fd = None
        self.started_waiting = None
        self.waited = 0.0
        self.yields = 0
        self.marker = os.path.join(self.root, "wait-%s-%s.json" % (os.getpid(), uuid.uuid4().hex))

    def ready(self, active):
        if self.slot:
            if self.main or not _locked(os.path.join(self.root, "priority.lock")):
                return True
            # Never interrupt an admitted unit to transfer the slot. No new
            # engine units start while we drain. Non-engine work is unaffected.
            if active:
                return False
            self.slot.release()
            self.slot = None
            self.yields += 1
        if self.started_waiting is None:
            self.started_waiting = time.monotonic()
            with open(self.marker, "w") as out:
                json.dump({"pid": os.getpid(), "birth": birth(os.getpid()), "main": self.main,
                           "label": self.label, "checkout": self.checkout, "count": self.count,
                           "since": time.time()}, out)
        if self.main and self.priority_fd is None:
            fd = os.open(os.path.join(self.root, "priority.lock"), os.O_RDWR | os.O_CREAT, 0o600)
            try:
                fcntl.flock(fd, fcntl.LOCK_SH | fcntl.LOCK_NB)
                self.priority_fd = fd
            except BlockingIOError:
                os.close(fd)  # an existing integration waiter already protects priority
        elapsed = time.monotonic() - self.started_waiting
        try:
            self.slot = acquire(self.count, self.label, self.checkout, self.units, wait=0,
                                out=io.StringIO())
        except Refused as exc:
            if self.wait is not None and self.waited + elapsed >= self.wait:
                self.close()
                raise Refused(self.waited, exc.rec, self.count, self.units, self.main)
            return False
        self.waited += elapsed
        self.started_waiting = None
        self._clear_priority()
        return True

    def _clear_priority(self):
        if self.priority_fd is not None:
            os.close(self.priority_fd)
            self.priority_fd = None
        try:
            os.unlink(self.marker)
        except FileNotFoundError:
            pass

    def close(self):
        if self.slot:
            self.slot.release()
            self.slot = None
        if self.started_waiting is not None:
            self.waited += time.monotonic() - self.started_waiting
            self.started_waiting = None
        self._clear_priority()


class Admission:
    """Main-checkout waiters hold shared priority; background admission needs exclusive.

    The guard spans the actual permit attempt, closing the check-then-acquire race.
    No queue age can override an integration waiter. Borrowing a caller's already
    held permit remains possible, so nested work can release occupied capacity.
    """
    def __init__(self, root, checkout, permits=None):
        self.root = os.path.join(root, "admission")
        os.makedirs(self.root, mode=0o700, exist_ok=True)
        self.main = is_main_checkout(checkout)
        self.fd = None
        self.wait_fd = None
        self.since = None
        self.permits = permits
        self.marker = os.path.join(self.root, "wait-%s-%s.json" % (os.getpid(), uuid.uuid4().hex))

    def register(self):
        if self.wait_fd is not None:
            return
        self.since = time.monotonic()
        # A held descriptor establishes liveness without PID-reuse guesses. Publish
        # only after the record is complete; a crash releases its kernel lease.
        temporary = self.marker + ".new"
        self.wait_fd = os.open(temporary, os.O_RDWR | os.O_CREAT | os.O_EXCL, 0o600)
        fcntl.flock(self.wait_fd, fcntl.LOCK_EX)
        record = {"pid": os.getpid(), "birth": birth(os.getpid()), "main": self.main,
                  "since": self.since, "queued_at": time.time(), "permits": self.permits}
        os.write(self.wait_fd, json.dumps(record).encode())
        os.replace(temporary, self.marker)

    def older_background_waiting(self):
        now = time.monotonic()
        for name in os.listdir(self.root):
            path = os.path.join(self.root, name)
            if not name.startswith("wait-") or not name.endswith(".json") or path == self.marker:
                continue
            try:
                with open(path, "r+") as source:
                    try:
                        fcntl.flock(source.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
                    except BlockingIOError:
                        record = json.load(source)
                    else:
                        # Dead requests cannot reserve a later opportunity.
                        os.unlink(path)
                        continue
            except FileNotFoundError:
                continue
            compatible = (self.permits is None or record.get("permits") is None or
                          bool(set(self.permits).intersection(record["permits"])))
            if (not record["main"] and compatible and
                    now - record["since"] >= BACKGROUND_PRIORITY_AGE and
                    (record["since"], path) < (self.since, self.marker)):
                return True
        return False

    def begin(self):
        self.register()
        if self.fd is not None:
            return True
        fd = os.open(os.path.join(self.root, "priority.lock"), os.O_RDWR | os.O_CREAT, 0o600)
        try:
            fcntl.flock(fd, (fcntl.LOCK_SH if self.main else fcntl.LOCK_EX) | fcntl.LOCK_NB)
        except BlockingIOError:
            os.close(fd)
            return False
        self.fd = fd
        if not self.main and self.older_background_waiting():
            os.close(self.fd)
            self.fd = None
            return False
        return True

    def attempted(self, acquired):
        # Keep integration intent while its permit is occupied. Background work
        # holds priority only during its attempt, never while queued.
        if acquired:
            self.close()
        elif not self.main:
            os.close(self.fd)
            self.fd = None

    def close(self):
        if self.fd is not None:
            os.close(self.fd)
            self.fd = None
        try:
            os.unlink(self.marker)
        except FileNotFoundError:
            pass
        if self.wait_fd is not None:
            os.close(self.wait_fd)
            self.wait_fd = None
        self.since = None


def acquire(count, label, checkout, units=(), wait=None, out=None):
    """Wait for the slot, reported, bounded only by an explicit `wait`; return a Slot or raise Refused."""
    out = out or sys.stderr
    d = directory()
    main = is_main_checkout(checkout)
    wait = SLOT_WAIT if wait is None else float(wait)
    slot = os.open(os.path.join(d, "slot.lock"), os.O_RDWR | os.O_CREAT, 0o600)
    priority = os.path.join(d, "priority.lock")
    prio_fd = None
    if main:
        prio_fd = os.open(priority, os.O_RDWR | os.O_CREAT, 0o600)
        try:
            fcntl.flock(prio_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:      # another land run already waits first; take turns after it
            os.close(prio_fd)
            prio_fd = None
    marker = os.path.join(d, "wait-%d.json" % os.getpid())
    start = time.monotonic()
    next_report = start
    try:
        while True:
            if main or not _locked(priority):
                try:
                    fcntl.flock(slot, fcntl.LOCK_EX | fcntl.LOCK_NB)
                    break
                except BlockingIOError:
                    pass
            now = time.monotonic()
            if wait is not None and now - start >= wait:
                os.close(slot)
                raise Refused(now - start, holder(), count, units, main)
            if now >= next_report:
                if next_report == start:
                    with open(marker, "w") as fh:
                        json.dump({"pid": os.getpid(), "label": label, "checkout": checkout, "main": main,
                                   "count": count, "since": time.time()}, fh)
                print("engine-pass: waiting %.0f s %s for the one-full-engine-pass slot "
                      "(%d units); held by %s%s" % (now - start, "(no limit: it runs when the slot is free)"
                                                    if wait is None else "of at most %.0f s" % wait,
                                                    count, describe(holder()),
                                                    "" if main or not _locked(priority)
                                                    else "; a land run is waiting and goes first"),
                      file=out, flush=True)
                next_report += REPORT_EVERY
            time.sleep(POLL_SECONDS)
    finally:
        if prio_fd is not None:
            os.close(prio_fd)
        try:
            os.remove(marker)
        except OSError:
            pass
    rec = {"pid": os.getpid(), "birth": birth(os.getpid()), "label": label, "checkout": checkout,
           "branch": _branch(checkout), "main": main, "count": count, "started": time.time()}
    tmp = os.path.join(d, "holder.json.%d" % os.getpid())
    with open(tmp, "w") as fh:
        json.dump(rec, fh)
    os.replace(tmp, os.path.join(d, "holder.json"))
    waited = time.monotonic() - start
    print("engine-pass: this run holds the machine's one full-engine-pass slot (%d units%s)"
          % (count, ", after waiting %.0f s" % waited if waited >= 1 else ""), file=out, flush=True)
    return Slot(slot, waited)


def status():
    d = directory()
    waiting = []
    for name in sorted(os.listdir(d)):
        if name.startswith("wait-") and name.endswith(".json"):
            try:
                with open(os.path.join(d, name)) as fh:
                    rec = json.load(fh)
                os.kill(int(rec["pid"]), 0)
                waiting.append(rec)
            except (OSError, ValueError, KeyError):
                continue
    return {"threshold": FULL_PASS_UNITS, "holder": holder(), "waiting": waiting,
            "land_run_waiting": _locked(os.path.join(d, "priority.lock"))}


def hold(argv):
    p = argparse.ArgumentParser(prog="engine_pass.py hold")
    p.add_argument("--count", type=int, required=True)
    p.add_argument("--units", default="")
    p.add_argument("--label", required=True)
    p.add_argument("--checkout", required=True)
    p.add_argument("--wait", type=float)
    if "--" not in argv:
        p.error("hold needs -- CMD...")
    split = argv.index("--")
    a = p.parse_args(argv[:split])
    cmd = argv[split + 1:]
    if not cmd:
        p.error("hold needs a command after --")
    units = [u for u in a.units.split(",") if u]
    try:
        slot = acquire(a.count, a.label, a.checkout, units, a.wait)
    except Refused as r:
        print(r.message(), file=sys.stderr, flush=True)
        return REFUSED
    try:
        # The command inherits the lock's descriptor, so the slot stays held for as long as the
        # pass runs even if this wrapper is killed first; it frees when the last holder exits.
        inherited_env = {**os.environ, "RICHOS_ENGINE_PASS_FD": str(slot.fd)}
        child = subprocess.Popen(cmd, pass_fds=(slot.fd,), env=inherited_env)
        def forward(signum, _frame):
            child.send_signal(signum)
        previous = {s: signal.signal(s, forward) for s in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP)}
        try:
            rc = child.wait()
        finally:
            for s, h in previous.items():
                signal.signal(s, h)
        return rc if rc >= 0 else 128 - rc
    finally:
        slot.release()


def main(argv):
    if argv == ["threshold"]:
        print(FULL_PASS_UNITS)
        return 0
    if len(argv) == 2 and argv[0] == "needed" and argv[1].isdigit():
        return NEEDED if needs_slot(int(argv[1])) and not held_by_ancestor() else 0
    if argv == ["held"]:
        return 0 if held_by_ancestor() else 1
    if argv == ["status"]:
        print(json.dumps(status(), indent=1))
        return 0
    if argv[:1] == ["hold"]:
        return hold(argv[1:])
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

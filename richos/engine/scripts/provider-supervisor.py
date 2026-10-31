#!/usr/bin/env python3
"""Own a native provider group even if the desktop disappears without running Drop.

The desktop creates this process as its group leader. The provider inherits its
stdio unchanged. Parent death is observed from the OS, never from a timeout on
model activity. No persistent job or automatic restart is created.
"""
import os
import signal
import sys
import time


def main():
    if len(sys.argv) < 2 or os.getpgrp() != os.getpid():
        raise SystemExit("provider supervisor requires an executable and its own process group")
    parent = os.getppid()
    if parent <= 1:
        raise SystemExit("provider supervisor has no desktop owner")
    provider = os.fork()
    if provider == 0:
        os.environ["RICHOS_SESSION_PID"] = str(os.getpid())
        try:
            os.execvp(sys.argv[1], sys.argv[1:])
        except OSError as error:
            print(f"Provider could not start: {error}", file=sys.stderr, flush=True)
            os._exit(127)
    try:
        while os.getppid() == parent:
            ended, _ = os.waitpid(provider, os.WNOHANG)
            if ended:
                break
            time.sleep(0.1)
    finally:
        # This live group leader reserves the group identity. Include ourselves:
        # no detached watcher can later target a recycled PID or process group.
        os.killpg(os.getpid(), signal.SIGKILL)


# ---------------------------------------------------------------------------
# REAPING MODE: `--reap-descendants` (spec r3 (q) item 2, F5; Frank G8, G9; and
# the product reap, richos-hq docs/plans/2026-09-27-product-reap-gap-design.md)
# ---------------------------------------------------------------------------
# main() above is the COMPATIBILITY path: byte-identical and at the same lines
# as before this section existed (other files cite its line numbers), and it is
# what runs when no option is given. Both callers that ship now run the section
# below: the operator's leads (`operator_profile.rs`) and, since the product reap
# gap design, every product lease (`native.rs`, design 1.3(a)).
#
# OPTIONS. Every leading argument that starts with `--reap-` is this
# supervisor's own and is STRIPPED before `execvp`, so the provider's argv and
# environment are exactly what main() would have given it (design C7, R11):
#   --reap-descendants      reaping mode (required by the three below)
#   --reap-grace=SECONDS    how long the provider gets after SIGTERM; 0 = none
#   --reap-state=PATH       the host's snapshot file (below)
#   --reap-log=PATH         where kills and failures are written
# The operator passes only the first and keeps its environment names
# (OPERATOR_REAP_GRACE, RICHOS_OPERATOR_REAP_STATE, RICHOS_OPERATOR_REAP_LOG),
# which stay the fallback. The product passes all four on the command line, so
# nothing of the reap reaches `claude`'s environment or any tool shell's.
#
# WHY. A tool shell of a Claude lead runs in its OWN session and process group
# (measured: shell pid = pgid, tty `??`), so the final `killpg` of this group
# never reaches it. With the flag, this supervisor reads the process table IN
# PROCESS once a second (libproc on macOS, /proc on Linux; never by spawning
# `ps`) and records every descendant of the provider as (pid, start time, pgid),
# and every process group a descendant belongs to. Those are processes its own
# child created: owned, never matched by name. An entry is forgotten once its
# process is gone, and a group once it is no longer proven ours, so the record
# holds what is alive rather than everything a long lease ever ran (design C4).
#
# WHEN THE PROVIDER MUST END (the desktop died, a SIGTERM arrived, or the
# provider exited by itself, G9): a final snapshot, then the provider is ended:
# with a grace above zero, SIGTERM and a wait of up to that many seconds
# (operator default 5); with a grace of zero (the product), SIGKILL at once to
# the provider, which is this process's own unreaped child and so cannot be a
# recycled id (design C3). Then SIGKILL to every recorded group that still has a
# recorded member alive or whose leader still has its recorded start time, and
# to every recorded descendant still alive outside those groups, checked by
# start time so a recycled id is never signaled. It re-scans and repeats, at
# most three passes, then ends its own group exactly as main() does. Every kill,
# anything found alive after the last pass, and a reap that failed goes to the
# reap log (--reap-log, else RICHOS_OPERATOR_REAP_LOG, else stderr).
#
# THE HOST NEVER SIGKILLS THIS GROUP FIRST. A SIGKILL cannot be caught, so a
# host that killed this group would end the reap before it began and leave the
# tool shells running (design C1, R9). The product host sends SIGTERM to this
# process alone and escalates to the group only after a bound
# (`OwnedChild::supervised`, owned_process.rs).
#
# FOR THE HOST'S IDLE TEST (G8) AND THE PRODUCT'S WORK READING (design C5):
# with a state path, each snapshot writes {"owner": <the desktop's pid>,
# "outside_provider_group": [pid, ...], "outside_provider_groups": [pgid, ...]}:
# descendants alive OUTSIDE the provider's own group, and their distinct groups
# (one per running command). Language servers, MCP servers and caffeinate share
# the provider's group and do not count; tool shells and background commands
# have their own groups and do.
#
# THE LIMIT, STATED: a process that detaches itself and starts children, all
# inside the second between two snapshots, can leave those children. A shared
# daemon a lead started (the `adb` server) is killed with that lead. A VM guest a
# lead started is killed mid-boot; its clone on disk is left for the scratch
# reaper (Frank §3).


class Settings(object):
    """The reap's parameters: the supervisor's own command line first (the product), then its
    environment (the operator), then the operator's defaults."""
    grace = None
    state = None
    log = None


SETTINGS = Settings()


def _darwin_libproc():
    """ctypes, libproc and the proc_bsdinfo layout, loaded once. Called before the fork so a
    runtime that cannot load them says so at the start, never silently at the first snapshot
    (design C8)."""
    cached = getattr(_darwin_libproc, "cached", None)
    if cached:
        return cached
    import ctypes

    class Info(ctypes.Structure):
        _fields_ = [("a", ctypes.c_uint32 * 3), ("pid", ctypes.c_uint32), ("ppid", ctypes.c_uint32),
                    ("b", ctypes.c_uint32 * 7), ("comm", ctypes.c_char * 16), ("name", ctypes.c_char * 32),
                    ("nfiles", ctypes.c_uint32), ("pgid", ctypes.c_uint32), ("c", ctypes.c_uint32 * 3),
                    ("nice", ctypes.c_int32), ("start", ctypes.c_uint64), ("ustart", ctypes.c_uint64)]
    _darwin_libproc.cached = (ctypes, ctypes.CDLL("/usr/lib/libproc.dylib"), Info)
    return _darwin_libproc.cached


def _process_table():
    """{pid: (ppid, pgid, start)} for every process we can read. `start` is the process's
    start time in whole MICROSECONDS (design C10), an identity rather than a clock."""
    if sys.platform == "darwin":
        ctypes, lib, Info = _darwin_libproc()
        count = lib.proc_listallpids(None, 0)
        pids = (ctypes.c_int * (count + 256))()
        n = lib.proc_listallpids(pids, ctypes.sizeof(pids))
        table, info = {}, Info()
        for pid in pids[:max(0, n)]:
            if pid <= 0:
                continue
            if lib.proc_pidinfo(pid, 3, ctypes.c_uint64(0), ctypes.byref(info), ctypes.sizeof(info)) == 136:
                if int(info.a[1]) != 5:          # pbi_status SZOMB: a zombie is already dead
                    table[pid] = (int(info.ppid), int(info.pgid), int(info.start) * 1000000 + int(info.ustart))
        return table
    table = {}
    try:
        with open("/proc/stat") as fh:
            btime = next(int(l.split()[1]) for l in fh if l.startswith("btime "))
    except (OSError, StopIteration, ValueError):
        btime = 0
    ticks = os.sysconf("SC_CLK_TCK")
    for name in os.listdir("/proc"):
        if not name.isdigit():
            continue
        try:
            with open("/proc/%s/stat" % name) as fh:
                raw = fh.read()
            rest = raw[raw.rindex(")") + 2:].split()
            if rest[0] != "Z":
                table[int(name)] = (int(rest[1]), int(rest[2]), btime * 1000000 + int(rest[19]) * 1000000 // ticks)
        except (OSError, ValueError, IndexError):
            continue
    return table


SESSION_TAG = "RICHOS_SESSION_PID"   # the provider's own pid, set in its environment before the exec


def _environment(pid):
    """The process's environment strings (bytes), or None when the OS will not show them.
    Read in process (KERN_PROCARGS2 on macOS, /proc on Linux), never by spawning `ps`."""
    if sys.platform == "darwin":
        try:
            ctypes = _darwin_libproc()[0]
            libc = ctypes.CDLL(None, use_errno=True)
            argmax = ctypes.c_int(0)
            size = ctypes.c_size_t(ctypes.sizeof(argmax))
            if libc.sysctl((ctypes.c_int * 2)(1, 8), 2, ctypes.byref(argmax), ctypes.byref(size), None, 0):
                return None                                           # KERN_ARGMAX
            buf = ctypes.create_string_buffer(argmax.value)
            size = ctypes.c_size_t(len(buf))
            if libc.sysctl((ctypes.c_int * 3)(1, 49, pid), 3, buf, ctypes.byref(size), None, 0):
                return None                                           # KERN_PROCARGS2
            raw = buf.raw[:size.value]
            argc, rest = int.from_bytes(raw[:4], "little"), raw[4:]
            i = rest.index(b"\0")
            while i < len(rest) and rest[i] == 0:
                i += 1
            env = []
            for item in rest[i:].split(b"\0")[argc:]:
                if not item:
                    break
                env.append(item)
            return env
        except (OSError, ValueError, AttributeError):
            return None
    try:
        with open("/proc/%d/environ" % pid, "rb") as fh:
            return [item for item in fh.read().split(b"\0") if item]
    except OSError:
        return None


class Record(object):
    """What the provider has below it now, with birth identities."""

    def __init__(self, provider, own_group):
        self.provider = provider
        self.own_group = own_group
        self.procs = {}           # pid -> (start, pgid)
        self.groups = {}          # pgid -> leader start (or None when the leader was never seen)
        self.tag = ("%s=%d" % (SESSION_TAG, provider)).encode()
        self.judged = {}          # pid -> start of a reparented process whose environment says not ours

    def snapshot(self, table=None):
        table = _process_table() if table is None else table
        children = {}
        for pid, (ppid, _pg, _st) in table.items():
            children.setdefault(ppid, []).append(pid)
        self.walk([self.provider] + list(self.procs), table, children)
        self.adopt_detached(table, children)
        # Everything in a recorded group, including members reparented to launchd,
        # but only for a group PROVEN still ours by a live recorded member or by its
        # leader's recorded start. A group id recycled by an unrelated process is
        # never adopted, so it is never signaled either.
        ours = self.live_groups(table)
        for pid, (_ppid, pgid, start) in table.items():
            if pgid in ours and pid not in self.procs:
                self.procs[pid] = (start, pgid)
        self.prune(table, ours)
        return table

    def walk(self, roots, table, children):
        """Record `roots` and everything below them, with every process group they are in."""
        stack = [p for p in roots if p in table]
        seen = set()
        while stack:
            pid = stack.pop()
            if pid in seen:
                continue
            seen.add(pid)
            ppid, pgid, start = table[pid]
            known = self.procs.get(pid)
            if known and known[0] != start:
                continue                      # a recycled id: not ours
            if pid != self.provider or not known:
                self.procs[pid] = (start, pgid)
            if pgid not in (self.own_group, 0, 1):
                leader = table.get(pgid)
                self.groups.setdefault(pgid, leader[2] if leader and leader[1] == pgid else None)
            stack.extend(children.get(pid, []))

    def adopt_detached(self, table, children):
        """A COMMAND STARTED DETACHED (Tom's review of P0, finding 1, 2026-10-29: nightly 80
        started with `nohup … &` was in none of the lease files). Its tool shell exits at once,
        inside the second between two snapshots, so the command is reparented to launchd in a
        group no snapshot ever saw: neither counted by the update gate and the quit question
        nor reaped. Every process the provider starts inherits SESSION_TAG=<provider pid>
        (set before the exec, below), so a tree reparented to launchd, started after the
        provider, in which any process carries exactly that tag is the provider's own: it is
        recorded with its groups like any other descendant. macOS shows no environment for its
        own binaries (/bin/sh, /bin/sleep), so the tree is searched, not only its root; a root
        whose readable environment lacks the tag is decided once and never read again."""
        born = (self.procs.get(self.provider) or (None,))[0]
        if born is None:
            return
        self.judged = {pid: start for pid, start in self.judged.items() if pid in table and table[pid][2] == start}
        for pid, (ppid, _pgid, start) in table.items():
            if ppid != 1 or pid in self.procs or start < born or self.judged.get(pid) == start:
                continue
            tree, stack = [], [pid]
            while stack and len(tree) < 256:
                p = stack.pop()
                tree.append(p)
                stack.extend(children.get(p, []))
            first = _environment(pid)
            if (first is not None and self.tag in first) or any(
                    self.tag in (_environment(p) or ()) for p in tree[1:]):
                self.walk([pid], table, children)
            elif first is not None:
                self.judged[pid] = start

    def prune(self, table, ours):
        """Forget what can never be signaled again (design C4): a process that is gone or
        whose id now belongs to another start, and a group no longer proven ours."""
        self.procs = {pid: (start, pgid) for pid, (start, pgid) in self.procs.items()
                      if pid in table and table[pid][2] == start}
        self.groups = {pgid: start for pgid, start in self.groups.items() if pgid in ours}

    def live_groups(self, table):
        """The recorded groups still proven ours: the leader alive with its recorded start, a
        recorded member alive, or (a detached command left in its exited tool shell's group,
        review rv-20261031T063529Z-4ca7addc-9ce9) a leader gone while the group still has a
        member. A group id is never reused while any process is in that group (POSIX), and a
        group with no member at one snapshot is forgotten (prune), so a group kept here was never
        empty since it was recorded. A leader id now held by another start is a reused id: not ours."""
        members = {pg for pid, (start, pg) in self.procs.items() if pid in table and table[pid][2] == start}
        occupied = {pg for _ppid, pg, _start in table.values()}
        ours = set()
        for pgid, leader_start in self.groups.items():
            if leader_start is not None and pgid in table and table[pgid][2] == leader_start:
                ours.add(pgid)
            elif pgid in members:
                ours.add(pgid)
            elif pgid not in table and pgid in occupied:
                ours.add(pgid)
        return ours

    def outside_provider_group(self, table):
        """G8: descendants still alive OUTSIDE the provider's own group. Language
        servers, MCP servers and caffeinate share the provider's group and do not
        count; tool shells and background commands have their own groups and do."""
        return sorted(pid for pid, (start, pgid) in self.procs.items()
                      if pgid != self.own_group and pid != self.provider and pid in table and table[pid][2] == start)


def _log(line):
    path = (SETTINGS.log or os.environ.get("RICHOS_OPERATOR_REAP_LOG") or "").strip()
    stamp = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    text = "%s provider-supervisor[%d]: %s\n" % (stamp, os.getpid(), line)
    try:
        if path:
            with open(path, "a", encoding="utf-8") as fh:
                fh.write(text)
        else:
            sys.stderr.write(text)
            sys.stderr.flush()
    except OSError:
        pass


def _write_state(record, table, owner, ended=False):
    path = (SETTINGS.state or os.environ.get("RICHOS_OPERATOR_REAP_STATE") or "").strip()
    if not path:
        return
    try:
        import json
        outside = record.outside_provider_group(table)
        groups = sorted({record.procs[pid][1] for pid in outside})
        tmp = "%s.%d.tmp" % (path, os.getpid())
        with open(tmp, "w", encoding="utf-8") as fh:
            json.dump({"provider": record.provider, "own_group": record.own_group, "owner": owner, "at": time.time(),
                       "ended": ended, "outside_provider_group": outside, "outside_provider_groups": groups}, fh)
        os.replace(tmp, path)
    except OSError:
        pass


def _grace():
    if SETTINGS.grace is not None:
        return SETTINGS.grace
    try:
        return max(0.0, float(os.environ.get("OPERATOR_REAP_GRACE") or 5))
    except ValueError:
        return 5.0


def reap(record, provider_running):
    """The sequence of (q) item 2. Returns the pids found alive after the last pass."""
    table = record.snapshot() if provider_running else _process_table()
    grace = _grace()
    if provider_running and grace <= 0:
        # The product (design C3): no grace, so the provider ends as promptly as the host's
        # old group kill ended it, and the passes below follow within one pass.
        try:
            os.kill(record.provider, signal.SIGKILL)
        except ProcessLookupError:
            pass
    elif provider_running:
        try:
            os.kill(record.provider, signal.SIGTERM)
        except ProcessLookupError:
            pass
        deadline = time.monotonic() + grace
        while time.monotonic() < deadline:
            try:
                ended, _ = os.waitpid(record.provider, os.WNOHANG)
            except ChildProcessError:
                ended = record.provider
            if ended:
                break
            time.sleep(0.05)
    killed = []
    for _pass in range(3):
        table = _process_table()
        record.snapshot(table)
        # A group is signaled only while a recorded member is alive with its
        # recorded start, or its leader is: a group with neither is never hit.
        targets = [("group", pgid) for pgid in sorted(record.live_groups(table))]
        in_groups = {pg for _k, pg in targets}
        for pid, (start, pgid) in sorted(record.procs.items()):
            if pid == os.getpid() or pgid in in_groups or pgid == record.own_group:
                continue
            if pid in table and table[pid][2] == start:
                targets.append(("pid", pid))
        if not targets:
            break
        for kind, ident in targets:
            try:
                (os.killpg if kind == "group" else os.kill)(ident, signal.SIGKILL)
                killed.append("%s %d" % (kind, ident))
            except (ProcessLookupError, PermissionError):
                pass
        time.sleep(0.1)
    try:
        os.waitpid(record.provider, os.WNOHANG)
    except ChildProcessError:
        pass
    table = _process_table()
    left = sorted(pid for pid, (start, pgid) in record.procs.items()
                  if pid in table and table[pid][2] == start and pgid != record.own_group and pid != os.getpid())
    if killed:
        _log("killed %s" % ", ".join(killed))
    if left:
        _log("ALIVE after the last pass: %s" % ", ".join(str(p) for p in left))
    return left


SNAPSHOT_SECONDS = 1.0   # the reap's process snapshot, as before (every tenth 100 ms tick)
# A TOOL SHELL IS SEEN WHILE IT LIVES (review rv-20261031T063529Z-4ca7addc-9ce9): the provider's
# fork wakes the watch for a snapshot at once, so a shell that starts `nohup … &` and exits is
# recorded with its process group, and live_groups keeps that group while the detached command
# is in it. This finds a detached command whose environment macOS hides (its own binaries, such
# as /bin/sleep), which the session tag cannot. The limit: a shell that forks its command and
# exits within the few milliseconds before that snapshot, all of it system binaries (a login
# zsh alone lives 18-30 ms, measured 2026-10-31; Claude Code's tool shell also sources its snapshot).
FORK_NOTE = 0x40000000   # NOTE_FORK (sys/event.h), select.KQ_NOTE_FORK on macOS
FORK_FOLLOW_UPS = (0.01, 0.03)   # seconds after a fork: two more snapshots


def _exit_watch(provider, parent, wake_read):
    """A kqueue that fires when the provider or the owner exits or a signal's wakeup byte
    arrives, or None where there is no kqueue. Standard library only, like the rest of this file."""
    import select
    if not hasattr(select, "kqueue"):
        return None
    try:
        kq = select.kqueue()
    except OSError:
        return None
    for pid, notes in ((provider, select.KQ_NOTE_EXIT | FORK_NOTE), (parent, select.KQ_NOTE_EXIT)):
        try:
            kq.control([select.kevent(pid, select.KQ_FILTER_PROC, select.KQ_EV_ADD | select.KQ_EV_CLEAR,
                                      notes)], 0, 0)
        except OSError:
            pass   # already gone: the loop's own checks see it at once
    try:
        kq.control([select.kevent(wake_read, select.KQ_FILTER_READ, select.KQ_EV_ADD | select.KQ_EV_CLEAR)], 0, 0)
    except OSError:
        kq.close()
        return None
    return kq


def operator_main(parent, argv):
    import select
    terminate = []
    signal.signal(signal.SIGTERM, lambda *_a: terminate.append(True))
    # A SIGTERM wakes the wait below at once instead of at the next 100 ms tick (design C12):
    # Python restarts an interrupted sleep (PEP 475), but a wakeup descriptor ends a select.
    # Both ends are non-inheritable (PEP 446), so the provider's exec closes them.
    wake_read, wake_write = os.pipe()
    os.set_blocking(wake_write, False)
    signal.set_wakeup_fd(wake_write)
    if sys.platform == "darwin":
        try:
            _darwin_libproc()
        except Exception as error:   # noqa: BLE001  the watch still runs; the reap degrades
            _log("reap unavailable, the process table cannot be read: %s" % error)
    provider = os.fork()
    if provider == 0:
        signal.set_wakeup_fd(-1)
        signal.signal(signal.SIGTERM, signal.SIG_DFL)
        os.environ["RICHOS_SESSION_PID"] = str(os.getpid())
        try:
            os.execvp(argv[0], argv)
        except OSError as error:
            print(f"Provider could not start: {error}", file=sys.stderr, flush=True)
            os._exit(127)
    record = Record(provider, os.getpgrp())
    running = True
    # 2026-10-28: the watch blocks in the kernel until the provider or the owner exits or a
    # SIGTERM arrives (kqueue), waking only for the once-a-second snapshot; it used to wake every
    # 100 ms to ask. Where kqueue is missing it keeps the 100 ms select on the wakeup descriptor.
    kq = _exit_watch(provider, parent, wake_read)
    try:
        next_snapshot = time.monotonic()
        soon = []                         # the quick snapshots that follow a fork
        while os.getppid() == parent and not terminate:
            ended, _ = os.waitpid(provider, os.WNOHANG)
            if ended:
                running = False           # G9: reap from the last snapshot
                break
            if time.monotonic() >= next_snapshot:
                soon = [t for t in soon if t > time.monotonic()]
                next_snapshot = min([time.monotonic() + SNAPSHOT_SECONDS] + soon)
                try:
                    table = record.snapshot()
                    _write_state(record, table, parent)
                except Exception as error:   # noqa: BLE001  never let a read end the watch
                    _log("process table read failed: %s" % error)
            if kq is None:
                select.select([wake_read], [], [], 0.1)
                continue
            try:
                events = kq.control(None, 4, max(0.0, next_snapshot - time.monotonic()))
                if any(e.ident == provider and e.fflags & FORK_NOTE for e in events):
                    # A new tool shell: seen at once, and again once it has left the provider's
                    # group for its own (the fork comes before the child's setsid).
                    now = time.monotonic()
                    next_snapshot, soon = now, [now + d for d in FORK_FOLLOW_UPS]
            except OSError:
                select.select([wake_read], [], [], 0.1)
        cause = "the provider exited" if not running else ("SIGTERM" if terminate else "the owner died")
        _log("ending: %s" % cause)
        try:
            reap(record, running)
            # The last word in the state file is what is left, usually nothing: a host that
            # still holds this lease must not read its final snapshot's commands as running.
            _write_state(record, _process_table(), parent, ended=True)
            _sweep_process_scratch()
        except Exception as error:   # noqa: BLE001  said, then the group kill below still runs
            _log("reap failed: %s: %s" % (type(error).__name__, error))
    finally:
        os.killpg(os.getpid(), signal.SIGKILL)


def _sweep_process_scratch():
    # Only RichOS jobs that carry a resolved settings snapshot. The subprocess
    # inherits the exact allocating TMPDIR and stays in our teardown group.
    if os.environ.get("RICHOS_MACHINE_SETTINGS_RESOLVED") != "1":
        return
    from pathlib import Path
    import subprocess
    sweep = Path(__file__).with_name("scratch-reaper.sh")
    if not sweep.is_file():
        return
    budget = max(0.1, _grace())
    try:
        result = subprocess.run(["bash", str(sweep), "--process-only", "--apply", "--deadline", str(budget * 0.8)],
                                stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=budget)
        if result.returncode:
            _log("process scratch sweep incomplete (exit %s): %s" % (result.returncode, result.stderr.decode(errors="replace")[-500:]))
    except (OSError, subprocess.TimeoutExpired) as error:
        _log("process scratch sweep incomplete: %s" % error)


def _options(args):
    """Split the supervisor's own leading `--reap-*` options from the provider's command.
    Returns (reaping, provider argv). Nothing the provider is given starts before the first
    argument that is not one of these."""
    reaping, taken = False, 0
    for arg in args:
        if not arg.startswith("--reap-"):
            break
        name, _sep, value = arg.partition("=")
        if arg == "--reap-descendants":
            reaping = True
        elif name == "--reap-grace" and _sep:
            try:
                SETTINGS.grace = max(0.0, float(value))
            except ValueError:
                raise SystemExit("provider supervisor: --reap-grace needs a number of seconds, not %r" % value)
        elif name == "--reap-state" and value:
            SETTINGS.state = value
        elif name == "--reap-log" and value:
            SETTINGS.log = value
        else:
            raise SystemExit("provider supervisor: unknown option %r" % arg)
        taken += 1
    if taken and not reaping:
        raise SystemExit("provider supervisor: reap options need --reap-descendants")
    return reaping, args[taken:]


def _entry():
    reaping, argv = _options(sys.argv[1:])
    if reaping:
        if not argv or os.getpgrp() != os.getpid():
            raise SystemExit("provider supervisor requires an executable and its own process group")
        parent = os.getppid()
        if parent <= 1:
            raise SystemExit("provider supervisor has no desktop owner")
        return operator_main(parent, argv)
    return main()


if __name__ == "__main__":
    _entry()

#!/usr/bin/env python3
"""Own test processes through normal completion, timeouts and caller death.

`run OWNER -- COMMAND` starts a separate supervisor. It registers the child before
allowing exec, retains birth identities and process groups and tags descendants with
an inherited random scope. It cleans on normal exit too. A killed caller leaves the
supervisor holding any inherited worker leases until cleanup finishes.

`kill PID` is the legacy snapshot-based fallback. It cannot recover a process which
already detached and was reparented before the snapshot. New runners use `run`.

This is cooperative supervision, not an OS container. macOS hides environment tags
on SIP-protected binaries, so tracked ancestry and groups remain necessary. A program
that immediately double-forks, creates a new session and strips its inherited scope
can escape discovery. Deliberate destruction of the supervisor itself also requires
an external OS boundary. Do not claim isolation against arbitrary untrusted programs.
"""
import os
import resource
import uuid
import signal
import subprocess
import sys
import time
import operator_fences


class HeldClock:
    """Elapsed time that does not count the time this process was held (suspended).

    A pause (agent_hold.py) suspends an agent's running work with SIGSTOP and
    continues it with SIGCONT. time.monotonic() keeps advancing while a process is
    stopped, so every deadline inside held work expired during a long pause and the
    work killed itself the moment it was continued (Sage's catch 2, 2026-09-27).

    A stopped process cannot observe its own suspension except as a GAP: a polling
    loop that normally comes round in well under a second sees many seconds pass
    between two iterations. Each gap beyond `gap` seconds counts as held time:

        clock = HeldClock()
        deadline = clock.now() + 600       # instead of time.monotonic() + 600
        while ...:
            if clock.now() >= deadline: ...
            time.sleep(0.2)

    The loop must call now() at least once per iteration. The first `gap` seconds of
    a gap still count, so a loop that is merely slow under load loses nothing it
    did not spend; a host that slept is treated like a pause, which is right for a
    deadline meant to catch a HUNG process. It lives in this file, not a module of
    its own, because every runner and fixture that copies proc_tree.py (cpu_guard's
    installed runtime included) must keep working with that one file.
    """
    def __init__(self, gap=5.0, clock=time.monotonic):
        self.gap = float(gap)
        self.clock = clock
        self.held = 0.0
        self.gaps = 0
        self._last = clock()

    def now(self):
        t = self.clock()
        d = t - self._last
        if d > self.gap:
            self.held += d - self.gap
            self.gaps += 1
        self._last = t
        return t - self.held

    def since(self, start):
        return self.now() - start


# A `ps` that "timed out" may only have been SUSPENDED with its caller: a pause
# (agent_hold.py) stops the supervisor and its ps child together, and at RESUME
# Python finds the 10 s timeout long past and raises, which used to end the
# supervised check (finish_scope): a RESUME killing its own test, Sage's catch 2.
# ps is a read, so it is simply asked again. A timeout raised well after its time
# was a suspension, and is never counted; a ps that really hangs still fails after
# PS_ATTEMPTS timeouts that were not.
PS_TIMEOUT = 10
PS_ATTEMPTS = 3


def _ps(argv):
    hung = 0
    for _ in range(100):
        began = time.monotonic()
        try:
            return subprocess.run(argv, capture_output=True, text=True, timeout=PS_TIMEOUT)
        except subprocess.TimeoutExpired:
            if time.monotonic() - began - PS_TIMEOUT <= 0.25 * PS_TIMEOUT:
                hung += 1
                if hung >= PS_ATTEMPTS:
                    raise
    raise subprocess.TimeoutExpired(argv, PS_TIMEOUT)


def snapshot():
    """{pid: (ppid, pgid)} for every process, from one `ps` call."""
    out = subprocess.run(["ps", "-A", "-o", "pid=,ppid=,pgid="], capture_output=True, text=True).stdout
    table = {}
    for line in out.splitlines():
        parts = line.split()
        if len(parts) == 3 and all(p.isdigit() for p in parts):
            pid, ppid, pgid = (int(p) for p in parts)
            table[pid] = (ppid, pgid)
    return table


def members(root, table=None):
    """(pids, groups): the root, its descendants, and the groups they are in (minus our own)."""
    table = table if table is not None else snapshot()
    children = {}
    for pid, (ppid, _pgid) in table.items():
        children.setdefault(ppid, []).append(pid)
    pids, stack = set(), [root]
    while stack:
        p = stack.pop()
        if p in pids:
            continue
        pids.add(p)
        stack.extend(children.get(p, []))
    own = os.getpgid(0)
    groups = {table[p][1] for p in pids if p in table} - {own, 0, 1}
    # Everything in those groups, including members re-parented to init before the snapshot.
    pids |= {pid for pid, (_pp, pg) in table.items() if pg in groups}
    pids.discard(os.getpid())
    return pids, groups


def _send(pids, groups, sig, table=None):
    """Each process gets the signal ONCE: through its group when its group is one of ours,
    directly otherwise. A second SIGTERM landing while a shell runs its EXIT trap (the first
    one's) kills it mid-trap, and the trap's cleanup is lost — measured, 1 run in 11 of
    mutation-focus.test.sh F1c before this was one signal per process."""
    table = table if table is not None else snapshot()
    for g in groups:
        try:
            os.killpg(g, sig)
        except (ProcessLookupError, PermissionError):
            pass
    for p in pids:
        if p in table and table[p][1] in groups:
            continue
        try:
            os.kill(p, sig)
        except (ProcessLookupError, PermissionError):
            pass


def _alive(pids):
    pids = set(pids)
    if not pids:
        return []
    # Unreadable native identities are common for other users' processes, not
    # an exceptional one-PID case. One status snapshot preserves zombie/exit
    # handling without spawning a ps child for every such process on every tick.
    result = _ps(["ps", "-o", "pid=,stat=", "-p", ",".join(map(str, sorted(pids)))])
    if result.returncode not in (0, 1) or (result.returncode == 0 and not result.stdout.strip()):
        raise RuntimeError("cannot read owned process states")
    left = []
    for line in result.stdout.splitlines():
        fields = line.split()
        if len(fields) != 2 or not fields[0].isdigit() or int(fields[0]) not in pids:
            raise RuntimeError("invalid process state snapshot")
        if not fields[1].startswith("Z"):
            left.append(int(fields[0]))
    return left


def kill_tree(root, grace=3.0):
    table = snapshot()
    pids, groups = members(root, table)
    if not pids:
        return []
    _send(pids, groups, signal.SIGTERM, table)
    deadline = time.monotonic() + grace
    while time.monotonic() < deadline and _alive(pids):
        time.sleep(0.1)
    # Anything spawned since the first snapshot, in the same groups, goes too.
    later = snapshot()
    pids |= {pid for pid, (_pp, pg) in later.items() if pg in groups}
    pids.discard(os.getpid())
    _send(pids, groups, signal.SIGKILL, later)
    time.sleep(0.2)
    return _alive(pids)


SCOPE_ENV = "RICHOS_PROCESS_SCOPE"


def python_command():
    flags = (["-S"] if sys.flags.no_site else []) + (["-s"] if sys.flags.no_user_site else [])
    return [sys.executable, *flags]


def command(argv, owner=None, verification=None, release=None):
    """A separate supervisor survives the caller's SIGKILL and owns normal-exit cleanup too."""
    options = ["--verification", str(verification)] if verification else []
    if release:
        options += ["--release-gradle-daemons", str(release)]
    return [*python_command(), os.path.abspath(__file__), "run", str(owner or os.getpid()), *options, "--", *map(str, argv)]


def identity(pid):
    row = operator_fences.proc(pid, precise=True)
    return row["start"] if row and not row["zombie"] else None


def native_processes(pids):
    """Reconcile native lookup with process exit; retain live unknown identities.

    Both the supervisor and controller use this boundary. A stale ps row is not
    a live unreadable process, and a status result is never a birth identity.
    """
    table = {}
    unknown = set()
    for pid in pids:
        row = operator_fences.proc(pid, precise=True)
        if row and row["zombie"]:
            continue
        if row is None:
            # libproc can return ESRCH for an unreaped zombie while kill(pid, 0)
            # still succeeds. Batch the status check for all unreadable rows.
            unknown.add(pid)
        table[pid] = row
    live_unknown = set(_alive(unknown))
    for pid in unknown - live_unknown:
        del table[pid]
    # Native identity and ps status are separate observations. A process can
    # cross exec while the first native read is unavailable. Use the later
    # native generation when available; never infer it from PID or status.
    unresolved = set()
    for pid in live_unknown:
        row = operator_fences.proc(pid, precise=True)
        if row and row["zombie"]:
            del table[pid]
        elif row:
            table[pid] = row
        else:
            unresolved.add(pid)
    # A short-lived process may exit after the status sample but before the
    # second native read. Confirm only these remaining unknowns once more.
    # A still-live unreadable process remains unknown and cannot pass cleanup.
    for pid in unresolved - set(_alive(unresolved)):
        del table[pid]
    return table


def _tied(topology, seeds, groups):
    """Every pid tied to `seeds` by parent links or process groups, read off the
    one `ps` topology: the seeds, their descendants, members of their groups and
    of `groups`, repeated until nothing new joins. The caller's own group, 0 and
    1 never tie anything, the same exclusion TrackedTree.refresh applies."""
    skip = {os.getpgrp(), 0, 1}
    groups = set(groups) - skip
    tied = {pid for pid in seeds if pid in topology}
    while True:
        groups |= {topology[pid][1] for pid in tied} - skip
        more = {pid for pid, (parent, group, _) in topology.items()
                if parent in tied or group in groups} - tied
        if not more:
            return tied
        tied |= more


# THE PROCESS TABLE IS READ IN-PROCESS, NOT BY STARTING `ps` (2026-10-22). Every supervisor
# tick started a setuid `ps -ax`; at 20 supervisors that was about 100 new processes a second
# and 12.4% of the Mac in Hank's busy window (richos-hq
# docs/research/2026-10-22-mac-system-cpu-after.md). `ps` itself gets these three numbers
# from one sysctl(KERN_PROC_ALL); reading it here gives the same rows (every process, zombies
# included) without a process. struct kinfo_proc: kp_proc.p_pid at 40, kp_eproc.e_ppid at 560,
# e_pgid at 564, 648 bytes a row on 64-bit macOS. The layout is checked against this process's
# own row on every read, and anything unexpected falls back to `ps`.
_KINFO_SIZE, _PID_AT, _PPID_AT, _PGID_AT = 648, 40, 560, 564
_libc = []


def _kernel_topology():
    """[(pid, ppid, pgid)] for every process from one sysctl, or None when not available."""
    if sys.platform != "darwin":
        return None
    import ctypes
    import struct
    try:
        if not _libc:
            _libc.append(ctypes.CDLL(None, use_errno=True))
        libc = _libc[0]
        mib = (ctypes.c_int * 4)(1, 14, 0, 0)       # CTL_KERN, KERN_PROC, KERN_PROC_ALL
        for _ in range(5):
            size = ctypes.c_size_t(0)
            if libc.sysctl(mib, 4, None, ctypes.byref(size), None, 0):
                return None
            size.value += size.value // 4 + _KINFO_SIZE * 32   # processes born in between
            buf = ctypes.create_string_buffer(size.value)
            if libc.sysctl(mib, 4, buf, ctypes.byref(size), None, 0) == 0:
                break
            if ctypes.get_errno() != 12:            # ENOMEM: the table grew; ask again
                return None
        else:
            return None
    except (OSError, AttributeError, ValueError):
        return None
    if size.value % _KINFO_SIZE:
        return None
    raw = buf.raw[:size.value]
    rows = [struct.unpack_from("i", raw, at + _PID_AT) + struct.unpack_from("ii", raw, at + _PPID_AT)
            for at in range(0, len(raw), _KINFO_SIZE)]
    me = os.getpid()
    if (me, os.getppid(), os.getpgrp()) not in rows:
        return None
    return rows


def process_rows(seeds=None, groups=()):
    """Topology plus native birth generations, without a subprocess per identity.

    With `seeds`, only processes tied to them (see _tied) get a native identity
    read; every other row keeps its `ps` topology and no generation (None), so it
    can never pass a birth-identity check. A supervisor's tick asks this five
    times a second, and reading all ~650 processes each time, plus a second `ps`
    for the ~300 root processes it can never own, was ~8% of the Mac
    (2026-10-21). The protection against a reused PID is unchanged: every process
    the tree could claim is still read natively and compared with its recorded
    birth, and a newcomer reached only through a native row is read again by
    refresh's own reconciliation before it can be owned. The table comes from the kernel
    in-process (_kernel_topology), `ps` only where that is unavailable."""
    topology = {}
    kernel = _kernel_topology()
    if kernel is not None:
        for pid, parent, group in kernel:
            if pid > 0:                     # `ps -ax` never lists pid 0 (kernel_task)
                topology[pid] = (parent, group, None)
    else:
        result = _ps(["ps", "-ax", "-o", "pid=,ppid=,pgid="])
        if result.returncode:
            raise subprocess.CalledProcessError(result.returncode, result.args, result.stdout, result.stderr)
        for line in result.stdout.splitlines():
            fields = line.split()
            if len(fields) == 3 and all(field.isdigit() for field in fields):
                pid, parent, group = map(int, fields)
                topology[pid] = (parent, group, None)
    if seeds is None:
        return {pid: (row['ppid'], row['pgid'], row['start']) if row else topology[pid]
                for pid, row in native_processes(topology).items()}
    tied = _tied(topology, seeds, groups)
    table = {pid: row for pid, row in topology.items() if pid not in tied}
    table.update({pid: (row['ppid'], row['pgid'], row['start']) if row else topology[pid]
                  for pid, row in native_processes({p: topology[p] for p in tied}).items()})
    return table


def scoped_members(scope):
    """Inherited random scope finds detached, reparented descendants. Never print environments."""
    result = _ps(["ps", "-Eww" if sys.platform == "darwin" else "eww", "-ax", "-o", "uid=,pid=,command="])
    if result.returncode:
        raise RuntimeError("cannot inspect owned processes: ps failed")
    found = set()
    for line in result.stdout.splitlines():
        parts = line.split(None, 2)
        if len(parts) != 3 or parts[0] != str(os.getuid()):
            continue
        for field in parts[2].split():
            if field.startswith(SCOPE_ENV + "=") and scope in field.split("=", 1)[1].split(":"):
                found.add(int(parts[1]))
    found.discard(os.getpid())
    return found


class TrackedTree:
    """Retain birth identities before any parent exits, including protected macOS binaries.

    macOS hides environments of SIP-protected executables. Parent links and process
    groups complement inherited scope tags; no process names or executable paths grant ownership.
    """
    def __init__(self, root, scope):
        self.root, self.scope, self.known = root, scope, {}
        self.root_birth = identity(root)
        if self.root_birth is None:
            raise RuntimeError("cannot establish owned root generation: %s" % root)
        self.groups = {}
        self.empty_groups = set()
        # {pid: last proven generation, or None}: members whose generation could not be
        # read at the last sample. See refresh() for why they are carried, not fatal.
        self.unresolved = {}
        # {pid: generation}: proven members handed over by release() (a Gradle daemon kept
        # warm for its workspace). They, their descendants and a group one of them leads are
        # no longer this tree's: never signaled, never counted as survivors.
        self.released = {}
        self.refresh()

    def release(self, members):
        """Hand proven members over to their next owner. Only a member whose generation was
        proven by this tree can be released; the caller decides which (see
        release_gradle_daemons)."""
        for pid, generation in members.items():
            if self.known.get(pid) == generation:
                self.released[pid] = generation
                self.known.pop(pid, None)
                self.groups.pop(pid, None)

    def _released_closure(self, table):
        """Released members still alive as themselves, their descendants by current parent
        links, and the members of a process group a released member leads."""
        if not self.released:
            return set()
        out = {pid for pid, generation in self.released.items()
               if pid in table and table[pid][2] in (generation, None)}
        leaders = {pid for pid in out if table[pid][1] == pid}
        grew = True
        while grew:
            grew = False
            for pid, (parent, group, _) in table.items():
                if pid not in out and (parent in out or group in leaders):
                    out.add(pid)
                    grew = True
        return out

    def refresh(self, tags=False):
        """Re-sample the tree. Returns every live member: proven ones (`known`, the only
        processes finish_scope may ever signal) and `unresolved` ones.

        UNRESOLVED IS NOT FATAL (nightly 20260928T165947Z-b83ab374, gates/ui-suite, pid
        42499). A live member whose native generation cannot be read is not rare: libproc
        answers nothing for a process that is setuid (macOS /bin/ps is setuid root, and a
        nested supervisor runs ps several times a second) or still inside exit, and at
        100% CPU either state easily outlasts the one-second reconciliation below. That
        used to raise, which failed the supervision, skipped its cleanup (finish_scope
        raised the same error) and refused a UI suite in which nothing had failed.

        Now such a process is CARRIED: never signaled (its identity is not proven, so it is
        not in `known`), still counted as a live member so the scope is not called clean
        and its permits are not released while it runs, and re-read at every later sample;
        the moment its generation is readable and matches, it is proven again. If it is
        still alive when cleanup's hard deadline passes, finish_scope reports it as an
        unidentified survivor, which is the same verdict any surviving member gets.
        """
        tagged = scoped_members(self.scope) if tags else set()
        # Only this tree's processes are read natively: everything it already
        # holds, what it released, what carries its scope tag, and whatever is
        # tied to those by parent or group (process_rows).
        table = process_rows(seeds={self.root, *self.known, *self.unresolved, *self.released,
                                    *self.groups, *tagged},
                             groups=self.groups)
        clock = HeldClock(gap=0.5)
        deadline = clock.now() + 1.0
        carried = {pid: birth for pid, birth in self.unresolved.items() if birth is not None}
        settle = False
        while True:
            excluded = self._released_closure(table)
            owned = {pid for pid, birth in {**carried, **self.known}.items()
                     if pid in table and table[pid][2] == birth} - excluded
            if self.root in table and table[self.root][2] == self.root_birth:
                owned.add(self.root)
            # Preserve a group until empty, but reject a reused leader. An
            # unreadable leader must first be reconciled, never assumed owned.
            groups = {g: birth for g, birth in self.groups.items()
                      if g not in table or table[g][2] in (birth, None)}
            owned |= {p for p, row in table.items() if row[1] in groups} - excluded
            owned |= tagged - excluded
            while True:
                active_groups = {table[p][1] for p in owned if p in table} - {os.getpgrp(), 0, 1}
                more = {pid for pid, (parent, group, _) in table.items()
                        if parent in owned or group in active_groups} - excluded
                if more <= owned:
                    break
                owned |= more
            unknown = {pid for pid in owned | set(self.known) | set(self.unresolved) | {self.root}
                       if pid in table and table[pid][2] is None}
            if not unknown or settle:
                break
            # exec/exit can make libproc temporarily unavailable while ps still
            # sees the process. Hold ownership and permits during this bounded
            # reconciliation. Neither PID nor ps status establishes a generation.
            alive = set(_alive(unknown))
            for pid in unknown:
                row = operator_fences.proc(pid, precise=True)
                if row and not row["zombie"]:
                    table[pid] = (row["ppid"], row["pgid"], row["start"])
                elif row or pid not in alive:
                    del table[pid]
            unresolved = {pid for pid in unknown if pid in table and table[pid][2] is None}
            # Wait (bounded) only for a NEWLY unreadable member: one already carried from the
            # last sample is re-read at the next, not waited on here at every tick. When the
            # bound passes, carry what is left instead of failing the supervision.
            if not unresolved or unresolved <= set(self.unresolved) or clock.now() >= deadline:
                settle = True
            else:
                time.sleep(0.01)
            # Recompute ownership with the newly read topology/generation. A
            # recycled PID with a different parent/group must not join our tree.
        empty = {g for g in groups if not any(row[1] == g for row in table.values())}
        self.groups = {g: birth for g, birth in groups.items()
                       if not (g in empty and g in self.empty_groups)}
        self.empty_groups = empty
        for pid in owned:
            if pid in table:
                group = table[pid][1]
                if group in table and group not in (os.getpgrp(), 0, 1):
                    # An unreadable leader never overwrites the generation already proven
                    # for its group; a new group led by one is kept only until its leader
                    # reads as anything at all (see the filter at the top of the loop).
                    if table[group][2] is not None:
                        self.groups[group] = table[group][2]
                    else:
                        self.groups.setdefault(group, None)
        previous = {**self.unresolved, **self.known}
        members = (owned | set(self.known) | set(self.unresolved) | {self.root}) - {os.getpid()} - excluded
        self.known = {pid: table[pid][2] for pid in owned
                      if pid in table and pid != os.getpid() and table[pid][2] is not None}
        self.unresolved = {pid: (previous.get(pid) if pid != self.root else self.root_birth)
                           for pid in members if pid in table and table[pid][2] is None}
        return set(self.known) | set(self.unresolved)


def release_gradle_daemons(tracker, registry):
    """At a normal end of a build: release the Gradle daemon(s) it started for its workspace.

    A survivor is released only when BOTH hold: it is a proven member of this tree (its birth
    generation tracked by this supervisor, re-read now), and Gradle recorded THIS process as a
    daemon in `registry`, the workspace's private daemon registry (a daemon-<pid>.out.log
    written at or after its start; scripts/lib/gradle_daemons.py). Its descendants and its
    own process group go with it. Everything else is cleaned up exactly as before. What was
    released is written to <registry>.released.json for native-work.py to register.
    """
    import gradle_daemons
    tracker.refresh(tags=True)
    released = {pid: birth for pid, birth in tracker.known.items()
                if pid != tracker.root and identity(pid) == birth
                and gradle_daemons.is_announced_daemon(pid, birth, registry)}
    if released:
        tracker.release(released)
        gradle_daemons.write_json(gradle_daemons.released_path(registry), {
            'released': [{'pid': pid, 'generation': birth} for pid, birth in sorted(released.items())],
            'supervisor': os.getpid(), 'at': time.time()})
    return released


def finish_scope(child, tracker, grace=8.0):
    """Re-scan while stopping so a TERM trap cannot fork an uncounted survivor."""
    clock = HeldClock()
    deadline = clock.now() + grace
    killed_at = deadline + 3.0
    initial = True
    empty = False
    while True:
        child.poll()
        alive = set(_alive(tracker.refresh(tags=True)))
        if not alive:
            # Topology and native identities are two observations. A parent can
            # fork and exit between them. Confirm absence in a subsequent sample
            # before releasing the permits that cover its background child.
            if empty:
                return []
            empty = True
            time.sleep(0.05)
            continue
        empty = False
        hard = clock.now() >= deadline
        # Signal the original tree once. A shell's EXIT trap may start cleanup
        # commands during the grace period; terminating those immediately defeats
        # cooperative cleanup. Track them and kill any survivors at the deadline.
        # Only a PROVEN member is ever signaled: an unresolved one (tracker.unresolved) has
        # no generation to compare, and None == None must never pass for identity.
        for pid in alive if hard or initial else ():
            birth = tracker.known.get(pid)
            try:
                if birth is not None and identity(pid) == birth:
                    os.kill(pid, signal.SIGKILL if hard else signal.SIGTERM)
            except ProcessLookupError:
                pass
        initial = False
        if clock.now() >= killed_at:
            return sorted(alive)
        time.sleep(0.05)


GUARD_ROLES = ("session", "release-build")

# THE SUPERVISOR WAITS FOR ITS TREE TO CHANGE, NOT FOR A CLOCK (2026-10-28). It used to sample
# the whole tree every 0.2 s for as long as the command ran: about 0.8 s of CPU per minute for
# a command that only waited (measured: `proc_tree.py run <owner> -- sleep 30`, 0.41 s), in every
# supervised check at once. Now it blocks in the kernel (quiet_wait.py) until a member it knows
# forks or exits, the command or the owner exits, or a signal arrives, and samples then, at
# most every SAMPLE_SECONDS as before (so a tree forking all the time costs what it did, and a
# new child is seen at least as soon). With nothing happening it samples every
# IDLE_SAMPLE_SECONDS: the verification controller's health is read once a second
# (cpu_guard.VerificationClient.check_health), and HeldClock counts a gap over 5 s as a pause.
SAMPLE_SECONDS = 0.2
IDLE_SAMPLE_SECONDS = 1.0


def tree_watch(child, owner, members):
    """A quiet_wait.Watch on the command, its owner, the members' forks and exits and this
    process's stop signals, or None where quiet_wait.py is not beside this file (a fixture or
    runtime that copies proc_tree.py alone keeps sampling every SAMPLE_SECONDS, as before)."""
    try:
        from quiet_wait import Watch
    except ImportError:
        return None
    watch = Watch(pids=[child, owner])
    watch.add_pids(members, forks=True)
    watch.add_signals((signal.SIGINT, signal.SIGTERM, signal.SIGHUP))
    return watch


def supervise(owner, argv, deadline=None, timeout_marker=None, verification=None, guard_role="session",
              release=None):
    # `release` is a workspace's private Gradle daemon registry (native-work.py). At a normal
    # end of the command, the daemon Gradle started there is released instead of ended
    # (release_gradle_daemons). Never with verification: managed accounting owns the tree.
    # Enroll managed workloads even when invoked by Codex or a nightly without
    # Claude hooks. Fixtures copying just this helper keep working unchanged.
    # `release-build` is the one other role: nightly-local.py names it for its build
    # step alone, so the CPU guard gives that tree's compiler its longer window
    # (cpu_guard.BUILD_WINDOW). It is registered exactly where "session" would be.
    if guard_role not in GUARD_ROLES:
        raise ValueError("unknown CPU guard role: %s" % guard_role)
    guard_path = os.path.join(os.path.dirname(__file__), "cpu_guard.py")
    client = None
    if verification:
        import cpu_guard
        client = cpu_guard.VerificationClient(verification)
    elif os.path.isfile(guard_path):
        import cpu_guard
        import pwd
        managed = cpu_guard.verification_enabled()
        inherited_owner = cpu_guard.inherited_verification_owner(os.environ.get('RICHOS_VERIFICATION_OWNER'))
        if managed and inherited_owner is None:
            print("proc_tree: REFUSED: managed verification requires a current proof-run plan "
                  "or its live registered ancestor; an owner environment hint is not admission.", file=sys.stderr)
            return 75
        # A PRIVATE config is a fixture and registers nothing. The canonical
        # ~/.claude named explicitly is production: on 2026-10-11 the second
        # review of af4b68db1 measured that an explicit CLAUDE_CONFIG_DIR=~/.claude
        # skipped registration, so package-app.sh compiled without its
        # release-build role. Same test as cpu_guard.hook(): unset, or resolving
        # to the real home's .claude.
        actual_home = pwd.getpwuid(os.getuid()).pw_dir
        config = os.environ.get("CLAUDE_CONFIG_DIR")
        private_config = bool(config) and (
            os.path.realpath(config) != os.path.realpath(os.path.join(actual_home, ".claude")))
        if (not managed and inherited_owner is None
                and os.path.realpath(os.path.expanduser("~")) == actual_home
                and not private_config and cpu_guard.healthy()):
            cpu_guard.register(os.getpid(), "managed " + os.path.basename(argv[0]), guard_role)
    owner_id = identity(owner)
    if owner_id is None:
        return 125
    scope = uuid.uuid4().hex
    inherited = os.environ.get(SCOPE_ENV, "")
    env = {**os.environ, SCOPE_ENV: (inherited + ":" if inherited else "") + scope}
    if client:
        env['RICHOS_VERIFICATION_OWNER'] = '%s:%s' % (client.pid, client.generation)
    interrupted = []
    def stop(signum, _frame):
        interrupted.append(signum)
    for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(sig, stop)
    read_fd, write_fd = os.pipe()
    bootstrap = ("import os,sys,signal; fd=int(sys.argv[1]); ready=os.read(fd,1); os.close(fd); "
                 "ready == b'1' or sys.exit(125); "
                 "argv=sys.argv[2:];\n"
                 "for name in ('SIGPIPE','SIGXFZ','SIGXFSZ'):\n"
                 " if hasattr(signal,name): signal.signal(getattr(signal,name),signal.SIG_DFL)\n"
                 "try: os.execvpe(argv[0],argv,os.environ)\n"
                 "except OSError as e: print('could not start: '+str(e),file=sys.stderr); sys.exit(127)")
    try:
        child = subprocess.Popen([*python_command(), "-c", bootstrap, str(read_fd), *argv],
                                 env=env, start_new_session=True, pass_fds=(read_fd,))
    except OSError as exc:
        print("could not start: %s" % exc, file=sys.stderr)
        return 127
    os.close(read_fd)
    tracker = None
    rc = 125
    # The check's own deadline counts ACTIVE time: a pause is not the check hanging.
    clock = HeldClock()
    started = clock.now()
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    try:
        tracker = TrackedTree(child.pid, scope)
        if client:
            client.start(child.pid)
        os.write(write_fd, b"1")
        os.close(write_fd)
        write_fd = None
        while child.poll() is None:
            sampled = time.monotonic()
            # Registered BEFORE the sample, so a fork by a member it already knows is not missed.
            watch = tree_watch(child.pid, owner, tracker.known)
            try:
                tracker.refresh()
                if client:
                    client.check_health()
                if interrupted or identity(owner) != owner_id:
                    rc = 128 + interrupted[0] if interrupted else 125
                    break
                if deadline is not None and clock.since(started) >= deadline:
                    if timeout_marker:
                        with open(timeout_marker, "x"):
                            pass
                    rc = 124
                    break
                if watch is None:
                    time.sleep(SAMPLE_SECONDS)
                    continue
                watch.add_pids(tracker.known, forks=True)   # and the members this sample found
                idle = IDLE_SAMPLE_SECONDS
                if deadline is not None:
                    idle = min(idle, max(0.0, deadline - clock.since(started)))
                watch.wait(idle)
            finally:
                if watch is not None:
                    watch.close()
            # Never sampled more often than before: a tree forking all the time costs what it did.
            time.sleep(max(0.0, sampled + SAMPLE_SECONDS - time.monotonic()))
        else:
            rc = child.returncode
            if release and client is None:
                try:
                    release_gradle_daemons(tracker, release)
                except Exception as exc:  # never fail the build: nothing released, all ended
                    print('proc_tree: Gradle daemon not kept warm: %s' % exc, file=sys.stderr)
    except BlockingIOError as exc:
        print('verification admission refused: %s' % exc, file=sys.stderr)
        rc = 75
    except (RuntimeError, ValueError, OSError) as exc:
        print('verification supervision failed: %s' % exc, file=sys.stderr)
        rc = 125
    finally:
        if write_fd is not None:
            os.close(write_fd)  # A child not yet enrolled exits without executing.
        left = finish_scope(child, tracker) if tracker else []
        if left:
            unproven = sorted(set(left) & set(tracker.unresolved))
            print("process cleanup failed; owned survivors: %s%s" % (
                left, "; generation unreadable, never signaled: %s" % unproven if unproven else ""),
                file=sys.stderr)
            rc = 125
        try:
            child.wait(timeout=1)
        except subprocess.TimeoutExpired:
            rc = 125
        if client:
            after = resource.getrusage(resource.RUSAGE_CHILDREN)
            client.finish(rc if rc >= 0 else 128 - rc, left,
                          after.ru_utime + after.ru_stime - before.ru_utime - before.ru_stime)
    return rc if rc >= 0 else 128 - rc


def main(argv):
    if len(argv) >= 4 and argv[0] == "run" and "--" in argv:
        import argparse
        import math
        parser = argparse.ArgumentParser(prog="proc_tree.py run")
        parser.add_argument("owner", type=int)
        parser.add_argument("--deadline", type=float)
        parser.add_argument("--timeout-marker")
        parser.add_argument("--verification")
        parser.add_argument("--guard-role", choices=GUARD_ROLES, default="session")
        parser.add_argument("--release-gradle-daemons", metavar="REGISTRY")
        split = argv.index("--")
        args = parser.parse_args(argv[1:split])
        if args.deadline is not None and (not math.isfinite(args.deadline) or args.deadline <= 0):
            parser.error("deadline must be finite and positive")
        try:
            return supervise(args.owner, argv[split + 1:], args.deadline, args.timeout_marker, args.verification,
                             args.guard_role, release=args.release_gradle_daemons)
        except (RuntimeError, ValueError, OSError) as exc:
            print('process supervision unavailable: %s' % exc, file=sys.stderr)
            return 125
    if len(argv) >= 2 and argv[0] in ("kill", "members") and argv[1].isdigit():
        root = int(argv[1])
        if argv[0] == "members":
            pids, _groups = members(root)
            print("\n".join(str(p) for p in sorted(pids)))
            return 0
        grace = 3.0
        if "--grace" in argv:
            grace = float(argv[argv.index("--grace") + 1])
        left = kill_tree(root, grace)
        if left:
            print("proc_tree.py: still alive after SIGKILL: %s" % " ".join(str(p) for p in left), file=sys.stderr)
            return 1
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

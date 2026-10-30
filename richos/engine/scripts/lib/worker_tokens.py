#!/usr/bin/env python3
"""worker_tokens.py — one budget of concurrent workers for a whole run, nested workers included.

    worker_tokens.py init <dir> <n>          make a budget of exactly n tokens in <dir>
    worker_tokens.py run <dir> -- <cmd...>   wait for a free token, run cmd holding it, exit with
                                             cmd's status; the token is released when this
                                             process ends, however it ends (an flock the kernel
                                             drops, never a file somebody must remember to delete)
    worker_tokens.py run <dir> --free <lock> -- <cmd...>
                                             the same, but <lock> (the caller's own free slot) is
                                             tried first and taken instead of a budget token
    worker_tokens.py held <dir>              how many tokens are held right now

    import: Budget(dir, reserved=R).try_acquire() -> token or None; .acquire(free=...) waits;
            token.release(); .held(); .waiting()

WHY (2026-09-23). proof-run.py limited how many CHECKS ran at once, and a check is not one worker:
an engine shard reaching workspace-spec-fourteen starts a mutation pool of eight suites of its own,
so "ten checks at once" could be eighty processes. The runner now holds one token per check it
starts, and exports RICHOS_WORKER_TOKENS=<dir>; a nested pool (scripts/lib/mutation-pool.sh) runs
one worker at a time on its caller's token (the pool's free slot, whichever worker holds it now)
and takes a token of this budget for every other worker. So the total is bounded by the budget
however deep the nesting, and nothing can deadlock: every check can always make progress one
worker at a time on the token it already holds.

A RESERVE FOR CHECKS NOT YET STARTED. With the whole budget open to nested workers, four mutation
pools wanting eight workers each took every token the moment one was freed, and the runner could
not start a single further check until every pool had drained: in the third full run
native-ios-share waited 2519 s to start. So the last RICHOS_WORKER_TOKENS_RESERVED tokens (set by
the runner) are never taken by a nested worker; only the runner starts checks on them, and only
once the shared ones are all held, so a light check can always start and finish beside the pools.

THE CONTRACT FOR A NESTED WORKER: it passes `--free <a lock of its caller's own>`. A check holds
its token for as long as it runs, so once running checks hold every shared token, a worker that
can only wait for a budget token waits for a check that is waiting for it. The free slot is what
keeps every check moving (proof-run.test.py P12 deadlocked without it; mutation-harness.sh and
native-ios-ui.test.sh both pass one).

Tokens are files locked with flock(2): a crashed or killed holder releases its token with its
last file descriptor. No token is ever handed out by name or reclaimed by guessing.

Each run also acquires from the per-user machine budget. Independent checkouts and nightlies
share that ceiling. A local --capacity can lower a run's limit without resizing shared lock files.
"""
import fcntl
import json
import signal
import os
import re
import subprocess
import sys
import time

# A PERMIT WAIT BLOCKS ON WHAT FREES A PERMIT (2026-10-28). It used to look every 0.2 s for as
# long as it waited (up to 30 min): about 0.55 s of CPU a minute per waiter, measured, in every
# queued unit at once. Now it blocks (quiet_wait.py) until a permit's recorded holder exits or
# the budget directory changes (a waiter ahead leaves the queue), and looks then, never more
# often than every POLL_SECONDS (the admission queue's oldest waiter takes a freed permit at
# once; the rest defer to it) and at least every PERMIT_RECHECK_SECONDS (a permit released
# without its holder exiting, or the CPU admission clearing, has no event).
POLL_SECONDS = 0.2
PERMIT_RECHECK_SECONDS = 1.0


def _permit_watch(budget, free=None):
    """A quiet_wait.Watch on the exits of every permit's recorded holder and on the budget
    directories, or None where quiet_wait.py is not beside this file (it then looks every
    POLL_SECONDS, as before). Registered before the caller's look, so a release between is seen."""
    try:
        from quiet_wait import Watch
    except ImportError:
        return None
    paths, pids, files = [], [], [free] if free else []
    while budget is not None:
        paths.append(budget.dir)
        files.extend(budget.usable)
        budget = budget.shared
    for path in files:
        try:
            with open(path) as f:
                holder = (json.loads(f.read() or "{}") or {}).get("holder")
        except (OSError, ValueError, AttributeError):
            continue
        if isinstance(holder, list) and holder and isinstance(holder[0], int):
            pids.append(holder[0])
    return Watch(paths=paths, pids=pids)


def _wait_for_permit(watch, looked):
    """Block on `watch` (from _permit_watch, made before the look at `looked`) until a permit
    may be free, then keep the old minimum gap between looks."""
    if watch is None:
        time.sleep(POLL_SECONDS)
        return
    watch.wait(PERMIT_RECHECK_SECONDS)
    time.sleep(max(0.0, looked + POLL_SECONDS - time.monotonic()))


class Token:
    def __init__(self, fd, path):
        self.fd, self.path = fd, path
        self.extra = None
        self.parent = None

    def publish(self, beneficiary=None):
        """Bind this locked inode to its live holder and the worker it serves."""
        import proc_tree
        beneficiary = os.getpid() if beneficiary is None else int(beneficiary)
        holder = proc_tree.identity(os.getpid())
        generation = proc_tree.identity(beneficiary)
        if holder is None or generation is None or native_ancestry().get(beneficiary) != generation:
            raise ValueError('worker permit has no live native owner')
        if self.extra:
            self.extra.publish(beneficiary)
        row = {'schema': 1, 'holder': [os.getpid(), holder],
               'beneficiary': [beneficiary, generation],
               'parent': self.extra.path if self.extra else self.parent}
        os.lseek(self.fd, 0, os.SEEK_SET)
        os.ftruncate(self.fd, 0)
        os.write(self.fd, json.dumps(row).encode())

    @property
    def fds(self):
        return ([self.fd] if self.fd is not None else []) + (self.extra.fds if self.extra else [])

    def release(self):
        if self.extra:
            self.extra.release()
        if self.fd is not None:
            try:
                fcntl.flock(self.fd, fcntl.LOCK_UN)
            finally:
                os.close(self.fd)
                self.fd = None


class Budget:
    def __init__(self, directory, reserved=0, runner=False, shared=None, resource=None):
        """`reserved` tokens at the end of the budget are the runner's alone. `runner=True` is the
        runner itself: every token is open to it, the shared ones first, so the reserved ones
        are left for the moments nested workers hold all the rest."""
        self.dir = directory
        self.resource = resource
        if resource is not None:
            if shared is not False or not re.fullmatch(
                    r'native-build|simulator-(?:live|boot|cache-[0-9a-f]{64})', resource):
                raise ValueError('unknown resource lock or shared worker budget on a resource lock')
            import cpu_guard
            cpu_guard.governed_directory(
                directory, cpu_guard.CANONICAL_WORKERS.parent / (resource + '-v1'))
        shared = (shared or os.environ.get("RICHOS_MACHINE_WORKERS")) if shared is not False else None
        self.shared = (Budget(shared, reserved, runner, shared=shared)
                       if shared and os.path.realpath(shared) != os.path.realpath(directory) else None)
        if not self.shared and not resource and os.path.isfile(os.path.join(os.path.dirname(__file__), 'cpu_guard.py')):
            import cpu_guard
            cpu_guard.governed_directory(directory, cpu_guard.CANONICAL_WORKERS)
        self.files = sorted(os.path.join(directory, f) for f in os.listdir(directory) if f.startswith("token-") and f[6:].isdigit())
        if not self.files:
            raise ValueError("no tokens in %s; make them with `worker_tokens.py init`" % directory)
        reserved = max(0, min(int(reserved), len(self.files) - 1))
        if runner:
            self.usable = list(self.files)
        else:
            self.usable = self.files[:len(self.files) - reserved]

    def try_acquire(self):
        # These existing serialization/device locks constrain already admitted
        # work. They neither grant worker capacity nor enter the worker queue.
        if self.resource:
            return self._try_acquire()
        # Only the final machine budget gates priority; recursively acquiring a
        # local and shared budget must not deadlock against our own guard.
        if self.shared:
            return self._try_acquire()
        self.refusal = None
        if os.path.isfile(os.path.join(os.path.dirname(__file__), 'cpu_guard.py')):
            import cpu_guard
            self.refusal = cpu_guard.verification_admission()
            if self.refusal:
                return None
        import engine_pass
        if not hasattr(self, "admission"):
            self.admission = engine_pass.Admission(
                self.dir, os.environ.get("RICHOS_VERIFICATION_CHECKOUT", os.getcwd()),
                permits=[os.path.basename(path) for path in self.usable])
        if not self.admission.begin():
            return None
        token = None
        try:
            token = self._try_acquire()
            return token
        finally:
            self.admission.attempted(token is not None)

    def close(self):
        if self.shared:
            self.shared.close()
        if hasattr(self, "admission"):
            self.admission.close()

    def _try_acquire(self):
        for path in self.usable:
            fd = os.open(path, os.O_RDWR)
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                os.close(fd)
                continue
            token = Token(fd, path)
            token.resource = self.resource
            if self.shared:
                token.extra = self.shared.try_acquire()
                if token.extra is None:
                    token.release()
                    continue
            try:
                token.publish()
            except BaseException:
                token.release()
                raise
            return token
        return None

    def acquire(self, free=None, timeout=1800):
        """Wait for a token. With `free` (a lock file of the caller's own), that lock is tried
        first and taken instead of a budget token when it is free: it is the ONE worker a nested
        pool runs on its caller's token, held by whichever of its workers holds the lock now —
        not by the first worker ever submitted, which starved every later worker of the pool
        in the second full run (2026-09-23: workspaces.test.sh at one worker for 40 minutes).
        While waiting, a `wait-<pid>` marker says so (Budget.waiting, for the run's report)."""
        if self.resource and free:
            raise ValueError('a resource lock cannot borrow a worker permit')
        if free:
            free = os.environ.get("RICHOS_WORKER_BORROW_LOCK", free)
        t = self._try_free(free) or self.try_acquire()
        if t:
            return t
        import proc_tree
        # Active time: a waiter suspended by a pause has not waited (Sage's catch 2).
        clock = proc_tree.HeldClock()
        deadline = clock.now() + timeout
        marker = os.path.join(self.dir, "wait-%d" % os.getpid())
        open(marker, "w").close()
        try:
            while True:
                watch, looked = _permit_watch(self, free), time.monotonic()
                try:
                    t = self._try_free(free) or self.try_acquire()
                    if t:
                        return t
                    if clock.now() >= deadline:
                        raise TimeoutError("worker admission timed out after %gs: %s" % (timeout, self.dir))
                    _wait_for_permit(watch, looked)
                finally:
                    if watch is not None:
                        watch.close()
        finally:
            self.close()
            try:
                os.remove(marker)
            except OSError:
                pass

    @staticmethod
    def _try_free(path):
        if not path:
            return None
        if managed_policy():
            validate_borrow(path)
        fd = os.open(path, os.O_RDWR | os.O_CREAT, 0o600)
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            os.close(fd)
            return None
        token = Token(fd, path)
        token.parent = path.removesuffix('.child')
        try:
            token.publish()
        except BaseException:
            token.release()
            raise
        return token

    def held(self):
        n = 0
        for path in self.files:
            fd = os.open(path, os.O_RDWR)
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                fcntl.flock(fd, fcntl.LOCK_UN)
            except BlockingIOError:
                n += 1
            finally:
                os.close(fd)
        return n

    def waiting(self):
        """How many nested workers are waiting for a token right now (markers of live pids)."""
        n = 0
        for f in os.listdir(self.dir):
            if f.startswith("wait-") and f[5:].isdigit():
                try:
                    os.kill(int(f[5:]), 0)
                    n += 1
                except ProcessLookupError:
                    try:
                        os.remove(os.path.join(self.dir, f))
                    except OSError:
                        pass
                except PermissionError:
                    n += 1
        return n


def managed_policy():
    if not os.path.isfile(os.path.join(os.path.dirname(__file__), 'cpu_guard.py')):
        return False
    import cpu_guard
    return cpu_guard.verification_enabled()


def inherited_admission():
    """Validate the existing launcher shortcut before skipping its wrapper."""
    if not (os.environ.get('RICHOS_WORKER_TOKENS')
            and os.environ.get('RICHOS_WORKER_SLOT_HELD') == '1'):
        return False
    if not managed_policy():
        return True
    import cpu_guard
    if not cpu_guard.inherited_verification_owner(os.environ.get('RICHOS_VERIFICATION_OWNER')):
        return False
    validate_borrow(os.environ.get('RICHOS_WORKER_BORROW_LOCK'))
    return True


def native_ancestry():
    import operator_fences
    current = operator_fences.proc(os.getpid(), precise=True)
    result = {}
    while current and not current['zombie'] and current['pid'] not in result:
        result[current['pid']] = current['start']
        current = operator_fences.proc(current['ppid'], precise=True)
    return result


def validate_borrow(path):
    """An environment hint must resolve to a live, locked machine permit.

    Local budgets and nested free slots form a short chain of held inodes. The
    beneficiary must be this process or its native ancestor. A shell pool's
    separate lease keeper can serve its parent, without pretending to be its
    ancestor. A released, replaced or stale inode never authorizes borrowing.
    """
    import cpu_guard
    import proc_tree
    machine = os.environ.get('RICHOS_MACHINE_WORKERS') or str(cpu_guard.CANONICAL_WORKERS)
    machine = os.path.realpath(cpu_guard.governed_directory(machine, cpu_guard.CANONICAL_WORKERS))
    if not path or not path.endswith('.child'):
        raise ValueError('worker borrow requires the held permit child path')
    ancestors = native_ancestry()
    cursor, seen = path[:-len('.child')], set()
    while cursor and len(seen) < 64:
        canonical = os.path.realpath(cursor)
        if canonical in seen:
            break
        seen.add(canonical)
        fd = os.open(cursor, os.O_RDONLY)
        try:
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                pass
            else:
                fcntl.flock(fd, fcntl.LOCK_UN)
                raise ValueError('worker borrow parent is not held: ' + cursor)
            row = json.loads(os.read(fd, 65536))
            holder, beneficiary = row['holder'], row['beneficiary']
            if (row.get('schema') != 1 or ancestors.get(beneficiary[0]) != beneficiary[1]
                    or proc_tree.identity(beneficiary[0]) != beneficiary[1]
                    or proc_tree.identity(holder[0]) != holder[1]):
                raise ValueError('worker borrow has no live native ancestor: ' + cursor)
            # Keep the observation tied to the inode whose lock was inspected.
            actual, expected = os.fstat(fd), os.stat(cursor)
            if (actual.st_dev, actual.st_ino) != (expected.st_dev, expected.st_ino):
                raise ValueError('worker borrow parent inode changed')
            parent = row.get('parent')
            if parent is None:
                if (os.path.dirname(canonical) != machine
                        or not re.fullmatch(r'token-\d+', os.path.basename(canonical))):
                    raise ValueError('worker borrow does not reach the shared machine budget')
                return
            cursor = parent
        except (KeyError, TypeError, IndexError, json.JSONDecodeError) as exc:
            raise ValueError('invalid worker permit ownership: ' + cursor) from exc
        finally:
            os.close(fd)
    raise ValueError('invalid worker permit ownership chain')


def init(directory, n):
    """Create an immutable budget under an initialization lock. Never unlink a live lease."""
    n = int(n)
    if n < 1:
        raise ValueError("a budget needs at least one token")
    os.makedirs(directory, mode=0o700, exist_ok=True)
    with open(os.path.join(directory, "init.lock"), "a") as guard:
        fcntl.flock(guard, fcntl.LOCK_EX)
        config = os.path.join(directory, "capacity.json")
        if os.path.exists(config):
            with open(config) as existing:
                old = json.load(existing)
            if old != n:
                raise ValueError("budget capacity is immutable (%s, requested %s): %s" % (old, n, directory))
        for i in range(n):
            open(os.path.join(directory, "token-%03d" % i), "a").close()
        with open(config, "w") as out:
            json.dump(n, out)


def machine_directory():
    """One per-user budget across checkouts, runners and nightlies; never release.lock."""
    directory = os.environ.get("RICHOS_MACHINE_WORKERS") or os.path.join(
        os.path.expanduser("~"), ".richos-nightly", "worker-budget-v1")
    if os.path.isfile(os.path.join(os.path.dirname(__file__), 'cpu_guard.py')):
        import cpu_guard
        if cpu_guard.installed_verification_policy():
            directory = os.environ.get('RICHOS_MACHINE_WORKERS') or str(cpu_guard.CANONICAL_WORKERS)
            directory = str(cpu_guard.governed_directory(directory, cpu_guard.CANONICAL_WORKERS))
    # The machine ceiling is independent of a run's smaller --capacity.
    init(directory, max(1, int((os.cpu_count() or 4) * 0.8)))
    return directory


def run_command(cmd, token, env=None, worker=True, release=None):
    # The supervisor inherits the lease. If this wrapper is killed, admission cannot
    # reuse that slot until the supervisor has stopped the command's descendants.
    # `release`: a workspace's private Gradle daemon registry; see proc_tree.release_gradle_daemons.
    import proc_tree
    env = dict(env if env is not None else os.environ)
    env.setdefault("RICHOS_VERIFICATION_CHECKOUT", os.getcwd())
    if worker:
        env.update(RICHOS_WORKER_SLOT_HELD="1", RICHOS_WORKER_BORROW_LOCK=token.path + ".child")
    try:
        if worker and getattr(token, 'resource', None):
            raise ValueError('a resource lock cannot authorize a worker')
        inherited = env.pop("RICHOS_ENGINE_PASS_FD", None)
        lease_fds = list(token.fds)
        if inherited is not None:
            import engine_pass
            fd = int(inherited)
            actual = os.fstat(fd)
            expected = os.stat(os.path.join(engine_pass.directory(), "slot.lock"))
            if ((actual.st_dev, actual.st_ino) != (expected.st_dev, expected.st_ino) or
                    not engine_pass.held_by_ancestor()):
                raise ValueError("invalid inherited engine-slot descriptor")
            # The supervisor retains the slot through owned cleanup even if this
            # wrapper dies. Its child does not inherit an unusable FD hint.
            lease_fds.append(fd)
        child = subprocess.Popen(proc_tree.command(cmd, owner=os.getpid(), release=release), env=env,
                                 pass_fds=tuple(lease_fds), start_new_session=True)
        def stop(signum, _frame):
            child.send_signal(signum)
        previous = {sig: signal.signal(sig, stop) for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP)}
        try:
            rc = child.wait()
        finally:
            for sig, handler in previous.items():
                signal.signal(sig, handler)
        return rc if rc >= 0 else 128 - rc
    finally:
        token.release()


def lease_worker(directory, owner, ready, release, free):
    """Keep a bash 3.2 pool worker's lease until it finishes or its descendants are cleaned."""
    import proc_tree
    import uuid
    owner = int(owner)
    born = proc_tree.identity(owner)
    if born is None:
        return 125
    scope = uuid.uuid4().hex
    tracker = proc_tree.TrackedTree(owner, scope)
    budget = Budget(directory, int(os.environ.get("RICHOS_WORKER_TOKENS_RESERVED", "0")))
    interrupted = []
    for sig in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
        signal.signal(sig, lambda number, _frame: interrupted.append(number))
    if free != "-":
        free = os.environ.get("RICHOS_WORKER_BORROW_LOCK", free)
    token = None
    clock = proc_tree.HeldClock()      # a pause is not time spent waiting (Sage's catch 2)
    deadline = clock.now() + 1800
    while token is None:
        if interrupted or proc_tree.identity(owner) != born or clock.now() >= deadline:
            return 125
        watch, looked = _permit_watch(budget, free if free != "-" else None), time.monotonic()
        try:
            if watch is not None:
                watch.add_pids([owner])            # the pool worker's end ends this wait at once
                watch.add_signals((signal.SIGTERM, signal.SIGINT, signal.SIGHUP))
            token = budget._try_free(free if free != "-" else None) or budget.try_acquire()
            if token is None:
                _wait_for_permit(watch, looked)
        finally:
            if watch is not None:
                watch.close()
    try:
        token.publish(owner)
        with open(ready + ".borrow", "w") as out:
            out.write(token.path + ".child")
        with open(ready + ".new", "w") as out:
            out.write(scope)
        os.replace(ready + ".new", ready)
        while not os.path.exists(release) and not interrupted and proc_tree.identity(owner) == born:
            # 2026-10-28: blocks until the release file appears, the worker or a member of its tree
            # forks or exits, or a signal arrives (proc_tree.tree_watch), then samples, at most every
            # POLL_SECONDS as before; it used to sample the tree every 0.2 s for the worker's whole run.
            sampled = time.monotonic()
            watch = proc_tree.tree_watch(owner, owner, tracker.known)
            try:
                if watch is not None:
                    watch.add_paths([os.path.dirname(os.path.abspath(release))])
                tracker.refresh()
                if watch is None:
                    time.sleep(POLL_SECONDS)
                    continue
                if os.path.exists(release):
                    break
                watch.add_pids(tracker.known, forks=True)
                watch.wait(proc_tree.IDLE_SAMPLE_SECONDS)
            finally:
                if watch is not None:
                    watch.close()
            time.sleep(max(0.0, sampled + POLL_SECONDS - time.monotonic()))
        # The worker waits for us on normal completion. Exclude it from cleanup while
        # retaining its children, including children reparented after an abrupt death.
        class Owner:
            def poll(self):
                return None
        original = tracker.refresh
        def remaining(tags=False):
            return original(tags) - {owner}
        tracker.refresh = remaining
        left = proc_tree.finish_scope(Owner(), tracker)
        return 125 if left else 0
    finally:
        token.release()


def execution_outcome(rc):
    marker = os.environ.get("RICHOS_UNIT_DEADLINE_MARKER")
    if rc == 124 and (marker is None or (marker and os.path.isfile(marker))):
        return "timed-out"
    return "completed" if rc < 125 else "infrastructure-error"


def machine_command(command, timing=None):
    start = time.monotonic()
    acquired = None
    rc = 125
    outcome = "infrastructure-error"
    budget = None
    try:
        inherited = bool(os.environ.get("RICHOS_WORKER_TOKENS")
                         and os.environ.get("RICHOS_WORKER_SLOT_HELD") == "1")
        managed = managed_policy()
        if managed:
            import cpu_guard
            if not cpu_guard.inherited_verification_owner(os.environ.get('RICHOS_VERIFICATION_OWNER')):
                rc, outcome = 75, 'not-admitted'
                print('worker_tokens: REFUSED before resource wait: use the current proof-run plan '
                      'or its live admitted ancestor', file=sys.stderr)
                return rc
        if inherited and not managed:
            if timing is None:
                import proc_tree
                os.execv(sys.executable, proc_tree.command(command, owner=os.getppid()))
            acquired = time.monotonic()
            rc = run_command(command, Token(None, ""), worker=False)
            outcome = execution_outcome(rc)
            return rc
        directory = machine_directory()
        budget = Budget(directory, runner=True)
        free = os.environ.get('RICHOS_WORKER_BORROW_LOCK') if inherited else None
        if inherited:
            validate_borrow(free)
        token = budget.acquire(free=free)
        acquired = time.monotonic()
        env = {**os.environ, "RICHOS_MACHINE_WORKERS": directory,
               "RICHOS_WORKER_TOKENS": directory,
               "RICHOS_WORKER_TOKENS_TOOL": os.path.abspath(__file__),
               "RICHOS_WORKER_TOKENS_RESERVED": str(max(1, len(budget.files) // 4))}
        import uuid
        env["RICHOS_TEST_DEVICE_RUN_ID"] = uuid.uuid4().hex
        try:
            rc = run_command(command, token, env)
        finally:
            # The complete engine owns test-device cleanup. Minimal test fixtures
            # copy only the process/budget helpers and never create real devices.
            if os.path.isfile(os.path.join(os.path.dirname(__file__), "testdevices.py")):
                import testdevices
                errors = testdevices.cleanup_run_simulators(env["RICHOS_TEST_DEVICE_RUN_ID"])
                if errors:
                    print("worker_tokens: simulator cleanup FAILED: " + "; ".join(errors), file=sys.stderr)
                    rc = 125
        outcome = execution_outcome(rc)
        return rc
    except (TimeoutError, OSError, ValueError) as exc:
        refused = isinstance(exc, TimeoutError) and acquired is None
        outcome = "not-admitted" if refused else "infrastructure-error"
        rc = 75 if refused else 125
        print("worker_tokens: %s: %s" % (outcome, exc), file=sys.stderr)
        return rc
    finally:
        if budget:
            budget.close()
        if timing is not None:
            end = time.monotonic()
            with open(timing + ".new", "w") as out:
                json.dump({"admission_seconds": (acquired or end) - start,
                           "execution_seconds": end - acquired if acquired else 0,
                           "exit": rc, "admitted": acquired is not None,
                           "outcome": outcome}, out)
            os.replace(timing + ".new", timing)


def main(argv):
    if argv == ['inherited-admission']:
        return 0 if inherited_admission() else 1
    if len(argv) >= 5 and argv[:2] == ["machine", "--timing"] and argv[3] == "--":
        return machine_command(argv[4:], timing=argv[2])
    if argv == ["directory"]:
        print(machine_directory())
        return 0
    if len(argv) == 6 and argv[0] == "lease":
        return lease_worker(*argv[1:])
    if len(argv) >= 3 and argv[:2] == ["machine", "--"]:
        return machine_command(argv[2:])
    reserved = os.environ.get("RICHOS_WORKER_TOKENS_RESERVED", "0")
    reserved = int(reserved) if reserved.isdigit() else 0
    if len(argv) == 3 and argv[0] == "init":
        init(argv[1], argv[2])
        return 0
    if len(argv) == 2 and argv[0] == "held":
        print(Budget(argv[1]).held())
        return 0
    free = None
    if len(argv) >= 6 and argv[0] == "run" and argv[2] == "--free" and argv[4] == "--":
        free, cmd = argv[3], argv[5:]
    elif len(argv) >= 4 and argv[0] == "run" and argv[2] == "--":
        cmd = argv[3:]
    else:
        print(__doc__, file=sys.stderr)
        return 2
    token = Budget(argv[1], reserved).acquire(free=free)
    return run_command(cmd, token)



if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

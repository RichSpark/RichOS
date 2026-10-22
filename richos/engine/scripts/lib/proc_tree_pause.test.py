#!/usr/bin/env python3
"""A pause never ends the check a supervisor runs (Sage's catch 2, 2026-09-27).

A pause suspends the supervisor together with the check it runs (agent_hold.py),
and RESUME continues both. Two things inside the supervisor used to turn that
into a kill at RESUME: its check deadline counted the pause, and a `ps` it was
waiting on when the pause landed "timed out" by the clock. Both cases below
suspend a real supervisor tree for longer than those bounds and require the
check to finish with its own exit status. Only processes this test started are
ever signaled; cpu_guard's state is a private fixture root.
"""
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

WORK = r'''
import sys, time
out = sys.argv[1]
for i in range(int(sys.argv[2])):
    time.sleep(0.1)
    open(out, "w").write(str(i + 1))
'''

# The supervisor on a slow host: each ps takes 0.3 s more and is allowed 0.8 s,
# so most of its loop is spent waiting on ps and a pause of 1.2 s that lands
# there outlasts the timeout. It changes the numbers, never how proc_tree calls ps.
SHORT_PS = r'''
import subprocess, sys
sys.path.insert(0, sys.argv[1])
real, short = subprocess.run, float(sys.argv[2])
def run(argv, *a, **kw):
    if kw.get("timeout") is not None and argv and argv[0] == "ps":
        kw["timeout"] = short
        argv = ["/bin/sh", "-c", 'sleep 0.3; exec ps "$@"', "ps", *argv[1:]]
    return real(argv, *a, **kw)
subprocess.run = run
import proc_tree
proc_tree.PS_TIMEOUT = short          # the same number for its own arithmetic
sys.exit(proc_tree.main(sys.argv[3:]))
'''


def descendants(root):
    rows = subprocess.run(["ps", "-A", "-o", "pid=,ppid="], capture_output=True, text=True).stdout.split("\n")
    kids = {}
    for line in rows:
        f = line.split()
        if len(f) == 2:
            kids.setdefault(int(f[1]), []).append(int(f[0]))
    out, stack = [], [root]
    while stack:
        p = stack.pop()
        out.append(p)
        stack.extend(kids.get(p, []))
    return out


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="proc-tree-pause.")
        home = os.path.join(self.tmp, "home")
        os.makedirs(os.path.join(home, ".claude"))
        self.env = dict(os.environ, HOME=home, CLAUDE_CONFIG_DIR=os.path.join(home, ".claude"),
                        RICHOS_CPU_GUARD_STATE=os.path.join(self.tmp, "cpu-guard"),
                        RICHOS_VERIFICATION_MODE="fixture", RICHOS_VERIFICATION_FIXTURE_ROOT=self.tmp,
                        PYTHONDONTWRITEBYTECODE="1")
        self.work = os.path.join(self.tmp, "work.py")
        Path(self.work).write_text(WORK)
        self.started = []

    def tearDown(self):
        for p in self.started:
            if p.poll() is None:
                for pid in descendants(p.pid):
                    for sig in (signal.SIGCONT, signal.SIGKILL):
                        try:
                            os.kill(pid, sig)
                        except (ProcessLookupError, PermissionError):
                            pass
                p.wait()
        shutil.rmtree(self.tmp, ignore_errors=True)

    def supervise(self, argv, steps, out):
        p = subprocess.Popen(argv + ["--", sys.executable, self.work, out, str(steps)], env=self.env,
                             stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
        self.started.append(p)
        deadline = time.monotonic() + 20
        while not os.path.exists(out) and time.monotonic() < deadline:
            time.sleep(0.02)
        self.assertTrue(os.path.exists(out), "the check started")
        return p

    def freeze(self, p, seconds):
        """The whole tree, supervisor first, as a hold does; then continue it."""
        tree = descendants(p.pid)
        for pid in tree:
            os.kill(pid, signal.SIGSTOP)
        time.sleep(seconds)
        for pid in tree:
            try:
                os.kill(pid, signal.SIGCONT)
            except ProcessLookupError:
                pass


class Fake:
    def __init__(self):
        self.t = 100.0

    def __call__(self):
        return self.t


class Clock(unittest.TestCase):
    def test_small_steps_all_count(self):
        import proc_tree
        f = Fake()
        c = proc_tree.HeldClock(gap=5, clock=f)
        start = c.now()
        for _ in range(50):
            f.t += 0.2
            c.now()
        self.assertAlmostEqual(c.since(start), 10.0)
        self.assertEqual(c.gaps, 0)

    def test_a_gap_counts_only_its_first_seconds(self):
        import proc_tree
        f = Fake()
        c = proc_tree.HeldClock(gap=5, clock=f)
        start = c.now()
        f.t += 300.0            # suspended for five minutes
        self.assertAlmostEqual(c.since(start), 5.0)
        f.t += 0.2
        self.assertAlmostEqual(c.since(start), 5.2)
        self.assertEqual((c.gaps, round(c.held, 6)), (1, 295.0))

    def test_real_work_still_reaches_the_deadline(self):
        import proc_tree
        f = Fake()
        c = proc_tree.HeldClock(gap=5, clock=f)
        deadline = c.now() + 60
        f.t += 50
        self.assertLess(c.now(), deadline, "a 50 s gap is a pause, not 50 s of work")
        for _ in range(300):
            f.t += 0.2
            c.now()
        self.assertGreaterEqual(c.now(), deadline)


class Unit(unittest.TestCase):
    def test_a_ps_that_timed_out_while_suspended_is_asked_again(self):
        import proc_tree
        real = subprocess.run
        calls = []

        def once_then_real(argv, **kw):
            calls.append(argv)
            if len(calls) == 1:
                raise subprocess.TimeoutExpired(argv, kw.get("timeout"))
            return real(argv, **kw)

        # process_rows reads the table from the kernel since 2026-10-22; the `ps` that is still
        # started (a status read of owned PIDs, _alive) keeps this rule.
        with mock.patch.object(proc_tree.subprocess, "run", once_then_real):
            alive = proc_tree._alive({os.getpid()})
        self.assertEqual(alive, [os.getpid()])
        self.assertEqual(calls[0], calls[1], "the same ps is asked again")

    def test_the_kernel_table_is_the_table_ps_prints(self):
        """2026-10-22: a supervisor tick reads the process table in-process, with no `ps`
        started, and gets the rows `ps -ax -o pid=,ppid=,pgid=` prints."""
        import proc_tree
        started = []
        real = subprocess.run

        def counted(argv, **kw):
            started.append(argv)
            return real(argv, **kw)

        with mock.patch.object(proc_tree.subprocess, "run", counted):
            rows = proc_tree.process_rows()
        if sys.platform == "darwin":
            # Unseeded, root's unreadable processes still get the one status read (_alive).
            self.assertEqual([a for a in started if a[:2] == ["ps", "-ax"]], [], "no ps for the table")
        self.assertEqual(rows[os.getpid()][:2], (os.getppid(), os.getpgrp()))
        child = subprocess.Popen(["sleep", "30"], start_new_session=True)
        try:
            started.clear()
            with mock.patch.object(proc_tree.subprocess, "run", counted):
                table = proc_tree.process_rows(seeds={child.pid})
            if sys.platform == "darwin":
                self.assertEqual(started, [], "a supervisor's tick starts no process")
            self.assertEqual(table[child.pid][:2], (os.getpid(), child.pid))
            self.assertEqual(table[child.pid][2], proc_tree.identity(child.pid), "its birth is read natively")
            listed = real(["ps", "-ax", "-o", "pid=,ppid=,pgid="], capture_output=True, text=True).stdout
            shown = {int(f[0]): (int(f[1]), int(f[2])) for f in (line.split() for line in listed.splitlines())
                     if len(f) == 3}
            for pid in (os.getpid(), child.pid, 1):
                self.assertEqual(rows.get(pid, table.get(pid))[:2], shown[pid], pid)
            self.assertNotIn(0, table, "ps -ax never lists pid 0")
        finally:
            child.kill()
            child.wait()

    def test_a_ps_that_always_hangs_still_fails(self):
        import proc_tree

        def hang(argv, **kw):
            raise subprocess.TimeoutExpired(argv, kw.get("timeout"))

        with mock.patch.object(proc_tree.subprocess, "run", hang):
            with self.assertRaises(subprocess.TimeoutExpired):
                proc_tree._alive({os.getpid()})


class RealSupervisor(Base):
    def test_check_deadline_does_not_count_the_pause(self):
        out = os.path.join(self.tmp, "deadline.out")
        # 40 steps of 0.1 s under a 12 s deadline, suspended for 20 s: without the fix, rc 124.
        p = self.supervise([sys.executable, str(HERE / "proc_tree.py"), "run", str(os.getpid()),
                            "--deadline", "12", "--timeout-marker", os.path.join(self.tmp, "marker")], 40, out)
        time.sleep(0.5)
        self.freeze(p, 20.0)
        text, _ = p.communicate(timeout=60)
        self.assertEqual(p.returncode, 0, text)
        self.assertEqual(Path(out).read_text(), "40", "the check ran to its end")
        self.assertFalse(os.path.exists(os.path.join(self.tmp, "marker")), "no timeout was recorded")

    def test_repeated_pauses_during_ps_never_end_the_check(self):
        out = os.path.join(self.tmp, "ps.out")
        script = os.path.join(self.tmp, "short_ps.py")
        Path(script).write_text(SHORT_PS)
        # ps is allowed 0.8 s, and each pause lasts 1.2 s: any pause that lands while the
        # supervisor waits on a ps outlasts that timeout.
        p = self.supervise([sys.executable, script, str(HERE), "0.8", "run", str(os.getpid())], 300, out)
        for _ in range(20):
            self.freeze(p, 1.2)
            time.sleep(0.15)
            if p.poll() is not None:
                self.fail("the supervisor ended after a pause (rc %s): %s" % (p.returncode, p.communicate()[0][-3000:]))
        text, _ = p.communicate(timeout=120)
        self.assertEqual(p.returncode, 0, text)
        self.assertEqual(Path(out).read_text(), "300")


if __name__ == "__main__":
    unittest.main(verbosity=2)

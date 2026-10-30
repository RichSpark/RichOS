#!/usr/bin/env python3
"""quiet_wait.Watch wakes on the event it watches, at once, and otherwise sleeps to its bound.

Each case starts the event a fixed delay after the wait begins and checks the wait ended
soon after it (not at the bound), or, with nothing happening, that it slept its whole
bound without spending CPU. The 60 s CPU bound of every waiter built on it is
wait-cpu.test.py's; these are the mechanism's own fast checks.
"""
import os
from pathlib import Path
import resource
import shutil
import signal
import subprocess
import sys
import tempfile
import threading
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import quiet_wait  # noqa: E402

BOUND = 5.0       # the wait's own bound in every case
EVENT_AT = 0.5    # when the event happens
LATE = 2.0        # a wake later than EVENT_AT + this came from the bound, not the event


TIMERS = []


def after(seconds, action):
    t = threading.Timer(seconds, action)
    t.daemon = True
    t.start()
    TIMERS.append(t)
    return t


def append(path, text):
    with open(path, "a") as out:
        out.write(text)


class Watch(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="quiet-wait-test.")

    def tearDown(self):
        while TIMERS:                      # an event still to happen happens before its directory goes
            TIMERS.pop().join(BOUND)
        shutil.rmtree(self.tmp, ignore_errors=True)

    def timed(self, watch):
        start = time.monotonic()
        woke = watch.wait(BOUND)
        return woke, time.monotonic() - start

    def test_a_new_file_in_a_watched_directory_wakes_it(self):
        def write():
            Path(self.tmp, "record.json.new").write_text("{}")
            os.replace(os.path.join(self.tmp, "record.json.new"), os.path.join(self.tmp, "record.json"))
        after(EVENT_AT, write)
        with quiet_wait.Watch(paths=[self.tmp]) as w:
            woke, took = self.timed(w)
        self.assertTrue(woke)
        self.assertLess(took, EVENT_AT + LATE)

    def test_an_append_to_a_watched_file_wakes_it(self):
        log = Path(self.tmp, "polls.log")
        log.write_text("")
        after(EVENT_AT, lambda: append(log, "one line\n"))
        with quiet_wait.Watch(paths=[str(log)]) as w:
            woke, took = self.timed(w)
        self.assertTrue(woke)
        self.assertLess(took, EVENT_AT + LATE)

    def test_a_file_that_does_not_exist_yet_is_watched_through_its_directory(self):
        later = os.path.join(self.tmp, "events.jsonl")
        after(EVENT_AT, lambda: Path(later).write_text("{}\n"))
        with quiet_wait.Watch(paths=[later]) as w:
            woke, took = self.timed(w)
        self.assertTrue(woke)
        self.assertLess(took, EVENT_AT + LATE)

    def test_any_process_exit_wakes_it_not_only_a_childs(self):
        # A grandchild: the shell starts it in the background and exits, so it is no child of ours.
        out = subprocess.run(["/bin/sh", "-c", "sleep %s >/dev/null 2>&1 & echo $!" % EVENT_AT],
                             capture_output=True, text=True, check=True).stdout
        with quiet_wait.Watch(pids=[int(out.strip())]) as w:
            woke, took = self.timed(w)
        self.assertTrue(woke)
        self.assertLess(took, EVENT_AT + LATE)

    def test_a_fork_wakes_a_tree_watch(self):
        parent = subprocess.Popen([sys.executable, "-c",
                                   "import os, sys, time; time.sleep(%s); os.fork() or sys.exit(0); time.sleep(3)"
                                   % EVENT_AT])
        try:
            with quiet_wait.Watch() as w:
                w.add_pids([parent.pid], forks=True)
                woke, took = self.timed(w)
            self.assertTrue(woke)
            self.assertLess(took, EVENT_AT + LATE)
        finally:
            parent.kill()
            parent.wait()

    def test_a_gone_process_wakes_it_once_and_never_spins(self):
        p = subprocess.Popen(["true"])
        p.wait()
        with quiet_wait.Watch(pids=[p.pid]) as w:
            self.assertTrue(w.wait(BOUND), "gone already: the first wait returns at once")
        start = time.monotonic()
        with quiet_wait.Watch(pids=[p.pid]) as w:
            self.assertFalse(w.wait(1.0), "the same gone pid again: it waits for its bound")
        self.assertGreaterEqual(time.monotonic() - start, 0.9)

    def test_a_signal_wakes_it(self):
        old = signal.signal(signal.SIGUSR1, lambda *_a: None)
        try:
            after(EVENT_AT, lambda: os.kill(os.getpid(), signal.SIGUSR1))
            with quiet_wait.Watch() as w:
                w.add_signals([signal.SIGUSR1])
                woke, took = self.timed(w)
            self.assertLess(took, EVENT_AT + LATE)
        finally:
            signal.signal(signal.SIGUSR1, old)

    def test_with_nothing_happening_it_sleeps_its_bound_at_about_zero_cpu(self):
        Path(self.tmp, "quiet").write_text("")
        before = resource.getrusage(resource.RUSAGE_SELF)
        with quiet_wait.Watch(paths=[self.tmp, os.path.join(self.tmp, "quiet")], pids=[os.getppid()]) as w:
            woke, took = self.timed(w)
        used = resource.getrusage(resource.RUSAGE_SELF)
        cpu = (used.ru_utime - before.ru_utime) + (used.ru_stime - before.ru_stime)
        self.assertFalse(woke)
        self.assertGreaterEqual(took, BOUND - 0.1)
        self.assertLess(cpu, 0.05, "%.3f s of CPU over a %.0f s wait" % (cpu, BOUND))

    def test_its_own_reads_of_a_watched_directory_do_not_wake_it(self):
        with quiet_wait.Watch(paths=[self.tmp]) as w:
            os.listdir(self.tmp)
            Path(self.tmp).stat()
            self.assertFalse(w.wait(1.0))


if __name__ == "__main__":
    unittest.main()

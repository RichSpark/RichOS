#!/usr/bin/env python3
"""Wait for something to happen at about 0% CPU: block on the event, never poll for it.

2026-10-28: ten agents' `agent_hold.py wait` calls held about 4.6 of the Mac's 10
cores while they only waited, and every proof run was refused CPU admission. A loop
that sleeps 0.05 s and looks at the whole world again each time spends the CPU the
work it waits for needs. The kernel already knows when the thing happens: on macOS,
kqueue reports a process's exit (EVFILT_PROC NOTE_EXIT, for any process this user
can see, not only a child) and a change to a file or a directory (EVFILT_VNODE: a
write, an append, a rename, a delete, a new or removed entry in a directory).

A waiter registers what it waits on, looks at the world ONCE, then blocks in the
kernel until one of those fires or its own bound passes:

    with Watch(paths=[held_dir]) as w:    # registered BEFORE the look: nothing between is missed
        if done():
            break
        w.add_pids(pids)                  # a process that is already gone wakes it at once
        w.wait(min(left, RECHECK))

It is level-triggered: a wake is only a reason to look again, never the answer. A
missed or spurious event therefore costs latency (at most the caller's re-check
interval), never a wrong result. Where kqueue is missing (not macOS) wait() sleeps
min(seconds, FALLBACK_POLL): slower to notice, still about 0% CPU.
"""
import errno
import os
import select
import time

FALLBACK_POLL = 1.0   # only where kqueue is unavailable; the engine runs on macOS
_O_EVTONLY = getattr(os, "O_EVTONLY", 0x8000 if hasattr(select, "kqueue") else os.O_RDONLY)
_GONE_SEEN = set()    # pids already reported gone once by this process: see add_pids


class Watch:
    """Process exits and path changes, registered now, waited on later. Never raises."""

    def __init__(self, paths=(), pids=()):
        self._kq = None
        self._fds = []
        self._fired = False
        try:
            self._kq = select.kqueue() if hasattr(select, "kqueue") else None
        except OSError:
            self._kq = None
        self.add_paths(paths)
        self.add_pids(pids)

    def __enter__(self):
        return self

    def __exit__(self, *_exc):
        self.close()
        return False

    def add_paths(self, paths):
        """Watch each path (a file or a directory). A path that does not exist yet is
        watched through its parent directory, whose entry list changes when it appears."""
        if self._kq is None:
            return
        flags = select.KQ_EV_ADD | select.KQ_EV_CLEAR
        # No NOTE_ATTRIB: a waiter's own reads must never count as a change it wakes for.
        notes = select.KQ_NOTE_WRITE | select.KQ_NOTE_EXTEND | select.KQ_NOTE_DELETE | select.KQ_NOTE_RENAME
        for path in paths:
            if not path:
                continue
            for candidate in (path, os.path.dirname(os.path.abspath(path))):
                try:
                    fd = os.open(candidate, _O_EVTONLY)
                except OSError:
                    continue
                try:
                    self._kq.control([select.kevent(fd, select.KQ_FILTER_VNODE, flags, notes)], 0, 0)
                    self._fds.append(fd)
                except OSError:
                    os.close(fd)
                    continue
                break

    def add_pids(self, pids, forks=False):
        """Watch each process's exit (and, with forks=True, each fork it makes: a process
        tree's supervisor looks again the moment the tree grows). One that is already gone
        wakes the next wait at once, but only the first time this process sees it gone: a
        pid the caller keeps treating as running (a zombie it does not reap) then waits for
        the bound instead of waking it again and again. A process this user may not watch
        (a setuid program) is left to the caller's bound."""
        if self._kq is None:
            return
        notes = select.KQ_NOTE_EXIT | (select.KQ_NOTE_FORK if forks else 0)
        for pid in pids:
            if not isinstance(pid, int) or pid <= 0:
                continue
            try:
                self._kq.control([select.kevent(pid, select.KQ_FILTER_PROC,
                                                select.KQ_EV_ADD | select.KQ_EV_CLEAR, notes)], 0, 0)
            except OSError as error:
                if error.errno == errno.ESRCH and pid not in _GONE_SEEN:
                    _GONE_SEEN.add(pid)
                    self._fired = True

    def add_signals(self, signums):
        """Wake when one of these signals is delivered to this process. Its handler still
        runs; this only ends the wait at once instead of at its bound (EVFILT_SIGNAL)."""
        if self._kq is None:
            return
        for signum in signums:
            try:
                self._kq.control([select.kevent(int(signum), select.KQ_FILTER_SIGNAL,
                                                select.KQ_EV_ADD | select.KQ_EV_CLEAR)], 0, 0)
            except OSError:
                continue

    def wait(self, seconds):
        """Block until something watched happens (True) or `seconds` pass (False)."""
        if self._fired:
            self._fired = False
            return True
        seconds = max(0.0, float(seconds))
        if seconds <= 0:
            return False
        if self._kq is None:
            time.sleep(min(seconds, FALLBACK_POLL))
            return False
        try:
            return bool(self._kq.control(None, 4, seconds))
        except OSError:
            time.sleep(min(seconds, FALLBACK_POLL))
            return False

    def close(self):
        for fd in self._fds:
            try:
                os.close(fd)
            except OSError:
                pass
        self._fds = []
        if self._kq is not None:
            try:
                self._kq.close()
            except OSError:
                pass
            self._kq = None


def until(seconds, paths=(), pids=()):
    """One blocking wait: True when a watched path changed or process exited, False at `seconds`."""
    with Watch(paths, pids) as w:
        return w.wait(seconds)

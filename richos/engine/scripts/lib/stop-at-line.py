#!/usr/bin/env python3
"""stop-at-line.py — run a command, and stop it the moment its output shows a named line.

    stop-at-line.py --out <file> --line <text> --marker <file> -- <command> [args...]

The command's stdout and stderr go to --out, exactly as `cmd >out 2>&1` would put them.
While it runs, --out is read as it grows. The first complete line CONTAINING --line (a
fixed string, never a pattern) ends the run: the marker file is written, the command's
whole process group gets SIGTERM, then SIGKILL after a grace period, and this exits 0.
If the command ends first, this exits with the command's own exit status and writes no
marker, so a caller that sees no marker judges the run exactly as it would have without
this helper.

WHY IT EXISTS (2026-09-23). The mutation harness (mutation-harness.sh) proves a property
is load-bearing by removing it and watching ONE named case go red. For a suite whose
checks run in sequence and share one sandbox — workspace-spec-fourteen.test.sh, measured
at 84 s a run, 86 mutants — every mutant ran the whole suite even though its verdict was
settled at the named line, on average about 40% of the way in. The rest of the run could
only print more lines nobody reads. A harness opts in with `mutation_focus stop-at-want`,
which is a claim about ITS suite: a printed `FAIL  <case>` line always ends the run red.

What is stopped is the command's whole tree (scripts/lib/proc_tree.py: every descendant and
every group they are in), never a pid chosen by name. The command runs in a session of its own,
so a signal to THIS process's group does not reach it by itself; this process therefore
forwards SIGTERM, SIGINT and SIGHUP as a kill of that tree before it exits, which is what lets
ci-shard.sh's deadline, or an interrupt, take a mutant's suite down with the harness.
"""
import argparse
import os
import signal
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from proc_tree import kill_tree  # noqa: E402
from quiet_wait import Watch  # noqa: E402

# The longest it goes without looking when neither a write nor the exit woke it: a safety net only.
RECHECK_SECONDS = 5.0
GRACE_SECONDS = 10


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--out", required=True)
    p.add_argument("--line", required=True)
    p.add_argument("--marker", required=True)
    # --boundary: the line must also END a word right after --line, as `\b` does in
    # a caller's own `grep "FAIL  <case>\b"`, so `D1` never stops at `FAIL  D14`. A
    # stop must never be looser than the witness test the caller would run instead.
    p.add_argument("--boundary", action="store_true")
    p.add_argument("command", nargs=argparse.REMAINDER)
    a = p.parse_args()
    cmd = a.command[1:] if a.command[:1] == ["--"] else a.command
    if not cmd or not a.line:
        p.error("a command and a non-empty --line are required")
    needle = a.line.encode()
    if a.boundary:
        import re
        bounded = re.compile(re.escape(needle) + rb"\b")
        def hit(line):
            return bounded.search(line) is not None
    else:
        def hit(line):
            return needle in line
    with open(a.out, "wb") as out:
        child = subprocess.Popen(cmd, stdout=out, stderr=subprocess.STDOUT, stdin=subprocess.DEVNULL,
                                 start_new_session=True)

    def forward(signum, _frame):
        kill_tree(child.pid, GRACE_SECONDS)
        child.wait()
        sys.exit(128 + signum)
    for sig in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
        signal.signal(sig, forward)
    pending = b""
    with open(a.out, "rb") as reader:
        while True:
            # Blocks until the command writes to --out or exits (quiet_wait.py), registered
            # before the look so nothing between is missed; it used to look every 0.1 s.
            with Watch(paths=[a.out], pids=[child.pid]) as w:
                rc = child.poll()
                chunk = reader.read()
                if chunk:
                    pending += chunk
                    *lines, pending = pending.split(b"\n")
                    if any(hit(line) for line in lines):
                        return stop(child, a.marker)
                if rc is not None:
                    # One last read: the command may have written its final line and exited
                    # between the read above and the poll.
                    tail = pending + reader.read()
                    if any(hit(line) for line in tail.split(b"\n")):
                        return stop(child, a.marker)
                    return rc if rc >= 0 else 128 - rc
                w.wait(RECHECK_SECONDS)


def stop(child, marker):
    # TERM to the whole tree (EXIT traps run), KILL after the grace for whatever ignored it.
    left = kill_tree(child.pid, GRACE_SECONDS)
    child.wait(timeout=5)
    if left:
        print("stop-at-line: cleanup failed: %s" % left, file=sys.stderr)
        return 125
    with open(marker, "w") as fh:
        fh.write("stopped at the named line\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""An overdue AX read is photographed while it is overdue (esc-20261021T122950Z-b8b7362a).

Runs ax-deadline.py's guest side for real against owned stub children: `sample`, `top`,
`pgrep` and `sysctl` are stubs on PATH that record how they were called, so nothing
here samples a real process or needs a VM.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent.parent
DEADLINE = HERE / 'ax-deadline.py'

# The shape `sample` prints (macOS 15), main thread first, blocked in app code.
SAMPLE = """Analysis of sampling richos-tauri (pid 4242) every 10 milliseconds
Process:         richos-tauri [4242]

Call graph:
    183 Thread_100   DispatchQueue_1: com.apple.main-thread  (serial)
    + 183 start  (in dyld) + 6076  [0x18c5a8274]
    +   183 main  (in richos-tauri) + 52  [0x100001000]
    +     180 richos_tauri::blocking_thing  (in richos-tauri) + 10  [0x100002000]
    +     ! 180 __psynch_mutexwait  (in libsystem_kernel.dylib) + 8  [0x18c8e9000]
    +     3 other_branch  (in richos-tauri) + 1  [0x100003000]
    +     : 3 deeper_in_other_branch  (in richos-tauri) + 1  [0x100004000]
    +     :   3 deepest_in_other_branch  (in richos-tauri) + 1  [0x100005000]
    183 Thread_200: tokio-runtime-worker
    + 183 thread_start  (in libsystem_pthread.dylib) + 8  [0x18c900000]

Total number in stack (recursive counted multiple times):
"""

TOP = """Processes: 300 total
Load Avg: 1.0, 1.0, 1.0
CPU usage: 10.0% user, 5.0% sys, 85.0% idle
PID COMMAND %CPU STATE #TH MEM
1 first 1.0 sleeping 1 1M
Processes: 300 total
Load Avg: 4.20, 3.0, 2.0
CPU usage: 90.00% user, 9.00% sys, 1.00% idle
PhysMem: 4G used
PID    COMMAND      %CPU  STATE    #TH  MEM
4242   richos-tauri 99.0  running  30   400M
"""


def stubs(root, calls):
    binary = root / 'bin'
    binary.mkdir()
    (root / 'sample.txt').write_text(SAMPLE)
    (root / 'top.txt').write_text(TOP)
    for name, body in {
        'sample': 'cat "$STUB_ROOT/sample.txt"',
        'top': 'cat "$STUB_ROOT/top.txt"',
        'pgrep': 'case "$*" in *"System Events"*) echo 777 ;; *) exit 1 ;; esac',
        'sysctl': 'echo "vm.loadavg: { 4.20 3.00 2.00 }"',
    }.items():
        tool = binary / name
        tool.write_text('#!/bin/sh\nprintf "%s\\n" "' + name + ' $*" >> "' + str(calls) + '"\n' + body + '\n')
        tool.chmod(0o700)
    return binary


class SlowRead(unittest.TestCase):
    def guest(self, child, seconds='4'):
        with tempfile.TemporaryDirectory(prefix='ax-slow-') as directory:
            root = Path(directory)
            calls = root / 'calls'
            binary = stubs(root, calls)
            scratch = root / 'tmp'
            scratch.mkdir()
            env = dict(os.environ, PATH=str(binary) + os.pathsep + os.environ['PATH'],
                       STUB_ROOT=str(root), TMPDIR=str(scratch), TESTVM_AX_SLOW_SECONDS='1')
            r = subprocess.run([sys.executable, '-B', str(DEADLINE), '--pid', '4242', seconds, *child],
                               input='', text=True, capture_output=True, env=env, timeout=120)
            self.assertEqual(list(scratch.iterdir()), [], 'the snapshot leaves no files behind')
            return r, calls.read_text().splitlines() if calls.exists() else []

    def test_overdue_read_is_sampled_before_its_deadline(self):
        r, calls = self.guest(['sleep', '30'])
        self.assertEqual(r.returncode, 124)
        self.assertEqual(json.loads(r.stdout.splitlines()[0])['error'], 'guest_deadline',
                         'the classification still comes first')
        self.assertIn('sample 4242 1 10 -mayDie', calls, 'the app under test is sampled')
        self.assertIn('sample 777 1 10 -mayDie', calls, 'System Events (keystrokes, raw scripts) is sampled')
        self.assertIn('--- AX client pid ', r.stderr, 'the read itself (ax.py, the AX client) is sampled')
        marker = next(json.loads(l) for l in r.stderr.splitlines() if l.startswith('{"ax_slow_read"'))
        self.assertGreaterEqual(marker['snapshot_started_at_seconds'], 1.0)
        self.assertLess(marker['snapshot_started_at_seconds'], 3.0, 'started at the line, not at the deadline')
        # The main thread's heaviest path, and nothing from the next thread or a lighter branch.
        self.assertIn('180 __psynch_mutexwait  (in libsystem_kernel.dylib) + 8', r.stderr)
        self.assertNotIn('other_branch', r.stderr)
        # A lighter sibling deeper than the heaviest leaf is not spliced onto its path.
        self.assertNotIn('deepest_in_other_branch', r.stderr)
        self.assertNotIn('tokio-runtime-worker', r.stderr)
        self.assertIn('CPU usage: 90.00% user, 9.00% sys, 1.00% idle', r.stderr, 'the second top sample')
        self.assertNotIn('85.0% idle', r.stderr)

    def test_a_read_that_answers_in_time_costs_nothing(self):
        r, calls = self.guest(['true'])
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(calls, [])
        self.assertNotIn('ax_slow_read', r.stderr)

    def test_a_slow_read_that_answers_keeps_its_output_and_exit(self):
        r, calls = self.guest(['sh', '-c', 'sleep 2; echo answered; exit 3'], seconds='8')
        self.assertEqual(r.returncode, 3)
        self.assertEqual(r.stdout.strip(), 'answered', 'the read is never retried or replaced')
        self.assertIn('ax_slow_read', r.stderr)
        self.assertEqual(sum(c.startswith('sample ') for c in calls), 3)

    def test_host_reading_is_taken_while_the_read_is_overdue(self):
        with tempfile.TemporaryDirectory(prefix='ax-slow-') as directory:
            env = dict(os.environ, TMPDIR=directory, TESTVM_AX_SLOW_SECONDS='1')
            r = subprocess.run([sys.executable, '-B', str(DEADLINE), '--host', '5', 'sleep', '30'],
                               input='', text=True, capture_output=True, env=env, timeout=120)
        self.assertEqual(r.returncode, 124)
        reading = next(json.loads(l) for l in r.stderr.splitlines() if l.startswith('{"ax_host_reading"'))
        self.assertLess(reading['at_seconds'], 3.0, 'taken at the slow line, not after the deadline')
        self.assertIn('busy_percent', reading, r.stderr)


if __name__ == '__main__':
    unittest.main()

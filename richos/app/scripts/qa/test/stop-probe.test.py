#!/usr/bin/env python3
"""The Stop measurement keeps its verdict even when the version process hangs."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import unittest

QA = Path(os.environ.get("QA_PROBE_DIR", Path(__file__).resolve().parents[1]))
FIXTURE = '''
import json
import os
from pathlib import Path
import signal
import sys
import time

if sys.argv[1] == "--version":
    if not os.environ.get("VERSION_STALL"):
        print("9.9.9 (fixture)")
        sys.exit(0)
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    ready, announce = os.pipe()
    child = os.fork()
    if child == 0:
        os.close(ready)
        os.setsid()
        os.write(announce, b"1")
        time.sleep(12)
        sys.exit(0)
    os.close(announce)
    os.read(ready, 1)
    Path(os.environ["VERSION_PIDS"]).write_text(f"{os.getpid()} {child}")
    time.sleep(12)
    sys.exit(0)

for _ in range(2):
    sys.stdin.readline()
tool = {"type": "tool_use", "name": "Bash", "id": "fg", "input": {"command": "bash ./long-task.sh 60"}}
print(json.dumps({"type": "assistant", "message": {"content": [tool]}}), flush=True)
sys.stdin.read()
'''


class StopProbe(unittest.TestCase):
    def run_version(self, stall):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            provider = root / "provider"
            provider.write_text(f"#!{sys.executable}\n" + FIXTURE)
            provider.chmod(0o700)
            pids = root / "version.pids"
            env = {**os.environ, "VERSION_PIDS": str(pids), "VERSION_STALL": "1" if stall else ""}
            out = root / "out"
            argv = [sys.executable, str(QA / "stop-probe.py"), "--mode", "none", "--out", str(out),
                    "--claude", str(provider), "--start-within", "2", "--stop-after", "0", "--observe", "0"]
            try:
                # load-bound: the outer timeout catches hangs; the verdict and process status prove the result.
                result = subprocess.run(argv, env=env, capture_output=True, text=True, timeout=7)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                summary = json.loads((out / "summary.json").read_text())
                self.assertEqual(summary["claude_version"], "" if stall else "9.9.9 (fixture)")
                self.assertIn("sleeps_3s_after_stop", summary)
                if stall:
                    self.assertIn("claude_version_error", summary)
                    for pid in pids.read_text().split():
                        status = subprocess.run(["ps", "-p", pid, "-o", "stat="],
                                                capture_output=True, text=True).stdout.strip()
                        self.assertTrue(not status or status.startswith("Z"), status)
            finally:
                if pids.exists():
                    for pid in pids.read_text().split():
                        try:
                            os.kill(int(pid), signal.SIGKILL)
                        except ProcessLookupError:
                            pass

    def test_fast_version_is_kept(self):
        self.run_version(stall=False)

    def test_hung_version_and_child_are_stopped(self):
        self.run_version(stall=True)


if __name__ == "__main__":
    unittest.main()

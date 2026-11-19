#!/usr/bin/env python3
"""Exercise context and effort deadlines and ownership with a local stream stand-in."""
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

mode = os.environ["FIXTURE_MODE"]
project = Path(os.environ["FIXTURE_PROJECT"])
args = sys.argv[1:]
session = args[args.index("--session-id") + 1]
model, effort = "fable", "low"

def send(frame):
    print(json.dumps(frame), flush=True)

# Reparent a child outside the provider's group while it still holds stdout.
ready, announce = os.pipe()
child = os.fork()
if child == 0:
    os.close(ready)
    os.setsid()
    os.close(0)
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    os.write(announce, b"1")
    time.sleep(20)
    sys.exit(0)
os.close(announce)
os.read(ready, 1)
os.close(ready)
Path("owned.pid").write_text(str(child))

turns = 0
for line in sys.stdin:
    frame = json.loads(line)
    if frame["type"] == "user":
        turns += 1
        with (project / (session + ".jsonl")).open("a") as out:
            print(json.dumps({"type": "assistant", "message": {"model": model}, "effort": effort}), file=out)
        send({"type": "stream_event", "event": {"type": "message_delta", "usage": {"input_tokens": 42}}})
        if mode == "broken-input":
            os.close(0)
        send({"type": "result", "usage": {"input_tokens": 42}})
        if mode == "broken-input":
            sys.exit(0)
        if turns == 2 and os.environ["FIXTURE_PROBE"] == "live-effort":
            sys.exit(0)
        continue
    request = frame["request"]["subtype"]
    if request == "initialize":
        continue
    if mode == "unrelated" and turns == 1 and request != "apply_flag_settings":
        # A matching response arrives only after the allowed budget. Unrelated
        # replies keep an idle timeout alive throughout.
        for n in range(60):
            send({"type": "control_response", "response": {"request_id": str(n)}})
            time.sleep(0.05)
    if request == "set_model":
        model = "opus"
    if request == "apply_flag_settings":
        effort = "high"
    send({"type": "control_response", "response": {"request_id": frame["request_id"],
          "subtype": "success", "response": {"totalTokens": 42}}})
    if turns == 3:
        sys.exit(0)
'''


class ProviderProbes(unittest.TestCase):
    def run_probe(self, name, mode, expected_code, expected_output):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            provider = root / "provider"
            provider.write_text(f"#!{sys.executable}\n" + FIXTURE)
            provider.chmod(0o700)
            project = root / "projects" / "fixture"
            project.mkdir(parents=True)
            args = ["--words", "4"] if name == "context-figure" else ["--projects", str(project.parent)]
            env = {**os.environ, "FIXTURE_MODE": mode, "FIXTURE_PROBE": name, "FIXTURE_PROJECT": str(project)}
            pidfile = root / "owned.pid"
            unrelated = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(20)"],
                                         start_new_session=True)
            try:
                result = subprocess.run([sys.executable, str(QA / (name + "-probe.py")), "--claude", str(provider),
                                         "--cwd", tmp, "--wait", "1" if mode == "unrelated" else "5", *args],
                                        env=env, capture_output=True, text=True,
                                        timeout=15)  # load-bound: hang guard; exit status and the missing control reply prove the verdict
                output = result.stdout + result.stderr
                self.assertEqual(result.returncode, expected_code, output)
                self.assertIn(expected_output, output)
                pid = int(pidfile.read_text())
                status = subprocess.run(["ps", "-p", str(pid), "-o", "stat="],
                                        capture_output=True, text=True).stdout.strip()
                self.assertTrue(not status or status.startswith("Z"), status)
                self.assertIsNone(unrelated.poll())
            finally:
                unrelated.kill()
                unrelated.wait()
                if pidfile.exists():
                    try:
                        os.kill(int(pidfile.read_text()), signal.SIGKILL)
                    except ProcessLookupError:
                        pass

    def test_success_cleans_up_after_leader_exit(self):
        for name, output in (("context-figure", "plain\t0\t42\t42\t42"),
                             ("live-effort", "2\topus:high\topus\thigh")):
            with self.subTest(probe=name):
                self.run_probe(name, "success", 0, output)

    def test_unrelated_replies_cannot_extend_budget(self):
        for name, output in (("context-figure", "no figure to compare"),
                             ("live-effort", "control request set_model was not confirmed")):
            with self.subTest(probe=name):
                self.run_probe(name, "unrelated", 2, output)

    def test_broken_provider_input_still_cleans_tree(self):
        for name in ("context-figure", "live-effort"):
            with self.subTest(probe=name):
                self.run_probe(name, "broken-input", 2, "Broken pipe")


if __name__ == "__main__":
    unittest.main()

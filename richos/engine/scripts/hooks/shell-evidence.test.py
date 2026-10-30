#!/usr/bin/env python3
"""shell-evidence.sh: failure propagation for every Bash call, and ownership capture for a subagent's.

Runs the registered entry (shell-evidence.sh) as the host does, with a payload on
stdin. Ownership state goes to a temporary RICHOS_AGENT_HOLD_DIR.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent
HOOK = HERE / "shell-evidence.sh"
SHELL = shutil.which("zsh") or "/bin/sh"
PREFIX = "set -e -o pipefail\n"


class ShellEvidence(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="shell-evidence-test.")
        self.env = {**os.environ, "RICHOS_AGENT_HOLD_DIR": os.path.join(self.tmp, "hold"),
                    "CLAUDE_CODE_ENTRYPOINT":"cli"}
        self.env.pop("RICHOS_AGENT_BASH_FOREGROUND",None)

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def run_hook(self, payload, env=None):
        raw = payload if isinstance(payload, str) else json.dumps(payload)
        p = subprocess.run(["bash", str(HOOK)], input=raw, capture_output=True, text=True, env=env or self.env)
        self.assertEqual(p.returncode, 0, p.stderr)
        return json.loads(p.stdout) if p.stdout.strip() else {}

    def bash(self, command, **extra):
        ti = {"command": command, "description": "fixture", "timeout": 5000, "run_in_background": False}
        return {"tool_name": "Bash", "session_id": "fixture-session", "tool_use_id": "toolu_fixture",
                "cwd": self.tmp, "tool_input": ti, **extra}

    def test_lead_call_gets_only_the_failure_prefix(self):
        out = self.run_hook(self.bash("echo hi | cat"))
        ui = out["hookSpecificOutput"]["updatedInput"]
        self.assertEqual(ui["command"], PREFIX + "echo hi | cat")
        self.assertEqual({k: v for k, v in ui.items() if k != "command"},
                         {"description": "fixture", "timeout": 5000, "run_in_background": False})
        self.assertFalse(os.path.exists(os.path.join(self.tmp, "hold")), "the lead's calls record nothing")

    def test_already_rewritten_call_is_left_alone(self):
        self.assertEqual(self.run_hook(self.bash(PREFIX + "true")), {})
        out = self.run_hook(self.bash(PREFIX + "true", agent_id="afixture1"))
        self.assertFalse(out["hookSpecificOutput"]["updatedInput"]["run_in_background"])

    def test_other_tools_are_untouched(self):
        self.assertEqual(self.run_hook({"tool_name": "Read", "tool_input": {"file_path": "/x"}}), {})

    def test_unreadable_calls_say_so(self):
        self.assertIn("invalid payload", self.run_hook("not json")["systemMessage"])
        self.assertIn("Bash command missing", self.run_hook({"tool_name": "Bash", "tool_input": {}})["systemMessage"])

    def test_subagent_call_records_its_owner(self):
        out = self.run_hook(self.bash("echo hi", agent_id="afixture1"))
        cmd = out["hookSpecificOutput"]["updatedInput"]["command"]
        self.assertTrue(cmd.startswith(PREFIX), cmd)
        self.assertTrue(cmd.endswith("\necho hi"))
        self.assertNotIn("$$", cmd)
        self.assertIn(" mark --state ", cmd)
        self.assertIn("export RICHOS_AGENT_OWNER=afixture1", cmd)
        self.assertNotIn("kill -STOP $$", cmd, "a held agent's call is refused, never self-suspended")
        self.assertNotIn("USR1", cmd)
        self.assertFalse(out["hookSpecificOutput"]["updatedInput"]["run_in_background"])
        record = Path(self.tmp, "hold", "shells", "fixture-session", "afixture1", "toolu_fixture.json")
        self.assertEqual(json.loads(record.read_text())["mode"], "native")
        self.assertEqual(out["hookSpecificOutput"]["updatedInput"]["timeout"], 6000)

    def test_an_app_workers_ordinary_call_runs_as_in_the_terminal(self):
        """2026-10-30: an app (sdk-cli) worker's ordinary call through the registered hook comes
        back with its own foreground choice and timeout and no wait instruction. On cb30e5746 it
        came back with run_in_background true and 'run agent_hold.py wait' on every command."""
        payload = self.bash("sleep 3; echo done", agent_id="afixture1")
        payload["tool_input"]["timeout"] = 120000
        del payload["tool_input"]["run_in_background"]
        out = self.run_hook(payload, env={**self.env, "CLAUDE_CODE_ENTRYPOINT": "sdk-cli"})
        hso = out["hookSpecificOutput"]
        ui = hso["updatedInput"]
        self.assertNotIn("run_in_background", ui)
        self.assertEqual(ui["timeout"], 120000)
        self.assertTrue(ui["command"].startswith(PREFIX) and ui["command"].endswith("\nsleep 3; echo done"))
        self.assertIn(" mark --state ", ui["command"], "ownership is still recorded, so a pause can hold it")
        self.assertNotIn("agent_hold.py", hso["additionalContext"])
        self.assertNotIn("wait", hso["additionalContext"].lower())
        record = Path(self.tmp, "hold", "shells", "fixture-session", "afixture1", "toolu_fixture.json")
        self.assertEqual(json.loads(record.read_text())["mode"], "host")
        # It runs in the foreground and its output comes back directly.
        p = subprocess.run([SHELL, "-c", ui["command"]], capture_output=True, text=True,
                           env={**self.env, "CLAUDE_CODE_ENTRYPOINT": "sdk-cli"}, timeout=30)
        self.assertEqual((p.returncode, p.stdout), (0, "done\n"), p.stderr)

    def test_background_call_is_not_wrapped(self):
        payload = self.bash("echo hi", agent_id="afixture1")
        payload["tool_input"]["run_in_background"] = True
        cmd = self.run_hook(payload)["hookSpecificOutput"]["updatedInput"]["command"]
        self.assertTrue(cmd.endswith("\necho hi"), cmd)
        self.assertNotIn("USR1", cmd)

    def test_the_wait_command_gets_the_longest_timeout_and_no_hold_check(self):
        out = self.run_hook(self.bash("python3 ~/.claude/richos-engine/scripts/lib/agent_hold.py wait",
                                      agent_id="afixture1"))
        ui = out["hookSpecificOutput"]["updatedInput"]
        self.assertEqual(ui["timeout"], 600000)
        self.assertFalse(ui["run_in_background"])
        self.assertTrue(ui["command"].endswith("\npython3 %s wait" % (HERE.parent / "lib" / "agent_hold.py")),
                        "the wait runs the same engine file that wrapped the agent's calls")
        self.assertNotIn("exit 75", ui["command"])
        self.assertIn("RICHOS_AGENT_HOLD_WAIT_SECONDS=270", ui["command"])
        # The lead's own call of the same text is left as it was.
        lead = self.run_hook(self.bash("python3 ~/.claude/richos-engine/scripts/lib/agent_hold.py wait"))
        self.assertEqual(lead["hookSpecificOutput"]["updatedInput"]["timeout"], 5000)

    def test_rewritten_command_runs_as_before(self):
        out = self.run_hook(self.bash('printf "%s" "$RICHOS_AGENT_OWNER"; exit 3', agent_id="afixture1"))
        cmd = out["hookSpecificOutput"]["updatedInput"]["command"]
        p = subprocess.run([SHELL, "-c", cmd], capture_output=True, text=True)
        self.assertEqual((p.returncode, p.stdout), (3, "afixture1"))
        pid_file = Path(self.tmp, "hold", "shells", "fixture-session", "afixture1", "toolu_fixture.pid")
        self.assertEqual(len(pid_file.read_text().split()), 2, "the shell recorded its own pid and parent")
        # Failure propagation still holds after the capture lines.
        out = self.run_hook(self.bash("false | cat; echo unreachable", agent_id="afixture1"))
        p = subprocess.run([SHELL, "-c", out["hookSpecificOutput"]["updatedInput"]["command"]],
                           capture_output=True, text=True)
        self.assertNotEqual(p.returncode, 0)
        self.assertNotIn("unreachable", p.stdout)

    def test_held_call_is_denied_before_execution_even_with_failure_prefix(self):
        held = Path(self.tmp, "hold", "held", "fixture-session__afixture1.json")
        held.parent.mkdir(parents=True)
        held.write_text("{}")
        for command in ("echo never", PREFIX + "echo never"):
            out = self.run_hook(self.bash(command, agent_id="afixture1"))["hookSpecificOutput"]
            self.assertEqual(out["permissionDecision"], "deny")
            self.assertIn("WAIT:", out["permissionDecisionReason"])
            self.assertNotIn("updatedInput", out)

    def test_capture_failure_never_breaks_the_call(self):
        blocked = os.path.join(self.tmp, "a-file")
        Path(blocked).write_text("")
        env = {**self.env, "RICHOS_AGENT_HOLD_DIR": blocked}
        out = self.run_hook(self.bash("echo hi", agent_id="afixture1"), env=env)
        self.assertEqual(out["hookSpecificOutput"]["updatedInput"]["command"], PREFIX + "echo hi")


if __name__ == "__main__":
    unittest.main(verbosity=2)

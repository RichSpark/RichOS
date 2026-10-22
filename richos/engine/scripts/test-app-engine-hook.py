#!/usr/bin/env python3
"""Synthetic tests of the exact desktop wrapper and canonical Stop analyzer."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ENGINE = Path(__file__).resolve().parents[1]


class DesktopHooks(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="desktop hooks ")
        self.addCleanup(self.scratch.cleanup)
        self.root = Path(self.scratch.name).resolve()
        self.coordination = self.root / "coordination"
        (self.coordination / ".claude/agents").mkdir(parents=True)
        (self.coordination / "orchestration.config").write_text("")
        (self.coordination / ".claude/agents/worker.md").write_text("---\nname: worker\nmodel: sonnet\n---\n")
        self.scope = self.root / "scope.json"
        self.scope.write_text(json.dumps({"version": 1, "actions_allowed": True}))
        self.env = {**os.environ, "RICHOS_APP_STATE": str(self.root / "state"),
            "RICHOS_APP_SCOPE": str(self.scope), "RICHOS_ENTITY_ROOT": str(self.coordination),
            "RICHOS_ENGINE_ROOT": str(ENGINE), "RICHOS_SA_ENTITY_ROOT": str(self.coordination),
            "RICHOS_SA_TEAMS_DIR": str(self.root / "teams"),
            "RICHOS_WORKSPACES_DIR": str(self.root / "workspaces"),
            "CLAUDE_PROJECT_DIR": str(self.coordination), "PYTHONDONTWRITEBYTECODE": "1"}
        subprocess.run(["git", "init", "-q", str(self.coordination)], check=True, capture_output=True)

    def hook(self, event, **fields):
        payload = {"hook_event_name": event, "session_id": "fictional-session", "prompt_id": "fictional-turn",
            "cwd": str(self.coordination), **fields}
        return subprocess.run([sys.executable, str(ENGINE / "scripts/app-engine-hook.py")],
            input=json.dumps(payload), text=True, capture_output=True, env=self.env, timeout=25)

    def test_stop_refuses_a_claim_of_dispatch_without_dispatch(self):
        self.assertEqual(self.hook("UserPromptSubmit", prompt="Please delegate this work.").returncode, 0)
        result = self.hook("Stop",
            last_assistant_message="I'm dispatching Worker now.", stop_hook_active=False)
        self.assertEqual(result.returncode, 2, result.stderr + result.stdout)
        self.assertIn("dispatch", result.stderr.lower())

    def _wait_on_file(self):
        import time
        holds = self.root / "workspaces" / "holds"
        holds.mkdir(parents=True)
        (holds / "stream-b.json").write_text(json.dumps({"name": "stream-b", "what": "Stream B of his order",
            "why": "on hold, deliberately", "end": "", "at": time.time() - 31 * 60, "session_id": "a-stu-session"}))

    def test_richs_turn_opens_with_every_hold_nothing_checkable_ends(self):
        """No silent holds (2026-10-22): the app's Rich is told in his own turn what the
        terminal's turn end shows, including a hold Stu's lease made."""
        self._wait_on_file()
        result = self.hook("UserPromptSubmit", prompt="How is it going?")
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        context = json.loads(result.stdout)["hookSpecificOutput"]["additionalContext"]
        self.assertIn("HELD BY RICH, NOT BY YOU: Stream B of his order since", context)
        self.assertIn("Why: on hold, deliberately.", context)

    def test_a_reply_holding_work_opens_a_hold_at_the_apps_turn_end(self):
        """Second review rv-20261022T013056Z-12c77c90-1291 finding 1: the app's Stop ran only the
        stated-actions guard, so 'Spelling work is on hold, deliberately.' opened nothing."""
        transcript = self.root / "transcript.jsonl"
        transcript.write_text(json.dumps({"type": "user", "isSidechain": False,
                                          "message": {"role": "user", "content": "How is it going?"}}) + "\n")
        result = self.hook("Stop", last_assistant_message="Spelling work is on hold, deliberately.",
                           transcript_path=str(transcript), stop_hook_active=False)
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        holds = sorted((self.root / "workspaces" / "holds").glob("held-*.json"))
        self.assertEqual(len(holds), 1, result.stderr)
        self.assertEqual(json.loads(holds[0].read_text())["what"], "Spelling work is on hold, deliberately.")

    def test_stus_turn_carries_no_hold_lines(self):
        self._wait_on_file()
        (self.root / "state/evidence/fictional-session").mkdir(parents=True)
        (self.root / "state/evidence/fictional-session/binding.json").write_text('{"role": "stu"}')
        result = self.hook("UserPromptSubmit", prompt="Carry on.")
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertNotIn("HELD BY RICH", result.stdout)

    def test_native_response_formatting_records_evidence_without_opening_a_work_grant(self):
        self.scope.write_text('{"version":1,"actions_allowed":false}')
        for event in ("PreToolUse", "PostToolUse"):
            result = self.hook(event, tool_name="StructuredOutput", tool_use_id="format-reply", tool_input={"reply":"Ready."})
            self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse(json.loads(self.scope.read_text())["actions_allowed"])
        self.assertTrue((self.root / "state/evidence/fictional-session/callbacks.jsonl").exists())
        denied = self.hook("PreToolUse", tool_name="StructuredOutput", agent_id="a-worker", tool_input={"reply":"Not the main session"})
        self.assertEqual(denied.returncode, 2)
        for tool in ("Bash", "Read", "Agent"):
            self.assertEqual(self.hook("PreToolUse", tool_name=tool, tool_input={}).returncode, 2)

    def test_closed_grant_refuses_new_tools_before_recording_an_attempt(self):
        self.scope.write_text('{"version":1,"actions_allowed":false}')
        result = self.hook("PreToolUse", tool_name="Bash", tool_use_id="tool-1", tool_input={"command":"true"})
        self.assertEqual(result.returncode, 2)
        self.assertFalse((self.root / "state/evidence/fictional-session/callbacks.jsonl").exists())
        self.assertEqual(self.hook("Stop", last_assistant_message="I'm dispatching Worker now.").returncode, 0)

    def test_reading_context_requires_an_open_grant(self):
        read = self.hook("PreToolUse", tool_name="Read", tool_use_id="open-read",
                         tool_input={"file_path": str(self.scope)})
        self.assertEqual(read.returncode, 0, read.stderr + read.stdout)
        self.scope.write_text(json.dumps({"version": 1, "actions_allowed": False}))
        for tool in ("Read", "Write", "Bash", "Agent", "mcp__richos_assignments__record"):
            denied = self.hook("PreToolUse", tool_name=tool, tool_use_id="closed-action", tool_input={})
            self.assertEqual(denied.returncode, 2, tool + denied.stderr)
            self.assertIn("This app turn is stopped", denied.stderr)

    def test_a_dispatched_worker_still_acts_after_the_turn_that_dispatched_it_ended(self):
        """The turn's grant is the LEASE's; a background worker's is its receipt's.

        Measured 2026-09-18 (`work_lease_roundtrip`, that run's own `callbacks.jsonl` row
        21, the worker's last words): with the turn closed, the worker's every tool call —
        including `SubagentHandback` — was refused with the sentence below, so it changed
        nothing and its worktree was then disposed of as landed. It is dispatched
        `run_in_background: true` and does ALL of its work after that turn has ended.

        Admitted here is not waved through: the call goes on to `guard-sealed-worktree.sh`
        and to `app.py`'s `worker_context`, which refuses any agent id that does not join
        exactly one live receipt of this session. There is no receipt in this fixture, so
        the attempt is still refused — by the WORKER's gate, naming the worker's own
        reason, and with the attempt recorded, which the turn gate refuses before
        recording anything."""
        self.scope.write_text(json.dumps({"version": 1, "actions_allowed": False,
                                          "background_work_allowed": True}))
        result = self.hook("PreToolUse", tool_name="Bash", tool_use_id="tool-2",
                           agent_id="a1b2c3d4e5f6a7b8c", tool_input={"command": "true"})
        self.assertNotIn("This app turn is stopped", result.stderr)
        self.assertTrue((self.root / "state/evidence/fictional-session/callbacks.jsonl").exists(),
                        "the worker's attempt was not even recorded")
        # Its own gate, not the turn's, and it is the one that answers.
        self.assertEqual(result.returncode, 2)

        # FAIL CLOSED both ways: no standing grant, or no worker identity, and the turn
        # gate is exactly as closed as it was before.
        self.scope.write_text('{"version":1,"actions_allowed":false}')
        without_grant = self.hook("PreToolUse", tool_name="Bash", tool_use_id="tool-3",
                                  agent_id="a1b2c3d4e5f6a7b8c", tool_input={"command": "true"})
        self.assertEqual(without_grant.returncode, 2)
        self.assertIn("This app turn is stopped", without_grant.stderr)
        self.scope.write_text(json.dumps({"version": 1, "actions_allowed": False,
                                          "background_work_allowed": True}))
        lease_itself = self.hook("PreToolUse", tool_name="Bash", tool_use_id="tool-4",
                                 tool_input={"command": "true"})
        self.assertEqual(lease_itself.returncode, 2)
        self.assertIn("This app turn is stopped", lease_itself.stderr)

    def test_missing_or_corrupt_grant_fails_closed(self):
        for content in ('{', '{"version":99,"actions_allowed":true}'):
            self.scope.write_text(content)
            self.assertEqual(self.hook("PreToolUse", tool_name="Read", tool_use_id="tool-1").returncode, 2)
        self.scope.unlink()
        self.assertEqual(self.hook("PreToolUse", tool_name="Read", tool_use_id="tool-1").returncode, 2)

    def test_provider_callbacks_preserve_identity_and_do_not_attest_user_authority(self):
        self.assertEqual(self.hook("UserPromptSubmit", prompt="Fictional request").returncode, 0)
        self.assertEqual(self.hook("PreToolUse", tool_name="Read", tool_use_id="tool-7", tool_input={"file_path":"fixture"}).returncode, 0)
        path = self.root / "state/evidence/fictional-session/guard-transcript.jsonl"
        rows = [json.loads(line) for line in path.read_text().splitlines()]
        self.assertEqual(rows[0]["promptSource"], "runtime")
        self.assertEqual(rows[1]["message"]["content"][0]["id"], "tool-7")
        self.assertEqual(path.stat().st_mode & 0o777, 0o600)

    def test_malformed_dispatch_is_refused_by_canonical_guard(self):
        result = self.hook("PreToolUse", tool_name="Agent", tool_use_id="tool-8",
            tool_input={"subagent_type":"worker", "prompt":"Edit an arbitrary file"})
        self.assertEqual(result.returncode, 2)

    def test_stu_native_tools_do_not_inherit_a_jobs_stopped_authority(self):
        import hashlib
        partition = hashlib.sha256(b'["company","thread"]').hexdigest()
        assignments = self.root / "state/assignments" / partition
        assignments.mkdir(parents=True)
        stopped = {"ledger_ref":"ledger:thread:stopped", "sha256":"a" * 64}
        (assignments / "job.json").write_text(json.dumps({"schema":1, "id":"job",
            "entity_id":"company", "thread_id":"thread", "obligation_id":"work-job",
            "instruction_ledger_ref":stopped["ledger_ref"], "instruction_sha256":stopped["sha256"],
            "state":"interrupted", "user_stop_requested_at_ms":1234}))
        grant = {"version":1, "actions_allowed":True, "background_work_allowed":True,
            "binding":{"entity_id":"company", "thread_id":"thread", "session_id":"fictional-session", "turn_id":"peer:1"}}
        # Old saved metadata must not become a per-action grant either.
        for instruction in [None, stopped, {"ledger_ref":"ledger:thread:other", "sha256":"b" * 64}]:
            self.scope.write_text(json.dumps({**grant, "user_instruction":instruction}))
            for tool, args in [("Bash", {"command":"true", "run_in_background":False, "timeout":30000, "description":"Unchanged command"}),
                               ("Write", {"file_path":str(self.coordination / "result.txt"), "content":"A continues"}),
                               ("Read", {"file_path":str(self.scope)}),
                               ("TaskOutput", {"task_id":"command-for-other-job", "block":False, "timeout":1000}),
                               ("mcp__richos_status__send", {"text":"Stopped B; continuing A"}),
                               ("mcp__richos_questions__withdraw", {"question_id":"own-question", "reason":"Stopped"})]:
                result = self.hook("PreToolUse", tool_name=tool, tool_use_id="direct-" + tool, tool_input=args)
                self.assertEqual(result.returncode, 0, tool + result.stderr)
                output = json.loads(result.stdout).get("hookSpecificOutput", {}) if result.stdout.strip() else {}
                if tool == "Bash":
                    self.assertEqual(output.get("updatedInput"), {**args, "run_in_background":True},
                                     "§129 changes only the native background option")
                else:
                    self.assertNotIn("updatedInput", output, "unrelated native arguments must stay unchanged")
        self.assertFalse((self.coordination / "result.txt").exists(), "the fixture only runs the production hook")
        self.scope.write_text(json.dumps({**grant, "background_work_allowed":False}))
        rich = self.hook("PreToolUse", tool_name="Bash", tool_use_id="rich-command", tool_input={"command":"true", "run_in_background":False})
        self.assertEqual(rich.returncode, 0, rich.stderr)
        rich_output = json.loads(rich.stdout).get("hookSpecificOutput", {}) if rich.stdout.strip() else {}
        self.assertNotIn("updatedInput", rich_output, "§129 applies to Stu's commands")


    def test_an_agent_dispatch_answers_with_one_envelope_so_its_updated_input_reaches_the_teammate(self):
        """Claude Code acts on a hook's stdout only when it is ONE JSON object (measured on 2.1.306:
        an Agent hook that printed a context object before its answer left the prompt unchanged).
        The canonical guards forward their own output, so a dispatch printed several, and the
        repository's own instruction file never reached the worker (folder-instructions walk of
        2026-10-21). The hook now answers an Agent call with one merged envelope."""
        import importlib.util, io
        spec = importlib.util.spec_from_file_location("app_engine_hook_one", ENGINE / "scripts/app-engine-hook.py")
        hook = importlib.util.module_from_spec(spec); spec.loader.exec_module(hook)
        delivered = {"subagent_type": "richos-app-engine:norm", "prompt": "the full prompt", "name": "norm-sonnet-1"}
        def handle(payload):
            print(json.dumps({"hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": "a guard's note"}}))
            print("a guard's plain line")
            print(json.dumps({"hookSpecificOutput": {"hookEventName": "PreToolUse", "updatedInput": delivered}}))
        hook.handle = handle
        out = io.StringIO()
        hook.main(io.StringIO(json.dumps({"hook_event_name": "PreToolUse", "tool_name": "Agent"})), out)
        lines = [line for line in out.getvalue().splitlines() if line.strip()]
        self.assertEqual(len(lines), 1, out.getvalue())
        answer = json.loads(lines[0])["hookSpecificOutput"]
        self.assertEqual(answer["updatedInput"], delivered)
        self.assertEqual(answer["additionalContext"], "a guard's note")
        self.assertNotIn("permissionDecision", answer, "merging decides nothing a guard did not")
        # A refusal among them is never softened by a later answer.
        def refusing(payload):
            print(json.dumps({"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "deny",
                                                     "permissionDecisionReason": "refused"}}))
            print(json.dumps({"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "allow",
                                                     "updatedInput": delivered}}))
        hook.handle = refusing
        out = io.StringIO()
        hook.main(io.StringIO(json.dumps({"hook_event_name": "PreToolUse", "tool_name": "Agent"})), out)
        self.assertEqual(json.loads(out.getvalue())["hookSpecificOutput"]["permissionDecision"], "deny")


if __name__ == "__main__":
    unittest.main()

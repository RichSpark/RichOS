#!/usr/bin/env python3
"""Synthetic tests of the exact desktop wrapper and canonical Stop analyzer."""
import contextlib
import io
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
            "STALL_WATCH_STATE_DIR": str(self.root / "stall-watch"),
            "CLAUDE_PROJECT_DIR": str(self.coordination), "PYTHONDONTWRITEBYTECODE": "1",
            # The turn end runs every registered Stop check, and some read machine-wide stores
            # (the escalation ledger, the land leases): the engine's own sandbox convention keeps
            # them in the fixture (machine_settings.claude_home).
            "CLAUDE_CONFIG_DIR": str(self.root / "claude-home"), "HOME": str(self.root / "home")}
        (self.root / "home").mkdir()
        subprocess.run(["git", "init", "-q", str(self.coordination)], check=True, capture_output=True)

    def hook(self, event, **fields):
        payload = {"hook_event_name": event, "session_id": "fictional-session", "prompt_id": "fictional-turn",
            "cwd": str(self.coordination), **fields}
        return subprocess.run([sys.executable, str(ENGINE / "scripts/app-engine-hook.py")],
            input=json.dumps(payload), text=True, capture_output=True, env=self.env, timeout=25)

    def native_transcript(self, prompt):
        """Claude Code's own transcript of this turn, the one the guards read."""
        transcript = self.root / "transcript.jsonl"
        transcript.write_text(json.dumps({"type": "user", "isSidechain": False, "promptId": "fictional-turn",
                                          "message": {"role": "user", "content": prompt}}) + "\n")
        return str(transcript)

    def test_stop_refuses_a_claim_of_dispatch_without_dispatch(self):
        transcript = self.native_transcript("Please delegate this work.")
        self.assertEqual(self.hook("UserPromptSubmit", prompt="Please delegate this work.",
                                   transcript_path=transcript).returncode, 0)
        result = self.hook("Stop", transcript_path=transcript,
            last_assistant_message="I'm dispatching Worker now.", stop_hook_active=False)
        self.assertEqual(result.returncode, 2, result.stderr + result.stdout)
        self.assertIn("dispatch", result.stderr.lower())

    def _wait_on_file(self):
        import time
        holds = self.root / "workspaces" / "holds"
        holds.mkdir(parents=True)
        (holds / "stream-b.json").write_text(json.dumps({"name": "stream-b", "what": "Stream B of his order",
            "why": "on hold, deliberately", "end": "", "at": time.time() - 31 * 60, "session_id": "a-stu-session"}))

    def test_richs_turn_opens_with_due_work_once_while_no_watcher_runs(self):
        """Orchestration reliability R5/R6: with no healthy watcher for this conversation,
        Rich's turn opens with the due work (a hold Stu's lease made included), once per
        change; never the old wall at every turn."""
        self._wait_on_file()
        result = self.hook("UserPromptSubmit", prompt="How is it going?")
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        context = json.loads(result.stdout)["hookSpecificOutput"]["additionalContext"]
        self.assertIn("STALL WATCH NOT ACTIVE", context)
        self.assertIn("stream-b", context)
        self.assertNotIn("HELD BY RICH", context)
        again = self.hook("UserPromptSubmit", prompt="And now?")
        self.assertEqual((again.returncode, again.stdout.strip()), (0, ""))

    def test_a_reply_holding_work_reaches_the_lead_at_the_apps_turn_end_and_records_nothing(self):
        """Orchestration reliability R1 on the app host: the deferral check is report only,
        and reaches Rich as Stop additionalContext, in one JSON answer."""
        # The app's Stop reads Claude Code's own transcript of the turn, as the terminal's does.
        transcript = self.native_transcript("How is it going?")
        self.assertEqual(self.hook("UserPromptSubmit", prompt="How is it going?",
                                   transcript_path=transcript).returncode, 0)
        result = self.hook("Stop", last_assistant_message="Spelling work is on hold, deliberately.",
                           transcript_path=transcript, stop_hook_active=False)
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertEqual(sorted((self.root / "workspaces" / "holds").glob("held-*.json")), [])
        answer = json.loads(result.stdout)["hookSpecificOutput"]
        self.assertEqual(answer["hookEventName"], "Stop")
        self.assertIn("DEFERRAL CHECK (unverified", answer["additionalContext"])
        self.assertIn("Spelling work is on hold, deliberately.", answer["additionalContext"])

    def test_stus_turn_carries_no_hold_lines(self):
        self._wait_on_file()
        (self.root / "state/evidence/fictional-session").mkdir(parents=True)
        (self.root / "state/evidence/fictional-session/binding.json").write_text('{"role": "stu"}')
        result = self.hook("UserPromptSubmit", prompt="Carry on.")
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertNotIn("stream-b", result.stdout)

    def test_stu_cannot_read_linkedin_himself_and_rich_still_can(self):
        reader = "mcp__richos_linkedin__read_profile"
        read = {"tool_name": reader, "tool_use_id": "read-1", "tool_input": {"url": "https://www.linkedin.com/in/ada-walker/"}}
        self.assertEqual(self.hook("PreToolUse", **read).returncode, 0, "an unbound session is not refused")
        (self.root / "state/evidence/fictional-session").mkdir(parents=True, exist_ok=True)
        binding = self.root / "state/evidence/fictional-session/binding.json"
        binding.write_text('{"role": "rich"}')
        rich = self.hook("PreToolUse", **read)
        self.assertEqual(rich.returncode, 0, rich.stderr + rich.stdout)
        binding.write_text('{"role": "stu"}')
        stu = self.hook("PreToolUse", **read)
        self.assertEqual(stu.returncode, 2, stu.stderr + stu.stdout)
        self.assertIn("teammate's tool", stu.stderr)
        self.assertIn("Nothing was opened", stu.stderr)
        # Stu's other tools are not touched by it.
        self.assertEqual(self.hook("PreToolUse", tool_name="Read", tool_use_id="read-2",
                                   tool_input={"file_path": str(self.scope)}).returncode, 0)

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

    def test_provider_callbacks_are_kept_whole_and_no_second_transcript_is_written(self):
        """Fails on main 47e314bb6, whose hook wrote guard-transcript.jsonl beside callbacks.jsonl
        (richos-hq docs/research/2026-10-31-app-terminal-differences.md finding 10)."""
        transcript = self.native_transcript("Fictional request")
        self.assertEqual(self.hook("UserPromptSubmit", prompt="Fictional request", transcript_path=transcript).returncode, 0)
        self.assertEqual(self.hook("PreToolUse", tool_name="Read", tool_use_id="tool-7", transcript_path=transcript,
                                   tool_input={"file_path":"fixture"}).returncode, 0)
        folder = self.root / "state/evidence/fictional-session"
        self.assertFalse((folder / "guard-transcript.jsonl").exists(), "a second copy of the transcript was written")
        rows = [json.loads(line)["callback"] for line in (folder / "callbacks.jsonl").read_text().splitlines()]
        self.assertEqual([(row["hook_event_name"], row["transcript_path"]) for row in rows],
                         [("UserPromptSubmit", transcript), ("PreToolUse", transcript)])
        self.assertEqual(rows[1]["tool_use_id"], "tool-7")
        self.assertEqual((folder / "callbacks.jsonl").stat().st_mode & 0o777, 0o600)

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
                self.assertNotIn("updatedInput", output, "native arguments must stay unchanged")
        self.assertFalse((self.coordination / "result.txt").exists(), "the fixture only runs the production hook")
        self.scope.write_text(json.dumps({**grant, "background_work_allowed":False}))
        rich = self.hook("PreToolUse", tool_name="Bash", tool_use_id="rich-command", tool_input={"command":"true", "run_in_background":False})
        self.assertEqual(rich.returncode, 0, rich.stderr)
        rich_output = json.loads(rich.stdout).get("hookSpecificOutput", {}) if rich.stdout.strip() else {}
        self.assertNotIn("updatedInput", rich_output, "Rich's commands run as he issued them")

    def test_stus_own_ordinary_bash_runs_in_the_foreground_as_in_the_terminal(self):
        """§129 forced Stu's own Bash into the background so a job's Stop could end it by its task.
        On Claude Code 2.1.314 a foreground command is a native task with its own id: the job's
        Stop ends it alone with stop_task (work_host.rs
        a_jobs_stop_of_stus_foreground_command_leaves_another_jobs_worker_running). So his command now
        runs as he issued it and waits for its own end, as in the terminal. On 1005c35e8 the hook
        answered this call with updatedInput carrying run_in_background: true."""
        self.scope.write_text(json.dumps({"version":1, "actions_allowed":True, "background_work_allowed":True,
            "binding":{"entity_id":"company", "thread_id":"thread", "session_id":"fictional-session", "turn_id":"peer:1"}}))
        args = {"command":"python3 heartbeat.py beat b 300", "description":"work-b: heartbeat"}
        result = self.hook("PreToolUse", tool_name="Bash", tool_use_id="stu-foreground", tool_input=args)
        self.assertEqual(result.returncode, 0, result.stderr)
        output = json.loads(result.stdout).get("hookSpecificOutput", {}) if result.stdout.strip() else {}
        self.assertNotIn("run_in_background", output.get("updatedInput") or {},
                         "Stu's own foreground command must not be moved to the background")


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

    def test_each_dispatch_guard_gets_at_least_the_budget_the_engine_registers_for_it(self):
        """A dispatch failed with "guard-worktree-isolation.sh timed out after 20 seconds" (follow-up
        "app work tools refused dispatches for unexplained reasons", 2026-10-28). That guard waits
        up to 30 s for the registry lock and runs Pierce inside register-spawn, and the engine's own
        registration (hooks/hooks.json, the terminal's) gives it 330 s; the app gave every guard 20."""
        import importlib.util, types
        spec = importlib.util.spec_from_file_location("app_engine_hook_budgets", ENGINE / "scripts/app-engine-hook.py")
        hook = importlib.util.module_from_spec(spec); spec.loader.exec_module(hook)
        given = {}
        def run(command, payload, timeout=20):
            given[Path(command[-1]).name] = timeout
        real_load = hook.load
        work = types.SimpleNamespace(
            validate_shell_target=lambda payload: None, worker_context=lambda active, payload: None,
            dispatch_intent=lambda *args, **kwargs: None,
            W=types.SimpleNamespace(_recovery_adapter=lambda: (
                types.SimpleNamespace(cancellation_notice=lambda host, session: None), None)))
        def load(name, path):
            if path.name == "app-evidence.py":
                return types.SimpleNamespace(capture=lambda payload, *args, **kwargs: payload)
            if path.name == "app.py":
                return work
            return real_load(name, path)
        hook.run, hook.load, hook.mid_job_notice = run, load, lambda *args: False
        saved = dict(os.environ)
        self.addCleanup(lambda: (os.environ.clear(), os.environ.update(saved)))
        os.environ.update(self.env)
        with contextlib.redirect_stdout(io.StringIO()):
            hook.handle({"hook_event_name": "PreToolUse", "tool_name": "Agent", "session_id": "fictional-session",
                         "tool_use_id": "fictional-call", "cwd": str(self.coordination),
                         "tool_input": {"name": "worker-sonnet-1", "subagent_type": "worker", "prompt": "fictional"}})
        registered = {}
        for group in json.loads((ENGINE / "hooks/hooks.json").read_text())["hooks"]["PreToolUse"]:
            if group.get("matcher") == "Agent":
                for entry in group["hooks"]:
                    registered[entry["command"].split("/")[-1]] = entry["timeout"]
        self.assertIn("guard-worktree-isolation.sh", given, given)
        self.assertGreaterEqual(given["guard-worktree-isolation.sh"], registered["guard-worktree-isolation.sh"])
        # guard-pierce.sh's bounded inspection has its own line above the loop (315 s, inside the
        # app's outer PreToolUse budget); every guard the loop runs takes the registered budget.
        for guard, timeout in given.items():
            if guard in registered and guard != "guard-pierce.sh":
                self.assertGreaterEqual(timeout, registered[guard], guard)

    # --- §154: the app runs the terminal's own dispatch guards and turn-end checks ----------------
    # (Hank's audit, richos-hq docs/research/2026-10-31-app-terminal-differences.md, findings 7, 8)

    def _hook_in_process(self, name):
        """The production hook, loaded here, with the app's evidence capture and dispatch adapter
        stubbed so only the engine guards and checks it starts are real."""
        import importlib.util, types
        spec = importlib.util.spec_from_file_location(name, ENGINE / "scripts/app-engine-hook.py")
        hook = importlib.util.module_from_spec(spec); spec.loader.exec_module(hook)
        work = types.SimpleNamespace(
            validate_shell_target=lambda payload: None, worker_context=lambda active, payload: None,
            dispatch_intent=lambda *args, **kwargs: None,
            W=types.SimpleNamespace(fallback_notice=lambda *args: "", _recovery_adapter=lambda: (
                types.SimpleNamespace(cancellation_notice=lambda host, session: None), None)))
        real_load = hook.load
        def load(module, path):
            if path.name == "app-evidence.py":
                return types.SimpleNamespace(capture=lambda payload, *args, **kwargs: payload)
            if path.name == "app.py":
                return work
            return real_load(module, path)
        hook.load, hook.mid_job_notice = load, lambda *args: False
        saved = dict(os.environ)
        self.addCleanup(lambda: (os.environ.clear(), os.environ.update(saved)))
        os.environ.update(self.env)
        return hook

    def test_a_dispatch_the_terminal_refuses_is_refused_in_the_app_with_the_same_message(self):
        """A brief that rebuilds a surface the adoption ledger answered, with no `reference:` line, is
        refused by guard-reference-ledger.sh in the terminal (§66). On 47e314bb6 the app never ran
        that guard (spawn-guard-audience.declaration had it operator-session), so the same dispatch
        went through. Every other guard is passed through here so the one under test is reached."""
        tool_input = {"subagent_type": "worker", "name": "worker-sonnet-1", "description": "Fix the link",
                      "prompt": "## Scope\nFix `richos/web/web-app/lib/link.js` outbox reconnect.\n"}
        payload = {"hook_event_name": "PreToolUse", "tool_name": "Agent", "session_id": "fictional-session",
                   "tool_use_id": "fictional-call", "cwd": str(self.coordination), "tool_input": tool_input}
        terminal = subprocess.run(["/bin/bash", str(ENGINE / "scripts/hooks/guard-reference-ledger.sh")],
                                  input=json.dumps(payload), text=True, capture_output=True, env=self.env, timeout=30)
        self.assertEqual(terminal.returncode, 2, terminal.stderr + terminal.stdout)
        self.assertIn("ALREADY ANSWERED", terminal.stderr)

        hook = self._hook_in_process("app_engine_hook_reference")
        ran, real_run = [], hook.run
        def run(command, payload, timeout=20):
            ran.append(Path(command[-1]).name)
            if Path(command[-1]).name == "guard-reference-ledger.sh":
                return real_run(command, payload, timeout)
        hook.run = run
        err = io.StringIO()
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(err):
            with self.assertRaises(RuntimeError):
                hook.handle(dict(payload))
        self.assertIn(terminal.stderr.strip(), err.getvalue(), "the app's refusal is not the terminal's message")
        # With every guard passing, the terminal's other dispatch guards are run too, and femcboost's
        # own proof-scope guard, which the CEO ruled off in the app (§146 #3), is not.
        ran.clear()
        hook.run = lambda command, payload, timeout=20: ran.append(Path(command[-1]).name)
        with contextlib.redirect_stdout(io.StringIO()):
            hook.handle(dict(payload))
        for guard in ("guard-owned-state.sh", "guard-brief-scope.sh",
                      "guard-no-home-network-phone.sh", "guard-reference-ledger.sh", "guard-public-record-repo.sh"):
            self.assertIn(guard, ran)
        self.assertNotIn("guard-brief-verification-scope.sh", ran)
        # The staging guard reads the governed repository's main and deployment record, and the app's
        # governed folder is the private coordination folder, not the user's product repository: run
        # there it asked the wrong Git database (the second review of 2d5f7ded561a), so it stays out.
        self.assertNotIn("guard-stale-staging.sh", ran)

    def test_stus_turn_end_runs_every_check_the_terminal_registers_except_the_terminal_only_ones(self):
        """The terminal ends a lead's turn with every Stop command in hooks/hooks.json; on 47e314bb6
        the app ran the deferral notice and ass-kicker.py, and nothing else."""
        hook = self._hook_in_process("app_engine_hook_turn_end")
        ran = []
        import types
        def fake(argv, **kwargs):
            ran.append(Path(argv[1]).name)
            return types.SimpleNamespace(returncode=0, stdout="", stderr="")
        hook.subprocess = types.SimpleNamespace(run=fake, TimeoutExpired=subprocess.TimeoutExpired)
        (self.root / "state/evidence/fictional-session").mkdir(parents=True)
        (self.root / "state/evidence/fictional-session/binding.json").write_text('{"role": "stu"}')
        with contextlib.redirect_stdout(io.StringIO()):
            hook.handle({"hook_event_name": "Stop", "session_id": "fictional-session", "cwd": str(self.coordination),
                         "last_assistant_message": "Done.", "stop_hook_active": False})
        registered = [entry["command"].split("/")[-1].split()[0]
                      for group in json.loads((ENGINE / "hooks/hooks.json").read_text())["hooks"]["Stop"]
                      for entry in group["hooks"]]
        for name in hook.TERMINAL_ONLY_TURN_END:
            self.assertIn(name, registered, "a terminal-only check that is no longer registered")
        self.assertEqual(sorted(ran), sorted(n for n in registered if n not in hook.TERMINAL_ONLY_TURN_END))

    def test_a_registered_command_keeps_an_engine_path_with_a_space_in_one_word(self):
        """The app installs its engine under ~/Library/Application Support/RichOS/engine. On
        b9e7662d75af the path was substituted before the command was split, so every check ran as
        bash .../Library/Application and failed (the second review of that commit, P1)."""
        hook = self._hook_in_process("app_engine_hook_spaces")
        installed = self.root / "Library/Application Support/RichOS"
        installed.mkdir(parents=True)
        (installed / "engine").symlink_to(ENGINE)
        hook.ENGINE = installed / "engine"
        checks = hook.registered("Stop")
        self.assertTrue(checks)
        for name, argv, _timeout in checks:
            self.assertEqual(argv[1], str(hook.ENGINE / "scripts/hooks" / name), argv)
        disk = [argv for name, argv, _timeout in checks if name == "notice-disk-alert.sh"]
        self.assertEqual(disk[0][2:], ["--event", "Stop"])

    def test_the_claim_check_sees_the_connected_repositories_in_the_app(self):
        """The terminal refuses "commit X has been pushed" for a commit only on the local branch;
        it finds the commit because its repository sits beside the lead's. The app's coordination
        folder sits beside nothing: on 0aa9a71f60cf the app let the same false claim through (the
        second review of that commit, finding 1). A connected repository whose path has a space."""
        product = self.root / "my projects/website"
        product.mkdir(parents=True)
        git_env = {**self.env, "GIT_AUTHOR_NAME": "Fixture", "GIT_AUTHOR_EMAIL": "fixture@example.invalid",
                   "GIT_COMMITTER_NAME": "Fixture", "GIT_COMMITTER_EMAIL": "fixture@example.invalid"}
        def git(*args, body=None):
            return subprocess.run(["git", "-C", str(product), *args], input=body, text=True, env=git_env,
                                  capture_output=True, check=True, timeout=10).stdout.strip()
        git("init", "-q", "--template=", "--initial-branch=main")
        sha = git("commit-tree", git("mktree", body=""), body="fixture\n")
        git("update-ref", "refs/heads/main", sha)
        self.env["RICHOS_APP_READ_ROOTS"] = json.dumps([str(product)])
        result = self.hook("Stop", last_assistant_message="Commit %s has been pushed." % sha, stop_hook_active=False)
        self.assertEqual(result.returncode, 2, result.stderr + result.stdout)
        self.assertIn("PUSHED", result.stderr)

    def test_a_question_left_unanswered_is_put_back_until_his_own_words_answer_it(self):
        """notice-unanswered-question.sh reads earlier replies and whose turn each input started
        from Claude Code's own transcript. The app sends every input over the SDK (turnOrigin
        sdk), his words and his teammates' messages alike, so in the app his answer is the input
        whose text is his original words in the app's ledger. Here: the lead asks, a teammate's
        message wakes it and the question is put back; then his answer clears it. On 2f9632e77
        (w4c merged onto main) his answer was never his (no turnOrigin human in the app), so the
        question was put back after he had answered it."""
        self.scope.write_text(json.dumps({"version": 1, "actions_allowed": True, "binding": {
            "entity_id": "fictional-company", "thread_id": "fictional-thread", "session_id": "fictional-session"}}))
        ledger = self.root / "conversation-ledger.jsonl"
        transcript = self.root / "transcript.jsonl"

        def said(text):
            turn = "turn-%d" % (len(ledger.read_text().splitlines()) if ledger.exists() else 0)
            with ledger.open("a") as out:
                out.write(json.dumps({"event": "PromptReceived", "entity_id": "fictional-company",
                    "thread_id": "fictional-thread", "source": "text", "turn_id": turn, "text": text}) + "\n")

        def row(kind, text, at):
            body = ({"type": "user", "turnOrigin": "sdk", "promptSource": "sdk",
                     "message": {"role": "user", "content": [{"type": "text", "text": text}]}}
                    if kind == "user" else
                    {"type": "assistant", "message": {"role": "assistant", "content": [{"type": "text", "text": text}]}})
            with transcript.open("a") as out:
                out.write(json.dumps({**body, "isSidechain": False, "timestamp": at}) + "\n")

        question = "QUESTION FOR YOU: Should I publish the website?"
        said("Prepare the website.")
        row("user", "Prepare the website.", "2026-11-01T10:00:00Z")
        row("assistant", question, "2026-11-01T10:01:00Z")
        asked = self.hook("Stop", transcript_path=str(transcript), last_assistant_message=question, stop_hook_active=False)
        self.assertEqual(asked.returncode, 0, asked.stderr + asked.stdout)
        self.assertNotIn("STILL WAITING FOR YOUR ANSWER", asked.stdout)
        row("user", json.dumps({"from": "stu", "text": "The worker finished."}), "2026-11-01T10:05:00Z")
        later = self.hook("Stop", transcript_path=str(transcript), last_assistant_message="Noted.", stop_hook_active=False)
        self.assertEqual(later.returncode, 0, later.stderr + later.stdout)
        self.assertIn("STILL WAITING FOR YOUR ANSWER", later.stdout)
        row("assistant", "Noted.", "2026-11-01T10:05:30Z")
        said("Yes, publish it.")
        row("user", "Yes, publish it.", "2026-11-01T10:09:00Z")
        answered = self.hook("Stop", transcript_path=str(transcript), last_assistant_message="Publishing.", stop_hook_active=False)
        self.assertEqual(answered.returncode, 0, answered.stderr + answered.stdout)
        self.assertNotIn("STILL WAITING FOR YOUR ANSWER", answered.stdout + answered.stderr)
        self.assertNotIn("WATCH DID NOT RUN", answered.stdout + answered.stderr)

    def test_a_turn_end_check_the_terminal_runs_refuses_stus_false_claim_in_the_app(self):
        """guard-unresolved-claims.sh refuses a reply that says a role is working when none was ever
        dispatched. It reads the session's team folder; the app's are under RICHOS_SA_TEAMS_DIR."""
        (self.root / "teams/session-fictiona").mkdir(parents=True)
        self.env["RICHOS_SA_TEAMS_DIR"] = str(self.root / "teams")
        result = self.hook("Stop", last_assistant_message="Worker is fixing the login page now.",
                           stop_hook_active=False)
        self.assertEqual(result.returncode, 2, result.stderr + result.stdout)
        self.assertIn("A CLAIM IN THE FINAL MESSAGE CONTRADICTS THE REPOSITORY", result.stderr)
        self.assertIn("Worker is fixing", result.stderr)


if __name__ == "__main__":
    unittest.main()
